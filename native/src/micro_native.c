#include "micro_native.h"

#include "micro_appbar.h"
#include "micro_host_menu.h"

#include <SDL3/SDL.h>
#include <lvgl.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Tab titles are created inside LVGL's tabview and therefore do not receive a
 * Rust-owned text-style handle. Use the generated 14 px UI face directly so
 * Chinese titles do not fall back to LVGL's Latin-only default font. */
extern const lv_font_t micro_ui_sans_14;

/* The LVGL canvas is the device screen. On desktop the host can switch the
 * device between landscape (800x480) and portrait (480x800); the OS app bar
 * and the App layout reflow in the new logical size instead of being rotated
 * as a bitmap. The host toolbar is a top icon strip drawn by SDL, so it never
 * becomes part of the App/OS layout. */
#define MICRO_NATIVE_EDGE_ZONE 40
#define MICRO_NATIVE_SWIPE_THRESHOLD 60
#define MICRO_MIN_ZOOM_PERCENT 25
#define MICRO_MAX_ZOOM_PERCENT 400
#define MICRO_MIN_WINDOW_WIDTH 120
#define MICRO_MIN_WINDOW_HEIGHT 120
#define MICRO_ZOOM_STEP_PERCENT 25
/* One SDL wheel "tick" (a classic mouse notch) scrolls this many LVGL px.
 * Trackpads report fractional ticks, so the remainder is accumulated. */
#define MICRO_WHEEL_STEP_PX 48.0f
#define MICRO_WINDOW_MARGIN 0.94f
#define MICRO_PRESENT_BG 0x101418

/* Desktop host toolbar: a 40 px icon strip above the device canvas with
 * Landscape / Portrait / Rotate controls and a platform device menu. The icons
 * are drawn with SDL primitives; the built-in debug font is only used for the
 * hover label, so the host stays free of an extra font dependency. */
#define MICRO_TOOLBAR_HEIGHT 40
#define MICRO_TOOLBAR_MARGIN 12
#define MICRO_TOOLBAR_BUTTON_WIDTH 40
#define MICRO_TOOLBAR_BUTTON_HEIGHT 28
#define MICRO_TOOLBAR_BUTTON_GAP 6
#define MICRO_TOOLBAR_ICON_SIZE 20.0f
#define MICRO_TOOLBAR_BG 0x141A16
#define MICRO_TOOLBAR_BUTTON_HOVER 0x2C362F
#define MICRO_TOOLBAR_ACTIVE 0x2F7D5B
#define MICRO_TOOLBAR_ACTIVE_HOVER 0x38936B
#define MICRO_TOOLBAR_BORDER 0x0A0F0C
#define MICRO_TOOLBAR_SEPARATOR 0x39443B
#define MICRO_TOOLBAR_ICON 0xD6E0D8
#define MICRO_TOOLBAR_ICON_ACTIVE 0xFFFFFF
#define MICRO_TOOLBAR_MUTED 0x93A398
#define MICRO_TOOLBAR_ACTION_LANDSCAPE 0
#define MICRO_TOOLBAR_ACTION_PORTRAIT 1
#define MICRO_TOOLBAR_ACTION_ROTATE 2
#define MICRO_TOOLBAR_ACTION_MENU 3
#define MICRO_TOOLBAR_ACTION_COUNT 4
#define MICRO_HOST_ACTION_EVENT (SDL_EVENT_USER + 0x4D)
#define MICRO_HOST_ACTION_CAPACITY 8U
/* Portable fallback menu, used by non-Apple desktop hosts and as a headless
 * test path when AppKit is unavailable. It is drawn by SDL above the LVGL
 * canvas so it never becomes part of the App layout. */
#define MICRO_HOST_MENU_WIDTH 260.0f
#define MICRO_HOST_MENU_MARGIN 8.0f
#define MICRO_HOST_MENU_ROW 18.0f
#define MICRO_HOST_MENU_PAD 6.0f
#define MICRO_HOST_MENU_ITEM_MAX 24
#define MICRO_HOST_MENU_LABEL_MAX 64
#define MICRO_HOST_MENU_VALUE_MAX 24
#define MICRO_HOST_MENU_BG 0x1A211C
#define MICRO_HOST_MENU_BORDER 0x4A5C4F
#define MICRO_HOST_MENU_HOVER 0x2C362F
#define MICRO_HOST_MENU_PRESSED 0x38503F
#define MICRO_HOST_MENU_TEXT 0xE6EEE7
#define MICRO_HOST_MENU_MUTED 0x93A398
#define MICRO_HOST_MENU_HEADER_COLOR 0x9FD3A8
#define MICRO_HOST_MENU_CHECK 0x6FCF97
#define MICRO_HOST_MENU_RUNNING 0x7BD88F
#define MICRO_HOST_MENU_OFF 0xE0B36A
#define MICRO_HOST_MENU_ACTION_CLOSE 100
#define MICRO_HOST_MENU_ACTION_ZOOM_CYCLE 101
#define MICRO_PI 3.14159265358979323846f
#define MICRO_DEG_TO_RAD (MICRO_PI / 180.0f)

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
    uint8_t mask; /* bit0=left,bit1=top,bit2=width,bit3=height,bit4=anchor_l,bit5=anchor_t,bit6=anchor_r,bit7=anchor_b */
    double left;
    double top;
    double width;
    double height;
    double anchor_left;
    double anchor_top;
    double anchor_right;
    double anchor_bottom;
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
    /* Device viewport. LVGL always renders `design_width` x `design_height`.
     * Switching orientation swaps those dimensions and reflows the OS chrome
     * and App layout; the canvas is only scaled (never bitmap-rotated) to fit
     * the window. `rotation` remains 0 for the logical viewport; `portrait`
     * reflects which dimension pair is active. */
    int design_width;
    int design_height;
    int landscape_width;
    int landscape_height;
    int window_width;
    int window_height;
    int output_width;
    int output_height;
    int rotation;
    bool portrait;
    bool zoom_fit;
    int zoom_percent;
    float view_scale;
    float view_x;
    float view_y;
    bool view_dirty;
    bool hidden;
    bool resizing_window;
    SDL_WindowID window_id;
    /* Desktop toolbar interaction (drawn by SDL above the device canvas). */
    bool show_toolbar;
    bool host_menu_available;
    int toolbar_hover;
    int toolbar_pressed;
    /* Portable SDL menu state. AppKit has its own retained NSMenu; these
     * fields are only used when `micro_host_menu_is_native()` is false. */
    bool sdl_menu_open;
    int sdl_menu_hover;
    int sdl_menu_pressed;
    int sdl_menu_scroll;
    bool sdl_menu_swallow_release;
    /* Host actions posted by the platform menu (SDL user events) and drained
     * by the Rust host loop. Kept separate from LVGL input. */
    int host_actions[MICRO_HOST_ACTION_CAPACITY];
    unsigned host_action_read;
    unsigned host_action_write;
    /* Edge-swipe back gesture: track a drag that starts near the physical
     * left/right screen edge and moves inward. `gesture_start` is stored in
     * rotated-canvas coordinates so the gesture follows the device rotation. */
    bool back_gesture;
    bool gesture_active;
    int gesture_edge; /* 0 = none, 1 = left, 2 = right */
    lv_point_t gesture_start;
    /* Sub-pixel wheel/trackpad remainder. SDL reports fractional ticks on a
     * trackpad; keeping the remainder makes slow two-finger scrolling move the
     * scrollbar instead of rounding every event to zero. */
    float wheel_pixels_x;
    float wheel_pixels_y;
    lv_obj_t *objects[MICRO_MAX_NODES];
    lv_obj_t *text_targets[MICRO_MAX_NODES];
    /* Textareas receive SDL keyboard/clipboard text directly; LVGL keyboard
     * indev groups are not needed for the single-line fields used by apps. */
    lv_obj_t *focused_textarea;
    /* Backing storage for the test hook's SDL text-input event. Real SDL
     * events carry a pointer valid only while the event is being handled. */
    char injected_text[MICRO_INPUT_TEXT_MAX];
    /* A Cmd/Ctrl+V keydown is pasted at the end of the poll only if SDL did
     * not already turn it into a text-input event (macOS IME may do both). */
    bool pending_paste;
    lv_obj_t *needles[MICRO_MAX_NODES];
    lv_obj_t *arcs[MICRO_MAX_NODES];
    lv_obj_t *tabview;
    lv_obj_t *tab_target;
    /* Containers mounted directly into a tab page fill that page's height;
     * they are not sized to their own natural content. */
    bool fill_height[MICRO_MAX_NODES];
    /* Per-display Delphi layout handle; layout ids do not survive a new
     * display, so this cannot be a process-global static. */
    uint32_t delphi_layout_id;
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
    lv_coord_t fill_natural_h[MICRO_MAX_NODES];
};

/* The desktop host creates one visible window. The native menu queries this
 * snapshot when it opens; headless tests have no menu. */
static micro_native_t *s_active_native = NULL;

static void copy_error(char *target, size_t length, const char *message) {
    if (target == NULL || length == 0U) return;
    snprintf(target, length, "%s", message == NULL ? "unknown native error" : message);
}

static uint32_t tick_callback(void) {
    return (uint32_t)SDL_GetTicks();
}

/* -------------------------------------------------------------------------
 * Device viewport
 *
 * LVGL renders the current logical device screen (`design_width` x
 * `design_height`). The window reserves a host-drawn toolbar at the top and
 * fits the canvas into the remaining area:
 *
 *   design coords --(fit scale)--> output px, centered below the toolbar
 *
 * Orientation switches change the design dimensions, so the OS app bar and the
 * App's `ui.place` anchors reflow instead of being rotated as a bitmap.
 * ---------------------------------------------------------------------- */

static int rotated_width(const micro_native_t *native) {
    return native->design_width;
}

static int rotated_height(const micro_native_t *native) {
    return native->design_height;
}

/* Height of the host toolbar in renderer-output pixels (0 when hidden). */
static int toolbar_output_height(const micro_native_t *native) {
    if (native == NULL || !native->show_toolbar || native->window_height <= 0) {
        return 0;
    }
    int height = (int)lrintf((float)MICRO_TOOLBAR_HEIGHT *
                             (float)native->output_height /
                             (float)native->window_height);
    return height < native->output_height ? height : native->output_height;
}

static void refresh_presentation_metrics(micro_native_t *native) {
    if (native == NULL || native->window == NULL || native->renderer == NULL) {
        return;
    }
    SDL_GetWindowSize(native->window, &native->window_width, &native->window_height);
    if (native->window_width <= 0) native->window_width = native->design_width;
    if (native->window_height <= 0) native->window_height = native->design_height;
    if (!SDL_GetRenderOutputSize(native->renderer, &native->output_width,
                                 &native->output_height) ||
        native->output_width <= 0 || native->output_height <= 0) {
        /* Renderers that cannot report an output size (the dummy driver)
         * still draw 1:1 with the window. */
        native->output_width = native->window_width;
        native->output_height = native->window_height;
    }

    int toolbar_height = toolbar_output_height(native);
    int canvas_height = native->output_height - toolbar_height;
    if (canvas_height < 1) canvas_height = 1;
    float scale_x = (float)native->output_width / (float)native->design_width;
    float scale_y = (float)canvas_height / (float)native->design_height;
    native->view_scale = scale_x < scale_y ? scale_x : scale_y;
    if (!(native->view_scale > 0.0f)) native->view_scale = 1.0f;

    native->view_x = ((float)native->output_width -
                      (float)native->design_width * native->view_scale) * 0.5f;
    native->view_y = (float)toolbar_height +
                     ((float)canvas_height -
                      (float)native->design_height * native->view_scale) * 0.5f;

    if (native->texture != NULL) {
        /* Integer upscales look crisp with nearest-neighbour sampling; any
         * fractional scale (including downscaling) needs linear filtering. */
        bool integer_scale = native->view_scale >= 1.0f &&
            fabsf(native->view_scale - roundf(native->view_scale)) < 0.001f;
        SDL_SetTextureScaleMode(native->texture,
                                integer_scale ? SDL_SCALEMODE_NEAREST
                                              : SDL_SCALEMODE_LINEAR);
    }
    native->view_dirty = true;
}

/* Map a point in design space to the rotated canvas space. Rotation is the
 * same clockwise transform SDL_RenderTextureRotated applies. */
static void logical_to_rotated(const micro_native_t *native, float x, float y,
                               float *rotated_x, float *rotated_y) {
    switch (native->rotation) {
        case 90:
            *rotated_x = (float)native->design_height - y;
            *rotated_y = x;
            break;
        case 180:
            *rotated_x = (float)native->design_width - x;
            *rotated_y = (float)native->design_height - y;
            break;
        case 270:
            *rotated_x = y;
            *rotated_y = (float)native->design_width - x;
            break;
        default:
            *rotated_x = x;
            *rotated_y = y;
            break;
    }
}

static void rotated_to_logical(const micro_native_t *native, float rotated_x,
                               float rotated_y, float *x, float *y) {
    switch (native->rotation) {
        case 90:
            *x = rotated_y;
            *y = (float)native->design_height - rotated_x;
            break;
        case 180:
            *x = (float)native->design_width - rotated_x;
            *y = (float)native->design_height - rotated_y;
            break;
        case 270:
            *x = (float)native->design_width - rotated_y;
            *y = rotated_x;
            break;
        default:
            *x = rotated_x;
            *y = rotated_y;
            break;
    }
}

static float clamp_float(float value, float low, float high) {
    if (value < low) return low;
    if (value > high) return high;
    return value;
}

/* Window coordinates (points) -> design coordinates. Returns 1 when the point
 * is inside the visible canvas, 0 when it is in a letterbox bar (the output
 * coordinates are still clamped to the canvas edge so drags remain sane). */
static int window_to_logical(micro_native_t *native, float window_x,
                             float window_y, float *logical_x,
                             float *logical_y) {
    if (native == NULL || native->window_width <= 0 || native->window_height <= 0) {
        *logical_x = 0.0f;
        *logical_y = 0.0f;
        return 0;
    }
    float pixel_x = window_x * (float)native->output_width /
                    (float)native->window_width;
    float pixel_y = window_y * (float)native->output_height /
                    (float)native->window_height;
    float rotated_x = (pixel_x - native->view_x) / native->view_scale;
    float rotated_y = (pixel_y - native->view_y) / native->view_scale;
    float max_x = (float)rotated_width(native);
    float max_y = (float)rotated_height(native);
    bool inside = rotated_x >= 0.0f && rotated_x <= max_x &&
                  rotated_y >= 0.0f && rotated_y <= max_y;
    rotated_x = clamp_float(rotated_x, 0.0f, max_x);
    rotated_y = clamp_float(rotated_y, 0.0f, max_y);
    rotated_to_logical(native, rotated_x, rotated_y, logical_x, logical_y);
    *logical_x = clamp_float(*logical_x, 0.0f, (float)native->design_width);
    *logical_y = clamp_float(*logical_y, 0.0f, (float)native->design_height);
    return inside ? 1 : 0;
}

/* Design coordinates -> window coordinates (points). */
static void logical_to_window(micro_native_t *native, float logical_x,
                              float logical_y, float *window_x,
                              float *window_y) {
    float rotated_x = 0.0f;
    float rotated_y = 0.0f;
    logical_to_rotated(native, logical_x, logical_y, &rotated_x, &rotated_y);
    float pixel_x = native->view_x + rotated_x * native->view_scale;
    float pixel_y = native->view_y + rotated_y * native->view_scale;
    if (native->output_width > 0) {
        pixel_x *= (float)native->window_width / (float)native->output_width;
    }
    if (native->output_height > 0) {
        pixel_y *= (float)native->window_height / (float)native->output_height;
    }
    *window_x = pixel_x;
    *window_y = pixel_y;
}

/* -------------------------------------------------------------------------
 * Host icon toolbar
 *
 * SDL renders a 40 px icon strip above the device canvas. It is intentionally
 * outside LVGL: the toolbar never appears in `ui.place` coordinates, never
 * scrolls, and is not affected by the device orientation. Clicks on the strip
 * are consumed by the host instead of reaching the App.
 * ---------------------------------------------------------------------- */

