#include "micro_native.h"

#include <SDL3/SDL.h>
#include <lvgl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MICRO_MAX_NODES 256U
#define MICRO_EVENT_CAPACITY 64U
#define MICRO_NO_PARENT UINT32_MAX
#define MICRO_NO_HANDLER UINT32_MAX
#define MICRO_INPUT_CAPACITY 16U
#define MICRO_INPUT_TEXT_MAX 256U
#define MICRO_SLIDER_CAPACITY 32U
#define MICRO_CHECKBOX_CAPACITY 32U
#define MICRO_DROPDOWN_CAPACITY 32U
#define MICRO_ROLLER_CAPACITY 32U

typedef struct micro_click_context {
    struct micro_native *native;
    uint32_t handler_id;
} micro_click_context_t;

typedef struct micro_input_change {
    uint32_t handler_id;
    size_t len;
    char text[MICRO_INPUT_TEXT_MAX];
} micro_input_change_t;

typedef struct micro_slider_change {
    uint32_t handler_id;
    double value;
} micro_slider_change_t;

typedef struct micro_checkbox_change {
    uint32_t handler_id;
    int checked;
} micro_checkbox_change_t;

typedef struct micro_dropdown_change {
    uint32_t handler_id;
    double index;
} micro_dropdown_change_t;

typedef struct micro_layout_spec {
    uint8_t mask; /* bit0=left, bit1=top, bit2=right, bit3=bottom */
    double left;
    double top;
    double right;
    double bottom;
} micro_layout_spec_t;

typedef struct micro_roller_change {
    uint32_t handler_id;
    double index;
} micro_roller_change_t;

struct micro_native {
    SDL_Window *window;
    SDL_Renderer *renderer;
    SDL_Texture *texture;
    lv_display_t *display;
    lv_indev_t *pointer;
    uint8_t *display_buffer;
    size_t display_buffer_size;
    lv_point_t pointer_position;
    bool pointer_pressed;
    bool quit;
    lv_obj_t *objects[MICRO_MAX_NODES];
    lv_obj_t *text_targets[MICRO_MAX_NODES];
    lv_obj_t *needles[MICRO_MAX_NODES];
    lv_obj_t *tabview;
    lv_obj_t *tab_target;
    micro_click_context_t clicks[MICRO_MAX_NODES];
    uint32_t activations[MICRO_EVENT_CAPACITY];
    unsigned activation_read;
    unsigned activation_write;
    micro_input_change_t input_changes[MICRO_INPUT_CAPACITY];
    unsigned input_read;
    unsigned input_write;
    micro_slider_change_t slider_changes[MICRO_SLIDER_CAPACITY];
    unsigned slider_read;
    unsigned slider_write;
    micro_checkbox_change_t checkbox_changes[MICRO_CHECKBOX_CAPACITY];
    unsigned checkbox_read;
    unsigned checkbox_write;
    micro_dropdown_change_t dropdown_changes[MICRO_DROPDOWN_CAPACITY];
    unsigned dropdown_read;
    unsigned dropdown_write;
    micro_roller_change_t roller_changes[MICRO_ROLLER_CAPACITY];
    unsigned roller_read;
    unsigned roller_write;
    micro_layout_spec_t layout_specs[MICRO_MAX_NODES];
};

static void copy_error(char *target, size_t length, const char *message) {
    if (target == NULL || length == 0U) return;
    snprintf(target, length, "%s", message == NULL ? "unknown native error" : message);
}

static uint32_t tick_callback(void) {
    return (uint32_t)SDL_GetTicks();
}

static void flush_callback(lv_display_t *display, const lv_area_t *area, uint8_t *pixels) {
    micro_native_t *native = lv_display_get_user_data(display);
    SDL_Rect rectangle = {
        .x = area->x1,
        .y = area->y1,
        .w = area->x2 - area->x1 + 1,
        .h = area->y2 - area->y1 + 1,
    };
    SDL_UpdateTexture(native->texture, &rectangle, pixels, rectangle.w * 4);
    SDL_RenderClear(native->renderer);
    SDL_RenderTexture(native->renderer, native->texture, NULL, NULL);
    SDL_RenderPresent(native->renderer);
    lv_display_flush_ready(display);
}

static void pointer_callback(lv_indev_t *input, lv_indev_data_t *data) {
    micro_native_t *native = lv_indev_get_user_data(input);
    data->point = native->pointer_position;
    data->state = native->pointer_pressed ? LV_INDEV_STATE_PRESSED : LV_INDEV_STATE_RELEASED;
}

static void enqueue_activation(micro_native_t *native, uint32_t handler_id) {
    unsigned next = (native->activation_write + 1U) % MICRO_EVENT_CAPACITY;
    if (next != native->activation_read) {
        native->activations[native->activation_write] = handler_id;
        native->activation_write = next;
    }
}

static void click_callback(lv_event_t *event) {
    micro_click_context_t *context = lv_event_get_user_data(event);
    enqueue_activation(context->native, context->handler_id);
}

