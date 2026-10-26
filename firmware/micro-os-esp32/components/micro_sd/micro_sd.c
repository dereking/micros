/*
 * External SD card for the Micro OS storage API.
 *
 * Hardware (Spotpear / Waveshare ESP32-S3 Touch LCD 7 V1.2):
 *   MOSI GPIO11, MISO GPIO13, SCLK GPIO12, CS via CH422G EXIO4 (active low).
 *
 * The CH422G line is not a GPIO, so the official demo holds it low and mounts
 * SDSPI with GPIO_NUM_NC as the card-select. We do the same, but preserve the
 * backlight/touch-reset bits in the shared CH422G output register.
 *
 * The VFS mount below also mounts FATFS drive 0. File operations use the FATFS
 * API directly rather than POSIX `opendir`/`mkdir`: Rust's std links newlib's
 * libnosys, whose directory stubs collide with the VFS implementations at link
 * time. FATFS is the same filesystem the VFS uses, and the Micro OS storage
 * calls are serialized on the app task, so no extra lock is needed.
 *
 * Every operation is confined to /sdcard/micros/apps/<app-id>/: the app id and
 * the relative path are validated again here (defense in depth) before the
 * prefix is prepended. The API never exposes the mount root, the OS itself, or
 * another App's directory.
 *
 * External programs live outside the per-App sandbox in the card's top-level
 * `/apps` directory. That directory is read-only to Apps: the launcher lists
 * `*.mbc` images there and the OS loads them by bare file name, so an App can
 * never ask the host to read an arbitrary path.
 */

#include "micro_sd.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "driver/gpio.h"
#include "driver/sdspi_host.h"
#include "driver/spi_master.h"
#include "esp_log.h"
#include "esp_vfs_fat.h"
#include "ff.h"
#include "sdmmc_cmd.h"

#include "micro_bsp_lcd7.h"

#define SD_MOUNT_POINT "/sdcard"
/* `esp_vfs_fat_sdspi_mount` mounts the first FATFS volume as drive 0. */
#define SD_FAT_ROOT "0:/micros/apps"
/* Top-level directory for installable external Apps (not per-App data). */
#define SD_EXTERNAL_APP_DIR "0:/apps"
#define SD_MAX_PATH 320
#define SD_MAX_FILE (64 * 1024)
#define SD_MAX_EXTERNAL_APP (2 * 1024 * 1024)
#define SD_MAX_EXTERNAL_NAME 64
#define SD_MAX_LIST 128
#define SD_MAX_SCAN 1024
#define SD_SPI_MOSI_GPIO 11
#define SD_SPI_MISO_GPIO 13
#define SD_SPI_SCLK_GPIO 12
#define SD_SPI_FREQ_KHZ 10000

static const char *TAG = "micro_sd";
static bool s_initialized;
static bool s_mounted;
static sdmmc_card_t *s_card;

static bool valid_app_id(const char *app_id)
{
    if (app_id == NULL) {
        return false;
    }
    size_t len = strlen(app_id);
    if (len == 0 || len > 32 || !isalnum((unsigned char)app_id[0])) {
        return false;
    }
    for (size_t i = 0; i < len; ++i) {
        unsigned char ch = (unsigned char)app_id[i];
        if (!isalnum(ch) && ch != '_' && ch != '-' && ch != '.') {
            return false;
        }
    }
    return true;
}

/* A launcher-visible external App file: a bare file name in /apps, at most
 * 64 bytes, ending in ".mbc" (FATFS may report 8.3 or long names). Reject path
 * separators and control characters so the name can never escape /apps. */
static bool valid_external_app_name(const char *name)
{
    if (name == NULL) {
        return false;
    }
    size_t len = strlen(name);
    if (len < 5 || len > SD_MAX_EXTERNAL_NAME || name[0] == '.' || name[0] == ' ') {
        return false;
    }
    if (name[len - 1] == ' ' || name[len - 1] == '.') {
        return false;
    }
    if (tolower((unsigned char)name[len - 4]) != '.' ||
        tolower((unsigned char)name[len - 3]) != 'm' ||
        tolower((unsigned char)name[len - 2]) != 'b' ||
        tolower((unsigned char)name[len - 1]) != 'c') {
        return false;
    }
    for (size_t i = 0; i < len; ++i) {
        unsigned char ch = (unsigned char)name[i];
        if (ch < 0x20 || ch == 0x7f || ch == '/' || ch == '\\' || ch == ':') {
            return false;
        }
    }
    return true;
}

