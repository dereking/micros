#include "micro_runtime_ffi.h"

struct micro_abi_exports {
    micro_runtime_t *(*runtime_create)(const uint8_t *, size_t, uint64_t,
                                       char *, size_t);
    micro_error_t (*runtime_activate)(micro_runtime_t *, uint32_t);
    micro_error_t (*runtime_tick)(micro_runtime_t *, char *, size_t);
    void (*runtime_destroy)(micro_runtime_t *);
    micro_os_t *(*os_create)(void);
    micro_error_t (*os_dispatch)(micro_os_t *, const micro_event_t *,
                                 micro_action_buffer_t *, char *, size_t);
    micro_state_t (*os_state)(const micro_os_t *);
    void (*os_destroy)(micro_os_t *);
};

static const struct micro_abi_exports MICRO_ABI_EXPORTS = {
    .runtime_create = micro_runtime_create,
    .runtime_activate = micro_runtime_activate,
    .runtime_tick = micro_runtime_tick,
    .runtime_destroy = micro_runtime_destroy,
    .os_create = micro_os_create,
    .os_dispatch = micro_os_dispatch,
    .os_state = micro_os_state,
    .os_destroy = micro_os_destroy,
};

const void *micro_runtime_ffi_keepalive(void)
{
    return &MICRO_ABI_EXPORTS;
}

int micro_esp_ui_create_column(uint32_t node, uint32_t parent)
{
    (void)node;
    (void)parent;
    return 0;
}

int micro_esp_ui_create_label(uint32_t node, uint32_t parent,
                              const uint8_t *text, size_t len,
                              uintptr_t font_handle, uint32_t line_height_px)
{
    (void)node;
    (void)parent;
    (void)text;
    (void)len;
    (void)font_handle;
    (void)line_height_px;
    return 0;
}

int micro_esp_ui_create_button(uint32_t node, uint32_t parent,
                               const uint8_t *text, size_t len,
                               uint32_t handler, uintptr_t font_handle,
                               uint32_t line_height_px)
{
    (void)node;
    (void)parent;
    (void)text;
    (void)len;
    (void)handler;
    (void)font_handle;
    (void)line_height_px;
    return 0;
}

int micro_esp_ui_set_label_text(uint32_t node, const uint8_t *text, size_t len)
{
    (void)node;
    (void)text;
    (void)len;
    return 0;
}

int micro_esp_ui_destroy_app_root(void)
{
    return 0;
}
