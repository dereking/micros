# ESP32-S3 Micro OS Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Boot Micro OS on the Spotpear/Waveshare ESP32-S3-Touch-LCD-7 V1.2 (N8R8), configure Wi-Fi, show a trusted launcher, and run the existing Counter MBC through the shared Rust Runtime and LVGL 9.

**Architecture:** ESP-IDF 5.5.4 owns startup, hardware, Wi-Fi, NVS, FreeRTOS, and the LVGL task. A CMake-built Rust static component owns the portable OS state reducer and existing MBC Runtime; a narrow C ABI and `micro-lvgl::NativeUi` bridge connect it to trusted C system UI and board drivers. The board BSP is adapted only from the official CC0/Apache-2.0 demo inputs and pins LVGL 9.5.0 plus Espressif's LVGL port.

**Tech Stack:** Rust 2024, Espressif Xtensa Rust toolchain, ESP-IDF 5.5.4, C/CMake, FreeRTOS, LVGL 9.5.0, `esp_lvgl_port` 2.8.0~1, `esp_lcd_touch_gt911` 1.2.0~2, NVS, Wi-Fi station mode.

---

## File map

- `crates/micro-board-profile/`: portable profile schema, validation, and JSON loading.
- `crates/micro-os-core/`: hardware-independent boot, Wi-Fi, launcher, and App lifecycle reducer.
- `crates/micro-host-esp32/`: Runtime owner, ESP LVGL bridge, and exported C ABI.
- `firmware/micro-os-esp32/main/`: ESP-IDF composition root and serial safe-mode command loop.
- `firmware/micro-os-esp32/components/micro_bsp_lcd7/`: reference-board LCD, touch, CH422G, and backlight support.
- `firmware/micro-os-esp32/components/micro_system_ui/`: trusted LVGL setup, launcher, settings, and error screens.
- `firmware/micro-os-esp32/components/micro_wifi/`: Wi-Fi station/NVS adapter producing typed system events.
- `firmware/micro-os-esp32/components/micro_runtime_ffi/`: CMake-to-Cargo integration and public C header.
- `profiles/esp32s3/spotpear-touch-lcd-7.json`: reviewed V1.2 N8R8 preset.
- `scripts/fetch-spotpear-demo.sh`: reproducible official Demo fetch and checksum verification.
- `third_party/NOTICE.md`: exact upstream provenance and retained-code notices.

### Task 1: Pin official sources and bootstrap the ESP32 toolchain

**Files:**
- Create: `scripts/fetch-spotpear-demo.sh`
- Create: `third_party/NOTICE.md`
- Create: `firmware/micro-os-esp32/TOOLCHAIN.md`
- Create: `tests/esp32_vendor_source.sh`
- Modify: `.gitignore`

- [ ] **Step 1: Write the failing provenance test**

Create `tests/esp32_vendor_source.sh` to assert the official URL, SHA-256, fixed ESP-IDF/LVGL versions, and ignored vendor cache:

```sh
#!/bin/zsh
set -euo pipefail
rg -q 'ESP32-S3-Touch-LCD-7-Demo.zip' scripts/fetch-spotpear-demo.sh
rg -q '5351d443eaa605cab1eb80d050d867c18e1ce2b33c9cbc78aae1b7bca040b038' scripts/fetch-spotpear-demo.sh
rg -q 'ESP-IDF 5.5.4' firmware/micro-os-esp32/TOOLCHAIN.md
rg -q 'LVGL 9.5.0' firmware/micro-os-esp32/TOOLCHAIN.md
/usr/bin/git check-ignore -q work/vendor/spotpear/demo.zip
```

- [ ] **Step 2: Run the test and verify it fails**

Run: `zsh tests/esp32_vendor_source.sh`

Expected: FAIL because the source-pinning files do not exist.

- [ ] **Step 3: Implement the fetcher and provenance record**

`scripts/fetch-spotpear-demo.sh` must download to `work/vendor/spotpear`, verify the known archive hash, and extract only `ESP-IDF/08_lvgl_Porting`:

```sh
#!/bin/zsh
set -euo pipefail
readonly url='https://files.waveshare.net/wiki/ESP32-S3-Touch-LCD-7/ESP32-S3-Touch-LCD-7-Demo.zip'
readonly sha='5351d443eaa605cab1eb80d050d867c18e1ce2b33c9cbc78aae1b7bca040b038'
readonly out='work/vendor/spotpear'
mkdir -p "$out"
curl -fsSL "$url" -o "$out/demo.zip"
printf '%s  %s\n' "$sha" "$out/demo.zip" | shasum -a 256 -c -
unzip -oq "$out/demo.zip" 'ESP32-S3-Touch-LCD-7-Demo/ESP-IDF/08_lvgl_Porting/*' -d "$out"
```

Record retrieval date `2026-08-13`, V1.2/N8R8 schematic identity, CC0 files `main.c` and `waveshare_rgb_lcd_port.c`, Apache-2.0 `lvgl_port.c`, the archive hash, and canonical links in `third_party/NOTICE.md`. State that the LVGL 8 port is reference-only and will be replaced by `esp_lvgl_port` for LVGL 9.

Pin ESP-IDF 5.5.4, target `xtensa-esp32s3-espidf`, LVGL 9.5.0, `esp_lvgl_port` 2.8.0~1, and GT911 component 1.2.0~2 in `TOOLCHAIN.md`. Add `work/vendor/` and all ESP-IDF build/managed-component directories to `.gitignore`.

- [ ] **Step 4: Install and verify the toolchain**

Install the pinned SDK and Espressif Rust toolchain outside tracked source:

```sh
mkdir -p work/toolchains
git clone --branch v5.5.4 --depth 1 --recursive \
  https://github.com/espressif/esp-idf.git work/toolchains/esp-idf-v5.5.4
work/toolchains/esp-idf-v5.5.4/install.sh esp32s3
cargo install espup --locked
espup install --targets esp32s3
. work/toolchains/esp-idf-v5.5.4/export.sh
. "$HOME/export-esp.sh"
idf.py --version
rustc +esp --version
rustc +esp --print target-list | rg '^xtensa-esp32s3-espidf$'
```

Expected: ESP-IDF reports 5.5.4 and the Xtensa target is present. Do not add either installation directory to Git.

- [ ] **Step 5: Fetch and build the untouched vendor demo**

Run the fetcher, then from the extracted `ESP-IDF/08_lvgl_Porting` directory run `idf.py set-target esp32s3 && idf.py build`. With only the board's UART-labelled Type-C port connected, run `idf.py flash monitor` and record LCD, touch, PSRAM, and build results in `firmware/micro-os-esp32/TOOLCHAIN.md`; `idf.py` discovers the single serial device automatically.

Expected: vendor widgets render, touch works, and the log reports 8 MB octal PSRAM. Stop if the untouched demo fails; the BSP must not be debugged inside Micro OS first.

- [ ] **Step 6: Re-run the test and commit**

Run: `zsh tests/esp32_vendor_source.sh && git diff --check`

Commit: `chore: pin ESP32 board sources and toolchain`

### Task 2: Add the versioned Board Profile and validator

**Files:**
- Create: `crates/micro-board-profile/Cargo.toml`
- Create: `crates/micro-board-profile/src/lib.rs`
- Create: `crates/micro-board-profile/tests/profile.rs`
- Create: `profiles/esp32s3/spotpear-touch-lcd-7.json`
- Modify: `Cargo.toml`

- [ ] **Step 1: Write failing profile tests**

Tests must load the checked-in JSON, assert `esp32s3`, 8 MB Flash/PSRAM, 800×480 RGB565, 16 MHz PCLK, GT911 on GPIO 8/9 with IRQ 4, and reject duplicate pins, unknown drivers, dimensions above the hardware class, and Flash/PSRAM mismatch:

