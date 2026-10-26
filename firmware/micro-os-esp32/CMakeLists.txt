cmake_minimum_required(VERSION 3.16)

include($ENV{IDF_PATH}/tools/cmake/project.cmake)

# LVGL 9's ESP-IDF component hardcodes the builtin TLSF memory backend (a 64 KB
# pool) which the full Micro UI demo + draw-layer buffers exhaust, making the
# TLSF allocator crawl and the task watchdog fire during scroll/refresh. Force
# the C-lib backend so LVGL allocates via malloc()/PSRAM instead.
idf_build_set_property(COMPILE_DEFINITIONS "-DLV_USE_STDLIB_MALLOC=1" APPEND)

# lv_font_conv writes `#ifdef LV_LVGL_H_INCLUDE_SIMPLE / #include "lvgl.h" /
# #else / #include "lvgl/lvgl.h"` into every generated font. The ESP-IDF LVGL
# component is installed as managed_components/lvgl__lvgl, so the nested
# "lvgl/lvgl.h" path cannot resolve; the simple form is what LVGL puts on the
# include path (both the component root and src/ are added).
idf_build_set_property(COMPILE_DEFINITIONS "-DLV_LVGL_H_INCLUDE_SIMPLE=1" APPEND)

project(micro_os_esp32)

# The Rust host's native text-style table references the C font objects. After
# micro_sd made the BSP a transitive dependency, the BSP archive is scanned
# before the Rust archive, so those members are not pulled by archive order.
# Keep the symbols live explicitly; --undefined makes the linker pull the font
# members regardless of component order.
idf_build_get_property(project_elf EXECUTABLE)
target_link_options(${project_elf} PRIVATE
  "-Wl,--undefined=micro_ui_sans_12"
  "-Wl,--undefined=micro_ui_sans_32"
  # Rust's std and mbedTLS reference POSIX directory functions. newlib's
  # libnosys provides stubs for them, while newlib/realpath.c also defines
  # getcwd/chdir; with VFS directory support disabled the only remaining
  # duplicates are these harmless newlib/libnosys stubs. The first definition
  # (newlib) wins.
  "-Wl,--allow-multiple-definition")

# A successful final ELF link is the commit point for Rust ABI evidence. This
# must be attached after project(), because ESP-IDF creates and publishes the
# application ELF target only after processing all component CMake files.
idf_build_get_property(PROJECT_ELF EXECUTABLE)
if(CMAKE_BUILD_TYPE STREQUAL "Debug")
  set(RUST_ABI_CARGO_PROFILE "debug")
else()
  set(RUST_ABI_CARGO_PROFILE "release")
endif()
set(RUST_ABI_HASH_SCRIPT
    "${CMAKE_CURRENT_LIST_DIR}/../../scripts/esp32-rust-input-hash.sh")
set(RUST_ABI_ARCHIVE
    "${CMAKE_BINARY_DIR}/esp-idf/micro_runtime_ffi/target/xtensa-esp32s3-espidf/${RUST_ABI_CARGO_PROFILE}/libmicro_host_esp32.a")
set(RUST_ABI_MAP "${CMAKE_BINARY_DIR}/${CMAKE_PROJECT_NAME}.map")
set(RUST_ABI_COMPLETION_STAMP
    "${CMAKE_BINARY_DIR}/esp-idf/micro_runtime_ffi/rust-abi-complete.sha256")
add_custom_command(
  TARGET ${PROJECT_ELF}
  POST_BUILD
  COMMAND zsh "${RUST_ABI_HASH_SCRIPT}" --write-completion
          "${RUST_ABI_COMPLETION_STAMP}" "${RUST_ABI_ARCHIVE}" "${RUST_ABI_MAP}"
  BYPRODUCTS "${RUST_ABI_COMPLETION_STAMP}"
  VERBATIM
)
