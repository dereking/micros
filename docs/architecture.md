# Micro OS Architecture

> **Status (2026-10-26).** This document describes the behavior implemented in
> this checkout. Sections marked **Target** are design direction, not shipped
> code. If this document and the code disagree, the code wins. If it and the
> historical specs under `docs/superpowers/` disagree, this document wins.

## 1. What Micro OS is

Micro OS is a portable **application OS**: one App format and runtime, a Home
shell, and a set of host-provided system services, running on top of whatever
platform owns the machine.

It is not a kernel and does not aim to be one:

- it does not schedule threads, own virtual memory, or expose drivers as a
  kernel API;
- on ESP32 the kernel is ESP-IDF/FreeRTOS, the display stack is LVGL, and
  Wi-Fi/NVS/flash belong to the vendor stack;
- on macOS the process, window and event loop belong to SDL3/AppKit;
- in the browser the page owns the thread, and Micro OS runs as WebAssembly.

What Micro OS owns is the **App contract**: the bytecode format, execution
semantics, UI/state model, host API boundary, launcher and lifecycle, and the
failure policy for App code.

| Term | Meaning |
| --- | --- |
| App | an MBC program plus a manifest; not a native process |
| Host | adapter that owns the platform and renders the tree |
| shell / Home | the App installed at registry index 0, launched at boot |
| Runtime | a `micro-core` instance: event queue, state, bindings, UI tree, renderer |
| Capability | a host API category an App is allowed to call |
| Patch | a renderer instruction produced by a changed binding |

## 2. Principles

1. **One format, several hosts.** Apps ship as MBC. Hosts differ only in
   rendering, input and host-call implementations; no host compiles TypeScript.
2. **Deterministic by construction.** App code is a typed stack machine with no
   threads, timers or ambient I/O. Every effect goes through a host call, and
   every handler has an instruction budget.
3. **Bounded work per event.** A handler that exceeds 10,000 instructions
   stops. State writes completed before exhaustion remain committed and dirty
   bindings are still flushed.
4. **Static UI, reactive values.** The tree is built once by `ui.mount`; state
   changes produce patches for existing nodes. There is no runtime widget
   creation.
5. **Host owns chrome and lifecycle.** The app bar, power state, orientation,
   back gesture and App switching are host policy. Apps draw only content.
6. **Small trusted core.** `micro-ir`, `micro-vm` and `micro-core` have no
   platform dependencies; they are the conformance surface.
7. **Failure is local to the App.** On ESP32 an App fault returns to Home;
   macOS reports and keeps running; the Web Player stops the frame loop. The
   shell is the current exception: it is an App too, so its own failure aborts
   boot (see §10).

## 3. Layer map

```text
App source (restricted TypeScript)
        │  microc: parse → allow-list → lower → validate → encode
        ▼
      .mbc (MBC v28)
        │  decode → structural validation
        ▼
┌─────────────────────────────────────────────────────────┐
│ micro-core Runtime                                      │
│   EventQueue → VM handler → StateStore → bindings →     │
│   RenderPatch                                           │
└──────────────────────────┬──────────────────────────────┘
                           │ RenderPort
              ┌────────────┴────────────┐
              ▼                         ▼
       micro-lvgl                 micro-renderer-web
       (LVGL objects)             (DOM nodes)
              │                         │
   micro-host-sdl / micro-host-esp32   micro-host-web (Wasm)
              │                         │
   SDL3 / ESP-IDF + FreeRTOS          Browser
```

`micro-os-core` sits beside the runtime, not above it: it is the
host-independent lifecycle reducer for boot, setup, Wi-Fi and App-session
state. ESP32 drives it for the boot chain; the Web system layer drives the
full reducer (Wi-Fi scan/connect, backlight, safe mode). Runtime switching
itself remains host policy.

The dependency rule is one-way: platform code never enters `micro-ir`,
`micro-vm` or `micro-core`, so those crates stay testable without a display.

## 4. Components

