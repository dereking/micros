# Native Pointer Click Fix Design

## Problem

The visible macOS Counter window renders correctly, but clicking **Add** does not update the count. The existing hidden smoke test passes because it injects a handler ID directly and bypasses SDL pointer input.

## Root cause

`micro_native_poll` drains all queued SDL events before LVGL reads the pointer. A normal click commonly places both mouse-down and mouse-up in that batch, leaving `pointer_pressed` false when `lv_timer_handler` eventually reads it. LVGL never observes the pressed state and therefore emits no `LV_EVENT_CLICKED` event.

The bridge also updates pointer coordinates only for motion events, even though SDL3 button events contain their own coordinates.

## Selected design

Follow the event-handling pattern used by LVGL's SDL driver:

1. For mouse motion, button-down, and button-up, update the stored coordinates from that specific SDL event.
2. Update the stored pressed state for left-button down/up only.
3. Call `lv_indev_read(native->pointer)` immediately after each relevant pointer event so LVGL observes transitions in order.
4. Continue draining the SDL queue and keep the Rust Runtime, EventQueue, StateStore, MBC, and renderer interfaces unchanged.

This is preferred over processing only one SDL event per host iteration, which can add backlog and latency, and over replacing the bridge with LVGL's full SDL driver, which is a larger architectural change.

## Regression coverage

Add native test plumbing that queues an SDL3 mouse-down and mouse-up at the center of a rendered button. Change the hidden end-to-end Counter smoke path to use this synthetic SDL click instead of injecting the handler ID directly.

The regression test must fail before the fix because both events are drained without an intermediate LVGL read. After the fix, it must prove the complete path:

```text
SDL3 pointer events
  -> LVGL input device
  -> LV_EVENT_CLICKED
  -> native activation queue
  -> Rust EventQueue
  -> VM handler
  -> StateStore
  -> bound label patch
```

Two queued clicks must produce state value `2` in smoke mode. Existing platform-neutral tests and the visible Counter Demo must continue to work.

## Scope and safety

- No TypeScript SDK, compiler, MBC, VM, Runtime, or UI tree changes.
- No timing sleeps in tests; the test drives queued events deterministically.
- Ignore non-left mouse buttons for button activation.
- Preserve queue capacity and error behavior.
