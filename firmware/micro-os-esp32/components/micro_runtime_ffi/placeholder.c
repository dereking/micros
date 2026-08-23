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
#define MICRO_UI_CHECKBOX_CAPACITY 32U
#define MICRO_UI_DROPDOWN_CAPACITY 32U
#define MICRO_UI_ROLLER_CAPACITY 32U

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

struct micro_checkbox_change {
    uint32_t handler;
    int checked;
};

struct micro_dropdown_change {
    uint32_t handler;
    double index;
};

struct micro_roller_change {
    uint32_t handler;
    double index;
};

/* Per-child LTRB anchor offsets. `mask` bit0=left, bit1=top, bit2=right,
 * bit3=bottom; a set edge pins the child's side to the parent's corresponding
 * side at that offset. mask == 0 means no spec (default: top dock, full
 * width). */
struct micro_layout_spec {
    uint8_t mask;
    double left;
    double top;
    double right;
    double bottom;
};

static lv_obj_t *objects[MICRO_UI_MAX_NODES];
static lv_obj_t *text_targets[MICRO_UI_MAX_NODES];
static lv_obj_t *needles[MICRO_UI_MAX_NODES];
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
static struct micro_checkbox_change checkbox_changes[MICRO_UI_CHECKBOX_CAPACITY];
static unsigned checkbox_read;
static unsigned checkbox_write;
static struct micro_dropdown_change dropdown_changes[MICRO_UI_DROPDOWN_CAPACITY];
static unsigned dropdown_read;
static unsigned dropdown_write;
static struct micro_roller_change roller_changes[MICRO_UI_ROLLER_CAPACITY];
static struct micro_layout_spec layout_specs[MICRO_UI_MAX_NODES];
static unsigned roller_read;
static unsigned roller_write;
static lv_obj_t *s_keyboard;
static lv_obj_t *s_candidate_bar;
static lv_obj_t *s_pinyin_label;
static lv_obj_t *s_ime_toggle;
static lv_obj_t *s_target_ta;
static bool s_ime_active;
static lv_obj_t *s_tabview;
static lv_obj_t *s_tab_target;
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
    (void)event;
    micro_esp_ui_show_keyboard(textarea);
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

static void checkbox_callback(lv_event_t *event)
{
    const struct micro_click_context *context = lv_event_get_user_data(event);
    lv_obj_t *target = lv_event_get_target(event);
    if (target == NULL) {
        return;
    }
    unsigned next = (checkbox_write + 1U) % MICRO_UI_CHECKBOX_CAPACITY;
    if (next == checkbox_read) {
        ESP_LOGW("micro_ui", "checkbox queue full; dropping handler %lu",
                 (unsigned long)context->handler);
        return;
    }
    checkbox_changes[checkbox_write].handler = context->handler;
    checkbox_changes[checkbox_write].checked =
        lv_obj_has_state(target, LV_STATE_CHECKED) ? 1 : 0;
    checkbox_write = next;
}

static void dropdown_callback(lv_event_t *event)
{
    const struct micro_click_context *context = lv_event_get_user_data(event);
    lv_obj_t *target = lv_event_get_target(event);
    if (target == NULL) {
        return;
    }
    unsigned next = (dropdown_write + 1U) % MICRO_UI_DROPDOWN_CAPACITY;
    if (next == dropdown_read) {
        ESP_LOGW("micro_ui", "dropdown queue full; dropping handler %lu",
                 (unsigned long)context->handler);
        return;
    }
    dropdown_changes[dropdown_write].handler = context->handler;
    dropdown_changes[dropdown_write].index = (double)lv_dropdown_get_selected(target);
    dropdown_write = next;
}

static void roller_callback(lv_event_t *event)
{
    const struct micro_click_context *context = lv_event_get_user_data(event);
    lv_obj_t *target = lv_event_get_target(event);
    if (target == NULL) {
        return;
    }
    unsigned next = (roller_write + 1U) % MICRO_UI_ROLLER_CAPACITY;
    if (next == roller_read) {
        ESP_LOGW("micro_ui", "roller queue full; dropping handler %lu",
                 (unsigned long)context->handler);
        return;
    }
    roller_changes[roller_write].handler = context->handler;
    roller_changes[roller_write].index = (double)lv_roller_get_selected(target);
    roller_write = next;
}