```rust
#[test]
fn validates_the_spotpear_v12_n8r8_profile() {
    let profile = BoardProfile::from_json(include_str!(
        "../../../profiles/esp32s3/spotpear-touch-lcd-7.json"
    )).unwrap();
    profile.validate(&DriverCatalog::esp32s3_v1()).unwrap();
    assert_eq!((profile.display.width, profile.display.height), (800, 480));
    assert_eq!((profile.hardware.flash_mb, profile.hardware.psram_mb), (8, 8));
}
```

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test -p micro-board-profile`

Expected: FAIL because the package is absent.

- [ ] **Step 3: Implement focused schema types**

Use `serde`/`serde_json` and define `BoardProfile`, `HardwareClass`, `RgbDisplay`, `RgbTiming`, `Touch`, `ExpanderSignals`, `ResourcePolicy`, `DriverCatalog`, and `ProfileError`. Validation must build a `BTreeMap<u8, Vec<&str>>`, allowing only explicitly declared shared I²C pins while rejecting every other duplicate assignment. Reject schema versions other than `1` and catalog IDs other than `esp32s3-v1`.

The preset must encode the vendor pin order exactly:

```json
{
  "schemaVersion": 1,
  "id": "spotpear-esp32s3-touch-lcd-7-v1.2-n8r8",
  "chipFamily": "esp32s3",
  "hardware": { "flashMb": 8, "psramMb": 8, "psramMode": "octal" },
  "display": {
    "driver": "esp-lcd-rgb", "width": 800, "height": 480,
    "pixelClockHz": 16000000, "pclkActiveNegative": true,
    "hsync": 46, "vsync": 3, "de": 5, "pclk": 7,
    "data": [14,38,18,17,10,39,0,45,48,47,21,1,2,42,41,40],
    "timing": { "hPulse": 4, "hBack": 8, "hFront": 8, "vPulse": 4, "vBack": 8, "vFront": 8 }
  },
  "touch": { "driver": "gt911", "sda": 8, "scl": 9, "irq": 4, "resetExpander": 1 },
  "backlight": { "kind": "binary", "enableExpander": 2 },
  "resources": { "framebuffers": 2, "bounceBufferLines": 10 },
  "driverCatalog": "esp32s3-v1"
}
```

- [ ] **Step 4: Run tests and commit**

Run: `cargo test -p micro-board-profile && cargo test --workspace`

Commit: `feat: add ESP32-S3 board profile validation`

### Task 3: Model trusted Micro OS lifecycle as a pure reducer

**Files:**
- Create: `crates/micro-os-core/Cargo.toml`
- Create: `crates/micro-os-core/src/lib.rs`
- Create: `crates/micro-os-core/src/state.rs`
- Create: `crates/micro-os-core/src/wifi.rs`
- Create: `crates/micro-os-core/tests/lifecycle.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Write failing lifecycle tests**

Cover normal boot, BOOT-held Safe Mode, first-run skip, Wi-Fi success/failure, launcher→Counter→Home, App failure→error→restart, and prevention of App launch before `SystemUiReady`:

```rust
#[test]
fn counter_failure_returns_to_trusted_system_ui() {
    let mut os = booted_launcher();
    assert_eq!(os.dispatch(Event::OpenApp(AppId::Counter)), Action::StartApp(AppId::Counter));
    assert_eq!(os.dispatch(Event::AppFailed("budget".into())), Action::ShowAppError);
    assert_eq!(os.dispatch(Event::HomePressed), Action::ShowLauncher);
    assert_eq!(os.state(), State::Launcher);
}
```

- [ ] **Step 2: Verify failure**

Run: `cargo test -p micro-os-core`

Expected: FAIL because the crate is absent.

- [ ] **Step 3: Implement the reducer**

Define `State`, `Event`, and `Action` as closed enums. `MicroOs::dispatch` is the only transition entry point and returns one side-effect request; hardware adapters later report completion as another `Event`. Keep Wi-Fi reconnect policy deterministic with delays `[1, 2, 5, 10, 30]` seconds capped at 30.

- [ ] **Step 4: Run tests and commit**

Run: `cargo test -p micro-os-core && cargo test --workspace`

Commit: `feat: model Micro OS lifecycle`

### Task 4: Scaffold the reproducible 8 MB ESP-IDF firmware

