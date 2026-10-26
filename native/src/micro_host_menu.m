#import <Cocoa/Cocoa.h>

#include "micro_host_menu.h"
#include "micro_native.h"

#include <SDL3/SDL.h>

@interface MicroHostMenuController : NSObject <NSMenuDelegate>
@property(nonatomic, strong) NSMenu *menu;
@property(nonatomic, strong) NSMenuItem *statusItem;
@property(nonatomic, strong) NSMenuItem *powerOnItem;
@property(nonatomic, strong) NSMenuItem *powerOffItem;
@property(nonatomic, strong) NSMenuItem *restartItem;
@property(nonatomic, strong) NSMenuItem *landscapeItem;
@property(nonatomic, strong) NSMenuItem *portraitItem;
@property(nonatomic, strong) NSMenuItem *fitItem;
@property(nonatomic, strong) NSMenuItem *zoom75Item;
@property(nonatomic, strong) NSMenuItem *zoom100Item;
@property(nonatomic, strong) NSMenuItem *zoom125Item;
@property(nonatomic, strong) NSMenuItem *zoom150Item;
@property(nonatomic, strong) NSMenuItem *resolutionItem;
@end

@implementation MicroHostMenuController

- (instancetype)init {
    self = [super init];
    if (self == nil) return nil;

    _menu = [[NSMenu alloc] initWithTitle:@"Device"];
    _menu.delegate = self;
    _menu.autoenablesItems = NO;

    [self header:@"DEVICE"];
    _statusItem = [self item:@"Running" action:nil key:@"" assign:NULL];
    _statusItem.enabled = NO;
    [self item:@"Power On" action:@selector(powerOn:) key:@"" assign:&_powerOnItem];
    [self item:@"Power Off" action:@selector(powerOff:) key:@"" assign:&_powerOffItem];
    [self item:@"Force Restart…" action:@selector(forceRestart:) key:@"" assign:&_restartItem];
    [_menu addItem:[NSMenuItem separatorItem]];

    [self header:@"DISPLAY"];
    NSMenuItem *orientation =
        [self item:@"Orientation" action:nil key:@"" assign:NULL];
    orientation.submenu = [self submenuWithTitle:@"Orientation"];
    orientation.submenu.autoenablesItems = NO;
    [self submenuItem:@"Landscape" action:@selector(landscape:)
                key:@"" menu:orientation.submenu assign:&_landscapeItem];
    [self submenuItem:@"Portrait" action:@selector(portrait:)
                key:@"" menu:orientation.submenu assign:&_portraitItem];

    NSMenuItem *zoom = [self item:@"Zoom" action:nil key:@"" assign:NULL];
    zoom.submenu = [self submenuWithTitle:@"Zoom"];
    zoom.submenu.autoenablesItems = NO;
    [self submenuItem:@"Fit to Window" action:@selector(zoomFit:)
                key:@"" menu:zoom.submenu assign:&_fitItem];
    [self submenuItem:@"75%" action:@selector(zoom75:)
                key:@"" menu:zoom.submenu assign:&_zoom75Item];
    [self submenuItem:@"100%" action:@selector(zoom100:)
                key:@"" menu:zoom.submenu assign:&_zoom100Item];
    [self submenuItem:@"125%" action:@selector(zoom125:)
                key:@"" menu:zoom.submenu assign:&_zoom125Item];
    [self submenuItem:@"150%" action:@selector(zoom150:)
                key:@"" menu:zoom.submenu assign:&_zoom150Item];
    [_menu addItem:[NSMenuItem separatorItem]];

    NSMenuItem *info = [self item:@"Emulator Info" action:nil key:@"" assign:NULL];
    info.submenu = [self submenuWithTitle:@"Emulator Info"];
    info.submenu.autoenablesItems = NO;
    [self infoItem:@"ESP32-S3 Touch LCD 7" menu:info.submenu];
    _resolutionItem = [self infoItem:@"Resolution: 800 × 480" menu:info.submenu];
    [self infoItem:@"Memory: 8 MiB Flash · 8 MiB PSRAM" menu:info.submenu];
    [self infoItem:@"Touch: GT911 · Expander: CH422G" menu:info.submenu];
    [self infoItem:@"Runtime budget: 10,000 instructions" menu:info.submenu];

    return self;
}

- (NSMenu *)submenuWithTitle:(NSString *)title {
    return [[NSMenu alloc] initWithTitle:title];
}

- (NSMenuItem *)header:(NSString *)title {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title action:nil keyEquivalent:@""];
    item.enabled = NO;
    [self.menu addItem:item];
    return item;
}

- (NSMenuItem *)item:(NSString *)title
              action:(SEL)action
                 key:(NSString *)key
              assign:(NSMenuItem *__strong *)slot {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:key];
    item.target = self;
    [self.menu addItem:item];
    if (slot != NULL) *slot = item;
    return item;
}

- (NSMenuItem *)submenuItem:(NSString *)title
                      action:(SEL)action
                         key:(NSString *)key
                        menu:(NSMenu *)menu
                      assign:(NSMenuItem *__strong *)slot {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:key];
    item.target = self;
    [menu addItem:item];
    if (slot != NULL) *slot = item;
    return item;
}

- (NSMenuItem *)infoItem:(NSString *)title menu:(NSMenu *)menu {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title action:nil keyEquivalent:@""];
    item.enabled = NO;
    [menu addItem:item];
    return item;
}

