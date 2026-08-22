#include <inttypes.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>

#include "esp_err.h"
#include "esp_flash.h"
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_partition.h"
#include "esp_psram.h"
#include "esp_system.h"

#include "lvgl.h"

#include "micro_bsp_lcd7.h"
#include "micro_runtime_ffi.h"

#define MICRO_EXPECTED_MEMORY_BYTES (8U * 1024U * 1024U)
#define MICRO_APP_PARTITION_LABEL "micro_app"
#define MICRO_RUNTIME_EVENT_BUDGET 10000U
#define MICRO_RUNTIME_TICK_PERIOD_MS 30U
#define MICRO_RUNTIME_ERROR_BUFFER_SIZE 192U

/* MBC1 file layout (little-endian, see crates/micro-ir/src/codec.rs):
 *   offset 0..4   magic "MBC1"
 *   offset 4..6   version (u16)
 *   offset 6..10  payload length (u32)
 *   offset 10..14 crc32 (u32) over the payload
 *   offset 14..   payload bytes
 * The total file size is therefore 14 + payload_length. */
#define MICRO_MBC_HEADER_SIZE 14U

static const char *TAG = "micro_os";

static void micro_draw_error_label(const char *message)
{
    lv_obj_t *screen = lv_screen_active();
    lv_obj_clean(screen);
    lv_obj_set_style_bg_color(screen, lv_color_hex(0x200000), 0);
    lv_obj_set_style_bg_opa(screen, LV_OPA_COVER, 0);

    lv_obj_t *label = lv_label_create(screen);
    lv_label_set_text(label, message);
    lv_obj_set_style_text_color(label, lv_color_hex(0xFFFFFF), 0);
    lv_obj_set_width(label, LV_PCT(90));
    lv_label_set_long_mode(label, LV_LABEL_LONG_WRAP);
    lv_obj_align(label, LV_ALIGN_CENTER, 0, 0);
}

static void micro_runtime_tick_cb(lv_timer_t *timer)
{
    micro_runtime_t *runtime = (micro_runtime_t *)lv_timer_get_user_data(timer);
    if (runtime == NULL) {
        return;
    }
    char error[MICRO_RUNTIME_ERROR_BUFFER_SIZE] = {0};
    micro_error_t result = micro_runtime_tick(runtime, error, sizeof(error));
    if (result != MICRO_OK) {
        ESP_LOGE(TAG, "runtime tick failed: code=%d message=%s",
                 (int)result, error);
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

    micro_bsp_display_t display;
    esp_err_t result = micro_bsp_init(&display);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "LCD7 BSP initialization failed: %s", esp_err_to_name(result));
        abort();
    }

    result = micro_bsp_backlight_set(&display, true);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "backlight initialization failed: %s", esp_err_to_name(result));
        abort();
    }

    if (!lvgl_port_lock(0)) {
        ESP_LOGE(TAG, "could not lock LVGL for health screen");
        abort();
    }
    result = micro_bsp_draw_smoke_screen(&display);
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

    ESP_LOGI(TAG, "clearing smoke screen for MBC runtime bring-up");
    if (!lvgl_port_lock(0)) {
        ESP_LOGE(TAG, "could not lock LVGL to clear smoke screen");
        abort();
    }
    lv_obj_clean(lv_screen_active());
    lv_obj_set_style_bg_color(lv_screen_active(), lv_color_hex(0xFFFFFF), 0);
    lv_obj_set_style_bg_opa(lv_screen_active(), LV_OPA_COVER, 0);
    lvgl_port_unlock();

    const esp_partition_t *app_partition = esp_partition_find_first(
        ESP_PARTITION_TYPE_DATA, ESP_PARTITION_SUBTYPE_ANY, MICRO_APP_PARTITION_LABEL);
    if (app_partition == NULL) {
        ESP_LOGE(TAG, "micro_app partition not found in partition table");
        if (lvgl_port_lock(0)) {
            micro_draw_error_label("micro_app partition missing from table");
            lvgl_port_unlock();
        }
        abort();
    }
    ESP_LOGI(TAG, "micro_app partition: offset=0x%" PRIx32 " size=%" PRIu32,
             app_partition->address, (uint32_t)app_partition->size);

    uint8_t header[MICRO_MBC_HEADER_SIZE] = {0};
    result = esp_partition_read(app_partition, 0, header, sizeof(header));
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "failed to read MBC header: %s", esp_err_to_name(result));
        abort();
    }
    if (memcmp(header, "MBC1", 4U) != 0) {
        ESP_LOGE(TAG, "no MBC1 magic in micro_app partition (got %02x %02x %02x %02x)",
                 header[0], header[1], header[2], header[3]);
        if (lvgl_port_lock(0)) {
            micro_draw_error_label("micro_app partition is empty (no MBC1 magic)\nRe-flash micro_app.bin");
            lvgl_port_unlock();
        }
        abort();
    }
    uint16_t mbc_version = (uint16_t)header[4] | ((uint16_t)header[5] << 8);
    uint32_t payload_length = 0U;
    memcpy(&payload_length, header + 6U, sizeof(payload_length));
    const uint64_t total_size = (uint64_t)MICRO_MBC_HEADER_SIZE + payload_length;
    if (total_size > app_partition->size || total_size > UINT32_MAX) {
        ESP_LOGE(TAG, "MBC payload length %" PRIu32 " exceeds partition size",
                 payload_length);
        if (lvgl_port_lock(0)) {
            micro_draw_error_label("MBC payload length out of range");
            lvgl_port_unlock();
        }
        abort();
    }
    const size_t mbc_size = (size_t)total_size;
    ESP_LOGI(TAG, "MBC header: magic OK, version=%u, payload=%" PRIu32
             ", total=%zu",
             (unsigned)mbc_version, payload_length, mbc_size);

    uint8_t *app_buffer = heap_caps_malloc(mbc_size, MALLOC_CAP_SPIRAM);
    if (app_buffer == NULL) {
        ESP_LOGE(TAG, "failed to allocate %zu bytes in PSRAM for MBC", mbc_size);
        abort();
    }
    result = esp_partition_read(app_partition, 0, app_buffer, mbc_size);
    if (result != ESP_OK) {
        ESP_LOGE(TAG, "failed to read MBC body: %s", esp_err_to_name(result));
        heap_caps_free(app_buffer);
        abort();
    }
    ESP_LOGI(TAG, "loaded MBC: %zu bytes from micro_app partition", mbc_size);

    char runtime_error[MICRO_RUNTIME_ERROR_BUFFER_SIZE] = {0};
    micro_runtime_t *runtime = micro_runtime_create(
        app_buffer, mbc_size, MICRO_RUNTIME_EVENT_BUDGET,
        runtime_error, sizeof(runtime_error));
    /* The decoder copies what it needs; the raw MBC buffer is no longer
     * referenced after micro_runtime_create returns successfully. */
    heap_caps_free(app_buffer);
    app_buffer = NULL;

    if (runtime == NULL) {
        ESP_LOGE(TAG, "micro_runtime_create failed: %s", runtime_error);
        if (lvgl_port_lock(0)) {
            micro_draw_error_label(runtime_error[0] != '\0'
                                       ? runtime_error
                                       : "micro_runtime_create failed");
            lvgl_port_unlock();
        }
        abort();
    }
    ESP_LOGI(TAG, "micro runtime created; ticking every %u ms",
             (unsigned)MICRO_RUNTIME_TICK_PERIOD_MS);

    lv_timer_create(micro_runtime_tick_cb, MICRO_RUNTIME_TICK_PERIOD_MS, runtime);
}
