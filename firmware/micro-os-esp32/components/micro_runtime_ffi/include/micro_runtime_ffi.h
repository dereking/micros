#ifndef MICRO_RUNTIME_FFI_H
#define MICRO_RUNTIME_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct micro_runtime micro_runtime_t;
typedef struct micro_os micro_os_t;

/*
 * Opaque handles are created only by their matching create function. They must
 * be passed back with their original provenance and alignment, never cast
 * between runtime and OS types, and destroyed exactly once. No call may race
 * with destruction or alias the same handle through another mutable pointer.
 * During micro_os_dispatch, event, action-buffer metadata, action storage, and
 * the OS handle must refer to distinct live objects and must not be mutated by
 * another thread until the call returns. Every (pointer, length/capacity) pair
 * must describe a live, correctly aligned, addressable region for the whole
 * call: mbc[0..len] is readable; actions[0..capacity] and error[0..error_len]
 * are writable. A NULL error pointer or NULL actions pointer is allowed only
 * when its paired length/capacity is zero; MBC requires non-NULL and len > 0.
 * The error region must not overlap any handle, MBC, event, action-buffer
 * metadata, or action storage. Event, action-buffer metadata, and action
 * storage must also be mutually disjoint. Returned handles remain live until
 * their matching destroy call; no borrowed input or output pointer is retained.
 */

typedef enum micro_error {
    MICRO_OK = 0,
    MICRO_ERR_MBC = 1,
    MICRO_ERR_RUNTIME = 2,
    MICRO_ERR_UI = 3,
    MICRO_ERR_INVALID_ARGUMENT = 4,
    MICRO_ERR_PANIC = 5,
    MICRO_ERR_STOPPED = 6,
    MICRO_ERR_BUFFER_TOO_SMALL = 7,
} micro_error_t;

typedef enum micro_state {
    MICRO_STATE_EARLY_BOOT = 0,
    MICRO_STATE_SAFE_MODE = 1,
    MICRO_STATE_STORAGE_READY = 2,
    MICRO_STATE_BOARD_PROFILE_VALIDATED = 3,
    MICRO_STATE_DISPLAY_READY = 4,
    MICRO_STATE_SYSTEM_UI_READY = 5,
    MICRO_STATE_FIRST_RUN_SETUP = 6,
    MICRO_STATE_LAUNCHER = 7,
    MICRO_STATE_APP_STARTING = 8,
    MICRO_STATE_APP_RUNNING = 9,
    MICRO_STATE_APP_STOPPING = 10,
    MICRO_STATE_APP_ERROR = 11,
    MICRO_STATE_SETTINGS = 12,
} micro_state_t;

typedef enum micro_result {
    MICRO_RESULT_UNUSED = 0,
    MICRO_RESULT_OK = 1,
    MICRO_RESULT_ERR = 2,
} micro_result_t;

typedef enum micro_failure_reason {
    MICRO_FAILURE_UNUSED = 0,
    MICRO_FAILURE_SAFE_MODE_REQUESTED = 1,
    MICRO_FAILURE_STORAGE_CORRUPT = 2,
    MICRO_FAILURE_INVALID_BOARD_PROFILE = 3,
    MICRO_FAILURE_HARDWARE_UNAVAILABLE = 4,
    MICRO_FAILURE_APP_CRASHED = 5,
    MICRO_FAILURE_INTERNAL = 6,
} micro_failure_reason_t;

typedef enum micro_wifi_failure {
    MICRO_WIFI_FAILURE_UNUSED = 0,
    MICRO_WIFI_FAILURE_AUTHENTICATION = 1,
    MICRO_WIFI_FAILURE_NETWORK_MISSING = 2,
    MICRO_WIFI_FAILURE_TIMEOUT = 3,
    MICRO_WIFI_FAILURE_INTERNAL = 4,
} micro_wifi_failure_t;

typedef enum micro_app_id {
    MICRO_APP_UNUSED = 0,
    MICRO_APP_COUNTER = 1,
} micro_app_id_t;