/* Mirrors micro-store::validate_path. Empty is the App root. */
static bool valid_rel_path(const char *path)
{
    if (path == NULL) {
        return false;
    }
    size_t total = strlen(path);
    if (total > 240 || (total > 0 && path[0] == '/')) {
        return false;
    }
    if (total == 0) {
        return true;
    }

    int depth = 0;
    const char *cursor = path;
    while (true) {
        const char *slash = strchr(cursor, '/');
        size_t len = slash == NULL ? strlen(cursor) : (size_t)(slash - cursor);
        depth++;
        if (len == 0 || len > 63 || depth > 8) {
            return false;
        }
        if ((len == 1 && cursor[0] == '.') ||
            (len == 2 && cursor[0] == '.' && cursor[1] == '.')) {
            return false;
        }
        if (cursor[0] == ' ' || cursor[len - 1] == ' ' || cursor[len - 1] == '.') {
            return false;
        }
        for (size_t i = 0; i < len; ++i) {
            unsigned char ch = (unsigned char)cursor[i];
            if (ch < 0x20 || ch == 0x7f || ch == '\\' || ch == ':') {
                return false;
            }
            if (!isalnum(ch) && ch != '_' && ch != '-' && ch != '.' && ch != ' ') {
                return false;
            }
        }
        if (slash == NULL) {
            break;
        }
        cursor = slash + 1;
    }
    return true;
}

static int build_path(const char *app_id, const char *path, char *out, size_t cap)
{
    if (!valid_app_id(app_id) || !valid_rel_path(path)) {
        return -1;
    }
    int written = path[0] == '\0'
                      ? snprintf(out, cap, "%s/%s", SD_FAT_ROOT, app_id)
                      : snprintf(out, cap, "%s/%s/%s", SD_FAT_ROOT, app_id, path);
    if (written < 0 || (size_t)written >= cap) {
        return -1;
    }
    return 0;
}

/* Create every component of an absolute FATFS path. Existing directories
 * (including the drive root) are fine. */
static int ensure_tree(const char *path)
{
    char buffer[SD_MAX_PATH];
    int written = snprintf(buffer, sizeof buffer, "%s", path);
    if (written < 0 || (size_t)written >= sizeof buffer) {
        return -1;
    }
    /* Skip the drive root ("0:/" or "/"), then create each subdirectory. */
    char *first_slash = strchr(buffer, '/');
    if (first_slash == NULL) {
        return -1;
    }
    for (char *cursor = first_slash + 1; *cursor != '\0'; ++cursor) {
        if (*cursor != '/') {
            continue;
        }
        *cursor = '\0';
        FRESULT result = f_mkdir(buffer);
        *cursor = '/';
        if (result != FR_OK && result != FR_EXIST) {
            return -1;
        }
    }
    FRESULT result = f_mkdir(buffer);
    return (result == FR_OK || result == FR_EXIST) ? 0 : -1;
}

static int ensure_parent(const char *path)
{
    char buffer[SD_MAX_PATH];
    int written = snprintf(buffer, sizeof buffer, "%s", path);
    if (written < 0 || (size_t)written >= sizeof buffer) {
        return -1;
    }
    char *slash = strrchr(buffer, '/');
    if (slash == NULL || slash == buffer) {
        return -1;
    }
    *slash = '\0';
    return ensure_tree(buffer);
}

