#ifndef TEST_LVGL_H
#define TEST_LVGL_H
#include <stdint.h>
typedef struct lv_font_t { uint32_t line_height; } lv_font_t;
typedef struct lv_event_t lv_event_t;
typedef void (*lv_event_cb_t)(lv_event_t *event);
typedef struct lv_obj_t { struct lv_obj_t *parent; int kind; char text[64]; const lv_font_t *font; int32_t line_space; int deleted; lv_event_cb_t callback; void *user_data; } lv_obj_t;
struct lv_event_t { void *user_data; };
typedef int lv_style_selector_t;
#define LV_PART_MAIN 0
#define LV_FLEX_FLOW_COLUMN 1
#define LV_EVENT_CLICKED 1
lv_obj_t *lv_screen_active(void);
lv_obj_t *lv_obj_create(lv_obj_t *parent);
lv_obj_t *lv_label_create(lv_obj_t *parent);
lv_obj_t *lv_button_create(lv_obj_t *parent);
void lv_label_set_text(lv_obj_t *obj, const char *text);
void lv_obj_set_flex_flow(lv_obj_t *obj, int flow);
void lv_obj_set_style_text_font(lv_obj_t *obj, const lv_font_t *font, lv_style_selector_t selector);
void lv_obj_set_style_text_line_space(lv_obj_t *obj, int32_t value, lv_style_selector_t selector);
void lv_obj_delete(lv_obj_t *obj);
int32_t lv_font_get_line_height(const lv_font_t *font);
void lv_obj_add_event_cb(lv_obj_t *obj, lv_event_cb_t callback, int event, void *user_data);
void *lv_event_get_user_data(lv_event_t *event);
#endif
