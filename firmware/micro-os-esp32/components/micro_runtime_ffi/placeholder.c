#include "micro_runtime_ffi.h"
#include "esp_log.h"
#include "esp_lvgl_port.h"
#include "lvgl.h"

#include "pinyin_table.h"
#include "pinyin_phrase_table.h"

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
#define MICRO_UI_INPUT_CAPACITY 16U
#define MICRO_UI_INPUT_TEXT_MAX 256U
#define MICRO_UI_SLIDER_CAPACITY 32U

struct micro_click_context {
    uint32_t handler;
};

struct micro_input_change {
    uint32_t handler;
    size_t len;
    char text[MICRO_UI_INPUT_TEXT_MAX];
};

struct micro_slider_change {
    uint32_t handler;
    double value;
};

static lv_obj_t *objects[MICRO_UI_MAX_NODES];
static lv_obj_t *text_targets[MICRO_UI_MAX_NODES];
static lv_obj_t *app_root;
static struct micro_click_context click_contexts[MICRO_UI_MAX_NODES];
static uint32_t activations[MICRO_UI_ACTIVATION_CAPACITY];
static unsigned activation_read;
static unsigned activation_write;
static struct micro_input_change input_changes[MICRO_UI_INPUT_CAPACITY];
static unsigned input_read;
static unsigned input_write;
static struct micro_slider_change slider_changes[MICRO_UI_SLIDER_CAPACITY];
static unsigned slider_read;
static unsigned slider_write;
static lv_obj_t *s_keyboard;
static lv_obj_t *s_candidate_bar;
static lv_obj_t *s_pinyin_label;
static lv_obj_t *s_ime_toggle;
static lv_obj_t *s_target_ta;
static bool s_ime_active;
#define MICRO_IME_CANDIDATE_COUNT 8U
#define MICRO_IME_CANDIDATE_HEIGHT 44
#define MICRO_IME_PINYIN_MAX 12U
static lv_obj_t *s_candidate_buttons[MICRO_IME_CANDIDATE_COUNT];
static const char *s_candidate_texts[MICRO_IME_CANDIDATE_COUNT];
static uint8_t s_candidate_lens[MICRO_IME_CANDIDATE_COUNT];
static uint8_t s_candidate_shown;
static char s_pinyin[MICRO_IME_PINYIN_MAX];
static uint8_t s_pinyin_len;

extern const lv_font_t micro_ui_sans_24;

static void micro_esp_ui_hide_keyboard(void);
static void micro_esp_ui_update_candidates(void);

static void keyboard_callback(lv_event_t *event)
{
    lv_event_code_t code = lv_event_get_code(event);
    if (code == LV_EVENT_READY || code == LV_EVENT_CANCEL) {
        micro_esp_ui_hide_keyboard();
    }
}

static void keyboard_value_callback(lv_event_t *event)
{
    /* In Chinese mode the keyboard is detached (ta == NULL) so its built-in
     * handler drops the pressed characters; we buffer pinyin ourselves. */
    if (!s_ime_active) {
        return;
    }
    lv_obj_t *kb = lv_event_get_current_target(event);
    uint32_t btn = lv_buttonmatrix_get_selected_button(kb);
    if (btn == LV_BUTTONMATRIX_BUTTON_NONE) {
        return;
    }
    const char *txt = lv_buttonmatrix_get_button_text(kb, btn);
    if (txt == NULL || txt[0] == '\0') {
        return;
    }
    char c = txt[0];
    if (c >= 'A' && c <= 'Z') {
        c = (char)(c - 'A' + 'a');
    }
    if (c >= 'a' && c <= 'z') {
        if (s_pinyin_len < MICRO_IME_PINYIN_MAX - 1U) {
            s_pinyin[s_pinyin_len++] = c;
            s_pinyin[s_pinyin_len] = '\0';
        }
        micro_esp_ui_update_candidates();
    } else if (strcmp(txt, LV_SYMBOL_BACKSPACE) == 0) {
        if (s_pinyin_len > 0) {
            s_pinyin[--s_pinyin_len] = '\0';
            micro_esp_ui_update_candidates();
        }
    } else if (txt[0] == ' ') {
        if (s_target_ta != NULL) {
            lv_textarea_add_char(s_target_ta, ' ');
        }
    }
}

