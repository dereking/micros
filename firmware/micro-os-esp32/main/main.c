#include <ctype.h>
#include <inttypes.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <strings.h>

#include "driver/gpio.h"
#include "esp_err.h"
#include "esp_flash.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_partition.h"
#include "esp_psram.h"
#include "esp_system.h"

#include "lvgl.h"

#include "micro_bsp_lcd7.h"
#include "micro_clock.h"
#include "micro_kv.h"
#include "micro_sd.h"
#include "micro_runtime_ffi.h"
#include "micro_wifi.h"

#define MICRO_EXPECTED_MEMORY_BYTES (8U * 1024U * 1024U)
#define MICRO_APP_PARTITION_LABEL "micro_app"
#define MICRO_RUNTIME_EVENT_BUDGET 10000U
#define MICRO_RUNTIME_TICK_PERIOD_MS 30U
#define MICRO_RUNTIME_ERROR_BUFFER_SIZE 192U
/* Cap for the mirrored Wi-Fi radio state string ("off"|"connecting"|"connected"|"error"). */
#define MICRO_WIFI_STATE_CAP 16U
#define MICRO_OS_MAX_ACTIONS 16U
#define MICRO_BOOT_BUTTON_GPIO 0U
#define MICRO_BOOT_DEBOUNCE_TICKS 4U
/* Edge-swipe back gesture on the 800x480 display (Android gesture-nav style):
 * a drag starting within the left/right edge zone that moves inward. */
#define MICRO_SWIPE_EDGE_ZONE 64
#define MICRO_SWIPE_THRESHOLD 80

/* MBC1 file layout (little-endian, see crates/micro-ir/src/codec.rs):
 *   offset 0..4   magic "MBC1"
 *   offset 4..6   version (u16)
 *   offset 6..10  payload length (u32)
 *   offset 10..14 crc32 (u32) over the payload
 *   offset 14..   payload bytes
 * The total file size is therefore 14 + payload_length. */
#define MICRO_MBC_HEADER_SIZE 14U
/* Payload sections are [tag:u8][len:u32][bytes]; the metadata section is 6. */
#define MICRO_MBC_SECTION_HEADER_SIZE 5U
#define MICRO_MBC_SECTION_METADATA 6U

/* The shell launcher has eight fixed slots (see apps/shell/app.ts), and the
 * host registry exposes at most that many installable Apps. Slot 0 of the
 * on-flash registry is the shell itself, so the total MBC cap is slots + 1.
 * Built-in Apps are scanned first; external `/apps/<app>.mbc` images fill the
 * remaining slots (built-ins always win an id collision). */
#define MICRO_APP_SLOTS 8U
#define MICRO_APPS_MAX (MICRO_APP_SLOTS + 1U)
#define MICRO_APP_NAME_MAX 32U
#define MICRO_APP_ICON_MAX 16U
#define MICRO_EXTERNAL_APP_NAME_MAX 65U

static const char *TAG = "micro_os";

static micro_os_t *s_os;
static micro_bsp_display_t s_display;
static const esp_partition_t *s_partition;
static micro_runtime_t *s_runtime;
static bool s_is_shell;
static char s_runtime_error[MICRO_RUNTIME_ERROR_BUFFER_SIZE];
/* Last Wi-Fi radio state seen by the tick loop; when it changes, the running
 * App's bindings are re-evaluated (they bind `net.wifiState()` once at mount
 * and only re-run when a state they read changes). */
static char s_last_wifi_state[MICRO_WIFI_STATE_CAP];

static micro_action_t s_actions[MICRO_OS_MAX_ACTIONS];
static micro_action_buffer_t s_action_buffer;

static uint32_t s_backlight = 3;

static uint32_t s_boot_level_count;

/* --- installed-app registry (micro_app partition + /apps on the SD card) --- */

typedef struct {
    uint32_t offset; /* partition offset (built-in entries only) */
    uint32_t len;    /* total MBC length (built-in entries only) */
    char id[MICRO_APP_NAME_MAX];
    char name[MICRO_APP_NAME_MAX];
    char icon[MICRO_APP_ICON_MAX];
    /* External entries are loaded from `/sdcard/apps/<file>` on demand. */
    bool external;
    char file[MICRO_EXTERNAL_APP_NAME_MAX];
} micro_app_entry_t;

