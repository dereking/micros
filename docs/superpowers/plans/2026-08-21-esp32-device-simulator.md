# ESP32-S3 Device Simulator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an interactive 800×480 ESP32-S3 Touch-LCD-7 Web simulator that runs Counter through the shared Runtime and runs system navigation through the shared Micro OS reducer.

**Architecture:** `MicroWebRuntime` remains the sole MBC execution host. A small Wasm `MicroWebSystem` facade owns `micro_os_core::MicroOs`, returns serializable reducer snapshots, and accepts only named valid intents. A JavaScript device shell renders those snapshots, manages Runtime lifetime, and exposes fixed board constants.

**Tech Stack:** Rust 2024, wasm-bindgen, micro-os-core, Vite, DOM, Node test runner, Playwright.

---

### Task 1: Reducer-backed Web system bridge

**Files:**
- Modify: `crates/micro-host-web/Cargo.toml`
- Modify: `crates/micro-host-web/src/lib.rs`
- Create: `crates/micro-host-web/tests/system_bridge.rs`

- [ ] **Step 1: Write failing tests**

```rust
use micro_host_web::{SystemIntent, SystemShell};

#[test]
fn configured_boot_reaches_launcher() {
    let shell = SystemShell::configured_boot();
    assert_eq!(shell.snapshot().screen, "Launcher");
    assert_eq!(shell.snapshot().last_action, "ConnectSavedWifi { operation: WifiOperationId(1) }");
}

#[test]
fn counter_start_stop_use_one_reducer_session() {
    let mut shell = SystemShell::configured_boot();
    shell.dispatch(SystemIntent::OpenCounter);
    assert_eq!(shell.dispatch(SystemIntent::CounterStarted).screen, "AppRunning(Counter, 1)");
    shell.dispatch(SystemIntent::Back);
    assert_eq!(shell.dispatch(SystemIntent::CounterStopped).screen, "Launcher");
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p micro-host-web --test system_bridge`

Expected: FAIL because the system facade is absent.

- [ ] **Step 3: Implement the smallest facade**

Add `micro-os-core.workspace = true`, `serde.workspace = true`, and `serde_json.workspace = true`. Add a target-independent module containing:

```rust
pub enum SystemIntent { OpenCounter, CounterStarted, CounterStopped, OpenSettings, Back, WifiScan, WifiConnect, WifiConnected, WifiPersisted, SafeMode, ToggleBacklight }
pub struct SystemShell { os: MicroOs, active_app: Option<AppSessionId>, active_wifi: Option<WifiOperationId>, backlight: Backlight, actions: Vec<String> }
pub struct SystemSnapshot { pub screen: String, pub wifi: String, pub backlight: String, pub last_action: String, pub actions: Vec<String>, pub counter_session: Option<u64> }
```

`configured_boot()` must dispatch the valid `BootSampled`, `StorageInitialized`, `ProfileValidated`, `DisplayInitialized`, `SystemUiInitialized`, and `NetworkConfigLoaded { configured: true }` sequence. Retain operation/session IDs from returned `StartApp`, `StartWifiScan`, and `ConnectWifi` actions. Append formatted reducer actions to a bounded 24-item log.

- [ ] **Step 4: Expose this facade to Wasm**

Add `MicroWebSystem` under `cfg(target_arch = "wasm32")`:

```rust
#[wasm_bindgen]
pub struct MicroWebSystem { shell: SystemShell }
#[wasm_bindgen]
impl MicroWebSystem {
    #[wasm_bindgen(constructor)] pub fn new() -> MicroWebSystem;
    pub fn dispatch(&mut self, intent: &str) -> Result<String, JsValue>;
    pub fn snapshot(&self) -> String;
}
```

Accept precisely: `open-counter`, `counter-started`, `counter-stopped`, `open-settings`, `back`, `wifi-scan`, `wifi-connect`, `wifi-connected`, `wifi-persisted`, `safe-mode`, and `backlight-toggle`. All other strings return `WEB_SYSTEM: unknown intent`.

- [ ] **Step 5: Verify and commit**

Run: `cargo test -p micro-host-web --test system_bridge && cargo clippy -p micro-host-web --all-targets -- -D warnings`

Expected: PASS.

```bash
git add crates/micro-host-web/Cargo.toml crates/micro-host-web/src/lib.rs crates/micro-host-web/tests/system_bridge.rs
git commit -m "feat: expose Micro OS state to the web host"
```

### Task 2: JavaScript device-shell controller

**Files:**
- Create: `products/micro-web-player/src/device-shell.js`
- Create: `tests/web/device-shell.test.js`

- [ ] **Step 1: Write failing behavior tests**

```js
test("Counter starts after the reducer allocates its session", async () => {
  const calls = [];
  const shell = createDeviceShell({ system, startRuntime: async () => calls.push("start"), stopRuntime: () => calls.push("stop"), render() {} });
  await shell.intent("open-counter");
  assert.deepEqual(calls, ["start"]);
  assert.equal(shell.snapshot().screen, "AppRunning(Counter, 1)");
});

test("pointer coordinates map into the 800 by 480 screen", () => {
  assert.deepEqual(mapTouch({ clientX: 210, clientY: 120 }, { left: 10, top: 20, width: 400, height: 240 }), { x: 400, y: 200 });
});
```

- [ ] **Step 2: Verify RED**

Run: `node --test tests/web/device-shell.test.js`

Expected: FAIL because `device-shell.js` does not exist.

- [ ] **Step 3: Implement the controller**

