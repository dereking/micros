#include "micro_runtime_ffi.h"
#include "esp_log.h"
#include "esp_lvgl_port.h"
#include "lvgl.h"

#include <stdlib.h>
#include <string.h>

#ifndef MICRO_UI_BRIDGE_HOST_TEST
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
#endif

#define MICRO_UI_MAX_NODES 256U
#define MICRO_UI_NO_PARENT UINT32_MAX
#define MICRO_UI_NO_HANDLER UINT32_MAX
#define MICRO_UI_ACTIVATION_CAPACITY 64U

struct micro_click_context {
    uint32_t handler;
};

static lv_obj_t *objects[MICRO_UI_MAX_NODES];
static lv_obj_t *text_targets[MICRO_UI_MAX_NODES];
static lv_obj_t *app_root;
static struct micro_click_context click_contexts[MICRO_UI_MAX_NODES];
static uint32_t activations[MICRO_UI_ACTIVATION_CAPACITY];
static unsigned activation_read;
static unsigned activation_write;

static void click_callback(lv_event_t *event)
{
    const struct micro_click_context *context = lv_event_get_user_data(event);
    unsigned next = (activation_write + 1U) % MICRO_UI_ACTIVATION_CAPACITY;
    if (next == activation_read) {
        ESP_LOGW("micro_ui", "activation queue full; dropping handler %lu",
                 (unsigned long)context->handler);
        return;
    }
    activations[activation_write] = context->handler;
    activation_write = next;
}

static lv_obj_t *parent_object(uint32_t parent)
{
    if (parent == MICRO_UI_NO_PARENT) {
        return lv_screen_active();
    }
    return parent < MICRO_UI_MAX_NODES ? objects[parent] : NULL;
}

static char *copy_text(const uint8_t *text, size_t len)
{
    if ((text == NULL && len != 0) || len == SIZE_MAX) {
        return NULL;
    }
    char *copy = malloc(len + 1);
    if (copy == NULL) {
        return NULL;
    }
    if (len != 0) {
        memcpy(copy, text, len);
    }
    copy[len] = '\0';
    return copy;
}

static void apply_text_style(lv_obj_t *label, uintptr_t font_handle,
                             uint32_t line_height_px)
{
    if (font_handle == 0) {
        return;
    }
    const lv_font_t *font = (const lv_font_t *)font_handle;
    lv_obj_set_style_text_font(label, font, LV_PART_MAIN);
    int32_t line_space = (int32_t)line_height_px - lv_font_get_line_height(font);
    lv_obj_set_style_text_line_space(label, line_space, LV_PART_MAIN);
}

static int begin_create(uint32_t node, uint32_t parent, lv_obj_t **resolved_parent)
{
    if (node >= MICRO_UI_MAX_NODES || objects[node] != NULL ||
        (parent == MICRO_UI_NO_PARENT && app_root != NULL)) {
        return -1;
    }
    *resolved_parent = parent_object(parent);
    return *resolved_parent == NULL ? -2 : 0;
}

