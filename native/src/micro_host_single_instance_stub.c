#include "micro_host_single_instance.h"

/* Non-Apple hosts have no AppKit notice/activation bridge. The Rust guard still
 * refuses the duplicate launch and reports it on stderr; these stubs just keep
 * the C surface linkable. */
void micro_host_single_instance_warn_and_activate(int owner_pid) {
    (void)owner_pid;
}

void micro_host_single_instance_observe(void *sdl_window) {
    (void)sdl_window;
}
