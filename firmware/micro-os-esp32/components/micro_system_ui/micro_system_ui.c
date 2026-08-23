/* Trusted native system shell: launcher + settings, rendered with raw LVGL.
 *
 * The shell is a separate root object from the App's root (which the App
 * bridge `micro_esp_ui_*` creates). main.c shows the shell when no App runs
 * and hides it (and the App root replaces it) while an App owns the screen.
 * All LVGL access happens on the LVGL task under the LVGL lock, like the App
 * bridge. Button taps are enqueued and drained by main.c, which owns the OS
 * reducer.
 */

#include <stdint.h>
#include <string.h>

#include "esp_lvgl_port.h"
#include "lvgl.h"

#include "micro_system_ui.h"

extern const lv_font_t micro_ui_sans_24;

#define MICRO_SYSTEM_UI_TAP_CAPACITY 16U

static lv_obj_t *s_root;
static lv_obj_t *s_content;
static lv_obj_t *s_status_title;
static lv_obj_t *s_status_wifi;
static lv_obj_t *s_status_backlight;

static micro_system_ui_tap_t s_taps[MICRO_SYSTEM_UI_TAP_CAPACITY];
static unsigned s_tap_read;
static unsigned s_tap_write;

static void enqueue_tap(micro_system_ui_tap_t tap)
{
    unsigned next = (s_tap_write + 1U) % MICRO_SYSTEM_UI_TAP_CAPACITY;
    if (next == s_tap_read) {
        return; /* queue full; drop the tap */
    }
    s_taps[s_tap_write] = tap;
    s_tap_write = next;
}

static void tap_callback(lv_event_t *event)
{
    micro_system_ui_tap_t tap =
        (micro_system_ui_tap_t)(uintptr_t)lv_event_get_user_data(event);
    enqueue_tap(tap);
}