| Crate | Responsibility | Platform dependency |
| --- | --- | --- |
| `micro-ir` | MBC model, codec, CRC-32, structural validation, icon catalog | None |
| `micro-vm` | Typed stack machine, host-call interface, instruction budgets | None |
| `micro-core` | Event queue, state slots, reactive bindings, UI tree, patches | None |
| `micro-os-core` | OS lifecycle reducer and Wi-Fi provisioning state | None |
| `micro-compiler` | SWC parsing, subset diagnostics, lowering, `microc` CLI | None |
| `micro-store` | `ScopedKv` / `ScopedFiles` contracts, path sandbox, native JSON store | None |
| `micro-json` | Path-based JSON query used by HTTP host calls | None |
| `micro-net` | HTTP validation/envelope, dictionary batching, macOS ureq + rustls transport | None |
| `micro-board-profile` | Board profile schema and compiled reference-board preset | None |
| `micro-lvgl` | Maps `MicroUiTree` and `RenderPatch` to a narrow native UI trait | None at test time |
| `micro-renderer-web` | Maps the same tree and patches to a narrow DOM trait | None at test time |
| `micro-host-sdl` | SDL event/timer loop, LVGL bridge, native host services | LVGL + SDL3 |
| `micro-host-esp32` | ESP Runtime ownership, LVGL bridge, C ABI, NVS/SD/Wi-Fi/HTTP | ESP-IDF component build |
| `micro-host-web` | WebAssembly boundary, DOM bridge, activation queue, system layer | `web-sys` on `wasm32` |
| `micro-web-player` | Browser product consuming MBC and the generated Wasm package | Browser DOM |
| `native` | CMake FetchContent and the shared C bridge, including `micro_appbar.c` | LVGL 9.5.0 + SDL 3.4.10 |

## 5. Execution model

### 5.1 Compile time

`microc` parses `.ts` with SWC, validates the allow-list, lowers to MBC and
encodes it. MBC sections are constants, state declarations, bytecode
functions, UI nodes, host-request descriptors and the App manifest. The current
version is 28; the decoder rejects every other version, truncated payloads, bad
CRCs, invalid references/jumps and excessive stack depth before execution.

Diagnostics carry `path:line:column` and a stable `MTS...` code. The compiler is
a development-host tool: firmware never compiles TypeScript on the device.

### 5.2 Boot on ESP32

1. Verify the 8 MiB Flash / 8 MiB PSRAM hardware class; abort on mismatch.
2. Initialize the RGB LCD, GT911 touch and CH422G expander and enable the
   backlight.
3. Mount the TF card. A missing card logs a warning and does not stop boot;
   `storage.available()` reports it to Apps.
4. Draw the smoke/health screen, then mark the board profile healthy and clear
   the screen for the shell.
5. Initialize the BOOT button, Wi-Fi STA, KV and SNTP clock, then create the OS
   reducer.
6. Dispatch the boot events (`BOOT_SAMPLED` … `SETUP_SKIPPED`). First-run setup
   is skipped in this slice.
7. Scan the raw `micro_app` partition for MBCs (up to 8 built-in Apps beside the
   shell) and read each manifest's `id`, `name` and `icon`.
8. Scan the card's top-level `/apps` directory, skip malformed images and ids
   already claimed by a built-in App, then append the rest to the launcher
   registry (eight slots total). A missing card or `/apps` directory leaves the
   built-ins only. The scan repeats before every Home boot once the card was
   mounted at startup; a card inserted later needs a reboot.
9. Destroy any previous runtime, load `shell.mbc` into PSRAM, create its
   Runtime, and install the host app bar.
10. Create the 30 ms LVGL timer that drives the tick.

### 5.3 Tick

Each tick, in host order:

1. drain host intents (Wi-Fi connect/disconnect, `os.launchIndex`, `os.goBack`);
2. reconcile bindings when the Wi-Fi state changed;
3. refresh the app bar's Wi-Fi icon and SSID (the bar owns its clock);
4. process the edge-swipe gesture and the BOOT button;
5. run `micro_runtime_tick` until the event queue is empty.

`tick()` handles exactly one event. The host drains the whole queue every
frame; there is no per-frame event cap, so a long queue extends the frame.
macOS runs this loop from SDL; the Web Player runs it from
`requestAnimationFrame` through the Wasm `tick`.

