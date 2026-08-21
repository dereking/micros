set(MICRO_UI_FONT_DIR "${CMAKE_CURRENT_LIST_DIR}/../../../../assets/fonts/lvgl")

idf_component_register(
  SRCS "micro_bsp_lcd7.c"
       "${MICRO_UI_FONT_DIR}/micro_ui_sans_12.c"
       "${MICRO_UI_FONT_DIR}/micro_ui_sans_14.c"
       "${MICRO_UI_FONT_DIR}/micro_ui_sans_18.c"
       "${MICRO_UI_FONT_DIR}/micro_ui_sans_24.c"
       "${MICRO_UI_FONT_DIR}/micro_ui_sans_32.c"
  INCLUDE_DIRS "include"
  REQUIRES esp_lcd esp_driver_i2c esp_driver_gpio nvs_flash
           espressif__esp_lcd_touch espressif__esp_lcd_touch_gt911
           espressif__esp_lvgl_port
)