static void input_callback(lv_event_t *event) {
    micro_click_context_t *context = lv_event_get_user_data(event);
    lv_obj_t *target = lv_event_get_target(event);
    if (target == NULL) return;
    const char *text = lv_textarea_get_text(target);
    if (text == NULL) return;
    size_t len = strlen(text);
    if (len > MICRO_INPUT_TEXT_MAX - 1U) len = MICRO_INPUT_TEXT_MAX - 1U;
    unsigned next = (context->native->input_write + 1U) % MICRO_INPUT_CAPACITY;
    if (next != context->native->input_read) {
        micro_input_change_t *change = &context->native->input_changes[context->native->input_write];
        change->handler_id = context->handler_id;
        change->len = len;
        memcpy(change->text, text, len);
        change->text[len] = '\0';
        context->native->input_write = next;
    }
}

static void slider_callback(lv_event_t *event) {
    micro_click_context_t *context = lv_event_get_user_data(event);
    lv_obj_t *target = lv_event_get_target(event);
    if (target == NULL) return;
    unsigned next = (context->native->slider_write + 1U) % MICRO_SLIDER_CAPACITY;
    if (next != context->native->slider_read) {
        micro_slider_change_t *change = &context->native->slider_changes[context->native->slider_write];
        change->handler_id = context->handler_id;
        change->value = (double)lv_slider_get_value(target);
        context->native->slider_write = next;
    }
}

static void dropdown_callback(lv_event_t *event) {
    micro_click_context_t *context = lv_event_get_user_data(event);
    lv_obj_t *target = lv_event_get_target(event);
    if (target == NULL) return;
    unsigned next = (context->native->dropdown_write + 1U) % MICRO_DROPDOWN_CAPACITY;
    if (next != context->native->dropdown_read) {
        micro_dropdown_change_t *change = &context->native->dropdown_changes[context->native->dropdown_write];
        change->handler_id = context->handler_id;
        change->index = (double)lv_dropdown_get_selected(target);
        context->native->dropdown_write = next;
    }
}

static void roller_callback(lv_event_t *event) {
    micro_click_context_t *context = lv_event_get_user_data(event);
    lv_obj_t *target = lv_event_get_target(event);
    if (target == NULL) return;
    unsigned next = (context->native->roller_write + 1U) % MICRO_ROLLER_CAPACITY;
    if (next != context->native->roller_read) {
        micro_roller_change_t *change = &context->native->roller_changes[context->native->roller_write];
        change->handler_id = context->handler_id;
        change->index = (double)lv_roller_get_selected(target);
        context->native->roller_write = next;
    }
}

static void checkbox_callback(lv_event_t *event) {
    micro_click_context_t *context = lv_event_get_user_data(event);
    lv_obj_t *target = lv_event_get_target(event);
    if (target == NULL) return;
    unsigned next = (context->native->checkbox_write + 1U) % MICRO_CHECKBOX_CAPACITY;
    if (next != context->native->checkbox_read) {
        micro_checkbox_change_t *change = &context->native->checkbox_changes[context->native->checkbox_write];
        change->handler_id = context->handler_id;
        change->checked = lv_obj_has_state(target, LV_STATE_CHECKED) ? 1 : 0;
        context->native->checkbox_write = next;
    }
}

static lv_obj_t *parent_object(micro_native_t *native, uint32_t parent_id) {
    if (parent_id == MICRO_NO_PARENT) return lv_screen_active();
    if (parent_id >= MICRO_MAX_NODES) return NULL;
    lv_obj_t *obj = native->objects[parent_id];
    if (native->tab_target != NULL && obj != NULL &&
        lv_obj_check_type(obj, &lv_tabview_class)) {
        return native->tab_target;
    }
    return obj;
}

static void apply_text_style(lv_obj_t *object, uintptr_t font_handle, uint32_t line_height_px) {
    if (font_handle == 0U) return;
    const lv_font_t *font = (const lv_font_t *)font_handle;
    lv_obj_set_style_text_font(object, font, LV_PART_MAIN);
    int32_t line_space = (int32_t)line_height_px - lv_font_get_line_height(font);
    lv_obj_set_style_text_line_space(object, line_space, LV_PART_MAIN);
}