**Files:**
- Create: `firmware/micro-os-esp32/CMakeLists.txt`
- Create: `firmware/micro-os-esp32/main/CMakeLists.txt`
- Create: `firmware/micro-os-esp32/main/idf_component.yml`
- Create: `firmware/micro-os-esp32/main/main.c`
- Create: `firmware/micro-os-esp32/partitions_8m.csv`
- Create: `firmware/micro-os-esp32/sdkconfig.defaults`
- Create: `tests/esp32_layout.sh`

- [ ] **Step 1: Write the failing layout test**

The test parses the CSV and asserts exact contiguous bounds ending at `0x800000`, then checks fixed component versions and N8R8 settings:

```sh
rg -q 'factory,app,factory,0x10000,0x380000' firmware/micro-os-esp32/partitions_8m.csv
rg -q 'micro_apps,data,littlefs,0x3A0000,0x440000' firmware/micro-os-esp32/partitions_8m.csv
rg -q 'CONFIG_ESPTOOLPY_FLASHSIZE_8MB=y' firmware/micro-os-esp32/sdkconfig.defaults
rg -q 'CONFIG_SPIRAM_MODE_OCT=y' firmware/micro-os-esp32/sdkconfig.defaults
rg -q 'version: "=9.5.0"' firmware/micro-os-esp32/main/idf_component.yml
```

- [ ] **Step 2: Verify failure, then create the scaffold**

Run: `zsh tests/esp32_layout.sh`; expect missing files.

Create the partition rows exactly as the approved design. Set ESP32-S3, 240 MHz CPU, QIO 80 MHz Flash, 8 MB Flash, octal 80 MHz PSRAM, 1000 Hz FreeRTOS tick, custom partition table, coredumps to Flash, and performance optimization. Pin component-manager dependencies:

```yaml
dependencies:
  idf: { version: "=5.5.4" }
  lvgl/lvgl: { version: "=9.5.0", public: true }
  espressif/esp_lvgl_port: { version: "=2.8.0~1", public: true }
  espressif/esp_lcd_touch_gt911: { version: "=1.2.0~2", public: true }
```

`app_main` initially logs reset reason, detected Flash/PSRAM size, and fails fast unless both are 8 MB.

- [ ] **Step 3: Build, test, and commit**

Run: `zsh tests/esp32_layout.sh && idf.py -C firmware/micro-os-esp32 set-target esp32s3 && idf.py -C firmware/micro-os-esp32 build`

Expected: the firmware links and its partition check reports no overlap.

Commit: `feat: scaffold 8 MB Micro OS firmware`

### Task 5: Integrate the shared Rust Runtime as an ESP-IDF component

**Files:**
- Create: `crates/micro-host-esp32/Cargo.toml`
- Create: `crates/micro-host-esp32/build.rs`
- Create: `crates/micro-host-esp32/rust-toolchain.toml`
- Create: `crates/micro-host-esp32/src/lib.rs`
- Create: `crates/micro-host-esp32/src/bridge.rs`
- Create: `crates/micro-host-esp32/tests/host.rs`
- Create: `firmware/micro-os-esp32/components/micro_runtime_ffi/CMakeLists.txt`
- Create: `firmware/micro-os-esp32/components/micro_runtime_ffi/placeholder.c`
- Create: `firmware/micro-os-esp32/components/micro_runtime_ffi/include/micro_runtime_ffi.h`
- Modify: `Cargo.toml`

- [ ] **Step 1: Write failing host ABI tests**

Test a `RuntimeHost<FakeNativeUi>` created from encoded Counter bytes: activation IDs remain FIFO, two activations produce `Count: 2`, stop removes App-owned nodes, and corrupt MBC returns `MICRO_ERR_MBC` without panic.

- [ ] **Step 2: Verify failure**

Run: `cargo test -p micro-host-esp32`

- [ ] **Step 3: Implement RuntimeHost and C ABI**

`RuntimeHost<B: NativeUi>` owns `Runtime<LvglRenderer<B>>` and exposes safe Rust `new`, `activate`, `tick`, and `stop`. ESP-only exports use opaque `micro_runtime_t *`, copy incoming MBC bytes before decode, catch panics, and fill a caller-owned diagnostic buffer:

```c
micro_runtime_t *micro_runtime_create(const uint8_t *mbc, size_t len,
                                      uint64_t budget, char *error, size_t error_len);
int micro_runtime_activate(micro_runtime_t *, uint32_t handler_id);
int micro_runtime_tick(micro_runtime_t *, char *error, size_t error_len);
void micro_runtime_destroy(micro_runtime_t *);
```

The CMake component follows the official `esp-idf-template` CMake integration, builds package `micro-host-esp32` as `staticlib` for `xtensa-esp32s3-espidf` with `-Zbuild-std=std,panic_abort`, adds `--cfg espidf_time64`, and links `pthread`, `newlib`, and the component library. Do not add a second copy of `micro-ir`, `micro-vm`, or `micro-core`.

Export the OS reducer through the same opaque-handle ABI so C never recreates its transition rules:

```c
micro_os_t *micro_os_create(void);
micro_action_t micro_os_dispatch(micro_os_t *, micro_event_t event);
micro_state_t micro_os_state(const micro_os_t *);
void micro_os_destroy(micro_os_t *);
```

Use fixed-width C enums with explicit discriminants mirrored by `#[repr(C)]` Rust enums, and add compile-time C assertions plus Rust tests for every discriminant.

- [ ] **Step 4: Test host and target builds**

Run:

```sh
cargo test -p micro-host-esp32
idf.py -C firmware/micro-os-esp32 fullclean build
```

Expected: host Counter tests pass and `libmicro_host_esp32.a` is linked into the firmware map.

- [ ] **Step 5: Commit**

Commit: `feat: link shared Rust Runtime into ESP-IDF`

### Task 6: Adapt the official LCD/touch Demo to the LVGL 9 BSP

**Files:**
- Create: `firmware/micro-os-esp32/components/micro_bsp_lcd7/CMakeLists.txt`
- Create: `firmware/micro-os-esp32/components/micro_bsp_lcd7/include/micro_bsp_lcd7.h`
- Create: `firmware/micro-os-esp32/components/micro_bsp_lcd7/micro_bsp_lcd7.c`
- Create: `firmware/micro-os-esp32/components/micro_bsp_lcd7/LICENSES/CC0-1.0.txt`
- Modify: `third_party/NOTICE.md`
- Modify: `firmware/micro-os-esp32/main/main.c`

- [ ] **Step 1: Add a compile-time board contract**

Static assertions and one host-parsed header test must verify 800×480, PCLK 16 MHz, GPIO order `[14,38,18,17,10,39,0,45,48,47,21,1,2,42,41,40]`, I²C 8/9, IRQ 4, CH422G reset EXIO1, and backlight EXIO2.

- [ ] **Step 2: Port only the hardware initialization**

Adapt the official CC0 `waveshare_rgb_lcd_port.c` pin/timing and CH422G sequence. Replace legacy I²C APIs with ESP-IDF 5.5 master-bus APIs. Create the RGB panel in PSRAM with two framebuffers and a 10-line bounce buffer, create GT911, then register both with `esp_lvgl_port` 2.8.0~1 using LVGL 9 RGB565.

Return a `micro_bsp_display_t` containing panel, touch, LVGL display, and input handles. Every function returns `esp_err_t`; do not use `ESP_ERROR_CHECK` below `app_main`. Backlight capability is binary on V1.2, so expose `micro_bsp_backlight_set(bool)` and show an on/off setting rather than a misleading continuous slider.

Initialize the custom `micro_cfg` NVS partition. Validate the compiled recovery profile before display initialization. If `pending` exists, validate and try it once; mark it `active` only after the LVGL health screen renders and touch initialization succeeds. On any failure erase `pending`, restore `active` or the compiled profile, and log the rollback reason.

- [ ] **Step 3: Build and run the board smoke screen**

Render a trusted screen with four corner targets and live touch coordinates. Flash and verify all corners, correct orientation, no red/blue swap, stable backlight, and no tearing during a 60-second color animation.