static void ime_toggle_callback(lv_event_t *event)
{
    (void)event;
    s_ime_active = !s_ime_active;
    lv_obj_t *toggle_label = lv_obj_get_child(s_ime_toggle, 0);
    if (s_ime_active) {
        /* Detach the text area so pressed letters are dropped by the built-in
         * handler instead of reaching the input, then keep the real input
         * visually focused so its caret does not disappear. */
        lv_keyboard_set_textarea(s_keyboard, NULL);
        if (s_target_ta != NULL) {
            lv_obj_add_state(s_target_ta, LV_STATE_FOCUSED);
        }
        lv_keyboard_set_mode(s_keyboard, LV_KEYBOARD_MODE_TEXT_LOWER);
        lv_obj_clear_flag(s_candidate_bar, LV_OBJ_FLAG_HIDDEN);
        if (toggle_label != NULL) {
            lv_label_set_text(toggle_label, "EN");
        }
        micro_esp_ui_update_candidates();
    } else {
        lv_keyboard_set_textarea(s_keyboard, s_target_ta);
        lv_obj_add_flag(s_candidate_bar, LV_OBJ_FLAG_HIDDEN);
        if (toggle_label != NULL) {
            lv_label_set_text(toggle_label, "中");
        }
        s_pinyin_len = 0;
        s_pinyin[0] = '\0';
    }
}

static void candidate_callback(lv_event_t *event)
{
    uint32_t index = (uint32_t)(uintptr_t)lv_event_get_user_data(event);
    if (index >= s_candidate_shown || s_target_ta == NULL) {
        return;
    }
    /* A candidate is either one Han (3 bytes + NUL) or a whole phrase
     * (up to 3 Han = 10 bytes + NUL). 16 covers both. */
    char ch[16];
    uint8_t len = s_candidate_lens[index];
    memcpy(ch, s_candidate_texts[index], len);
    ch[len] = '\0';
    lv_textarea_add_text(s_target_ta, ch);
    s_pinyin_len = 0;
    s_pinyin[0] = '\0';
    micro_esp_ui_update_candidates();
}

static void micro_esp_ui_update_candidates(void)
{
    s_candidate_shown = 0;
    if (s_pinyin_label != NULL) {
        lv_label_set_text(s_pinyin_label, s_pinyin);
    }

    /* Phrases first: an exact pinyin match is the most useful candidate. */
    if (s_pinyin_len > 0) {
        for (unsigned i = 0; i < MICRO_PINYIN_PHRASES_LEN; ++i) {
            if (strcmp(MICRO_PINYIN_PHRASES[i].pinyin, s_pinyin) == 0) {
                if (s_candidate_shown >= MICRO_IME_CANDIDATE_COUNT) {
                    break;
                }
                const char *phrase = MICRO_PINYIN_PHRASES[i].phrase;
                s_candidate_texts[s_candidate_shown] = phrase;
                s_candidate_lens[s_candidate_shown] = (uint8_t)strlen(phrase);
                lv_label_set_text(
                    lv_obj_get_child(s_candidate_buttons[s_candidate_shown], 0), phrase);
                lv_obj_clear_flag(s_candidate_buttons[s_candidate_shown], LV_OBJ_FLAG_HIDDEN);
                s_candidate_shown++;
            }
        }
    }

    /* Then single-character candidates for the typed syllable. */
    if (s_candidate_shown < MICRO_IME_CANDIDATE_COUNT && s_pinyin_len > 0) {
        const char *candidates = NULL;
        for (unsigned i = 0; i < MICRO_PINYIN_TABLE_LEN; ++i) {
            if (strcmp(MICRO_PINYIN_TABLE[i].pinyin, s_pinyin) == 0) {
                candidates = MICRO_PINYIN_TABLE[i].candidates;
                break;
            }
        }
        if (candidates != NULL) {
            const char *cursor = candidates;
            while (s_candidate_shown < MICRO_IME_CANDIDATE_COUNT && *cursor != '\0') {
                const char *start = cursor;
                uint8_t len = 0;
                /* Decode one UTF-8 char (Han are 3 bytes). */
                uint8_t b0 = (uint8_t)*cursor;
                if (b0 < 0x80) {
                    len = 1;
                } else if ((b0 & 0xE0) == 0xC0) {
                    len = 2;
                } else if ((b0 & 0xF0) == 0xE0) {
                    len = 3;
                } else if ((b0 & 0xF8) == 0xF0) {
                    len = 4;
                } else {
                    break;
                }
                cursor += len;
                s_candidate_texts[s_candidate_shown] = start;
                s_candidate_lens[s_candidate_shown] = len;
                char temp[5];
                memcpy(temp, start, len);
                temp[len] = '\0';
                lv_label_set_text(
                    lv_obj_get_child(s_candidate_buttons[s_candidate_shown], 0), temp);
                lv_obj_clear_flag(s_candidate_buttons[s_candidate_shown], LV_OBJ_FLAG_HIDDEN);
                s_candidate_shown++;
            }
        }
    }

    for (uint8_t i = s_candidate_shown; i < MICRO_IME_CANDIDATE_COUNT; ++i) {
        lv_obj_add_flag(s_candidate_buttons[i], LV_OBJ_FLAG_HIDDEN);
    }
    /* Keep the bar visible while composing pinyin even before any candidate
     * matches, so the user sees what they have typed. */
    if (s_candidate_shown > 0 || s_pinyin_len > 0) {
        lv_obj_clear_flag(s_candidate_bar, LV_OBJ_FLAG_HIDDEN);
    } else {
        lv_obj_add_flag(s_candidate_bar, LV_OBJ_FLAG_HIDDEN);
    }
}