micro_native_t *micro_native_create(int width, int height, int hidden, char *error, size_t error_length) {
    if (width <= 0 || height <= 0) {
        copy_error(error, error_length, "invalid window dimensions");
        return NULL;
    }
    if (hidden) SDL_SetHint(SDL_HINT_VIDEO_DRIVER, "dummy");
    if (!SDL_Init(SDL_INIT_VIDEO)) {
        copy_error(error, error_length, SDL_GetError());
        return NULL;
    }
    micro_native_t *native = calloc(1U, sizeof(*native));
    if (native == NULL) {
        copy_error(error, error_length, "out of memory");
        SDL_Quit();
        return NULL;
    }
    SDL_WindowFlags flags = hidden ? SDL_WINDOW_HIDDEN : 0;
    native->window = SDL_CreateWindow("Micro App", width, height, flags);
    if (native->window == NULL) goto sdl_error;
    native->renderer = SDL_CreateRenderer(native->window, NULL);
    if (native->renderer == NULL) goto sdl_error;
    native->texture = SDL_CreateTexture(native->renderer, SDL_PIXELFORMAT_ARGB8888, SDL_TEXTUREACCESS_STREAMING, width, height);
    if (native->texture == NULL) goto sdl_error;

    lv_init();
    lv_tick_set_cb(tick_callback);
    native->display = lv_display_create(width, height);
    if (native->display == NULL) {
        copy_error(error, error_length, "LVGL display creation failed");
        micro_native_destroy(native);
        return NULL;
    }
    native->display_buffer_size = (size_t)width * 40U * 4U;
    native->display_buffer = malloc(native->display_buffer_size);
    if (native->display_buffer == NULL) {
        copy_error(error, error_length, "LVGL display buffer allocation failed");
        micro_native_destroy(native);
        return NULL;
    }
    lv_display_set_user_data(native->display, native);
    lv_display_set_flush_cb(native->display, flush_callback);
    lv_display_set_buffers(native->display, native->display_buffer, NULL, native->display_buffer_size, LV_DISPLAY_RENDER_MODE_PARTIAL);
    native->pointer = lv_indev_create();
    lv_indev_set_type(native->pointer, LV_INDEV_TYPE_POINTER);
    lv_indev_set_user_data(native->pointer, native);
    lv_indev_set_read_cb(native->pointer, pointer_callback);
    lv_indev_set_display(native->pointer, native->display);
    return native;

sdl_error:
    copy_error(error, error_length, SDL_GetError());
    micro_native_destroy(native);
    return NULL;
}

void micro_native_destroy(micro_native_t *native) {
    if (native == NULL) return;
    if (native->display != NULL) lv_deinit();
    free(native->display_buffer);
    if (native->texture != NULL) SDL_DestroyTexture(native->texture);
    if (native->renderer != NULL) SDL_DestroyRenderer(native->renderer);
    if (native->window != NULL) SDL_DestroyWindow(native->window);
    SDL_Quit();
    free(native);
}

int micro_native_destroy_app_root(micro_native_t *native) {
    if (native == NULL || native->display == NULL) {
        return 0;
    }
    lv_obj_clean(lv_display_get_screen_active(native->display));
    memset(native->objects, 0, sizeof(native->objects));
    memset(native->text_targets, 0, sizeof(native->text_targets));
    memset(native->needles, 0, sizeof(native->needles));
    native->input_read = 0;
    native->input_write = 0;
    native->tabview = NULL;
    native->tab_target = NULL;
    native->slider_read = 0;
    native->slider_write = 0;
    native->checkbox_read = 0;
    native->checkbox_write = 0;
    native->dropdown_read = 0;
    native->dropdown_write = 0;
    native->roller_read = 0;
    native->roller_write = 0;
    memset(native->layout_specs, 0, sizeof(native->layout_specs));
    return 1;
}

int micro_native_poll(micro_native_t *native) {
    SDL_Event event;
    while (SDL_PollEvent(&event)) {
        switch (event.type) {
            case SDL_EVENT_QUIT: native->quit = true; break;
            case SDL_EVENT_MOUSE_MOTION:
                native->pointer_position.x = (lv_coord_t)event.motion.x;
                native->pointer_position.y = (lv_coord_t)event.motion.y;
                lv_indev_read(native->pointer);
                break;
            case SDL_EVENT_MOUSE_BUTTON_DOWN:
                if (event.button.button == SDL_BUTTON_LEFT) {
                    native->pointer_position.x = (lv_coord_t)event.button.x;
                    native->pointer_position.y = (lv_coord_t)event.button.y;
                    native->pointer_pressed = true;
                    lv_indev_read(native->pointer);
                }
                break;
            case SDL_EVENT_MOUSE_BUTTON_UP:
                if (event.button.button == SDL_BUTTON_LEFT) {
                    native->pointer_position.x = (lv_coord_t)event.button.x;
                    native->pointer_position.y = (lv_coord_t)event.button.y;
                    native->pointer_pressed = false;
                    lv_indev_read(native->pointer);
                }
                break;
            default: break;
        }
    }
    return native->quit ? 0 : 1;
}

uint32_t micro_native_timer(micro_native_t *native) {
    (void)native;
    return lv_timer_handler();
}

int micro_native_take_activation(micro_native_t *native, uint32_t *handler_id) {
    if (native->activation_read == native->activation_write) return 0;
    *handler_id = native->activations[native->activation_read];
    native->activation_read = (native->activation_read + 1U) % MICRO_EVENT_CAPACITY;
    return 1;
}

void micro_native_inject_activation(micro_native_t *native, uint32_t handler_id) {
    enqueue_activation(native, handler_id);
}