static micro_app_entry_t s_apps[MICRO_APPS_MAX];
static uint32_t s_app_count;
/* Number of entries found on the micro_app partition (shell + built-ins). */
static uint32_t s_builtin_count;

static void micro_os_apply_action(const micro_action_t *action);
static size_t micro_os_apply_batch(const micro_action_t *actions, size_t count);
static void os_dispatch_event(const micro_event_t *event);

static uint32_t read_u32_le(const uint8_t *bytes)
{
    uint32_t value = 0;
    memcpy(&value, bytes, sizeof value);
    return value;
}

/* Parse the metadata section: three put_bytes strings in order id, name, icon. */
static int micro_app_parse_metadata(const uint8_t *section, uint32_t len,
                                    micro_app_entry_t *out)
{
    const uint8_t *cursor = section;
    const uint8_t *end = section + len;
    for (int field = 0; field < 3; ++field) {
        if (cursor + 4 > end) {
            return -1;
        }
        uint32_t field_len = read_u32_le(cursor);
        cursor += 4;
        if (cursor + field_len > end) {
            return -1;
        }
        char *target = field == 0 ? out->id : (field == 1 ? out->name : out->icon);
        size_t capacity = field == 2 ? sizeof out->icon : sizeof out->name;
        size_t room = field_len < capacity - 1 ? field_len : capacity - 1;
        memcpy(target, cursor, room);
        target[room] = '\0';
        cursor += field_len;
    }
    return 0;
}

static int micro_app_scan_metadata(const uint8_t *mbc, micro_app_entry_t *out)
{
    uint32_t payload_len = read_u32_le(mbc + 6);
    const uint8_t *cursor = mbc + MICRO_MBC_HEADER_SIZE;
    const uint8_t *end = cursor + payload_len;
    while (cursor + MICRO_MBC_SECTION_HEADER_SIZE <= end) {
        uint8_t tag = cursor[0];
        uint32_t section_len = read_u32_le(cursor + 1);
        const uint8_t *section = cursor + MICRO_MBC_SECTION_HEADER_SIZE;
        if (section + section_len > end) {
            return -1;
        }
        if (tag == MICRO_MBC_SECTION_METADATA) {
            return micro_app_parse_metadata(section, section_len, out);
        }
        cursor = section + section_len;
    }
    return -1;
}

/* App ids become the per-App storage namespace, so an external MBC may only
 * claim a conservative ASCII identifier. Mirrors micro_sd's app-id rules. */
static bool micro_app_valid_id(const char *id)
{
    if (id == NULL) {
        return false;
    }
    size_t len = strlen(id);
    if (len == 0 || len >= MICRO_APP_NAME_MAX || !isalnum((unsigned char)id[0])) {
        return false;
    }
    for (size_t i = 0; i < len; ++i) {
        unsigned char ch = (unsigned char)id[i];
        if (!isalnum(ch) && ch != '_' && ch != '-' && ch != '.') {
            return false;
        }
    }
    return true;
}

/* Case-insensitive id comparison: `Counter` and `counter` are the same App. */
static bool micro_app_id_taken(const char *id)
{
    for (uint32_t i = 1; i < s_app_count; ++i) {
        if (strcasecmp(s_apps[i].id, id) == 0) {
            return true;
        }
    }
    return false;
}

/* The host registry indexes built-in Apps first, then external Apps. The shell
 * itself is registry slot 0 on flash and never appears in `os.appName`. */
static void micro_app_refresh_host_registry(void)
{
    uint32_t host_count = s_app_count > 0 ? s_app_count - 1 : 0;
    if (host_count > MICRO_APP_SLOTS) {
        host_count = MICRO_APP_SLOTS;
    }
    uint32_t builtin_count = s_builtin_count > 0 ? s_builtin_count - 1 : 0;
    uint32_t external_count =
        host_count > builtin_count ? host_count - builtin_count : 0;
    micro_esp_host_set_app_count(host_count);
    for (uint32_t i = 1; i <= host_count; ++i) {
        micro_esp_host_set_app_entry(i - 1, s_apps[i].name, s_apps[i].icon);
    }
    ESP_LOGI(TAG, "launcher registry: %" PRIu32 " app(s) (%" PRIu32
                  " built-in, %" PRIu32 " external)",
             host_count, builtin_count, external_count);
}