### 5.4 Handlers, budgets, patches

A host turns input into an `Event` carrying a handler id and, for value
widgets, the new value. The VM invokes the handler with a 10,000-instruction
budget. State writes dirty only the bindings that subscribed to those slots;
after the handler the runtime flushes them and emits patches (`SetText`,
`SetProgress`, `SetVisible`, `SetChecked`, `SetSelectionValue`, …), which the
renderer applies to existing LVGL or DOM objects.

If a handler exhausts its budget, that handler stops, later events remain
processable, and the UI is flushed to match the committed state.

### 5.5 UI model

Tree kinds: Column, Row, Text, Button, Surface, Icon, Input, Slider, Checkbox,
Dropdown, Roller, LED, Spinner, Scale, List, Tabview, Progress.

Layout is Delphi-style `left/top/width/height` plus edge anchors, expressed
with `ui.place`. Column and Row are pure containers with no automatic layout:
every child must be placed, and the three layout engines (native C, ESP32 C,
Web DOM) must agree. Text uses the fixed `uiSans` metric pairs
`12/14`, `14/18`, `18/24`, `24/32`, `32/40`.

The host reserves a 40 px app bar; the App tree mounts into a content container
(440 px in landscape, 760 px in portrait) so layouts measure the real remaining
height.

### 5.6 Async host calls

Most host calls are synchronous scalar reads or fire-and-forget actions.
`net.scanWifi`, `net.http*`, `net.lookupWords` and `os.delay` are asynchronous:
they register a callback and return no value. When the platform completes the
request it posts a result; the next tick drains it as `Event::HostResult` and
runs the 1-argument callback handler.

The VM sees a `HostAccess` trait. `NullHost` rejects every call, which is how a
host that has not wired a capability reports the gap.

## 6. Isolation, memory and scheduling

| Boundary | Mechanism | Enforced today |
| --- | --- | --- |
| App code vs runtime | typed bytecode, validated jumps/stack, instruction budget | yes |
| App vs hardware and OS | only `HostCallKind` calls reach the platform | yes |
| App data vs other Apps | per-App key/path namespacing and path sandbox | partial |
| shell vs other Apps | host policy: `os.launchIndex` only from the shell | partial |
| App vs process memory | none — one address space, no MMU | no |

The ESP32-S3 has no MMU and no virtual memory, so hardware process isolation is
not available. The VM is the sandbox: Apps cannot form pointers, call native
code, or reach the host except through the compiled `HostCallKind` enum.
Rust `unsafe` is confined to host bindings; ESP32 builds use `panic=abort` and
no panic may cross the C ABI.

All App work runs on the host's UI thread or task. On ESP32 that is the LVGL
task (16 KB stack), while Wi-Fi runs in an IDF task. Both macOS and ESP32 run
HTTP on a Rust worker thread so the UI never blocks on a request. There is no
preemption inside a handler, and the budget is instructions, not milliseconds,
so a slow handler stalls the frame.

MBC bytes are loaded into PSRAM, validated, and freed after the Runtime copies
what it needs. Runtime state and strings live on the heap. There is no per-App
heap quota; the instruction budget is the only runtime limit.

### Capability model

Every manifest can declare the host API categories it uses. MBC v27 appended an
optional allow-list to the metadata section (MBC v28 adds per-node visibility and
`os.setTitle`): `clock`, `kv`, `config`, `storage`,
`net`, `audio`, `json`, `device`, `os`. The compiler rejects unknown or duplicate
names; the codec rejects unknown names when decoding. `microc` also rejects any
host call outside the declared list at the call site (`MTS002`), so a mismatch
fails the build instead of the first launch.

`Runtime` wraps the host's `HostAccess` in a capability gate. Every
`HostCallKind` maps to one `Capability` (`HostCallKind::capability`), and a call
outside a declared list fails with `VmError::Host` before the host sees it.
`Some([])` is a valid, fully sealed App.

An omitted list is **not** unrestricted: it grants nothing. This is a
default-deny model, so a manifest must opt in to every category it uses. Every
shipped App declares a list, and the compiler test
`every_shipped_app_declares_every_host_capability_it_uses` keeps each list in
sync with the host calls in its source.

