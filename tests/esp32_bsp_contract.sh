#!/bin/zsh

set -euo pipefail

header="firmware/micro-os-esp32/components/micro_bsp_lcd7/include/micro_bsp_lcd7.h"
source="firmware/micro-os-esp32/components/micro_bsp_lcd7/micro_bsp_lcd7.c"

for contract_file in "$header" "$source"; do
  [[ -f "$contract_file" ]] || {
    print -u2 -- "missing BSP contract file: $contract_file"
    exit 1
  }
done

for expected in \
  '#define MICRO_BSP_LCD_WIDTH 800' \
  '#define MICRO_BSP_LCD_HEIGHT 480' \
  '#define MICRO_BSP_LCD_PCLK_HZ 16000000' \
  '#define MICRO_BSP_LCD_HSYNC_GPIO 46' \
  '#define MICRO_BSP_LCD_VSYNC_GPIO 3' \
  '#define MICRO_BSP_LCD_DE_GPIO 5' \
  '#define MICRO_BSP_LCD_PCLK_GPIO 7' \
  '#define MICRO_BSP_I2C_PORT 0' \
  '#define MICRO_BSP_I2C_SDA_GPIO 8' \
  '#define MICRO_BSP_I2C_SCL_GPIO 9' \
  '#define MICRO_BSP_TOUCH_IRQ_GPIO 4' \
  '#define MICRO_BSP_CH422G_TOUCH_RESET_EXIO 1' \
  '#define MICRO_BSP_CH422G_BACKLIGHT_EXIO 2' \
  '#define MICRO_BSP_RGB_BOUNCE_BUFFER_LINES 10' \
  '_Static_assert(MICRO_BSP_LCD_WIDTH == 800' \
  '_Static_assert(MICRO_BSP_CH422G_BACKLIGHT_EXIO == 2'; do
  /usr/bin/grep -Fq -- "$expected" "$header" || {
    print -u2 -- "missing BSP header contract: $expected"
    exit 1
  }
done

expected_data='14, 38, 18, 17, 10, 39, 0, 45, 48, 47, 21, 1, 2, 42, 41, 40'
actual_data=$(sed -n '/MICRO_BSP_LCD_DATA_GPIOS/,/};/p' "$source" | tr '\n' ' ' | tr -s ' ')
[[ "$actual_data" == *"$expected_data"* ]] || {
  print -u2 -- "RGB data GPIO order differs from the checked board profile"
  exit 1
}

for expected in \
  'i2c_new_master_bus' \
  'esp_lcd_touch_new_i2c_gt911' \
  'lvgl_port_add_disp_rgb' \
  'lvgl_port_add_touch' \
  'esp_lcd_touch_read_data' \
  'esp_lcd_touch_get_data' \
  'lv_timer_create' \
  'lv_obj_is_valid' \
  'micro_bsp_backlight_set'; do
  /usr/bin/grep -Fq -- "$expected" "$source" || {
    print -u2 -- "missing BSP implementation element: $expected"
    exit 1
  }
done

if /usr/bin/grep -Fq 'ESP_ERROR_CHECK' "$source"; then
  print -u2 -- "BSP must return esp_err_t instead of calling ESP_ERROR_CHECK"
  exit 1
fi

print -- "ESP32 LCD7 BSP contract passed"