static int remove_tree(const char *path)
{
    FILINFO info;
    FRESULT result = f_stat(path, &info);
    if (result == FR_NO_FILE || result == FR_NO_PATH) {
        return 0;
    }
    if (result != FR_OK) {
        return -1;
    }
    if (!(info.fattrib & AM_DIR)) {
        return f_unlink(path) == FR_OK ? 0 : -1;
    }

    FF_DIR dir;
    result = f_opendir(&dir, path);
    if (result != FR_OK) {
        return -1;
    }
    int status = 0;
    while (true) {
        FILINFO entry;
        result = f_readdir(&dir, &entry);
        if (result != FR_OK || entry.fname[0] == '\0') {
            if (result != FR_OK) {
                status = -1;
            }
            break;
        }
        if (strcmp(entry.fname, ".") == 0 || strcmp(entry.fname, "..") == 0) {
            continue;
        }
        char child[SD_MAX_PATH];
        int written = snprintf(child, sizeof child, "%s/%s", path, entry.fname);
        if (written < 0 || (size_t)written >= sizeof child || remove_tree(child) != 0) {
            status = -1;
            break;
        }
    }
    f_closedir(&dir);
    if (status != 0) {
        return status;
    }
    return f_rmdir(path) == FR_OK ? 0 : -1;
}

static int compare_names(const void *left, const void *right)
{
    const char *const *a = left;
    const char *const *b = right;
    return strcmp(*a, *b);
}

esp_err_t micro_sd_init(void)
{
    if (s_initialized) {
        return s_mounted ? ESP_OK : ESP_FAIL;
    }
    s_initialized = true;

    esp_err_t result = micro_bsp_sd_enable();
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "CH422G SD chip-select failed: %s", esp_err_to_name(result));
        return result;
    }

    sdmmc_host_t host = SDSPI_HOST_DEFAULT();
    host.max_freq_khz = SD_SPI_FREQ_KHZ;

    const spi_bus_config_t bus = {
        .mosi_io_num = SD_SPI_MOSI_GPIO,
        .miso_io_num = SD_SPI_MISO_GPIO,
        .sclk_io_num = SD_SPI_SCLK_GPIO,
        .quadwp_io_num = -1,
        .quadhd_io_num = -1,
        .max_transfer_sz = 4096,
    };
    result = spi_bus_initialize(host.slot, &bus, SDSPI_DEFAULT_DMA);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "SPI bus initialization failed: %s", esp_err_to_name(result));
        return result;
    }

    sdspi_device_config_t slot = SDSPI_DEVICE_CONFIG_DEFAULT();
    slot.host_id = host.slot;
    /* CH422G EXIO4 is held low, so no GPIO drives CS. */
    slot.gpio_cs = GPIO_NUM_NC;

    const esp_vfs_fat_mount_config_t mount = {
        .format_if_mount_failed = false,
        .max_files = 8,
        .allocation_unit_size = 16 * 1024,
    };
    result = esp_vfs_fat_sdspi_mount(SD_MOUNT_POINT, &host, &slot, &mount, &s_card);
    if (result != ESP_OK) {
        ESP_LOGW(TAG, "TF card not mounted (%s); storage API disabled", esp_err_to_name(result));
        return result;
    }

    s_mounted = true;
    if (ensure_tree(SD_FAT_ROOT) != 0) {
        ESP_LOGE(TAG, "cannot create %s", SD_FAT_ROOT);
        s_mounted = false;
        esp_vfs_fat_sdcard_unmount(SD_MOUNT_POINT, s_card);
        s_card = NULL;
        return ESP_FAIL;
    }

    ESP_LOGI(TAG, "TF card mounted at %s (%s)", SD_MOUNT_POINT, s_card->cid.name);
    return ESP_OK;
}

int micro_sd_available(void)
{
    return s_mounted ? 1 : 0;
}

int micro_sd_read(const char *app_id, const char *path, char **out, size_t *out_len)
{
    if (out == NULL || out_len == NULL) {
        return -1;
    }
    *out = NULL;
    *out_len = 0;
    if (!s_mounted || !valid_app_id(app_id) || !valid_rel_path(path) || path[0] == '\0') {
        return -1;
    }

    char full[SD_MAX_PATH];
    if (build_path(app_id, path, full, sizeof full) != 0) {
        return -1;
    }
    FILINFO info;
    FRESULT result = f_stat(full, &info);
    if (result == FR_NO_FILE || result == FR_NO_PATH) {
        return 0;
    }
    if (result != FR_OK || (info.fattrib & AM_DIR) || info.fsize > SD_MAX_FILE) {
        return -1;
    }

    FIL file;
    result = f_open(&file, full, FA_READ);
    if (result != FR_OK) {
        return -1;
    }
    size_t size = (size_t)f_size(&file);
    if (size > SD_MAX_FILE) {
        f_close(&file);
        return -1;
    }
    char *buffer = malloc(size + 1);
    if (buffer == NULL) {
        f_close(&file);
        return -1;
    }
    UINT read = 0;
    result = f_read(&file, buffer, (UINT)size, &read);
    f_close(&file);
    if (result != FR_OK || read != size) {
        free(buffer);
        return -1;
    }
    buffer[size] = '\0';
    *out = buffer;
    *out_len = size;
    return 0;
}

