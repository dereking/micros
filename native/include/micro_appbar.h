/*
 * Micro OS default app bar.
 *
 * A 40 px LVGL system bar rendered above every screen, including the shell
 * launcher ("Home"). It shows the current App name (centred), a Wi-Fi status
 * icon (hover/tap reveals the SSID), a wall clock, and a close button. On an
 * App the close button returns to the shell; on the shell it powers the OS
 * view off, and tapping the off surface boots Home again. The App's UI tree is
 * mounted into `content`, a child of the active screen that is inset below the
 * bar.
 *
 * The host calls `micro_appbar_set_visible(1)` and sets the title before a
 * Runtime is created so the layout engine measures the inset content height.
 * Only the single-App native debug viewport hides the bar.
 *
 * All functions must be called with the LVGL lock held (the ESP32 bridge locks
 * around its own calls; the native host is single-threaded).
 */
#ifndef MICRO_APPBAR_H
#define MICRO_APPBAR_H

#include <lvgl.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MICRO_APPBAR_HEIGHT 40

/* Create the bar/content pair on the active screen if it does not exist yet.
 * Idempotent. The bar defaults to visible. */
void micro_appbar_ensure(void);

/* Show (1) or hide (0) the bar. Resizes the content container so App layouts
 * measure the real available height. The shell also shows it. */
void micro_appbar_set_visible(int visible);
int micro_appbar_visible(void);

/* Resize the system chrome after a device-orientation change. The App root
 * inside the content box is resized and its `ui.place` anchors are laid out
 * again; the App runtime is not restarted. */
void micro_appbar_set_viewport(int width, int height);

/* The container App UI nodes mount into (replaces `lv_screen_active()` as the
 * parent of a root node). */
lv_obj_t *micro_appbar_content(void);

/* Current App title, centred in the bar. */
void micro_appbar_set_title(const char *title);

/* Wi-Fi state ("off" | "connecting" | "connected" | "error") and optional
 * SSID. The icon turns green only when connected; hovering it (mouse) or
 * clicking/tapping it shows the SSID in a tooltip below the bar. */
void micro_appbar_set_wifi(const char *state, const char *ssid);

/* 1 when the close button was pressed since the last read (and clears it). */
int micro_appbar_take_back(void);

/* Power the OS view off: hide the app bar/content and show the full-screen
 * "Powered off" surface. The surface is clickable; a tap queues a power-on
 * request that the host drains with `micro_appbar_take_power_on`. */
void micro_appbar_power_off(void);
int micro_appbar_power_off_active(void);
int micro_appbar_take_power_on(void);

/* Leave the power-off surface and restore the app bar/content. */
void micro_appbar_wake(void);

/* Queue a power-on request as if the off surface had been tapped. Used by the
 * headless smoke scripts; host UI should let the off surface handle the tap. */
void micro_appbar_press_power_on(void);

/* Release the cached LVGL object pointers before the display is destroyed.
 * The next `micro_appbar_ensure` creates a fresh bar. */
void micro_appbar_reset(void);

#ifdef __cplusplus
}
#endif

#endif
