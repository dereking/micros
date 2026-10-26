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
/* Returns 1 (and clears) when an edge-swipe-back gesture or the app bar's
 * close button was activated. */
MICRO_EXPORT int micro_native_take_back_gesture(micro_native_t *native);
/* The two back sources separately, so Home's close control can power off while
 * an edge swipe on Home stays inert (matching the Web/ESP32 hosts). */
MICRO_EXPORT int micro_native_take_appbar_close(micro_native_t *native);
MICRO_EXPORT int micro_native_take_swipe_back(micro_native_t *native);
/* Show (1) or hide (0) the default OS app bar and update its status. The shell
 * and every installed App pass their display name plus the current Wi-Fi
 * state/SSID; only the single-App debug viewport hides the bar.
 * Call before creating the Runtime so the layout measures the content inset. */
MICRO_EXPORT void micro_native_set_app_chrome(micro_native_t *native, int visible,
                                              const char *title,
                                              const char *wifi_state,
                                              const char *wifi_ssid);
/* Replace the centred app-bar title without touching visibility or Wi-Fi
 * (backs `os.setTitle`). No-op before the bar exists. */
MICRO_EXPORT void micro_native_set_appbar_title(micro_native_t *native,
                                                const char *title);
MICRO_EXPORT int micro_native_appbar_visible(micro_native_t *native);
/* Whether the App content container is on screen (power-off hides it). */
MICRO_EXPORT int micro_native_appbar_content_visible(micro_native_t *native);
MICRO_EXPORT uint32_t micro_native_timer(micro_native_t *native);

/* System power state. Powering off from the shell hides the app bar/content
 * and shows the "Powered off" screen; tapping it sets the power-on request. */
MICRO_EXPORT void micro_native_power_off(micro_native_t *native);
MICRO_EXPORT int micro_native_power_off_active(micro_native_t *native);
MICRO_EXPORT int micro_native_take_power_on(micro_native_t *native);
MICRO_EXPORT void micro_native_wake(micro_native_t *native);
/* Smoke-test hook: queue the off-screen tap without synthesising pointer
 * events. Host UI should call nothing here — the LVGL surface handles taps. */
MICRO_EXPORT void micro_native_press_power_on(micro_native_t *native);

/* Device viewport and presentation scale. LVGL renders the current logical
 * device screen; switching orientation swaps the width/height of the size
 * passed to micro_native_create and reflows the OS app bar and App layout in
 * place (the running runtime is preserved). The desktop host draws a top
 * icon toolbar above the canvas; toolbar points are outside the logical
 * canvas.
 * `percent` is the device scale (25..400, 100 = one design pixel per window
 * point); pass 0 to fit the current display. */
MICRO_EXPORT int micro_native_set_orientation(micro_native_t *native, int portrait);
MICRO_EXPORT int micro_native_get_orientation(micro_native_t *native);
MICRO_EXPORT int micro_native_set_rotation(micro_native_t *native, int degrees);
MICRO_EXPORT int micro_native_get_rotation(micro_native_t *native);
MICRO_EXPORT int micro_native_set_zoom(micro_native_t *native, int percent);
MICRO_EXPORT int micro_native_set_zoom_fit(micro_native_t *native);
MICRO_EXPORT int micro_native_get_zoom(micro_native_t *native);

/* Host-level device actions raised by the platform menu (macOS NSMenu today,
 * other native menus later) and drained by the Rust host loop. Keeping them
 * out of LVGL means power and restart never look like App input. */
enum {
    MICRO_HOST_ACTION_POWER_OFF = 0,
    MICRO_HOST_ACTION_POWER_ON = 1,
    MICRO_HOST_ACTION_FORCE_RESTART = 2,
    MICRO_HOST_ACTION_LANDSCAPE = 3,
    MICRO_HOST_ACTION_PORTRAIT = 4,
    MICRO_HOST_ACTION_ZOOM_FIT = 5,
    MICRO_HOST_ACTION_ZOOM_75 = 6,
    MICRO_HOST_ACTION_ZOOM_100 = 7,
    MICRO_HOST_ACTION_ZOOM_125 = 8,
    MICRO_HOST_ACTION_ZOOM_150 = 9
};

/* Snapshot used by the platform menu to enable/check items. */
typedef struct micro_native_host_state {
    int powered_off;
    int portrait;
    int zoom_fit;
    int zoom_percent;
} micro_native_host_state_t;

/* Queue a host action; safe to call from a native menu callback. */
MICRO_EXPORT void micro_native_request_host_action(int action);
/* Drain one queued host action. Returns 1 and writes `action` when available. */
MICRO_EXPORT int micro_native_take_host_action(micro_native_t *native, int *action);
/* Read the active visible host's power/orientation/zoom state. */
MICRO_EXPORT int micro_native_host_state(micro_native_host_state_t *state);
/* Open the platform device menu at its toolbar anchor. Returns 0 when the
 * platform has no native menu (headless tests, non-Apple hosts). */
MICRO_EXPORT int micro_native_open_host_menu(micro_native_t *native);
/* Coordinate conversion for tests, hit-testing and host-side overlays.
 * window_* are SDL window coordinates (points); logical_* are design-canvas
 * pixels. Both return 1 when the point is inside the canvas. */