- [ ] **Step 4: Commit**

Run: `idf.py -C firmware/micro-os-esp32 build && git diff --check`

Commit: `feat: bring up the ESP32-S3 7-inch BSP`

### Task 7: Build the trusted launcher, settings, and navigation

**Files:**
- Create: `firmware/micro-os-esp32/components/micro_system_ui/CMakeLists.txt`
- Create: `firmware/micro-os-esp32/components/micro_system_ui/include/micro_system_ui.h`
- Create: `firmware/micro-os-esp32/components/micro_system_ui/micro_system_ui.c`
- Create: `firmware/micro-os-esp32/components/micro_system_ui/micro_system_theme.c`
- Modify: `firmware/micro-os-esp32/main/main.c`

- [ ] **Step 1: Define UI events and ownership test hooks**

Use one callback `micro_system_ui_event_cb(micro_system_ui_event_t, void *)`. Add debug counters for system-root and App-root objects so a host/firmware test can prove App teardown never deletes Home, Back, or the error screen.

- [ ] **Step 2: Implement the minimal trusted UI**

Create separate LVGL roots for splash, setup, launcher, settings, App, and App error. The launcher has status bar, Counter tile, Settings tile, Home, and Back. Settings contains Wi-Fi, binary backlight, screen timeout, language, device info, clear network, factory reset, and reboot rows. All LVGL mutations occur while holding the `esp_lvgl_port` lock on the UI task.

Implement the screen timeout with the LVGL inactivity counter: at the selected timeout hide the display root and switch the binary backlight off; the next GT911 press restores the backlight and consumes that wake press instead of activating an App control. Factory reset and clear-network actions require a trusted confirmation dialog.

- [ ] **Step 3: Verify navigation on hardware**

Flash and verify Launcher→Settings→Back→Launcher and that Home always restores the trusted launcher. Run for ten minutes and confirm LVGL object count returns to baseline after 50 open/close cycles.

- [ ] **Step 4: Commit**

Commit: `feat: add the Micro OS launcher and settings`

### Task 8: Add first-run Wi-Fi setup and durable settings

**Files:**
- Create: `firmware/micro-os-esp32/components/micro_wifi/CMakeLists.txt`
- Create: `firmware/micro-os-esp32/components/micro_wifi/include/micro_wifi.h`
- Create: `firmware/micro-os-esp32/components/micro_wifi/micro_wifi.c`
- Create: `firmware/micro-os-esp32/components/micro_wifi/micro_wifi_store.c`
- Modify: `firmware/micro-os-esp32/components/micro_system_ui/micro_system_ui.c`
- Modify: `firmware/micro-os-esp32/main/main.c`

- [ ] **Step 1: Add reducer tests for adapter events**

Test scan results, authentication failure, missing AP, timeout, success-before-save, offline skip, bounded reconnect, and clear-network transitions in `micro-os-core` before writing ESP code.

- [ ] **Step 2: Implement the ESP-IDF adapter**

The Wi-Fi event loop copies event data and posts `micro_wifi_event_t` messages to the system queue; it never calls LVGL. NVS keys are `wifi_pending`, `wifi_active`, `language`, `backlight`, and `screen_timeout`. Promote pending credentials only after `IP_EVENT_STA_GOT_IP`; erase pending on failure/cancel. Redact passwords from all logs.

- [ ] **Step 3: Implement first-run UI**

Add Chinese/English selection, AP list, LVGL keyboard with password masking, Connect, Retry, and Skip. On connection, start SNTP and update the status bar through a typed UI event. Keep the launcher usable offline.

- [ ] **Step 4: Hardware persistence test and commit**

Complete setup, power-cycle, verify automatic reconnect and persisted language/backlight, clear network, and verify the wizard returns.

Commit: `feat: add Micro OS Wi-Fi provisioning`

### Task 9: Run Counter MBC inside the launcher lifecycle