/* A missing/corrupt metadata section falls back to the file name so a plain
 * MBC still appears in the launcher (the runtime validates it on launch). */
static void micro_app_external_fallback(const char *file, micro_app_entry_t *out)
{
    size_t len = strlen(file);
    const char *dot = strrchr(file, '.');
    size_t stem = dot != NULL && dot != file ? (size_t)(dot - file) : len;
    size_t room = stem < sizeof out->id - 1 ? stem : sizeof out->id - 1;
    memcpy(out->id, file, room);
    out->id[room] = '\0';
    memcpy(out->name, file, room);
    out->name[room] = '\0';
    out->icon[0] = (char)toupper((unsigned char)out->id[0]);
    out->icon[1] = '\0';
}

/* Read and parse one `/apps/<app>.mbc` image into a registry entry.
 * Returns 0 on success, 1 when the file disappeared, -1 when it is unusable. */
static int micro_app_read_external(const char *file, micro_app_entry_t *out)
{
    char *bytes = NULL;
    size_t len = 0;
    if (micro_sd_read_external_app(file, &bytes, &len) != 0) {
        return -1;
    }
    if (bytes == NULL) {
        return 1;
    }
    if (len < MICRO_MBC_HEADER_SIZE || memcmp(bytes, "MBC1", 4U) != 0) {
        micro_sd_free(bytes);
        return -1;
    }
    uint32_t payload_len = read_u32_le((const uint8_t *)bytes + 6);
    if ((uint64_t)MICRO_MBC_HEADER_SIZE + payload_len != len) {
        micro_sd_free(bytes);
        return -1;
    }

    memset(out, 0, sizeof *out);
    if (micro_app_scan_metadata((const uint8_t *)bytes, out) != 0 || out->id[0] == '\0') {
        micro_app_external_fallback(file, out);
    }
    micro_sd_free(bytes);

    if (!micro_app_valid_id(out->id)) {
        return -1;
    }
    if (out->name[0] == '\0') {
        snprintf(out->name, sizeof out->name, "%s", out->id);
    }
    if (out->icon[0] == '\0') {
        out->icon[0] = (char)toupper((unsigned char)out->id[0]);
        out->icon[1] = '\0';
    }
    snprintf(out->file, sizeof out->file, "%s", file);
    out->external = true;
    return 0;
}

/* Rebuild the external tail of the registry from `/apps`. Called before every
 * shell boot, so a mounted card's contents are re-read when the user returns
 * Home. A card that was absent at startup needs a reboot (the FATFS mount is
 * initialized once); a missing card simply leaves built-ins only. */
static void micro_app_scan_external(void)
{
    s_app_count = s_builtin_count;
    if (s_builtin_count == 0) {
        return;
    }
    if (!micro_sd_available()) {
        micro_app_refresh_host_registry();
        return;
    }

    char *listing = NULL;
    size_t listing_len = 0;
    if (micro_sd_list_external_apps(&listing, &listing_len) != 0) {
        ESP_LOGW(TAG, "cannot list /sdcard/apps; ignoring external Apps");
        micro_app_refresh_host_registry();
        return;
    }
    (void)listing_len;
    if (listing == NULL) {
        micro_app_refresh_host_registry();
        return;
    }

    char *cursor = listing;
    while (*cursor != '\0' && s_app_count < MICRO_APPS_MAX) {
        char *line = cursor;
        char *newline = strchr(cursor, '\n');
        if (newline != NULL) {
            *newline = '\0';
            cursor = newline + 1;
        } else {
            cursor += strlen(cursor);
        }
        if (line[0] == '\0') {
            continue;
        }

        micro_app_entry_t entry;
        int read = micro_app_read_external(line, &entry);
        if (read < 0) {
            ESP_LOGW(TAG, "ignoring SD App %s (not a usable MBC)", line);
            continue;
        }
        if (read > 0) {
            continue;
        }
        if (strcasecmp(entry.id, "shell") == 0) {
            ESP_LOGW(TAG, "ignoring SD App %s: id '%s' is reserved for the shell",
                     line, entry.id);
            continue;
        }
        if (micro_app_id_taken(entry.id)) {
            ESP_LOGW(TAG,
                     "ignoring SD App %s: id '%s' is already registered",
                     line, entry.id);
            continue;
        }
        s_apps[s_app_count++] = entry;
        ESP_LOGI(TAG, "external App %s (%s) from /sdcard/apps/%s",
                 entry.id, entry.name, entry.file);
    }
    micro_sd_free(listing);

    if (s_app_count == MICRO_APPS_MAX) {
        ESP_LOGI(TAG, "launcher slots are full; remaining /sdcard/apps files ignored");
    }
    micro_app_refresh_host_registry();
}