- (void)menuNeedsUpdate:(NSMenu *)menu {
    micro_native_host_state_t state;
    if (!micro_native_host_state(&state)) {
        self.statusItem.title = @"Emulator unavailable";
        self.powerOnItem.enabled = NO;
        self.powerOffItem.enabled = NO;
        self.restartItem.enabled = NO;
        return;
    }

    self.statusItem.title = state.powered_off ? @"Powered off" : @"Running";
    self.powerOnItem.enabled = state.powered_off != 0;
    self.powerOffItem.enabled = state.powered_off == 0;
    self.restartItem.enabled = state.powered_off == 0;

    self.landscapeItem.state = state.portrait ? NSControlStateValueOff : NSControlStateValueOn;
    self.portraitItem.state = state.portrait ? NSControlStateValueOn : NSControlStateValueOff;

    self.fitItem.state = state.zoom_fit ? NSControlStateValueOn : NSControlStateValueOff;
    self.zoom75Item.state = (!state.zoom_fit && state.zoom_percent == 75)
                                ? NSControlStateValueOn : NSControlStateValueOff;
    self.zoom100Item.state = (!state.zoom_fit && state.zoom_percent == 100)
                                 ? NSControlStateValueOn : NSControlStateValueOff;
    self.zoom125Item.state = (!state.zoom_fit && state.zoom_percent == 125)
                                 ? NSControlStateValueOn : NSControlStateValueOff;
    self.zoom150Item.state = (!state.zoom_fit && state.zoom_percent == 150)
                                 ? NSControlStateValueOn : NSControlStateValueOff;

    self.resolutionItem.title = state.portrait
        ? @"Resolution: 480 × 800"
        : @"Resolution: 800 × 480";
}

- (void)postAction:(int)action {
    micro_native_request_host_action(action);
}

- (void)powerOn:(id)sender {
    (void)sender;
    [self postAction:MICRO_HOST_ACTION_POWER_ON];
}

- (void)powerOff:(id)sender {
    (void)sender;
    [self postAction:MICRO_HOST_ACTION_POWER_OFF];
}

- (void)forceRestart:(id)sender {
    (void)sender;
    NSAlert *alert = [[NSAlert alloc] init];
    alert.messageText = @"Force restart the emulator?";
    alert.informativeText =
        @"The running App will be closed and the device will cold boot Home. "
        @"Installed Apps and storage are kept.";
    alert.alertStyle = NSAlertStyleWarning;
    [alert addButtonWithTitle:@"Force Restart"];
    [alert addButtonWithTitle:@"Cancel"];
    if ([alert runModal] == NSAlertFirstButtonReturn) {
        [self postAction:MICRO_HOST_ACTION_FORCE_RESTART];
    }
}

- (void)landscape:(id)sender {
    (void)sender;
    [self postAction:MICRO_HOST_ACTION_LANDSCAPE];
}

- (void)portrait:(id)sender {
    (void)sender;
    [self postAction:MICRO_HOST_ACTION_PORTRAIT];
}

- (void)zoomFit:(id)sender {
    (void)sender;
    [self postAction:MICRO_HOST_ACTION_ZOOM_FIT];
}

- (void)zoom75:(id)sender {
    (void)sender;
    [self postAction:MICRO_HOST_ACTION_ZOOM_75];
}

- (void)zoom100:(id)sender {
    (void)sender;
    [self postAction:MICRO_HOST_ACTION_ZOOM_100];
}

- (void)zoom125:(id)sender {
    (void)sender;
    [self postAction:MICRO_HOST_ACTION_ZOOM_125];
}

- (void)zoom150:(id)sender {
    (void)sender;
    [self postAction:MICRO_HOST_ACTION_ZOOM_150];
}

- (void)openForWindow:(void *)window atX:(float)x y:(float)y {
    SDL_Window *sdlWindow = (SDL_Window *)window;
    if (sdlWindow == NULL) return;

    SDL_PropertiesID properties = SDL_GetWindowProperties(sdlWindow);
    NSWindow *nsWindow = (__bridge NSWindow *)SDL_GetPointerProperty(
        properties, SDL_PROP_WINDOW_COCOA_WINDOW_POINTER, NULL);
    if (nsWindow == nil) return;
    NSView *view = nsWindow.contentView;
    if (view == nil) return;

    NSRect bounds = view.bounds;
    NSPoint point = NSMakePoint((CGFloat)x, bounds.size.height - (CGFloat)y);
    /* popUpMenu does not always ask the delegate before its first display;
     * refresh disabled states and checkmarks explicitly. */
    [self menuNeedsUpdate:self.menu];
    [NSApp activateIgnoringOtherApps:YES];
    [self.menu popUpMenuPositioningItem:nil atLocation:point inView:view];
}

@end

static MicroHostMenuController *g_menu = nil;

int micro_host_menu_install(void) {
    if (g_menu != nil) return 1;
    if (NSApp == nil) return 0;
    g_menu = [[MicroHostMenuController alloc] init];
    return g_menu != nil ? 1 : 0;
}

int micro_host_menu_available(void) {
    return g_menu != nil ? 1 : 0;
}

int micro_host_menu_is_native(void) {
    return g_menu != nil ? 1 : 0;
}

void micro_host_menu_open(void *sdl_window, float x, float y) {
    if (g_menu == nil) return;
    [g_menu openForWindow:sdl_window atX:x y:y];
}