int micro_native_queue_click(micro_native_t *native, uint32_t node_id) {
    if (node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    lv_obj_update_layout(native->objects[node_id]);
    lv_area_t coordinates;
    lv_obj_get_coords(native->objects[node_id], &coordinates);
    float x = (float)(coordinates.x1 + coordinates.x2) / 2.0F;
    float y = (float)(coordinates.y1 + coordinates.y2) / 2.0F;
    SDL_Event down = {0};
    down.type = SDL_EVENT_MOUSE_BUTTON_DOWN;
    down.button.windowID = SDL_GetWindowID(native->window);
    down.button.button = SDL_BUTTON_LEFT;
    down.button.down = true;
    down.button.clicks = 1;
    down.button.x = x;
    down.button.y = y;
    SDL_Event up = down;
    up.type = SDL_EVENT_MOUSE_BUTTON_UP;
    up.button.down = false;
    return SDL_PushEvent(&down) && SDL_PushEvent(&up);
}

int micro_native_create_column(micro_native_t *native, uint32_t node_id, uint32_t parent_id) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *object = lv_obj_create(parent);
    lv_obj_set_size(object, LV_PCT(100), LV_SIZE_CONTENT);
    lv_obj_set_layout(object, LV_LAYOUT_FLEX);
    lv_obj_set_flex_flow(object, LV_FLEX_FLOW_COLUMN);
    lv_obj_set_flex_align(object, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER);
    lv_obj_set_style_border_width(object, 0, LV_PART_MAIN);
    /* Blend into the parent: invisible but opaque (a transparent container
     * changed LVGL's draw path and intermittently hid the top rows). */
    lv_obj_set_style_bg_color(object, lv_obj_get_style_bg_color(parent, LV_PART_MAIN), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(object, LV_OPA_COVER, LV_PART_MAIN);
    if (parent_id == MICRO_NO_PARENT) lv_obj_center(object);
    native->objects[node_id] = object;
    return 1;
}

int micro_native_create_row(micro_native_t *native, uint32_t node_id, uint32_t parent_id) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *object = lv_obj_create(parent);
    lv_obj_set_size(object, LV_PCT(100), LV_SIZE_CONTENT);
    lv_obj_set_layout(object, LV_LAYOUT_FLEX);
    lv_obj_set_flex_flow(object, LV_FLEX_FLOW_ROW);
    lv_obj_set_flex_align(object, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER, LV_FLEX_ALIGN_CENTER);
    lv_obj_set_style_pad_column(object, 16, LV_PART_MAIN);
    lv_obj_set_style_border_width(object, 0, LV_PART_MAIN);
    lv_obj_set_style_bg_color(object, lv_obj_get_style_bg_color(parent, LV_PART_MAIN), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(object, LV_OPA_COVER, LV_PART_MAIN);
    native->objects[node_id] = object;
    return 1;
}

int micro_native_create_list(micro_native_t *native, uint32_t node_id, uint32_t parent_id) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *list = lv_list_create(parent);
    lv_obj_set_size(list, LV_PCT(100), LV_SIZE_CONTENT);
    lv_obj_set_style_pad_row(list, 4, LV_PART_MAIN);
    /* Borderless and blended into the parent so it matches the app
     * background; the items themselves stand out. */
    lv_obj_set_style_border_width(list, 0, LV_PART_MAIN);
    lv_obj_set_style_bg_color(list, lv_obj_get_style_bg_color(parent, LV_PART_MAIN), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(list, LV_OPA_COVER, LV_PART_MAIN);
    native->objects[node_id] = list;
    return 1;
}

int micro_native_create_tabview(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                                 const char *titles) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *tabview = lv_tabview_create(parent);
    /* Bounded height so the tabview sits inside the scrollable
     * column without filling the screen, blocking column scroll,
     * or driving a layout feedback loop that flickers. */
            lv_obj_set_size(tabview, LV_PCT(100), 280);
    char *copy = strdup(titles);
    if (copy != NULL) {
        char *save = NULL;
        for (char *token = strtok_r(copy, "\n", &save); token != NULL;
             token = strtok_r(NULL, "\n", &save)) {
            lv_tabview_add_tab(tabview, token);
        }
        free(copy);
    }
    /* One uniform light background across the app; no padding or borders at
     * the tab level, scrollbar flush with the page edge. */
    lv_color_t app_bg = lv_color_hex(0xF2F4F0);
    if (parent_id == MICRO_NO_PARENT) {
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
        lv_obj_set_style_pad_all(page, 0, LV_PART_MAIN);
        lv_obj_set_style_bg_color(page, app_bg, LV_PART_MAIN);
        lv_obj_set_style_bg_opa(page, LV_OPA_COVER, LV_PART_MAIN);
        lv_obj_set_style_pad_all(page, 0, LV_PART_SCROLLBAR);
        lv_obj_set_style_bg_color(page, lv_color_hex(0xB8BFB4), LV_PART_SCROLLBAR);
        lv_obj_set_style_bg_opa(page, LV_OPA_COVER, LV_PART_SCROLLBAR);
    }
    native->tabview = tabview;
    native->tab_target = NULL;
    native->objects[node_id] = tabview;
    return 1;
}

int micro_native_create_tab_content(micro_native_t *native, uint32_t index) {
    if (native == NULL || native->tabview == NULL) return 0;
    lv_obj_t *content = lv_tabview_get_content(native->tabview);
    if (content == NULL) return 0;
    lv_obj_t *page = lv_obj_get_child(content, index);
    if (page == NULL) return 0;
    native->tab_target = page;
    return 1;
}

int micro_native_create_progress(micro_native_t *native, uint32_t node_id, uint32_t parent_id, double fraction) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *bar = lv_bar_create(parent);
    lv_bar_set_range(bar, 0, 100);
    lv_bar_set_value(bar, (int32_t)(fraction * 100.0), LV_ANIM_OFF);
    lv_obj_set_size(bar, LV_PCT(100), 12);
    native->objects[node_id] = bar;
    return 1;
}