static void set_render_color(SDL_Renderer *renderer, uint32_t rgb) {
    SDL_SetRenderDrawColor(renderer, (Uint8)((rgb >> 16) & 0xFF),
                           (Uint8)((rgb >> 8) & 0xFF),
                           (Uint8)(rgb & 0xFF), 0xFF);
}

/* SDL has no rounded-rectangle primitive; draw one as one-pixel horizontal
 * spans so the toolbar's pill-shaped states scale cleanly with the window. */
static void toolbar_fill_round_rect(SDL_Renderer *renderer, SDL_FRect rect,
                                    float radius) {
    if (rect.w <= 0.0f || rect.h <= 0.0f) return;
    if (radius <= 0.0f) {
        SDL_RenderFillRect(renderer, &rect);
        return;
    }
    if (radius > rect.w * 0.5f) radius = rect.w * 0.5f;
    if (radius > rect.h * 0.5f) radius = rect.h * 0.5f;
    int rows = (int)ceilf(rect.h);
    if (rows < 1) rows = 1;
    float row_height = rect.h / (float)rows;
    float center_y = rect.y + rect.h * 0.5f;
    float straight = rect.h * 0.5f - radius;
    for (int row = 0; row < rows; ++row) {
        float y = rect.y + (float)row * row_height;
        float dy = fabsf(y + row_height * 0.5f - center_y);
        float inset = 0.0f;
        if (dy > straight) {
            float k = dy - straight;
            inset = radius - sqrtf(fmaxf(0.0f, radius * radius - k * k));
        }
        SDL_FRect span = {
            .x = rect.x + inset,
            .y = y,
            .w = rect.w - inset * 2.0f,
            .h = row_height + 0.25f,
        };
        SDL_RenderFillRect(renderer, &span);
    }
}

static void toolbar_outline_rect(SDL_Renderer *renderer, SDL_FRect rect,
                                 float thickness) {
    if (rect.w <= 0.0f || rect.h <= 0.0f) return;
    if (thickness < 1.0f) thickness = 1.0f;
    if (thickness * 2.0f > rect.w) thickness = rect.w * 0.5f;
    if (thickness * 2.0f > rect.h) thickness = rect.h * 0.5f;
    SDL_FRect top = {rect.x, rect.y, rect.w, thickness};
    SDL_FRect bottom = {rect.x, rect.y + rect.h - thickness, rect.w, thickness};
    SDL_FRect left = {rect.x, rect.y + thickness, thickness,
                      rect.h - thickness * 2.0f};
    SDL_FRect right = {rect.x + rect.w - thickness, rect.y + thickness,
                       thickness, rect.h - thickness * 2.0f};
    SDL_RenderFillRect(renderer, &top);
    SDL_RenderFillRect(renderer, &bottom);
    SDL_RenderFillRect(renderer, &left);
    SDL_RenderFillRect(renderer, &right);
}

/* SDL_RenderLine paints a single-pixel line. Offset several parallel lines so
 * the icon strokes keep the same weight as the rest of the host chrome. */
static void toolbar_stroke(SDL_Renderer *renderer, float x1, float y1, float x2,
                           float y2, float thickness) {
    if (thickness < 1.0f) thickness = 1.0f;
    float dx = x2 - x1;
    float dy = y2 - y1;
    float length = sqrtf(dx * dx + dy * dy);
    if (length <= 0.001f) return;
    float nx = -dy / length;
    float ny = dx / length;
    int passes = (int)ceilf(thickness);
    if (passes < 1) passes = 1;
    float step = thickness / (float)passes;
    for (int pass = 0; pass < passes; ++pass) {
        float offset = -thickness * 0.5f + step * ((float)pass + 0.5f);
        SDL_RenderLine(renderer, x1 + nx * offset, y1 + ny * offset,
                       x2 + nx * offset, y2 + ny * offset);
    }
}

static void toolbar_fill_triangle(SDL_Renderer *renderer, float x1, float y1,
                                  float x2, float y2, float x3, float y3) {
    Uint8 r = 255;
    Uint8 g = 255;
    Uint8 b = 255;
    Uint8 a = 255;
    SDL_GetRenderDrawColor(renderer, &r, &g, &b, &a);
    SDL_Vertex vertices[3] = {
        {{x1, y1}, {r, g, b, a}, {0.0f, 0.0f}},
        {{x2, y2}, {r, g, b, a}, {0.0f, 0.0f}},
        {{x3, y3}, {r, g, b, a}, {0.0f, 0.0f}},
    };
    SDL_RenderGeometry(renderer, NULL, vertices, 3, NULL, 0);
}

static void toolbar_arc(SDL_Renderer *renderer, float cx, float cy, float radius,
                        float start_degrees, float end_degrees,
                        float thickness) {
    float sweep = end_degrees - start_degrees;
    int segments = (int)ceilf(fabsf(sweep) / 10.0f);
    if (segments < 2) segments = 2;
    float start = start_degrees * MICRO_DEG_TO_RAD;
    float previous_x = cx + radius * cosf(start);
    float previous_y = cy + radius * sinf(start);
    for (int segment = 1; segment <= segments; ++segment) {
        float angle = (start_degrees + sweep * (float)segment /
                                           (float)segments) * MICRO_DEG_TO_RAD;
        float x = cx + radius * cosf(angle);
        float y = cy + radius * sinf(angle);
        toolbar_stroke(renderer, previous_x, previous_y, x, y, thickness);
        previous_x = x;
        previous_y = y;
    }
}

/* A phone/tablet outline with its home indicator on the long edge makes the
 * two orientation modes readable at 20 px without any text. */
static void toolbar_icon_landscape(SDL_Renderer *renderer, float cx, float cy,
                                   float scale, float thickness) {
    float width = MICRO_TOOLBAR_ICON_SIZE * scale;
    float height = width * 0.65f;
    SDL_FRect device = {cx - width * 0.5f, cy - height * 0.5f, width, height};
    toolbar_outline_rect(renderer, device, thickness);
    float indicator_x = device.x + device.w - 3.5f * scale;
    toolbar_stroke(renderer, indicator_x, cy - 2.6f * scale, indicator_x,
                   cy + 2.6f * scale, thickness);
}

static void toolbar_icon_portrait(SDL_Renderer *renderer, float cx, float cy,
                                  float scale, float thickness) {
    float height = MICRO_TOOLBAR_ICON_SIZE * scale;
    float width = height * 0.65f;
    SDL_FRect device = {cx - width * 0.5f, cy - height * 0.5f, width, height};
    toolbar_outline_rect(renderer, device, thickness);
    float indicator_y = device.y + device.h - 3.5f * scale;
    toolbar_stroke(renderer, cx - 2.6f * scale, indicator_y,
                   cx + 2.6f * scale, indicator_y, thickness);
}

/* Clockwise circular arrow with a 60-degree gap at the top, matching the
 * host's Cmd/Ctrl+R action. */
static void toolbar_icon_rotate(SDL_Renderer *renderer, float cx, float cy,
                                float scale, float thickness) {
    float radius = 7.3f * scale;
    toolbar_arc(renderer, cx, cy, radius, 300.0f, 600.0f, thickness);
    float end = 600.0f * MICRO_DEG_TO_RAD;
    float tip_x = cx + radius * cosf(end);
    float tip_y = cy + radius * sinf(end);
    float tangent_x = -sinf(end);
    float tangent_y = cosf(end);
    float normal_x = -tangent_y;
    float normal_y = tangent_x;
    float arm = 4.2f * scale;
    float wing = 2.4f * scale;
    float base_x = tip_x - tangent_x * arm;
    float base_y = tip_y - tangent_y * arm;
    toolbar_fill_triangle(renderer, tip_x, tip_y,
                          base_x + normal_x * wing, base_y + normal_y * wing,
                          base_x - normal_x * wing, base_y - normal_y * wing);
}

static void toolbar_circle(SDL_Renderer *renderer, float cx, float cy,
                           float radius, float thickness) {
    const int segments = 24;
    float previous_x = cx + radius;
    float previous_y = cy;
    for (int segment = 1; segment <= segments; ++segment) {
        float angle = 2.0f * MICRO_PI * (float)segment / (float)segments;
        float x = cx + radius * cosf(angle);
        float y = cy + radius * sinf(angle);
        toolbar_stroke(renderer, previous_x, previous_y, x, y, thickness);
        previous_x = x;
        previous_y = y;
    }
}

/* A gear reads as "device settings" next to the orientation controls. Eight
 * teeth around two rings stay legible at 20 px and at Retina scale. */
static void toolbar_icon_menu(SDL_Renderer *renderer, float cx, float cy,
                              float scale, float thickness) {
    float outer = 6.7f * scale;
    float inner = 2.5f * scale;
    toolbar_circle(renderer, cx, cy, outer, thickness * 0.9f);
    toolbar_circle(renderer, cx, cy, inner, thickness * 0.9f);
    for (int tooth = 0; tooth < 8; ++tooth) {
        float angle = (float)tooth * 45.0f * MICRO_DEG_TO_RAD;
        float start = outer + 0.2f * scale;
        float end = 9.3f * scale;
        toolbar_stroke(renderer, cx + start * cosf(angle),
                       cy + start * sinf(angle), cx + end * cosf(angle),
                       cy + end * sinf(angle), thickness);
    }
}

static void draw_toolbar_icon(SDL_Renderer *renderer, int action,
                              SDL_FRect button, uint32_t color) {
    float scale = fminf(button.w / 28.0f, button.h / 28.0f);
    if (scale < 0.55f) scale = 0.55f;
    float cx = button.x + button.w * 0.5f;
    float cy = button.y + button.h * 0.5f;
    float thickness = fmaxf(1.0f, 1.7f * scale);
    set_render_color(renderer, color);
    switch (action) {
        case MICRO_TOOLBAR_ACTION_PORTRAIT:
            toolbar_icon_portrait(renderer, cx, cy, scale, thickness);
            break;
        case MICRO_TOOLBAR_ACTION_ROTATE:
            toolbar_icon_rotate(renderer, cx, cy, scale, thickness);
            break;
        case MICRO_TOOLBAR_ACTION_MENU:
            toolbar_icon_menu(renderer, cx, cy, scale, thickness);
            break;
        case MICRO_TOOLBAR_ACTION_LANDSCAPE:
        default:
            toolbar_icon_landscape(renderer, cx, cy, scale, thickness);
            break;
    }
}

/* The debug font is authored at 8 px, which is unreadable on a Retina output
 * when drawn 1:1. Scale the renderer for the hover label and restore it right
 * away so the icon geometry keeps using output pixels. */
static void toolbar_debug_text(SDL_Renderer *renderer, float x, float y,
                               float scale, const char *text, uint32_t color) {
    if (scale < 1.0f) scale = 1.0f;
    set_render_color(renderer, color);
    if (scale > 1.001f) SDL_SetRenderScale(renderer, scale, scale);
    SDL_RenderDebugText(renderer, x / scale, y / scale, text);
    if (scale > 1.001f) SDL_SetRenderScale(renderer, 1.0f, 1.0f);
}

static void toolbar_metrics(const micro_native_t *native, float *scale_x,
                            float *scale_y, float *bar_y, float *bar_height) {
    float sx = native->window_width > 0
                   ? (float)native->output_width / (float)native->window_width
                   : 1.0f;
    float sy = native->window_height > 0
                   ? (float)native->output_height / (float)native->window_height
                   : 1.0f;
    float height = (float)toolbar_output_height(native);
    *scale_x = sx > 0.0f ? sx : 1.0f;
    *scale_y = sy > 0.0f ? sy : 1.0f;
    *bar_height = height;
    *bar_y = 0.0f;
}

static int toolbar_action_count(const micro_native_t *native) {
    return native->host_menu_available ? MICRO_TOOLBAR_ACTION_COUNT
                                       : MICRO_TOOLBAR_ACTION_COUNT - 1;
}

static SDL_FRect toolbar_button_rect(const micro_native_t *native, int index) {
    float sx = 1.0f;
    float sy = 1.0f;
    float bar_y = 0.0f;
    float bar_height = 0.0f;
    toolbar_metrics(native, &sx, &sy, &bar_y, &bar_height);
    float margin = (float)MICRO_TOOLBAR_MARGIN * sx;
    float gap = (float)MICRO_TOOLBAR_BUTTON_GAP * sx;
    float width = (float)MICRO_TOOLBAR_BUTTON_WIDTH * sx;
    float height = (float)MICRO_TOOLBAR_BUTTON_HEIGHT * sy;
    int count = toolbar_action_count(native);
    float available = (float)native->output_width - margin * 2.0f -
                      gap * (float)(count - 1);
    if (available > 0.0f) {
        float fitted = available / (float)count;
        if (width > fitted) width = fitted;
    }
    if (width < 24.0f * sx) width = 24.0f * sx;
    float max_height = bar_height - 8.0f * sy;
    if (height > max_height) height = max_height;
    if (height < 20.0f * sy) height = 20.0f * sy;
    if (height > bar_height) height = bar_height;
    return (SDL_FRect){
        .x = margin + (float)index * (width + gap),
        .y = bar_y + (bar_height - height) * 0.5f,
        .w = width,
        .h = height,
    };
}

static bool point_in_rect(float x, float y, SDL_FRect rect) {
    return x >= rect.x && x <= rect.x + rect.w &&
           y >= rect.y && y <= rect.y + rect.h;
}

/* Returns the toolbar button under a window point, -1 for empty toolbar
 * space, or -2 when the point is not in the toolbar at all. */
static int toolbar_hit(const micro_native_t *native, float window_x,
                       float window_y) {
    if (!native->show_toolbar || native->window_width <= 0 ||
        native->window_height <= 0) {
        return -2;
    }
    float px = window_x * (float)native->output_width /
               (float)native->window_width;
    float py = window_y * (float)native->output_height /
               (float)native->window_height;
    float toolbar_height = (float)toolbar_output_height(native);
    if (py < 0.0f || py >= toolbar_height) return -2;
    for (int index = 0; index < toolbar_action_count(native); ++index) {
        if (point_in_rect(px, py, toolbar_button_rect(native, index))) {
            return index;
        }
    }
    return -1;
}