static lv_obj_t *parent_object(uint32_t parent)
{
    if (parent == MICRO_UI_NO_PARENT) {
        return lv_screen_active();
    }
    if (parent >= MICRO_UI_MAX_NODES) {
        return NULL;
    }
    lv_obj_t *obj = objects[parent];
    /* Content nodes created right after create_tab_content(i) mount into the
     * i-th tab page instead of the tabview bar itself. */
    if (s_tab_target != NULL && obj != NULL && lv_obj_check_type(obj, &lv_tabview_class)) {
        return s_tab_target;
    }
    return obj;
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
            lv_obj_set_style_pad_top(column, 6, LV_PART_MAIN);
            lv_obj_set_style_pad_bottom(column, 6, LV_PART_MAIN);
            lv_obj_set_style_pad_row(column, 6, LV_PART_MAIN);
            /* Blend into the parent so the layout container shows no visible
             * box or border, yet stays OPAQUE (matching the parent's color).
             * A fully transparent container changes LVGL's draw path and made
             * the top rows intermittently vanish on the display tab. */
            lv_obj_set_style_radius(column, 0, LV_PART_MAIN);
            lv_obj_set_style_border_width(column, 0, LV_PART_MAIN);
            lv_obj_set_style_bg_color(column,
                                      lv_obj_get_style_bg_color(parent_obj, LV_PART_MAIN),
                                      LV_PART_MAIN);
            lv_obj_set_style_bg_opa(column, LV_OPA_COVER, LV_PART_MAIN);
            /* The tab page owns scrolling; a column is a plain content stack. */
            lv_obj_remove_flag(column, LV_OBJ_FLAG_SCROLLABLE);
            lv_obj_set_scrollbar_mode(column, LV_SCROLLBAR_MODE_OFF);
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
            lv_obj_set_style_radius(row, 0, LV_PART_MAIN);
            lv_obj_set_style_border_width(row, 0, LV_PART_MAIN);
            lv_obj_set_style_bg_color(row,
                                      lv_obj_get_style_bg_color(parent_obj, LV_PART_MAIN),
                                      LV_PART_MAIN);
            lv_obj_set_style_bg_opa(row, LV_OPA_COVER, LV_PART_MAIN);
            objects[node] = row;
            if (parent == MICRO_UI_NO_PARENT) app_root = row;
        }
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_create_list(uint32_t node, uint32_t parent)
{
    if (!lvgl_port_lock(0)) return -3;
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *list = lv_list_create(parent_obj);
        if (list == NULL) result = -4;
        else {
            lv_obj_set_size(list, LV_PCT(100), LV_SIZE_CONTENT);
            lv_obj_set_style_pad_row(list, 4, LV_PART_MAIN);
            /* Borderless and blended into the parent so it matches the app
             * background exactly; the items themselves stand out. */
            lv_obj_set_style_border_width(list, 0, LV_PART_MAIN);
            lv_obj_set_style_bg_color(list,
                                      lv_obj_get_style_bg_color(parent_obj, LV_PART_MAIN),
                                      LV_PART_MAIN);
            lv_obj_set_style_bg_opa(list, LV_OPA_COVER, LV_PART_MAIN);
            /* Let the outer column own scrolling. */
            lv_obj_set_scrollbar_mode(list, LV_SCROLLBAR_MODE_OFF);
            lv_obj_remove_flag(list, LV_OBJ_FLAG_SCROLLABLE);
            objects[node] = list;
            if (parent == MICRO_UI_NO_PARENT) app_root = list;
        }
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_create_tabview(uint32_t node, uint32_t parent,
                                 const uint8_t *titles, size_t titles_len)
{
    char *copy = copy_text(titles, titles_len);
    if (copy == NULL) return -5;
    if (!lvgl_port_lock(0)) { free(copy); return -3; }
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *tabview = lv_tabview_create(parent_obj);
        if (tabview == NULL) result = -4;
        else {
            /* As the root (parent == screen) fill the display; as a child of
             * a scrollable column keep a bounded height so the column can
             * scroll past it. */
            lv_obj_set_size(tabview, LV_PCT(100),
                            parent == MICRO_UI_NO_PARENT ? LV_PCT(100) : 280);
            /* Titles arrive '\n'-joined; add one tab per title. */
            char *cursor = copy;
            char *save = NULL;
            for (char *token = strtok_r(cursor, "\n", &save); token != NULL;
                 token = strtok_r(NULL, "\n", &save)) {
                lv_tabview_add_tab(tabview, token);
            }
            /* One uniform light background for the whole app: screen, tab bar,
             * content panel and every page. Columns/rows blend into their
             * parent, so the whole tab body reads as a single consistent
             * surface instead of a patchwork of theme shades. */
            lv_color_t app_bg = lv_color_hex(0xF2F4F0);
            if (parent == MICRO_UI_NO_PARENT) {
                lv_obj_set_style_bg_color(lv_screen_active(), app_bg, LV_PART_MAIN);
                lv_obj_set_style_bg_opa(lv_screen_active(), LV_OPA_COVER, LV_PART_MAIN);
            }
            lv_obj_set_style_bg_color(tabview, app_bg, LV_PART_MAIN);
            lv_obj_set_style_bg_opa(tabview, LV_OPA_COVER, LV_PART_MAIN);
            lv_obj_set_style_pad_all(tabview, 0, LV_PART_MAIN);

            lv_obj_t *tab_content = lv_tabview_get_content(tabview);
            lv_obj_set_style_border_width(tab_content, 0, LV_PART_MAIN);
            lv_obj_set_style_pad_all(tab_content, 0, LV_PART_MAIN);
            lv_obj_set_style_bg_color(tab_content, app_bg, LV_PART_MAIN);
            lv_obj_set_style_bg_opa(tab_content, LV_OPA_COVER, LV_PART_MAIN);

            lv_obj_t *tab_bar = lv_tabview_get_tab_bar(tabview);
            /* A thin bottom divider separates the tab bar from the content. */
            lv_obj_set_style_border_width(tab_bar, 1, LV_PART_MAIN);
            lv_obj_set_style_border_side(tab_bar, LV_BORDER_SIDE_BOTTOM, LV_PART_MAIN);
            lv_obj_set_style_border_color(tab_bar, lv_color_hex(0xD5D9CE), LV_PART_MAIN);
            lv_obj_set_style_bg_color(tab_bar, app_bg, LV_PART_MAIN);
            lv_obj_set_style_bg_opa(tab_bar, LV_OPA_COVER, LV_PART_MAIN);

            uint32_t page_count = tab_content == NULL ? 0 : lv_obj_get_child_count(tab_content);
            for (uint32_t pi = 0; pi < page_count; ++pi) {
                lv_obj_t *page = lv_obj_get_child(tab_content, pi);
                lv_obj_set_style_border_width(page, 0, LV_PART_MAIN);
                /* No page-level padding: the columns' own padding is the only
                 * content inset. */
                lv_obj_set_style_pad_all(page, 0, LV_PART_MAIN);
                lv_obj_set_style_bg_color(page, app_bg, LV_PART_MAIN);
                lv_obj_set_style_bg_opa(page, LV_OPA_COVER, LV_PART_MAIN);
                /* Scrollbar flush with the page edge (the theme leaves a gap)
                 * and on the same surface. */
                lv_obj_set_style_pad_all(page, 0, LV_PART_SCROLLBAR);
                lv_obj_set_style_bg_color(page, lv_color_hex(0xB8BFB4), LV_PART_SCROLLBAR);
                lv_obj_set_style_bg_opa(page, LV_OPA_COVER, LV_PART_SCROLLBAR);
                lv_obj_set_scrollbar_mode(page, LV_SCROLLBAR_MODE_AUTO);
            }
            s_tabview = tabview;
            s_tab_target = NULL;
            objects[node] = tabview;
            if (parent == MICRO_UI_NO_PARENT) app_root = tabview;
        }
    }
    lvgl_port_unlock();
    free(copy);
    return result;
}