int micro_esp_ui_create_column(uint32_t node, uint32_t parent)
{
    if (!lvgl_port_lock(0)) return -3;
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *column = lv_obj_create(parent_obj);
        if (column == NULL) result = -4;
        else {
            lv_obj_set_flex_flow(column, LV_FLEX_FLOW_COLUMN);
            objects[node] = column;
            if (parent == MICRO_UI_NO_PARENT) app_root = column;
        }
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_create_row(uint32_t node, uint32_t parent)
{
    if (!lvgl_port_lock(0)) return -3;
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *row = lv_obj_create(parent_obj);
        if (row == NULL) result = -4;
        else {
            lv_obj_set_flex_flow(row, LV_FLEX_FLOW_ROW);
            lv_obj_set_style_pad_column(row, 16, LV_PART_MAIN);
            objects[node] = row;
            if (parent == MICRO_UI_NO_PARENT) app_root = row;
        }
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_create_progress(uint32_t node, uint32_t parent, double fraction)
{
    if (!lvgl_port_lock(0)) return -3;
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *bar = lv_bar_create(parent_obj);
        if (bar == NULL) result = -4;
        else {
            lv_bar_set_range(bar, 0, 100);
            lv_bar_set_value(bar, (int32_t)(fraction * 100.0), LV_ANIM_OFF);
            lv_obj_set_size(bar, LV_PCT(100), 12);
            objects[node] = bar;
            if (parent == MICRO_UI_NO_PARENT) app_root = bar;
        }
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_create_switch(uint32_t node, uint32_t parent, int checked,
                               uint32_t handler)
{
    if (!lvgl_port_lock(0)) return -3;
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *toggle = lv_switch_create(parent_obj);
        if (toggle == NULL) result = -4;
        else {
            if (checked) lv_obj_add_state(toggle, LV_STATE_CHECKED);
            if (handler == MICRO_UI_NO_HANDLER) {
                lv_obj_remove_flag(toggle, LV_OBJ_FLAG_CLICKABLE);
            } else {
                click_contexts[node].handler = handler;
                lv_obj_add_event_cb(toggle, click_callback, LV_EVENT_CLICKED,
                                    &click_contexts[node]);
            }
            objects[node] = toggle;
            if (parent == MICRO_UI_NO_PARENT) app_root = toggle;
        }
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_set_progress_value(uint32_t node, double fraction)
{
    if (!lvgl_port_lock(0)) return -3;
    int result = -1;
    if (node < MICRO_UI_MAX_NODES && objects[node] != NULL) {
        if (fraction < 0.0) fraction = 0.0;
        if (fraction > 1.0) fraction = 1.0;
        lv_bar_set_value(objects[node], (int32_t)(fraction * 100.0), LV_ANIM_OFF);
        result = 0;
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_set_switch_checked(uint32_t node, int checked)
{
    if (!lvgl_port_lock(0)) return -3;
    int result = -1;
    if (node < MICRO_UI_MAX_NODES && objects[node] != NULL) {
        if (checked) lv_obj_add_state(objects[node], LV_STATE_CHECKED);
        else lv_obj_clear_state(objects[node], LV_STATE_CHECKED);
        result = 0;
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_create_label(uint32_t node, uint32_t parent,
                              const uint8_t *text, size_t len,
                              uintptr_t font_handle, uint32_t line_height_px)
{
    char *copy = copy_text(text, len);
    if (copy == NULL) return -5;
    if (!lvgl_port_lock(0)) { free(copy); return -3; }
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *label = lv_label_create(parent_obj);
        if (label == NULL) result = -4;
        else {
            lv_label_set_text(label, copy);
            apply_text_style(label, font_handle, line_height_px);
            objects[node] = label;
            text_targets[node] = label;
            if (parent == MICRO_UI_NO_PARENT) app_root = label;
        }
    }
    lvgl_port_unlock();
    free(copy);
    return result;
}

int micro_esp_ui_create_button(uint32_t node, uint32_t parent,
                               const uint8_t *text, size_t len,
                               uint32_t handler, uintptr_t font_handle,
                               uint32_t line_height_px)
{
    (void)handler;
    char *copy = copy_text(text, len);
    if (copy == NULL) return -5;
    if (!lvgl_port_lock(0)) { free(copy); return -3; }
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *button = lv_button_create(parent_obj);
        lv_obj_t *label = button == NULL ? NULL : lv_label_create(button);
        if (button == NULL || label == NULL) {
            if (button != NULL) lv_obj_delete(button);
            result = -4;
        }
        else {
            lv_label_set_text(label, copy);
            apply_text_style(label, font_handle, line_height_px);
            click_contexts[node].handler = handler;
            lv_obj_add_event_cb(button, click_callback, LV_EVENT_CLICKED,
                                &click_contexts[node]);
            objects[node] = button;
            text_targets[node] = label;
            if (parent == MICRO_UI_NO_PARENT) app_root = button;
        }
    }
    lvgl_port_unlock();
    free(copy);
    return result;
}

int micro_esp_ui_set_label_text(uint32_t node, const uint8_t *text, size_t len)
{
    char *copy = copy_text(text, len);
    if (copy == NULL) return -5;
    if (!lvgl_port_lock(0)) { free(copy); return -3; }
    int result = -1;
    if (node < MICRO_UI_MAX_NODES && text_targets[node] != NULL) {
        lv_label_set_text(text_targets[node], copy);
        result = 0;
    }
    lvgl_port_unlock();
    free(copy);
    return result;
}

int micro_esp_ui_destroy_app_root(void)
{
    if (!lvgl_port_lock(0)) return -3;
    if (app_root != NULL) lv_obj_delete(app_root);
    app_root = NULL;
    memset(objects, 0, sizeof objects);
    memset(text_targets, 0, sizeof text_targets);
    activation_read = 0;
    activation_write = 0;
    lvgl_port_unlock();
    return 0;
}

int micro_esp_ui_take_activation(uint32_t *handler_id)
{
    if (handler_id == NULL) return -1;
    if (!lvgl_port_lock(0)) return -3;
    if (activation_read == activation_write) {
        lvgl_port_unlock();
        return 0;
    }
    *handler_id = activations[activation_read];
    activation_read = (activation_read + 1U) % MICRO_UI_ACTIVATION_CAPACITY;
    lvgl_port_unlock();
    return 1;
}

void micro_esp_ui_report_diagnostic(uint32_t node, const uint8_t *message, size_t len)
{
    char *copy = copy_text(message, len);
    if (copy == NULL) {
        ESP_LOGW("micro_ui", "node %lu: diagnostic unavailable", (unsigned long)node);
        return;
    }
    ESP_LOGW("micro_ui", "node %lu: %s", (unsigned long)node, copy);
    free(copy);
}
