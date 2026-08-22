#include "micro_runtime_ffi.h"
#include "lvgl.h"
#include <assert.h>
#include <stdarg.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static lv_obj_t screen;
static lv_obj_t pool[16];
static size_t used;
static int locks;
static int warnings;
static uint32_t dispatched_handler;

bool lvgl_port_lock(uint32_t timeout_ms) { assert(timeout_ms == 0); locks++; return true; }
void lvgl_port_unlock(void) { locks--; }
lv_obj_t *lv_screen_active(void) { return &screen; }
static lv_obj_t *make(lv_obj_t *parent, int kind) { lv_obj_t *obj=&pool[used++]; obj->parent=parent; obj->kind=kind; return obj; }
lv_obj_t *lv_obj_create(lv_obj_t *parent) { return make(parent, 1); }
lv_obj_t *lv_label_create(lv_obj_t *parent) { return make(parent, 2); }
lv_obj_t *lv_button_create(lv_obj_t *parent) { return make(parent, 3); }
lv_obj_t *lv_bar_create(lv_obj_t *parent) { return make(parent, 4); }
lv_obj_t *lv_switch_create(lv_obj_t *parent) { lv_obj_t *obj = make(parent, 5); obj->flags = LV_OBJ_FLAG_CLICKABLE; return obj; }
void lv_label_set_text(lv_obj_t *obj, const char *text) { snprintf(obj->text, sizeof obj->text, "%s", text); }
void lv_obj_set_flex_flow(lv_obj_t *obj, int flow) { obj->flex_flow = flow; }
void lv_obj_set_style_pad_column(lv_obj_t *obj, int32_t value, lv_style_selector_t selector) { assert(selector == LV_PART_MAIN); obj->pad_column = value; }
void lv_bar_set_range(lv_obj_t *obj, int32_t min, int32_t max) { obj->bar_min = min; obj->bar_max = max; }
void lv_bar_set_value(lv_obj_t *obj, int32_t value, int anim) { (void)anim; obj->bar_value = value; }
void lv_obj_set_size(lv_obj_t *obj, int32_t width, int32_t height) { obj->width = width; obj->height = height; }
void lv_obj_add_state(lv_obj_t *obj, int state) { obj->state |= (uint32_t)state; }
void lv_obj_clear_state(lv_obj_t *obj, int state) { obj->state &= ~(uint32_t)state; }
void lv_obj_remove_flag(lv_obj_t *obj, int flag) { obj->flags &= ~(uint32_t)flag; }
void lv_obj_set_style_text_font(lv_obj_t *obj, const lv_font_t *font, lv_style_selector_t selector) { assert(selector == LV_PART_MAIN); obj->font=font; }
void lv_obj_set_style_text_line_space(lv_obj_t *obj, int32_t value, lv_style_selector_t selector) { assert(selector == LV_PART_MAIN); obj->line_space=value; }
void lv_obj_delete(lv_obj_t *obj) { obj->deleted=1; }
int32_t lv_font_get_line_height(const lv_font_t *font) { return (int32_t)font->line_height; }
void lv_obj_add_event_cb(lv_obj_t *obj, lv_event_cb_t callback, int event, void *user_data) { assert(event == LV_EVENT_CLICKED); obj->callback=callback; obj->user_data=user_data; }
void *lv_event_get_user_data(lv_event_t *event) { return event->user_data; }
void test_log_warning(const char *tag, const char *format, ...) { (void)tag; (void)format; warnings++; }
static void click(lv_obj_t *obj) { lv_event_t event = { .user_data = obj->user_data }; assert(obj->callback); obj->callback(&event); }
static void dispatch_queued_activations(void) { uint32_t handler; while (micro_esp_ui_take_activation(&handler) == 1) dispatched_handler = handler; }