static void micro_esp_ui_show_keyboard(lv_obj_t *textarea)
{
    if (textarea == NULL) {
        return;
    }
    s_target_ta = textarea;
    s_ime_active = false;
    s_pinyin_len = 0;
    s_pinyin[0] = '\0';
    if (s_keyboard == NULL) {
        s_keyboard = lv_keyboard_create(lv_screen_active());
        lv_obj_set_size(s_keyboard, LV_PCT(100), LV_PCT(38));
        lv_obj_align(s_keyboard, LV_ALIGN_BOTTOM_MID, 0, 0);
        lv_obj_add_event_cb(s_keyboard, keyboard_callback, LV_EVENT_READY, NULL);
        lv_obj_add_event_cb(s_keyboard, keyboard_callback, LV_EVENT_CANCEL, NULL);
        lv_obj_add_event_cb(s_keyboard, keyboard_value_callback, LV_EVENT_VALUE_CHANGED, NULL);

        /* Candidate row shown above the keyboard in Chinese mode. */
        s_candidate_bar = lv_obj_create(lv_screen_active());
        lv_obj_set_size(s_candidate_bar, LV_PCT(100), MICRO_IME_CANDIDATE_HEIGHT);
        lv_obj_align(s_candidate_bar, LV_ALIGN_BOTTOM_MID, 0, -182);
        lv_obj_set_flex_flow(s_candidate_bar, LV_FLEX_FLOW_ROW);
        lv_obj_set_style_pad_column(s_candidate_bar, 8, LV_PART_MAIN);
        lv_obj_set_style_pad_row(s_candidate_bar, 0, LV_PART_MAIN);
        lv_obj_set_style_pad_left(s_candidate_bar, 12, LV_PART_MAIN);
        lv_obj_set_style_pad_right(s_candidate_bar, 12, LV_PART_MAIN);
        lv_obj_set_style_pad_top(s_candidate_bar, 4, LV_PART_MAIN);
        lv_obj_set_style_pad_bottom(s_candidate_bar, 4, LV_PART_MAIN);
        lv_obj_set_style_bg_color(s_candidate_bar, lv_color_hex(0xDDDDDD), LV_PART_MAIN);
        lv_obj_set_style_bg_opa(s_candidate_bar, LV_OPA_COVER, LV_PART_MAIN);
        lv_obj_add_flag(s_candidate_bar, LV_OBJ_FLAG_HIDDEN);

        /* Pinyin composition label: shows what is being typed (e.g. "nihao")
         * as the first element of the candidate row. */
        s_pinyin_label = lv_label_create(s_candidate_bar);
        lv_label_set_text(s_pinyin_label, "");
        lv_obj_set_style_text_font(s_pinyin_label, &micro_ui_sans_24, LV_PART_MAIN);
        lv_obj_set_style_text_color(s_pinyin_label, lv_color_hex(0x101820), LV_PART_MAIN);
        lv_obj_set_style_bg_color(s_pinyin_label, lv_color_hex(0xFFFFFF), LV_PART_MAIN);
        lv_obj_set_style_bg_opa(s_pinyin_label, LV_OPA_COVER, LV_PART_MAIN);
        lv_obj_set_style_pad_left(s_pinyin_label, 8, LV_PART_MAIN);
        lv_obj_set_style_pad_right(s_pinyin_label, 8, LV_PART_MAIN);
        lv_obj_set_style_radius(s_pinyin_label, 4, LV_PART_MAIN);

        for (uint8_t i = 0; i < MICRO_IME_CANDIDATE_COUNT; ++i) {
            s_candidate_buttons[i] = lv_button_create(s_candidate_bar);
            lv_obj_set_height(s_candidate_buttons[i], MICRO_IME_CANDIDATE_HEIGHT - 8);
            lv_obj_set_style_pad_all(s_candidate_buttons[i], 0, LV_PART_MAIN);
            /* Do not steal keyboard/text-area focus when tapped. */
            lv_obj_remove_flag(s_candidate_buttons[i], LV_OBJ_FLAG_CLICK_FOCUSABLE);
            lv_obj_t *label = lv_label_create(s_candidate_buttons[i]);
            lv_label_set_text(label, "");
            lv_obj_set_style_text_font(label, &micro_ui_sans_24, LV_PART_MAIN);
            lv_obj_set_style_text_color(label, lv_color_hex(0x101820), LV_PART_MAIN);
            lv_obj_add_event_cb(s_candidate_buttons[i], candidate_callback, LV_EVENT_CLICKED,
                                (void *)(uintptr_t)i);
            lv_obj_add_flag(s_candidate_buttons[i], LV_OBJ_FLAG_HIDDEN);
        }

        /* 中/EN toggle floating over the keyboard's top-right corner. */
        s_ime_toggle = lv_button_create(lv_screen_active());
        lv_obj_set_size(s_ime_toggle, 56, 40);
        lv_obj_align(s_ime_toggle, LV_ALIGN_BOTTOM_RIGHT, -8, -186);
        /* Clicking the toggle must not defocus the text area, or the keyboard
         * would tear down and the IME could never engage. */
        lv_obj_remove_flag(s_ime_toggle, LV_OBJ_FLAG_CLICK_FOCUSABLE);
        lv_obj_t *toggle_label = lv_label_create(s_ime_toggle);
        lv_label_set_text(toggle_label, "中");
        lv_obj_center(toggle_label);
        lv_obj_set_style_text_font(toggle_label, &micro_ui_sans_24, LV_PART_MAIN);
        lv_obj_set_style_text_color(toggle_label, lv_color_hex(0x101820), LV_PART_MAIN);
        lv_obj_add_event_cb(s_ime_toggle, ime_toggle_callback, LV_EVENT_CLICKED, NULL);
        lv_obj_add_flag(s_ime_toggle, LV_OBJ_FLAG_HIDDEN);
    }
    lv_keyboard_set_textarea(s_keyboard, textarea);
    lv_obj_clear_flag(s_keyboard, LV_OBJ_FLAG_HIDDEN);
    lv_obj_clear_flag(s_ime_toggle, LV_OBJ_FLAG_HIDDEN);
    lv_obj_add_flag(s_candidate_bar, LV_OBJ_FLAG_HIDDEN);
    lv_obj_t *toggle_label = lv_obj_get_child(s_ime_toggle, 0);
    if (toggle_label != NULL) {
        lv_label_set_text(toggle_label, "中");
    }
    lv_obj_move_foreground(s_keyboard);
    lv_obj_move_foreground(s_ime_toggle);
    lv_obj_move_foreground(s_candidate_bar);
}