Export `createDeviceShell`, `mapTouch`, and this board model:

```js
export const BOARD_MONITOR = { width: 800, height: 480, pixelClockHz: 16_000_000, touch: "GT911", expander: "CH422G", flashMiB: 8, psramMiB: 8 };
```

`intent()` dispatches an intent, reads `JSON.parse(system.snapshot())`, invokes `startRuntime()` only after `open-counter` returns a non-null `counterSession`, invokes `stopRuntime()` on `AppStopping`, dispatches `counter-stopped` after stop, then calls `render(snapshot)`. `mapTouch()` uses `Math.floor` and clamps x to 0–799 and y to 0–479.

- [ ] **Step 4: Verify and commit**

Run: `node --test tests/web/device-shell.test.js && npm run test:web:unit`

Expected: PASS.

```bash
git add products/micro-web-player/src/device-shell.js tests/web/device-shell.test.js
git commit -m "feat: add ESP32 simulator shell controller"
```

### Task 3: Device and monitor rendering

**Files:**
- Modify: `products/micro-web-player/index.html`
- Modify: `products/micro-web-player/src/main.js`
- Modify: `products/micro-web-player/src/style.css`
- Modify: `tests/web/counter.spec.js`

- [ ] **Step 1: Extend browser acceptance first**

```js
await expect(page.getByRole("button", { name: "Counter" })).toBeVisible();
await page.getByRole("button", { name: "Counter" }).click();
await page.getByRole("button", { name: "Add" }).click();
await page.getByRole("button", { name: "Add" }).click();
await expect(page.getByText("Count: 2")).toBeVisible();
await page.getByRole("button", { name: "Back" }).click();
await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
```

- [ ] **Step 2: Verify RED**

Run: `npm run build:web && npx playwright test tests/web/counter.spec.js`

Expected: FAIL because Launcher controls do not exist.

- [ ] **Step 3: Add the simulator DOM and wire it**

Create a device `section` labelled `ESP32-S3 Touch-LCD-7 simulator`, a `#system-screen` layer, a `#app-screen` MBC layer, and `aside` monitor fields using `data-monitor`. Use exact accessible names: `Counter`, `Settings`, `Back`, `Scan Wi-Fi`, `Connect Wi-Fi`, `Toggle backlight`, and `Safe Mode`.

Import `MicroWebSystem` and `createDeviceShell`. `startRuntime()` fetches `/apps/counter.mbc`, creates `MicroWebRuntime("app-screen", bytes, 10_000n)`, starts `createRuntimeLoop`, then dispatches `counter-started`. `stopRuntime()` stops/disposes it, empties `#app-screen`, then dispatches `counter-stopped`. Render reducer snapshot/action log in the monitor. Update touch output from `pointermove` and `pointerdown` through `mapTouch`.

- [ ] **Step 4: Style a clearly simulated board**

Render a 5:3 landscape device frame, visible `800 × 480` coordinate output, and a panel headed `Simulated board monitor`. Show RGB565 16 MHz, GT911, CH422G, 8 MiB Flash, 8 MiB PSRAM, Wi-Fi, backlight, Micro OS state, action log, and 10,000 instruction budget. Do not use text implying physical flash or electrical success.

- [ ] **Step 5: Verify and commit**

Run: `npm run test:web:unit && npm run build:web && npx playwright test tests/web/counter.spec.js`

Expected: PASS.

```bash
git add products/micro-web-player/index.html products/micro-web-player/src/main.js products/micro-web-player/src/style.css tests/web/counter.spec.js
git commit -m "feat: render the ESP32-S3 web simulator"
```

### Task 4: System and monitor acceptance

**Files:**
- Modify: `tests/web/counter.spec.js`
- Modify: `README.md`

- [ ] **Step 1: Add failing system monitor assertions**

```js
await page.getByRole("button", { name: "Settings" }).click();
await page.getByRole("button", { name: "Toggle backlight" }).click();
await expect(page.getByText("Backlight: Off")).toBeVisible();
await page.getByRole("button", { name: "Safe Mode" }).click();
await expect(page.getByText("Safe Mode")).toBeVisible();
await expect(page.getByRole("button", { name: "Counter" })).toBeDisabled();
await page.locator('[data-device-screen]').hover({ position: { x: 200, y: 120 } });
await expect(page.getByText(/Touch: 4\d\d, 2\d\d/)).toBeVisible();
```

- [ ] **Step 2: Verify RED**

Run: `npm run build:web && npx playwright test tests/web/counter.spec.js`

Expected: FAIL until settings, Safe Mode, and touch monitor behavior are complete.

- [ ] **Step 3: Document usage and boundary**

Add the `npm run dev:web` command and state: “The browser simulator executes the shared Runtime and Micro OS reducer, but does not emulate ESP-IDF, RGB timing, GT911 electrical I2C, PSRAM allocation, Wi-Fi radio, or physical hardware success.”

- [ ] **Step 4: Verify and commit**

Run: `npm run test:web:unit && npm run build:web && npx playwright test tests/web/counter.spec.js && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check && git diff --check`

Expected: all commands PASS.

```bash
git add README.md tests/web/counter.spec.js
git commit -m "docs: explain ESP32 simulator limits"
```

## Plan Self-Review

Task 1 implements real reducer state/actions; Task 2 implements deterministic runtime lifecycle, board constants, and pointer mapping; Task 3 renders the 800×480 device and uses real MBC execution; Task 4 proves Counter, system, touch, and documentation behavior. File names, public intent strings, snapshot names, and test commands are consistent throughout.
