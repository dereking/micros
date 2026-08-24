idf_component_register(
  SRCS "main.c"
  INCLUDE_DIRS "."
  REQUIRES esp_psram esp_system spi_flash
            micro_runtime_ffi micro_bsp_lcd7 micro_wifi
)

# Stage the shell + app MBCs into the build tree as micro_app.bin. The firmware
# scans consecutive MBCs in the micro_app raw partition (see main.c), treating
# index 0 as the OS shell and the rest as installable apps — so the shell MBC
# must come first. Re-running `npm run build:app` refreshes the staged copy.
set(MBC_SHELL_PATH "${CMAKE_SOURCE_DIR}/../../apps/shell/dist/shell.mbc")
set(MBC_APP_PATH "${CMAKE_SOURCE_DIR}/../../apps/counter/dist/counter.mbc")
set(MBC_SETTINGS_PATH "${CMAKE_SOURCE_DIR}/../../apps/settings/dist/settings.mbc")
set(MBC_STAGED_PATH "${CMAKE_CURRENT_BINARY_DIR}/micro_app.bin")

add_custom_command(
  OUTPUT "${MBC_STAGED_PATH}"
  COMMAND sh -c "cat '${MBC_SHELL_PATH}' '${MBC_APP_PATH}' '${MBC_SETTINGS_PATH}' > '${MBC_STAGED_PATH}'"
  DEPENDS "${MBC_SHELL_PATH}" "${MBC_APP_PATH}" "${MBC_SETTINGS_PATH}"
  COMMENT "Staging shell + counter + settings MBCs as micro_app partition image"
  VERBATIM
)
add_custom_target(stage_counter_mbc ALL DEPENDS "${MBC_STAGED_PATH}")
