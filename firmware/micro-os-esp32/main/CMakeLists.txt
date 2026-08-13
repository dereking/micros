idf_component_register(
  SRCS "main.c"
  INCLUDE_DIRS "."
  REQUIRES esp_psram esp_system spi_flash micro_runtime_ffi
)
