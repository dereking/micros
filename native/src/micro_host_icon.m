#import <Cocoa/Cocoa.h>

#include "micro_host_icon.h"

int micro_host_icon_set_png(const void *bytes, size_t length) {
    if (bytes == NULL || length == 0) return 0;

    NSData *data = [NSData dataWithBytes:bytes length:length];
    NSImage *image = [[NSImage alloc] initWithData:data];
    if (image == nil) return 0;

    /* SDL has already created NSApplication by the time the host calls this.
     * Assigning applicationIconImage updates the Dock tile and Cmd-Tab entry
     * for both bundled and bare-binary launches. */
    NSApplication *application = [NSApplication sharedApplication];
    application.applicationIconImage = image;
    return 1;
}