int micro_sd_write(const char *app_id, const char *path, const char *data,
                   size_t data_len, int append)
{
    if (!s_mounted || data == NULL || !valid_app_id(app_id) || !valid_rel_path(path) ||
        path[0] == '\0' || data_len > SD_MAX_FILE) {
        return -1;
    }
    char full[SD_MAX_PATH];
    if (build_path(app_id, path, full, sizeof full) != 0) {
        return -1;
    }
    if (ensure_parent(full) != 0) {
        return -1;
    }
    if (append) {
        FILINFO info;
        FRESULT result = f_stat(full, &info);
        if (result == FR_OK) {
            if ((info.fattrib & AM_DIR) || info.fsize + data_len > SD_MAX_FILE) {
                return -1;
            }
        } else if (result != FR_NO_FILE && result != FR_NO_PATH) {
            return -1;
        }
    }

    FIL file;
    BYTE mode = FA_WRITE | (append ? FA_OPEN_APPEND : FA_CREATE_ALWAYS);
    FRESULT result = f_open(&file, full, mode);
    if (result != FR_OK) {
        return -1;
    }
    UINT written = 0;
    result = f_write(&file, data, (UINT)data_len, &written);
    FRESULT closed = f_close(&file);
    return result == FR_OK && closed == FR_OK && written == data_len ? 0 : -1;
}

int micro_sd_exists(const char *app_id, const char *path, int *out)
{
    if (!s_mounted || out == NULL || !valid_app_id(app_id) || !valid_rel_path(path)) {
        return -1;
    }
    *out = 0;
    if (path[0] == '\0') {
        *out = 1;
        return 0;
    }
    char full[SD_MAX_PATH];
    if (build_path(app_id, path, full, sizeof full) != 0) {
        return -1;
    }
    FILINFO info;
    FRESULT result = f_stat(full, &info);
    if (result == FR_OK) {
        *out = 1;
        return 0;
    }
    return result == FR_NO_FILE || result == FR_NO_PATH ? 0 : -1;
}

int micro_sd_remove(const char *app_id, const char *path, int *out)
{
    if (!s_mounted || out == NULL || !valid_app_id(app_id) || !valid_rel_path(path) ||
        path[0] == '\0') {
        return -1;
    }
    *out = 0;
    char full[SD_MAX_PATH];
    if (build_path(app_id, path, full, sizeof full) != 0) {
        return -1;
    }
    FILINFO info;
    FRESULT result = f_stat(full, &info);
    if (result == FR_NO_FILE || result == FR_NO_PATH) {
        return 0;
    }
    if (result != FR_OK || remove_tree(full) != 0) {
        return -1;
    }
    *out = 1;
    return 0;
}