/* Walk the consecutive MBCs in the micro_app partition (each is self-describing:
 * 14 + payload_len bytes). Index 0 is the shell; the rest are installed apps. */
static void micro_app_scan(void)
{
    s_partition = esp_partition_find_first(ESP_PARTITION_TYPE_DATA,
                                           ESP_PARTITION_SUBTYPE_ANY,
                                           MICRO_APP_PARTITION_LABEL);
    if (s_partition == NULL) {
        ESP_LOGE(TAG, "micro_app partition not found in partition table");
        return;
    }
    s_app_count = 0;
    uint32_t offset = 0;
    while (s_app_count < MICRO_APPS_MAX) {
        uint8_t header[MICRO_MBC_HEADER_SIZE] = {0};
        if (esp_partition_read(s_partition, offset, header, sizeof header) != ESP_OK) {
            break;
        }
        if (memcmp(header, "MBC1", 4U) != 0) {
            break;
        }
        uint32_t payload_len = read_u32_le(header + 6);
        uint32_t total = MICRO_MBC_HEADER_SIZE + payload_len;
        if (total < MICRO_MBC_HEADER_SIZE ||
            (uint64_t)offset + total > s_partition->size) {
            break;
        }
        uint8_t *mbc = heap_caps_malloc(total, MALLOC_CAP_SPIRAM);
        if (mbc == NULL) {
            ESP_LOGE(TAG, "failed to allocate %" PRIu32 " bytes for MBC scan", total);
            break;
        }
        if (esp_partition_read(s_partition, offset, mbc, total) != ESP_OK) {
            heap_caps_free(mbc);
            break;
        }
        micro_app_entry_t *entry = &s_apps[s_app_count];
        memset(entry, 0, sizeof *entry);
        entry->offset = offset;
        entry->len = total;
        if (micro_app_scan_metadata(mbc, entry) != 0) {
            snprintf(entry->id, sizeof entry->id, "app%" PRIu32, s_app_count);
            snprintf(entry->name, sizeof entry->name, "App %" PRIu32, s_app_count);
            entry->icon[0] = (char)('A' + s_app_count % 26);
        }
        heap_caps_free(mbc);
        ESP_LOGI(TAG, "scanned MBC[%" PRIu32 "] %s (%s) @ 0x%" PRIx32 " (%" PRIu32 " B)",
                 s_app_count, entry->id, entry->name, offset, total);
        offset += total;
        s_app_count += 1;
    }

    /* Remember where built-ins end. `micro_app_scan_external` truncates back to
     * this count on every shell boot and then appends `/apps` images. */
    s_builtin_count = s_app_count;
    micro_app_refresh_host_registry();
    ESP_LOGI(TAG, "scanned %" PRIu32 " built-in MBC(s); %" PRIu32
                  " installable app(s) before external Apps",
             s_app_count, s_app_count > 0 ? s_app_count - 1 : 0);
}

