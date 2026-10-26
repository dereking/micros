idf_component_register(
  SRCS "micro_sd.c"
  INCLUDE_DIRS "include"
  REQUIRES micro_bsp_lcd7 esp_driver_spi esp_driver_sdspi sdmmc fatfs vfs
)