int micro_sd_list(const char *app_id, const char *path, char **out, size_t *out_len)
{
    if (out == NULL || out_len == NULL) {
        return -1;
    }
    *out = NULL;
    *out_len = 0;
    if (!s_mounted || !valid_app_id(app_id) || !valid_rel_path(path)) {
        return -1;
    }
    char full[SD_MAX_PATH];
    if (build_path(app_id, path, full, sizeof full) != 0) {
        return -1;
    }

    FF_DIR dir;
    FRESULT result = f_opendir(&dir, full);
    if (result == FR_NO_FILE || result == FR_NO_PATH) {
        return 0;
    }
    if (result != FR_OK) {
        return -1;
    }
    char **names = NULL;
    size_t count = 0;
    size_t capacity = 0;
    while (count < SD_MAX_SCAN) {
        FILINFO entry;
        result = f_readdir(&dir, &entry);
        if (result != FR_OK) {
            f_closedir(&dir);
            for (size_t i = 0; i < count; ++i) {
                free(names[i]);
            }
            free(names);
            return -1;
        }
        if (entry.fname[0] == '\0') {
            break;
        }
        if (strcmp(entry.fname, ".") == 0 || strcmp(entry.fname, "..") == 0) {
            continue;
        }
        if (count == capacity) {
            size_t next = capacity == 0 ? 32 : capacity * 2;
            if (next > SD_MAX_SCAN) {
                next = SD_MAX_SCAN;
            }
            if (next == capacity) {
                break;
            }
            char **grown = realloc(names, next * sizeof names[0]);
            if (grown == NULL) {
                f_closedir(&dir);
                for (size_t i = 0; i < count; ++i) {
                    free(names[i]);
                }
                free(names);
                return -1;
            }
            names = grown;
            capacity = next;
        }
        bool is_dir = (entry.fattrib & AM_DIR) != 0;
        size_t len = strlen(entry.fname) + (is_dir ? 1 : 0);
        char *name = malloc(len + 1);
        if (name == NULL) {
            f_closedir(&dir);
            for (size_t i = 0; i < count; ++i) {
                free(names[i]);
            }
            free(names);
            return -1;
        }
        memcpy(name, entry.fname, len - (is_dir ? 1 : 0));
        if (is_dir) {
            name[len - 1] = '/';
        }
        name[len] = '\0';
        names[count++] = name;
    }
    f_closedir(&dir);

    if (count > 1) {
        qsort(names, count, sizeof names[0], compare_names);
    }
    size_t listed = count < SD_MAX_LIST ? count : SD_MAX_LIST;
    size_t total = 0;
    for (size_t i = 0; i < listed; ++i) {
        total += strlen(names[i]) + (i + 1 < listed ? 1 : 0);
    }
    char *buffer = NULL;
    if (total > 0) {
        buffer = malloc(total + 1);
        if (buffer != NULL) {
            size_t offset = 0;
            for (size_t i = 0; i < listed; ++i) {
                size_t len = strlen(names[i]);
                memcpy(buffer + offset, names[i], len);
                offset += len;
                if (i + 1 < listed) {
                    buffer[offset++] = '\n';
                }
            }
            buffer[offset] = '\0';
            *out = buffer;
            *out_len = offset;
        }
    }
    for (size_t i = 0; i < count; ++i) {
        free(names[i]);
    }
    free(names);
    return buffer == NULL && total > 0 ? -1 : 0;
}

int micro_sd_list_external_apps(char **out, size_t *out_len)
{
    if (out == NULL || out_len == NULL) {
        return -1;
    }
    *out = NULL;
    *out_len = 0;
    /* No card (or a failed mount) means no external Apps; this is expected and
     * must never stop the OS from booting. */
    if (!s_mounted) {
        return 0;
    }

    FF_DIR dir;
    FRESULT result = f_opendir(&dir, SD_EXTERNAL_APP_DIR);
    if (result == FR_NO_FILE || result == FR_NO_PATH) {
        return 0;
    }
    if (result != FR_OK) {
        return -1;
    }

    char **names = NULL;
    size_t count = 0;
    size_t capacity = 0;
    while (count < SD_MAX_SCAN) {
        FILINFO entry;
        result = f_readdir(&dir, &entry);
        if (result != FR_OK) {
            f_closedir(&dir);
            for (size_t i = 0; i < count; ++i) {
                free(names[i]);
            }
            free(names);
            return -1;
        }
        if (entry.fname[0] == '\0') {
            break;
        }
        if ((entry.fattrib & AM_DIR) != 0 || !valid_external_app_name(entry.fname)) {
            continue;
        }
        if (count == capacity) {
            size_t next = capacity == 0 ? 16 : capacity * 2;
            if (next > SD_MAX_SCAN) {
                next = SD_MAX_SCAN;
            }
            if (next == capacity) {
                break;
            }
            char **grown = realloc(names, next * sizeof names[0]);
            if (grown == NULL) {
                f_closedir(&dir);
                for (size_t i = 0; i < count; ++i) {
                    free(names[i]);
                }
                free(names);
                return -1;
            }
            names = grown;
            capacity = next;
        }
        char *name = strdup(entry.fname);
        if (name == NULL) {
            f_closedir(&dir);
            for (size_t i = 0; i < count; ++i) {
                free(names[i]);
            }
            free(names);
            return -1;
        }
        names[count++] = name;
    }
    f_closedir(&dir);

    if (count > 1) {
        qsort(names, count, sizeof names[0], compare_names);
    }
    size_t listed = count < SD_MAX_LIST ? count : SD_MAX_LIST;
    size_t total = 0;
    for (size_t i = 0; i < listed; ++i) {
        total += strlen(names[i]) + (i + 1 < listed ? 1 : 0);
    }
    char *buffer = NULL;
    if (total > 0) {
        buffer = malloc(total + 1);
        if (buffer != NULL) {
            size_t offset = 0;
            for (size_t i = 0; i < listed; ++i) {
                size_t len = strlen(names[i]);
                memcpy(buffer + offset, names[i], len);
                offset += len;
                if (i + 1 < listed) {
                    buffer[offset++] = '\n';
                }
            }
            buffer[offset] = '\0';
            *out = buffer;
            *out_len = offset;
        }
    }
    for (size_t i = 0; i < count; ++i) {
        free(names[i]);
    }
    free(names);
    return buffer == NULL && total > 0 ? -1 : 0;
}