static int micro_runtime_boot_index(uint32_t registry_index)
{
    if (registry_index >= s_app_count || s_app_count == 0) {
        ESP_LOGE(TAG, "boot index %" PRIu32 " out of range", registry_index);
        return -1;
    }
    if (s_runtime != NULL) {
        micro_runtime_destroy(s_runtime);
        s_runtime = NULL;
    }
    const micro_app_entry_t *entry = &s_apps[registry_index];
    uint8_t *mbc = NULL;
    uint32_t mbc_len = 0;
    if (entry->external) {
        char *bytes = NULL;
        size_t len = 0;
        if (micro_sd_read_external_app(entry->file, &bytes, &len) != 0 ||
            bytes == NULL || len == 0) {
            ESP_LOGE(TAG, "failed to read external MBC %s from /sdcard/apps/%s",
                     entry->id, entry->file);
            micro_sd_free(bytes);
            return -1;
        }
        mbc = (uint8_t *)bytes;
        mbc_len = (uint32_t)len;
    } else {
        mbc = heap_caps_malloc(entry->len, MALLOC_CAP_SPIRAM);
        if (mbc == NULL) {
            ESP_LOGE(TAG, "failed to allocate %" PRIu32 " bytes for %s", entry->len,
                     entry->id);
            return -1;
        }
        esp_err_t result =
            esp_partition_read(s_partition, entry->offset, mbc, entry->len);
        if (result != ESP_OK) {
            ESP_LOGE(TAG, "failed to read MBC %s: %s", entry->id,
                     esp_err_to_name(result));
            heap_caps_free(mbc);
            return -1;
        }
        mbc_len = entry->len;
    }
    /* Every runtime — the shell home screen included — gets the default OS app
     * bar with the live clock/Wi-Fi and a close control; the App title is the
     * MBC's display name ("Home" for the shell). Set this before creating the
     * Runtime so its root layout uses the inset content height. */
    micro_esp_ui_set_app_chrome(1, entry->name);
    micro_runtime_t *runtime = micro_runtime_create(
        mbc, mbc_len, MICRO_RUNTIME_EVENT_BUDGET, s_runtime_error,
        sizeof s_runtime_error);
    if (entry->external) {
        micro_sd_free(mbc);
    } else {
        heap_caps_free(mbc);
    }
    if (runtime == NULL) {
        ESP_LOGE(TAG, "micro_runtime_create failed: %s", s_runtime_error);
        return -1;
    }
    s_runtime = runtime;
    s_is_shell = (registry_index == 0);
    micro_esp_ui_settle_after_switch();
    ESP_LOGI(TAG, "%s runtime created: %s", s_is_shell ? "shell" : "app", entry->id);
    return 0;
}

static int micro_runtime_boot_shell(void)
{
    /* Refresh `/apps` before each Home boot: a mounted card may have changed
     * while an App was running. The shell MBC binds the updated host registry
     * when its Runtime is created. */
    micro_app_scan_external();
    return micro_runtime_boot_index(0);
}

static int micro_runtime_boot_app(uint32_t host_index)
{
    return micro_runtime_boot_index(host_index + 1);
}

static void micro_backlight_apply(uint32_t level)
{
    s_backlight = level > 4 ? 4 : level;
    micro_esp_host_mirror_backlight(s_backlight);
    micro_bsp_backlight_set(&s_display, s_backlight > 1);
}

/* --- OS reducer (boot chain + reboot; runtime switching is direct) --- */

static void os_dispatch_event(const micro_event_t *event)
{
    s_action_buffer.actions = s_actions;
    s_action_buffer.capacity = MICRO_OS_MAX_ACTIONS;
    s_action_buffer.count = 0;
    s_action_buffer.required = 0;
    micro_error_t result =
        micro_os_dispatch(s_os, event, &s_action_buffer, s_runtime_error,
                          sizeof s_runtime_error);
    if (result != MICRO_OK) {
        ESP_LOGE(TAG, "os dispatch failed: code=%d message=%s", (int)result,
                 s_runtime_error);
        return;
    }
    micro_os_apply_batch(s_action_buffer.actions, s_action_buffer.count);
}

static size_t micro_os_apply_batch(const micro_action_t *actions, size_t count)
{
    if (count == 0) {
        return 0;
    }
    const micro_action_t *action = &actions[0];
    if (action->kind == MICRO_ACTION_ACTIONS) {
        size_t consumed = 1;
        for (uint32_t child = 0; child < action->child_count; ++child) {
            consumed += micro_os_apply_batch(&actions[consumed], count - consumed);
        }
        return consumed;
    }
    micro_os_apply_action(action);
    return 1;
}

