#include "micro_appbar.h"

#include <stdio.h>
#include <string.h>
#include <time.h>

/* The shared 14 px / 18 px UI faces are linked by both LVGL hosts. The Wi-Fi
 * glyph comes from LVGL's built-in Montserrat face. */
extern const lv_font_t micro_ui_sans_14;
extern const lv_font_t micro_ui_sans_18;

#define MICRO_APPBAR_BG 0xE7EAE4
#define MICRO_APPBAR_BORDER 0xD5D9CE
#define MICRO_APPBAR_TEXT 0x101820
#define MICRO_APPBAR_MUTED 0x49705E
#define MICRO_APPBAR_GREEN 0x1E7256
#define MICRO_APPBAR_TOOLTIP 0x101820
#define MICRO_APPBAR_TOOLTIP_TEXT 0xF3F6F1

static lv_obj_t *s_screen;
static lv_obj_t *s_bar;
static lv_obj_t *s_content;
static lv_obj_t *s_wifi;
static lv_obj_t *s_wifi_icon;
static lv_obj_t *s_tip;
static lv_obj_t *s_tip_label;
static lv_obj_t *s_title;
static lv_obj_t *s_clock;
static lv_obj_t *s_close;
static lv_obj_t *s_off;
static lv_timer_t *s_clock_timer;
static lv_timer_t *s_tip_timer;
static int s_width;
static int s_height;
static int s_visible = 1;
static int s_back;
static int s_off_active;
static int s_power_on;
static char s_wifi_state[16];
static char s_wifi_ssid[33];

