# Micro App Platform MVP

A runnable multi-platform Micro App platform slice. Apps use a deliberately restricted TypeScript syntax, compile to versioned Micro Bytecode (MBC), execute in a budgeted Rust runtime, and render through LVGL 9 + SDL3 on macOS or WebAssembly + DOM in a browser.

The Counter App is real end to end:

```ts
const count = state(0);

ui.mount(
  ui.column([
    ui.text(bind(() => `Count: ${count.value}`)),
    ui.button("Add", {
      onClick: () => count.value++,
    }),
  ]),
);
```

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

## Quick start

```bash
npm install
npm run build:app
npm run demo
```

`npm run demo` compiles `apps/counter/app.ts`, loads the generated MBC, and opens a 480×320 window. Click **Add** to increment the existing LVGL label.

The first demo build downloads and compiles SDL 3.4.10 and LVGL 9.5.0. Later builds reuse the cache.

## Web Player

Install the one-time Web toolchain and start the browser runtime:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
npm install
npx playwright install chromium
npm run dev:web
```

Open the printed local URL. The page loads the same Counter MBC, executes `micro-ir`, `micro-vm`, and `micro-core` inside WebAssembly, and renders the Micro UI Tree through the DOM. DOM click callbacks enqueue the same `Event::Activate` handler IDs used by the native host.

The Web Player is also an interactive **ESP32-S3 Touch-LCD-7 simulator**. It
renders an 800×480 landscape device with the checked RGB565/16 MHz, GT911,
CH422G, 8 MiB Flash, and 8 MiB PSRAM profile. Launcher and Settings use the
shared `micro-os-core` reducer; Counter still executes the compiled MBC through
the shared Runtime. Use Counter → Add → Back, then Settings to inspect
reducer-backed backlight, Wi-Fi, and Safe Mode flows.

The browser simulator executes the shared Runtime and Micro OS reducer, but
does not emulate ESP-IDF, RGB timing, GT911 electrical I2C, PSRAM allocation,
Wi-Fi radio, or physical hardware success.

The device screen remains a responsive outer frame, while its contents render
on one fixed 800×480 logical canvas and scale uniformly to fit. Device UI
measurements therefore stay in logical pixels; pointer coordinates are mapped
back into the same 800×480 coordinate space.

Run the real-browser acceptance test with:

```bash
npm run test:web
```

The test builds TypeScript → MBC and Rust → Wasm, opens Chromium, clicks **Add** twice, and verifies `Count: 2` with no Runtime error. Generated Wasm, MBC, Vite output, Playwright reports, and dependencies stay untracked.

## Commands

```bash
# Compile TypeScript to apps/counter/dist/app.mbc
npm run build:app

# Run all platform-neutral Rust tests; no native download required
npm test

# Build the real macOS host and native bridge
cargo build -p micro-host-sdl --features native

# Run the hidden native end-to-end smoke mode
cargo run -p micro-host-sdl --features native -- \
  --smoke apps/counter/dist/app.mbc

# Open the visible Counter window
npm run demo

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
| `micro-compiler` | SWC parsing, subset diagnostics, lowering and `microc` CLI | None |
| `micro-lvgl` | Maps a `MicroUiTree` and `RenderPatch` to a narrow native UI trait | None at test time |
| `micro-host-sdl` | macOS event/timer loop and bridge ownership | LVGL + SDL3 with `native` feature |
| `micro-host-esp32` | ESP Runtime ownership, LVGL bridge, and C ABI | ESP-IDF component build |
| `micro-renderer-web` | Maps the same tree and patches to a narrow DOM trait | None at test time |
| `micro-host-web` | WebAssembly boundary, browser DOM bridge and activation queue | `web-sys` on `wasm32` |
| `micro-web-player` | Minimal browser product consuming MBC and the generated Wasm package | Browser DOM |
| `native` | CMake FetchContent and the C ABI bridge | LVGL 9.5.0 + SDL 3.4.10 |

The Core never imports LVGL, SDL3, ESP-IDF, macOS, or browser types. SDL,
ESP32-S3, and Web hosts load the same MBC through renderer-specific bridges.

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

The MVP supports:

- `const` and `let` identifier declarations
- number, string, Boolean and `null` values
- numeric arithmetic, equality and comparisons
- assignment and `++`/`--` on locals and `state.value`
- template literals
- blocks, `if`/`else`, `while`, and arrow handlers/bindings
- `state`, `bind`, `ui.mount`, `ui.column`, `ui.text`, and `ui.button`
- `{ onClick: () => ..., textStyle?: ... }` button options

The compiler rejects unsupported syntax with `path:line:column`, a stable `MTS...` code, and a message. Rejected features include JSX, classes, general functions, async/Promise, exceptions, runtime imports, arbitrary objects/arrays, spread, destructuring, computed properties, browser/Node globals and dynamic UI creation.

Editor declarations live in `sdk/index.d.ts`; they document the accepted surface but are not executed.

### Typography

`ui.text` accepts an optional style argument, and `ui.button` accepts an
optional `textStyle` option. The only public family/weight is
`font: "uiSans"` with `weight: "regular"`. The supported fixed metric pairs
are `12/14`, `14/18`, `18/24`, `24/32`, and `32/40` logical pixels, written as
`size/lineHeight`. Omitting a style resolves to `24/32` for Text and `14/18`
for Button on every renderer.

The embedded and browser assets cover printable ASCII, common Chinese
punctuation, U+FFFD, and all 3,755 GB2312 level-1 Han characters. Unsupported
literal glyphs are rejected by the compiler. Unsupported glyphs produced by a
runtime binding are replaced with U+FFFD and emit a host diagnostic.

## MBC v2

MBC begins with `MBC1`, a version, payload length and CRC-32. Version 2 stores
the optional immutable text style for every UI node in addition to constants,
state declarations, bytecode functions, and the static UI specification. The
codec is deterministic, rejects v1 and unknown versions explicitly, and
rejects truncated or malformed data without panicking.

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
- **First native build cannot reach GitHub:** restore network access and rerun the same command; fetched sources remain cached.
- **Stale native dependency cache:** remove only `target/native-deps` and rebuild. Do not remove the repository.
- **Unsupported TypeScript:** follow the first `MTS...` diagnostic. The language is intentionally an allow-list, not full TypeScript.
- **Bad or old MBC:** rebuild it with `npm run build:app`; incompatible files are rejected before Runtime initialization.
- **Web build tool missing:** install `wasm32-unknown-unknown` and `wasm-pack`, then rerun `npm run build:web`.
- **Browser test executable missing:** run `npx playwright install chromium`.

## MVP limitations

The UI tree is static after initialization. The available widgets are Column, Text and Button, with reactive text patches and minimal built-in styling. The desktop host has one fixed-size macOS window, and the Web Player is the shared runtime foundation rather than the full Studio. Async App effects, App network APIs, App persistence, packages, dynamic lists, source-level debugging, signing, and production distribution are deferred.

The approved architecture and step-by-step implementation record are under `docs/superpowers/`.