static void draw_toolbar(micro_native_t *native) {
    if (!native->show_toolbar) return;
    float sx = 1.0f;
    float sy = 1.0f;
    float bar_y = 0.0f;
    float bar_height = 0.0f;
    toolbar_metrics(native, &sx, &sy, &bar_y, &bar_height);
    if (bar_height <= 0.0f) return;

    set_render_color(native->renderer, MICRO_TOOLBAR_BG);
    SDL_FRect bar = {0.0f, bar_y, (float)native->output_width, bar_height};
    SDL_RenderFillRect(native->renderer, &bar);
    set_render_color(native->renderer, MICRO_TOOLBAR_BORDER);
    float border_height = fmaxf(1.0f, sy);
    SDL_FRect border = {0.0f, bar_y + bar_height - border_height,
                        (float)native->output_width, border_height};
    SDL_RenderFillRect(native->renderer, &border);

    /* The two orientation modes are a segmented choice; Rotate is a one-shot
     * action, so a hairline separates them. */
    SDL_FRect portrait = toolbar_button_rect(native, MICRO_TOOLBAR_ACTION_PORTRAIT);
    SDL_FRect rotate = toolbar_button_rect(native, MICRO_TOOLBAR_ACTION_ROTATE);
    float separator_x = (portrait.x + portrait.w + rotate.x) * 0.5f;
    set_render_color(native->renderer, MICRO_TOOLBAR_SEPARATOR);
    SDL_FRect separator = {
        .x = separator_x,
        .y = bar_y + 10.0f * sy,
        .w = fmaxf(1.0f, sx),
        .h = fmaxf(1.0f, bar_height - 20.0f * sy),
    };
    SDL_RenderFillRect(native->renderer, &separator);
    if (native->host_menu_available) {
        SDL_FRect menu = toolbar_button_rect(native, MICRO_TOOLBAR_ACTION_MENU);
        float menu_separator_x = (rotate.x + rotate.w + menu.x) * 0.5f;
        SDL_FRect menu_separator = {
            .x = menu_separator_x,
            .y = separator.y,
            .w = separator.w,
            .h = separator.h,
        };
        SDL_RenderFillRect(native->renderer, &menu_separator);
    }

    static const char *const labels[] = {"Landscape", "Portrait", "Rotate",
                                         "Device menu"};
    for (int index = 0; index < toolbar_action_count(native); ++index) {
        SDL_FRect rect = toolbar_button_rect(native, index);
        bool active = (index == MICRO_TOOLBAR_ACTION_LANDSCAPE && !native->portrait) ||
                      (index == MICRO_TOOLBAR_ACTION_PORTRAIT && native->portrait);
        bool hovered = native->toolbar_hover == index;
        if (active || hovered) {
            uint32_t background = active
                                      ? (hovered ? MICRO_TOOLBAR_ACTIVE_HOVER
                                                 : MICRO_TOOLBAR_ACTIVE)
                                      : MICRO_TOOLBAR_BUTTON_HOVER;
            set_render_color(native->renderer, background);
            toolbar_fill_round_rect(native->renderer, rect,
                                    fminf(rect.h * 0.5f, 8.0f * sy));
        }
        draw_toolbar_icon(native->renderer, index, rect,
                          active || hovered ? MICRO_TOOLBAR_ICON_ACTIVE
                                            : MICRO_TOOLBAR_ICON);
    }

    if (native->toolbar_hover >= 0 &&
        native->toolbar_hover < MICRO_TOOLBAR_ACTION_COUNT) {
        float text_scale = fmaxf(1.0f, fminf(sx, sy));
        if (text_scale > 3.0f) text_scale = 3.0f;
        const char *label = labels[native->toolbar_hover];
        float text_width = (float)strlen(label) *
                           (float)SDL_DEBUG_TEXT_FONT_CHARACTER_SIZE * text_scale;
        float label_x = rotate.x + rotate.w + 12.0f * sx;
        if (native->toolbar_hover == MICRO_TOOLBAR_ACTION_MENU) {
            SDL_FRect menu = toolbar_button_rect(native, MICRO_TOOLBAR_ACTION_MENU);
            label_x = menu.x + menu.w + 12.0f * sx;
        }
        float label_y = (bar_height -
                         (float)SDL_DEBUG_TEXT_FONT_CHARACTER_SIZE * text_scale) *
                        0.5f;
        if (label_x + text_width <=
            (float)native->output_width - (float)MICRO_TOOLBAR_MARGIN * sx) {
            toolbar_debug_text(native->renderer, label_x, label_y, text_scale,
                               label, MICRO_TOOLBAR_MUTED);
        }
    }
}

/* Portable host menu -------------------------------------------------------
 * macOS uses a native NSMenu. Other desktop hosts (and the forced fallback
 * used by tests) get this SDL-drawn popup: a compact list anchored below the
 * Device gear. It is deliberately outside LVGL, so menu clicks never reach
 * the App and it stays visible on the power-off surface. */
typedef enum micro_host_menu_kind {
    MICRO_HOST_MENU_HEADER = 0,
    MICRO_HOST_MENU_STATUS,
    MICRO_HOST_MENU_ACTION,
    MICRO_HOST_MENU_INFO,
} micro_host_menu_kind_t;

typedef struct micro_host_menu_item {
    micro_host_menu_kind_t kind;
    char label[MICRO_HOST_MENU_LABEL_MAX];
    char value[MICRO_HOST_MENU_VALUE_MAX];
    int action;
    bool enabled;
    bool checked;
} host_menu_item_t;

typedef struct micro_host_menu_geometry {
    SDL_FRect panel;
    SDL_FRect content;
    float scale;
    float row_height;
    float pad;
    int visible_rows;
    int scroll_max;
} host_menu_geometry_t;

static void host_menu_add(host_menu_item_t *items, int *count,
                          micro_host_menu_kind_t kind, const char *label,
                          const char *value, int action, bool enabled,
                          bool checked) {
    if (*count >= MICRO_HOST_MENU_ITEM_MAX) return;
    host_menu_item_t *item = &items[(*count)++];
    memset(item, 0, sizeof(*item));
    item->kind = kind;
    item->action = action;
    item->enabled = enabled;
    item->checked = checked;
    snprintf(item->label, sizeof(item->label), "%s", label != NULL ? label : "");
    snprintf(item->value, sizeof(item->value), "%s", value != NULL ? value : "");
}

static int host_menu_build(micro_native_t *native, host_menu_item_t *items) {
    int count = 0;
    host_menu_add(items, &count, MICRO_HOST_MENU_HEADER, "DEVICE", NULL,
                  -1, false, false);

    micro_native_host_state_t state;
    if (!micro_native_host_state(&state)) {
        host_menu_add(items, &count, MICRO_HOST_MENU_INFO,
                      "Emulator unavailable", NULL, -1, false, false);
        host_menu_add(items, &count, MICRO_HOST_MENU_ACTION, "Close", NULL,
                      MICRO_HOST_MENU_ACTION_CLOSE, true, false);
        return count;
    }

    const bool off = state.powered_off != 0;
    host_menu_add(items, &count, MICRO_HOST_MENU_STATUS,
                  off ? "Powered off" : "Running", NULL, -1, false, !off);
    host_menu_add(items, &count, MICRO_HOST_MENU_ACTION, "Power On", NULL,
                  MICRO_HOST_ACTION_POWER_ON, off, false);
    host_menu_add(items, &count, MICRO_HOST_MENU_ACTION, "Power Off", NULL,
                  MICRO_HOST_ACTION_POWER_OFF, !off, false);
    host_menu_add(items, &count, MICRO_HOST_MENU_ACTION, "Force Restart...",
                  NULL, MICRO_HOST_ACTION_FORCE_RESTART, !off, false);

    host_menu_add(items, &count, MICRO_HOST_MENU_HEADER, "DISPLAY", NULL,
                  -1, false, false);
    host_menu_add(items, &count, MICRO_HOST_MENU_ACTION, "Landscape", NULL,
                  MICRO_HOST_ACTION_LANDSCAPE, true, state.portrait == 0);
    host_menu_add(items, &count, MICRO_HOST_MENU_ACTION, "Portrait", NULL,
                  MICRO_HOST_ACTION_PORTRAIT, true, state.portrait != 0);
    host_menu_add(items, &count, MICRO_HOST_MENU_ACTION, "Fit to window",
                  NULL, MICRO_HOST_ACTION_ZOOM_FIT, true, state.zoom_fit != 0);
    char zoom[16];
    if (state.zoom_fit) {
        snprintf(zoom, sizeof zoom, "Fit");
    } else {
        snprintf(zoom, sizeof zoom, "%d%%", state.zoom_percent);
    }
    host_menu_add(items, &count, MICRO_HOST_MENU_ACTION, "Zoom", zoom,
                  MICRO_HOST_MENU_ACTION_ZOOM_CYCLE, true, false);

    host_menu_add(items, &count, MICRO_HOST_MENU_HEADER, "EMULATOR", NULL,
                  -1, false, false);
    host_menu_add(items, &count, MICRO_HOST_MENU_INFO,
                  "ESP32-S3 Touch LCD 7", NULL, -1, false, false);
    char display[32];
    snprintf(display, sizeof display, "Display: %s",
             state.portrait ? "480x800" : "800x480");
    host_menu_add(items, &count, MICRO_HOST_MENU_INFO, display, NULL,
                  -1, false, false);
    host_menu_add(items, &count, MICRO_HOST_MENU_INFO,
                  "Memory: 8 MiB Flash / 8 MiB PSRAM", NULL, -1, false, false);
    host_menu_add(items, &count, MICRO_HOST_MENU_INFO,
                  "Touch: GT911 / CH422G", NULL, -1, false, false);
    host_menu_add(items, &count, MICRO_HOST_MENU_ACTION, "Close", NULL,
                  MICRO_HOST_MENU_ACTION_CLOSE, true, false);
    return count;
}

static void host_menu_layout(micro_native_t *native, int count,
                             host_menu_geometry_t *geometry) {
    float sx = 1.0f;
    float sy = 1.0f;
    float bar_y = 0.0f;
    float bar_height = 0.0f;
    toolbar_metrics(native, &sx, &sy, &bar_y, &bar_height);
    (void)bar_y;
    float scale = fmaxf(1.5f, fminf(fmaxf(sx, sy), 2.5f));
    float pad = MICRO_HOST_MENU_PAD * scale;
    float row_height = MICRO_HOST_MENU_ROW * scale;
    float margin = MICRO_HOST_MENU_MARGIN * scale;
    float panel_width = MICRO_HOST_MENU_WIDTH * scale;
    if (panel_width > (float)native->output_width - margin * 2.0f) {
        panel_width = (float)native->output_width - margin * 2.0f;
    }
    if (panel_width < 160.0f * scale) panel_width = 160.0f * scale;
    if (panel_width > (float)native->output_width) {
        panel_width = (float)native->output_width;
    }

    float panel_y = (float)toolbar_output_height(native) + 4.0f * scale;
    float available = (float)native->output_height - panel_y - margin;
    if (available < row_height + pad * 2.0f) {
        available = row_height + pad * 2.0f;
    }
    float total_height = (float)count * row_height + pad * 2.0f;
    float panel_height = fminf(total_height, available);

    SDL_FRect menu = toolbar_button_rect(native, MICRO_TOOLBAR_ACTION_MENU);
    float panel_x = menu.x + menu.w - panel_width;
    if (panel_x < margin) panel_x = margin;
    float max_x = (float)native->output_width - panel_width - margin;
    if (panel_x > max_x) panel_x = max_x;
    if (panel_x < 0.0f) panel_x = 0.0f;

    geometry->panel = (SDL_FRect){panel_x, panel_y, panel_width, panel_height};
    geometry->content = (SDL_FRect){
        panel_x + pad,
        panel_y + pad,
        panel_width - pad * 2.0f,
        panel_height - pad * 2.0f,
    };
    geometry->scale = scale;
    geometry->row_height = row_height;
    geometry->pad = pad;
    geometry->visible_rows = (int)(geometry->content.h / row_height);
    if (geometry->visible_rows < 1) geometry->visible_rows = 1;
    geometry->scroll_max = count - geometry->visible_rows;
    if (geometry->scroll_max < 0) geometry->scroll_max = 0;
    if (native->sdl_menu_scroll > geometry->scroll_max) {
        native->sdl_menu_scroll = geometry->scroll_max;
    }
    if (native->sdl_menu_scroll < 0) native->sdl_menu_scroll = 0;
}

static void host_menu_event_point(const micro_native_t *native, float window_x,
                                  float window_y, float *output_x,
                                  float *output_y) {
    *output_x = native->window_width > 0
                    ? window_x * (float)native->output_width /
                          (float)native->window_width
                    : window_x;
    *output_y = native->window_height > 0
                    ? window_y * (float)native->output_height /
                          (float)native->window_height
                    : window_y;
}

/* Returns the enabled action row under an output-space point, and reports
 * whether the point is inside the panel at all (clicks outside close it). */
static int host_menu_hit(micro_native_t *native, const host_menu_item_t *items,
                         int count, float x, float y, bool *inside_panel) {
    host_menu_geometry_t geometry;
    host_menu_layout(native, count, &geometry);
    *inside_panel = point_in_rect(x, y, geometry.panel);
    if (!*inside_panel || !point_in_rect(x, y, geometry.content)) return -1;
    int row = native->sdl_menu_scroll +
              (int)((y - geometry.content.y) / geometry.row_height);
    if (row < 0 || row >= count) return -1;
    if (items[row].kind != MICRO_HOST_MENU_ACTION ||
        !items[row].enabled) {
        return -1;
    }
    return row;
}

static void host_menu_close(micro_native_t *native) {
    if (native == NULL || !native->sdl_menu_open) return;
    fprintf(stderr, "host_menu_close\n");
    native->sdl_menu_open = false;
    native->sdl_menu_hover = -1;
    native->sdl_menu_pressed = -1;
    native->sdl_menu_scroll = 0;
    native->view_dirty = true;
}

static void host_menu_open(micro_native_t *native) {
    if (native == NULL || !native->show_toolbar ||
        !native->host_menu_available) {
        return;
    }
    native->sdl_menu_open = true;
    native->sdl_menu_hover = -1;
    native->sdl_menu_pressed = -1;
    native->sdl_menu_scroll = 0;
    native->view_dirty = true;
}

static void host_menu_scroll_to(micro_native_t *native, int index) {
    if (index < 0) return;
    host_menu_item_t items[MICRO_HOST_MENU_ITEM_MAX];
    int count = host_menu_build(native, items);
    host_menu_geometry_t geometry;
    host_menu_layout(native, count, &geometry);
    if (index < native->sdl_menu_scroll) {
        native->sdl_menu_scroll = index;
    }
    if (index >= native->sdl_menu_scroll + geometry.visible_rows) {
        native->sdl_menu_scroll = index - geometry.visible_rows + 1;
    }
    if (native->sdl_menu_scroll > geometry.scroll_max) {
        native->sdl_menu_scroll = geometry.scroll_max;
    }
    if (native->sdl_menu_scroll < 0) native->sdl_menu_scroll = 0;
}

static bool host_menu_confirm_restart(micro_native_t *native) {
    SDL_MessageBoxButtonData buttons[2] = {
        {.flags = SDL_MESSAGEBOX_BUTTON_ESCAPEKEY_DEFAULT,
         .buttonID = 0,
         .text = "Cancel"},
        {.flags = SDL_MESSAGEBOX_BUTTON_RETURNKEY_DEFAULT,
         .buttonID = 1,
         .text = "Force Restart"},
    };
    SDL_MessageBoxData data = {
        .flags = SDL_MESSAGEBOX_WARNING,
        .window = native->window,
        .title = "Force restart the emulator?",
        .message = "The running App will be closed and the device will cold "
                   "boot Home. Installed Apps and storage are kept.",
        .numbuttons = 2,
        .buttons = buttons,
        .colorScheme = NULL,
    };
    int chosen = 0;
    if (!SDL_ShowMessageBox(&data, &chosen)) return false;
    return chosen == 1;
}

static void host_menu_activate(micro_native_t *native, int index) {
    host_menu_item_t items[MICRO_HOST_MENU_ITEM_MAX];
    int count = host_menu_build(native, items);
    if (index < 0 || index >= count) return;
    int action = items[index].action;
    host_menu_close(native);

    switch (action) {
        case MICRO_HOST_ACTION_POWER_ON:
        case MICRO_HOST_ACTION_POWER_OFF:
            micro_native_request_host_action(action);
            break;
        case MICRO_HOST_ACTION_FORCE_RESTART:
            if (host_menu_confirm_restart(native)) {
                micro_native_request_host_action(action);
            }
            break;
        case MICRO_HOST_ACTION_LANDSCAPE:
            micro_native_set_orientation(native, 0);
            break;
        case MICRO_HOST_ACTION_PORTRAIT:
            micro_native_set_orientation(native, 1);
            break;
        case MICRO_HOST_ACTION_ZOOM_FIT:
            micro_native_set_zoom_fit(native);
            break;
        case MICRO_HOST_MENU_ACTION_ZOOM_CYCLE: {
            int next = 75;
            if (!native->zoom_fit) {
                if (native->zoom_percent == 75) next = 100;
                else if (native->zoom_percent == 100) next = 125;
                else if (native->zoom_percent == 125) next = 150;
                else next = 75;
            }
            micro_native_set_zoom(native, next);
            break;
        }
        case MICRO_HOST_MENU_ACTION_CLOSE:
        default:
            break;
    }
    native->view_dirty = true;
}

static int host_menu_first_action(micro_native_t *native,
                                  const host_menu_item_t *items, int count,
                                  int start, int direction) {
    (void)native;
    if (count <= 0) return -1;
    if (start < 0) start = direction > 0 ? -1 : count;
    for (int step = 0; step < count; ++step) {
        start += direction;
        if (start < 0) start = count - 1;
        if (start >= count) start = 0;
        if (items[start].kind == MICRO_HOST_MENU_ACTION &&
            items[start].enabled) {
            return start;
        }
    }
    return -1;
}

