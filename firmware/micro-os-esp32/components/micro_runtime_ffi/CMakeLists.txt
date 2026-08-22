# Adapted from the official esp-rs/esp-idf-template CMake component pattern:
# https://github.com/esp-rs/esp-idf-template/tree/08115a069d167a5ee37363e84f168a565f17bbca/cmake/components
set(RUST_DEPS "pthread" "newlib" "lvgl__lvgl" "espressif__esp_lvgl_port")

# pinyin_table.h is generated from assets/fonts/ui-sans-common.txt and consumed
# by the on-screen IME in placeholder.c.
set(MICRO_FONT_ASSET_DIR "${CMAKE_CURRENT_LIST_DIR}/../../../../assets/fonts")

idf_component_register(
  SRCS "placeholder.c"
  INCLUDE_DIRS "include" "${MICRO_FONT_ASSET_DIR}"
  PRIV_REQUIRES "${RUST_DEPS}"
)

if(NOT CONFIG_IDF_TARGET_ESP32S3)
  message(FATAL_ERROR "micro-host-esp32 supports only ESP32-S3 in this release")
endif()

if(CMAKE_BUILD_TYPE STREQUAL "Debug")
  set(CARGO_BUILD_TYPE "debug")
  set(CARGO_BUILD_ARG "")
else()
  set(CARGO_BUILD_TYPE "release")
  set(CARGO_BUILD_ARG "--release")
endif()

set(RUST_TARGET "xtensa-esp32s3-espidf")
set(CARGO_BUILD_STD_ARG -Zbuild-std=std,panic_abort)
set(ESP_RUSTFLAGS "--cfg espidf_time64")
get_filename_component(REPOSITORY_ROOT "${CMAKE_CURRENT_LIST_DIR}/../../../.." ABSOLUTE)
set(CARGO_PROJECT_DIR "${REPOSITORY_ROOT}/crates/micro-host-esp32")
set(CARGO_BUILD_DIR "${CMAKE_CURRENT_BINARY_DIR}")
set(CARGO_TARGET_DIR "${CARGO_BUILD_DIR}/target")
set(LOCAL_CARGO_HOME "${REPOSITORY_ROOT}/work/toolchains/cargo")
set(LOCAL_RUSTUP_HOME "${REPOSITORY_ROOT}/work/toolchains/rustup")
set(RUST_STATIC_LIBRARY
    "${CARGO_TARGET_DIR}/${RUST_TARGET}/${CARGO_BUILD_TYPE}/libmicro_host_esp32.a")

idf_build_get_property(sdkconfig SDKCONFIG)
idf_build_get_property(idf_path IDF_PATH)

ExternalProject_Add(
  project_micro_host_esp32
  PREFIX "${CARGO_BUILD_DIR}/cargo-project"
  DOWNLOAD_COMMAND ""
  CONFIGURE_COMMAND ${CMAKE_COMMAND} -E env
    CARGO_HOME=${LOCAL_CARGO_HOME}
    RUSTUP_HOME=${LOCAL_RUSTUP_HOME}
    cargo clean --package micro-host-esp32 --target ${RUST_TARGET}
      --target-dir ${CARGO_TARGET_DIR}
  USES_TERMINAL_BUILD true
  BUILD_COMMAND ${CMAKE_COMMAND} -E env
    CARGO_HOME=${LOCAL_CARGO_HOME}
    RUSTUP_HOME=${LOCAL_RUSTUP_HOME}
    CARGO_CMAKE_BUILD_INCLUDES=$<TARGET_PROPERTY:${COMPONENT_LIB},INCLUDE_DIRECTORIES>
    CARGO_CMAKE_BUILD_LINK_LIBRARIES=$<TARGET_PROPERTY:${COMPONENT_LIB},LINK_LIBRARIES>
    CARGO_CMAKE_BUILD_SDKCONFIG=${sdkconfig}
    CARGO_CMAKE_BUILD_ESP_IDF=${idf_path}
    CARGO_CMAKE_BUILD_COMPILER=${CMAKE_C_COMPILER}
    RUSTFLAGS=${ESP_RUSTFLAGS}
    MCU=${CONFIG_IDF_TARGET}
    cargo build --manifest-path ${CARGO_PROJECT_DIR}/Cargo.toml
      --package micro-host-esp32 --target ${RUST_TARGET} --locked
      --target-dir ${CARGO_TARGET_DIR} ${CARGO_BUILD_ARG} ${CARGO_BUILD_STD_ARG}
  INSTALL_COMMAND ""
  BUILD_ALWAYS TRUE
  TMP_DIR "${CARGO_BUILD_DIR}/tmp"
  STAMP_DIR "${CARGO_BUILD_DIR}/stamp"
  DOWNLOAD_DIR "${CARGO_BUILD_DIR}"
  SOURCE_DIR "${CARGO_PROJECT_DIR}"
  BINARY_DIR "${CARGO_PROJECT_DIR}"
  INSTALL_DIR "${CARGO_BUILD_DIR}"
  BUILD_BYPRODUCTS "${RUST_STATIC_LIBRARY}"
)

add_prebuilt_library(micro_host_esp32_rust "${RUST_STATIC_LIBRARY}"
                     PRIV_REQUIRES "${RUST_DEPS}")
add_dependencies(micro_host_esp32_rust project_micro_host_esp32)
target_link_libraries(${COMPONENT_LIB} PRIVATE micro_host_esp32_rust)
target_link_options(${COMPONENT_LIB} INTERFACE
                    "LINKER:--undefined=micro_runtime_ffi_keepalive")
