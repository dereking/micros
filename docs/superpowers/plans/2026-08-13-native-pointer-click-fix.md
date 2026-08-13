# Native Pointer Click Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make visible SDL3 mouse clicks activate LVGL buttons and update bound Counter state.

**Architecture:** Keep SDL ownership in `micro_native.c` and preserve all Rust runtime boundaries. Add deterministic native test plumbing that queues genuine SDL3 button events at a rendered node, then make the existing poller feed every pointer transition immediately into LVGL, matching LVGL's SDL driver pattern.

**Tech Stack:** Rust, C, SDL 3.4.10, LVGL 9.5, Cargo/CMake native tests

---

### Task 1: Add a failing SDL pointer regression test

**Files:**
- Modify: `native/include/micro_native.h`
- Modify: `native/src/micro_native.c`
- Modify: `crates/micro-host-sdl/src/native.rs`
- Modify: `crates/micro-host-sdl/tests/native_smoke.rs`

- [ ] **Step 1: Add deterministic SDL click test plumbing**

Declare `micro_native_queue_click(micro_native_t *, uint32_t)` in the C header. Implement it by validating the requested LVGL object, updating its layout, finding its center with `lv_obj_get_coords`, and queueing one left-button-down plus one left-button-up `SDL_Event` through `SDL_PushEvent`. Return failure if the node is invalid or either event cannot be queued.

Expose it in Rust as:

```rust
pub fn queue_click(&mut self, node: NodeId) -> Result<(), String> {
    native_result(
        unsafe { micro_native_queue_click(self.raw.as_ptr(), node.0) },
        "queue click",
    )
}
```

- [ ] **Step 2: Replace the activation-injection smoke assertion with a real pointer click**

Build a Column and Button through `NativeUi`, queue a click on the button, poll SDL, and assert handler `7` was activated:

```rust
let mut bridge = NativeBridge::create(320, 240, true).unwrap();
bridge.create_column(NodeId(0), None).unwrap();
bridge
    .create_button(NodeId(1), Some(NodeId(0)), "Add", FunctionId(7))
    .unwrap();
let _ = bridge.timer();
bridge.queue_click(NodeId(1)).unwrap();
assert!(bridge.poll());
assert_eq!(bridge.take_activation(), Some(FunctionId(7)));
```

- [ ] **Step 3: Run the focused test and confirm the regression is red**

Run:

```bash
cargo test -p micro-host-sdl --features native --test native_smoke -- --nocapture
```

Expected: FAIL because the current poller drains down and up before LVGL reads either transition.

### Task 2: Feed SDL pointer transitions to LVGL immediately

**Files:**
- Modify: `native/src/micro_native.c`
- Test: `crates/micro-host-sdl/tests/native_smoke.rs`

- [ ] **Step 1: Implement the minimal input fix**

For motion, update from `event.motion.x/y` and call `lv_indev_read(native->pointer)`. For left-button down/up, update from `event.button.x/y`, set `pointer_pressed`, and immediately call `lv_indev_read(native->pointer)`. Ignore other mouse buttons.

```c
case SDL_EVENT_MOUSE_BUTTON_DOWN:
    if (event.button.button == SDL_BUTTON_LEFT) {
        native->pointer_position.x = (lv_coord_t)event.button.x;
        native->pointer_position.y = (lv_coord_t)event.button.y;
        native->pointer_pressed = true;
        lv_indev_read(native->pointer);
    }
    break;
```

Use the equivalent state change for button-up and the equivalent coordinate/read sequence for motion.

- [ ] **Step 2: Run the focused native test and confirm green**

Run:

```bash
cargo test -p micro-host-sdl --features native --test native_smoke -- --nocapture
```

Expected: PASS and activation `FunctionId(7)` is observed.

### Task 3: Cover the complete Counter path and verify

**Files:**
- Modify: `crates/micro-host-sdl/src/main.rs`
- Modify: `README.md`

- [ ] **Step 1: Make smoke mode use actual queued SDL clicks**

Select the first node containing `on_click`, retain both its node and handler validation, queue two clicks at that node, and run one host iteration. Keep the existing assertion that `StateId(0)` equals `Value::Number(2.0)`.

- [ ] **Step 2: Document that native smoke uses the SDL pointer route**

Update the testing bullet in `README.md` to state that the hidden native Counter test queues SDL pointer clicks rather than injecting runtime activations.

- [ ] **Step 3: Run focused end-to-end native smoke**

Run:

```bash
npm run build:app
cargo run -p micro-host-sdl --features native -- --smoke apps/counter/dist/app.mbc
```

Expected: exit status `0` with Counter state reaching `2` through SDL, LVGL, Rust EventQueue, VM, StateStore, and binding patching.

- [ ] **Step 4: Run all tests and formatting checks**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test -p micro-host-sdl --features native
```

Expected: all commands exit `0` with no failures.

- [ ] **Step 5: Commit and launch the visible Demo**

Run:

```bash
git add native/include/micro_native.h native/src/micro_native.c crates/micro-host-sdl/src/native.rs crates/micro-host-sdl/src/main.rs crates/micro-host-sdl/tests/native_smoke.rs README.md docs/superpowers/plans/2026-08-13-native-pointer-click-fix.md
git commit -m "fix: deliver SDL pointer transitions to LVGL"
npm run demo
```

Expected: the visible Counter window opens and each **Add** click increments the label once.