static void micro_esp_ui_hide_keyboard(void)
{
    if (s_keyboard != NULL) {
        lv_keyboard_set_textarea(s_keyboard, NULL);
        lv_obj_add_flag(s_keyboard, LV_OBJ_FLAG_HIDDEN);
        lv_obj_add_flag(s_ime_toggle, LV_OBJ_FLAG_HIDDEN);
        lv_obj_add_flag(s_candidate_bar, LV_OBJ_FLAG_HIDDEN);
        s_ime_active = false;
        s_pinyin_len = 0;
        s_pinyin[0] = '\0';
    }
}

static void input_focus_callback(lv_event_t *event)
{
    lv_obj_t *textarea = lv_event_get_target(event);
    lv_event_code_t code = lv_event_get_code(event);
    if (code == LV_EVENT_CLICKED || code == LV_EVENT_FOCUSED) {
        micro_esp_ui_show_keyboard(textarea);
    }
    /* Note: no DEFOCUSED handler here. Hiding on defocus makes the keyboard
     * disappear the moment the IME toggle or a keyboard key steals focus, and
     * the IME can never engage. The keyboard is dismissed only by its own
     * READY (✓) / CANCEL keys via keyboard_callback. */
}

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

static void input_callback(lv_event_t *event)
{
    const struct micro_click_context *context = lv_event_get_user_data(event);
    lv_obj_t *target = lv_event_get_target(event);
    if (target == NULL) {
        return;
    }
    const char *text = lv_textarea_get_text(target);
    if (text == NULL) {
        return;
    }
    size_t len = strlen(text);
    if (len > MICRO_UI_INPUT_TEXT_MAX - 1U) {
        len = MICRO_UI_INPUT_TEXT_MAX - 1U;
    }
    unsigned next = (input_write + 1U) % MICRO_UI_INPUT_CAPACITY;
    if (next == input_read) {
        ESP_LOGW("micro_ui", "input queue full; dropping handler %lu",
                 (unsigned long)context->handler);
        return;
    }
    input_changes[input_write].handler = context->handler;
    input_changes[input_write].len = len;
    memcpy(input_changes[input_write].text, text, len);
    input_changes[input_write].text[len] = '\0';
    input_write = next;
}