static void host_menu_move(micro_native_t *native, int direction, int page) {
    host_menu_item_t items[MICRO_HOST_MENU_ITEM_MAX];
    int count = host_menu_build(native, items);
    int current = native->sdl_menu_hover;
    int next = current;
    int steps = page ? (direction > 0 ? 5 : 5) : 1;
    for (int i = 0; i < steps; ++i) {
        next = host_menu_first_action(native, items, count, next, direction);
        if (next < 0) break;
    }
    native->sdl_menu_hover = next;
    host_menu_scroll_to(native, next);
    native->view_dirty = true;
}

static void host_menu_draw(micro_native_t *native) {
    if (native == NULL || !native->sdl_menu_open) return;
    host_menu_item_t items[MICRO_HOST_MENU_ITEM_MAX];
    int count = host_menu_build(native, items);
    if (count <= 0) return;
    host_menu_geometry_t geometry;
    host_menu_layout(native, count, &geometry);
    SDL_Renderer *renderer = native->renderer;
    float scale = geometry.scale;

    /* Soft drop shadow: SDL has no shadow primitive. */
    SDL_SetRenderDrawBlendMode(renderer, SDL_BLENDMODE_BLEND);
    SDL_SetRenderDrawColor(renderer, 0, 0, 0, 120);
    SDL_FRect shadow = {
        geometry.panel.x + 3.0f * scale,
        geometry.panel.y + 3.0f * scale,
        geometry.panel.w,
        geometry.panel.h,
    };
    toolbar_fill_round_rect(renderer, shadow, 6.0f * scale);
    SDL_SetRenderDrawBlendMode(renderer, SDL_BLENDMODE_NONE);

    set_render_color(renderer, MICRO_HOST_MENU_BORDER);
    toolbar_fill_round_rect(renderer, geometry.panel, 6.0f * scale);
    SDL_FRect inner = {
        geometry.panel.x + fmaxf(1.0f, scale * 0.75f),
        geometry.panel.y + fmaxf(1.0f, scale * 0.75f),
        geometry.panel.w - fmaxf(2.0f, scale * 1.5f),
        geometry.panel.h - fmaxf(2.0f, scale * 1.5f),
    };
    set_render_color(renderer, MICRO_HOST_MENU_BG);
    toolbar_fill_round_rect(renderer, inner, 5.0f * scale);

    SDL_Rect clip = {
        (int)floorf(geometry.content.x),
        (int)floorf(geometry.content.y),
        (int)ceilf(geometry.content.w),
        (int)ceilf(geometry.content.h),
    };
    SDL_SetRenderClipRect(renderer, &clip);

    for (int row = 0; row < count; ++row) {
        float row_y = geometry.content.y -
                      (float)native->sdl_menu_scroll * geometry.row_height +
                      (float)row * geometry.row_height;
        if (row_y + geometry.row_height <= geometry.content.y ||
            row_y >= geometry.content.y + geometry.content.h) {
            continue;
        }
        const host_menu_item_t *item = &items[row];
        bool action = item->kind == MICRO_HOST_MENU_ACTION;
        bool hovered = action && item->enabled &&
                       native->sdl_menu_hover == row;
        bool pressed = action && item->enabled &&
                       native->sdl_menu_pressed == row;
        if (hovered || pressed) {
            set_render_color(renderer, pressed ? MICRO_HOST_MENU_PRESSED
                                               : MICRO_HOST_MENU_HOVER);
            SDL_FRect pill = {
                geometry.content.x,
                row_y + 1.0f * scale,
                geometry.content.w,
                geometry.row_height - 2.0f * scale,
            };
            toolbar_fill_round_rect(renderer, pill, 4.0f * scale);
        }

        float text_y = row_y + (geometry.row_height -
                                (float)SDL_DEBUG_TEXT_FONT_CHARACTER_SIZE *
                                    scale) *
                                   0.5f;
        uint32_t color = MICRO_HOST_MENU_TEXT;
        if (item->kind == MICRO_HOST_MENU_HEADER) {
            color = MICRO_HOST_MENU_HEADER_COLOR;
        } else if (item->kind == MICRO_HOST_MENU_INFO) {
            color = MICRO_HOST_MENU_MUTED;
        } else if (!item->enabled) {
            color = MICRO_HOST_MENU_MUTED;
        }

        if (item->kind == MICRO_HOST_MENU_STATUS) {
            set_render_color(renderer, item->checked ? MICRO_HOST_MENU_RUNNING
                                                     : MICRO_HOST_MENU_OFF);
            SDL_FRect dot = {
                geometry.content.x + 7.0f * scale,
                row_y + geometry.row_height * 0.5f - 3.0f * scale,
                6.0f * scale,
                6.0f * scale,
            };
            toolbar_fill_round_rect(renderer, dot, 3.0f * scale);
            toolbar_debug_text(renderer, geometry.content.x + 18.0f * scale,
                               text_y, scale, item->label, color);
            continue;
        }

        float label_x = geometry.content.x;
        if (item->kind == MICRO_HOST_MENU_ACTION) {
            if (item->checked) {
                toolbar_debug_text(renderer, geometry.content.x + 6.0f * scale,
                                   text_y, scale, "*", MICRO_HOST_MENU_CHECK);
            }
            label_x += 18.0f * scale;
        } else if (item->kind == MICRO_HOST_MENU_INFO) {
            label_x += 6.0f * scale;
        }

        /* Header rows are full-width section labels. */
        toolbar_debug_text(renderer, label_x, text_y, scale, item->label,
                           color);

        if (item->value[0] != '\0') {
            float value_width =
                (float)strlen(item->value) *
                (float)SDL_DEBUG_TEXT_FONT_CHARACTER_SIZE * scale;
            float value_x = geometry.content.x + geometry.content.w -
                            value_width - 4.0f * scale;
            if (value_x > label_x) {
                toolbar_debug_text(renderer, value_x, text_y, scale,
                                   item->value, MICRO_HOST_MENU_MUTED);
            }
        }
    }

    SDL_SetRenderClipRect(renderer, NULL);

    if (geometry.scroll_max > 0) {
        float track_x = geometry.panel.x + geometry.panel.w - 5.0f * scale;
        float track_y = geometry.content.y;
        float track_h = geometry.content.h;
        set_render_color(renderer, MICRO_HOST_MENU_BORDER);
        SDL_FRect track = {track_x, track_y, 2.0f * scale, track_h};
        SDL_RenderFillRect(renderer, &track);
        float thumb_h = track_h * (float)geometry.visible_rows / (float)count;
        if (thumb_h < 12.0f * scale) thumb_h = 12.0f * scale;
        float travel = track_h - thumb_h;
        float thumb_y = track_y + (geometry.scroll_max > 0
                                       ? travel * (float)native->sdl_menu_scroll /
                                             (float)geometry.scroll_max
                                       : 0.0f);
        set_render_color(renderer, MICRO_HOST_MENU_HEADER_COLOR);
        SDL_FRect thumb = {track_x, thumb_y, 2.0f * scale, thumb_h};
        SDL_RenderFillRect(renderer, &thumb);
    }
}

static bool host_menu_handle_event(micro_native_t *native,
                                   const SDL_Event *event) {
    if (native == NULL || !native->sdl_menu_open) return false;
    switch (event->type) {
        case SDL_EVENT_WINDOW_RESIZED:
        case SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED:
        case SDL_EVENT_WINDOW_DISPLAY_SCALE_CHANGED:
        case SDL_EVENT_WINDOW_DISPLAY_CHANGED:
        case SDL_EVENT_WINDOW_MAXIMIZED:
        case SDL_EVENT_WINDOW_RESTORED:
            fprintf(stderr, "menu closed by window event %u\n", event->type);
            host_menu_close(native);
            return false;
        case SDL_EVENT_MOUSE_MOTION: {
            float x = 0.0f;
            float y = 0.0f;
            host_menu_event_point(native, event->motion.x, event->motion.y,
                                  &x, &y);
            host_menu_item_t items[MICRO_HOST_MENU_ITEM_MAX];
            int count = host_menu_build(native, items);
            bool inside = false;
            int hit = host_menu_hit(native, items, count, x, y, &inside);
            if (native->sdl_menu_hover != hit) {
                native->sdl_menu_hover = hit;
                native->view_dirty = true;
            }
            return true;
        }
        case SDL_EVENT_MOUSE_BUTTON_DOWN:
            if (event->button.button != SDL_BUTTON_LEFT) return true;
            float x = 0.0f;
            float y = 0.0f;
            host_menu_event_point(native, event->button.x, event->button.y,
                                  &x, &y);
            host_menu_item_t items[MICRO_HOST_MENU_ITEM_MAX];
            int count = host_menu_build(native, items);
            bool inside = false;
            int hit = host_menu_hit(native, items, count, x, y, &inside);
            if (!inside) {
                native->sdl_menu_swallow_release = true;
                host_menu_close(native);
                return true;
            }
            native->sdl_menu_pressed = hit;
            native->sdl_menu_hover = hit;
            native->view_dirty = true;
            return true;
        case SDL_EVENT_MOUSE_BUTTON_UP:
            if (event->button.button != SDL_BUTTON_LEFT) return true;
            float up_x = 0.0f;
            float up_y = 0.0f;
            host_menu_event_point(native, event->button.x, event->button.y,
                                  &up_x, &up_y);
            host_menu_item_t up_items[MICRO_HOST_MENU_ITEM_MAX];
            int up_count = host_menu_build(native, up_items);
            bool up_inside = false;
            int up_hit = host_menu_hit(native, up_items, up_count, up_x, up_y,
                                       &up_inside);
            int pressed = native->sdl_menu_pressed;
            native->sdl_menu_pressed = -1;
            if (pressed >= 0 && pressed == up_hit) {
                host_menu_activate(native, pressed);
            } else {
                native->sdl_menu_hover = up_inside ? up_hit : -1;
                native->view_dirty = true;
            }
            return true;
        case SDL_EVENT_MOUSE_WHEEL: {
            float x = 0.0f;
            float y = 0.0f;
            host_menu_event_point(native, event->wheel.mouse_x,
                                  event->wheel.mouse_y, &x, &y);
            host_menu_item_t items[MICRO_HOST_MENU_ITEM_MAX];
            int count = host_menu_build(native, items);
            host_menu_geometry_t geometry;
            host_menu_layout(native, count, &geometry);
            if (!point_in_rect(x, y, geometry.panel)) return true;
            if (event->wheel.y > 0.0f) {
                native->sdl_menu_scroll--;
            } else if (event->wheel.y < 0.0f) {
                native->sdl_menu_scroll++;
            }
            if (native->sdl_menu_scroll < 0) native->sdl_menu_scroll = 0;
            if (native->sdl_menu_scroll > geometry.scroll_max) {
                native->sdl_menu_scroll = geometry.scroll_max;
            }
            native->view_dirty = true;
            return true;
        }
        case SDL_EVENT_KEY_DOWN:
            if (event->key.key == SDLK_ESCAPE) {
                host_menu_close(native);
            } else if (event->key.key == SDLK_DOWN) {
                host_menu_move(native, 1, 0);
            } else if (event->key.key == SDLK_UP) {
                host_menu_move(native, -1, 0);
            } else if (event->key.key == SDLK_PAGEDOWN) {
                host_menu_move(native, 1, 1);
            } else if (event->key.key == SDLK_PAGEUP) {
                host_menu_move(native, -1, 1);
            } else if (event->key.key == SDLK_HOME) {
                host_menu_item_t items[MICRO_HOST_MENU_ITEM_MAX];
                int count = host_menu_build(native, items);
                native->sdl_menu_hover =
                    host_menu_first_action(native, items, count, -1, 1);
                host_menu_scroll_to(native, native->sdl_menu_hover);
                native->view_dirty = true;
            } else if (event->key.key == SDLK_END) {
                host_menu_item_t items[MICRO_HOST_MENU_ITEM_MAX];
                int count = host_menu_build(native, items);
                native->sdl_menu_hover =
                    host_menu_first_action(native, items, count, count, -1);
                host_menu_scroll_to(native, native->sdl_menu_hover);
                native->view_dirty = true;
            } else if (event->key.key == SDLK_RETURN ||
                       event->key.key == SDLK_KP_ENTER) {
                if (native->sdl_menu_hover >= 0) {
                    host_menu_activate(native, native->sdl_menu_hover);
                }
            }
            return true;
        case SDL_EVENT_TEXT_INPUT:
            return true;
        default:
            return false;
    }
}

static void open_host_menu(micro_native_t *native) {
    if (native == NULL || !native->host_menu_available) return;
    const char *force_sdl = getenv("MICRO_HOST_MENU_UI");
    bool use_sdl = !micro_host_menu_is_native() ||
                   (force_sdl != NULL && strcmp(force_sdl, "1") == 0);
    if (use_sdl) {
        if (native->sdl_menu_open) {
            host_menu_close(native);
        } else {
            host_menu_open(native);
        }
        return;
    }
    float sx = 1.0f;
    float sy = 1.0f;
    float bar_y = 0.0f;
    float bar_height = 0.0f;
    toolbar_metrics(native, &sx, &sy, &bar_y, &bar_height);
    SDL_FRect menu = toolbar_button_rect(native, MICRO_TOOLBAR_ACTION_MENU);
    /* AppKit expects content-view points; the toolbar rects are in renderer
     * output pixels (Retina is typically 2x). */
    micro_host_menu_open(native->window, (menu.x + menu.w * 0.5f) / sx,
                         (float)toolbar_output_height(native) / sy);
}

static void apply_toolbar_action(micro_native_t *native, int action) {
    if (action == MICRO_TOOLBAR_ACTION_MENU) {
        open_host_menu(native);
    } else if (action == MICRO_TOOLBAR_ACTION_ROTATE) {
        host_menu_close(native);
        micro_native_set_orientation(native, native->portrait ? 0 : 1);
    } else if (action == MICRO_TOOLBAR_ACTION_PORTRAIT) {
        host_menu_close(native);
        micro_native_set_orientation(native, 1);
    } else if (action == MICRO_TOOLBAR_ACTION_LANDSCAPE) {
        host_menu_close(native);
        micro_native_set_orientation(native, 0);
    }
}

static void present_frame(micro_native_t *native) {
    if (native == NULL || native->renderer == NULL || native->texture == NULL) {
        return;
    }
    SDL_SetRenderDrawColor(native->renderer,
                           (Uint8)((MICRO_PRESENT_BG >> 16) & 0xFF),
                           (Uint8)((MICRO_PRESENT_BG >> 8) & 0xFF),
                           (Uint8)(MICRO_PRESENT_BG & 0xFF), 0xFF);
    SDL_RenderClear(native->renderer);
    SDL_FRect destination = {
        .x = ((float)native->output_width -
              (float)native->design_width * native->view_scale) * 0.5f,
        .y = native->view_y,
        .w = (float)native->design_width * native->view_scale,
        .h = (float)native->design_height * native->view_scale,
    };
    SDL_RenderTexture(native->renderer, native->texture, NULL, &destination);
    draw_toolbar(native);
    host_menu_draw(native);
    SDL_RenderPresent(native->renderer);
    native->view_dirty = false;
}

static void update_window_title(micro_native_t *native) {
    if (native == NULL || native->window == NULL) return;
    char title[96];
    snprintf(title, sizeof title, "Micro App - %s - %d%%",
             native->portrait ? "Portrait" : "Landscape", native->zoom_percent);
    SDL_SetWindowTitle(native->window, title);
}

static void display_usable_bounds(micro_native_t *native, SDL_Rect *bounds) {
    SDL_DisplayID display = SDL_GetDisplayForWindow(native->window);
    if (display == 0) display = SDL_GetPrimaryDisplay();
    if (display == 0 || !SDL_GetDisplayUsableBounds(display, bounds)) {
        bounds->x = 0;
        bounds->y = 0;
        bounds->w = 1280;
        bounds->h = 800;
    }
}