typedef enum micro_backlight {
    MICRO_BACKLIGHT_UNUSED = 0,
    MICRO_BACKLIGHT_OFF = 1,
    MICRO_BACKLIGHT_LOW = 2,
    MICRO_BACKLIGHT_MEDIUM = 3,
    MICRO_BACKLIGHT_HIGH = 4,
} micro_backlight_t;

typedef enum micro_event_kind {
    MICRO_EVENT_BOOT_SAMPLED = 0,
    MICRO_EVENT_STORAGE_INITIALIZED = 1,
    MICRO_EVENT_PROFILE_VALIDATED = 2,
    MICRO_EVENT_DISPLAY_INITIALIZED = 3,
    MICRO_EVENT_SYSTEM_UI_INITIALIZED = 4,
    MICRO_EVENT_NETWORK_CONFIG_LOADED = 5,
    MICRO_EVENT_SETUP_SKIPPED = 6,
    MICRO_EVENT_OPEN_SETTINGS = 7,
    MICRO_EVENT_BACK_PRESSED = 8,
    MICRO_EVENT_HOME_PRESSED = 9,
    MICRO_EVENT_OPEN_APP = 10,
    MICRO_EVENT_APP_STARTED = 11,
    MICRO_EVENT_APP_FAILED = 12,
    MICRO_EVENT_RESTART_APP = 13,
    MICRO_EVENT_APP_STOPPED = 14,
    MICRO_EVENT_WIFI_SCAN_REQUESTED = 15,
    MICRO_EVENT_WIFI_SCAN_COMPLETED = 16,
    MICRO_EVENT_WIFI_SCAN_FAILED = 17,
    MICRO_EVENT_WIFI_CONNECT_REQUESTED = 18,
    MICRO_EVENT_WIFI_CONNECTED = 19,
    MICRO_EVENT_WIFI_PERSISTED = 20,
    MICRO_EVENT_WIFI_FAILED = 21,
    MICRO_EVENT_RECONNECT_DUE = 22,
    MICRO_EVENT_RECONNECT_NOW_REQUESTED = 23,
    MICRO_EVENT_CLEAR_NETWORK_REQUESTED = 24,
    MICRO_EVENT_CLEAR_NETWORK_CONFIRMED = 25,
    MICRO_EVENT_CLEAR_NETWORK_COMPLETED = 26,
    MICRO_EVENT_FACTORY_RESET_REQUESTED = 27,
    MICRO_EVENT_FACTORY_RESET_CONFIRMED = 28,
    MICRO_EVENT_FACTORY_RESET_COMPLETED = 29,
    MICRO_EVENT_REBOOT_REQUESTED = 30,
    MICRO_EVENT_SET_BACKLIGHT = 31,
} micro_event_kind_t;

typedef struct micro_event {
    micro_event_kind_t kind;
    micro_result_t result;
    micro_failure_reason_t failure;
    micro_wifi_failure_t wifi_failure;
    micro_app_id_t app;
    uint32_t flag;
    uint32_t after_secs;
    uint32_t reserved;
    uint64_t session_id;
    uint64_t operation_id;
    uint64_t confirmation_id;
} micro_event_t;

