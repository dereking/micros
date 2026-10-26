# Micro App Platform MVP

A runnable multi-platform Micro App platform slice. Apps use a deliberately restricted TypeScript syntax, compile to versioned Micro Bytecode (MBC), execute in a budgeted Rust runtime, and render through LVGL 9 + SDL3 on macOS or WebAssembly + DOM in a browser.

**Micro OS** is the product name for this slice: a portable application OS made of an app runtime, a Home shell, and host-provided system services. It is not a kernel. On the ESP32 the kernel is still ESP-IDF/FreeRTOS, and on macOS and the Web the process and event loop belong to the host. See [docs/architecture.md](docs/architecture.md) for the layer map, the isolation model, and the current-versus-target boundaries.

The Counter App is real end to end:

```ts
const count = state(0);
const presses = state(0);
const level = state(3);
const power = state(0);
const note = state("micro");

ui.mount(
  ui.column([
    ui.text("Counter Studio", { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 }),
    ui.text(bind(() => `Count: ${count.value}`), { font: "uiSans", size: 24, weight: "regular", lineHeight: 32 }),
    ui.text(bind(() => {
      if (count.value > 0) { return "positive"; }
      if (count.value < 0) { return "negative"; }
      return "zero";
    })),
    ui.text(bind(() => `total presses: ${presses.value}`)),
    ui.button("Add", { onClick: () => { count.value++; presses.value++; } }),
    ui.button("Reset", { onClick: () => { count.value = 0; } }),
    ui.button("Double", { onClick: () => { count.value = count.value * 2; presses.value++; } }),
    ui.text(bind(() => { if (power.value === 1) { return "power: on"; } return "power: off"; })),
    ui.row([
      ui.text(bind(() => `battery: ${level.value / 10}`)),
      ui.progress(bind(() => level.value / 10)),
      ui.button("-", { onClick: () => { if (power.value === 1) { if (level.value > 0) { level.value = level.value - 1; } } } }),
      ui.button("+", { onClick: () => { if (power.value === 1) { if (level.value < 10) { level.value = level.value + 1; } } } }),
    ]),
    ui.switch(bind(() => power.value === 1), { onToggle: () => { power.value = 1 - power.value; } }),
    ui.input(bind(() => note.value), {
      placeholder: "type a note",
      onChange: (s) => { note.value = s; },
    }),
    ui.text(bind(() => `note: ${note.value}`)),
  ]),
);
```

The launcher also ships **momo**, a 墨墨-style vocabulary trainer written in
the same restricted TypeScript. It compiles to MBC v28, keeps a per-word
Ebbinghaus schedule in `kv`, and runs the four-step listen → spell → cloze →
definition flow through the same Runtime. Up to five local users can share one
install: momo prefixes every KV/config key with the active user's namespace
(`""` for the migrated default user, `u2:`–`u5:` for the others), shows the
active user's name read-only on the 用户 tab until 编辑 reveals the name and
Maimemo API key fields, names the active profile in the OS app-bar title, and
restores the last active user on launch. Its sync tab
imports the active account's real `today_items` into that queue on macOS and
enriches them through the native dictionary bridge. See
[apps/momo/README.md](apps/momo/README.md); the new host capabilities are
documented in [docs/host-api.md](docs/host-api.md). Per-App KV goes through
the shared `micro-store` `ScopedKv` contract (native JSON under `~/.micros/kv`,
Web per-key localStorage, ESP32 NVS on the `micro_cfg` partition via the
`micro_kv` component), while per-App configuration uses the unified `config`
host API — `~/.micros/<app>/config.json` on macOS, `localStorage["micro:config:<app>"]`
on Web, and the NVS `micro_config` namespace on ESP32 — so App code never writes
host-specific files itself. App-private files use the `storage` API, which is
sandboxed to `/sdcard/micros/apps/<app-id>/` on the ESP32's removable TF card,
`~/.micros/sdcard/micros/apps/<app-id>/` on macOS, and per-App localStorage in
the Web Player; it exposes no host path and no cross-App handle. A pre-`~/.micros`
native store is migrated from
`~/.micrapp` on first launch, and legacy config values that older Apps kept in
`kv` are read through `config` and promoted on the first write. The ESP32 wall
clock is SNTP-backed through
`micro_clock`, and the sync tab reads Maimemo's today-items, daily-progress and
per-word study-record endpoints through the native host's header-aware HTTP
request plus `micro-json`, then calls `net.lookupWords` for definitions and
example sentences.

