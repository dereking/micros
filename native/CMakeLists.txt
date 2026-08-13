cmake_minimum_required(VERSION 3.24)
project(micro_native C)

include(FetchContent)

set(SDL_SHARED OFF CACHE BOOL "" FORCE)
set(SDL_STATIC ON CACHE BOOL "" FORCE)
set(SDL_TESTS OFF CACHE BOOL "" FORCE)
set(SDL_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SDL_DISABLE_INSTALL ON CACHE BOOL "" FORCE)
FetchContent_Declare(
  SDL3
  GIT_REPOSITORY https://github.com/libsdl-org/SDL.git
  GIT_TAG release-3.4.10
  GIT_SHALLOW TRUE
)

set(LV_BUILD_CONF_PATH ${CMAKE_CURRENT_SOURCE_DIR}/lv_conf.h CACHE PATH "" FORCE)
set(LV_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(LV_BUILD_DEMOS OFF CACHE BOOL "" FORCE)
FetchContent_Declare(
  lvgl
  GIT_REPOSITORY https://github.com/lvgl/lvgl.git
  GIT_TAG v9.5.0
  GIT_SHALLOW TRUE
)

FetchContent_MakeAvailable(SDL3 lvgl)

add_library(micro_native SHARED src/micro_native.c)
target_include_directories(micro_native PUBLIC include)
target_link_libraries(micro_native PRIVATE SDL3::SDL3-static lvgl)
set_target_properties(micro_native PROPERTIES
  C_VISIBILITY_PRESET hidden
  VISIBILITY_INLINES_HIDDEN YES
  INSTALL_RPATH "@loader_path"
)

install(TARGETS micro_native LIBRARY DESTINATION lib RUNTIME DESTINATION lib)