MICRO_EXPORT int micro_native_window_to_logical(micro_native_t *native,
                                                double window_x, double window_y,
                                                double *logical_x, double *logical_y);
MICRO_EXPORT int micro_native_logical_to_window(micro_native_t *native,
                                                double logical_x, double logical_y,
                                                double *window_x, double *window_y);
/* Test/introspection hook: absolute LVGL coordinates of a mounted node. */
MICRO_EXPORT int micro_native_object_rect(micro_native_t *native, uint32_t node_id,
                                          int *x, int *y, int *width, int *height);

MICRO_EXPORT int micro_native_take_activation(micro_native_t *native, uint32_t *handler_id);
MICRO_EXPORT void micro_native_inject_activation(micro_native_t *native, uint32_t handler_id);
MICRO_EXPORT int micro_native_queue_click(micro_native_t *native, uint32_t node_id);
/* Test hook: queue an SDL mouse-wheel event at a logical canvas point.
 * Positive wheel_x scrolls right; positive wheel_y is a wheel-up / two-finger
 * swipe toward the user, matching SDL's convention. */
MICRO_EXPORT int micro_native_queue_wheel(micro_native_t *native,
                                          double logical_x, double logical_y,
                                          float wheel_x, float wheel_y);
/* Test hooks: focus a textarea and inject SDL text input as a user would. */
MICRO_EXPORT int micro_native_focus_textarea(micro_native_t *native, uint32_t node_id);
MICRO_EXPORT int micro_native_queue_text_input(micro_native_t *native, const char *text);
MICRO_EXPORT int micro_native_create_column(micro_native_t *native, uint32_t node_id, uint32_t parent_id);
MICRO_EXPORT int micro_native_create_row(micro_native_t *native, uint32_t node_id, uint32_t parent_id);
MICRO_EXPORT int micro_native_create_surface(micro_native_t *native, uint32_t node_id,
                                             uint32_t parent_id, const char *tone,
                                             uint32_t handler_id);
MICRO_EXPORT int micro_native_set_visible(micro_native_t *native, uint32_t node_id, int visible);
MICRO_EXPORT int micro_native_create_list(micro_native_t *native, uint32_t node_id, uint32_t parent_id);
/* Tabbed container; `titles` are '\n'-joined. */
MICRO_EXPORT int micro_native_create_tabview(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *titles);
MICRO_EXPORT int micro_native_create_tab_content(micro_native_t *native, uint32_t index);
MICRO_EXPORT int micro_native_create_progress(micro_native_t *native, uint32_t node_id, uint32_t parent_id, double fraction);
MICRO_EXPORT int micro_native_create_switch(micro_native_t *native, uint32_t node_id, uint32_t parent_id, int checked, uint32_t handler_id);
/* A zero font handle preserves LVGL defaults; nonzero handles are const lv_font_t pointers. */
MICRO_EXPORT int micro_native_create_label(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *text, uintptr_t font_handle, uint32_t line_height_px);
MICRO_EXPORT int micro_native_create_button(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *text, uint32_t handler_id, uintptr_t font_handle, uint32_t line_height_px, int selected);
MICRO_EXPORT int micro_native_set_label_text(micro_native_t *native, uint32_t node_id, const char *text);
/* Shared OS icon (`micro_ir::icons`): `glyph` is the LVGL symbol glyph already
 * resolved from the icon name, `size_px` selects the Montserrat glyph size,
 * `tone` is ink|muted|accent|inverse and `variant` is plain|badge. */
MICRO_EXPORT int micro_native_create_icon(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *glyph, uint32_t size_px, const char *tone, const char *variant, uint32_t handler_id);
MICRO_EXPORT int micro_native_set_icon(micro_native_t *native, uint32_t node_id, const char *glyph);
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
MICRO_EXPORT int micro_native_set_layout_spec(micro_native_t *native, uint32_t node_id, uint32_t mask,
                                              double left, double top, double width, double height,
                                              double anchor_left, double anchor_top,
                                              double anchor_right, double anchor_bottom);
MICRO_EXPORT int micro_native_apply_delphi_layout(micro_native_t *native, uint32_t container, const uint32_t *child_ids, uint32_t child_count);
MICRO_EXPORT int micro_native_create_led(micro_native_t *native, uint32_t node_id, uint32_t parent_id, int on);
MICRO_EXPORT int micro_native_set_led(micro_native_t *native, uint32_t node_id, int on);
MICRO_EXPORT int micro_native_create_spinner(micro_native_t *native, uint32_t node_id, uint32_t parent_id, int active);
MICRO_EXPORT int micro_native_set_spinner(micro_native_t *native, uint32_t node_id, int active);
MICRO_EXPORT int micro_native_create_scale(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                                           double value, double min, double max);
MICRO_EXPORT int micro_native_set_scale_value(micro_native_t *native, uint32_t node_id, double value);
MICRO_EXPORT int micro_native_create_roller(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                                            const char *options, double index, uint32_t handler_id);
MICRO_EXPORT int micro_native_set_selection_value(micro_native_t *native, uint32_t node_id, double index);
MICRO_EXPORT int micro_native_take_dropdown_change(micro_native_t *native, uint32_t *handler_id, double *index);
MICRO_EXPORT int micro_native_take_roller_change(micro_native_t *native, uint32_t *handler_id, double *index);

#endif
