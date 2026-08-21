idf_component_register(
  SRCS "micro_bsp_lcd7.c"
  INCLUDE_DIRS "include"
  REQUIRES esp_lcd esp_driver_i2c esp_driver_gpio nvs_flash
           espressif__esp_lcd_touch espressif__esp_lcd_touch_gt911
           espressif__esp_lvgl_port
)