int main(void) {
    static const lv_font_t font = { .line_height = 18 };
    assert(micro_esp_ui_create_column(0, UINT32_MAX) == 0);
    assert(pool[0].parent == &screen);
    assert(micro_esp_ui_create_label(1, 0, (const uint8_t *)"Hello", 5, (uintptr_t)&font, 24) == 0);
    assert(pool[1].parent == &pool[0] && strcmp(pool[1].text, "Hello") == 0);
    assert(pool[1].font == &font && pool[1].line_space == 6);
    assert(micro_esp_ui_create_button(2, 0, (const uint8_t *)"Go", 2, 7, (uintptr_t)&font, 24) == 0);
    assert(pool[2].kind == 3 && pool[3].parent == &pool[2] && strcmp(pool[3].text, "Go") == 0);
    assert(micro_esp_ui_set_label_text(2, (const uint8_t *)"Next", 4) == 0);
    assert(strcmp(pool[3].text, "Next") == 0);
    click(&pool[2]);
    dispatch_queued_activations();
    assert(dispatched_handler == 7);
    uint32_t handler = 0;
    assert(micro_esp_ui_take_activation(&handler) == 0);
    micro_esp_ui_report_diagnostic(2, (const uint8_t *)"missing", 7);
    assert(warnings == 1);
    assert(micro_esp_ui_create_column(256, UINT32_MAX) != 0);
    assert(micro_esp_ui_create_label(4, 99, (const uint8_t *)"x", 1, 0, 0) != 0);
    assert(micro_esp_ui_destroy_app_root() == 0 && pool[0].deleted == 1 && locks == 0);

    static const lv_font_t font12 = { .line_height = 16 };
    assert(micro_esp_ui_create_label(4, UINT32_MAX, (const uint8_t *)"Root", 4, (uintptr_t)&font12, 12) == 0);
    assert(pool[4].line_space == -4);
    assert(micro_esp_ui_create_column(5, UINT32_MAX) != 0);
    assert(micro_esp_ui_destroy_app_root() == 0 && pool[4].deleted == 1);

    static const lv_font_t font14 = { .line_height = 17 };
    assert(micro_esp_ui_create_button(5, UINT32_MAX, (const uint8_t *)"Root", 4, 9, (uintptr_t)&font14, 14) == 0);
    assert(pool[6].line_space == -3);
    assert(micro_esp_ui_destroy_app_root() == 0 && pool[5].deleted == 1);

    /* Row, progress, and switch bridge contract. */
    assert(micro_esp_ui_create_row(8, UINT32_MAX) == 0);
    assert(pool[7].parent == &screen && pool[7].flex_flow == LV_FLEX_FLOW_ROW && pool[7].pad_column == 16);
    assert(micro_esp_ui_create_progress(9, 8, 0.5) == 0);
    assert(pool[8].parent == &pool[7] && pool[8].bar_min == 0 && pool[8].bar_max == 100 && pool[8].bar_value == 50);
    assert(pool[8].width == 100 && pool[8].height == 12);
    assert(micro_esp_ui_create_switch(10, 8, 1, 42) == 0);
    assert(pool[9].parent == &pool[7] && (pool[9].state & LV_STATE_CHECKED));
    assert(micro_esp_ui_create_switch(11, 8, 0, UINT32_MAX) == 0);
    assert(pool[10].parent == &pool[7] && !(pool[10].flags & LV_OBJ_FLAG_CLICKABLE) && !(pool[10].state & LV_STATE_CHECKED));
    assert(micro_esp_ui_set_progress_value(9, 0.75) == 0);
    assert(pool[8].bar_value == 75);
    assert(micro_esp_ui_set_progress_value(9, 1.5) == 0);
    assert(pool[8].bar_value == 100);
    assert(micro_esp_ui_set_progress_value(9, -0.5) == 0);
    assert(pool[8].bar_value == 0);
    assert(micro_esp_ui_set_switch_checked(10, 0) == 0);
    assert(!(pool[9].state & LV_STATE_CHECKED));
    assert(micro_esp_ui_set_switch_checked(10, 1) == 0);
    assert(pool[9].state & LV_STATE_CHECKED);
    click(&pool[9]);
    dispatch_queued_activations();
    assert(dispatched_handler == 42);
    assert(micro_esp_ui_take_activation(&handler) == 0);
    assert(micro_esp_ui_set_progress_value(99, 0.1) != 0);
    assert(micro_esp_ui_set_switch_checked(99, 1) != 0);
    assert(micro_esp_ui_destroy_app_root() == 0 && pool[7].deleted == 1 && locks == 0);
    puts("ESP32 LVGL UI bridge contract passed");
}
