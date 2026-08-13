#include "micro_runtime_ffi.h"

int micro_esp_ui_create_column(uint32_t node, uint32_t parent)
{
    (void)node;
    (void)parent;
    return 0;
}

int micro_esp_ui_create_label(uint32_t node, uint32_t parent,
                              const uint8_t *text, size_t len)
{
    (void)node;
    (void)parent;
    (void)text;
    (void)len;
    return 0;
}

int micro_esp_ui_create_button(uint32_t node, uint32_t parent,
                               const uint8_t *text, size_t len,
                               uint32_t handler)
{
    (void)node;
    (void)parent;
    (void)text;
    (void)len;
    (void)handler;
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