static void apply_flat_style(lv_obj_t *object, uint32_t background)
{
    lv_obj_set_style_pad_all(object, 0, LV_PART_MAIN);
    lv_obj_set_style_radius(object, 0, LV_PART_MAIN);
    lv_obj_set_style_border_width(object, 0, LV_PART_MAIN);
    lv_obj_set_style_bg_color(object, lv_color_hex(background), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(object, LV_OPA_COVER, LV_PART_MAIN);
    lv_obj_remove_flag(object, LV_OBJ_FLAG_SCROLLABLE);
}

static void close_callback(lv_event_t *event)
{
    (void)event;
    s_back = 1;
}

static void clock_timer_callback(lv_timer_t *timer)
{
    (void)timer;
    if (!s_visible || s_clock == NULL) {
        return;
    }
    time_t now = time(NULL);
    struct tm local;
    char text[16];
    if (now <= 0 || localtime_r(&now, &local) == NULL ||
        local.tm_year + 1900 < 2020) {
        snprintf(text, sizeof text, "--:--");
    } else {
        snprintf(text, sizeof text, "%02d:%02d", local.tm_hour, local.tm_min);
    }
    const char *current = lv_label_get_text(s_clock);
    if (current == NULL || strcmp(current, text) != 0) {
        lv_label_set_text(s_clock, text);
    }
}

static void tooltip_timer_callback(lv_timer_t *timer)
{
    (void)timer;
    /* The timer has repeat count 1, so LVGL deletes it after this callback. */
    s_tip_timer = NULL;
    if (s_tip != NULL) {
        lv_obj_add_flag(s_tip, LV_OBJ_FLAG_HIDDEN);
    }
}

static void hide_wifi_tooltip(void)
{
    if (s_tip_timer != NULL) {
        lv_timer_delete(s_tip_timer);
        s_tip_timer = NULL;
    }
    if (s_tip != NULL) {
        lv_obj_add_flag(s_tip, LV_OBJ_FLAG_HIDDEN);
    }
}

static void power_on_callback(lv_event_t *event)
{
    (void)event;
    s_power_on = 1;
}

/* The power-off surface is created lazily so a normal boot never allocates it.
 * It covers the whole display (bar included) and is clickable as a whole. */
static void ensure_off_screen(void)
{
    if (s_off != NULL || s_screen == NULL) {
        return;
    }
    s_off = lv_obj_create(s_screen);
    apply_flat_style(s_off, 0x101820);
    lv_obj_add_flag(s_off, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_add_event_cb(s_off, power_on_callback, LV_EVENT_CLICKED, NULL);

    lv_obj_t *title = lv_label_create(s_off);
    lv_obj_set_style_text_font(title, &micro_ui_sans_18, LV_PART_MAIN);
    lv_obj_set_style_text_color(title, lv_color_hex(MICRO_APPBAR_TOOLTIP_TEXT),
                                LV_PART_MAIN);
    lv_label_set_text(title, "Powered off");
    lv_obj_align(title, LV_ALIGN_CENTER, 0, -12);

    lv_obj_t *hint = lv_label_create(s_off);
    lv_obj_set_style_text_font(hint, &micro_ui_sans_14, LV_PART_MAIN);
    lv_obj_set_style_text_color(hint, lv_color_hex(0x93A69A), LV_PART_MAIN);
    lv_label_set_text(hint, "Tap to power on");
    lv_obj_align(hint, LV_ALIGN_CENTER, 0, 16);

    lv_obj_add_flag(s_off, LV_OBJ_FLAG_HIDDEN);
}

static void show_wifi_tooltip(int auto_hide)
{
    if (!s_visible || s_tip == NULL) {
        return;
    }
    if (s_tip_timer != NULL) {
        lv_timer_delete(s_tip_timer);
        s_tip_timer = NULL;
    }
    lv_obj_remove_flag(s_tip, LV_OBJ_FLAG_HIDDEN);
    if (!auto_hide) {
        /* Mouse hover: keep the SSID visible until the pointer leaves. */
        return;
    }
    /* Touch/click: auto-hide after a short read; tapping again restarts it. */
    s_tip_timer = lv_timer_create(tooltip_timer_callback, 2600, NULL);
    if (s_tip_timer != NULL) {
        lv_timer_set_repeat_count(s_tip_timer, 1);
    }
}

static void wifi_event_callback(lv_event_t *event)
{
    switch (lv_event_get_code(event)) {
        case LV_EVENT_CLICKED:
            /* A mouse click is already hovering; touch needs the timer. */
            show_wifi_tooltip(!lv_obj_has_state(s_wifi, LV_STATE_HOVERED));
            break;
        case LV_EVENT_HOVER_OVER:
            show_wifi_tooltip(0);
            break;
        case LV_EVENT_HOVER_LEAVE:
            hide_wifi_tooltip();
            break;
        default:
            break;
    }
}

static void update_wifi_state(void)
{
    if (s_wifi_icon == NULL) {
        return;
    }
    const char *state = s_wifi_state[0] != '\0' ? s_wifi_state : "off";
    int connected = strcmp(state, "connected") == 0;
    lv_obj_set_style_text_color(
        s_wifi_icon,
        lv_color_hex(connected ? MICRO_APPBAR_GREEN : 0x84937D),
        LV_PART_MAIN);
    if (s_tip_label != NULL) {
        const char *text = s_wifi_ssid[0] != '\0' ? s_wifi_ssid : state;
        const char *current = lv_label_get_text(s_tip_label);
        if (current == NULL || strcmp(current, text) != 0) {
            lv_label_set_text(s_tip_label, text);
        }
    }
}

static void reposition_children(void)
{
    if (s_bar == NULL || s_content == NULL) {
        return;
    }
    lv_obj_set_size(s_bar, s_width, MICRO_APPBAR_HEIGHT);
    lv_obj_set_pos(s_bar, 0, 0);
    if (s_visible) {
        lv_obj_remove_flag(s_bar, LV_OBJ_FLAG_HIDDEN);
        lv_obj_set_pos(s_content, 0, MICRO_APPBAR_HEIGHT);
        lv_obj_set_size(s_content, s_width, s_height - MICRO_APPBAR_HEIGHT);
    } else {
        lv_obj_add_flag(s_bar, LV_OBJ_FLAG_HIDDEN);
        lv_obj_set_pos(s_content, 0, 0);
        lv_obj_set_size(s_content, s_width, s_height);
        hide_wifi_tooltip();
    }
    /* Power-off hides the content too; only the off surface may keep it
     * hidden. Restoring the bar without this leaves Home running but blank
     * after a power cycle. */
    if (!s_off_active) {
        lv_obj_remove_flag(s_content, LV_OBJ_FLAG_HIDDEN);
    }

    if (s_tip != NULL) {
        lv_obj_set_pos(s_tip, 6, MICRO_APPBAR_HEIGHT + 6);
    }
    if (s_title != NULL) {
        /* Symmetric side zones keep the title on the display centre line in
         * both orientations; the label itself is centre-aligned. */
        int title_width = s_width - 2 * 96;
        if (title_width < 80) {
            title_width = s_width - 20;
        }
        lv_obj_set_width(s_title, title_width);
    }
    if (s_off_active) {
        if (s_bar != NULL) {
            lv_obj_add_flag(s_bar, LV_OBJ_FLAG_HIDDEN);
        }
        if (s_content != NULL) {
            lv_obj_add_flag(s_content, LV_OBJ_FLAG_HIDDEN);
        }
    }
}

void micro_appbar_ensure(void)
{
    lv_obj_t *screen = lv_screen_active();
    if (screen == NULL) {
        return;
    }
    if (s_screen == screen && s_bar != NULL) {
        return;
    }
    if (s_bar != NULL) {
        /* The display was recreated (native host restart): drop the stale
         * pointers without touching unknown/new objects. */
        micro_appbar_reset();
    }
    s_screen = screen;
    lv_display_t *display = lv_obj_get_display(screen);
    s_width = display != NULL
                  ? (int)lv_display_get_horizontal_resolution(display)
                  : (int)LV_HOR_RES;
    s_height = display != NULL
                   ? (int)lv_display_get_vertical_resolution(display)
                   : (int)LV_VER_RES;

    s_content = lv_obj_create(screen);
    apply_flat_style(s_content, 0xF2F4F0);
    lv_obj_set_pos(s_content, 0, MICRO_APPBAR_HEIGHT);
    lv_obj_set_size(s_content, s_width, s_height - MICRO_APPBAR_HEIGHT);

    s_bar = lv_obj_create(screen);
    apply_flat_style(s_bar, MICRO_APPBAR_BG);
    lv_obj_set_style_border_width(s_bar, 1, LV_PART_MAIN);
    lv_obj_set_style_border_side(s_bar, LV_BORDER_SIDE_BOTTOM, LV_PART_MAIN);
    lv_obj_set_style_border_color(s_bar, lv_color_hex(MICRO_APPBAR_BORDER),
                                  LV_PART_MAIN);

    /* Wi-Fi is represented by the system glyph only. Hover (mouse) and
     * click/tap both reveal a small tooltip with the SSID below the bar. */
    s_wifi = lv_button_create(s_bar);
    lv_obj_set_size(s_wifi, 30, 30);
    lv_obj_align(s_wifi, LV_ALIGN_LEFT_MID, 10, 0);
    lv_obj_set_ext_click_area(s_wifi, 4);
    lv_obj_set_style_radius(s_wifi, LV_RADIUS_CIRCLE, LV_PART_MAIN);
    lv_obj_set_style_bg_opa(s_wifi, LV_OPA_TRANSP, LV_PART_MAIN);
    lv_obj_set_style_bg_color(s_wifi, lv_color_hex(0xFFFFFF),
                              LV_STATE_HOVERED);
    lv_obj_set_style_bg_opa(s_wifi, LV_OPA_60, LV_STATE_HOVERED);
    lv_obj_set_style_border_width(s_wifi, 0, LV_PART_MAIN);
    lv_obj_set_style_shadow_width(s_wifi, 0, LV_PART_MAIN);
    lv_obj_set_style_pad_all(s_wifi, 0, LV_PART_MAIN);
    lv_obj_add_event_cb(s_wifi, wifi_event_callback, LV_EVENT_CLICKED, NULL);
    lv_obj_add_event_cb(s_wifi, wifi_event_callback, LV_EVENT_HOVER_OVER, NULL);
    lv_obj_add_event_cb(s_wifi, wifi_event_callback, LV_EVENT_HOVER_LEAVE, NULL);

    s_wifi_icon = lv_label_create(s_wifi);
    lv_obj_set_style_text_font(s_wifi_icon, &lv_font_montserrat_14,
                               LV_PART_MAIN);
    lv_obj_set_style_text_color(s_wifi_icon, lv_color_hex(0x84937D),
                                LV_PART_MAIN);
    lv_label_set_text(s_wifi_icon, LV_SYMBOL_WIFI);
    lv_obj_center(s_wifi_icon);

    s_title = lv_label_create(s_bar);
    lv_obj_set_style_text_font(s_title, &micro_ui_sans_18, LV_PART_MAIN);
    lv_obj_set_style_text_color(s_title, lv_color_hex(MICRO_APPBAR_TEXT),
                                LV_PART_MAIN);
    lv_label_set_long_mode(s_title, LV_LABEL_LONG_MODE_DOTS);
    lv_obj_set_style_text_align(s_title, LV_TEXT_ALIGN_CENTER, LV_PART_MAIN);
    lv_label_set_text(s_title, "");
    lv_obj_align(s_title, LV_ALIGN_CENTER, 0, 0);

    s_clock = lv_label_create(s_bar);
    lv_obj_set_style_text_font(s_clock, &micro_ui_sans_14, LV_PART_MAIN);
    lv_obj_set_style_text_color(s_clock, lv_color_hex(MICRO_APPBAR_TEXT),
                                LV_PART_MAIN);
    lv_label_set_text(s_clock, "--:--");
    lv_obj_align(s_clock, LV_ALIGN_RIGHT_MID, -54, 0);

    s_close = lv_button_create(s_bar);
    lv_obj_set_size(s_close, 28, 28);
    lv_obj_align(s_close, LV_ALIGN_RIGHT_MID, -10, 0);
    /* 28 px visual circle + 8 px halo = the 44 px minimum touch target. */
    lv_obj_set_ext_click_area(s_close, 8);
    lv_obj_set_style_radius(s_close, LV_RADIUS_CIRCLE, LV_PART_MAIN);
    lv_obj_set_style_bg_opa(s_close, LV_OPA_TRANSP, LV_PART_MAIN);
    lv_obj_set_style_bg_color(s_close, lv_color_hex(0xFFFFFF),
                              LV_STATE_HOVERED);
    lv_obj_set_style_bg_opa(s_close, LV_OPA_60, LV_STATE_HOVERED);
    lv_obj_set_style_bg_opa(s_close, LV_OPA_COVER, LV_STATE_PRESSED);
    lv_obj_set_style_border_width(s_close, 1, LV_PART_MAIN);
    lv_obj_set_style_border_color(s_close, lv_color_hex(0x84937D), LV_PART_MAIN);
    lv_obj_set_style_border_color(s_close, lv_color_hex(MICRO_APPBAR_TEXT),
                                  LV_STATE_HOVERED);
    lv_obj_set_style_shadow_width(s_close, 0, LV_PART_MAIN);
    lv_obj_set_style_pad_all(s_close, 0, LV_PART_MAIN);
    lv_obj_add_event_cb(s_close, close_callback, LV_EVENT_CLICKED, NULL);
    lv_obj_t *close_label = lv_label_create(s_close);
    lv_obj_set_style_text_font(close_label, &lv_font_montserrat_14, LV_PART_MAIN);
    lv_obj_set_style_text_color(close_label, lv_color_hex(MICRO_APPBAR_TEXT),
                                LV_PART_MAIN);
    lv_label_set_text(close_label, LV_SYMBOL_CLOSE);
    lv_obj_center(close_label);

    /* The tooltip is a screen child (not a bar child) so it can extend below
     * the 40 px strip and sit above the App content. It is created last so it
     * is on top of the bar/content siblings. */
    s_tip = lv_obj_create(screen);
    apply_flat_style(s_tip, MICRO_APPBAR_TOOLTIP);
    lv_obj_set_size(s_tip, 220, 32);
    lv_obj_set_pos(s_tip, 6, MICRO_APPBAR_HEIGHT + 6);
    lv_obj_set_style_radius(s_tip, 6, LV_PART_MAIN);
    lv_obj_set_style_shadow_width(s_tip, 8, LV_PART_MAIN);
    lv_obj_set_style_shadow_color(s_tip, lv_color_hex(0x101820),
                                  LV_PART_MAIN);
    lv_obj_set_style_shadow_opa(s_tip, LV_OPA_30, LV_PART_MAIN);
    lv_obj_set_style_shadow_offset_y(s_tip, 3, LV_PART_MAIN);
    lv_obj_remove_flag(s_tip, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_add_flag(s_tip, LV_OBJ_FLAG_HIDDEN);

    s_tip_label = lv_label_create(s_tip);
    lv_obj_set_style_text_font(s_tip_label, &micro_ui_sans_14, LV_PART_MAIN);
    lv_obj_set_style_text_color(s_tip_label,
                                lv_color_hex(MICRO_APPBAR_TOOLTIP_TEXT),
                                LV_PART_MAIN);
    lv_obj_set_width(s_tip_label, 200);
    lv_label_set_long_mode(s_tip_label, LV_LABEL_LONG_MODE_DOTS);
    lv_label_set_text(s_tip_label, "off");
    lv_obj_align(s_tip_label, LV_ALIGN_LEFT_MID, 10, 0);

    s_clock_timer = lv_timer_create(clock_timer_callback, 1000, NULL);
    update_wifi_state();
    reposition_children();
    clock_timer_callback(NULL);
}

void micro_appbar_set_visible(int visible)
{
    micro_appbar_ensure();
    s_visible = visible ? 1 : 0;
    if (!s_visible) {
        hide_wifi_tooltip();
    }
    reposition_children();
}

int micro_appbar_visible(void)
{
    return s_visible;
}

int micro_appbar_content_visible(void)
{
    if (s_content == NULL) {
        return 0;
    }
    return !lv_obj_has_flag(s_content, LV_OBJ_FLAG_HIDDEN);
}

void micro_appbar_set_viewport(int width, int height)
{
    if (width <= 0 || height <= 0) {
        return;
    }
    micro_appbar_ensure();
    s_width = width;
    s_height = height;
    reposition_children();
    if (s_off != NULL) {
        lv_obj_set_pos(s_off, 0, 0);
        lv_obj_set_size(s_off, s_width, s_height);
    }
    if (s_content != NULL) {
        /* The App root fills the content box; its `ui.place` children then
         * run their anchor layout against the new available width/height. */
        uint32_t count = lv_obj_get_child_count(s_content);
        for (uint32_t i = 0; i < count; ++i) {
            lv_obj_set_size(lv_obj_get_child(s_content, i), LV_PCT(100),
                            LV_PCT(100));
        }
        lv_obj_update_layout(s_content);
        lv_obj_invalidate(s_content);
    }
    if (s_screen != NULL) {
        lv_obj_invalidate(s_screen);
    }
}

lv_obj_t *micro_appbar_content(void)
{
    micro_appbar_ensure();
    return s_content;
}

void micro_appbar_set_title(const char *title)
{
    micro_appbar_ensure();
    if (s_title == NULL) {
        return;
    }
    lv_label_set_text(s_title, title != NULL ? title : "");
}

void micro_appbar_set_wifi(const char *state, const char *ssid)
{
    micro_appbar_ensure();
    snprintf(s_wifi_state, sizeof s_wifi_state, "%s", state != NULL ? state : "off");
    snprintf(s_wifi_ssid, sizeof s_wifi_ssid, "%s", ssid != NULL ? ssid : "");
    update_wifi_state();
}

int micro_appbar_take_back(void)
{
    int result = s_back;
    s_back = 0;
    return result;
}

void micro_appbar_power_off(void)
{
    micro_appbar_ensure();
    ensure_off_screen();
    if (s_off == NULL) {
        return;
    }
    hide_wifi_tooltip();
    s_off_active = 1;
    s_power_on = 0;
    lv_obj_set_pos(s_off, 0, 0);
    lv_obj_set_size(s_off, s_width, s_height);
    lv_obj_remove_flag(s_off, LV_OBJ_FLAG_HIDDEN);
    lv_obj_move_foreground(s_off);
    if (s_bar != NULL) {
        lv_obj_add_flag(s_bar, LV_OBJ_FLAG_HIDDEN);
    }
    if (s_content != NULL) {
        lv_obj_add_flag(s_content, LV_OBJ_FLAG_HIDDEN);
    }
}

int micro_appbar_power_off_active(void)
{
    return s_off_active;
}

int micro_appbar_take_power_on(void)
{
    int result = s_power_on;
    s_power_on = 0;
    return result;
}

void micro_appbar_wake(void)
{
    if (!s_off_active) {
        return;
    }
    s_off_active = 0;
    s_power_on = 0;
    if (s_off != NULL) {
        lv_obj_add_flag(s_off, LV_OBJ_FLAG_HIDDEN);
    }
    reposition_children();
    if (s_screen != NULL) {
        lv_obj_invalidate(s_screen);
    }
}

void micro_appbar_press_power_on(void)
{
    if (s_off_active) {
        s_power_on = 1;
    }
}

void micro_appbar_reset(void)
{
    if (s_clock_timer != NULL) {
        lv_timer_delete(s_clock_timer);
        s_clock_timer = NULL;
    }
    if (s_tip_timer != NULL) {
        lv_timer_delete(s_tip_timer);
        s_tip_timer = NULL;
    }
    s_screen = NULL;
    s_bar = NULL;
    s_content = NULL;
    s_wifi = NULL;
    s_wifi_icon = NULL;
    s_tip = NULL;
    s_tip_label = NULL;
    s_title = NULL;
    s_clock = NULL;
    s_close = NULL;
    s_off = NULL;
    s_visible = 1;
    s_back = 0;
    s_off_active = 0;
    s_power_on = 0;
    s_wifi_state[0] = '\0';
    s_wifi_ssid[0] = '\0';
}
