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
void lv_label_set_text(lv_obj_t *obj, const char *text) { snprintf(obj->text, sizeof obj->text, "%s", text); }
void lv_obj_set_flex_flow(lv_obj_t *obj, int flow) { assert(flow == LV_FLEX_FLOW_COLUMN); obj->kind = 1; }
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
    puts("ESP32 LVGL UI bridge contract passed");
}
