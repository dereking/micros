/*
 * SPDX-License-Identifier: CC0-1.0
 *
 * Board initialization is adapted from the CC0 Waveshare/Spotpear reference
 * identified in third_party/NOTICE.md, using the ESP-IDF 5.5 I2C master API
 * and the pinned LVGL 9 port.
 */

#include "micro_bsp_lcd7.h"

#include <inttypes.h>
#include <stdio.h>
#include <string.h>

#include "driver/gpio.h"
#include "esp_check.h"
#include "esp_lcd_io_i2c.h"
#include "esp_lcd_panel_rgb.h"
#include "esp_lcd_touch_gt911.h"
#include "esp_log.h"
#include "esp_rom_sys.h"
#include "nvs.h"
#include "nvs_flash.h"

#define MICRO_BSP_PROFILE_PARTITION "micro_cfg"
#define MICRO_BSP_PROFILE_NAMESPACE "profile"
#define MICRO_BSP_PENDING_PROFILE_KEY "pending"
#define MICRO_BSP_ACTIVE_PROFILE_KEY "active"
#define MICRO_BSP_PROFILE_MAGIC UINT32_C(0x4D425350)
#define MICRO_BSP_PROFILE_VERSION 1U
#define MICRO_BSP_I2C_TIMEOUT_MS 1000
#define MICRO_BSP_CH422G_MODE_ADDRESS 0x24
#define MICRO_BSP_CH422G_OUTPUT_ADDRESS 0x38
#define MICRO_BSP_CH422G_OUTPUT_MODE 0x01
#define MICRO_BSP_CH422G_TOUCH_RESET_ASSERTED 0x2C
#define MICRO_BSP_CH422G_TOUCH_RESET_RELEASED 0x2E
#define MICRO_BSP_CH422G_BACKLIGHT_OFF 0x1A
#define MICRO_BSP_CH422G_BACKLIGHT_ON 0x1E

static const char *TAG = "micro_bsp_lcd7";

static const int MICRO_BSP_LCD_DATA_GPIOS[] = {
    14, 38, 18, 17, 10, 39, 0, 45,
    48, 47, 21, 1, 2, 42, 41, 40,
};

_Static_assert(sizeof(MICRO_BSP_LCD_DATA_GPIOS) / sizeof(MICRO_BSP_LCD_DATA_GPIOS[0]) == 16,
               "RGB565 needs exactly sixteen data lines");

typedef struct {
    uint32_t magic;
    uint16_t version;
    uint16_t width;
    uint16_t height;
    uint32_t pixel_clock_hz;
} micro_bsp_profile_record_t;

typedef struct {
    i2c_master_dev_handle_t mode;
    i2c_master_dev_handle_t output;
} micro_bsp_ch422g_t;

static micro_bsp_ch422g_t s_ch422g;
static bool s_pending_profile;
static lv_obj_t *s_touch_label;
static esp_lcd_touch_handle_t s_smoke_touch;

static void micro_bsp_update_touch_coordinates(lv_timer_t *timer)
{
    (void)timer;
    if (s_touch_label == NULL || !lv_obj_is_valid(s_touch_label) ||
        s_smoke_touch == NULL) {
        return;
    }

    esp_lcd_touch_point_data_t point = {0};
    uint8_t point_count = 0;
    esp_err_t result = esp_lcd_touch_read_data(s_smoke_touch);
    if (result == ESP_OK) {
        result = esp_lcd_touch_get_data(s_smoke_touch, &point, &point_count, 1);
    }
    if (result != ESP_OK) {
        lv_label_set_text(s_touch_label, "Touch: read error");
        return;
    }
    if (point_count == 0) {
        lv_label_set_text(s_touch_label, "Touch: waiting for input");
        return;
    }

    char coordinates[40];
    (void)snprintf(coordinates, sizeof(coordinates), "Touch: %u, %u",
                   (unsigned int)point.x, (unsigned int)point.y);
    lv_label_set_text(s_touch_label, coordinates);
}

bool micro_bsp_touch_read(micro_bsp_display_t *display, int *x, int *y)
{
    if (display == NULL || display->touch == NULL || x == NULL || y == NULL) {
        return false;
    }
    esp_lcd_touch_point_data_t point = {0};
    uint8_t point_count = 0;
    if (esp_lcd_touch_read_data(display->touch) != ESP_OK) {
        return false;
    }
    if (esp_lcd_touch_get_data(display->touch, &point, &point_count, 1) != ESP_OK) {
        return false;
    }
    if (point_count == 0) {
        return false;
    }
    *x = (int)point.x;
    *y = (int)point.y;
    return true;
}

