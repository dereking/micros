#pragma once

#include <stdbool.h>
#include <stdint.h>

#include "driver/i2c_master.h"
#include "esp_err.h"
#include "esp_lcd_panel_ops.h"
#include "esp_lcd_touch.h"
#include "esp_lvgl_port.h"
#include "lvgl.h"

#ifdef __cplusplus
extern "C" {
#endif

#define MICRO_BSP_LCD_WIDTH 800
#define MICRO_BSP_LCD_HEIGHT 480
#define MICRO_BSP_LCD_PCLK_HZ 16000000

#define MICRO_BSP_LCD_HSYNC_GPIO 46
#define MICRO_BSP_LCD_VSYNC_GPIO 3
#define MICRO_BSP_LCD_DE_GPIO 5
#define MICRO_BSP_LCD_PCLK_GPIO 7

#define MICRO_BSP_I2C_PORT 0
#define MICRO_BSP_I2C_SDA_GPIO 8
#define MICRO_BSP_I2C_SCL_GPIO 9
#define MICRO_BSP_TOUCH_IRQ_GPIO 4

#define MICRO_BSP_CH422G_TOUCH_RESET_EXIO 1
#define MICRO_BSP_CH422G_BACKLIGHT_EXIO 2
#define MICRO_BSP_RGB_BOUNCE_BUFFER_LINES 32

_Static_assert(MICRO_BSP_LCD_WIDTH == 800, "Spotpear panel width must remain 800");
_Static_assert(MICRO_BSP_LCD_HEIGHT == 480, "Spotpear panel height must remain 480");
_Static_assert(MICRO_BSP_CH422G_BACKLIGHT_EXIO == 2,
               "V1.2 backlight is CH422G EXIO2");

typedef struct {
    esp_lcd_panel_handle_t panel;
    esp_lcd_touch_handle_t touch;
    i2c_master_bus_handle_t i2c_bus;
    lv_display_t *lvgl_display;
    lv_indev_t *lvgl_touch;
} micro_bsp_display_t;

/** Initialize the fixed Spotpear V1.2 N8R8 display, touch, and LVGL port. */
esp_err_t micro_bsp_init(micro_bsp_display_t *display);

/** Switch the binary CH422G-controlled backlight. */
esp_err_t micro_bsp_backlight_set(const micro_bsp_display_t *display, bool enabled);

/** Draw the trusted four-corner smoke screen while the LVGL port is locked. */
esp_err_t micro_bsp_draw_smoke_screen(const micro_bsp_display_t *display);

/** Promote a validated pending profile only after the health screen is visible. */
esp_err_t micro_bsp_mark_healthy(void);

#ifdef __cplusplus
}
#endif