typedef enum micro_action_kind {
    MICRO_ACTION_NONE = 0,
    MICRO_ACTION_REJECTED = 1,
    MICRO_ACTION_ACTIONS = 2,
    MICRO_ACTION_ENTER_SAFE_MODE = 3,
    MICRO_ACTION_INITIALIZE_STORAGE = 4,
    MICRO_ACTION_VALIDATE_PROFILE = 5,
    MICRO_ACTION_INITIALIZE_DISPLAY = 6,
    MICRO_ACTION_INITIALIZE_SYSTEM_UI = 7,
    MICRO_ACTION_LOAD_NETWORK_CONFIG = 8,
    MICRO_ACTION_SHOW_FIRST_RUN_SETUP = 9,
    MICRO_ACTION_SHOW_LAUNCHER = 10,
    MICRO_ACTION_SHOW_SETTINGS = 11,
    MICRO_ACTION_START_WIFI_SCAN = 12,
    MICRO_ACTION_CONNECT_WIFI = 13,
    MICRO_ACTION_CONNECT_SAVED_WIFI = 14,
    MICRO_ACTION_PERSIST_WIFI = 15,
    MICRO_ACTION_CLEAR_PENDING_WIFI = 16,
    MICRO_ACTION_SCHEDULE_WIFI_RECONNECT = 17,
    MICRO_ACTION_START_APP = 18,
    MICRO_ACTION_STOP_APP = 19,
    MICRO_ACTION_SHOW_APP_ERROR = 20,
    MICRO_ACTION_CONFIRM_CLEAR_NETWORK = 21,
    MICRO_ACTION_CLEAR_NETWORK = 22,
    MICRO_ACTION_CONFIRM_FACTORY_RESET = 23,
    MICRO_ACTION_FACTORY_RESET = 24,
    MICRO_ACTION_REBOOT = 25,
    MICRO_ACTION_APPLY_BACKLIGHT = 26,
} micro_action_kind_t;

typedef struct micro_action {
    micro_action_kind_t kind;
    uint32_t child_count;
    micro_failure_reason_t failure;
    micro_app_id_t app;
    uint32_t after_secs;
    micro_backlight_t backlight;
    uint32_t reserved_1;
    uint32_t reserved_2;
    uint64_t session_id;
    uint64_t operation_id;
    uint64_t confirmation_id;
} micro_action_t;

typedef struct micro_action_buffer {
    micro_action_t *actions;
    size_t capacity;
    size_t count;
    size_t required;
} micro_action_buffer_t;

_Static_assert(sizeof(micro_error_t) == 4, "micro_error_t must be 32-bit");
_Static_assert(sizeof(micro_state_t) == 4, "micro_state_t must be 32-bit");
_Static_assert(sizeof(micro_result_t) == 4, "micro_result_t must be 32-bit");
_Static_assert(sizeof(micro_failure_reason_t) == 4, "micro_failure_reason_t must be 32-bit");
_Static_assert(sizeof(micro_wifi_failure_t) == 4, "micro_wifi_failure_t must be 32-bit");
_Static_assert(sizeof(micro_app_id_t) == 4, "micro_app_id_t must be 32-bit");
_Static_assert(sizeof(micro_backlight_t) == 4, "micro_backlight_t must be 32-bit");
_Static_assert(sizeof(micro_event_kind_t) == 4, "micro_event_kind_t must be 32-bit");
_Static_assert(sizeof(micro_action_kind_t) == 4, "micro_action_kind_t must be 32-bit");
_Static_assert(sizeof(micro_event_t) == 56, "micro_event_t layout drifted");
_Static_assert(sizeof(micro_action_t) == 56, "micro_action_t layout drifted");
_Static_assert(offsetof(micro_event_t, session_id) == 32, "micro_event_t ID alignment drifted");
_Static_assert(offsetof(micro_action_t, session_id) == 32, "micro_action_t ID alignment drifted");
_Static_assert(MICRO_OK == 0 && MICRO_ERR_MBC == 1 && MICRO_ERR_RUNTIME == 2 &&
               MICRO_ERR_UI == 3 && MICRO_ERR_INVALID_ARGUMENT == 4 &&
               MICRO_ERR_PANIC == 5 && MICRO_ERR_STOPPED == 6 &&
               MICRO_ERR_BUFFER_TOO_SMALL == 7, "micro_error_t discriminants drifted");
_Static_assert(MICRO_STATE_EARLY_BOOT == 0 && MICRO_STATE_SAFE_MODE == 1 &&
               MICRO_STATE_STORAGE_READY == 2 && MICRO_STATE_BOARD_PROFILE_VALIDATED == 3 &&
               MICRO_STATE_DISPLAY_READY == 4 && MICRO_STATE_SYSTEM_UI_READY == 5 &&
               MICRO_STATE_FIRST_RUN_SETUP == 6 && MICRO_STATE_LAUNCHER == 7 &&
               MICRO_STATE_APP_STARTING == 8 && MICRO_STATE_APP_RUNNING == 9 &&
               MICRO_STATE_APP_STOPPING == 10 && MICRO_STATE_APP_ERROR == 11 &&
               MICRO_STATE_SETTINGS == 12, "micro_state_t discriminants drifted");