static void resize_window_centered(micro_native_t *native, int width, int height) {
    if (native == NULL || native->window == NULL || native->hidden) {
        return;
    }
    SDL_Rect bounds;
    display_usable_bounds(native, &bounds);
    int max_width = (int)((float)bounds.w * MICRO_WINDOW_MARGIN);
    int max_height = (int)((float)bounds.h * MICRO_WINDOW_MARGIN);
    if (max_width < MICRO_MIN_WINDOW_WIDTH) max_width = MICRO_MIN_WINDOW_WIDTH;
    if (max_height < MICRO_MIN_WINDOW_HEIGHT) max_height = MICRO_MIN_WINDOW_HEIGHT;
    if (width > max_width) width = max_width;
    if (height > max_height) height = max_height;
    if (width < MICRO_MIN_WINDOW_WIDTH) width = MICRO_MIN_WINDOW_WIDTH;
    if (height < MICRO_MIN_WINDOW_HEIGHT) height = MICRO_MIN_WINDOW_HEIGHT;

    native->resizing_window = true;
    SDL_SetWindowSize(native->window, width, height);
    SDL_SetWindowPosition(native->window,
                          bounds.x + (bounds.w - width) / 2,
                          bounds.y + (bounds.h - height) / 2);
    native->resizing_window = false;
    refresh_presentation_metrics(native);
}

/* Swap the LVGL device surface to `width` x `height` without recreating the
 * runtime or the App tree. The texture, partial display buffer and app-bar
 * content box are resized together, then the existing `ui.place` anchors are
 * laid out again against the new parent dimensions. */
static int resize_device_surface(micro_native_t *native, int width, int height) {
    if (native == NULL || native->display == NULL || native->renderer == NULL ||
        width <= 0 || height <= 0) {
        return 0;
    }
    size_t buffer_size = (size_t)width * 40U * 4U;
    uint8_t *buffer = realloc(native->display_buffer, buffer_size);
    if (buffer == NULL) return 0;
    native->display_buffer = buffer;
    native->display_buffer_size = buffer_size;

    SDL_Texture *texture = SDL_CreateTexture(native->renderer, SDL_PIXELFORMAT_ARGB8888,
                                             SDL_TEXTUREACCESS_STREAMING, width, height);
    if (texture == NULL) return 0;
    if (native->texture != NULL) SDL_DestroyTexture(native->texture);
    native->texture = texture;

    native->design_width = width;
    native->design_height = height;
    /* The logical viewport is canonical; no bitmap rotation is needed. */
    native->rotation = 0;
    lv_display_set_resolution(native->display, width, height);
    lv_display_set_buffers(native->display, native->display_buffer, NULL,
                           native->display_buffer_size, LV_DISPLAY_RENDER_MODE_PARTIAL);
    micro_appbar_set_viewport(width, height);
    lv_obj_invalidate(lv_screen_active());
    return 1;
}

int micro_native_set_orientation(micro_native_t *native, int portrait) {
    if (native == NULL) return 0;
    portrait = portrait ? 1 : 0;
    int width = portrait ? native->landscape_height : native->landscape_width;
    int height = portrait ? native->landscape_width : native->landscape_height;
    if (native->portrait == (bool)portrait && native->design_width == width &&
        native->design_height == height) {
        return 1;
    }
    native->portrait = portrait != 0;
    if (!resize_device_surface(native, width, height)) return 0;
    if (!native->hidden && native->zoom_fit) {
        /* Fit is a mode, not a one-shot percentage: a new orientation must
         * recompute the window size against the new aspect ratio. */
        return micro_native_set_zoom_fit(native);
    }
    if (!native->hidden) {
        int window_width = (int)lrintf((float)native->design_width *
                                       (float)native->zoom_percent / 100.0f);
        int window_height = (int)lrintf((float)native->design_height *
                                        (float)native->zoom_percent / 100.0f) +
                            MICRO_TOOLBAR_HEIGHT;
        resize_window_centered(native, window_width, window_height);
        int actual = (int)lrintf(native->view_scale * 100.0f);
        if (actual < MICRO_MIN_ZOOM_PERCENT) actual = MICRO_MIN_ZOOM_PERCENT;
        if (actual > MICRO_MAX_ZOOM_PERCENT) actual = MICRO_MAX_ZOOM_PERCENT;
        native->zoom_percent = actual;
    } else {
        refresh_presentation_metrics(native);
    }
    update_window_title(native);
    return 1;
}

int micro_native_get_orientation(micro_native_t *native) {
    return native != NULL && native->portrait ? 1 : 0;
}

int micro_native_set_rotation(micro_native_t *native, int degrees) {
    if (native == NULL) return 0;
    degrees %= 360;
    if (degrees < 0) degrees += 360;
    if (degrees % 90 != 0) return 0;
    /* The desktop toolbar has two logical orientations. 90/270 map to portrait;
     * 0/180 map to landscape (a 180-degree bitmap flip is not a new layout). */
    return micro_native_set_orientation(native, (degrees % 180) == 90);
}

int micro_native_get_rotation(micro_native_t *native) {
    return native != NULL && native->portrait ? 90 : 0;
}

int micro_native_set_zoom(micro_native_t *native, int percent) {
    if (native == NULL) return 0;
    if (percent <= 0) return micro_native_set_zoom_fit(native);
    native->zoom_fit = false;
    if (percent < MICRO_MIN_ZOOM_PERCENT) percent = MICRO_MIN_ZOOM_PERCENT;
    if (percent > MICRO_MAX_ZOOM_PERCENT) percent = MICRO_MAX_ZOOM_PERCENT;
    native->zoom_percent = percent;
    if (!native->hidden) {
        int width = (int)lrintf((float)rotated_width(native) *
                                (float)percent / 100.0f);
        int height = (int)lrintf((float)rotated_height(native) *
                                 (float)percent / 100.0f) + MICRO_TOOLBAR_HEIGHT;
        resize_window_centered(native, width, height);
        int actual = (int)lrintf(native->view_scale * 100.0f);
        if (actual < MICRO_MIN_ZOOM_PERCENT) actual = MICRO_MIN_ZOOM_PERCENT;
        if (actual > MICRO_MAX_ZOOM_PERCENT) actual = MICRO_MAX_ZOOM_PERCENT;
        native->zoom_percent = actual;
    } else {
        refresh_presentation_metrics(native);
    }
    update_window_title(native);
    return 1;
}

int micro_native_set_zoom_fit(micro_native_t *native) {
    if (native == NULL) return 0;
    if (native->hidden) {
        refresh_presentation_metrics(native);
        native->zoom_percent = (int)lrintf(native->view_scale * 100.0f);
        native->zoom_fit = true;
        return 1;
    }
    SDL_Rect bounds;
    display_usable_bounds(native, &bounds);
    float width_scale = (float)bounds.w * MICRO_WINDOW_MARGIN /
                        (float)rotated_width(native);
    float available_height = (float)bounds.h * MICRO_WINDOW_MARGIN -
                             (float)MICRO_TOOLBAR_HEIGHT;
    if (available_height < 1.0f) available_height = 1.0f;
    float height_scale = available_height / (float)rotated_height(native);
    float scale = width_scale < height_scale ? width_scale : height_scale;
    int percent = (int)lrintf(scale * 100.0f);
    if (percent < MICRO_MIN_ZOOM_PERCENT) percent = MICRO_MIN_ZOOM_PERCENT;
    if (percent > MICRO_MAX_ZOOM_PERCENT) percent = MICRO_MAX_ZOOM_PERCENT;
    int result = micro_native_set_zoom(native, percent);
    if (result) native->zoom_fit = true;
    return result;
}

int micro_native_get_zoom(micro_native_t *native) {
    return native != NULL ? native->zoom_percent : 0;
}

int micro_native_open_host_menu(micro_native_t *native) {
    if (native == NULL || !native->host_menu_available) return 0;
    open_host_menu(native);
    return 1;
}

int micro_native_host_state(micro_native_host_state_t *state) {
    if (state == NULL || s_active_native == NULL) return 0;
    state->powered_off = micro_appbar_power_off_active();
    state->portrait = s_active_native->portrait ? 1 : 0;
    state->zoom_fit = s_active_native->zoom_fit ? 1 : 0;
    state->zoom_percent = s_active_native->zoom_percent;
    return 1;
}

int micro_native_window_to_logical(micro_native_t *native, double window_x,
                                   double window_y, double *logical_x,
                                   double *logical_y) {
    if (native == NULL || logical_x == NULL || logical_y == NULL) return 0;
    float x = 0.0f;
    float y = 0.0f;
    int inside = window_to_logical(native, (float)window_x, (float)window_y, &x, &y);
    *logical_x = (double)x;
    *logical_y = (double)y;
    return inside;
}

int micro_native_logical_to_window(micro_native_t *native, double logical_x,
                                   double logical_y, double *window_x,
                                   double *window_y) {
    if (native == NULL || window_x == NULL || window_y == NULL) return 0;
    float x = 0.0f;
    float y = 0.0f;
    logical_to_window(native, (float)logical_x, (float)logical_y, &x, &y);
    *window_x = (double)x;
    *window_y = (double)y;
    return logical_x >= 0.0 && logical_y >= 0.0 &&
           logical_x <= (double)native->design_width &&
           logical_y <= (double)native->design_height;
}

int micro_native_object_rect(micro_native_t *native, uint32_t node_id,
                             int *x, int *y, int *width, int *height) {
    if (native == NULL || node_id >= MICRO_MAX_NODES ||
        native->objects[node_id] == NULL) {
        return 0;
    }
    lv_area_t area;
    lv_obj_get_coords(native->objects[node_id], &area);
    if (x != NULL) *x = area.x1;
    if (y != NULL) *y = area.y1;
    if (width != NULL) *width = area.x2 - area.x1 + 1;
    if (height != NULL) *height = area.y2 - area.y1 + 1;
    return 1;
}

static bool modifier_down(SDL_Keymod modifiers) {
    return (modifiers & (SDL_KMOD_GUI | SDL_KMOD_CTRL)) != 0;
}

/* Host-level shortcuts. They are deliberately Cmd/Ctrl-qualified so typing in
 * an App textarea is never intercepted. Return true when the key was used. */