int micro_sd_read_external_app(const char *name, char **out, size_t *out_len)
{
    if (out == NULL || out_len == NULL) {
        return -1;
    }
    *out = NULL;
    *out_len = 0;
    if (!s_mounted || !valid_external_app_name(name)) {
        return -1;
    }

    char full[SD_MAX_PATH];
    int written = snprintf(full, sizeof full, "%s/%s", SD_EXTERNAL_APP_DIR, name);
    if (written < 0 || (size_t)written >= sizeof full) {
        return -1;
    }

    FILINFO info;
    FRESULT result = f_stat(full, &info);
    if (result == FR_NO_FILE || result == FR_NO_PATH) {
        return 0;
    }
    if (result != FR_OK || (info.fattrib & AM_DIR) != 0 || info.fsize == 0 ||
        info.fsize > SD_MAX_EXTERNAL_APP) {
        return -1;
    }

    FIL file;
    result = f_open(&file, full, FA_READ);
    if (result != FR_OK) {
        return -1;
    }
    size_t size = (size_t)f_size(&file);
    if (size == 0 || size > SD_MAX_EXTERNAL_APP) {
        f_close(&file);
        return -1;
    }
    char *buffer = malloc(size + 1);
    if (buffer == NULL) {
        f_close(&file);
        return -1;
    }
    UINT read = 0;
    result = f_read(&file, buffer, (UINT)size, &read);
    f_close(&file);
    if (result != FR_OK || read != size) {
        free(buffer);
        return -1;
    }
    buffer[size] = '\0';
    *out = buffer;
    *out_len = size;
    return 0;
}

int micro_sd_mkdir(const char *app_id, const char *path, int *out)
{
    if (!s_mounted || out == NULL || !valid_app_id(app_id) || !valid_rel_path(path)) {
        return -1;
    }
    *out = 1;
    if (path[0] == '\0') {
        return 0;
    }
    char full[SD_MAX_PATH];
    if (build_path(app_id, path, full, sizeof full) != 0) {
        return -1;
    }
    if (ensure_tree(full) != 0) {
        return -1;
    }
    FILINFO info;
    if (f_stat(full, &info) != FR_OK || !(info.fattrib & AM_DIR)) {
        return -1;
    }
    return 0;
}

int micro_sd_size(const char *app_id, const char *path, int64_t *out)
{
    if (!s_mounted || out == NULL || !valid_app_id(app_id) || !valid_rel_path(path)) {
        return -1;
    }
    *out = -1;
    if (path[0] == '\0') {
        *out = 0;
        return 0;
    }
    char full[SD_MAX_PATH];
    if (build_path(app_id, path, full, sizeof full) != 0) {
        return -1;
    }
    FILINFO info;
    FRESULT result = f_stat(full, &info);
    if (result == FR_NO_FILE || result == FR_NO_PATH) {
        return 0;
    }
    if (result != FR_OK) {
        return -1;
    }
    *out = (info.fattrib & AM_DIR) ? 0 : (int64_t)info.fsize;
    return 0;
}

void micro_sd_free(void *ptr)
{
    free(ptr);
}
