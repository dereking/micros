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
/* Editable single-line text field. `text` is the current value, `placeholder`
 * the hint shown when empty, and `handler_id` the onChange handler (or
 * MICRO_NO_HANDLER to disable editing). */
MICRO_EXPORT int micro_native_create_input(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                                           const char *text, const char *placeholder, uint32_t handler_id,
                                           uintptr_t font_handle, uint32_t line_height_px);
MICRO_EXPORT int micro_native_set_input_text(micro_native_t *native, uint32_t node_id, const char *text);
MICRO_EXPORT int micro_native_take_input_change(micro_native_t *native, uint32_t *handler_id, char *text,
                                                size_t text_capacity, size_t *text_len);
/* Draggable numeric slider. `value` is the initial position within [min,max];
 * `handler_id` is the onChange handler (or MICRO_NO_HANDLER to disable). */
MICRO_EXPORT int micro_native_create_slider(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                                            double value, double min, double max, uint32_t handler_id);
MICRO_EXPORT int micro_native_set_slider_value(micro_native_t *native, uint32_t node_id, double value);
MICRO_EXPORT int micro_native_take_slider_change(micro_native_t *native, uint32_t *handler_id, double *value);
/* Checkbox with a text label. `checked` is the initial state; `handler_id`
 * is the onChange handler (or MICRO_NO_HANDLER to disable). */
MICRO_EXPORT int micro_native_create_checkbox(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                                              const char *label, int checked, uint32_t handler_id);
MICRO_EXPORT int micro_native_take_checkbox_change(micro_native_t *native, uint32_t *handler_id, int *checked);
/* Dropdown selection list. `options` is the choice strings joined with '\n';
 * `index` is the initially selected option; `handler_id` is the onChange
 * handler (or MICRO_NO_HANDLER to disable). */
MICRO_EXPORT int micro_native_create_dropdown(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                                              const char *options, double index, uint32_t handler_id);
MICRO_EXPORT int micro_native_set_dropdown_value(micro_native_t *native, uint32_t node_id, double index);
MICRO_EXPORT int micro_native_take_dropdown_change(micro_native_t *native, uint32_t *handler_id, double *index);

#endif
