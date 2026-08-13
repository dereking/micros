#ifndef MICRO_RUNTIME_FFI_H
#define MICRO_RUNTIME_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct micro_runtime micro_runtime_t;
typedef struct micro_os micro_os_t;

typedef int32_t micro_error_t;
enum {
    MICRO_OK = 0,
    MICRO_ERR_MBC = 1,
    MICRO_ERR_RUNTIME = 2,
    MICRO_ERR_UI = 3,
    MICRO_ERR_INVALID_ARGUMENT = 4,
    MICRO_ERR_PANIC = 5,
    MICRO_ERR_STOPPED = 6,
};

typedef uint32_t micro_state_t;
enum {
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
};

typedef uint32_t micro_event_kind_t;
enum {
    MICRO_EVENT_BOOT_NORMAL = 0,
    MICRO_EVENT_BOOT_SAFE_MODE = 1,
    MICRO_EVENT_STORAGE_READY = 2,
    MICRO_EVENT_STORAGE_FAILED = 3,
    MICRO_EVENT_PROFILE_VALID = 4,
    MICRO_EVENT_PROFILE_INVALID = 5,
    MICRO_EVENT_DISPLAY_READY = 6,
    MICRO_EVENT_DISPLAY_FAILED = 7,
    MICRO_EVENT_SYSTEM_UI_READY = 8,
    MICRO_EVENT_SYSTEM_UI_FAILED = 9,
    MICRO_EVENT_NETWORK_CONFIGURED = 10,
    MICRO_EVENT_NETWORK_UNCONFIGURED = 11,
    MICRO_EVENT_SETUP_SKIPPED = 12,
    MICRO_EVENT_OPEN_SETTINGS = 13,
    MICRO_EVENT_BACK_PRESSED = 14,
    MICRO_EVENT_HOME_PRESSED = 15,
    MICRO_EVENT_REBOOT_REQUESTED = 16,
};

typedef struct {
    micro_event_kind_t kind;
} micro_event_t;

typedef uint32_t micro_action_t;
enum {
    MICRO_ACTION_NONE = 0,
    MICRO_ACTION_REJECTED = 1,
    MICRO_ACTION_ENTER_SAFE_MODE = 2,
    MICRO_ACTION_INITIALIZE_STORAGE = 3,
    MICRO_ACTION_VALIDATE_PROFILE = 4,
    MICRO_ACTION_INITIALIZE_DISPLAY = 5,
    MICRO_ACTION_INITIALIZE_SYSTEM_UI = 6,
    MICRO_ACTION_LOAD_NETWORK_CONFIG = 7,
    MICRO_ACTION_SHOW_FIRST_RUN_SETUP = 8,
    MICRO_ACTION_SHOW_LAUNCHER = 9,
    MICRO_ACTION_SHOW_SETTINGS = 10,
    MICRO_ACTION_CONNECT_SAVED_WIFI = 11,
    MICRO_ACTION_REBOOT = 12,
    MICRO_ACTION_COMPOSITE = 13,
    MICRO_ACTION_OTHER = 14,
};

_Static_assert(sizeof(micro_error_t) == 4, "micro_error_t must be 32-bit");
_Static_assert(sizeof(micro_state_t) == 4, "micro_state_t must be 32-bit");
_Static_assert(sizeof(micro_event_kind_t) == 4, "micro_event_kind_t must be 32-bit");
_Static_assert(sizeof(micro_event_t) == 4, "micro_event_t layout drifted");
_Static_assert(sizeof(micro_action_t) == 4, "micro_action_t must be 32-bit");
_Static_assert(MICRO_OK == 0 && MICRO_ERR_MBC == 1 && MICRO_ERR_RUNTIME == 2 &&
               MICRO_ERR_UI == 3 && MICRO_ERR_INVALID_ARGUMENT == 4 &&
               MICRO_ERR_PANIC == 5 && MICRO_ERR_STOPPED == 6,
               "micro_error_t discriminants drifted");
