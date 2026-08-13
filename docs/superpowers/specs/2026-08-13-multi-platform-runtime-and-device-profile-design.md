# Multi-Platform Runtime and Device Profile Design

## 1. Product definition

Micro is a portable application platform. A TypeScript App is compiled once into MBC and runs with the same language, event, state, and instruction-budget semantics on embedded devices, desktop systems, and browsers.

The product family has four user-facing parts:

- **Micro OS**: firmware that runs MBC Apps on an embedded chip family such as ESP32-S3.
- **Micro Web Player**: an end-user browser runtime that opens and runs an MBC App.
- **Micro Studio**: a developer-facing browser simulator and debugger built on the same Web Runtime.
- **Micro Desktop Host**: a native macOS, Windows, and Linux runtime, evolving from the current SDL3 host.

These products share one compiler, MBC specification, Runtime Core, and conformance suite. They are not separate virtual-machine implementations.

## 2. Core invariants

Every supported host must preserve these observable behaviors:

- identical MBC admission checks and version handling;
- FIFO event ordering;
- identical instruction-budget accounting and failure behavior;
- identical StateStore writes, no-op detection, dependency tracking, and binding flush order;
- stable node, handler, state, and function identifiers;
- the same Platform Service capability names and permission checks;
- deterministic behavior for APIs that do not explicitly represent time, randomness, network, or device input.

Renderers may differ in pixels, text metrics, and native control details. Semantic conformance is mandatory; pixel fidelity is a separately stated renderer capability.

## 3. Repository organization

The platform remains in the `micros` monorepo while interfaces and the MBC ABI are still evolving.

```text
micros/
├── crates/
│   ├── micro-ir/                 # MBC model, codec, validation
│   ├── micro-vm/                 # bytecode execution and budgets
│   ├── micro-core/               # events, state, bindings, UI tree
│   ├── micro-compiler/           # TypeScript to MBC; development host only
│   ├── micro-platform-api/       # portable clocks, storage, network and device capabilities
│   ├── micro-board-profile/      # versioned board profile schema and validation
│   ├── micro-renderer-lvgl/      # LVGL renderer adapter
│   ├── micro-renderer-web/       # browser renderer
│   ├── micro-host-desktop/       # SDL3 host for macOS, Windows and Linux
│   ├── micro-host-web/           # WebAssembly/JavaScript host boundary
│   └── micro-host-esp32/         # ESP-IDF host boundary
├── firmware/
│   └── micro-os-esp32/           # bootable ESP32 family firmware
├── products/
│   ├── micro-web-player/         # embeddable/end-user browser player
│   └── micro-studio-web/         # simulator, inspector and board configurator
├── profiles/
│   └── esp32s3/                  # reviewed board presets
├── apps/
└── sdk/
```

The current `micro-lvgl` becomes `micro-renderer-lvgl`. The current `micro-host-sdl` becomes `micro-host-desktop` only after Windows and Linux build coverage exists; compatibility aliases may remain during migration.

## 4. Portable Runtime architecture

The compiler stays on a development computer or server. Devices and browsers receive an MBC image, not TypeScript source and not arbitrary native code.

```text
TypeScript + SDK
      |
      v
micro-compiler
      |
      v
versioned MBC
      |
      v
micro-ir -> micro-vm -> micro-core
                         |       |
                         |       +--> Platform Service API
                         v
                  Micro UI Tree/Patches
                         |
          +--------------+---------------+
          |              |               |
          v              v               v
       Web DOM/       Desktop LVGL     ESP32 LVGL
       Canvas Host      SDL3 Host       ESP-IDF Host
```

Host-specific code owns input, display, storage, network, clocks, lifecycle, logging, and installation. Core never imports browser, SDL, LVGL, ESP-IDF, FreeRTOS, GPIO, or filesystem types.

## 5. Web Runtime and browser products

The first implementation phase builds one shared `micro-host-web` runtime used by both browser products.

### 5.1 Micro Web Player

The Player exposes a small embeddable API:

```text
createPlayer(container, options)
loadMbc(bytes)
start()
pause()
sendInput(event)
dispose()
```

It loads MBC, creates a Web renderer, translates pointer and keyboard input into platform events, and runs Runtime ticks without exposing internal mutable state to the containing page.

### 5.2 Micro Studio

Studio composes the same Player with development tools:

- device viewport, scale, orientation, safe area, and pixel density;
- mouse-to-touch translation and configurable physical buttons;
- App reload and deterministic restart;
- logs, Runtime errors, instruction-budget failures, and event queue inspection;
- read-only StateStore and binding dependency inspection;
- performance counters for VM instructions, event latency, binding reevaluations, patches, and frame time;
- Board Profile editor, validator, import, and export;
- optional connection to a physical device over a later transport adapter.

The first Web renderer uses browser-native layout/painting for fast development and accessibility. Exact LVGL-in-WebAssembly pixel emulation is a later renderer profile, not a prerequisite for the shared Web Runtime.

### 5.3 Browser safety

The browser host applies the same App capability model planned for devices. Storage is namespaced by App ID. Network, clipboard, sensors, and host integration are denied unless declared and granted. Runtime instruction budgets prevent one handler from running indefinitely; the browser host also yields between work batches to keep the page responsive.

## 6. ESP32 family release model

A release is identified by Micro OS version, MBC ABI range, chip family, and driver catalog version. It is not required to be a single binary file.

Example:

```text
Micro OS 1.0.0 for ESP32-S3
├── esp32s3-quad-8m.bin
├── esp32s3-quad-16m.bin
├── esp32s3-octal-16m.bin
├── manifest.json
└── profiles/
```

Images in one release contain the same Runtime and features. Separate images are permitted only for boot-critical properties that cannot safely be deferred: flash mode and capacity class, PSRAM wiring/mode, partition layout, and secure-boot or flash-encryption policy.

Display, touch, buttons, encoders, SD, audio, sensors, bus frequencies, rotations, and ordinary GPIO assignments are selected by Board Profile at provisioning time.

## 7. Board Profile

`micro-board-profile` defines a versioned, portable schema. The same profile is consumed by Micro Studio, provisioning tools, and Micro OS.

A profile contains:

- schema version, profile ID, name, chip family, and optional board revision;
- display controller, bus, pins, resolution, color format, rotation, reset, and backlight;
- touch controller, bus, address or chip-select, interrupt, reset, calibration, and coordinate transform;
- buttons, encoders, SD, audio, sensors, LEDs, battery monitoring, and other declared peripherals;
- resource policy such as display-buffer size and PSRAM preference;
- driver-catalog compatibility range and optional signature metadata.

Validation rejects duplicate pins, reserved or unavailable pins, incompatible bus sharing, unsupported controllers, invalid dimensions, unsafe frequencies, missing required signals, resource requests that exceed the selected hardware class, and unknown schema versions.

Profiles are data, not executable driver code. A device can only select drivers compiled into its signed Micro OS driver catalog.

## 8. Provisioning and recovery

Display configuration cannot depend on a working display. First boot therefore exposes a display-independent provisioning path:

1. Detect chip revision, flash, PSRAM, and immutable security state.
2. Enter provisioning through USB serial initially and a Wi-Fi browser flow later.
3. Select a reviewed board preset or create an advanced profile.
4. Probe only buses and devices that can be discovered safely; do not assume SPI display auto-detection.
5. Validate and write the profile as `pending` in a dedicated NVS namespace.
6. Reboot, initialize drivers, render a confirmation screen, and mark the profile `active` only after a health checkpoint.
7. On failure, restore the previous active profile and return to safe provisioning mode.

A documented boot gesture and USB command clear a bad pending profile. Factory reset removes user Apps and profiles without modifying the signed firmware image.

## 9. Storage and update boundaries

Firmware OTA and App delivery are independent:

- ESP-IDF OTA slots update Micro OS, its driver catalog, and Runtime.
- A dedicated App data partition stores installed MBC packages and metadata.
- NVS stores small, infrequently changed system and Board Profile configuration.
- App state uses per-App namespaces and quotas separate from system configuration.

Before activating new firmware, the updater checks that installed MBC versions and the active Board Profile are supported. Before installing an App, the App manager validates checksum, size, MBC version, declared capabilities, and signature policy.

## 10. Error and compatibility model

- Unknown MBC versions are rejected before Runtime creation.
- Unknown Board Profile schemas or unavailable drivers enter safe provisioning mode.
- App failure does not terminate the host or launcher.
- A renderer failure identifies the node and operation and leaves system recovery input available.
- Instruction-budget exhaustion terminates only the current handler and records a diagnostic.
- Host service failures return structured effects to the App rather than panicking the Runtime.
- Release manifests declare exact MBC ABI, profile schema, driver catalog, and minimum hardware-class compatibility.

## 11. Conformance testing

The same fixtures run across Rust-native, WebAssembly, desktop, and ESP32 test targets:

- MBC codec, corruption, limits, and version fixtures;
- VM instruction and exact-budget fixtures;
- Event ordering, state, binding, and patch fixtures;
- capability permission and host-service contract fixtures;
- Board Profile parse, validation, migration, pin-conflict, and resource-limit fixtures;
- golden Counter interaction ending in `Count: 2`;
- trace comparison: identical input/event sequences must produce identical semantic traces across hosts.

Renderer-specific suites separately test layout, input hit-testing, screenshots where stable, and device drivers.

## 12. Delivery decomposition

This platform is delivered as independently testable subprojects:

1. **Shared Web Runtime foundation**: compile the current IR/VM/Core to WebAssembly, define the Web Host API, and run Counter through a minimal Web renderer.
2. **Web Player**: package the shared runtime as an embeddable end-user component.
3. **Micro Studio MVP**: add device viewport, App loading, restart, logs, event/state inspection, and performance counters on the same runtime.
4. **Board Profile schema and Studio editor**: define portable profiles, validation, presets, import, and export before hardware provisioning depends on them.
5. **ESP32-S3 Runtime bring-up**: port Core/VM constraints, integrate ESP-IDF and LVGL, and boot one bundled Counter MBC on one reference board.
6. **Universal ESP32-S3 provisioning**: add driver catalog, USB setup, pending/active profile recovery, and hardware-class release images.
7. **App manager and update system**: installation, launcher, per-App storage, signatures, capability grants, App delivery, and firmware OTA compatibility checks.
8. **Additional hosts**: harden desktop Windows/Linux support, then evaluate mobile hosts and a high-fidelity LVGL WebAssembly renderer.

The next implementation plan covers only subproject 1. Each later subproject receives its own reviewed design and plan.

## 13. Explicitly deferred items

The first Web Runtime foundation does not include ESP32 drivers, firmware OTA, App marketplace services, accounts, cloud synchronization, collaborative editing, mobile packaging, exact LVGL browser pixels, arbitrary third-party native drivers, or production code signing. Its output is a browser-executed Counter using the same MBC and semantic Runtime as the native host.