static void micro_os_apply_action(const micro_action_t *action)
{
    switch (action->kind) {
    case MICRO_ACTION_SHOW_APP_ERROR:
        ESP_LOGW(TAG, "app error page (reason=%d); returning to shell",
                 (int)action->failure);
        micro_runtime_boot_shell();
        break;
    case MICRO_ACTION_REBOOT:
        ESP_LOGW(TAG, "reboot requested");
        esp_restart();
        break;
    default:
        /* The shell MBC owns all OS UI; the reducer's launcher/settings/Wi-Fi
         * actions are not driven on-device. */
        break;
    }
}

/* --- host-intent drain (app → kernel via micro_esp_host pending intents) --- */

static void micro_os_drain_host_intents(void)
{
    uint32_t backlight = 0;
    if (micro_esp_host_take_backlight_intent(&backlight)) {
        micro_backlight_apply(backlight);
    }
    char ssid[33] = {0};
    char pass[65] = {0};
    if (micro_esp_host_take_wifi_connect(ssid, sizeof ssid, pass, sizeof pass)) {
        ESP_LOGI(TAG, "app requested Wi-Fi connect to %s", ssid);
        micro_wifi_connect(ssid, pass);
    }
    if (micro_esp_host_take_wifi_disconnect()) {
        ESP_LOGI(TAG, "app requested Wi-Fi disconnect");
        micro_wifi_disconnect();
    }
    char title[64] = {0};
    if (micro_esp_host_take_title(title, sizeof title)) {
        /* Apply before launch/back so booting another runtime's chrome wins in
         * the same tick. Requires the LVGL port lock, which this timer holds. */
        micro_esp_ui_set_appbar_title(title);
    }
    uint32_t launch = 0;
    if (micro_esp_host_take_launch_index(&launch)) {
        ESP_LOGI(TAG, "shell requested launch of app index %" PRIu32, launch);
        if (s_is_shell && micro_runtime_boot_app(launch) != 0) {
            ESP_LOGE(TAG, "app %" PRIu32 " failed to start; returning to Home", launch);
            if (micro_runtime_boot_shell() != 0) {
                ESP_LOGE(TAG, "Home failed to restart after app launch failure");
            }
        }
    }
    if (micro_esp_host_take_back()) {
        ESP_LOGI(TAG, "app requested goBack");
        if (!s_is_shell) {
            micro_runtime_boot_shell();
        }
    }
}

/* --- edge-swipe back gesture (Android gesture-nav style) --- */

static struct {
    bool active;
    int edge; /* 0 = none, 1 = left, 2 = right */
    int start_x;
    int start_y;
    int last_x;
    int last_y;
} s_swipe;

static void micro_os_swipe_tick(void)
{
    int x = 0;
    int y = 0;
    if (micro_bsp_touch_read(&s_display, &x, &y)) {
        if (!s_swipe.active) {
            s_swipe.active = true;
            s_swipe.start_x = x;
            s_swipe.start_y = y;
            s_swipe.last_x = x;
            s_swipe.last_y = y;
            s_swipe.edge = x < MICRO_SWIPE_EDGE_ZONE
                               ? 1
                               : (x >= MICRO_BSP_LCD_WIDTH - MICRO_SWIPE_EDGE_ZONE ? 2 : 0);
        } else {
            s_swipe.last_x = x;
            s_swipe.last_y = y;
        }
        return;
    }
    if (!s_swipe.active) {
        return;
    }
    /* Finger lifted: was it an inward edge swipe? */
    s_swipe.active = false;
    if (s_swipe.edge != 0) {
        int dx = s_swipe.last_x - s_swipe.start_x;
        int dy = s_swipe.last_y - s_swipe.start_y;
        int inward = s_swipe.edge == 1 ? dx : -dx;
        int adx = dx < 0 ? -dx : dx;
        int ady = dy < 0 ? -dy : dy;
        if (inward >= MICRO_SWIPE_THRESHOLD && adx >= 2 * ady) {
            ESP_LOGI(TAG, "edge swipe back");
            if (!s_is_shell) {
                micro_runtime_boot_shell();
            }
        }
    }
}

/* --- BOOT button (acts as Back) --- */

