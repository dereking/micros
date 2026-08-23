#ifndef MICRO_SYSTEM_UI_H
#define MICRO_SYSTEM_UI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* System-shell button taps. The OS shell (main.c) drains these and maps them
 * to OS reducer events; the shell itself is render-only. */
typedef enum {
    MICRO_SYSTEM_UI_TAP_NONE = 0,
    MICRO_SYSTEM_UI_TAP_OPEN_COUNTER,
    MICRO_SYSTEM_UI_TAP_OPEN_SETTINGS,
    MICRO_SYSTEM_UI_TAP_BACK,
    MICRO_SYSTEM_UI_TAP_BACKLIGHT_TOGGLE,
    MICRO_SYSTEM_UI_TAP_WIFI_CONNECT,
    MICRO_SYSTEM_UI_TAP_WIFI_DISCONNECT,
} micro_system_ui_tap_t;

/* Show the launcher: a status bar (device name, Wi-Fi, backlight) and the App
 * grid. Safe to call on the LVGL task; takes the LVGL lock. */
void micro_system_ui_show_launcher(const char *wifi_state, const char *wifi_ssid,
                                   uint32_t backlight);
/* Show the Settings page: Back, backlight toggle, Wi-Fi connect/disconnect,
 * and device info. */
void micro_system_ui_show_settings(const char *wifi_state, const char *wifi_ssid,
                                   uint32_t backlight);
/* Hide the shell while an App owns the screen. */
void micro_system_ui_hide(void);
/* Pop one tap, or MICRO_SYSTEM_UI_TAP_NONE when empty. */
micro_system_ui_tap_t micro_system_ui_take_tap(void);

#ifdef __cplusplus
}
#endif

#endif
