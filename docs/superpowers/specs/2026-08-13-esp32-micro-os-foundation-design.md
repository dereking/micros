# ESP32-S3 Micro OS Foundation Design

## 1. Goal and acceptance target

This phase builds the first bootable Micro OS foundation for the Spotpear/Waveshare ESP32-S3-Touch-LCD-7 capacitive-touch board. The confirmed hardware class is ESP32-S3 with 8 MB Flash, 8 MB octal PSRAM, an 800×480 RGB LCD, ST7262 panel controller, GT911 touch controller, and CH422G I/O expansion.

The release must boot to a resilient system shell, configure Wi-Fi, show an App launcher, run the existing Counter MBC through the shared Rust Runtime, and return safely to the launcher. A successful hardware acceptance run reaches `Count: 2` after two touch activations without using a second VM implementation.

This phase establishes one reviewed reference-board preset. It does not yet claim support for arbitrary ESP32-S3 boards.

## 2. Product boundary

Micro OS has three layers:

```text
Micro OS system shell
├── boot and recovery
├── first-run and Wi-Fi setup
├── launcher, settings and system error pages
└── App lifecycle
              │
              v
Shared Rust Runtime
├── MBC admission and decoding
├── VM and instruction budgets
├── EventQueue and StateStore
├── binding evaluation
└── Micro UI Tree and patches
              │
              v
ESP32-S3 board support
├── reviewed vendor-demo-derived BSP
├── LVGL 9 and ESP LCD RGB
├── GT911 touch and CH422G control
├── NVS, Wi-Fi, Flash and FreeRTOS
└── ESP-IDF
```

The compiler remains a development-host tool. The firmware embeds or installs MBC; it never compiles TypeScript on the device.

The system shell is trusted native UI, not an MBC App. It must remain available when an App is corrupt, exceeds its instruction budget, or fails to render. System UI and App UI share one LVGL instance but use distinct root objects. Starting or stopping an App creates or destroys the App root and Rust Runtime without restarting the device or LVGL.

The existing `micro-ir`, `micro-vm`, `micro-core`, and `micro-lvgl` semantics remain authoritative. ESP32-specific code must not enter those crates.

## 3. Implementation approach

ESP-IDF owns boot, tasks, Wi-Fi, NVS, board drivers, LVGL integration, and hardware recovery. Rust owns the portable MBC Runtime and the lifecycle of an executing App. A narrow C ABI connects the ESP-IDF system to the Rust host.

This is preferred over an all-Rust board port because the reference board's vendor examples already contain validated RGB timing, GT911, CH422G, PSRAM, and LVGL integration. It is preferred over a C Runtime port because two VM implementations would break cross-host conformance.

The bridge exposes operations in these categories only:

- create, tick, stop, and destroy an App Runtime;
- create and patch App-owned LVGL nodes;
- enqueue integer handler activations from LVGL callbacks;
- return structured Runtime and renderer diagnostics to the system shell.

No Rust panic, C exception, or borrowed pointer may cross the ABI. LVGL access stays on the designated UI task.

## 4. Official Demo provenance and use

The board hardware baseline comes from the official ESP32-S3-Touch-LCD-7 ESP-IDF example package linked by the vendor documentation. Before extracting code, the implementation records:

- the canonical download URL;
- archive filename and SHA-256 checksum;
- upstream release or retrieval date;
- ESP-IDF, ESP display-panel, and LVGL versions used by the example;
- upstream license files and per-file notices;
- every retained source file and the reason it is needed.

The untouched example is first built and exercised on the board to confirm LCD, touch, PSRAM, and Wi-Fi. The product repository then retains only the permitted minimum board-support code: pin assignments, RGB timing, GT911 and CH422G initialization, backlight control, required panel callbacks, and necessary configuration.

Vendor demo pages, sample business logic, generated binaries, dependency caches, and downloadable archives are not committed. If the upstream license does not permit source redistribution, a pinned fetch-and-patch workflow reproduces the BSP instead. `third_party/NOTICE.md` documents provenance and local modifications in either case.

Primary references:

- [Spotpear ESP32-S3-Touch-LCD-7 documentation](https://spotpear.cn/wiki/ESP32-S3N8R8-7inch-LCD-Display-TouchScreen-800x480-LVGL-CAN-Sensor-RS485.html)
- [Waveshare ESP32-S3-Touch-LCD-7 resources](https://docs.waveshare.net/ESP32-S3-Touch-LCD-7/Resources-And-Documents/)

## 5. Repository layout

```text
firmware/
└── micro-os-esp32/
    ├── CMakeLists.txt
    ├── main/                       # boot coordinator and system lifecycle
    ├── components/
    │   ├── micro_bsp_lcd7/         # vendor-derived reference-board BSP
    │   ├── micro_system_ui/        # setup, launcher, settings, error pages
    │   ├── micro_wifi/             # scan, connect, reconnect and persistence
    │   └── micro_runtime_ffi/       # C declarations and Rust linkage
    ├── partitions_8m.csv
    └── sdkconfig.defaults
crates/
├── micro-board-profile/            # versioned profile model and validation
└── micro-host-esp32/                # Rust Runtime and LVGL bridge lifecycle
profiles/
└── esp32s3/
    └── spotpear-touch-lcd-7.json
third_party/
└── NOTICE.md
```

The first Board Profile records the confirmed hardware class, resolution, RGB mapping and timing, GT911 bus and transform, CH422G-controlled reset/backlight signals, resource policy, and compatible BSP/driver-catalog identifiers. Profiles remain declarative data and cannot inject driver code.

## 6. Boot and recovery state machine

Boot follows explicit states so every failure has a recovery destination:

```text
EarlyBoot
  -> SafeMode (BOOT held or recovery flag)
  -> StorageReady
  -> BoardProfileValidated
  -> DisplayReady
  -> SystemUiReady
  -> FirstRunSetup (network not configured)
  -> Launcher (configured or setup skipped)
  -> AppRunning
  -> AppError -> Launcher or AppRunning(restart)
```

Early boot initializes serial logging, NVS, PSRAM checks, watchdog policy, reset-reason reporting, and BOOT-button sampling before any App starts. Invalid profile data, unavailable compiled drivers, or display initialization failures never fall through to App startup.

Holding BOOT during startup enters Safe Mode. Safe Mode does not launch an App. UART commands provide version/status reporting, network/profile clearing, and factory reset. This path remains usable if the display configuration is broken.

The reference-board profile is compiled into the firmware as a recovery preset. A saved profile is staged as `pending` and becomes `active` only after board initialization and a system-UI health checkpoint. Failure discards the pending value and restores the last active or compiled recovery profile.

## 7. First-run and Wi-Fi behavior

When no network configuration exists, the system shell presents a first-run wizard:

1. Select Chinese or English.
2. Scan and select a Wi-Fi network.
3. Enter the password using an on-screen keyboard.
4. Connect and distinguish authentication failure, missing access point, timeout, and internal failure.
5. Save credentials only after a successful connection.
6. Show device name, Micro OS version, chip, 8 MB Flash, 8 MB PSRAM, and active Board Profile.
7. Finish into the launcher.

The wizard offers an explicit offline skip. A failed connection never blocks the launcher. Saved credentials live in the system NVS namespace and are not visible to Apps. The system reconnects with bounded backoff and updates the status bar without blocking the UI task.

Credentials use ESP-IDF's protected storage facilities available under the selected device security policy. This phase does not claim production secure boot or flash encryption; release metadata must state when credentials are stored without those protections.

## 8. Launcher and system UI

The launcher is a tablet-style Micro OS home screen:

- a status bar with device name, Wi-Fi state, time, and system health;
- an App grid containing Counter and Settings in the first image;
- persistent system navigation for Home and Back;
- no background App multitasking in this phase.

Opening Counter hides the launcher root, creates a fresh App root, decodes the embedded Counter MBC, creates the Rust Runtime, and renders its Micro UI Tree through `micro-lvgl`. Touch callbacks enqueue integer handler IDs; Rust processes those activations through the existing FIFO EventQueue and patches LVGL labels after StateStore binding updates.

Back stops the Runtime, destroys only App-owned objects, and restores the launcher. Reopening Counter creates a deterministic fresh Runtime. App corruption, budget exhaustion, or renderer failure transitions to a trusted error page with Restart App and Home actions; it does not reboot the device.

Settings includes:

- Wi-Fi network management;
- display brightness and screen timeout;
- language;
- device, hardware, profile, and version information;
- clear network configuration;
- factory reset;
- reboot.

## 9. Storage and the 8 MB image

The first image deliberately uses a single factory App partition. Actual linked-image size is measured before adding OTA. The initial 8 MB layout is:

| Partition | Offset | Size | Purpose |
|---|---:|---:|---|
| `nvs` | `0x9000` | `0x6000` | system and Wi-Fi NVS |
| `phy_init` | `0xF000` | `0x1000` | radio calibration |
| `factory` | `0x10000` | `0x380000` | Micro OS firmware, 3.5 MB |
| `micro_cfg` | `0x390000` | `0x10000` | profile/recovery NVS, 64 KB |
| `micro_apps` | `0x3A0000` | `0x440000` | MBC packages and metadata, 4.25 MB |
| `coredump` | `0x7E0000` | `0x20000` | crash diagnostics, 128 KB |

These entries end exactly at `0x800000`. The implementation validates the CSV and fails the build if the detected Flash size is not the expected hardware class.

App data and system configuration use different partitions and namespaces. Factory reset erases network, profile overrides, installed Apps, and App state, but it does not rewrite the firmware image or compiled recovery profile.

OTA, when added, is a measured product decision. An 8 MB release may later provide separate local-flash and OTA layouts if two safe firmware slots leave insufficient App capacity.

## 10. Error and concurrency rules

- The LVGL UI task is the only owner of LVGL object mutation.
- Wi-Fi callbacks post typed system events instead of updating UI directly.
- Rust App ticks are bounded and scheduled without starving LVGL or Wi-Fi tasks.
- Instruction-budget exhaustion terminates the current handler, records a diagnostic, flushes the Runtime's defined partial-state behavior, and returns control to the system shell.
- Unknown MBC versions and corrupt checksums are rejected before UI creation.
- NVS writes are transactional at the state-machine level: new profile or credential data is not marked active before validation/connection succeeds.
- Watchdog resets, brownouts, and panics are reported with reset reason at the next boot.
- Repeated App failure never triggers an automatic reboot loop.

## 11. Verification strategy

Host-side tests cover logic that does not require the board:

- Board Profile parsing, schema rejection, driver compatibility, pin conflicts, RGB dimensions, bus sharing, and 8 MB resource limits;
- boot-state transitions and every recovery edge;
- Wi-Fi state transitions, bounded reconnect behavior, and credential activation rules;
- launcher/App lifecycle including repeated start/stop;
- existing MBC, VM, event, state, binding, renderer, Web, and desktop conformance suites.

Firmware checks pin the ESP-IDF, Rust target, LVGL, and vendor BSP inputs. CI or a reproducible local command builds the 8 MB image and validates partition bounds and linked image size.

The physical-board acceptance sequence is:

1. Boot logs report ESP32-S3, 8 MB Flash, 8 MB PSRAM, reset reason, profile ID, display, touch, NVS, Wi-Fi, and Runtime status.
2. A factory-reset device completes or skips Wi-Fi setup and reaches the launcher.
3. Touch input works across the screen and the launcher opens Counter.
4. Two Add taps display `Count: 2` through the shared Rust EventQueue, StateStore, binding, and LVGL patch path.
5. Home returns to the launcher; reopening Counter creates a fresh instance.
6. Power cycling preserves language, brightness, and successful Wi-Fi configuration.
7. Invalid MBC and an instruction-budget fixture show the system error page without rebooting.
8. Holding BOOT enters Safe Mode, and UART can clear bad configuration.

## 12. Scope exclusions and next phase

This phase excludes OTA delivery, an App marketplace, remote installation, App signatures, capability prompts, cloud accounts, background multitasking, arbitrary Board Profile editing on-device, Wi-Fi browser provisioning, Bluetooth provisioning, and production secure-boot/flash-encryption rollout.

After the reference board passes acceptance, the next phase adds universal ESP32-S3 provisioning: driver-catalog manifests, USB configuration, reviewed profile selection, pending/active rollback across multiple boards, and release images separated only by boot-critical Flash/PSRAM classes.
