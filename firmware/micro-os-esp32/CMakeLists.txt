cmake_minimum_required(VERSION 3.16)

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(micro_os_esp32)

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