int micro_esp_ui_create_tab_content(uint32_t index)
{
    if (!lvgl_port_lock(0)) return -3;
    int result = -1;
    if (s_tabview != NULL) {
        lv_obj_t *content = lv_tabview_get_content(s_tabview);
        if (content != NULL) {
            lv_obj_t *page = lv_obj_get_child(content, index);
            if (page != NULL) {
                s_tab_target = page;
                result = 0;
            }
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
                /* Fixed width so the row layout never re-solves (flex_grow
                 * here made the row relayout and the -/+ buttons flicker). */
                lv_obj_set_size(bar, 120, 12);
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
        lv_obj_t *button = NULL;
        lv_obj_t *label = NULL;
        if (parent_obj != NULL && lv_obj_check_type(parent_obj, &lv_list_class)) {
            /* Row inside a ui.list container. */
            button = lv_list_add_button(parent_obj, NULL, copy);
            label = button == NULL ? NULL : lv_obj_get_child(button, 0);
        } else {
            button = lv_button_create(parent_obj);
            label = button == NULL ? NULL : lv_label_create(button);
        }
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

int micro_esp_ui_create_checkbox(uint32_t node, uint32_t parent,
                                 const uint8_t *label, size_t label_len,
                                 int checked, uint32_t handler)
{
    char *copy = copy_text(label, label_len);
    if (copy == NULL) return -5;
    if (!lvgl_port_lock(0)) { free(copy); return -3; }
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *checkbox = lv_checkbox_create(parent_obj);
        if (checkbox == NULL) result = -4;
        else {
            lv_checkbox_set_text(checkbox, copy);
            lv_obj_set_style_text_font(checkbox, &micro_ui_sans_24, LV_PART_MAIN);
            lv_obj_set_style_text_color(checkbox, lv_color_hex(0x101820), LV_PART_MAIN);
            if (checked) {
                lv_obj_add_state(checkbox, LV_STATE_CHECKED);
            }
            if (handler == MICRO_UI_NO_HANDLER) {
                lv_obj_remove_flag(checkbox, LV_OBJ_FLAG_CLICKABLE);
            } else {
                click_contexts[node].handler = handler;
                lv_obj_add_event_cb(checkbox, checkbox_callback, LV_EVENT_VALUE_CHANGED,
                                    &click_contexts[node]);
            }
            objects[node] = checkbox;
            if (parent == MICRO_UI_NO_PARENT) app_root = checkbox;
        }
    }
    lvgl_port_unlock();
    free(copy);
    return result;
}

int micro_esp_ui_create_dropdown(uint32_t node, uint32_t parent,
                                 const uint8_t *options, size_t options_len,
                                 double index, uint32_t handler)
{
    char *copy = copy_text(options, options_len);
    if (copy == NULL) return -5;
    if (!lvgl_port_lock(0)) { free(copy); return -3; }
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *dropdown = lv_dropdown_create(parent_obj);
        if (dropdown == NULL) result = -4;
        else {
            lv_dropdown_set_options(dropdown, copy);
            lv_dropdown_set_selected(dropdown, (uint32_t)index);
            lv_obj_set_width(dropdown, LV_PCT(100));
            if (handler == MICRO_UI_NO_HANDLER) {
                lv_obj_remove_flag(dropdown, LV_OBJ_FLAG_CLICKABLE);
            } else {
                click_contexts[node].handler = handler;
                lv_obj_add_event_cb(dropdown, dropdown_callback, LV_EVENT_READY,
                                    &click_contexts[node]);
            }
            objects[node] = dropdown;
            if (parent == MICRO_UI_NO_PARENT) app_root = dropdown;
        }
    }
    lvgl_port_unlock();
    free(copy);
    return result;
}

static uint32_t s_delphi_layout_id = LV_LAYOUT_NONE;

static void delphi_layout_update_cb(lv_obj_t *container, void *user_data)
{
    (void)user_data;
    lv_coord_t avail_w = lv_obj_get_content_width(container);
    lv_coord_t avail_h = lv_obj_get_content_height(container);
    lv_coord_t row_gap = lv_obj_get_style_pad_row(container, LV_PART_MAIN);
    uint32_t count = lv_obj_get_child_count(container);
    lv_coord_t top_y = 0;
    lv_coord_t bottom_y = avail_h;
    lv_coord_t bottom_stack_top = avail_h;

    /* Pass 1 — stack top/bottom docked children; vertical fills are deferred
     * so their height is not computed before the stacks are known. */
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = (node < MICRO_UI_MAX_NODES) ? layout_specs[node].mask : 0;
        lv_obj_update_layout(child);
        lv_coord_t w = lv_obj_get_width(child);
        lv_coord_t h = lv_obj_get_height(child);
        /* Horizontal role: left+right stretch, one pin, else full width. */
        lv_coord_t x;
        if ((mask & 1) && (mask & 4)) {
            x = (lv_coord_t)layout_specs[node].left;
            lv_obj_set_width(child, avail_w - (lv_coord_t)layout_specs[node].left
                                       - (lv_coord_t)layout_specs[node].right);
        } else if (mask & 1) {
            x = (lv_coord_t)layout_specs[node].left;
            lv_obj_set_width(child, w);
        } else if (mask & 4) {
            lv_obj_set_width(child, w);
            x = avail_w - w - (lv_coord_t)layout_specs[node].right;
        } else {
            x = 0;
            lv_obj_set_width(child, avail_w);
        }
        /* Vertical role: top+bottom fill (pass 2), bottom dock, else top. */
        if ((mask & 2) && (mask & 8)) {
            continue;
        }
        if (mask & 8) {
            /* Bottom dock: the child's bottom edge stays `bottom` above the
             * parent's bottom edge, whatever the parent's size. */
            bottom_y -= h;
            lv_coord_t y = bottom_y - (lv_coord_t)layout_specs[node].bottom;
            lv_obj_set_pos(child, x, y);
            bottom_y = y - row_gap;
            if (y < bottom_stack_top) bottom_stack_top = y;
        } else {
            /* Top dock (default). */
            lv_coord_t y = top_y
                           + ((mask & 2) ? (lv_coord_t)layout_specs[node].top : 0);
            lv_obj_set_pos(child, x, y);
            top_y = y + h + row_gap;
        }
    }

    /* Pass 2 — vertical fills span the space between the two stacks. */
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = (node < MICRO_UI_MAX_NODES) ? layout_specs[node].mask : 0;
        if (!((mask & 2) && (mask & 8))) continue;
        lv_coord_t h = bottom_stack_top - top_y;
        if (h < 0) h = 0;
        lv_coord_t w = lv_obj_get_width(child);
        lv_coord_t x;
        if ((mask & 1) && (mask & 4)) {
            x = (lv_coord_t)layout_specs[node].left;
            lv_obj_set_width(child, avail_w - (lv_coord_t)layout_specs[node].left
                                       - (lv_coord_t)layout_specs[node].right);
        } else if (mask & 1) {
            x = (lv_coord_t)layout_specs[node].left;
            lv_obj_set_width(child, w);
        } else if (mask & 4) {
            lv_obj_set_width(child, w);
            x = avail_w - w - (lv_coord_t)layout_specs[node].right;
        } else {
            x = 0;
            lv_obj_set_width(child, avail_w);
        }
        lv_obj_set_pos(child, x, top_y);
        lv_obj_set_height(child, h);
    }
}

