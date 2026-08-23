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

static lv_obj_t *parent_object(micro_native_t *native, uint32_t parent_id) {
    if (parent_id == MICRO_NO_PARENT) return lv_screen_active();
    if (parent_id >= MICRO_MAX_NODES) return NULL;
    return native->objects[parent_id];
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
    native->input_read = 0;
    native->input_write = 0;
    native->slider_read = 0;
    native->slider_write = 0;
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
    native->objects[node_id] = object;
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
    lv_obj_t *button = lv_button_create(parent);
    lv_obj_t *label = lv_label_create(button);
    lv_label_set_text(label, text);
    apply_text_style(label, font_handle, line_height_px);
    lv_obj_center(label);
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
