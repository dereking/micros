#ifndef MICRO_HOST_MENU_H
#define MICRO_HOST_MENU_H

/* Platform host menu. macOS implements a native NSMenu; other desktop
 * platforms install the portable SDL menu in micro_native.c. The gear is
 * hidden only when neither backend is available (for example hidden test
 * windows). */
int micro_host_menu_install(void);
int micro_host_menu_available(void);
/* 1 when the installed backend draws a real native menu (macOS NSMenu).
 * 0 means the host should use its portable SDL menu. */
int micro_host_menu_is_native(void);
/* `sdl_window` is an SDL_Window *; x/y are window points (SDL's y-down
 * coordinates). The menu opens below the toolbar at that anchor. */
void micro_host_menu_open(void *sdl_window, float x, float y);

#endif
