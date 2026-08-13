# Micro App Platform MVP

A runnable macOS-first Micro App platform slice. Apps use a deliberately restricted TypeScript syntax, compile to versioned Micro Bytecode (MBC), execute in a budgeted Rust runtime, and render through LVGL 9 + SDL3.

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

There is no React, JSX, browser, Node.js App runtime, or JavaScript engine. Node/npm only provide familiar repository commands and TypeScript editor declarations.

## Prerequisites

- macOS on Apple Silicon
- Xcode Command Line Tools: `xcode-select --install`
- stable Rust with Cargo, rustfmt, and Clippy
- Node.js and npm
- CMake 3.24 or newer
- Git and network access for the first native build

SDL3 and LVGL do **not** need Homebrew packages. CMake fetches the pinned sources and caches them under `target/native-deps`.

## Quick start

```bash
npm install
npm run build:app
npm run demo
```

`npm run demo` compiles `apps/counter/app.ts`, loads the generated MBC, and opens a 480×320 window. Click **Add** to increment the existing LVGL label.

The first demo build downloads and compiles SDL 3.4.10 and LVGL 9.5.0. Later builds reuse the cache.

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
| `native` | CMake FetchContent and the C ABI bridge | LVGL 9.5.0 + SDL 3.4.10 |

The Core never imports LVGL, SDL3, or macOS types. A future ESP32-S3 host can load the same MBC and implement the renderer/host boundary independently.

## Runtime flow

1. `microc` parses `.ts` with SWC, validates the allow-list, and lowers it to MBC.
2. The decoder checks `MBC1` magic, version, payload length, CRC-32, references, jumps and stack depth before execution.
3. Runtime creates state slots, evaluates bindings while recording state reads, and creates one static UI tree.
4. LVGL converts pointer input into a handler ID; Core appends it to the FIFO `EventQueue`.
5. VM executes the handler with a default 10,000-instruction budget.
6. Changed state slots dirty only their subscribed bindings. Writes in one handler are coalesced.
7. Changed binding values produce patches such as `SetText`; LVGL updates the existing object.

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
- `{ onClick: () => ... }` button options

The compiler rejects unsupported syntax with `path:line:column`, a stable `MTS...` code, and a message. Rejected features include JSX, classes, general functions, async/Promise, exceptions, runtime imports, arbitrary objects/arrays, spread, destructuring, computed properties, browser/Node globals and dynamic UI creation.

Editor declarations live in `sdk/index.d.ts`; they document the accepted surface but are not executed.

## MBC v1

MBC begins with `MBC1`, a version, payload length and CRC-32. Its payload contains constants, state declarations, bytecode functions and a static UI specification. The codec is deterministic and rejects truncated or malformed data without panicking.

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
- full hidden native Counter execution to state `2`

## Troubleshooting

- **`rustc` or `cargo` not found:** install stable Rust from [rustup.rs](https://rustup.rs), then install `rustfmt` and `clippy` components.
- **CMake or compiler missing:** install CMake and Xcode Command Line Tools.
- **First native build cannot reach GitHub:** restore network access and rerun the same command; fetched sources remain cached.
- **Stale native dependency cache:** remove only `target/native-deps` and rebuild. Do not remove the repository.
- **Unsupported TypeScript:** follow the first `MTS...` diagnostic. The language is intentionally an allow-list, not full TypeScript.
- **Bad or old MBC:** rebuild it with `npm run build:app`; incompatible files are rejected before Runtime initialization.

## MVP limitations

The UI tree is static after initialization. The available widgets are Column, Text and Button, with reactive text patches and minimal built-in styling. The host has one fixed-size macOS window. Async effects, network APIs, persistence, packages, dynamic lists, stylesheets, accessibility, hot reload, source-level debugging, signing, ESP32-S3 hosting and production distribution are deferred.

The approved architecture and step-by-step implementation record are under `docs/superpowers/`.