_Static_assert(MICRO_STATE_EARLY_BOOT == 0 && MICRO_STATE_SAFE_MODE == 1 &&
               MICRO_STATE_STORAGE_READY == 2 &&
               MICRO_STATE_BOARD_PROFILE_VALIDATED == 3 &&
               MICRO_STATE_DISPLAY_READY == 4 && MICRO_STATE_SYSTEM_UI_READY == 5 &&
               MICRO_STATE_FIRST_RUN_SETUP == 6 && MICRO_STATE_LAUNCHER == 7 &&
               MICRO_STATE_APP_STARTING == 8 && MICRO_STATE_APP_RUNNING == 9 &&
               MICRO_STATE_APP_STOPPING == 10 && MICRO_STATE_APP_ERROR == 11 &&
               MICRO_STATE_SETTINGS == 12,
               "micro_state_t discriminants drifted");
_Static_assert(MICRO_EVENT_BOOT_NORMAL == 0 && MICRO_EVENT_BOOT_SAFE_MODE == 1 &&
               MICRO_EVENT_STORAGE_READY == 2 && MICRO_EVENT_STORAGE_FAILED == 3 &&
               MICRO_EVENT_PROFILE_VALID == 4 && MICRO_EVENT_PROFILE_INVALID == 5 &&
               MICRO_EVENT_DISPLAY_READY == 6 && MICRO_EVENT_DISPLAY_FAILED == 7 &&
               MICRO_EVENT_SYSTEM_UI_READY == 8 && MICRO_EVENT_SYSTEM_UI_FAILED == 9 &&
               MICRO_EVENT_NETWORK_CONFIGURED == 10 &&
               MICRO_EVENT_NETWORK_UNCONFIGURED == 11 && MICRO_EVENT_SETUP_SKIPPED == 12 &&
               MICRO_EVENT_OPEN_SETTINGS == 13 && MICRO_EVENT_BACK_PRESSED == 14 &&
               MICRO_EVENT_HOME_PRESSED == 15 && MICRO_EVENT_REBOOT_REQUESTED == 16,
               "micro_event_kind_t discriminants drifted");
_Static_assert(MICRO_ACTION_NONE == 0 && MICRO_ACTION_REJECTED == 1 &&
               MICRO_ACTION_ENTER_SAFE_MODE == 2 && MICRO_ACTION_INITIALIZE_STORAGE == 3 &&
               MICRO_ACTION_VALIDATE_PROFILE == 4 && MICRO_ACTION_INITIALIZE_DISPLAY == 5 &&
               MICRO_ACTION_INITIALIZE_SYSTEM_UI == 6 &&
               MICRO_ACTION_LOAD_NETWORK_CONFIG == 7 &&
               MICRO_ACTION_SHOW_FIRST_RUN_SETUP == 8 && MICRO_ACTION_SHOW_LAUNCHER == 9 &&
               MICRO_ACTION_SHOW_SETTINGS == 10 && MICRO_ACTION_CONNECT_SAVED_WIFI == 11 &&
               MICRO_ACTION_REBOOT == 12 && MICRO_ACTION_COMPOSITE == 13 &&
               MICRO_ACTION_OTHER == 14,
               "micro_action_t discriminants drifted");

micro_runtime_t *micro_runtime_create(const uint8_t *mbc, size_t len,
                                      uint64_t budget, char *error, size_t error_len);
micro_error_t micro_runtime_activate(micro_runtime_t *runtime, uint32_t handler_id);
micro_error_t micro_runtime_tick(micro_runtime_t *runtime, char *error, size_t error_len);
void micro_runtime_destroy(micro_runtime_t *runtime);

micro_os_t *micro_os_create(void);
micro_action_t micro_os_dispatch(micro_os_t *os, micro_event_t event);
micro_state_t micro_os_state(const micro_os_t *os);
void micro_os_destroy(micro_os_t *os);

int micro_esp_ui_create_column(uint32_t node, uint32_t parent);
int micro_esp_ui_create_label(uint32_t node, uint32_t parent,
                              const uint8_t *text, size_t len);
int micro_esp_ui_create_button(uint32_t node, uint32_t parent,
                               const uint8_t *text, size_t len,
                               uint32_t handler);
int micro_esp_ui_set_label_text(uint32_t node, const uint8_t *text, size_t len);
int micro_esp_ui_destroy_app_root(void);

#ifdef __cplusplus
}
#endif

#endif