**Files:**
- Create: `firmware/micro-os-esp32/components/micro_runtime_ffi/micro_lvgl_bridge.c`
- Create: `firmware/micro-os-esp32/components/micro_runtime_ffi/counter.mbc.S.in`
- Modify: `firmware/micro-os-esp32/components/micro_runtime_ffi/CMakeLists.txt`
- Modify: `crates/micro-host-esp32/src/bridge.rs`
- Modify: `firmware/micro-os-esp32/main/main.c`
- Modify: `package.json`

- [ ] **Step 1: Add the failing lifecycle integration test**

Compile `apps/counter/app.ts`, create Runtime, inject the button handler twice, assert patches end at `Count: 2`, destroy Runtime, recreate it, and assert `Count: 0`.

- [ ] **Step 2: Generate and embed MBC reproducibly**

Add `build:esp32:app` to compile Counter into the firmware build directory. CMake converts the bytes to a linked binary symbol; generated `.mbc`, `.S`, and build output remain ignored. The firmware logs the embedded length and checksum before Runtime creation.

- [ ] **Step 3: Implement the LVGL bridge and lifecycle**

`micro_lvgl_bridge.c` implements create-column, create-label, create-button, set-label-text, and destroy-App-root callbacks. Button callbacks enqueue only `uint32_t handler_id`. The main UI loop drains activations into `micro_runtime_activate`, calls a bounded `micro_runtime_tick`, and translates nonzero results into `AppFailed` events.

- [ ] **Step 4: Hardware Counter acceptance and commit**

Flash, open Counter, tap Add twice, verify `Count: 2`, Home, reopen, and verify `Count: 0`. Confirm the same Counter MBC checksum as desktop/Web build output.

Commit: `feat: run Counter in Micro OS`

### Task 10: Add Safe Mode, App errors, full verification, and operating docs

**Files:**
- Create: `firmware/micro-os-esp32/main/safe_mode.c`
- Create: `firmware/micro-os-esp32/main/safe_mode.h`
- Create: `apps/budget-fixture/app.ts`
- Create: `docs/esp32-bringup.md`
- Modify: `README.md`
- Modify: `tests/workspace_layout.sh`

- [ ] **Step 1: Add failing recovery tests**

Test BOOT-held startup, invalid profile, corrupt MBC, instruction-budget exhaustion, three repeated App failures, network clearing, and factory-reset actions in `micro-os-core`. Add the infinite-loop fixture used by existing exact-budget behavior.

- [ ] **Step 2: Implement Safe Mode and trusted App errors**

Sample GPIO0 before UI startup. Safe Mode accepts only `status`, `clear-network`, `clear-profile`, `factory-reset`, `reboot`, and `help` on UART; every destructive command requires the exact command plus a second `confirm` line within 10 seconds. Invalid MBC and budget exhaustion show Restart App/Home and never reboot automatically.

- [ ] **Step 3: Document build, flash, recovery, and licenses**

Document toolchain sourcing, vendor fetch, firmware build, UART port selection, flash/monitor commands, first-run flow, expected log milestones, Safe Mode, factory reset, partition layout, and the no-OTA limitation. Link official vendor resources and the Rust CMake integration source.

- [ ] **Step 4: Run complete automated verification**

```sh
cargo fmt --all -- --check
cargo test --workspace
npm run test:web
cargo test -p micro-host-sdl --features native
zsh tests/workspace_layout.sh
zsh tests/esp32_vendor_source.sh
zsh tests/esp32_layout.sh
idf.py -C firmware/micro-os-esp32 fullclean build
git diff --check
```

Expected: all commands pass, firmware image fits `factory`, and generated/vendor artifacts are untracked.

- [ ] **Step 5: Run final physical acceptance**

Factory reset; configure Wi-Fi; verify launcher, Settings, Counter `0→1→2`, Home/reopen reset, power-cycle persistence, invalid-MBC error, budget error, BOOT Safe Mode, and UART clear-network. Save the serial transcript and firmware SHA-256 under the release notes, not as an unbounded build artifact.

- [ ] **Step 6: Commit and push**

Commit: `docs: complete ESP32 Micro OS bring-up`

Push the verified master only after `git status --short` is empty and the remote head is checked.