int micro_native_create_switch(micro_native_t *native, uint32_t node_id, uint32_t parent_id, int checked, uint32_t handler_id) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *toggle = lv_switch_create(parent);
    if (checked) lv_obj_add_state(toggle, LV_STATE_CHECKED);
    if (handler_id == MICRO_NO_HANDLER) {
        lv_obj_remove_flag(toggle, LV_OBJ_FLAG_CLICKABLE);
    } else {
        native->clicks[node_id].native = native;
        native->clicks[node_id].handler_id = handler_id;
        lv_obj_add_event_cb(toggle, click_callback, LV_EVENT_CLICKED, &native->clicks[node_id]);
    }
    native->objects[node_id] = toggle;
    return 1;
}

int micro_native_set_progress_value(micro_native_t *native, uint32_t node_id, double fraction) {
    if (node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    if (fraction < 0.0) fraction = 0.0;
    if (fraction > 1.0) fraction = 1.0;
    lv_bar_set_value(native->objects[node_id], (int32_t)(fraction * 100.0), LV_ANIM_OFF);
    return 1;
}

int micro_native_set_switch_checked(micro_native_t *native, uint32_t node_id, int checked) {
    if (node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    if (checked) {
        lv_obj_add_state(native->objects[node_id], LV_STATE_CHECKED);
    } else {
        lv_obj_clear_state(native->objects[node_id], LV_STATE_CHECKED);
    }
    return 1;
}

int micro_native_create_label(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *text, uintptr_t font_handle, uint32_t line_height_px) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *label = lv_label_create(parent);
    lv_label_set_text(label, text);
    apply_text_style(label, font_handle, line_height_px);
    native->objects[node_id] = label;
    native->text_targets[node_id] = label;
    return 1;
}

int micro_native_create_button(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *text, uint32_t handler_id, uintptr_t font_handle, uint32_t line_height_px) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *button;
    lv_obj_t *label;
    if (lv_obj_check_type(parent, &lv_list_class)) {
        button = lv_list_add_button(parent, NULL, text);
        label = lv_obj_get_child(button, 0);
    } else {
        button = lv_button_create(parent);
        label = lv_label_create(button);
        lv_label_set_text(label, text);
        apply_text_style(label, font_handle, line_height_px);
        lv_obj_center(label);
    }
    native->clicks[node_id].native = native;
    native->clicks[node_id].handler_id = handler_id;
    lv_obj_add_event_cb(button, click_callback, LV_EVENT_CLICKED, &native->clicks[node_id]);
    native->objects[node_id] = button;
    native->text_targets[node_id] = label;
    return 1;
}

int micro_native_set_label_text(micro_native_t *native, uint32_t node_id, const char *text) {
    if (node_id >= MICRO_MAX_NODES || native->text_targets[node_id] == NULL) return 0;
    lv_label_set_text(native->text_targets[node_id], text);
    return 1;
}

int micro_native_create_input(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                              const char *text, const char *placeholder, uint32_t handler_id,
                              uintptr_t font_handle, uint32_t line_height_px) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *textarea = lv_textarea_create(parent);
    lv_textarea_set_one_line(textarea, true);
    lv_textarea_set_cursor_click_pos(textarea, true);
    lv_textarea_set_text(textarea, text);
    if (placeholder != NULL && placeholder[0] != '\0') {
        lv_textarea_set_placeholder_text(textarea, placeholder);
    }
    apply_text_style(textarea, font_handle, line_height_px);
    lv_obj_set_style_border_color(textarea, lv_color_hex(0x888888), LV_PART_MAIN);
    lv_obj_set_style_border_width(textarea, 2, LV_PART_MAIN);
    lv_obj_set_style_border_opa(textarea, LV_OPA_COVER, LV_PART_MAIN);
    lv_obj_set_style_pad_all(textarea, 8, LV_PART_MAIN);
    lv_obj_set_width(textarea, LV_PCT(100));
    lv_obj_set_height(textarea, LV_SIZE_CONTENT);
    if (handler_id == MICRO_NO_HANDLER) {
        lv_obj_remove_flag(textarea, LV_OBJ_FLAG_CLICKABLE);
    } else {
        native->clicks[node_id].native = native;
        native->clicks[node_id].handler_id = handler_id;
        lv_obj_add_event_cb(textarea, input_callback, LV_EVENT_VALUE_CHANGED,
                            &native->clicks[node_id]);
    }
    native->objects[node_id] = textarea;
    native->text_targets[node_id] = textarea;
    return 1;
}

int micro_native_set_input_text(micro_native_t *native, uint32_t node_id, const char *text) {
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    lv_textarea_set_text(native->objects[node_id], text);
    return 1;
}