_Static_assert(MICRO_RESULT_UNUSED == 0 && MICRO_RESULT_OK == 1 && MICRO_RESULT_ERR == 2,
               "micro_result_t discriminants drifted");
_Static_assert(MICRO_FAILURE_UNUSED == 0 && MICRO_FAILURE_SAFE_MODE_REQUESTED == 1 &&
               MICRO_FAILURE_STORAGE_CORRUPT == 2 && MICRO_FAILURE_INVALID_BOARD_PROFILE == 3 &&
               MICRO_FAILURE_HARDWARE_UNAVAILABLE == 4 && MICRO_FAILURE_APP_CRASHED == 5 &&
               MICRO_FAILURE_INTERNAL == 6, "micro_failure_reason_t discriminants drifted");
_Static_assert(MICRO_WIFI_FAILURE_UNUSED == 0 && MICRO_WIFI_FAILURE_AUTHENTICATION == 1 &&
               MICRO_WIFI_FAILURE_NETWORK_MISSING == 2 && MICRO_WIFI_FAILURE_TIMEOUT == 3 &&
               MICRO_WIFI_FAILURE_INTERNAL == 4, "micro_wifi_failure_t discriminants drifted");
_Static_assert(MICRO_APP_UNUSED == 0 && MICRO_APP_COUNTER == 1,
               "micro_app_id_t discriminants drifted");
_Static_assert(MICRO_BACKLIGHT_UNUSED == 0 && MICRO_BACKLIGHT_OFF == 1 &&
               MICRO_BACKLIGHT_LOW == 2 && MICRO_BACKLIGHT_MEDIUM == 3 &&
               MICRO_BACKLIGHT_HIGH == 4, "micro_backlight_t discriminants drifted");
_Static_assert(MICRO_EVENT_BOOT_SAMPLED == 0 && MICRO_EVENT_STORAGE_INITIALIZED == 1 &&
               MICRO_EVENT_PROFILE_VALIDATED == 2 && MICRO_EVENT_DISPLAY_INITIALIZED == 3 &&
               MICRO_EVENT_SYSTEM_UI_INITIALIZED == 4 && MICRO_EVENT_NETWORK_CONFIG_LOADED == 5 &&
               MICRO_EVENT_SETUP_SKIPPED == 6 && MICRO_EVENT_OPEN_SETTINGS == 7 &&
               MICRO_EVENT_BACK_PRESSED == 8 && MICRO_EVENT_HOME_PRESSED == 9 &&
               MICRO_EVENT_OPEN_APP == 10 && MICRO_EVENT_APP_STARTED == 11 &&
               MICRO_EVENT_APP_FAILED == 12 && MICRO_EVENT_RESTART_APP == 13 &&
               MICRO_EVENT_APP_STOPPED == 14 && MICRO_EVENT_WIFI_SCAN_REQUESTED == 15 &&
               MICRO_EVENT_WIFI_SCAN_COMPLETED == 16 && MICRO_EVENT_WIFI_SCAN_FAILED == 17 &&
               MICRO_EVENT_WIFI_CONNECT_REQUESTED == 18 && MICRO_EVENT_WIFI_CONNECTED == 19 &&
               MICRO_EVENT_WIFI_PERSISTED == 20 && MICRO_EVENT_WIFI_FAILED == 21 &&
               MICRO_EVENT_RECONNECT_DUE == 22 && MICRO_EVENT_RECONNECT_NOW_REQUESTED == 23 &&
               MICRO_EVENT_CLEAR_NETWORK_REQUESTED == 24 && MICRO_EVENT_CLEAR_NETWORK_CONFIRMED == 25 &&
               MICRO_EVENT_CLEAR_NETWORK_COMPLETED == 26 && MICRO_EVENT_FACTORY_RESET_REQUESTED == 27 &&
               MICRO_EVENT_FACTORY_RESET_CONFIRMED == 28 && MICRO_EVENT_FACTORY_RESET_COMPLETED == 29 &&
               MICRO_EVENT_REBOOT_REQUESTED == 30 && MICRO_EVENT_SET_BACKLIGHT == 31,
               "micro_event_kind_t discriminants drifted");
