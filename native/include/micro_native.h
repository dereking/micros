#ifndef MICRO_NATIVE_H
#define MICRO_NATIVE_H

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#define MICRO_EXPORT __declspec(dllexport)
#else
#define MICRO_EXPORT __attribute__((visibility("default")))
#endif

typedef struct micro_native micro_native_t;

MICRO_EXPORT micro_native_t *micro_native_create(
    int width,
    int height,
    int hidden,
    char *error,
    size_t error_length);
MICRO_EXPORT void micro_native_destroy(micro_native_t *native);
MICRO_EXPORT int micro_native_destroy_app_root(micro_native_t *native);
MICRO_EXPORT int micro_native_poll(micro_native_t *native);
MICRO_EXPORT uint32_t micro_native_timer(micro_native_t *native);
MICRO_EXPORT int micro_native_take_activation(micro_native_t *native, uint32_t *handler_id);
MICRO_EXPORT void micro_native_inject_activation(micro_native_t *native, uint32_t handler_id);
MICRO_EXPORT int micro_native_queue_click(micro_native_t *native, uint32_t node_id);
MICRO_EXPORT int micro_native_create_column(micro_native_t *native, uint32_t node_id, uint32_t parent_id);
MICRO_EXPORT int micro_native_create_row(micro_native_t *native, uint32_t node_id, uint32_t parent_id);
MICRO_EXPORT int micro_native_create_progress(micro_native_t *native, uint32_t node_id, uint32_t parent_id, double fraction);
MICRO_EXPORT int micro_native_create_switch(micro_native_t *native, uint32_t node_id, uint32_t parent_id, int checked, uint32_t handler_id);
/* A zero font handle preserves LVGL defaults; nonzero handles are const lv_font_t pointers. */
MICRO_EXPORT int micro_native_create_label(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *text, uintptr_t font_handle, uint32_t line_height_px);
MICRO_EXPORT int micro_native_create_button(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *text, uint32_t handler_id, uintptr_t font_handle, uint32_t line_height_px);
MICRO_EXPORT int micro_native_set_label_text(micro_native_t *native, uint32_t node_id, const char *text);
MICRO_EXPORT int micro_native_set_progress_value(micro_native_t *native, uint32_t node_id, double fraction);
MICRO_EXPORT int micro_native_set_switch_checked(micro_native_t *native, uint32_t node_id, int checked);

#endif