int micro_esp_ui_set_layout_spec(uint32_t node, uint32_t mask,
                                 double left, double top,
                                 double right, double bottom)
{
    if (node >= MICRO_UI_MAX_NODES) return -1;
    layout_specs[node].mask = (uint8_t)mask;
    layout_specs[node].left = left;
    layout_specs[node].top = top;
    layout_specs[node].right = right;
    layout_specs[node].bottom = bottom;
    /* Tag the object with its node id so the layout callback can look up its
     * spec. Object user_data does not collide with event-callback user_data. */
    if (objects[node] != NULL) {
        lv_obj_set_user_data(objects[node], (void *)(uintptr_t)node);
    }
    return 0;
}

static bool delphi_get_min_size_cb(lv_obj_t *container, int32_t *req_size,
                                    bool width, void *user_data)
{
    (void)user_data;
    if (width) {
        *req_size = lv_obj_get_content_width(container);
        return true;
    }
    /* Required height = top stack + bottom stack, plus the padding and the
     * row gaps the layout callback applies, so a content-sized container is
     * exactly as tall as its positioned children. Vertical-fill children
     * (top+bottom anchored) take the space between the stacks and impose a
     * natural-height floor so e.g. a left/right docked bar does not collapse.
     * When content overflows the page, the container grows and the scrollable
     * tab page can scroll to reveal the docked bottom. */
    int32_t top_extent = 0;
    int32_t bottom_extent = 0;
    int32_t fill_floor = 0;
    int32_t top_count = 0;
    int32_t bottom_count = 0;
    lv_coord_t row_gap = lv_obj_get_style_pad_row(container, LV_PART_MAIN);
    uint32_t count = lv_obj_get_child_count(container);
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = (node < MICRO_UI_MAX_NODES) ? layout_specs[node].mask : 0;
        int32_t h = lv_obj_get_height(child);
        if ((mask & 2) && (mask & 8)) {
            /* Vertical fill: spans the space between the stacks. */
            if (h > fill_floor) fill_floor = h;
            continue;
        }
        if (mask & 8) {
            /* Bottom dock: reserves its height plus the bottom offset. */
            bottom_extent += h + (int32_t)layout_specs[node].bottom;
            bottom_count++;
        } else {
            /* Top dock (default): reserves its height plus the top offset. */
            top_extent += h + ((mask & 2) ? (int32_t)layout_specs[node].top : 0);
            top_count++;
        }
    }
    int32_t gaps = (top_count ? top_count - 1 : 0)
                 + (bottom_count ? bottom_count - 1 : 0)
                 + (top_count && bottom_count ? 1 : 0);
    int32_t content = top_extent + bottom_extent + gaps * row_gap;
    if (fill_floor > content) content = fill_floor;
    *req_size = content
              + lv_obj_get_style_space_top(container, LV_PART_MAIN)
              + lv_obj_get_style_space_bottom(container, LV_PART_MAIN);
    return true;
}