int micro_native_create_slider(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                               double value, double min, double max, uint32_t handler_id) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *slider = lv_slider_create(parent);
    lv_slider_set_range(slider, (int32_t)min, (int32_t)max);
    lv_slider_set_value(slider, (int32_t)value, LV_ANIM_OFF);
    lv_obj_set_width(slider, LV_PCT(100));
    if (handler_id == MICRO_NO_HANDLER) {
        lv_obj_remove_flag(slider, LV_OBJ_FLAG_CLICKABLE);
    } else {
        native->clicks[node_id].native = native;
        native->clicks[node_id].handler_id = handler_id;
        lv_obj_add_event_cb(slider, slider_callback, LV_EVENT_VALUE_CHANGED,
                            &native->clicks[node_id]);
    }
    native->objects[node_id] = slider;
    return 1;
}

int micro_native_set_slider_value(micro_native_t *native, uint32_t node_id, double value) {
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    lv_slider_set_value(native->objects[node_id], (int32_t)value, LV_ANIM_OFF);
    return 1;
}

int micro_native_create_checkbox(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                                 const char *label, int checked, uint32_t handler_id) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *checkbox = lv_checkbox_create(parent);
    lv_checkbox_set_text(checkbox, label);
    if (checked) lv_obj_add_state(checkbox, LV_STATE_CHECKED);
    if (handler_id == MICRO_NO_HANDLER) {
        lv_obj_remove_flag(checkbox, LV_OBJ_FLAG_CLICKABLE);
    } else {
        native->clicks[node_id].native = native;
        native->clicks[node_id].handler_id = handler_id;
        lv_obj_add_event_cb(checkbox, checkbox_callback, LV_EVENT_VALUE_CHANGED,
                            &native->clicks[node_id]);
    }
    native->objects[node_id] = checkbox;
    return 1;
}

int micro_native_create_dropdown(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                                 const char *options, double index, uint32_t handler_id) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *dropdown = lv_dropdown_create(parent);
    lv_dropdown_set_options(dropdown, options);
    lv_dropdown_set_selected(dropdown, (uint32_t)index);
    lv_obj_set_width(dropdown, LV_PCT(100));
    if (handler_id == MICRO_NO_HANDLER) {
        lv_obj_remove_flag(dropdown, LV_OBJ_FLAG_CLICKABLE);
    } else {
        native->clicks[node_id].native = native;
        native->clicks[node_id].handler_id = handler_id;
        lv_obj_add_event_cb(dropdown, dropdown_callback, LV_EVENT_READY,
                            &native->clicks[node_id]);
    }
    native->objects[node_id] = dropdown;
    return 1;
}

static uint32_t s_delphi_layout_id = LV_LAYOUT_NONE;

static uint8_t layout_mask(const micro_native_t *native, uint32_t node)
{
    return (native != NULL && node < MICRO_MAX_NODES) ? native->layout_specs[node].mask : 0;
}

static void delphi_layout_update_cb(lv_obj_t *container, void *user_data)
{
    micro_native_t *native = (micro_native_t *)user_data;
    lv_coord_t avail_w = lv_obj_get_content_width(container);
    lv_coord_t avail_h = lv_obj_get_content_height(container);
    lv_coord_t row_gap = lv_obj_get_style_pad_row(container, LV_PART_MAIN);
    uint32_t count = lv_obj_get_child_count(container);
    lv_coord_t top_y = 0, bottom_y = avail_h, bottom_stack_top = avail_h;

    /* Pass 1 — stack top/bottom docked children; vertical fills deferred. */
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = layout_mask(native, node);
        lv_obj_update_layout(child);
        lv_coord_t w = lv_obj_get_width(child), h = lv_obj_get_height(child);
        lv_coord_t x;
        if ((mask & 1) && (mask & 4)) {
            x = (lv_coord_t)native->layout_specs[node].left;
            lv_obj_set_width(child, avail_w - (lv_coord_t)native->layout_specs[node].left
                                       - (lv_coord_t)native->layout_specs[node].right);
        } else if (mask & 1) {
            x = (lv_coord_t)native->layout_specs[node].left;
            lv_obj_set_width(child, w);
        } else if (mask & 4) {
            lv_obj_set_width(child, w);
            x = avail_w - w - (lv_coord_t)native->layout_specs[node].right;
        } else {
            x = 0;
            lv_obj_set_width(child, avail_w);
        }
        if ((mask & 2) && (mask & 8)) continue;
        if (mask & 8) {
            bottom_y -= h;
            lv_coord_t y = bottom_y - (lv_coord_t)native->layout_specs[node].bottom;
            lv_obj_set_pos(child, x, y);
            bottom_y = y - row_gap;
            if (y < bottom_stack_top) bottom_stack_top = y;
        } else {
            lv_coord_t y = top_y + ((mask & 2) ? (lv_coord_t)native->layout_specs[node].top : 0);
            lv_obj_set_pos(child, x, y);
            top_y = y + h + row_gap;
        }
    }

    /* Pass 2 — vertical fills span between the two stacks. */
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = layout_mask(native, node);
        if (!((mask & 2) && (mask & 8))) continue;
        lv_coord_t h = bottom_stack_top - top_y;
        if (h < 0) h = 0;
        lv_coord_t w = lv_obj_get_width(child);
        lv_coord_t x;
        if ((mask & 1) && (mask & 4)) {
            x = (lv_coord_t)native->layout_specs[node].left;
            lv_obj_set_width(child, avail_w - (lv_coord_t)native->layout_specs[node].left
                                       - (lv_coord_t)native->layout_specs[node].right);
        } else if (mask & 1) {
            x = (lv_coord_t)native->layout_specs[node].left;
            lv_obj_set_width(child, w);
        } else if (mask & 4) {
            lv_obj_set_width(child, w);
            x = avail_w - w - (lv_coord_t)native->layout_specs[node].right;
        } else {
            x = 0;
            lv_obj_set_width(child, avail_w);
        }
        lv_obj_set_pos(child, x, top_y);
        lv_obj_set_height(child, h);
    }
}