static lv_obj_t *make_button(lv_obj_t *parent, const char *label,
                             micro_system_ui_tap_t tap)
{
    lv_obj_t *button = lv_button_create(parent);
    lv_obj_set_style_text_font(button, &micro_ui_sans_24, LV_PART_MAIN);
    lv_obj_set_style_text_color(button, lv_color_hex(0x101820), LV_PART_MAIN);
    lv_obj_set_style_bg_color(button, lv_color_hex(0xE7EAE4), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(button, LV_OPA_COVER, LV_PART_MAIN);
    lv_obj_set_style_radius(button, 8, LV_PART_MAIN);
    lv_obj_t *text = lv_label_create(button);
    lv_label_set_text(text, label);
    lv_obj_center(text);
    lv_obj_add_event_cb(button, tap_callback, LV_EVENT_CLICKED,
                        (void *)(uintptr_t)tap);
    return button;
}

static void ensure_root(void)
{
    if (s_root != NULL) {
        return;
    }
    lv_obj_t *screen = lv_screen_active();
    s_root = lv_obj_create(screen);
    lv_obj_set_size(s_root, LV_PCT(100), LV_PCT(100));
    lv_obj_set_style_pad_all(s_root, 0, LV_PART_MAIN);
    lv_obj_set_style_border_width(s_root, 0, LV_PART_MAIN);
    lv_obj_set_style_radius(s_root, 0, LV_PART_MAIN);
    lv_obj_set_style_bg_color(s_root, lv_color_hex(0xF2F4F0), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(s_root, LV_OPA_COVER, LV_PART_MAIN);

    lv_obj_t *status_bar = lv_obj_create(s_root);
    lv_obj_set_size(status_bar, LV_PCT(100), 40);
    lv_obj_align(status_bar, LV_ALIGN_TOP_MID, 0, 0);
    lv_obj_set_style_pad_all(status_bar, 0, LV_PART_MAIN);
    lv_obj_set_style_border_width(status_bar, 1, LV_PART_MAIN);
    lv_obj_set_style_border_side(status_bar, LV_BORDER_SIDE_BOTTOM, LV_PART_MAIN);
    lv_obj_set_style_border_color(status_bar, lv_color_hex(0xD5D9CE), LV_PART_MAIN);
    lv_obj_set_style_bg_color(status_bar, lv_color_hex(0xE7EAE4), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(status_bar, LV_OPA_COVER, LV_PART_MAIN);

    s_status_title = lv_label_create(status_bar);
    lv_label_set_text(s_status_title, "micro-os");
    lv_obj_set_style_text_font(s_status_title, &micro_ui_sans_24, LV_PART_MAIN);
    lv_obj_set_style_text_color(s_status_title, lv_color_hex(0x101820), LV_PART_MAIN);
    lv_obj_align(s_status_title, LV_ALIGN_LEFT_MID, 12, 0);

    s_status_wifi = lv_label_create(status_bar);
    lv_obj_set_style_text_font(s_status_wifi, &micro_ui_sans_24, LV_PART_MAIN);
    lv_obj_set_style_text_color(s_status_wifi, lv_color_hex(0x101820), LV_PART_MAIN);
    lv_obj_align(s_status_wifi, LV_ALIGN_CENTER, 0, 0);

    s_status_backlight = lv_label_create(status_bar);
    lv_obj_set_style_text_font(s_status_backlight, &micro_ui_sans_24, LV_PART_MAIN);
    lv_obj_set_style_text_color(s_status_backlight, lv_color_hex(0x101820), LV_PART_MAIN);
    lv_obj_align(s_status_backlight, LV_ALIGN_RIGHT_MID, -12, 0);

    s_content = lv_obj_create(s_root);
    lv_obj_set_size(s_content, LV_PCT(100), LV_PCT(100));
    lv_obj_align(s_content, LV_ALIGN_TOP_MID, 0, 40);
    lv_obj_set_style_pad_all(s_content, 16, LV_PART_MAIN);
    lv_obj_set_style_border_width(s_content, 0, LV_PART_MAIN);
    lv_obj_set_style_radius(s_content, 0, LV_PART_MAIN);
    lv_obj_set_style_bg_color(s_content, lv_color_hex(0xF2F4F0), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(s_content, LV_OPA_COVER, LV_PART_MAIN);
    lv_obj_remove_flag(s_content, LV_OBJ_FLAG_SCROLLABLE);
}

static void update_status(const char *wifi_state, const char *wifi_ssid,
                          uint32_t backlight)
{
    char wifi[80];
    if (wifi_ssid != NULL && wifi_ssid[0] != '\0') {
        snprintf(wifi, sizeof wifi, "wifi %s · %s", wifi_state, wifi_ssid);
    } else {
        snprintf(wifi, sizeof wifi, "wifi %s", wifi_state);
    }
    lv_label_set_text(s_status_wifi, wifi);
    char bl[24];
    snprintf(bl, sizeof bl, "BL %lu", (unsigned long)backlight);
    lv_label_set_text(s_status_backlight, bl);
}

void micro_system_ui_show_launcher(const char *wifi_state, const char *wifi_ssid,
                                   uint32_t backlight)
{
    if (!lvgl_port_lock(0)) {
        return;
    }
    ensure_root();
    update_status(wifi_state, wifi_ssid, backlight);
    lv_obj_clean(s_content);

    lv_obj_t *column = lv_obj_create(s_content);
    lv_obj_set_width(column, LV_PCT(100));
    lv_obj_set_height(column, LV_SIZE_CONTENT);
    lv_obj_set_style_pad_all(column, 0, LV_PART_MAIN);
    lv_obj_set_style_border_width(column, 0, LV_PART_MAIN);
    lv_obj_set_style_bg_opa(column, LV_OPA_TRANSP, LV_PART_MAIN);
    lv_obj_set_flex_flow(column, LV_FLEX_FLOW_COLUMN);
    lv_obj_set_flex_align(column, LV_FLEX_ALIGN_START, LV_FLEX_ALIGN_CENTER,
                          LV_FLEX_ALIGN_CENTER);
    lv_obj_set_style_pad_row(column, 16, LV_PART_MAIN);

    lv_obj_t *title = lv_label_create(column);
    lv_label_set_text(title, "MICRO OS");
    lv_obj_set_style_text_font(title, &micro_ui_sans_24, LV_PART_MAIN);
    lv_obj_set_style_text_color(title, lv_color_hex(0x101820), LV_PART_MAIN);

    lv_obj_t *counter = make_button(column, "01  Counter",
                                    MICRO_SYSTEM_UI_TAP_OPEN_COUNTER);
    lv_obj_set_size(counter, 240, 64);

    lv_obj_t *settings = make_button(column, "02  Settings",
                                     MICRO_SYSTEM_UI_TAP_OPEN_SETTINGS);
    lv_obj_set_size(settings, 240, 64);

    lv_obj_remove_flag(s_root, LV_OBJ_FLAG_HIDDEN);
    lvgl_port_unlock();
}

void micro_system_ui_show_settings(const char *wifi_state, const char *wifi_ssid,
                                   uint32_t backlight)
{
    if (!lvgl_port_lock(0)) {
        return;
    }
    ensure_root();
    update_status(wifi_state, wifi_ssid, backlight);
    lv_obj_clean(s_content);

    lv_obj_t *column = lv_obj_create(s_content);
    lv_obj_set_width(column, LV_PCT(100));
    lv_obj_set_height(column, LV_SIZE_CONTENT);
    lv_obj_set_style_pad_all(column, 0, LV_PART_MAIN);
    lv_obj_set_style_border_width(column, 0, LV_PART_MAIN);
    lv_obj_set_style_bg_opa(column, LV_OPA_TRANSP, LV_PART_MAIN);
    lv_obj_set_flex_flow(column, LV_FLEX_FLOW_COLUMN);
    lv_obj_set_flex_align(column, LV_FLEX_ALIGN_START, LV_FLEX_ALIGN_START,
                          LV_FLEX_ALIGN_START);
    lv_obj_set_style_pad_row(column, 14, LV_PART_MAIN);

    lv_obj_t *back = make_button(column, "←  Back", MICRO_SYSTEM_UI_TAP_BACK);
    lv_obj_set_size(back, 160, 48);

    lv_obj_t *info = lv_label_create(column);
    lv_label_set_text(info, "ESP32-S3 · 8 MB Flash · 8 MB PSRAM");
    lv_obj_set_style_text_font(info, &micro_ui_sans_24, LV_PART_MAIN);
    lv_obj_set_style_text_color(info, lv_color_hex(0x101820), LV_PART_MAIN);

    lv_obj_t *backlight_button =
        make_button(column, "Toggle backlight", MICRO_SYSTEM_UI_TAP_BACKLIGHT_TOGGLE);
    lv_obj_set_size(backlight_button, 240, 48);

    lv_obj_t *wifi_button = make_button(column, "Connect Wi-Fi (micro-demo)",
                                        MICRO_SYSTEM_UI_TAP_WIFI_CONNECT);
    lv_obj_set_size(wifi_button, 300, 48);

    lv_obj_t *disconnect_button =
        make_button(column, "Disconnect Wi-Fi", MICRO_SYSTEM_UI_TAP_WIFI_DISCONNECT);
    lv_obj_set_size(disconnect_button, 240, 48);

    lv_obj_remove_flag(s_root, LV_OBJ_FLAG_HIDDEN);
    lvgl_port_unlock();
}

void micro_system_ui_hide(void)
{
    if (!lvgl_port_lock(0)) {
        return;
    }
    if (s_root != NULL) {
        lv_obj_add_flag(s_root, LV_OBJ_FLAG_HIDDEN);
    }
    lvgl_port_unlock();
}

micro_system_ui_tap_t micro_system_ui_take_tap(void)
{
    if (s_tap_read == s_tap_write) {
        return MICRO_SYSTEM_UI_TAP_NONE;
    }
    micro_system_ui_tap_t tap = s_taps[s_tap_read];
    s_tap_read = (s_tap_read + 1U) % MICRO_SYSTEM_UI_TAP_CAPACITY;
    return tap;
}