_Static_assert(MICRO_ACTION_NONE == 0 && MICRO_ACTION_REJECTED == 1 &&
               MICRO_ACTION_ACTIONS == 2 && MICRO_ACTION_ENTER_SAFE_MODE == 3 &&
               MICRO_ACTION_INITIALIZE_STORAGE == 4 && MICRO_ACTION_VALIDATE_PROFILE == 5 &&
               MICRO_ACTION_INITIALIZE_DISPLAY == 6 && MICRO_ACTION_INITIALIZE_SYSTEM_UI == 7 &&
               MICRO_ACTION_LOAD_NETWORK_CONFIG == 8 && MICRO_ACTION_SHOW_FIRST_RUN_SETUP == 9 &&
               MICRO_ACTION_SHOW_LAUNCHER == 10 && MICRO_ACTION_SHOW_SETTINGS == 11 &&
               MICRO_ACTION_START_WIFI_SCAN == 12 && MICRO_ACTION_CONNECT_WIFI == 13 &&
               MICRO_ACTION_CONNECT_SAVED_WIFI == 14 && MICRO_ACTION_PERSIST_WIFI == 15 &&
               MICRO_ACTION_CLEAR_PENDING_WIFI == 16 && MICRO_ACTION_SCHEDULE_WIFI_RECONNECT == 17 &&
               MICRO_ACTION_START_APP == 18 && MICRO_ACTION_STOP_APP == 19 &&
               MICRO_ACTION_SHOW_APP_ERROR == 20 && MICRO_ACTION_CONFIRM_CLEAR_NETWORK == 21 &&
               MICRO_ACTION_CLEAR_NETWORK == 22 && MICRO_ACTION_CONFIRM_FACTORY_RESET == 23 &&
               MICRO_ACTION_FACTORY_RESET == 24 && MICRO_ACTION_REBOOT == 25 &&
               MICRO_ACTION_APPLY_BACKLIGHT == 26,
               "micro_action_kind_t discriminants drifted");

micro_runtime_t *micro_runtime_create(const uint8_t *mbc, size_t len,
                                      uint64_t budget, char *error, size_t error_len);
micro_error_t micro_runtime_activate(micro_runtime_t *runtime, uint32_t handler_id);
micro_error_t micro_runtime_tick(micro_runtime_t *runtime, char *error, size_t error_len);
void micro_runtime_destroy(micro_runtime_t *runtime);

micro_os_t *micro_os_create(void);
micro_error_t micro_os_dispatch(micro_os_t *os, const micro_event_t *event,
                                micro_action_buffer_t *actions,
                                char *error, size_t error_len);
micro_state_t micro_os_state(const micro_os_t *os);
void micro_os_destroy(micro_os_t *os);

int micro_esp_ui_create_column(uint32_t node, uint32_t parent);
int micro_esp_ui_create_row(uint32_t node, uint32_t parent);
int micro_esp_ui_create_list(uint32_t node, uint32_t parent);
int micro_esp_ui_create_progress(uint32_t node, uint32_t parent, double fraction);
int micro_esp_ui_create_switch(uint32_t node, uint32_t parent, int checked, uint32_t handler);
/* Zero preserves platform defaults; nonzero font handles are platform-owned. */
int micro_esp_ui_create_label(uint32_t node, uint32_t parent,
                              const uint8_t *text, size_t len,
                              uintptr_t font_handle, uint32_t line_height_px);
int micro_esp_ui_create_button(uint32_t node, uint32_t parent,
                               const uint8_t *text, size_t len, uint32_t handler,
                               uintptr_t font_handle, uint32_t line_height_px);