/* Required height = top stack + bottom stack, plus padding and the row gaps
 * the layout callback applies. Runs after a synchronous layout so every
 * child's natural height is resolved, making the container height
 * deterministic instead of depending on min-size callback timing. */
static lv_coord_t delphi_content_height(lv_obj_t *container)
{
    int32_t top_extent = 0;
    int32_t bottom_extent = 0;
    int32_t fill_floor = 0;
    int32_t top_count = 0;
    int32_t bottom_count = 0;
    lv_coord_t row_gap = lv_obj_get_style_pad_row(container, LV_PART_MAIN);
    uint32_t count = lv_obj_get_child_count(container);
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = (node < MICRO_UI_MAX_NODES) ? layout_specs[node].mask : 0;
        int32_t h = lv_obj_get_height(child);
        if ((mask & 2) && (mask & 8)) {
            if (h > fill_floor) fill_floor = h;
            continue;
        }
        if (mask & 8) {
            bottom_extent += h + (int32_t)layout_specs[node].bottom;
            bottom_count++;
        } else {
            top_extent += h + ((mask & 2) ? (int32_t)layout_specs[node].top : 0);
            top_count++;
        }
    }
    int32_t gaps = (top_count ? top_count - 1 : 0)
                 + (bottom_count ? bottom_count - 1 : 0)
                 + (top_count && bottom_count ? 1 : 0);
    int32_t content = top_extent + bottom_extent + gaps * row_gap;
    if (fill_floor > content) content = fill_floor;
    return (lv_coord_t)(content
                        + lv_obj_get_style_space_top(container, LV_PART_MAIN)
                        + lv_obj_get_style_space_bottom(container, LV_PART_MAIN));
}