There is no React, JSX, Node.js App runtime, or embedded JavaScript engine. Browser Apps still execute the Rust VM through WebAssembly; JavaScript only boots the host and pumps browser frames.

## Prerequisites

- macOS on Apple Silicon
- Xcode Command Line Tools: `xcode-select --install`
- stable Rust with Cargo, rustfmt, and Clippy
- Node.js and npm
- CMake 3.24 or newer
- Git and network access for the first native build
- `wasm32-unknown-unknown`, `wasm-pack`, and Playwright Chromium for Web development

SDL3 and LVGL do **not** need Homebrew packages. CMake fetches the pinned sources and caches them under `target/native-deps`.

### Platform support

| Host | Status |
| --- | --- |
| macOS native (`micro-host-sdl`) | Supported; Apple Silicon is the tested target. |
| Web browser | Supported; desktop and mobile browsers, with a responsive Device dialog. |
| Windows / Linux native | The SDL/LVGL C layer includes the portable SDL-drawn Device menu, a no-rpath/DLL-staging build path, and POSIX/Windows shims. There is still no packaged launcher and the build has not been verified on real Windows/Linux desktops, so this remains source-level groundwork, not supported output. |
| Native iOS / Android | Not supported yet; no host or packaging layer. Mobile browser access works through the Web Player. |

## Quick start

```bash
npm install
npm run build:app
npm run demo
```

`npm run demo` compiles `apps/counter/app.ts`, `apps/settings/app.ts`,
`apps/momo/app.ts`, and the launcher, then opens the native window. Use the
launcher to enter Counter, Settings, or momo.

The first demo build downloads and compiles SDL 3.4.10 and LVGL 9.5.0. Later builds reuse the cache.

### macOS app bundle

Running from the terminal still shows the Micro OS Dock icon (the host embeds the
artwork and sets `NSApplication.applicationIconImage`). For a normal
double-clickable app, package the same binary with its icon and compiled Apps:

```bash
npm run bundle:macos          # → dist/Micro OS.app
npm run start:macos           # package and open it
```

Only one visible copy runs at a time. The first launch takes an advisory lock at
`<MICROS_HOME>/instance.lock` (default `~/.micros/instance.lock`) and writes its
PID there. A second copy — `open -n "dist/Micro OS.app"`, another double click
of the raw binary, or `cargo run` while the bundle is open — shows a native
**Micro OS is already running** notice instead of opening a window. Dismissing
the notice raises the running window (restoring it when minimized), then the
duplicate exits. The lock is a kernel `flock`, so a crash never leaves a stale
guard behind; headless `--smoke` / `--os-smoke` runs skip it.

