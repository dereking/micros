#include <inttypes.h>
#include <stdint.h>
#include <stdlib.h>

#include "esp_err.h"
#include "esp_flash.h"
#include "esp_log.h"
#include "esp_psram.h"
#include "esp_system.h"

#include "micro_bsp_lcd7.h"

#define MICRO_EXPECTED_MEMORY_BYTES (8U * 1024U * 1024U)

static const char *TAG = "micro_os";

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
}
