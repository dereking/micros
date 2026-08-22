idf_component_register(
  SRCS "main.c"
  INCLUDE_DIRS "."
  REQUIRES esp_psram esp_system spi_flash
            micro_runtime_ffi micro_bsp_lcd7
)

# Stage the Counter MBC into the build tree as micro_app.bin. The firmware
# reads the micro_app raw partition (see main.c), so this image is flashed
# alongside the app at the micro_app offset from partitions_8m.csv. Re-running
# `npm run build:app` (or editing apps/counter/app.ts) is enough to refresh
# the staged copy; no manual step.
set(MBC_SOURCE_PATH "${CMAKE_SOURCE_DIR}/../../apps/counter/dist/app.mbc")
set(MBC_STAGED_PATH "${CMAKE_CURRENT_BINARY_DIR}/micro_app.bin")

add_custom_command(
  OUTPUT "${MBC_STAGED_PATH}"
  COMMAND ${CMAKE_COMMAND} -E copy_if_different
          "${MBC_SOURCE_PATH}" "${MBC_STAGED_PATH}"
  DEPENDS "${MBC_SOURCE_PATH}"
  COMMENT "Staging Counter MBC as raw micro_app partition image"
  VERBATIM
)
add_custom_target(stage_counter_mbc ALL DEPENDS "${MBC_STAGED_PATH}")
