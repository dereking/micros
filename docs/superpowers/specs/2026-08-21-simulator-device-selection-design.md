# Simulator device selection design

## Goal

Make the simulated ESP32 display behave like a touch device rather than a browser document: users must not be able to select or drag its rendered text.

## Scope

- Apply selection and native drag suppression only to the simulated device screen and its descendants.
- Preserve pointer input, button activation, and the device screen's existing internal scrolling.
- Keep the simulator header and monitor/log panel selectable for debugging and copying values.

## Implementation

Use CSS on `.device-screen` with the standard `user-select: none` property and the WebKit-prefixed equivalent used by Chromium/Safari. Add the drag suppression property to the same scope. Do not add JavaScript event handlers: CSS expresses the browser interaction boundary directly and cannot interfere with the app runtime's event queue.

## Verification

Add a browser regression test that asserts the device-screen computed selection mode is `none`, then rerun the existing Counter, touch-monitor, scrolling, Web unit, and production-build checks.
