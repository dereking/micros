#include "micro_native.h"

#include <SDL3/SDL.h>
#include <lvgl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MICRO_MAX_NODES 256U
#define MICRO_EVENT_CAPACITY 64U
#define MICRO_NO_PARENT UINT32_MAX

typedef struct micro_click_context {
    struct micro_native *native;
    uint32_t handler_id;
} micro_click_context_t;

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
    micro_click_context_t clicks[MICRO_MAX_NODES];
    uint32_t activations[MICRO_EVENT_CAPACITY];
    unsigned activation_read;
    unsigned activation_write;
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

static lv_obj_t *parent_object(micro_native_t *native, uint32_t parent_id) {
    if (parent_id == MICRO_NO_PARENT) return lv_screen_active();
    if (parent_id >= MICRO_MAX_NODES) return NULL;
    return native->objects[parent_id];
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

int micro_native_create_label(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *text) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *label = lv_label_create(parent);
    lv_label_set_text(label, text);
    native->objects[node_id] = label;
    return 1;
}

int micro_native_create_button(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *text, uint32_t handler_id) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *button = lv_button_create(parent);
    lv_obj_t *label = lv_label_create(button);
    lv_label_set_text(label, text);
    lv_obj_center(label);
    native->clicks[node_id].native = native;
    native->clicks[node_id].handler_id = handler_id;
    lv_obj_add_event_cb(button, click_callback, LV_EVENT_CLICKED, &native->clicks[node_id]);
    native->objects[node_id] = button;
    return 1;
}

int micro_native_set_label_text(micro_native_t *native, uint32_t node_id, const char *text) {
    if (node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    lv_label_set_text(native->objects[node_id], text);
    return 1;
}
