#import <Cocoa/Cocoa.h>

#include "micro_host_single_instance.h"

#include <SDL3/SDL.h>

/* Posted by a duplicate instance after its notice is dismissed. The primary
 * answers by raising its own window, so a minimized/hidden host really does
 * come back even though LaunchServices activation alone only raises the app. */
static NSString *const kMicroActivateNotification = @"dev.micros.micro-os.activate";

static void *g_observed_window = NULL;
static id g_observer = nil;

/* Bring the process that owns the OS window to the front.
 *
 * NSRunningApplication covers bundled and bare-binary launches: AppKit
 * registers every GUI process with LaunchServices, whether or not it came from
 * an .app. `unhide` recovers a hidden instance and the AllWindows option
 * restores minimized windows. */
static void micro_host_activate_process(pid_t pid) {
    if (pid <= 0) return;

    NSRunningApplication *owner =
        [NSRunningApplication runningApplicationWithProcessIdentifier:pid];
    if (owner == nil) return;

    [owner unhide];
    if (@available(macOS 14.0, *)) {
        [owner activateFromApplication:[NSRunningApplication currentApplication]
                              options:NSApplicationActivateAllWindows];
    } else {
        [owner activateWithOptions:NSApplicationActivateIgnoringOtherApps |
                                  NSApplicationActivateAllWindows];
    }
}

/* Raise the primary window from inside the primary process. Runs on the main
 * queue, so touching the SDL window and AppKit is safe. */
static void micro_host_present_window(void) {
    NSApplication *application = [NSApplication sharedApplication];
    [application activateIgnoringOtherApps:YES];

    SDL_Window *window = (SDL_Window *)g_observed_window;
    if (window == NULL) return;
    if ((SDL_GetWindowFlags(window) & SDL_WINDOW_MINIMIZED) != 0) {
        SDL_RestoreWindow(window);
    }
    SDL_ShowWindow(window);
    SDL_RaiseWindow(window);
}

void micro_host_single_instance_observe(void *sdl_window) {
    g_observed_window = sdl_window;
    if (g_observer != nil) return;

    g_observer = [[NSDistributedNotificationCenter defaultCenter]
        addObserverForName:kMicroActivateNotification
                    object:nil
                     queue:[NSOperationQueue mainQueue]
                usingBlock:^(__unused NSNotification *notification) {
                    micro_host_present_window();
                }];
}

void micro_host_single_instance_warn_and_activate(int owner_pid) {
    @autoreleasepool {
        /* This process exists only to show the notice. Force an Accessory
         * policy: a bundled executable launched directly (cargo run,
         * Contents/MacOS/micro-os, another copy of the binary) inherits
         * Regular from Info.plist, but LaunchServices never made this process
         * activatable, so the alert would open behind the running window.
         * Accessory activation works there; for an `open -n` duplicate the
         * only difference is a missing Dock tile during the notice. */
        NSApplication *application = [NSApplication sharedApplication];
        [application setActivationPolicy:NSApplicationActivationPolicyAccessory];
        [application activateIgnoringOtherApps:YES];

        NSAlert *alert = [[NSAlert alloc] init];
        alert.alertStyle = NSAlertStyleInformational;
        alert.messageText = @"Micro OS is already running";
        alert.informativeText =
            @"Only one copy of Micro OS can run at a time. "
            @"Click OK to switch to the running window.";
        [alert addButtonWithTitle:@"OK"];
        [alert runModal];

        /* The notice is gone: give focus back to the instance the user kept.
         * The notification covers a minimized or hidden primary window; the
         * direct activation covers a primary that predates the observer. The
         * caller returns from main as soon as this function does. */
        [[NSDistributedNotificationCenter defaultCenter]
            postNotificationName:kMicroActivateNotification
                          object:nil
                        userInfo:nil
                deliverImmediately:YES];
        micro_host_activate_process((pid_t)owner_pid);
    }
}