static bool handle_presentation_key(micro_native_t *native,
                                    const SDL_KeyboardEvent *key) {
    if (native == NULL || key == NULL || !key->down || !modifier_down(key->mod)) {
        return false;
    }
    switch (key->key) {
        case SDLK_R:
            return micro_native_set_orientation(
                       native, native->portrait ? 0 : 1) != 0;
        case SDLK_0:
            return micro_native_set_zoom_fit(native) != 0;
        case SDLK_1:
            return micro_native_set_zoom(native, 100) != 0;
        case SDLK_EQUALS:
        case SDLK_PLUS:
        case SDLK_KP_PLUS:
            return micro_native_set_zoom(
                       native, native->zoom_percent + MICRO_ZOOM_STEP_PERCENT) != 0;
        case SDLK_MINUS:
        case SDLK_KP_MINUS:
            return micro_native_set_zoom(
                       native, native->zoom_percent - MICRO_ZOOM_STEP_PERCENT) != 0;
        case SDLK_COMMA:
            if (!native->host_menu_available) return false;
            open_host_menu(native);
            return true;
        default:
            return false;
    }
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
    present_frame(native);
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

static void textarea_focus_callback(lv_event_t *event) {
    micro_click_context_t *context = lv_event_get_user_data(event);
    if (context == NULL || context->native == NULL) return;
    lv_obj_t *target = lv_event_get_target(event);
    if (lv_event_get_code(event) == LV_EVENT_DEFOCUSED) {
        if (context->native->focused_textarea == target) {
            context->native->focused_textarea = NULL;
        }
    } else {
        context->native->focused_textarea = target;
    }
}

/* Find the textarea under a window-space point, walking up from a label or
 * other child object. Clicking anywhere else clears the focus. */
static lv_obj_t *textarea_at_point(lv_point_t point) {
    lv_obj_t *object = lv_indev_search_obj(lv_screen_active(), &point);
    while (object != NULL && !lv_obj_check_type(object, &lv_textarea_class)) {
        object = lv_obj_get_parent(object);
    }
    return object;
}

static void textarea_handle_key(micro_native_t *native, const SDL_KeyboardEvent *key) {
    lv_obj_t *textarea = native->focused_textarea;
    if (textarea == NULL) return;
    const bool command = (key->mod & (SDL_KMOD_GUI | SDL_KMOD_CTRL)) != 0;
    if (command && key->key == SDLK_V) {
        native->pending_paste = true;
        return;
    }
    if (command) return;
    switch (key->key) {
        case SDLK_BACKSPACE: lv_textarea_delete_char(textarea); break;
        case SDLK_DELETE: lv_textarea_delete_char_forward(textarea); break;
        case SDLK_LEFT: lv_textarea_cursor_left(textarea); break;
        case SDLK_RIGHT: lv_textarea_cursor_right(textarea); break;
        case SDLK_HOME: lv_textarea_set_cursor_pos(textarea, 0); break;
        case SDLK_END: lv_textarea_set_cursor_pos(textarea, LV_TEXTAREA_CURSOR_LAST); break;
        case SDLK_ESCAPE: native->focused_textarea = NULL; break;
        default: break;
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
    /* A root node mounts into the OS app-bar content container, not the raw
     * screen, so every launched App sits below the system bar. */
    if (parent_id == MICRO_NO_PARENT) {
        micro_appbar_ensure();
        return micro_appbar_content();
    }
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

/* Shared OS icons paint a dedicated 42-glyph Font Awesome subset instead of the
 * full Montserrat symbol faces. The App's uiSans faces do not carry those
 * private-use glyphs, and the subsets cut the flash cost from ~110 KiB to
 * ~21 KiB. Generated by scripts/generate-icon-fonts.py. */
extern const lv_font_t micro_icons_12;
extern const lv_font_t micro_icons_14;
extern const lv_font_t micro_icons_18;
extern const lv_font_t micro_icons_24;
extern const lv_font_t micro_icons_32;

static const lv_font_t *icon_font_for_size(uint32_t size_px) {
    if (size_px >= 32U) return &micro_icons_32;
    if (size_px >= 24U) return &micro_icons_24;
    if (size_px >= 18U) return &micro_icons_18;
    if (size_px >= 14U) return &micro_icons_14;
    return &micro_icons_12;
}

static uint32_t icon_tone_color(const char *tone) {
    if (tone == NULL) return 0x101820;
    if (strcmp(tone, "muted") == 0) return 0x84937D;
    if (strcmp(tone, "accent") == 0) return 0xC0683B;
    if (strcmp(tone, "inverse") == 0) return 0xFFFDF7;
    return 0x101820;
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
    SDL_WindowFlags flags = hidden ? SDL_WINDOW_HIDDEN : SDL_WINDOW_RESIZABLE;
    int window_height = height + (hidden ? 0 : MICRO_TOOLBAR_HEIGHT);
    native->window = SDL_CreateWindow("Micro App", width, window_height, flags);
    if (native->window == NULL) goto sdl_error;
    s_active_native = native;
    if (!hidden) {
        /* AppKit is preferred on macOS; every desktop has the SDL fallback even
         * if a native menu backend is unavailable. */
        (void)micro_host_menu_install();
        native->host_menu_available = 1;
    }
    native->design_width = width;
    native->design_height = height;
    native->landscape_width = width;
    native->landscape_height = height;
    native->hidden = hidden != 0;
    native->show_toolbar = hidden == 0;
    native->toolbar_hover = -1;
    native->toolbar_pressed = -1;
    native->rotation = 0;
    native->zoom_percent = 100;
    native->window_id = SDL_GetWindowID(native->window);
    if (!hidden) {
        SDL_SetWindowMinimumSize(native->window, MICRO_MIN_WINDOW_WIDTH,
                                 MICRO_MIN_WINDOW_HEIGHT);
        SDL_SetWindowPosition(native->window, SDL_WINDOWPOS_CENTERED,
                              SDL_WINDOWPOS_CENTERED);
    }
    /* Desktop textareas need SDL text input for typing and IME events. */
    SDL_StartTextInput(native->window);
    native->renderer = SDL_CreateRenderer(native->window, NULL);
    if (native->renderer == NULL) goto sdl_error;
    native->texture = SDL_CreateTexture(native->renderer, SDL_PIXELFORMAT_ARGB8888, SDL_TEXTUREACCESS_STREAMING, width, height);
    if (native->texture == NULL) goto sdl_error;
    refresh_presentation_metrics(native);
    update_window_title(native);

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
    /* Create the persistent app bar/content pair once for this display. The
     * shell and every installed App mount into its content container; only the
     * single-App debug viewport hides the bar via set_app_chrome. */
    micro_appbar_ensure();
    return native;

sdl_error:
    copy_error(error, error_length, SDL_GetError());
    micro_native_destroy(native);
    return NULL;
}

void micro_native_destroy(micro_native_t *native) {
    if (native == NULL) return;
    if (s_active_native == native) s_active_native = NULL;
    /* Drop the app bar's cached LVGL pointers before the display tree is
     * destroyed, so a later host instance starts clean. */
    micro_appbar_reset();
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
    /* Clear only the App content; the OS app bar is persistent across runtime
     * switches (shell -> app -> shell). */
    micro_appbar_ensure();
    lv_obj_clean(micro_appbar_content());
    memset(native->objects, 0, sizeof(native->objects));
    memset(native->text_targets, 0, sizeof(native->text_targets));
    memset(native->needles, 0, sizeof(native->needles));
    native->input_read = 0;
    native->input_write = 0;
    native->tabview = NULL;
    native->tab_target = NULL;
    memset(native->fill_height, 0, sizeof(native->fill_height));
    native->focused_textarea = NULL;
    native->pending_paste = false;
    native->slider_read = 0;
    native->slider_write = 0;
    native->checkbox_read = 0;
    native->checkbox_write = 0;
    native->dropdown_read = 0;
    native->dropdown_write = 0;
    native->roller_read = 0;
    native->roller_write = 0;
    memset(native->layout_specs, 0, sizeof(native->layout_specs));
    memset(native->fill_natural_h, 0, sizeof(native->fill_natural_h));
    return 1;
}

/* Whether the App content container is on screen. Power-off hides it; the
 * headless smoke asserts that waking reveals it again. */
int micro_native_appbar_content_visible(micro_native_t *native) {
    (void)native;
    return micro_appbar_content_visible();
}

/* -------------------------------------------------------------------------
 * Mouse-wheel / trackpad scrolling
 *
 * A mouse wheel and a two-finger trackpad swipe arrive as SDL mouse-wheel
 * events, not pointer drags, so LVGL's pointer indev never sees them. Resolve
 * the deepest object under the cursor, then walk up to the nearest scrollable
 * ancestor that still has room in the wheel direction and scroll it. This
 * keeps the scrollbar of the active tab page (and any future scroll container)
 * moving exactly as it does under a finger drag.
 * ---------------------------------------------------------------------- */

static bool object_contains_point(lv_obj_t *object, const lv_point_t *point) {
    lv_area_t area;
    lv_obj_get_coords(object, &area);
    return point->x >= area.x1 && point->x <= area.x2 &&
           point->y >= area.y1 && point->y <= area.y2;
}

static lv_obj_t *object_at_point(lv_obj_t *object, const lv_point_t *point) {
    if (object == NULL || lv_obj_has_flag(object, LV_OBJ_FLAG_HIDDEN)) return NULL;
    if (!object_contains_point(object, point)) return NULL;
    for (int32_t index = (int32_t)lv_obj_get_child_count(object) - 1; index >= 0;
         --index) {
        lv_obj_t *found = object_at_point(lv_obj_get_child(object, index), point);
        if (found != NULL) return found;
    }
    return object;
}

/* Scroll `object` or its nearest scrollable ancestor by `delta` on one axis.
 * Returning false means every scrollable ancestor is already at its edge, so
 * the caller can drop the accumulated sub-pixel remainder instead of letting
 * it fire later over a different widget. */
static bool scroll_axis(lv_obj_t *object, lv_dir_t direction, int32_t delta) {
    if (delta == 0) return false;
    for (lv_obj_t *current = object; current != NULL;
         current = lv_obj_get_parent(current)) {
        if (!lv_obj_has_flag(current, LV_OBJ_FLAG_SCROLLABLE)) continue;
        if ((lv_obj_get_scroll_dir(current) & direction) == 0) continue;
        int32_t room;
        if (direction == LV_DIR_VER) {
            /* Positive dy shows the content above (scroll up); negative dy
             * scrolls down. */
            room = delta > 0 ? lv_obj_get_scroll_top(current)
                             : lv_obj_get_scroll_bottom(current);
        } else {
            room = delta > 0 ? lv_obj_get_scroll_right(current)
                             : lv_obj_get_scroll_left(current);
        }
        if (room <= 0) continue;
        if (direction == LV_DIR_VER) {
            lv_obj_scroll_by_bounded(current, 0, delta, LV_ANIM_OFF);
        } else {
            lv_obj_scroll_by_bounded(current, delta, 0, LV_ANIM_OFF);
        }
        return true;
    }
    return false;
}

static void scroll_with_wheel(micro_native_t *native, float window_x,
                              float window_y, float wheel_x, float wheel_y) {
    if (native == NULL) return;
    /* The toolbar is host chrome above the canvas; never scroll an App for a
     * wheel event over it. */
    if (toolbar_hit(native, window_x, window_y) != -2) return;
    float logical_x = 0.0f;
    float logical_y = 0.0f;
    if (!window_to_logical(native, window_x, window_y, &logical_x, &logical_y)) {
        return;
    }
    lv_obj_update_layout(lv_screen_active());
    lv_point_t point = {
        .x = (lv_coord_t)lrintf(logical_x),
        .y = (lv_coord_t)lrintf(logical_y),
    };
    lv_obj_t *target = object_at_point(lv_screen_active(), &point);
    if (target == NULL) {
        native->wheel_pixels_x = 0.0f;
        native->wheel_pixels_y = 0.0f;
        return;
    }
    /* SDL and LVGL agree on the wheel sign: positive Y is wheel-up / show
     * earlier content, negative Y is wheel-down / show later content. */
    native->wheel_pixels_x += wheel_x * MICRO_WHEEL_STEP_PX;
    native->wheel_pixels_y += wheel_y * MICRO_WHEEL_STEP_PX;
    int32_t dx = (int32_t)truncf(native->wheel_pixels_x);
    int32_t dy = (int32_t)truncf(native->wheel_pixels_y);
    native->wheel_pixels_x -= (float)dx;
    native->wheel_pixels_y -= (float)dy;
    if (dx != 0 && !scroll_axis(target, LV_DIR_HOR, dx)) {
        native->wheel_pixels_x = 0.0f;
    }
    if (dy != 0 && !scroll_axis(target, LV_DIR_VER, dy)) {
        native->wheel_pixels_y = 0.0f;
    }
}

static void push_host_action(micro_native_t *native, int action) {
    unsigned next = (native->host_action_write + 1U) % MICRO_HOST_ACTION_CAPACITY;
    if (next == native->host_action_read) return;
    native->host_actions[native->host_action_write] = action;
    native->host_action_write = next;
}

void micro_native_request_host_action(int action) {
    if (action < 0 || action > MICRO_HOST_ACTION_ZOOM_150) return;
    SDL_Event event;
    SDL_zero(event);
    event.type = MICRO_HOST_ACTION_EVENT;
    event.user.code = 1;
    event.user.data1 = (void *)(intptr_t)action;
    SDL_PushEvent(&event);
}

int micro_native_take_host_action(micro_native_t *native, int *action) {
    if (native == NULL || action == NULL ||
        native->host_action_read == native->host_action_write) {
        return 0;
    }
    *action = native->host_actions[native->host_action_read];
    native->host_action_read =
        (native->host_action_read + 1U) % MICRO_HOST_ACTION_CAPACITY;
    return 1;
}

int micro_native_poll(micro_native_t *native) {
    SDL_Event event;
    while (SDL_PollEvent(&event)) {
        if (native->sdl_menu_swallow_release &&
            event.type == SDL_EVENT_MOUSE_BUTTON_UP &&
            event.button.button == SDL_BUTTON_LEFT) {
            native->sdl_menu_swallow_release = false;
            continue;
        }
        if (event.type == MICRO_HOST_ACTION_EVENT) {
            if (event.user.code == 1) {
                push_host_action(native, (int)(intptr_t)event.user.data1);
            }
            continue;
        }
        if (host_menu_handle_event(native, &event)) continue;
        switch (event.type) {
            case SDL_EVENT_QUIT: native->quit = true; break;
            case SDL_EVENT_MOUSE_MOTION: {
                int toolbar = toolbar_hit(native, event.motion.x, event.motion.y);
                if (toolbar != -2) {
                    if (native->toolbar_hover != toolbar) {
                        native->toolbar_hover = toolbar;
                        native->view_dirty = true;
                    }
                    /* The toolbar is host chrome: never forward its motion to
                     * LVGL, even while a toolbar button is held. */
                    break;
                }
                if (native->toolbar_hover != -1) {
                    native->toolbar_hover = -1;
                    native->view_dirty = true;
                }
                float logical_x = 0.0f;
                float logical_y = 0.0f;
                int inside = window_to_logical(native, event.motion.x,
                                               event.motion.y, &logical_x,
                                               &logical_y);
                /* Outside the canvas, only clamp while a drag is in progress
                 * so a click in the letterbox never lands on an edge widget. */
                if (inside || native->pointer_pressed) {
                    native->pointer_position.x = (lv_coord_t)lrintf(logical_x);
                    native->pointer_position.y = (lv_coord_t)lrintf(logical_y);
                    lv_indev_read(native->pointer);
                }
                break;
            }
            case SDL_EVENT_MOUSE_BUTTON_DOWN: {
                if (event.button.button == SDL_BUTTON_LEFT) {
                    int toolbar = toolbar_hit(native, event.button.x, event.button.y);
                    if (toolbar != -2) {
                        native->toolbar_pressed = toolbar;
                        if (toolbar >= 0) {
                            native->toolbar_hover = toolbar;
                            native->view_dirty = true;
                        }
                        break;
                    }
                    native->toolbar_hover = -1;
                    float logical_x = 0.0f;
                    float logical_y = 0.0f;
                    if (!window_to_logical(native, event.button.x, event.button.y,
                                           &logical_x, &logical_y)) {
                        break;
                    }
                    native->pointer_position.x = (lv_coord_t)lrintf(logical_x);
                    native->pointer_position.y = (lv_coord_t)lrintf(logical_y);
                    native->focused_textarea = textarea_at_point(native->pointer_position);
                    native->pointer_pressed = true;
                    lv_indev_read(native->pointer);
                    native->gesture_active = true;
                    /* The back gesture belongs to the physical screen edge, so
                     * measure it in on-screen canvas coordinates. */
                    float rotated_x = 0.0f;
                    float rotated_y = 0.0f;
                    logical_to_rotated(native, logical_x, logical_y, &rotated_x,
                                       &rotated_y);
                    native->gesture_start.x = (lv_coord_t)lrintf(rotated_x);
                    native->gesture_start.y = (lv_coord_t)lrintf(rotated_y);
                    native->gesture_edge = rotated_x < MICRO_NATIVE_EDGE_ZONE
                                               ? 1
                                               : (rotated_x >
                                                          ((float)rotated_width(native) -
                                                           MICRO_NATIVE_EDGE_ZONE)
                                                      ? 2
                                                      : 0);
                }
                break;
            }
            case SDL_EVENT_TEXT_INPUT:
                if (native->pending_paste) {
                    native->pending_paste = false;
                } else if (native->focused_textarea != NULL && event.text.text != NULL) {
                    lv_textarea_add_text(native->focused_textarea, event.text.text);
                }
                break;
            case SDL_EVENT_KEY_DOWN:
                if (!handle_presentation_key(native, &event.key)) {
                    textarea_handle_key(native, &event.key);
                }
                break;
            case SDL_EVENT_MOUSE_WHEEL:
                if (modifier_down(SDL_GetModState())) {
                    if (event.wheel.y > 0.0f) {
                        micro_native_set_zoom(native, native->zoom_percent +
                                                       MICRO_ZOOM_STEP_PERCENT);
                    } else if (event.wheel.y < 0.0f) {
                        micro_native_set_zoom(native, native->zoom_percent -
                                                       MICRO_ZOOM_STEP_PERCENT);
                    }
                } else {
                    /* Plain wheel / trackpad: scroll the scrollable widget
                     * under the cursor (the active tab page). Cmd/Ctrl+wheel
                     * stays reserved for presentation zoom. */
                    float wheel_x = event.wheel.x;
                    float wheel_y = event.wheel.y;
                    if (event.wheel.direction == SDL_MOUSEWHEEL_FLIPPED) {
                        wheel_x = -wheel_x;
                        wheel_y = -wheel_y;
                    }
                    scroll_with_wheel(native, event.wheel.mouse_x,
                                      event.wheel.mouse_y, wheel_x, wheel_y);
                }
                break;
            case SDL_EVENT_MOUSE_BUTTON_UP:
                if (event.button.button == SDL_BUTTON_LEFT) {
                    int toolbar = toolbar_hit(native, event.button.x, event.button.y);
                    if (native->toolbar_pressed >= 0) {
                        int pressed = native->toolbar_pressed;
                        native->toolbar_pressed = -1;
                        native->view_dirty = true;
                        if (toolbar == pressed) {
                            apply_toolbar_action(native, pressed);
                        }
                        break;
                    }
                    if (!native->pointer_pressed && toolbar != -2) {
                        native->toolbar_hover = toolbar;
                        native->view_dirty = true;
                        break;
                    }
                    float logical_x = 0.0f;
                    float logical_y = 0.0f;
                    window_to_logical(native, event.button.x, event.button.y,
                                      &logical_x, &logical_y);
                    native->pointer_position.x = (lv_coord_t)lrintf(logical_x);
                    native->pointer_position.y = (lv_coord_t)lrintf(logical_y);
                    native->pointer_pressed = false;
                    lv_indev_read(native->pointer);
                    if (native->gesture_active) {
                        native->gesture_active = false;
                        float rotated_x = 0.0f;
                        float rotated_y = 0.0f;
                        logical_to_rotated(native, logical_x, logical_y, &rotated_x,
                                           &rotated_y);
                        lv_coord_t dx = (lv_coord_t)lrintf(rotated_x) -
                                        native->gesture_start.x;
                        lv_coord_t dy = (lv_coord_t)lrintf(rotated_y) -
                                        native->gesture_start.y;
                        lv_coord_t inward = native->gesture_edge == 1 ? dx : -dx;
                        lv_coord_t adx = dx < 0 ? -dx : dx;
                        lv_coord_t ady = dy < 0 ? -dy : dy;
                        if (native->gesture_edge != 0 && inward >= MICRO_NATIVE_SWIPE_THRESHOLD &&
                            adx >= 2 * ady) {
                            native->back_gesture = true;
                        }
                    }
                }
                break;
            case SDL_EVENT_WINDOW_RESIZED:
            case SDL_EVENT_WINDOW_PIXEL_SIZE_CHANGED:
            case SDL_EVENT_WINDOW_DISPLAY_SCALE_CHANGED:
            case SDL_EVENT_WINDOW_DISPLAY_CHANGED:
            case SDL_EVENT_WINDOW_MAXIMIZED:
            case SDL_EVENT_WINDOW_RESTORED:
                if (event.window.windowID == native->window_id) {
                    int previous_zoom = native->zoom_percent;
                    refresh_presentation_metrics(native);
                    if (!native->resizing_window) {
                        int percent = (int)lrintf(native->view_scale * 100.0f);
                        if (percent < MICRO_MIN_ZOOM_PERCENT)
                            percent = MICRO_MIN_ZOOM_PERCENT;
                        if (percent > MICRO_MAX_ZOOM_PERCENT)
                            percent = MICRO_MAX_ZOOM_PERCENT;
                        native->zoom_percent = percent;
                        if (percent != previous_zoom) {
                            update_window_title(native);
                        }
                    }
                }
                break;
            default: break;
        }
    }
    if (native->pending_paste) {
        native->pending_paste = false;
        if (native->focused_textarea != NULL) {
            char *clipboard = SDL_GetClipboardText();
            if (clipboard != NULL) {
                lv_textarea_add_text(native->focused_textarea, clipboard);
                SDL_free(clipboard);
            }
        }
    }
    return native->quit ? 0 : 1;
}

int micro_native_take_back_gesture(micro_native_t *native) {
    if (native == NULL) return 0;
    /* The App bar's close button is the pointer-accessible equivalent of the
     * edge-swipe gesture; both return to the shell. */
    int result = (native->back_gesture || micro_appbar_take_back()) ? 1 : 0;
    native->back_gesture = false;
    return result;
}

int micro_native_take_appbar_close(micro_native_t *native) {
    (void)native;
    return micro_appbar_take_back();
}

int micro_native_take_swipe_back(micro_native_t *native) {
    if (native == NULL) return 0;
    int result = native->back_gesture ? 1 : 0;
    native->back_gesture = false;
    return result;
}

void micro_native_set_app_chrome(micro_native_t *native, int visible,
                                 const char *title, const char *wifi_state,
                                 const char *wifi_ssid) {
    if (native == NULL) return;
    micro_appbar_ensure();
    micro_appbar_set_title(title);
    micro_appbar_set_wifi(wifi_state, wifi_ssid);
    micro_appbar_set_visible(visible);
}

int micro_native_appbar_visible(micro_native_t *native) {
    (void)native;
    return micro_appbar_visible();
}

void micro_native_power_off(micro_native_t *native) {
    (void)native;
    micro_appbar_power_off();
}

int micro_native_power_off_active(micro_native_t *native) {
    (void)native;
    return micro_appbar_power_off_active();
}

int micro_native_take_power_on(micro_native_t *native) {
    (void)native;
    return micro_appbar_take_power_on();
}

void micro_native_wake(micro_native_t *native) {
    (void)native;
    micro_appbar_wake();
}

void micro_native_press_power_on(micro_native_t *native) {
    (void)native;
    micro_appbar_press_power_on();
}

uint32_t micro_native_timer(micro_native_t *native) {
    uint32_t delay = lv_timer_handler();
    /* A window resize / rotation / zoom changes only the presentation, so LVGL
     * may have nothing to redraw. Re-present the existing canvas texture. */
    if (native != NULL && native->view_dirty) {
        present_frame(native);
    }
    return delay;
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
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    lv_obj_update_layout(native->objects[node_id]);
    lv_area_t coordinates;
    lv_obj_get_coords(native->objects[node_id], &coordinates);
    float logical_x = (float)(coordinates.x1 + coordinates.x2) / 2.0F;
    float logical_y = (float)(coordinates.y1 + coordinates.y2) / 2.0F;
    float x = 0.0f;
    float y = 0.0f;
    logical_to_window(native, logical_x, logical_y, &x, &y);
    SDL_Event down = {0};
    down.type = SDL_EVENT_MOUSE_BUTTON_DOWN;
    down.button.windowID = native->window_id;
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

int micro_native_queue_wheel(micro_native_t *native, double logical_x,
                             double logical_y, float wheel_x, float wheel_y) {
    if (native == NULL) return 0;
    float window_x = 0.0f;
    float window_y = 0.0f;
    logical_to_window(native, (float)logical_x, (float)logical_y, &window_x,
                      &window_y);
    SDL_Event event = {0};
    event.type = SDL_EVENT_MOUSE_WHEEL;
    event.wheel.windowID = native->window_id;
    event.wheel.x = wheel_x;
    event.wheel.y = wheel_y;
    event.wheel.direction = SDL_MOUSEWHEEL_NORMAL;
    event.wheel.mouse_x = window_x;
    event.wheel.mouse_y = window_y;
    return SDL_PushEvent(&event);
}

/* Test/integration hooks for the SDL text-input path. They behave like a user
 * clicking a field and typing, without requiring a visible OS window. */
int micro_native_focus_textarea(micro_native_t *native, uint32_t node_id) {
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    if (!lv_obj_check_type(native->objects[node_id], &lv_textarea_class)) return 0;
    native->focused_textarea = native->objects[node_id];
    return 1;
}

int micro_native_queue_text_input(micro_native_t *native, const char *text) {
    if (native == NULL || text == NULL) return 0;
    snprintf(native->injected_text, sizeof(native->injected_text), "%s", text);
    SDL_Event event = {0};
    event.type = SDL_EVENT_TEXT_INPUT;
    event.text.windowID = SDL_GetWindowID(native->window);
    event.text.text = native->injected_text;
    return SDL_PushEvent(&event);
}

int micro_native_create_column(micro_native_t *native, uint32_t node_id, uint32_t parent_id) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *object = lv_obj_create(parent);
    bool tab_page = parent == native->tab_target;
    /* A root or tab-page container fills its page in every orientation. Nested
     * containers keep content height so `ui.place` can size them. */
    lv_obj_set_size(object, LV_PCT(100), (parent_id == MICRO_NO_PARENT || tab_page)
                                             ? LV_PCT(100)
                                             : LV_SIZE_CONTENT);
    if (tab_page) native->fill_height[node_id] = true;
    /* Plain container: no flex layout — every child is positioned by its
     * Delphi ltwh/anchor via ui.place. Zero the theme's card padding: the
     * layout engine interprets left/top/width/height in the container's
     * content box, so padding would shift every child and shrink the space
     * available inside an explicit height, clipping the bottom row. */
    lv_obj_set_style_pad_all(object, 0, LV_PART_MAIN);
    lv_obj_set_style_border_width(object, 0, LV_PART_MAIN);
    /* Blend into the parent: invisible but opaque (a transparent container
     * changed LVGL's draw path and intermittently hid the top rows). */
    lv_obj_set_style_bg_color(object, lv_obj_get_style_bg_color(parent, LV_PART_MAIN), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(object, LV_OPA_COVER, LV_PART_MAIN);
    if (parent_id == MICRO_NO_PARENT) lv_obj_set_pos(object, 0, 0);
    native->objects[node_id] = object;
    return 1;
}

int micro_native_create_row(micro_native_t *native, uint32_t node_id, uint32_t parent_id) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *object = lv_obj_create(parent);
    bool tab_page = parent == native->tab_target;
    lv_obj_set_size(object, LV_PCT(100), (parent_id == MICRO_NO_PARENT || tab_page)
                                             ? LV_PCT(100)
                                             : LV_SIZE_CONTENT);
    if (tab_page) native->fill_height[node_id] = true;
    /* Plain container: no flex layout — every child is positioned by its
     * Delphi ltwh/anchor via ui.place. Zero the theme's card padding for the
     * same reason as columns: explicit heights describe the content box. */
    lv_obj_set_style_pad_all(object, 0, LV_PART_MAIN);
    lv_obj_set_style_border_width(object, 0, LV_PART_MAIN);
    lv_obj_set_style_bg_color(object, lv_obj_get_style_bg_color(parent, LV_PART_MAIN), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(object, LV_OPA_COVER, LV_PART_MAIN);
    if (parent_id == MICRO_NO_PARENT) lv_obj_set_pos(object, 0, 0);
    lv_obj_remove_flag(object, LV_OBJ_FLAG_SCROLLABLE);
    lv_obj_set_scrollbar_mode(object, LV_SCROLLBAR_MODE_OFF);
    native->objects[node_id] = object;
    return 1;
}

int micro_native_create_surface(micro_native_t *native, uint32_t node_id,
                                uint32_t parent_id, const char *tone,
                                uint32_t handler_id) {
    if (native == NULL || tone == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *object = lv_obj_create(parent);
    bool tab_page = parent == native->tab_target;
    lv_obj_set_size(object, LV_PCT(100), tab_page ? LV_PCT(100) : LV_SIZE_CONTENT);
    if (tab_page) native->fill_height[node_id] = true;
    /* Surfaces never scroll and are decorative unless an onClick handler is
     * attached; even then labels and buttons above keep their own clicks. */
    lv_obj_remove_flag(object, LV_OBJ_FLAG_SCROLLABLE);
    lv_obj_remove_flag(object, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_set_style_pad_all(object, 0, LV_PART_MAIN);
    lv_obj_set_style_bg_opa(object, LV_OPA_COVER, LV_PART_MAIN);

    uint32_t bg = 0xFFFDF7;
    uint32_t border = 0xE4DAC6;
    int32_t radius = 18;
    int32_t border_width = 1;
    int32_t shadow_width = 18;
    if (strcmp(tone, "canvas") == 0) {
        bg = 0xF4EFE4;
        border = 0xF4EFE4;
        radius = 0;
        border_width = 0;
        shadow_width = 0;
    } else if (strcmp(tone, "sage") == 0) {
        bg = 0xE7F0E5;
        border = 0xD0DFCD;
        shadow_width = 12;
    } else if (strcmp(tone, "terracotta") == 0) {
        bg = 0xF7E4D8;
        border = 0xEBC9B6;
        shadow_width = 12;
    } else if (strcmp(tone, "line") == 0) {
        bg = 0xDED5C2;
        border = 0xDED5C2;
        radius = 0;
        border_width = 0;
        shadow_width = 0;
    }
    lv_obj_set_style_bg_color(object, lv_color_hex(bg), LV_PART_MAIN);
    lv_obj_set_style_border_color(object, lv_color_hex(border), LV_PART_MAIN);
    lv_obj_set_style_border_width(object, border_width, LV_PART_MAIN);
    lv_obj_set_style_radius(object, radius, LV_PART_MAIN);
    lv_obj_set_style_shadow_color(object, lv_color_hex(0x20342B), LV_PART_MAIN);
    lv_obj_set_style_shadow_width(object, shadow_width, LV_PART_MAIN);
    lv_obj_set_style_shadow_opa(object, LV_OPA_20, LV_PART_MAIN);
    lv_obj_set_style_shadow_ofs_y(object, 6, LV_PART_MAIN);
    native->clicks[node_id].native = native;
    native->clicks[node_id].handler_id = handler_id;
    if (handler_id != MICRO_NO_HANDLER) {
        lv_obj_add_flag(object, LV_OBJ_FLAG_CLICKABLE);
        lv_obj_add_event_cb(object, click_callback, LV_EVENT_CLICKED,
                            &native->clicks[node_id]);
    }
    native->objects[node_id] = object;
    return 1;
}

int micro_native_set_visible(micro_native_t *native, uint32_t node_id, int visible) {
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    if (visible) {
        lv_obj_clear_flag(native->objects[node_id], LV_OBJ_FLAG_HIDDEN);
    } else {
        lv_obj_add_flag(native->objects[node_id], LV_OBJ_FLAG_HIDDEN);
    }
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
    /* As the root (parent == screen) fill the display, like the Web
     * player and the ESP32 bridge; as a child of a scrollable column keep a
     * bounded height so the column can scroll past it. A fixed 280 px root
     * tabview cannot hold its tab pages' children. */
    lv_obj_set_size(tabview, LV_PCT(100),
                    parent_id == MICRO_NO_PARENT ? LV_PCT(100) : 280);
    char *copy = strdup(titles);
    if (copy != NULL) {
        char *save = NULL;
        for (char *token = strtok_r(copy, "\n", &save); token != NULL;
             token = strtok_r(NULL, "\n", &save)) {
            lv_tabview_add_tab(tabview, token);
        }
        free(copy);
    }
    /* LVGL derives the top bar height from the display DPI (65 px here),
     * which is taller than the Web player's 47 px and leaves too little page
     * height for tall tab content. Pin it to the shared design value so a tab
     * whose content is 424 px tall fits the 480 px screen. */
    lv_tabview_set_tab_bar_size(tabview, 48);
    /* One uniform light background across the app; no padding or borders at
     * the tab level, scrollbar flush with the page edge. */
    lv_color_t app_bg = lv_color_hex(0xF4EFE4);
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
    lv_obj_set_style_border_color(tab_bar, lv_color_hex(0xDED5C2), LV_PART_MAIN);
    lv_obj_set_style_bg_color(tab_bar, app_bg, LV_PART_MAIN);
    lv_obj_set_style_bg_opa(tab_bar, LV_OPA_COVER, LV_PART_MAIN);
    /* Flat journal tabs: no raised button chrome, muted inactive text and a
     * deep-green active label. LVGL's default theme otherwise uses its blue
     * primary color here, which fights the paper palette. */
    uint32_t tab_count = lv_obj_get_child_count(tab_bar);
    for (uint32_t ti = 0; ti < tab_count; ++ti) {
        lv_obj_t *tab = lv_obj_get_child(tab_bar, ti);
        if (tab == NULL) continue;
        lv_obj_set_style_bg_opa(tab, LV_OPA_TRANSP, LV_PART_MAIN);
        lv_obj_set_style_bg_opa(tab, LV_OPA_TRANSP, LV_PART_MAIN | LV_STATE_CHECKED);
        lv_obj_set_style_border_width(tab, 0, LV_PART_MAIN);
        lv_obj_set_style_border_width(tab, 0, LV_PART_MAIN | LV_STATE_CHECKED);
        lv_obj_set_style_radius(tab, 0, LV_PART_MAIN);
        lv_obj_set_style_text_color(tab, lv_color_hex(0x6F7F71), LV_PART_MAIN);
        lv_obj_set_style_text_color(tab, lv_color_hex(0x1D5C46), LV_STATE_CHECKED);
        lv_obj_set_style_text_font(tab, &micro_ui_sans_14, LV_PART_MAIN);
        lv_obj_t *tab_label = lv_obj_get_child(tab, 0);
        if (tab_label != NULL) {
            lv_obj_set_style_text_font(tab_label, &micro_ui_sans_14, LV_PART_MAIN);
        }
        lv_obj_set_style_bg_opa(tab, LV_OPA_TRANSP, LV_PART_INDICATOR);
        lv_obj_set_style_bg_color(tab, lv_color_hex(0x1D5C46), LV_PART_INDICATOR);
        lv_obj_set_style_bg_opa(tab, LV_OPA_COVER, LV_PART_INDICATOR | LV_STATE_CHECKED);
    }
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
    lv_obj_set_size(bar, LV_PCT(100), 8);
    lv_obj_set_style_bg_color(bar, lv_color_hex(0xE5DDCC), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(bar, LV_OPA_COVER, LV_PART_MAIN);
    lv_obj_set_style_radius(bar, LV_RADIUS_CIRCLE, LV_PART_MAIN);
    lv_obj_set_style_bg_color(bar, lv_color_hex(0x2E6B52), LV_PART_INDICATOR);
    lv_obj_set_style_bg_opa(bar, LV_OPA_COVER, LV_PART_INDICATOR);
    lv_obj_set_style_radius(bar, LV_RADIUS_CIRCLE, LV_PART_INDICATOR);
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

int micro_native_create_icon(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *glyph, uint32_t size_px, const char *tone, const char *variant, uint32_t handler_id) {
    if (native == NULL || glyph == NULL || node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *object = lv_obj_create(parent);
    if (object == NULL) return 0;
    /* The glyph is centered inside a box the Delphi layout sizes explicitly
     * (so a badge stays circular at any icon size). */
    lv_obj_set_size(object, LV_SIZE_CONTENT, LV_SIZE_CONTENT);
    lv_obj_remove_flag(object, LV_OBJ_FLAG_SCROLLABLE);
    lv_obj_set_style_pad_all(object, 0, LV_PART_MAIN);
    lv_obj_set_style_border_width(object, 0, LV_PART_MAIN);
    lv_obj_set_style_shadow_width(object, 0, LV_PART_MAIN);
    lv_obj_set_style_bg_opa(object, LV_OPA_TRANSP, LV_PART_MAIN);
    if (variant != NULL && strcmp(variant, "badge") == 0) {
        lv_obj_set_style_bg_color(object, lv_color_hex(0xE7F0E5), LV_PART_MAIN);
        lv_obj_set_style_bg_opa(object, LV_OPA_COVER, LV_PART_MAIN);
        lv_obj_set_style_radius(object, LV_RADIUS_CIRCLE, LV_PART_MAIN);
    }
    lv_obj_t *label = lv_label_create(object);
    if (label == NULL) {
        lv_obj_delete(object);
        return 0;
    }
    lv_label_set_text(label, glyph);
    lv_obj_set_style_text_font(label, icon_font_for_size(size_px), LV_PART_MAIN);
    lv_obj_set_style_text_color(label, lv_color_hex(icon_tone_color(tone)), LV_PART_MAIN);
    lv_obj_center(label);
    native->clicks[node_id].native = native;
    native->clicks[node_id].handler_id = handler_id;
    if (handler_id != MICRO_NO_HANDLER) {
        lv_obj_add_flag(object, LV_OBJ_FLAG_CLICKABLE);
        lv_obj_add_event_cb(object, click_callback, LV_EVENT_CLICKED,
                            &native->clicks[node_id]);
    } else {
        /* Decorative icons must not swallow taps meant for a surface tile. */
        lv_obj_remove_flag(object, LV_OBJ_FLAG_CLICKABLE);
    }
    native->objects[node_id] = object;
    native->text_targets[node_id] = label;
    return 1;
}

int micro_native_set_icon(micro_native_t *native, uint32_t node_id, const char *glyph) {
    if (native == NULL || glyph == NULL || node_id >= MICRO_MAX_NODES ||
        native->text_targets[node_id] == NULL) return 0;
    lv_label_set_text(native->text_targets[node_id], glyph);
    return 1;
}

int micro_native_create_button(micro_native_t *native, uint32_t node_id, uint32_t parent_id, const char *text, uint32_t handler_id, uintptr_t font_handle, uint32_t line_height_px, int selected) {
    if (node_id >= MICRO_MAX_NODES) return 0;
    lv_obj_t *parent = parent_object(native, parent_id);
    if (parent == NULL) return 0;
    lv_obj_t *button;
    lv_obj_t *label;
    uint32_t bg_color = selected ? 0xC67A4F : 0x1F6B4F;
    uint32_t pressed_color = selected ? 0xA65C36 : 0x26785A;
    uint32_t shadow_color = selected ? 0x8C4A2B : 0x1E4D3B;
    if (lv_obj_check_type(parent, &lv_list_class)) {
        button = lv_list_add_button(parent, NULL, text);
        label = lv_obj_get_child(button, 0);
    } else {
        button = lv_button_create(parent);
        label = lv_label_create(button);
        lv_label_set_text(label, text);
        apply_text_style(label, font_handle, line_height_px);
        lv_obj_center(label);
        lv_obj_set_style_bg_color(button, lv_color_hex(bg_color), LV_PART_MAIN);
        lv_obj_set_style_bg_color(button, lv_color_hex(pressed_color), LV_STATE_PRESSED);
        lv_obj_set_style_radius(button, 12, LV_PART_MAIN);
        lv_obj_set_style_shadow_width(button, 14, LV_PART_MAIN);
        lv_obj_set_style_shadow_color(button, lv_color_hex(shadow_color), LV_PART_MAIN);
        lv_obj_set_style_shadow_opa(button, LV_OPA_30, LV_PART_MAIN);
        lv_obj_set_style_text_color(label, lv_color_hex(0xFFFDF7), LV_PART_MAIN);
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
    lv_obj_set_style_bg_color(textarea, lv_color_hex(0xFFFDF7), LV_PART_MAIN);
    lv_obj_set_style_bg_opa(textarea, LV_OPA_COVER, LV_PART_MAIN);
    lv_obj_set_style_border_color(textarea, lv_color_hex(0xD8CDB9), LV_PART_MAIN);
    lv_obj_set_style_border_width(textarea, 1, LV_PART_MAIN);
    lv_obj_set_style_radius(textarea, 10, LV_PART_MAIN);
    lv_obj_set_style_border_opa(textarea, LV_OPA_COVER, LV_PART_MAIN);
    lv_obj_set_style_pad_all(textarea, 8, LV_PART_MAIN);
    lv_obj_set_width(textarea, LV_PCT(100));
    lv_obj_set_height(textarea, LV_SIZE_CONTENT);
    native->clicks[node_id].native = native;
    native->clicks[node_id].handler_id = handler_id;
    lv_obj_add_event_cb(textarea, textarea_focus_callback, LV_EVENT_CLICKED,
                        &native->clicks[node_id]);
    lv_obj_add_event_cb(textarea, textarea_focus_callback, LV_EVENT_FOCUSED,
                        &native->clicks[node_id]);
    lv_obj_add_event_cb(textarea, textarea_focus_callback, LV_EVENT_DEFOCUSED,
                        &native->clicks[node_id]);
    if (handler_id == MICRO_NO_HANDLER) {
        lv_obj_remove_flag(textarea, LV_OBJ_FLAG_CLICKABLE);
    } else {
        lv_obj_add_event_cb(textarea, input_callback, LV_EVENT_VALUE_CHANGED,
                            &native->clicks[node_id]);
    }
    native->objects[node_id] = textarea;
    native->text_targets[node_id] = textarea;
    return 1;
}

int micro_native_set_input_text(micro_native_t *native, uint32_t node_id, const char *text) {
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL) return 0;
    /* The onChange handler re-renders the same value after each keystroke.
     * Setting it again would move the cursor back to the start. */
    const char *current = lv_textarea_get_text(native->objects[node_id]);
    if (current != NULL && text != NULL && strcmp(current, text) == 0) return 1;
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

    /* Absolute positioning: every child is placed by its explicit left/top and
     * width/height, adjusted by edge anchors (Delphi model). No stacking. */
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = layout_mask(native, node);
        lv_obj_update_layout(child);
        lv_coord_t w = lv_obj_get_width(child), h = lv_obj_get_height(child);
        lv_coord_t ew = (mask & 4) ? (lv_coord_t)native->layout_specs[node].width : w;
        lv_coord_t eh = (mask & 8) ? (lv_coord_t)native->layout_specs[node].height : h;
        lv_coord_t x;
        if ((mask & 16) && (mask & 64)) {
            /* Stretch: left + right anchored. */
            x = (lv_coord_t)native->layout_specs[node].anchor_left;
            lv_obj_set_width(child, avail_w - (lv_coord_t)native->layout_specs[node].anchor_left
                                       - (lv_coord_t)native->layout_specs[node].anchor_right);
        } else if (mask & 64) {
            /* Pin the right edge. */
            lv_obj_set_width(child, ew);
            x = avail_w - ew - (lv_coord_t)native->layout_specs[node].anchor_right;
        } else if (mask & 16) {
            /* Pin the left edge (a lone left anchor, not a stretch). */
            lv_obj_set_width(child, ew);
            x = (lv_coord_t)native->layout_specs[node].anchor_left;
        } else {
            x = (mask & 1) ? (lv_coord_t)native->layout_specs[node].left : 0;
            lv_obj_set_width(child, ew);
        }
        lv_coord_t y;
        if ((mask & 32) && (mask & 128)) {
            /* Stretch: top + bottom anchored. */
            lv_obj_set_height(child, avail_h - (lv_coord_t)native->layout_specs[node].anchor_top
                                     - (lv_coord_t)native->layout_specs[node].anchor_bottom);
            y = (lv_coord_t)native->layout_specs[node].anchor_top;
        } else if (mask & 128) {
            /* Pin the bottom edge. */
            lv_obj_set_height(child, eh);
            y = avail_h - eh - (lv_coord_t)native->layout_specs[node].anchor_bottom;
        } else if (mask & 32) {
            /* Pin the top edge (a lone top anchor, not a stretch). */
            lv_obj_set_height(child, eh);
            y = (lv_coord_t)native->layout_specs[node].anchor_top;
        } else {
            lv_obj_set_height(child, eh);
            y = (mask & 2) ? (lv_coord_t)native->layout_specs[node].top : 0;
        }
        /* set_pos only writes a style, and LVGL skips style-position for
         * children of a layout container (lv_obj_refr_pos returns early for
         * layout-positioned objects). move_to applies the position directly,
         * like the built-in flex layout. */
        lv_obj_move_to(child, x, y);
    }
}

int micro_native_set_layout_spec(micro_native_t *native, uint32_t node_id, uint32_t mask,
                                 double left, double top, double width, double height,
                                 double anchor_left, double anchor_top,
                                 double anchor_right, double anchor_bottom) {
    if (native == NULL || node_id >= MICRO_MAX_NODES) return 0;
    native->layout_specs[node_id].mask = (uint8_t)mask;
    native->layout_specs[node_id].left = left;
    native->layout_specs[node_id].top = top;
    native->layout_specs[node_id].width = width;
    native->layout_specs[node_id].height = height;
    native->layout_specs[node_id].anchor_left = anchor_left;
    native->layout_specs[node_id].anchor_top = anchor_top;
    native->layout_specs[node_id].anchor_right = anchor_right;
    native->layout_specs[node_id].anchor_bottom = anchor_bottom;
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
    uint32_t count = lv_obj_get_child_count(container);
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = layout_mask(native, node);
        int32_t h = lv_obj_get_height(child);
        if ((mask & 8) && node < MICRO_MAX_NODES) h = (int32_t)native->layout_specs[node].height; /* explicit */
        if ((mask & 32) && (mask & 128)) {
            /* Vertical fill (anchor top+bottom): natural-height floor once. */
            if (native->fill_natural_h[node] == 0) native->fill_natural_h[node] = (lv_coord_t)h;
            if (native->fill_natural_h[node] > fill_floor) fill_floor = native->fill_natural_h[node];
            continue;
        }
        if (mask & 128) { bottom_extent += h + (int32_t)native->layout_specs[node].anchor_bottom; }
        else {
            int32_t top_edge = (mask & 32) ? (int32_t)native->layout_specs[node].anchor_top
                             : (mask & 2) ? (int32_t)native->layout_specs[node].top : 0;
            int32_t bottom_edge = top_edge + h;
            if (bottom_edge > top_extent) top_extent = bottom_edge;
        }
    }
    int32_t content = top_extent + bottom_extent;
    if (fill_floor > content) content = fill_floor;
    *req_size = content
              + lv_obj_get_style_space_top(container, LV_PART_MAIN)
              + lv_obj_get_style_space_bottom(container, LV_PART_MAIN);
    return true;
}

static lv_coord_t delphi_content_height(micro_native_t *native, lv_obj_t *container)
{
    int32_t top_extent = 0, bottom_extent = 0, fill_floor = 0;
    uint32_t count = lv_obj_get_child_count(container);
    for (uint32_t i = 0; i < count; ++i) {
        lv_obj_t *child = lv_obj_get_child(container, i);
        uint32_t node = (uint32_t)(uintptr_t)lv_obj_get_user_data(child);
        uint8_t mask = layout_mask(native, node);
        int32_t h = lv_obj_get_height(child);
        if ((mask & 8) && node < MICRO_MAX_NODES) h = (int32_t)native->layout_specs[node].height; /* explicit */
        if ((mask & 32) && (mask & 128)) {
            /* Vertical fill (anchor top+bottom): natural-height floor once. */
            if (native->fill_natural_h[node] == 0) native->fill_natural_h[node] = (lv_coord_t)h;
            if (native->fill_natural_h[node] > fill_floor) fill_floor = native->fill_natural_h[node];
            continue;
        }
        if (mask & 128) { bottom_extent += h + (int32_t)native->layout_specs[node].anchor_bottom; }
        else {
            int32_t top_edge = (mask & 32) ? (int32_t)native->layout_specs[node].anchor_top
                             : (mask & 2) ? (int32_t)native->layout_specs[node].top : 0;
            int32_t bottom_edge = top_edge + h;
            if (bottom_edge > top_extent) top_extent = bottom_edge;
        }
    }
    int32_t content = top_extent + bottom_extent;
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
    if (native->delphi_layout_id == LV_LAYOUT_NONE) {
        lv_layout_callbacks_t callbacks = { .layout_update_cb = delphi_layout_update_cb,
                                            .get_min_size_cb = delphi_get_min_size_cb };
        native->delphi_layout_id = lv_layout_create(callbacks, native);
    }
    lv_obj_set_layout(obj, native->delphi_layout_id);
    lv_obj_update_layout(obj);
    /* App roots fill the app-bar content box; do not shrink them back to the
     * measured content height, or a portrait viewport would leave the page
     * canvas centered in a short 440 px strip. A nested container that anchors
     * both top and bottom is likewise sized by its parent page, so keep the
     * stretched height instead of collapsing it to natural content. */
    uint8_t mask = layout_mask(native, container);
    if (lv_obj_get_parent(obj) != micro_appbar_content() && !native->fill_height[container] &&
        !((mask & 32) && (mask & 128))) {
        lv_obj_set_height(obj, delphi_content_height(native, obj));
        lv_obj_update_layout(obj);
    }
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
    lv_scale_set_angle_range(scale, 270);
    lv_scale_set_rotation(scale, 135);
    lv_scale_set_total_tick_count(scale, 11);
    lv_scale_set_major_tick_every(scale, 5);
    lv_scale_set_label_show(scale, true);
    lv_obj_set_size(scale, 100, 100);
    /* No padding: the needle pivot and the arc both use outer width/2,
     * which must equal the gauge center. */
    lv_obj_set_style_pad_all(scale, 0, LV_PART_MAIN);
    /* Outer ring (LV_PART_MAIN arc). */
    lv_obj_set_style_arc_color(scale, lv_color_hex(0x9E9E9E), LV_PART_MAIN);
    lv_obj_set_style_arc_width(scale, 3, LV_PART_MAIN);
    /* Minor ticks (LV_PART_ITEMS): gray. */
    lv_obj_set_style_line_color(scale, lv_color_hex(0x8A8A8A), LV_PART_ITEMS);
    lv_obj_set_style_line_width(scale, 1, LV_PART_ITEMS);
    lv_obj_set_style_length(scale, 6, LV_PART_ITEMS);
    /* Major ticks + labels (LV_PART_INDICATOR): white. */
    lv_obj_set_style_line_color(scale, lv_color_hex(0xFFFFFF), LV_PART_INDICATOR);
    lv_obj_set_style_line_width(scale, 2, LV_PART_INDICATOR);
    lv_obj_set_style_length(scale, 11, LV_PART_INDICATOR);
    lv_obj_set_style_text_color(scale, lv_color_hex(0xFFFFFF), LV_PART_INDICATOR);
    /* Colored value arc, overlaid on the outer ring. Created before the
     * needle so the needle draws on top. */
    lv_obj_t *arc = lv_arc_create(scale);
    lv_obj_remove_style_all(arc);
    lv_obj_set_size(arc, 100, 100);
    lv_obj_align(arc, LV_ALIGN_CENTER, 0, 0);
    lv_arc_set_rotation(arc, 135);
    lv_arc_set_bg_angles(arc, 0, 270);
    lv_arc_set_range(arc, (int32_t)min, (int32_t)max);
    lv_arc_set_value(arc, (int32_t)value);
    lv_obj_remove_flag(arc, LV_OBJ_FLAG_CLICKABLE);
    lv_obj_remove_style(arc, NULL, LV_PART_KNOB);
    lv_obj_set_style_arc_opa(arc, LV_OPA_TRANSP, LV_PART_MAIN);
    lv_obj_set_style_arc_color(arc, lv_color_hex(0x2196F3), LV_PART_INDICATOR);
    lv_obj_set_style_arc_width(arc, 6, LV_PART_INDICATOR);
    lv_obj_set_style_arc_rounded(arc, false, LV_PART_INDICATOR);
    native->arcs[node_id] = arc;
    lv_obj_t *needle = lv_line_create(scale);
    lv_point_precise_t points[2] = {{0, 0}, {0, -40}};
    lv_line_set_points(needle, points, 2);
    lv_obj_set_style_line_width(needle, 3, LV_PART_MAIN);
    lv_obj_set_style_line_color(needle, lv_color_hex(0x101820), LV_PART_MAIN);
    lv_scale_set_line_needle_value(scale, needle, 40, (int32_t)value);
    native->needles[node_id] = needle;
    native->objects[node_id] = scale;
    return 1;
}

int micro_native_set_scale_value(micro_native_t *native, uint32_t node_id, double value) {
    if (native == NULL || node_id >= MICRO_MAX_NODES || native->objects[node_id] == NULL ||
        native->needles[node_id] == NULL || native->arcs[node_id] == NULL) return 0;
    lv_scale_set_line_needle_value(native->objects[node_id], native->needles[node_id], 40, (int32_t)value);
    lv_arc_set_value(native->arcs[node_id], (int32_t)value);
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