The bundle's `Resources/apps` carries the shell and the installed Apps, so a
Finder launch boots the home screen with no arguments. Regenerate the artwork
with `npm run icon:generate` (source: `scripts/generate-app-icon.swift`); see
[docs/native-display.md](docs/native-display.md#9-macos-应用图标与-app-包) for the
icon pipeline and bundle layout.

### Default OS app bar

Every screen — the Home shell and every launched App — gets an OS-owned 40 px
app bar above its content. Three zones share the same geometry on every host:
Wi-Fi on the left, the screen name (the shell's manifest name is "Home")
dead-centre, and the wall clock plus the close button on the right. The close
button is a 28 px circle with a 44 px touch target; on an App it returns to the
shell. Home is therefore just another App under the bar: its MBC draws only the
launcher grid and keeps no status bar of its own.

The Home launcher is a compact list of installed Apps built from shared OS
icons: a masthead with the `apps` glyph, a hairline rule, and eight fixed slots.
Each visible slot is one `ui.surface` (with an accessible `label`) with
decorative icon/text siblings above it, so the whole row is the tap target on
every host. Rows bind `os.appName/Icon` and hide themselves when the host
registry has no app at that index. App manifests name their icon (`"plus"`,
`"settings"`, `"edit"`), but a legacy single letter still renders. A manifest
also declares the host API categories it uses (`capabilities: ["net", "kv"]`);
the compiler rejects an undeclared host call and the runtime enforces the same
allow-list. Every shipped App declares only what it needs. See
[docs/ui-icons.md](docs/ui-icons.md) for the 42-icon catalog, the `ui.icon`
widget and the per-host rendering contract, and
[docs/host-api.md](docs/host-api.md#manifest-capabilities) for the capability
model.

Apps can also be installed without re-flashing: drop compiled `*.mbc` files in
the card's `/apps` directory (or the native/Web simulated card). They appear
after the built-in Apps, and a built-in App always wins an id collision. See
[docs/external-apps.md](docs/external-apps.md) for the directory layout,
eight-slot limit and failure policy.

The bar is host chrome, not App code: apps never draw it and future apps get it
for free. On macOS and ESP32 it is the shared LVGL module
`native/src/micro_appbar.c`; on Web it is the DOM `.app-bar` in the player. In
all three hosts the App UI tree (including the shell's) mounts into a content
container inset by 40 px (440 px high in landscape, 760 px in portrait), so App
layouts measure the real remaining height. On Home the close button powers the
device view off: the bar and content are replaced by a dark "Powered off"
surface, and tapping it (also the BOOT button on hardware) boots a fresh Home.
On an App the close button behaves like the edge-swipe/BOOT back gesture:
`os.goBack` / shell return.

### Desktop host orientation, toolbar and zoom

The macOS host has a **top icon toolbar** above the device canvas with
**Landscape**, **Portrait** and **Rotate** controls (hover an icon to see its
name). A fourth **Device** gear opens a VM-style menu: Power On / Power Off,
Force Restart (with confirmation), orientation, zoom presets and read-only
emulator information. macOS uses a native `NSMenu`; the portable SDL fallback
draws the same menu inside the window for other desktop builds. Switching direction changes the logical device resolution
(`800×480` ↔ `480×800`), resizes the OS app bar/content container, and re-runs
the existing `ui.place` anchors; the App runtime and its state are preserved. It
is a real layout change, not a bitmap rotation of a fixed canvas. The toolbar and
menu are SDL/AppKit chrome outside LVGL, so they never become part of the App/OS
layout or rotate with the device.

```bash
# start in portrait and fit the current display
npm run demo -- --orientation portrait --zoom fit
```

In a running window: a plain mouse wheel or two-finger trackpad swipe scrolls the
scrollable widget under the cursor (the active tab page, including its
scrollbar); `Cmd/Ctrl+R` toggles portrait/landscape, `Cmd/Ctrl+,` opens the
Device menu, `Cmd/Ctrl+0` fits, `Cmd/Ctrl+1` resets to 100%, and
`Cmd/Ctrl+=` / `-` (or `Cmd/Ctrl+wheel`) zooms.
`--rotate 90/270` remains a compatibility alias for `--orientation portrait`.
The full design — toolbar drawing/hit-testing, OS/app reflow, app layout changes,
and resolution/DPI handling — is in
[docs/native-display.md](docs/native-display.md).

## Development guide

For the full development workflow — one-time environment setup
(`npm run setup:dev`), environment self-check (`npm run check:env`), building each
host, and debugging the native / Web / Wasm targets — see
[docs/development.md](docs/development.md).

## Web Player

Install the one-time Web toolchain and start the browser runtime:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
npm install
npx playwright install chromium
npm run dev:web
```

Open the printed local URL. The page loads the launcher plus the Counter,
Settings, and momo MBCs, executes `micro-ir`, `micro-vm`, and `micro-core`
inside WebAssembly, and renders the Micro UI Tree through the DOM. DOM click callbacks enqueue the same `Event::Activate` handler IDs used by the native host.

The Web Player is also an interactive **ESP32-S3 Touch-LCD-7 simulator**. It
renders an 800×480 landscape device by default (switchable to 480×800 portrait)
with the checked RGB565/16 MHz, GT911,
CH422G, 8 MiB Flash, and 8 MiB PSRAM profile. The launcher, Settings, Counter
and momo all execute the same compiled MBC through the shared Runtime, so the
bytes rendered in the browser are the bytes the device boots. Use Counter →
Add → Back, then open Settings — its categories are a root tab bar (Network,
Display, Sound, Security, System, About). Brightness, Wi-Fi scan/connect,
speech rate and the storage-card status are wired to the host APIs; anything
the OS does not expose yet is a dimmed "Coming soon" row instead of a dead
control. Open momo to study the daily vocabulary plan.

The browser simulator executes the shared Runtime and Micro OS reducer, but
does not emulate ESP-IDF, RGB timing, GT911 electrical I2C, PSRAM allocation,
Wi-Fi radio, or physical hardware success.

Above the panel, a **Device** button opens the Web equivalent of the native
Device menu: Power Off / Power On, two-step Force Restart, orientation, and the
simulator profile. Power and orientation are host chrome, outside the device
canvas. The device screen remains a responsive outer frame, while its contents
render on a logical 800×480 (or 480×800) canvas and scale uniformly to fit.
Pointer coordinates are mapped back into the active logical space. Changing
orientation cold-boots Home because the DOM renderer is first-paint driven,
whereas the macOS host reflows in place. Scrollable tab pages use native overflow
scrolling, so a mouse wheel or two-finger trackpad swipe moves the page and its
slim scrollbar exactly as a drag does.

Run the real-browser acceptance test with:

```bash
npm run test:web
```

The test builds TypeScript → MBC and Rust → Wasm, opens Chromium, exercises the shell, Counter, Settings, and momo, checks the simulated SD launcher (including the built-in-wins id collision), and verifies the KV-backed momo schedule survives a page reload. Generated Wasm, MBC, Vite output, Playwright reports, and dependencies stay untracked.

## Commands

```bash
# Compile every app/*.ts to apps/*/dist/*.mbc
npm run build:app

# Run all platform-neutral Rust tests; no native download required
npm test

# Build the real macOS host and native bridge
cargo build -p micro-host-sdl --features native

# Run the hidden native end-to-end smoke mode
cargo run -p micro-host-sdl --features native -- \
  --smoke apps/counter/dist/counter.mbc

# Drive the momo flow through the real LVGL renderer (headless)
cargo test -p micro-host-sdl --features native --test momo_native

# Open the visible Counter window
npm run demo

# One-command native test: build + headless Counter/momo/OS smoke + demo window
npm run test:native
npm run test:native -- --smoke  # headless smoke only

# Build the Web Player
npm run build:web

# Start the browser development server
npm run dev:web

# Run Chromium end-to-end verification
npm run test:web
```

## Architecture

| Unit | Responsibility | Native UI dependency |
|---|---|---|
| `micro-ir` | MBC model, little-endian codec, CRC-32 and structural validation | None |
| `micro-vm` | Typed stack machine and per-handler instruction budgets | None |
| `micro-core` | FIFO events, state slots, reactive dependencies, UI tree and patches | None |
| `micro-os-core` | Host-independent OS lifecycle reducer: boot chain, setup, launcher, app sessions, Wi-Fi state | None |
| `micro-compiler` | SWC parsing, subset diagnostics, lowering and `microc` CLI | None |
| `micro-store` | Per-App `ScopedKv`/`ScopedFiles` contracts, path sandbox and native JSON store | None |
| `micro-json` | Path-based JSON query used by the HTTP host calls | None |
| `micro-net` | HTTP request validation/envelope, dictionary batching, and the in-process macOS ureq + rustls transport | None |
| `micro-lvgl` | Maps a `MicroUiTree` and `RenderPatch` to a narrow native UI trait | None at test time |
| `micro-host-sdl` | macOS event/timer loop and bridge ownership | LVGL + SDL3 with `native` feature |
| `micro-host-esp32` | ESP Runtime ownership, LVGL bridge, and C ABI | ESP-IDF component build |
| `micro-renderer-web` | Maps the same tree and patches to a narrow DOM trait | None at test time |
| `micro-host-web` | WebAssembly boundary, browser DOM bridge and activation queue | `web-sys` on `wasm32` |
| `micro-web-player` | Minimal browser product consuming MBC and the generated Wasm package | Browser DOM |
| `micro-board-profile` | Board profile schema, compiled reference-board preset and validation | None |
| `native` | CMake FetchContent and the C ABI bridge | LVGL 9.5.0 + SDL 3.4.10 |

The Core never imports LVGL, SDL3, ESP-IDF, macOS, or browser types. SDL,
ESP32-S3, and Web hosts load the same MBC through renderer-specific bridges.
For the full layer map, isolation model and target boundaries, see
[docs/architecture.md](docs/architecture.md).

## Runtime flow

1. `microc` parses `.ts` with SWC, validates the allow-list, and lowers it to MBC.
2. The decoder checks `MBC1` magic, version, payload length, CRC-32, references, jumps and stack depth before execution.
3. Runtime creates state slots, evaluates bindings while recording state reads, and creates one static UI tree.
4. The active host converts pointer input into a handler ID; Core appends it to the FIFO `EventQueue`.
5. VM executes the handler with a default 10,000-instruction budget.
6. Changed state slots dirty only their subscribed bindings. Writes in one handler are coalesced.
7. Changed binding values produce patches such as `SetText`; the active renderer updates the existing LVGL or DOM object.

If a handler exhausts its budget, that handler stops and later events remain processable. State writes completed before exhaustion remain committed, and dirty bindings are flushed so UI and state stay consistent.

## Supported TypeScript subset

The compiler supports:

- `const` and `let` identifier declarations
- number, string, Boolean and `null` values
- numeric arithmetic, equality and comparisons
- assignment and `++`/`--` on locals and `state.value`
- template literals
- blocks, `if`/`else`, `while`, and arrow handlers/bindings
- `state`, `bind`, `ui.mount`, `ui.place`, `ui.column`, `ui.row`, `ui.surface`, `ui.icon`, `ui.show`, `ui.text`, `ui.button`, `ui.progress`, `ui.switch`, `ui.input`, `ui.slider`, `ui.checkbox`, `ui.dropdown`, `ui.roller`, `ui.led`, `ui.spinner`, `ui.scale`, `ui.list` and `ui.tabview`
- host calls `app`, `os.*`, `clock`, `kv`, `config`, `storage`, `json`, `audio`, `net` and `device.*` as declared in `sdk/index.d.ts`
- App-private `storage` files (`available`, `exists`, `readText`, `writeText`, `appendText`, `list`, `remove`, `mkdir`, `size`) with relative, sandboxed paths
- `{ onClick: () => ..., textStyle?: ..., variant?: "default" | "selected" }` button options, `ui.surface` options `{ onClick?: () => void }`, `{ onToggle?: () => ... }` switch options, and `ui.input` options `{ placeholder?: string, textStyle?: ..., onChange?: (text: string) => void }`

The compiler rejects unsupported syntax with `path:line:column`, a stable `MTS...` code, and a message. Rejected features include JSX, classes, general functions, async/Promise, exceptions, runtime imports, arbitrary objects/arrays, spread, destructuring, computed properties, browser/Node globals and dynamic UI creation.

Editor declarations live in `sdk/index.d.ts`; they document the accepted surface but are not executed.

### Typography

`ui.surface(tone)` is a decorative rectangle for cards, pills, rules and page
canvases. Tones are `paper` (default), `sage`, `terracotta`, `line` and
`canvas`. It carries no children; place it before the content it sits behind.
Without options the surface is non-interactive. Passing
`{ onClick: () => ... }` makes the blank surface a tap target, while labels and
buttons stacked above it keep their own handlers. Web and macOS implement the
tap; ESP32 falls back to a plain container with no hit testing. On hosts without
a palette it degrades to a plain container.

`ui.show(widget, visible)` conditionally shows any widget. `visible` can be a
Boolean or a `bind` expression; hidden widgets keep their layout slot but stop
painting and receiving clicks, and hiding a container hides its children. This
is used by momo to hide “开始今日学习” once a study round is running, to hide the
grade row while an answer is being reviewed, and to keep the selected profile
read-only until 编辑 reveals its name and Token fields.

`os.setTitle(title)` replaces the OS app-bar title for the running App (it is
ignored by the single-App debug viewport, which has no bar). momo uses it to show
`momo · <current user>` and refreshes it after every switch or save.

`ui.text` accepts an optional style argument; `ui.button` accepts optional
`textStyle` and `variant` options, and `ui.input` accepts an optional
`textStyle` option. `variant: "selected"` paints the button with the
terracotta active-user palette on Web, macOS and ESP32, while the default
remains green. The only public family/weight is
`font: "uiSans"` with `weight: "regular"`. The supported fixed metric pairs
are `12/14`, `14/18`, `18/24`, `24/32`, and `32/40` logical pixels, written as
`size/lineHeight`. Omitting a style resolves to `24/32` for Text and `14/18`
for Button on every renderer. `ui.row`, `ui.progress`, and `ui.switch` carry no
text style: Row is a flex container, Progress displays a 0..1 fraction, and
Switch shows a checked state bound to a Boolean. `ui.input` is a single-line
editable text field whose value is a constant or binding; its `onChange`
handler receives the new text and may write it back to a `state`. Input text
defaults to `24/32` and can be reduced with `textStyle`, as momo does for its
compact profile fields.

The embedded and browser assets cover printable ASCII, common Chinese
punctuation, U+FFFD, all 3,755 GB2312 level-1 Han characters, and the
International Phonetic Alphabet (IPA Extensions, Spacing Modifier Letters,
combining diacritics, and phonetic Latin/Greek letters). IPA glyphs missing
from the CJK face are rendered through a pinned Noto Sans fallback on LVGL and
a `MicroUiIpa` Web face. Unsupported literal glyphs are rejected by the
compiler. Unsupported glyphs produced by a runtime binding are replaced with
U+FFFD and emit a host diagnostic.

## MBC format

MBC starts with `MBC1`, a version, payload length and CRC-32, followed by constants, state declarations, bytecode functions, the static UI specification, host-request descriptors and the App manifest. The current version is **28**. The codec is deterministic; it rejects every other version, truncated payloads, bad CRCs, and invalid references, jumps or stack depth before execution, without panicking. Any format change bumps the version, so an older MBC must be rebuilt rather than patched in place.

MBC is generated output. Edit the TypeScript source and rebuild rather than editing `app.mbc`.

## Testing

The workspace tests cover:

- MBC round trips, corruption, versioning, bad references/jumps and stack limits
- VM arithmetic, branches, locals, state, strings, type errors and exact budget exhaustion
- FIFO events, no-op writes, binding dependency replacement, coalescing and renderer failures
- accepted/rejected TypeScript plus source-located diagnostics
- lowering of Counter, locals, loops, conditionals and assignments
- compiler CLI success/failure behavior
- real Counter compile → load → two clicks → `Count: 2`
- LVGL tree/patch mapping through a fake bridge
- SDL3/LVGL hidden-window creation and activation queue
- full hidden native Counter execution to state `2` through queued SDL pointer clicks
- Web renderer tree/patch mapping through a fake DOM port
- shared Wasm activation queue FIFO behavior
- Chromium Counter execution through real MBC and WebAssembly to `Count: 2`

## Troubleshooting

- **`rustc` or `cargo` not found:** install stable Rust from [rustup.rs](https://rustup.rs), then install `rustfmt` and `clippy` components.
- **CMake or compiler missing:** install CMake and Xcode Command Line Tools.
- **First native build cannot reach GitHub:** clone SDL3/LVGL into `target/native-deps/sdl3-src` and `target/native-deps/lvgl-src`; `scripts/test-native.sh` detects them and builds offline.
- **Stale native dependency cache:** remove only `target/native-deps` and rebuild. Do not remove the repository.
- **Unsupported TypeScript:** follow the first `MTS...` diagnostic. The language is intentionally an allow-list, not full TypeScript.
- **Bad or old MBC:** rebuild it with `npm run build:app`; incompatible files are rejected before Runtime initialization.
- **Web build tool missing:** install `wasm32-unknown-unknown` and `wasm-pack`, then rerun `npm run build:web`.
- **Browser test executable missing:** run `npx playwright install chromium`.

## Current limitations

This slice is an application OS, not a kernel. The boundaries are deliberate:

- **UI**: the tree is static after `ui.mount`. There is no runtime widget creation, no dynamic list, and no source-level debugging. Widgets are the `ui` catalog in `sdk/index.d.ts`.
- **Execution**: one App is in the foreground; there are no background Apps, services, timers or IPC. Each handler runs with a 10,000-instruction budget, and all App work runs on the host's UI thread or task.
- **Isolation**: Apps execute in one address space as bytecode, sandboxed by the VM and the host API rather than by an MMU. There is no hardware process isolation on the ESP32-S3.
- **Security**: MBC carries a CRC, not a signature. Host access is default-deny: a manifest must declare `capabilities`, and the runtime rejects calls outside that allow-list at the host-call boundary. But the manifest is unauthenticated, there is no installer UI that reviews or grants capabilities, and cross-App isolation is still key/path namespacing rather than a security boundary.
- **Update**: the ESP32 image uses a single `factory` App partition. There is no OTA, A/B slot, rollback, marketplace or package format.
- **Recovery**: Home is an MBC App too, so an unreadable `shell.mbc` aborts at boot instead of falling back to native UI.
- **Hosts**: macOS and Web are supported. Windows/Linux native builds are source-level groundwork; native iOS/Android are out of scope.

See [docs/architecture.md](docs/architecture.md) for the supervisor model (signed capabilities, OTA, shell fallback) and the gap list.

The approved design records and step-by-step implementation plans remain under `docs/superpowers/`; they are historical documents, so current code and [docs/architecture.md](docs/architecture.md) win on any conflict.