static bool micro_bsp_profile_is_valid(const micro_bsp_profile_record_t *profile)
{
    return profile->magic == MICRO_BSP_PROFILE_MAGIC &&
           profile->version == MICRO_BSP_PROFILE_VERSION &&
           profile->width == MICRO_BSP_LCD_WIDTH &&
           profile->height == MICRO_BSP_LCD_HEIGHT &&
           profile->pixel_clock_hz == MICRO_BSP_LCD_PCLK_HZ;
}

static esp_err_t micro_bsp_profile_prepare(void)
{
    esp_err_t result = nvs_flash_init_partition(MICRO_BSP_PROFILE_PARTITION);
    if (result == ESP_ERR_NVS_NO_FREE_PAGES || result == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_RETURN_ON_ERROR(nvs_flash_erase_partition(MICRO_BSP_PROFILE_PARTITION), TAG,
                            "erase profile partition failed");
        result = nvs_flash_init_partition(MICRO_BSP_PROFILE_PARTITION);
    }
    ESP_RETURN_ON_ERROR(result, TAG, "profile partition initialization failed");

    nvs_handle_t handle;
    ESP_RETURN_ON_ERROR(nvs_open_from_partition(MICRO_BSP_PROFILE_PARTITION,
                                                MICRO_BSP_PROFILE_NAMESPACE,
                                                NVS_READWRITE, &handle), TAG,
                        "open profile namespace failed");

    micro_bsp_profile_record_t pending = {0};
    size_t size = sizeof(pending);
    result = nvs_get_blob(handle, MICRO_BSP_PENDING_PROFILE_KEY, &pending, &size);
    if (result == ESP_ERR_NVS_NOT_FOUND) {
        s_pending_profile = false;
        nvs_close(handle);
        return ESP_OK;
    }
    if (result != ESP_OK || size != sizeof(pending) || !micro_bsp_profile_is_valid(&pending)) {
        ESP_LOGW(TAG, "discarding invalid pending board profile");
        result = nvs_erase_key(handle, MICRO_BSP_PENDING_PROFILE_KEY);
        if (result == ESP_OK) {
            result = nvs_commit(handle);
        }
        nvs_close(handle);
        return result;
    }

    s_pending_profile = true;
    nvs_close(handle);
    return ESP_OK;
}

static void micro_bsp_profile_rollback(void)
{
    if (!s_pending_profile) {
        return;
    }

    nvs_handle_t handle;
    if (nvs_open_from_partition(MICRO_BSP_PROFILE_PARTITION, MICRO_BSP_PROFILE_NAMESPACE,
                                NVS_READWRITE, &handle) == ESP_OK) {
        (void)nvs_erase_key(handle, MICRO_BSP_PENDING_PROFILE_KEY);
        (void)nvs_commit(handle);
        nvs_close(handle);
    }
    s_pending_profile = false;
}

static esp_err_t micro_bsp_ch422g_write(i2c_master_dev_handle_t device, uint8_t value)
{
    return i2c_master_transmit(device, &value, sizeof(value), MICRO_BSP_I2C_TIMEOUT_MS);
}

static esp_err_t micro_bsp_ch422g_set_output_mode(void)
{
    return micro_bsp_ch422g_write(s_ch422g.mode, MICRO_BSP_CH422G_OUTPUT_MODE);
}