static void slider_callback(lv_event_t *event)
{
    const struct micro_click_context *context = lv_event_get_user_data(event);
    lv_obj_t *target = lv_event_get_target(event);
    if (target == NULL) {
        return;
    }
    unsigned next = (slider_write + 1U) % MICRO_UI_SLIDER_CAPACITY;
    if (next == slider_read) {
        ESP_LOGW("micro_ui", "slider queue full; dropping handler %lu",
                 (unsigned long)context->handler);
        return;
    }
    slider_changes[slider_write].handler = context->handler;
    slider_changes[slider_write].value = (double)lv_slider_get_value(target);
    slider_write = next;
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
    /* Force an opaque foreground so theme defaults cannot make the text
     * invisible against the white screen background. The actual font glyph
     * rendering uses the per-app font handle below. */
    lv_obj_set_style_text_color(label, lv_color_hex(0x101820), LV_PART_MAIN);
    if (font_handle == 0) {
        ESP_LOGE("micro_ui", "apply_text_style: font_handle is NULL");
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
            /* Mirror the SDL host: column fills its parent horizontally. The
             * root column (parent == screen) also fills vertically so the
             * whole Micro UI Tree covers the 800x480 panel. Nested columns
             * size to their content height. */
            lv_obj_set_size(column, LV_PCT(100),
                            parent == MICRO_UI_NO_PARENT ? LV_PCT(100)
                                                         : LV_SIZE_CONTENT);
            /* Focus must not auto-scroll the column; that makes the UI lurch
             * when the on-screen keyboard or an input grabs focus. */
            lv_obj_remove_flag(column, LV_OBJ_FLAG_SCROLL_ON_FOCUS);
            lv_obj_set_style_pad_left(column, 16, LV_PART_MAIN);
            lv_obj_set_style_pad_right(column, 16, LV_PART_MAIN);
            lv_obj_set_style_pad_top(column, 12, LV_PART_MAIN);
            lv_obj_set_style_pad_bottom(column, 12, LV_PART_MAIN);
            lv_obj_set_style_pad_row(column, 8, LV_PART_MAIN);
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
            /* Row spans the full width of its parent column and sizes to its
             * tallest child (LV_SIZE_CONTENT). Spacing between children is
             * 16 logical pixels, matching the SDL host. */
            lv_obj_set_size(row, LV_PCT(100), LV_SIZE_CONTENT);
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
            /* When the bar lives in a row we want it to absorb leftover
             * horizontal space (flex_grow=1) at a fixed height, leaving the
             * siblings at their content width. Using LV_PCT(100) here would
             * push the row past 100% and trigger the row's horizontal
             * scrollbar. In a column the bar spans the full width via the
             * default cross-axis stretch. */
            lv_obj_t *parent_for_grow = (parent == MICRO_UI_NO_PARENT)
                                            ? NULL : objects[parent];
            bool in_row = parent_for_grow != NULL &&
                          lv_obj_get_style_flex_flow(parent_for_grow, LV_PART_MAIN)
                              == LV_FLEX_FLOW_ROW;
            if (in_row) {
                lv_obj_set_height(bar, 12);
                lv_obj_set_flex_grow(bar, 1);
            } else {
                lv_obj_set_size(bar, LV_PCT(100), 12);
            }
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

int micro_esp_ui_create_input(uint32_t node, uint32_t parent,
                              const uint8_t *text, size_t len,
                              const uint8_t *placeholder, size_t placeholder_len,
                              uint32_t handler, uintptr_t font_handle,
                              uint32_t line_height_px)
{
    char *copy = copy_text(text, len);
    if (copy == NULL) return -5;
    char *ph = copy_text(placeholder, placeholder_len);
    if (ph == NULL) { free(copy); return -5; }
    if (!lvgl_port_lock(0)) { free(copy); free(ph); return -3; }
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *textarea = lv_textarea_create(parent_obj);
        if (textarea == NULL) result = -4;
        else {
            lv_textarea_set_one_line(textarea, true);
            lv_textarea_set_cursor_click_pos(textarea, true);
            /* Do not let focus scroll the column when the keyboard opens. */
            lv_obj_remove_flag(textarea, LV_OBJ_FLAG_SCROLL_ON_FOCUS);
            lv_textarea_set_text(textarea, copy);
            if (ph[0] != '\0') {
                lv_textarea_set_placeholder_text(textarea, ph);
            }
            apply_text_style(textarea, font_handle, line_height_px);
            /* Border so the field is visibly editable against the white
             * column background; the text style above stays on LV_PART_MAIN. */
            lv_obj_set_style_border_color(textarea, lv_color_hex(0x888888), LV_PART_MAIN);
            lv_obj_set_style_border_width(textarea, 2, LV_PART_MAIN);
            lv_obj_set_style_border_opa(textarea, LV_OPA_COVER, LV_PART_MAIN);
            lv_obj_set_style_pad_all(textarea, 8, LV_PART_MAIN);
            lv_obj_set_width(textarea, LV_PCT(100));
            lv_obj_set_height(textarea, LV_SIZE_CONTENT);
            if (handler == MICRO_UI_NO_HANDLER) {
                lv_obj_remove_flag(textarea, LV_OBJ_FLAG_CLICKABLE);
            } else {
                click_contexts[node].handler = handler;
                lv_obj_add_event_cb(textarea, input_callback, LV_EVENT_VALUE_CHANGED,
                                    &click_contexts[node]);
            }
            lv_obj_add_event_cb(textarea, input_focus_callback, LV_EVENT_CLICKED, NULL);
            lv_obj_add_event_cb(textarea, input_focus_callback, LV_EVENT_FOCUSED, NULL);
            objects[node] = textarea;
            text_targets[node] = textarea;
            if (parent == MICRO_UI_NO_PARENT) app_root = textarea;
        }
    }
    lvgl_port_unlock();
    free(copy);
    free(ph);
    return result;
}

int micro_esp_ui_set_input_text(uint32_t node, const uint8_t *text, size_t len)
{
    char *copy = copy_text(text, len);
    if (copy == NULL) return -5;
    if (!lvgl_port_lock(0)) { free(copy); return -3; }
    int result = -1;
    if (node < MICRO_UI_MAX_NODES && objects[node] != NULL &&
        lv_obj_check_type(objects[node], &lv_textarea_class)) {
        lv_textarea_set_text(objects[node], copy);
        result = 0;
    }
    lvgl_port_unlock();
    free(copy);
    return result;
}

int micro_esp_ui_create_slider(uint32_t node, uint32_t parent,
                               double value, double min, double max,
                               uint32_t handler)
{
    if (!lvgl_port_lock(0)) return -3;
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *slider = lv_slider_create(parent_obj);
        if (slider == NULL) result = -4;
        else {
            lv_slider_set_range(slider, (int32_t)min, (int32_t)max);
            lv_slider_set_value(slider, (int32_t)value, LV_ANIM_OFF);
            lv_obj_set_width(slider, LV_PCT(100));
            if (handler == MICRO_UI_NO_HANDLER) {
                lv_obj_remove_flag(slider, LV_OBJ_FLAG_CLICKABLE);
            } else {
                click_contexts[node].handler = handler;
                lv_obj_add_event_cb(slider, slider_callback, LV_EVENT_VALUE_CHANGED,
                                    &click_contexts[node]);
            }
            objects[node] = slider;
            if (parent == MICRO_UI_NO_PARENT) app_root = slider;
        }
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_set_slider_value(uint32_t node, double value)
{
    if (!lvgl_port_lock(0)) return -3;
    int result = -1;
    if (node < MICRO_UI_MAX_NODES && objects[node] != NULL &&
        lv_obj_check_type(objects[node], &lv_slider_class)) {
        lv_slider_set_value(objects[node], (int32_t)value, LV_ANIM_OFF);
        result = 0;
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_destroy_app_root(void)
{
    if (!lvgl_port_lock(0)) return -3;
    if (s_keyboard != NULL) {
        lv_keyboard_set_textarea(s_keyboard, NULL);
        lv_obj_delete(s_keyboard);
        s_keyboard = NULL;
    }
    if (s_candidate_bar != NULL) {
        lv_obj_delete(s_candidate_bar);
        s_candidate_bar = NULL;
        s_pinyin_label = NULL;
    }
    if (s_ime_toggle != NULL) {
        lv_obj_delete(s_ime_toggle);
        s_ime_toggle = NULL;
    }
    s_target_ta = NULL;
    s_ime_active = false;
    s_candidate_shown = 0;
    if (app_root != NULL) lv_obj_delete(app_root);
    app_root = NULL;
    memset(objects, 0, sizeof objects);
    memset(text_targets, 0, sizeof text_targets);
    activation_read = 0;
    activation_write = 0;
    input_read = 0;
    input_write = 0;
    slider_read = 0;
    slider_write = 0;
    lvgl_port_unlock();
    return 0;
}

int micro_esp_ui_take_input_change(uint32_t *handler_id, uint8_t *text,
                                   size_t text_capacity, size_t *text_len)
{
    if (handler_id == NULL || text == NULL || text_len == NULL || text_capacity == 0) {
        return -1;
    }
    if (!lvgl_port_lock(0)) return -3;
    if (input_read == input_write) {
        lvgl_port_unlock();
        return 0;
    }
    *handler_id = input_changes[input_read].handler;
    size_t len = input_changes[input_read].len;
    if (len > text_capacity) len = text_capacity;
    memcpy(text, input_changes[input_read].text, len);
    *text_len = len;
    input_read = (input_read + 1U) % MICRO_UI_INPUT_CAPACITY;
    lvgl_port_unlock();
    return 1;
}

int micro_esp_ui_take_slider_change(uint32_t *handler_id, double *value)
{
    if (handler_id == NULL || value == NULL) {
        return -1;
    }
    if (!lvgl_port_lock(0)) return -3;
    if (slider_read == slider_write) {
        lvgl_port_unlock();
        return 0;
    }
    *handler_id = slider_changes[slider_read].handler;
    *value = slider_changes[slider_read].value;
    slider_read = (slider_read + 1U) % MICRO_UI_SLIDER_CAPACITY;
    lvgl_port_unlock();
    return 1;
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
