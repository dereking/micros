#include "micro_host_menu.h"

int micro_host_menu_install(void) {
    /* The portable SDL menu is always available on non-Apple desktops. */
    return 1;
}

int micro_host_menu_available(void) {
    return 1;
}

int micro_host_menu_is_native(void) {
    return 0;
}

void micro_host_menu_open(void *sdl_window, float x, float y) {
    (void)sdl_window;
    (void)x;
    (void)y;
}