static esp_err_t micro_bsp_touch_reset(void)
{
    ESP_RETURN_ON_ERROR(micro_bsp_ch422g_set_output_mode(), TAG, "CH422G mode failed");
    ESP_RETURN_ON_ERROR(micro_bsp_ch422g_write(s_ch422g.output,
                                                MICRO_BSP_CH422G_TOUCH_RESET_ASSERTED), TAG,
                        "assert touch reset failed");

    const gpio_config_t irq_drive_low = {
        .pin_bit_mask = UINT64_C(1) << MICRO_BSP_TOUCH_IRQ_GPIO,
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    ESP_RETURN_ON_ERROR(gpio_config(&irq_drive_low), TAG, "configure touch IRQ failed");
    ESP_RETURN_ON_ERROR(gpio_set_level(MICRO_BSP_TOUCH_IRQ_GPIO, 0), TAG,
                        "select GT911 address failed");
    esp_rom_delay_us(100000);
    ESP_RETURN_ON_ERROR(micro_bsp_ch422g_write(s_ch422g.output,
                                                MICRO_BSP_CH422G_TOUCH_RESET_RELEASED), TAG,
                        "release touch reset failed");
    esp_rom_delay_us(200000);

    const gpio_config_t irq_input = {
        .pin_bit_mask = UINT64_C(1) << MICRO_BSP_TOUCH_IRQ_GPIO,
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_ENABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_NEGEDGE,
    };
    return gpio_config(&irq_input);
}

static esp_err_t micro_bsp_i2c_init(micro_bsp_display_t *display)
{
    const i2c_master_bus_config_t bus_config = {
        .i2c_port = MICRO_BSP_I2C_PORT,
        .sda_io_num = MICRO_BSP_I2C_SDA_GPIO,
        .scl_io_num = MICRO_BSP_I2C_SCL_GPIO,
        .clk_source = I2C_CLK_SRC_DEFAULT,
        .glitch_ignore_cnt = 7,
        .flags.enable_internal_pullup = true,
    };
    ESP_RETURN_ON_ERROR(i2c_new_master_bus(&bus_config, &display->i2c_bus), TAG,
                        "create I2C master bus failed");

    const i2c_device_config_t device_config = {
        .dev_addr_length = I2C_ADDR_BIT_LEN_7,
        .scl_speed_hz = 400000,
    };
    i2c_device_config_t mode_config = device_config;
    mode_config.device_address = MICRO_BSP_CH422G_MODE_ADDRESS;
    ESP_RETURN_ON_ERROR(i2c_master_bus_add_device(display->i2c_bus, &mode_config,
                                                   &s_ch422g.mode), TAG,
                        "add CH422G mode device failed");
    i2c_device_config_t output_config = device_config;
    output_config.device_address = MICRO_BSP_CH422G_OUTPUT_ADDRESS;
    return i2c_master_bus_add_device(display->i2c_bus, &output_config, &s_ch422g.output);
}

static esp_err_t micro_bsp_rgb_init(micro_bsp_display_t *display)
{
    const esp_lcd_rgb_panel_config_t panel_config = {
        .clk_src = LCD_CLK_SRC_PLL160M,
        .timings = {
            .pclk_hz = MICRO_BSP_LCD_PCLK_HZ,
            .h_res = MICRO_BSP_LCD_WIDTH,
            .v_res = MICRO_BSP_LCD_HEIGHT,
            .hsync_pulse_width = 4,
            .hsync_back_porch = 8,
            .hsync_front_porch = 8,
            .vsync_pulse_width = 4,
            .vsync_back_porch = 8,
            .vsync_front_porch = 8,
            .flags.pclk_active_neg = true,
        },
        .data_width = 16,
        .bits_per_pixel = 16,
        .num_fbs = 2,
        .bounce_buffer_size_px = MICRO_BSP_LCD_WIDTH * MICRO_BSP_RGB_BOUNCE_BUFFER_LINES,
        .sram_trans_align = 4,
        .dma_burst_size = 64,
        .hsync_gpio_num = MICRO_BSP_LCD_HSYNC_GPIO,
        .vsync_gpio_num = MICRO_BSP_LCD_VSYNC_GPIO,
        .de_gpio_num = MICRO_BSP_LCD_DE_GPIO,
        .pclk_gpio_num = MICRO_BSP_LCD_PCLK_GPIO,
        .disp_gpio_num = GPIO_NUM_NC,
        .data_gpio_nums = {
            MICRO_BSP_LCD_DATA_GPIOS[0], MICRO_BSP_LCD_DATA_GPIOS[1],
            MICRO_BSP_LCD_DATA_GPIOS[2], MICRO_BSP_LCD_DATA_GPIOS[3],
            MICRO_BSP_LCD_DATA_GPIOS[4], MICRO_BSP_LCD_DATA_GPIOS[5],
            MICRO_BSP_LCD_DATA_GPIOS[6], MICRO_BSP_LCD_DATA_GPIOS[7],
            MICRO_BSP_LCD_DATA_GPIOS[8], MICRO_BSP_LCD_DATA_GPIOS[9],
            MICRO_BSP_LCD_DATA_GPIOS[10], MICRO_BSP_LCD_DATA_GPIOS[11],
            MICRO_BSP_LCD_DATA_GPIOS[12], MICRO_BSP_LCD_DATA_GPIOS[13],
            MICRO_BSP_LCD_DATA_GPIOS[14], MICRO_BSP_LCD_DATA_GPIOS[15],
        },
        .flags.fb_in_psram = true,
    };
    ESP_RETURN_ON_ERROR(esp_lcd_new_rgb_panel(&panel_config, &display->panel), TAG,
                        "create RGB panel failed");
    return esp_lcd_panel_init(display->panel);
}

static esp_err_t micro_bsp_touch_init(micro_bsp_display_t *display)
{
    ESP_RETURN_ON_ERROR(micro_bsp_touch_reset(), TAG, "touch reset failed");

    const esp_lcd_panel_io_i2c_config_t io_config = ESP_LCD_TOUCH_IO_I2C_GT911_CONFIG();
    esp_lcd_panel_io_handle_t touch_io = NULL;
    ESP_RETURN_ON_ERROR(esp_lcd_new_panel_io_i2c(display->i2c_bus, &io_config, &touch_io), TAG,
                        "create GT911 I2C IO failed");

    const esp_lcd_touch_io_gt911_config_t gt911_config = {
        .dev_addr = io_config.dev_addr,
    };
    const esp_lcd_touch_config_t touch_config = {
        .x_max = MICRO_BSP_LCD_WIDTH,
        .y_max = MICRO_BSP_LCD_HEIGHT,
        .rst_gpio_num = GPIO_NUM_NC,
        .int_gpio_num = MICRO_BSP_TOUCH_IRQ_GPIO,
        .levels = {.reset = 0, .interrupt = 0},
        .flags = {.swap_xy = false, .mirror_x = false, .mirror_y = false},
        .driver_data = (void *)&gt911_config,
    };
    return esp_lcd_touch_new_i2c_gt911(touch_io, &touch_config, &display->touch);
}

static esp_err_t micro_bsp_lvgl_init(micro_bsp_display_t *display)
{
    lvgl_port_cfg_t port_config = ESP_LVGL_PORT_INIT_CONFIG();
    /* The default 7 KB LVGL-task stack overflowed (vApplicationStackOverflowHook)
     * once LVGL used C-lib malloc (deeper call stacks) and rendered larger
     * scrolled/refresh areas. Give it a generous stack. */
    port_config.task_stack = 16384;
    ESP_RETURN_ON_ERROR(lvgl_port_init(&port_config), TAG, "initialize LVGL port failed");

    const lvgl_port_display_cfg_t display_config = {
        .panel_handle = display->panel,
        .buffer_size = MICRO_BSP_LCD_WIDTH * MICRO_BSP_LCD_HEIGHT,
        .double_buffer = false,
        .hres = MICRO_BSP_LCD_WIDTH,
        .vres = MICRO_BSP_LCD_HEIGHT,
        .monochrome = false,
        .color_format = LV_COLOR_FORMAT_RGB565,
        .rotation = {.swap_xy = false, .mirror_x = false, .mirror_y = false},
        .flags = {.buff_dma = false, .buff_spiram = false, .swap_bytes = false,
                  .full_refresh = false, .direct_mode = true},
    };
    const lvgl_port_display_rgb_cfg_t rgb_config = {
        .flags = {.bb_mode = true, .avoid_tearing = true},
    };
    display->lvgl_display = lvgl_port_add_disp_rgb(&display_config, &rgb_config);
    if (display->lvgl_display == NULL) {
        return ESP_ERR_NO_MEM;
    }

    const lvgl_port_touch_cfg_t touch_config = {
        .disp = display->lvgl_display,
        .handle = display->touch,
    };
    display->lvgl_touch = lvgl_port_add_touch(&touch_config);
    return display->lvgl_touch == NULL ? ESP_ERR_NO_MEM : ESP_OK;
}

esp_err_t micro_bsp_init(micro_bsp_display_t *display)
{
    if (display == NULL) {
        return ESP_ERR_INVALID_ARG;
    }
    memset(display, 0, sizeof(*display));
    ESP_RETURN_ON_ERROR(micro_bsp_profile_prepare(), TAG, "profile recovery failed");

    esp_err_t result = micro_bsp_i2c_init(display);
    if (result == ESP_OK) {
        result = micro_bsp_rgb_init(display);
    }
    if (result == ESP_OK) {
        result = micro_bsp_touch_init(display);
    }
    if (result == ESP_OK) {
        result = micro_bsp_lvgl_init(display);
    }
    if (result != ESP_OK) {
        micro_bsp_profile_rollback();
    }
    return result;
}

esp_err_t micro_bsp_backlight_set(const micro_bsp_display_t *display, bool enabled)
{
    if (display == NULL || s_ch422g.mode == NULL || s_ch422g.output == NULL) {
        return ESP_ERR_INVALID_STATE;
    }
    ESP_RETURN_ON_ERROR(micro_bsp_ch422g_set_output_mode(), TAG, "CH422G mode failed");
    return micro_bsp_ch422g_write(s_ch422g.output,
                                  enabled ? MICRO_BSP_CH422G_BACKLIGHT_ON
                                          : MICRO_BSP_CH422G_BACKLIGHT_OFF);
}

esp_err_t micro_bsp_draw_smoke_screen(const micro_bsp_display_t *display)
{
    if (display == NULL || display->lvgl_display == NULL) {
        return ESP_ERR_INVALID_STATE;
    }
    lv_obj_t *screen = lv_screen_active();
    lv_obj_clean(screen);
    lv_obj_set_style_bg_color(screen, lv_color_hex(0x101820), 0);
    lv_obj_set_style_bg_opa(screen, LV_OPA_COVER, 0);

    static const lv_align_t alignments[] = {
        LV_ALIGN_TOP_LEFT, LV_ALIGN_TOP_RIGHT, LV_ALIGN_BOTTOM_LEFT, LV_ALIGN_BOTTOM_RIGHT,
    };
    static const char *labels[] = {"TL", "TR", "BL", "BR"};
    for (size_t index = 0; index < sizeof(alignments) / sizeof(alignments[0]); ++index) {
        lv_obj_t *target = lv_button_create(screen);
        lv_obj_set_size(target, 96, 64);
        lv_obj_align(target, alignments[index], 12, 12);
        lv_obj_t *label = lv_label_create(target);
        lv_label_set_text(label, labels[index]);
        lv_obj_center(label);
    }

    lv_obj_t *title = lv_label_create(screen);
    lv_label_set_text(title, "Micro OS LCD7 health screen");
    lv_obj_align(title, LV_ALIGN_TOP_MID, 0, 20);
    s_touch_label = lv_label_create(screen);
    lv_label_set_text(s_touch_label, "Touch: waiting for input");
    lv_obj_align(s_touch_label, LV_ALIGN_CENTER, 0, 0);
    s_smoke_touch = display->touch;
    (void)lv_timer_create(micro_bsp_update_touch_coordinates, 100, NULL);
    return ESP_OK;
}

esp_err_t micro_bsp_mark_healthy(void)
{
    if (!s_pending_profile) {
        return ESP_OK;
    }

    nvs_handle_t handle;
    ESP_RETURN_ON_ERROR(nvs_open_from_partition(MICRO_BSP_PROFILE_PARTITION,
                                                MICRO_BSP_PROFILE_NAMESPACE,
                                                NVS_READWRITE, &handle), TAG,
                        "open profile namespace failed");
    micro_bsp_profile_record_t pending = {0};
    size_t size = sizeof(pending);
    esp_err_t result = nvs_get_blob(handle, MICRO_BSP_PENDING_PROFILE_KEY, &pending, &size);
    if (result == ESP_OK && size == sizeof(pending) && micro_bsp_profile_is_valid(&pending)) {
        result = nvs_set_blob(handle, MICRO_BSP_ACTIVE_PROFILE_KEY, &pending, sizeof(pending));
        if (result == ESP_OK) {
            result = nvs_erase_key(handle, MICRO_BSP_PENDING_PROFILE_KEY);
        }
        if (result == ESP_OK) {
            result = nvs_commit(handle);
        }
    } else {
        result = ESP_ERR_INVALID_STATE;
    }
    nvs_close(handle);
    if (result == ESP_OK) {
        s_pending_profile = false;
    }
    return result;
}