Still **Target**, not implemented:

1. Install-time grant/review UI; Settings has no capability surface yet.
2. Signed manifests, so a declared list is attributable to a publisher.
3. An audit trail for denials and a user-visible "App was denied" record.

Because `HostCallKind` is a compile-time enum, adding an API still bumps the
MBC version; gating itself did not change call encoding.

## 7. Storage

| Store | macOS | Web | ESP32 |
| --- | --- | --- | --- |
| `kv` (App state) | `~/.micros/kv` JSON | per-key `localStorage` | NVS `micro_cfg` via `micro_kv` |
| `config` (App settings) | `~/.micros/<app>/config.json` | `localStorage["micro:config:<app>"]` | NVS `micro_config` |
| `storage` (App files) | `~/.micros/sdcard/micros/apps/<id>/` | per-App `localStorage` | `/sdcard/micros/apps/<id>/` |

All three go through `micro-store` contracts. `ScopedKv` namespaces every key
by App id; `ScopedFiles` takes relative paths and rejects traversal, exposing
no host path and no cross-App handle. A pre-`~/.micros` native store migrates
from `~/.micrapp`. Legacy config values kept in `kv` are read through `config`
and promoted on first write. No quota is enforced on any host.

ESP32 partitions: `factory` (firmware), `micro_cfg` (NVS), `micro_app` (raw
MBC storage, 0x3F0000, ~4 MiB) and `coredump`. See
`firmware/micro-os-esp32/partitions_8m.csv`.

## 8. Host API surface

The full contract with per-host behavior lives in
[docs/host-api.md](host-api.md) and `sdk/index.d.ts`. Categories:

| Category | Examples | Shape |
| --- | --- | --- |
| `clock` | `now` | sync |
| `kv` | get/set number/text, remove | sync / action |
| `config` | get/set typed values | sync / action |
| `storage` | read/write/append/list/remove/mkdir | sync / action |
| `net` | Wi-Fi state, connect, scan, HTTP, dictionary | sync / async |
| `audio` | speak, stop, rate, URL audio, background music | action |
| `json` | path query over a JSON string | sync |
| `device` | name, chip, flash/PSRAM, reset reason, backlight, GPIO | sync / action |
| `os` | app name/icon, launch, go back, delay | sync / async |

Notes:

- `device.*` is real on ESP32; macOS and Web return profile constants.
- The Web host answers HTTP with canned data and returns 501 for
  header-aware requests and dictionary lookups, because browser `fetch` cannot
  bypass CORS and there is no native dictionary bridge.
- ESP32 has no audio codec yet: `playAudio` returns 0 and background audio is a
  no-op.
- Sync reads are allowed in bindings and handlers; writes and async requests
  are handlers-only.

## 9. System UI and lifecycle

The 40 px app bar is host chrome, shared by every screen: Wi-Fi state on the
left, the current screen's manifest name centred, and the clock plus a close
button on the right. macOS and ESP32 use `native/src/micro_appbar.c`; the Web
Player uses the DOM `.app-bar`. Apps never draw it.

Home is the MBC App at index 0. Launching is a runtime swap, not a task switch:
the host destroys the current Runtime and its root, then creates a fresh one for
the target MBC. Returning does the same in reverse. No App state survives the
swap by design; durable state belongs in `kv`, `config` or `storage`.

Host chrome behavior:

| Action | macOS | Web | ESP32 |
| --- | --- | --- | --- |
| Launch App | `os.launchIndex` | `os.launchIndex` | `os.launchIndex` |
| Back to Home | close, edge gesture | close, edge gesture | close, edge swipe, BOOT |
| Power off on Home | Device menu / close | Device dialog / close | close button |
| Orientation | reflow in place | cold boot | fixed 800×480 |

`micro-os-core` models the lifecycle as `EarlyBoot`, `SafeMode`,
`StorageReady`, `BoardProfileValidated`, `DisplayReady`, `SystemUiReady`,
`FirstRunSetup`, `Launcher`, `AppStarting`, `AppRunning`, `AppStopping`,
`AppError` and `Settings`. The reducer is pure: hosts translate platform
events into `Event` and apply the returned `Action`s.