int micro_native_set_layout_spec(micro_native_t *native, uint32_t node_id, uint32_t mask,
                                 double left, double top, double right, double bottom) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    native->layout_specs[node_id].mask = (uint8_t)mask;
    native->layout_specs[node_id].left = left;
    native->layout_specs[node_id].top = top;
    native->layout_specs[node_id].right = right;
    native->layout_specs[node_id].bottom = bottom;
    if (native->objects[node_id] != NULL) {
        lv_obj_set_user_data(native->objects[node_id], (void *)(uintptr_t)node_id);
    }
    return 1;
}

static bool delphi_get_min_size_cb(lv_obj_t *container, int32_t *req_size,
                                    bool width, void *user_data) {
    micro_native_t *native = (micro_native_t *)user_data;
    if (width) { *req_size = lv_obj_get_content_width(container); return true; }
    int32_t top_extent = 0, bottom_extent = 0, fill_floor = 0;
    int32_t top_count = 0, bottom_count = 0;
    lv_coord_t row_gap = lv_obj_get_style_pad_row(container, LV_PART_MAIN);
    uint32_t count = lv_obj_get_child_count(container);
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = layout_mask(native, node);
        int32_t h = lv_obj_get_height(child);
        if ((mask & 2) && (mask & 8)) { if (h > fill_floor) fill_floor = h; continue; }
        if (mask & 8) { bottom_extent += h + (int32_t)native->layout_specs[node].bottom; bottom_count++; }
        else { top_extent += h + ((mask & 2) ? (int32_t)native->layout_specs[node].top : 0); top_count++; }
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

static lv_coord_t delphi_content_height(const micro_native_t *native, lv_obj_t *container)
{
    int32_t top_extent = 0, bottom_extent = 0, fill_floor = 0;
    int32_t top_count = 0, bottom_count = 0;
    lv_coord_t row_gap = lv_obj_get_style_pad_row(container, LV_PART_MAIN);
    uint32_t count = lv_obj_get_child_count(container);
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = layout_mask(native, node);
        int32_t h = lv_obj_get_height(child);
        if ((mask & 2) && (mask & 8)) { if (h > fill_floor) fill_floor = h; continue; }
        if (mask & 8) { bottom_extent += h + (int32_t)native->layout_specs[node].bottom; bottom_count++; }
        else { top_extent += h + ((mask & 2) ? (int32_t)native->layout_specs[node].top : 0); top_count++; }
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

int micro_native_apply_delphi_layout(micro_native_t *native, uint32_t container,
                                     const uint32_t *child_ids, uint32_t child_count) {
    (void)child_ids; (void)child_count;
    if (native == NULL || container >= MICRO_MAX_NODES || native->objects[container] == NULL) return 0;
    lv_obj_t *obj = native->objects[container];
    if (s_delphi_layout_id == LV_LAYOUT_NONE) {
        lv_layout_callbacks_t callbacks = { .layout_update_cb = delphi_layout_update_cb,
                                            .get_min_size_cb = delphi_get_min_size_cb };
        s_delphi_layout_id = lv_layout_create(callbacks, native);
    }
    lv_obj_set_layout(obj, s_delphi_layout_id);
    /* Resolve children synchronously, then pin the container to its exact
     * content height so the first render cannot collapse the top rows. */
    lv_obj_update_layout(obj);
    lv_obj_set_height(obj, delphi_content_height(native, obj));
    return 1;
}

int micro_native_create_led(micro_native_t *native, uint32_t node_id, uint32_t parent_id, int on) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *led = lv_led_create(parent);
    lv_led_set_brightness(led, on ? LV_LED_BRIGHT_MAX : LV_LED_BRIGHT_MIN);
    native->objects[node_id] = led;
    return 1;
}

int micro_native_set_led(micro_native_t *native, uint32_t node_id, int on) {
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    lv_led_set_brightness(native->objects[node_id], on ? LV_LED_BRIGHT_MAX : LV_LED_BRIGHT_MIN);
    return 1;
}

int micro_native_create_spinner(micro_native_t *native, uint32_t node_id, uint32_t parent_id, int active) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *spinner = lv_spinner_create(parent);
    lv_obj_set_size(spinner, 48, 48);
    if (!active) lv_obj_add_flag(spinner, LV_OBJ_FLAG_HIDDEN);
    native->objects[node_id] = spinner;
    return 1;
}

int micro_native_set_spinner(micro_native_t *native, uint32_t node_id, int active) {
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    if (active) lv_obj_clear_flag(native->objects[node_id], LV_OBJ_FLAG_HIDDEN);
    else lv_obj_add_flag(native->objects[node_id], LV_OBJ_FLAG_HIDDEN);
    return 1;
}

