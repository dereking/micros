# ESP32-S3 Device Simulator Design

## Goal

Extend the current Counter demonstration into an interactive browser simulator
for the Spotpear ESP32-S3-Touch-LCD-7 V1.2 N8R8. It must show the platform's
existing application, runtime, operating-system state-machine, and board
configuration capabilities without claiming to emulate ESP-IDF or physical
electrical hardware.

## Scope

The simulator is a browser product in `products/micro-web-player`. It displays
a landscape 800 by 480 device screen inside a board frame and a host-side
monitor panel.

The screen exposes:

- A launcher with Counter and Settings entries.
- Counter loaded from the compiled `apps/counter/app.ts` MBC image. Its **Add**
  action continues to travel through the real WebAssembly `micro-core` event
  queue, VM instruction budget, reactive state binding, UI tree, and DOM
  renderer.
- Settings controls for backlight state, Wi-Fi scan/connection outcomes, and
  Safe Mode.
- Back navigation between Launcher, Settings, and Counter.

The monitor exposes:

- The fixed board profile: 800×480 RGB565, 16 MHz pixel clock, GT911, CH422G,
  8 MiB Flash, and 8 MiB PSRAM.
- Live 800×480 touch coordinates derived from pointer input on the device
  screen.
- Backlight state, Wi-Fi state, current Micro OS state, event/action history,
  and the Counter instruction budget.

## Architecture

`micro-host-web` remains the only execution host for an application MBC image.
The simulator does not reimplement the Counter reducer in JavaScript. It
creates one WebAssembly runtime when entering Counter, forwards DOM button
activation to that runtime, runs its animation-frame tick loop, and disposes it
when leaving Counter.

A JavaScript system-shell controller owns `micro-os-core` only through a small
WebAssembly bridge added to `micro-host-web`. The bridge creates `MicroOs`,
dispatches a constrained set of existing `micro-os-core::Event` values, and
returns a stable serializable state/action snapshot. JavaScript renders that
snapshot; it must not duplicate reducer rules such as Wi-Fi retry, Safe Mode,
or app-session identity.

The simulated board data is a read-only projection of the checked Spotpear
profile and the committed LCD7 BSP constants. It is a diagnostic display, not
a GPIO, RGB timing, I2C, PSRAM, or GT911 driver emulator.

## User Flow

1. The simulator starts in Launcher after a normal configured boot.
2. Selecting Counter emits the existing `OpenApp(Counter)` flow, then starts
   the real MBC runtime after the reducer reaches `AppStarting`.
3. Counter's **Add** button updates `Count` through the shared Runtime. The
   monitor records a runtime activation and shows the configured 10,000
   instruction budget.
4. Back stops the runtime, waits for the matching reducer stop transition, and
   returns to Launcher.
5. Settings exposes an explicit button for each demo Wi-Fi outcome. The shell
   sends only valid reducer callbacks using the operation IDs returned from the
   previous action.
6. A Safe Mode action uses the existing reducer state and disables app launch.

## Error Handling

- MBC download, decode, or runtime errors remain visible in the existing
  runtime error area and stop the app tick loop.
- Invalid or stale system-shell commands are rendered as `Rejected` actions;
  they do not mutate displayed state.
- The simulator labels all board fields as simulated. It never reports a
  successful flash, touch-controller transaction, Wi-Fi connection, or PSRAM
  allocation on physical hardware.
- Actual ESP32 flash/display/touch verification stays a separate hardware
  acceptance task.

## Testing and Acceptance

Automated browser coverage must prove:

- Launcher opens Counter, two **Add** activations render `Count: 2`, and Back
  returns to Launcher after runtime disposal.
- Settings can dispatch a valid Wi-Fi flow and show the reducer's returned
  action/state history.
- Backlight and Safe Mode controls update the shell from reducer-derived state;
  Safe Mode rejects app launch.
- Pointer input updates monitor coordinates in the 0–799 and 0–479 ranges.
- Board monitor values exactly match the committed board profile/BSP contract.

This is intentionally a browser simulator of the Micro platform boundary. It
is not a replacement for the ESP32-S3 LCD7 flash, display, backlight, GT911,
or Wi-Fi hardware smoke test.