static void micro_boot_button_init(void)
{
    gpio_config_t config = {
        .pin_bit_mask = 1ULL << MICRO_BOOT_BUTTON_GPIO,
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_ENABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    gpio_config(&config);
    s_boot_level_count = 0;
}

static bool micro_boot_button_back_pressed(void)
{
    bool pressed = gpio_get_level(MICRO_BOOT_BUTTON_GPIO) == 0;
    if (pressed) {
        if (s_boot_level_count < MICRO_BOOT_DEBOUNCE_TICKS) {
            s_boot_level_count++;
        }
        return s_boot_level_count >= MICRO_BOOT_DEBOUNCE_TICKS;
    }
    s_boot_level_count = 0;
    return false;
}

/* The Wi-Fi radio reaches its live state asynchronously (the wifi task writes
 * it in the event handler), so an App's Wi-Fi bindings would otherwise stay
 * frozen at whatever the radio reported at mount. On each transition,
 * re-evaluate every binding; the runtime only patches the ones that changed.
 * (The OS app bar's Wi-Fi icon is updated separately from the same state.) */
static void micro_os_sync_wifi_state(void)
{
    if (s_runtime == NULL) {
        return;
    }
    char state[MICRO_WIFI_STATE_CAP];
    if (micro_esp_host_wifi_state(state, sizeof state) != 0 ||
        strcmp(state, s_last_wifi_state) == 0) {
        return;
    }
    strcpy(s_last_wifi_state, state);
    micro_error_t result = micro_runtime_reconcile(s_runtime, s_runtime_error,
                                                   sizeof s_runtime_error);
    if (result != MICRO_OK) {
        ESP_LOGE(TAG, "runtime reconcile failed: code=%d message=%s",
                 (int)result, s_runtime_error);
    }
}

static void micro_os_tick_cb(lv_timer_t *timer)
{
    (void)timer;

    /* Powered off: the LVGL timer still runs so the off surface can receive
     * taps, but the shell/app runtime is not ticked and Wi-Fi state is frozen.
     * A tap (or the BOOT button) boots a fresh Home. */
    if (micro_esp_ui_power_off_active()) {
        if (micro_esp_ui_take_power_on() || micro_boot_button_back_pressed()) {
            ESP_LOGI(TAG, "power on: booting the shell");
            micro_esp_ui_wake();
            micro_runtime_boot_shell();
        }
        return;
    }

    micro_os_drain_host_intents();

    micro_os_sync_wifi_state();

    /* Keep the app bar's Wi-Fi icon and SSID tooltip live; the shared bar
     * caches the tooltip text so this is a cheap string compare on every tick. */
    {
        char wifi_state[MICRO_WIFI_STATE_CAP] = {0};
        char wifi_ssid[33] = {0};
        micro_esp_host_wifi_state(wifi_state, sizeof wifi_state);
        micro_esp_host_wifi_ssid(wifi_ssid, sizeof wifi_ssid);
        micro_esp_ui_set_app_wifi(wifi_state, wifi_ssid);
    }

    micro_os_swipe_tick();

    if (micro_esp_ui_take_appbar_back()) {
        if (s_is_shell) {
            ESP_LOGI(TAG, "app bar close: powering off");
            micro_esp_ui_power_off();
        } else {
            ESP_LOGI(TAG, "app bar close: returning to shell");
            micro_runtime_boot_shell();
        }
    }

    if (micro_boot_button_back_pressed()) {
        if (!s_is_shell) {
            ESP_LOGI(TAG, "BOOT back: returning to shell");
            micro_runtime_boot_shell();
        }
    }

    if (s_runtime != NULL) {
        micro_error_t result = micro_runtime_tick(s_runtime, s_runtime_error,
                                                  sizeof s_runtime_error);
        if (result != MICRO_OK) {
            ESP_LOGE(TAG, "runtime tick failed: code=%d message=%s",
                     (int)result, s_runtime_error);
            micro_runtime_boot_shell();
        }
    }
}

void app_main(void)
{
    uint32_t flash_size = 0;
    esp_err_t flash_result = esp_flash_get_size(NULL, &flash_size);
    size_t psram_size = esp_psram_get_size();

    ESP_LOGI(TAG, "reset reason: %d", (int)esp_reset_reason());

    if (flash_result != ESP_OK) {
        ESP_LOGE(TAG, "failed to detect Flash size: %s", esp_err_to_name(flash_result));
        abort();
    }

    ESP_LOGI(TAG, "detected Flash: %" PRIu32 " bytes", flash_size);
    ESP_LOGI(TAG, "detected PSRAM: %zu bytes", psram_size);

    if (flash_size != MICRO_EXPECTED_MEMORY_BYTES ||
        psram_size != MICRO_EXPECTED_MEMORY_BYTES) {
        ESP_LOGE(TAG,
                 "unsupported memory class: expected 8 MB Flash and 8 MB PSRAM");
        abort();
    }

    ESP_LOGI(TAG, "8 MB Flash / 8 MB PSRAM hardware class verified");

    esp_err_t result = micro_bsp_init(&s_display);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "LCD7 BSP initialization failed: %s", esp_err_to_name(result));
        abort();
    }

    result = micro_bsp_backlight_set(&s_display, true);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "backlight initialization failed: %s", esp_err_to_name(result));
        abort();
    }

    /* The TF card is optional: a missing card must not stop the OS from
     * booting. `storage.available()` reports the mount result to Apps. */
    result = micro_sd_init();
    if (result != ESP_OK) {
        ESP_LOGW(TAG, "external storage unavailable: %s", esp_err_to_name(result));
    }

    if (!lvgl_port_lock(0)) {
        ESP_LOGE(TAG, "could not lock LVGL for health screen");
        abort();
    }
    result = micro_bsp_draw_smoke_screen(&s_display);
    lvgl_port_unlock();
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "health screen failed: %s", esp_err_to_name(result));
        abort();
    }

    result = micro_bsp_mark_healthy();
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "profile health commit failed: %s", esp_err_to_name(result));
        abort();
    }

    ESP_LOGI(TAG, "clearing smoke screen for OS shell bring-up");
    if (!lvgl_port_lock(0)) {
        ESP_LOGE(TAG, "could not lock LVGL to clear smoke screen");
        abort();
    }
    lv_obj_clean(lv_screen_active());
    lv_obj_set_style_bg_color(lv_screen_active(), lv_color_hex(0xF2F4F0), 0);
    lv_obj_set_style_bg_opa(lv_screen_active(), LV_OPA_COVER, 0);
    lvgl_port_unlock();

    micro_boot_button_init();
    micro_backlight_apply(3);

    /* Bring up the STA radio (default NVS, netif, event loop). A network
     * persisted by a previous connect auto-reconnects from this point. */
    micro_wifi_init();
    micro_kv_init();
    micro_clock_init();

    s_os = micro_os_create();
    if (s_os == NULL) {
        ESP_LOGE(TAG, "micro_os_create failed");
        abort();
    }

    /* The result field is only carried by the *_INITIALIZED events; the other
     * boot events reject a non-UNUSED result (validate_canonical). */
    micro_event_t boot_events[] = {
        {.kind = MICRO_EVENT_BOOT_SAMPLED, .flag = 0},
        {.kind = MICRO_EVENT_STORAGE_INITIALIZED, .result = MICRO_RESULT_OK},
        {.kind = MICRO_EVENT_PROFILE_VALIDATED, .result = MICRO_RESULT_OK},
        {.kind = MICRO_EVENT_DISPLAY_INITIALIZED, .result = MICRO_RESULT_OK},
        {.kind = MICRO_EVENT_SYSTEM_UI_INITIALIZED, .result = MICRO_RESULT_OK},
        {.kind = MICRO_EVENT_NETWORK_CONFIG_LOADED, .flag = 0},
        {.kind = MICRO_EVENT_SETUP_SKIPPED},
    };
    for (size_t i = 0; i < sizeof boot_events / sizeof boot_events[0]; ++i) {
        os_dispatch_event(&boot_events[i]);
    }

    /* Scan the app partition and boot the shell MBC (index 0). */
    micro_app_scan();
    if (micro_runtime_boot_shell() != 0) {
        ESP_LOGE(TAG, "failed to boot the OS shell; aborting");
        abort();
    }

    ESP_LOGI(TAG, "OS shell ready; ticking every %u ms",
             (unsigned)MICRO_RUNTIME_TICK_PERIOD_MS);
    lv_timer_create(micro_os_tick_cb, MICRO_RUNTIME_TICK_PERIOD_MS, NULL);
}