## 10. Failure model

| Failure | Detection | Current behavior |
| --- | --- | --- |
| Corrupt or unsupported MBC | decode/validate | App is rejected; if it is the shell, boot aborts |
| App fails to launch (decode, capability gate, renderer) | `micro_runtime_boot_app` | ESP32 logs and reboots Home; other hosts report at the launch call site |
| Invalid jump, stack depth, reference | validation | MBC rejected before execution |
| Handler budget exhausted | VM | handler stops, earlier writes stay committed, patches flush, later events still run |
| Host call error | VM host error | ESP32 destroys the Runtime and boots Home; macOS logs and continues; Web stops the frame loop and reports |
| Renderer error | `RenderPort` | same path as a host call error |
| Rust panic on ESP32 | `panic=abort` | device resets; coredump goes to flash |
| Shell MBC failure | boot step 9 | `abort()`; no native fallback UI |
| Task watchdog (5 s) | IDF WDT | logged, panic disabled |

Only ESP32 has automatic App recovery: a failed App is replaced by Home. macOS
logs the error and keeps the current runtime; the Web Player stops its frame
loop and reports. No host shows an error page, a "Restart App" action or a safe
mode UI. The originally designed trusted error page was not implemented.
Coredump is enabled to flash as ELF but is not surfaced in the UI.

## 11. Security and update posture

Current:

- MBC protects integrity with CRC-32 only. There is no signature, publisher
  identity, version pinning or rollback counter.
- Manifest capabilities are enforced by the Runtime's host-call gate, but the
  field is optional and there is no install-time review or grant UI. An App that
  omits the list keeps full host access.
- MBC is data, not native code. There is no path from bytecode to executable
  memory, which removes the classic shellcode/ROP surface but does not make the
  host API safe.
- Wi-Fi credentials live in NVS. Secure boot and flash encryption are supported
  by the SoC but not enabled in this image.
- The Flash layout has one `factory` App partition; there is no OTA, A/B slot or
  rollback. Updates mean reflashing firmware and the `micro_app` partition.
- Coredump and reset reasons are available, but crash reporting is manual.

**Target** (not implemented): signed MBC envelopes with publisher keys,
install-time capability review and grants, A/B App slots with atomic switch,
secure boot plus flash encryption for production, and a built-in fallback shell
copied into firmware so a bad `shell.mbc` cannot brick the device.

## 12. Cross-host conformance

The same MBC and the same Runtime semantics run on all hosts; only rendering,
input and host-call implementations differ.

| Concern | macOS | Web | ESP32 |
| --- | --- | --- | --- |
| Renderer | LVGL objects | DOM nodes | LVGL objects |
| Frame source | SDL loop | `requestAnimationFrame` | 30 ms LVGL timer |
| Storage | files under `~/.micros` | `localStorage` | NVS + TF card |
| HTTP | ureq + rustls | canned / 501 | `esp_http_client` |
| Audio | `AVSpeechSynthesizer`, URL audio | `SpeechSynthesis` + `HtmlAudioElement` | no codec |
| `device.*` | profile constants | profile constants | real hardware |
| Input | mouse, trackpad | pointer events | GT911 touch |
| Orientation | reflow in place | cold boot | fixed landscape |

Verification is layered: platform-neutral Rust tests for IR/VM/core, fake
render ports for both renderers, a hidden native end-to-end smoke, and
Playwright against real MBC and Wasm in Chromium. The Rust core has no display
dependency, so most behavior is testable without a window.

## 13. Non-goals

- POSIX compatibility, ELF/native App loading, or a kernel scheduler
- An embedded JavaScript engine, React/JSX, or runtime code evaluation
- Emulating an MMU or per-App address spaces on the ESP32-S3
- Multi-window or background App multitasking in this phase
- Dynamic UI trees or runtime widget creation
- Compiling TypeScript on the device
- Replacing ESP-IDF, FreeRTOS, SDL, the browser or macOS

## 14. Maturity model