int micro_esp_ui_apply_delphi_layout(uint32_t container,
                                     const uint32_t *child_ids, uint32_t child_count)
{
    (void)child_ids;
    (void)child_count;
    if (!lvgl_port_lock(0)) return -3;
    if (container >= MICRO_UI_MAX_NODES || objects[container] == NULL) {
        lvgl_port_unlock();
        return -1;
    }
    lv_obj_t *obj = objects[container];
    if (s_delphi_layout_id == LV_LAYOUT_NONE) {
        lv_layout_callbacks_t callbacks = {
            .layout_update_cb = delphi_layout_update_cb,
            .get_min_size_cb = delphi_get_min_size_cb,
        };
        s_delphi_layout_id = lv_layout_create(callbacks, NULL);
    }
    lv_obj_set_layout(obj, s_delphi_layout_id);
    /* Resolve every child's natural size synchronously, then pin the container
     * to its exact content height. Computing the height explicitly (instead of
     * relying on the min-size callback on the first refresh) avoids a
     * timing-dependent collapse where the top rows read as 0-height and get
     * clipped, leaving only the docked list visible. */
    lv_obj_update_layout(obj);
    lv_obj_set_height(obj, delphi_content_height(obj));
    lvgl_port_unlock();
    return 0;
}

int micro_esp_ui_create_led(uint32_t node, uint32_t parent, int on)
{
    if (!lvgl_port_lock(0)) return -3;
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *led = lv_led_create(parent_obj);
        if (led == NULL) result = -4;
        else {
            lv_led_set_brightness(led, on ? LV_LED_BRIGHT_MAX : LV_LED_BRIGHT_MIN);
            objects[node] = led;
            if (parent == MICRO_UI_NO_PARENT) app_root = led;
        }
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_set_led(uint32_t node, int on)
{
    if (!lvgl_port_lock(0)) return -3;
    int result = -1;
    if (node < MICRO_UI_MAX_NODES && objects[node] != NULL &&
        lv_obj_check_type(objects[node], &lv_led_class)) {
        lv_led_set_brightness(objects[node], on ? LV_LED_BRIGHT_MAX : LV_LED_BRIGHT_MIN);
        result = 0;
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_create_spinner(uint32_t node, uint32_t parent, int active)
{
    if (!lvgl_port_lock(0)) return -3;
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *spinner = lv_spinner_create(parent_obj);
        if (spinner == NULL) result = -4;
        else {
            lv_obj_set_size(spinner, 48, 48);
            if (!active) {
                lv_obj_add_flag(spinner, LV_OBJ_FLAG_HIDDEN);
                /* lv_spinner_create starts an infinite arc animation that keeps
                 * invalidating even when hidden. Stop it so an idle spinner
                 * cannot drive a periodic redraw/flicker. */
                lv_anim_del(spinner, NULL);
            }
            objects[node] = spinner;
            if (parent == MICRO_UI_NO_PARENT) app_root = spinner;
        }
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_set_spinner(uint32_t node, int active)
{
    if (!lvgl_port_lock(0)) return -3;
    int result = -1;
    if (node < MICRO_UI_MAX_NODES && objects[node] != NULL &&
        lv_obj_check_type(objects[node], &lv_spinner_class)) {
        if (active) {
            lv_obj_clear_flag(objects[node], LV_OBJ_FLAG_HIDDEN);
            /* Recreate the spinner arc animation after it was stopped. */
            lv_spinner_set_anim_params(objects[node], 1000, 300);
        } else {
            lv_obj_add_flag(objects[node], LV_OBJ_FLAG_HIDDEN);
            lv_anim_del(objects[node], NULL);
        }
        result = 0;
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_create_scale(uint32_t node, uint32_t parent,
                              double value, double min, double max)
{
    if (!lvgl_port_lock(0)) return -3;
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *scale = lv_scale_create(parent_obj);
        if (scale == NULL) result = -4;
        else {
            lv_scale_set_mode(scale, LV_SCALE_MODE_ROUND_INNER);
            lv_scale_set_range(scale, (int32_t)min, (int32_t)max);
            lv_scale_set_angle_range(scale, 270);
            lv_scale_set_rotation(scale, 135);
            lv_scale_set_total_tick_count(scale, 11);
            lv_scale_set_major_tick_every(scale, 5);
            lv_scale_set_label_show(scale, true);
            lv_obj_set_size(scale, 120, 120);
            /* No padding: the needle pivot uses outer width/2, which must equal
             * the gauge center. */
            lv_obj_set_style_pad_all(scale, 0, LV_PART_MAIN);
            lv_obj_t *needle = lv_line_create(scale);
            lv_point_precise_t points[2] = {{0, 0}, {0, -40}};
            lv_line_set_points(needle, points, 2);
            lv_obj_set_style_line_width(needle, 3, LV_PART_MAIN);
            lv_obj_set_style_line_color(needle, lv_color_hex(0x101820), LV_PART_MAIN);
            lv_scale_set_line_needle_value(scale, needle, 40, (int32_t)value);
            needles[node] = needle;
            objects[node] = scale;
            if (parent == MICRO_UI_NO_PARENT) app_root = scale;
        }
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_set_scale_value(uint32_t node, double value)
{
    if (!lvgl_port_lock(0)) return -3;
    int result = -1;
    if (node < MICRO_UI_MAX_NODES && objects[node] != NULL &&
        lv_obj_check_type(objects[node], &lv_scale_class) && needles[node] != NULL) {
        lv_scale_set_line_needle_value(objects[node], needles[node], 40, (int32_t)value);
        result = 0;
    }
    lvgl_port_unlock();
    return result;
}

int micro_esp_ui_create_roller(uint32_t node, uint32_t parent,
                                 const uint8_t *options, size_t options_len,
                                 double index, uint32_t handler)
{
    char *copy = copy_text(options, options_len);
    if (copy == NULL) return -5;
    if (!lvgl_port_lock(0)) { free(copy); return -3; }
    lv_obj_t *parent_obj;
    int result = begin_create(node, parent, &parent_obj);
    if (result == 0) {
        lv_obj_t *roller = lv_roller_create(parent_obj);
        if (roller == NULL) result = -4;
        else {
            lv_roller_set_options(roller, copy, LV_ROLLER_MODE_NORMAL);
            lv_roller_set_selected(roller, (uint32_t)index, LV_ANIM_OFF);
            lv_obj_set_width(roller, LV_PCT(100));
            if (handler == MICRO_UI_NO_HANDLER) {
                lv_obj_remove_flag(roller, LV_OBJ_FLAG_CLICKABLE);
            } else {
                click_contexts[node].handler = handler;
                lv_obj_add_event_cb(roller, roller_callback, LV_EVENT_VALUE_CHANGED,
                                    &click_contexts[node]);
            }
            objects[node] = roller;
            if (parent == MICRO_UI_NO_PARENT) app_root = roller;
        }
    }
    lvgl_port_unlock();
    free(copy);
    return result;
}

int micro_esp_ui_set_selection_value(uint32_t node, double index)
{
    if (!lvgl_port_lock(0)) return -3;
    int result = -1;
    if (node < MICRO_UI_MAX_NODES && objects[node] != NULL) {
        if (lv_obj_check_type(objects[node], &lv_dropdown_class)) {
            lv_dropdown_set_selected(objects[node], (uint32_t)index);
            result = 0;
        } else if (lv_obj_check_type(objects[node], &lv_roller_class)) {
            lv_roller_set_selected(objects[node], (uint32_t)index, LV_ANIM_OFF);
            result = 0;
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
    s_tabview = NULL;
    s_tab_target = NULL;
    s_candidate_shown = 0;
    if (app_root != NULL) lv_obj_delete(app_root);
    app_root = NULL;
    memset(objects, 0, sizeof objects);
    memset(text_targets, 0, sizeof text_targets);
    memset(needles, 0, sizeof needles);
    memset(layout_specs, 0, sizeof layout_specs);
    activation_read = 0;
    activation_write = 0;
    input_read = 0;
    input_write = 0;
    slider_read = 0;
    slider_write = 0;
    checkbox_read = 0;
    checkbox_write = 0;
    dropdown_read = 0;
    dropdown_write = 0;
    roller_read = 0;
    roller_write = 0;
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

int micro_esp_ui_take_checkbox_change(uint32_t *handler_id, int *checked)
{
    if (handler_id == NULL || checked == NULL) {
        return -1;
    }
    if (!lvgl_port_lock(0)) return -3;
    if (checkbox_read == checkbox_write) {
        lvgl_port_unlock();
        return 0;
    }
    *handler_id = checkbox_changes[checkbox_read].handler;
    *checked = checkbox_changes[checkbox_read].checked;
    checkbox_read = (checkbox_read + 1U) % MICRO_UI_CHECKBOX_CAPACITY;
    lvgl_port_unlock();
    return 1;
}

int micro_esp_ui_take_dropdown_change(uint32_t *handler_id, double *index)
{
    if (handler_id == NULL || index == NULL) {
        return -1;
    }
    if (!lvgl_port_lock(0)) return -3;
    if (dropdown_read == dropdown_write) {
        lvgl_port_unlock();
        return 0;
    }
    *handler_id = dropdown_changes[dropdown_read].handler;
    *index = dropdown_changes[dropdown_read].index;
    dropdown_read = (dropdown_read + 1U) % MICRO_UI_DROPDOWN_CAPACITY;
    lvgl_port_unlock();
    return 1;
}

int micro_esp_ui_take_roller_change(uint32_t *handler_id, double *index)
{
    if (handler_id == NULL || index == NULL) {
        return -1;
    }
    if (!lvgl_port_lock(0)) return -3;
    if (roller_read == roller_write) {
        lvgl_port_unlock();
        return 0;
    }
    *handler_id = roller_changes[roller_read].handler;
    *index = roller_changes[roller_read].index;
    roller_read = (roller_read + 1U) % MICRO_UI_ROLLER_CAPACITY;
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