int micro_native_create_scale(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                              double value, double min, double max) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *scale = lv_scale_create(parent);
    lv_scale_set_mode(scale, LV_SCALE_MODE_ROUND_INNER);
    lv_scale_set_range(scale, (int32_t)min, (int32_t)max);
    lv_obj_set_size(scale, 100, 100);
    lv_obj_t *needle = lv_line_create(scale);
    lv_point_precise_t points[2] = {{0, 0}, {0, -40}};
    lv_line_set_points(needle, points, 2);
    lv_scale_set_line_needle_value(scale, needle, 40, (int32_t)value);
    native->needles[node_id] = needle;
    native->objects[node_id] = scale;
    return 1;
}

int micro_native_set_scale_value(micro_native_t *native, uint32_t node_id, double value) {
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL ||
        native->needles[node_id] == NULL) return 0;
    lv_scale_set_line_needle_value(native->objects[node_id], native->needles[node_id], 60, (int32_t)value);
    return 1;
}

int micro_native_create_roller(micro_native_t *native, uint32_t node_id, uint32_t parent_id,
                               const char *options, double index, uint32_t handler_id) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *roller = lv_roller_create(parent);
    lv_roller_set_options(roller, options, LV_ROLLER_MODE_NORMAL);
    lv_roller_set_selected(roller, (uint32_t)index, LV_ANIM_OFF);
    lv_obj_set_width(roller, LV_PCT(100));
    if (handler_id == MICRO_NO_HANDLER) {
        lv_obj_remove_flag(roller, LV_OBJ_FLAG_CLICKABLE);
    } else {
        native->clicks[node_id].native = native;
        native->clicks[node_id].handler_id = handler_id;
        lv_obj_add_event_cb(roller, roller_callback, LV_EVENT_VALUE_CHANGED,
                            &native->clicks[node_id]);
    }
    native->objects[node_id] = roller;
    return 1;
}

int micro_native_set_selection_value(micro_native_t *native, uint32_t node_id, double index) {
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    lv_obj_t *obj = native->objects[node_id];
    if (lv_obj_check_type(obj, &lv_dropdown_class)) {
        lv_dropdown_set_selected(obj, (uint32_t)index);
    } else if (lv_obj_check_type(obj, &lv_roller_class)) {
        lv_roller_set_selected(obj, (uint32_t)index, LV_ANIM_OFF);
    }
    return 1;
}

int micro_native_take_checkbox_change(micro_native_t *native, uint32_t *handler_id, int *checked) {
    if (native == NULL || handler_id == NULL || checked == NULL) return 0;
    if (native->checkbox_read == native->checkbox_write) return 0;
    *handler_id = native->checkbox_changes[native->checkbox_read].handler_id;
    *checked = native->checkbox_changes[native->checkbox_read].checked;
    native->checkbox_read = (native->checkbox_read + 1U) % MICRO_CHECKBOX_CAPACITY;
    return 1;
}

int micro_native_take_dropdown_change(micro_native_t *native, uint32_t *handler_id, double *index) {
    if (native == NULL || handler_id == NULL || index == NULL) return 0;
    if (native->dropdown_read == native->dropdown_write) return 0;
    *handler_id = native->dropdown_changes[native->dropdown_read].handler_id;
    *index = native->dropdown_changes[native->dropdown_read].index;
    native->dropdown_read = (native->dropdown_read + 1U) % MICRO_DROPDOWN_CAPACITY;
    return 1;
}

int micro_native_take_roller_change(micro_native_t *native, uint32_t *handler_id, double *index) {
    if (native == NULL || handler_id == NULL || index == NULL) return 0;
    if (native->roller_read == native->roller_write) return 0;
    *handler_id = native->roller_changes[native->roller_read].handler_id;
    *index = native->roller_changes[native->roller_read].index;
    native->roller_read = (native->roller_read + 1U) % MICRO_ROLLER_CAPACITY;
    return 1;
}

int micro_native_take_slider_change(micro_native_t *native, uint32_t *handler_id, double *value) {
    if (native == NULL || handler_id == NULL || value == NULL) return 0;
    if (native->slider_read == native->slider_write) return 0;
    *handler_id = native->slider_changes[native->slider_read].handler_id;
    *value = native->slider_changes[native->slider_read].value;
    native->slider_read = (native->slider_read + 1U) % MICRO_SLIDER_CAPACITY;
    return 1;
}

int micro_native_take_input_change(micro_native_t *native, uint32_t *handler_id, char *text,
                                   size_t text_capacity, size_t *text_len) {
    if (native == NULL || handler_id == NULL || text == NULL || text_len == NULL ||
        text_capacity == 0) {
        return 0;
    }
    if (native->input_read == native->input_write) return 0;
    *handler_id = native->input_changes[native->input_read].handler_id;
    size_t len = native->input_changes[native->input_read].len;
    if (len > text_capacity) len = text_capacity;
    memcpy(text, native->input_changes[native->input_read].text, len);
    *text_len = len;
    native->input_read = (native->input_read + 1U) % MICRO_INPUT_CAPACITY;
    return 1;
}