| Level | Name | What it means | Status |
| --- | --- | --- | --- |
| L0 | Runtime | MBC, VM, core, renderers, hosts | done |
| L1 | App platform | install partition + SD /apps, manifests, per-App storage, launcher, app bar | mostly done |
| L2 | Device OS | supervisor, capabilities, signing, update/rollback, shell recovery, diagnostics | partial: capability gate |
| L3 | Ecosystem | marketplace, publisher keys, stable SDK, telemetry | not yet |

To be an OS without overclaiming, the minimum is the L2 trust boundary:

1. **Shell recovery** — a fallback Home in firmware and a native recovery
   screen, so a bad shell MBC cannot brick boot.
2. **Capabilities** — default-deny declaration and runtime enforcement at the
   host-call boundary are done; install-time grant/review, a Settings surface
   and denial auditing are not.
3. **Signing** — signed MBC, publisher identity, version/rollback rules.
4. **Update** — A/B or staged update with atomic switch and rollback.

Background services, multi-board profile selection and power management are
device-maturity work after that. Until 1–4 exist, "Micro OS" describes the App
contract, not a trust boundary.

## 15. Extending the platform

### Adding a widget

1. Add the kind and its fields in `micro-ir/src/model.rs`; encode/decode it in
   `codec.rs` and bump `VERSION` (currently 28).
2. Accept the intrinsic in the compiler allow-list and lower it in
   `micro-compiler`.
3. Add the patch variant in `micro-core/src/ui.rs` and the flush branch in
   `runtime.rs`.
4. Implement it in `micro-lvgl` and `micro-renderer-web`.
5. Wire the C bridges: `native/src` for macOS and
   `firmware/micro-os-esp32/components/micro_runtime_ffi/placeholder.c` for
   ESP32. Layout, hit testing and patch application must stay identical.
6. Update `sdk/index.d.ts`, `docs/host-api.md` or `docs/ui-icons.md`, and add
   compiler plus renderer tests.

### Adding a host call

1. Add `HostCallKind` and its argument count in `micro-ir/src/model.rs`, then
   bump `VERSION`.
2. Add the matching arm to `HostCallKind::capability` so the call belongs to a
   manifest category, and declare that capability in the Apps that use it.
3. Add the compiler intrinsic and the `sdk/index.d.ts` declaration.
4. Implement `HostAccess` in `micro-host-sdl`, `micro-host-esp32` and
   `micro-host-web`. `NullHost` already rejects unwired calls.
5. Async calls return no value, register a callback, and deliver results
   through `drain_results` → `Event::HostResult`.
6. Document per-host behavior in `docs/host-api.md`; a host that cannot
   implement a call should degrade explicitly (stub value or 501), not lie.

### Adding a host

1. Implement `RenderPort` (or reuse `micro-lvgl` / `micro-renderer-web`) and
   `HostAccess`.
2. Boot loop: load and validate MBC, create the Runtime, drive `tick` from the
   platform frame source, translate input into `Event`s, and drain async host
   results.
3. Implement host chrome (app bar, power, back, orientation) and the storage
   contracts. Chrome belongs to the host, never to App code.
4. Add a hidden smoke test and update the platform support table in
   [README](../README.md).

Any change to MBC shape, opcodes or host-call encoding requires a version bump
and a rebuild of every App; there is no backward compatibility inside a
version.

## 16. References

- [README](../README.md) — product overview, build and test commands
- [docs/host-api.md](host-api.md) — per-call host behavior
- [docs/external-apps.md](external-apps.md) — SD-card Apps, slot limit and id collisions
- [docs/native-display.md](native-display.md) — macOS window, toolbar, layout
- [docs/ui-icons.md](ui-icons.md) — icon catalog and rendering contract
- [docs/development.md](development.md) — toolchain and host builds
- [sdk/index.d.ts](../sdk/index.d.ts) — the App-facing API surface
- [firmware/micro-os-esp32/partitions_8m.csv](../firmware/micro-os-esp32/partitions_8m.csv)
  — Flash layout
- [docs/superpowers/specs/2026-08-13-esp32-micro-os-foundation-design.md](superpowers/specs/2026-08-13-esp32-micro-os-foundation-design.md)
  — original design record (historical; see its status note)