int micro_esp_ui_set_label_text(uint32_t node, const uint8_t *text, size_t len);
int micro_esp_ui_set_progress_value(uint32_t node, double fraction);
int micro_esp_ui_set_switch_checked(uint32_t node, int checked);
/* Create an editable single-line text field. `text` is the current value,
 * `placeholder` the hint shown when empty, and `handler` the onChange
 * handler id (or MICRO_UI_NO_HANDLER to disable editing). */
int micro_esp_ui_create_input(uint32_t node, uint32_t parent,
                              const uint8_t *text, size_t len,
                              const uint8_t *placeholder, size_t placeholder_len,
                              uint32_t handler, uintptr_t font_handle,
                              uint32_t line_height_px);
int micro_esp_ui_set_input_text(uint32_t node, const uint8_t *text, size_t len);
/* Draggable numeric slider. `value` is the initial position within [min,max];
 * `handler` is the onChange handler id (or MICRO_UI_NO_HANDLER to disable). */
int micro_esp_ui_create_slider(uint32_t node, uint32_t parent,
                               double value, double min, double max,
                               uint32_t handler);
int micro_esp_ui_set_slider_value(uint32_t node, double value);
/* Checkbox with a text label. `checked` is the initial state; `handler` is
 * the onChange handler id (or MICRO_UI_NO_HANDLER to disable). */
int micro_esp_ui_create_checkbox(uint32_t node, uint32_t parent,
                                 const uint8_t *label, size_t label_len,
                                 int checked, uint32_t handler);
/* Dropdown selection list. `options` is the choice strings joined with '\n';
 * `index` is the initially selected option. `handler` is the onChange handler
 * id (or MICRO_UI_NO_HANDLER to disable). */
int micro_esp_ui_create_dropdown(uint32_t node, uint32_t parent,
                                 const uint8_t *options, size_t options_len,
                                 double index, uint32_t handler);
/* LED indicator. `on` sets full/off brightness. */
int micro_esp_ui_create_led(uint32_t node, uint32_t parent, int on);
int micro_esp_ui_set_led(uint32_t node, int on);
/* Loading spinner. `active` shows/hides it. */
int micro_esp_ui_create_spinner(uint32_t node, uint32_t parent, int active);
int micro_esp_ui_set_spinner(uint32_t node, int active);
/* Read-only gauge. `value` is the needle position within [min,max]. */
int micro_esp_ui_create_scale(uint32_t node, uint32_t parent,
                              double value, double min, double max);
int micro_esp_ui_set_scale_value(uint32_t node, double value);
int micro_esp_ui_create_roller(uint32_t node, uint32_t parent,
                                const uint8_t *options, size_t options_len,
                                double index, uint32_t handler);
int micro_esp_ui_set_selection_value(uint32_t node, double index);
int micro_esp_ui_destroy_app_root(void);
/* Returns 1 with a handler, 0 when empty, and a negative bridge error. */
int micro_esp_ui_take_activation(uint32_t *handler_id);
/* Returns 1 with a handler and its new text, 0 when empty, and a negative
 * bridge error. Copies at most text_capacity bytes into text. */
int micro_esp_ui_take_input_change(uint32_t *handler_id, uint8_t *text,
                                   size_t text_capacity, size_t *text_len);
/* Returns 1 with a handler and its new value, 0 when empty, and a negative
 * bridge error. */
int micro_esp_ui_take_slider_change(uint32_t *handler_id, double *value);
/* Returns 1 with a handler and its new checked state, 0 when empty. */
int micro_esp_ui_take_checkbox_change(uint32_t *handler_id, int *checked);
/* Returns 1 with a handler and the selected index, 0 when empty. */
int micro_esp_ui_take_dropdown_change(uint32_t *handler_id, double *index);
int micro_esp_ui_take_roller_change(uint32_t *handler_id, double *index);
void micro_esp_ui_report_diagnostic(uint32_t node, const uint8_t *message, size_t len);

#ifdef __cplusplus
}
#endif

#endif
