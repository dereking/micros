# Micro App Platform MVP Design

**Status:** Approved in conversation on 2026-08-13; awaiting written-spec review  
**Target:** macOS MVP  
**Primary goal:** Compile a deliberately restricted TypeScript application into a portable Micro Bytecode container, execute it in a platform-neutral Rust runtime, and render an interactive Counter through LVGL 9 and SDL3.

## 1. Product outcome

The repository will contain a runnable end-to-end platform slice rather than disconnected demonstrations. A developer writes an App using a TypeScript-shaped, event-driven SDK:

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

`npm run build:app` invokes the Rust compiler and produces `app.mbc`. `npm run demo` builds the App when needed, builds the vendored native dependencies on first use, and opens a macOS window showing `Count: 0` and an `Add` button. Each click is converted into a runtime event and updates the existing LVGL label to `Count: 1`, `Count: 2`, and so on without rebuilding the UI tree.

This MVP establishes the boundaries needed for a future ESP32-S3 host. It does not attempt to provide general TypeScript or a React-compatible runtime.

## 2. Decisions

- App syntax is TypeScript without JSX or React semantics.
- Bindings are explicit through `bind(() => expression)`.
- The compiler is implemented in Rust and uses SWC's Rust parser.
- The compiler emits a versioned binary MBC container; MBC is interpreted rather than transpiled into Rust.
- Runtime, VM, state, events, UI tree, and renderer interfaces remain independent of LVGL, SDL3, and macOS.
- LVGL 9.5.0 and SDL 3.4.10 are pinned and fetched by CMake during the first native build.
- LVGL's SDL2 driver is not used. A small native bridge connects LVGL's display/input APIs to SDL3.
- Event handlers execute with a fixed instruction budget, defaulting to 10,000 VM instructions per event.
- The first renderer creates a static UI tree once and applies incremental property patches afterward.

## 3. Repository architecture

```text
apps/counter/app.ts
        │
        ▼
micro-compiler (SWC parse, subset and semantic validation, lowering)
        │
        ▼
micro-ir (.mbc encode/decode and validation)
        │
        ▼
micro-runtime ── micro-vm
  EventQueue      instruction budget
  StateStore      bytecode execution
  bindings
  Micro UI Tree
        │ RenderPort / PlatformEvent
        ▼
micro-lvgl (LVGL object adapter)
        │
        ▼
micro-host-sdl + native C bridge (SDL3 window, framebuffer, pointer, clock)
```

The planned workspace units are:

- `crates/micro-ir`: MBC types, binary codec, format validation, and opcodes. It has no dependency on SWC or native UI libraries.
- `crates/micro-compiler`: SWC parsing, supported-syntax validation, local semantic checks, lowering, diagnostics, and the `microc` executable.
- `crates/micro-vm`: typed values, stack machine, function execution, and instruction-budget enforcement. It depends only on `micro-ir`.
- `crates/micro-core`: event queue, state slots, binding dependency tracking, UI tree, render patches, and runtime orchestration. It depends on `micro-ir` and `micro-vm`.
- `crates/micro-lvgl`: implementation of the renderer port against a narrow native bridge API. Core never imports it.
- `crates/micro-host-sdl`: macOS executable and event/tick loop. It wires the runtime to the LVGL renderer and SDL3 host.
- `native/`: CMake project, pinned dependency fetch declarations, `lv_conf.h`, and the small C bridge.
- `sdk/`: TypeScript declarations for editor completion and compile-time API documentation; no JavaScript runtime is shipped.
- `apps/counter`: TypeScript Counter source and generated MBC output directory.

All cross-layer APIs use stable Rust data such as node IDs, handler IDs, `UiNode`, `RenderPatch`, and `PlatformEvent`. LVGL object pointers and SDL types do not cross into Core.

## 4. Supported TypeScript subset

SWC supplies parsing, source spans, and TypeScript AST nodes. It does not provide full TypeScript type checking. `micro-compiler` therefore performs precise validation for the supported SDK and value types and rejects everything else with a source-located diagnostic.

Supported constructs are deliberately small:

- `const` and `let` declarations with identifier patterns;
- number, string, Boolean, and `null` literals;
- arithmetic `+`, `-`, `*`, `/`, comparisons, equality, and Boolean operators;
- assignment and `++`/`--` on local variables or `.value` of a state handle;
- template literals whose interpolations have supported scalar types;
- expression statements, blocks, `if`/`else`, and `while`;
- arrow functions only where required by `bind` and event properties;
- the SDK calls `state`, `bind`, `ui.mount`, `ui.column`, `ui.text`, and `ui.button`;
- array literals only for UI children;
- object literals only for known UI option objects, initially `{ onClick }`;
- lexical capture of state handles and supported scalar constants by binding and handler arrows.

Explicitly rejected constructs include imports other than an optional type-only SDK import, JSX/TSX, user-defined classes and general functions, recursion, `async`/`await`, Promise, exceptions, generators, `new`, dynamic property access, arbitrary objects and arrays, spread, destructuring, modules with runtime imports, `eval`, and browser/Node globals.

The compiler checks SDK arity and prop names, scalar operations, state assignment compatibility, binding return compatibility, exactly one `ui.mount`, stable ownership of UI nodes, and handler/binding placement. Unsupported syntax is a compile error, never silently ignored or delegated to a JavaScript engine.

## 5. SDK semantics

`state(initial)` allocates a state declaration in MBC and gives the compiler a typed state handle. `.value` reads or writes its runtime slot.

`bind(() => expression)` marks a function as reactive. During evaluation, the Runtime records every state slot read by that function and updates the reverse subscription map. A subsequent write marks only subscribers of the changed slot dirty. Dirty bindings are reevaluated after the current handler completes and coalesced so several writes in one handler cause at most one patch per binding.

`ui.column(children)` creates a static container node. `ui.text(valueOrBinding)` creates a label. `ui.button(label, { onClick })` creates a button with a static label and handler reference. `ui.mount(root)` identifies the only root node.

The MVP UI tree is immutable after initialization. Reactive values may patch supported properties, initially text content. Dynamic lists, conditional node creation, keys, reconciliation, stylesheets, and layout mutation are deferred.

## 6. MBC format and VM

MBC v1 is a deterministic little-endian binary container. It starts with the ASCII magic `MBC1`; its CRC-32 covers every byte after the fixed header. It has these sections:

1. Header: magic bytes, format version, section count, payload length, and checksum.
2. Constant pool: UTF-8 strings, IEEE-754 numbers, Booleans, and null.
3. State declarations: initial constant and scalar type.
4. Functions: function kind (`init`, `binding`, or `handler`), local count, maximum stack depth, captures, and instruction bytes.
5. Static UI specification: stable node IDs, node kind, children, static props, binding IDs, and handler IDs.

The initial stack-machine instruction set covers constants, locals, state reads/writes, scalar conversion, arithmetic/comparison/Boolean operations, string concatenation, pop/duplicate, conditional and unconditional jumps, and return. Every instruction has a specified stack effect. The decoder validates indices, jump targets, function kinds, stack depth, section sizes, UTF-8, version, and checksum before the App is admitted to Runtime.

The VM exposes a host interface for state reads and writes. It does not know about LVGL, events, or SDL. Each invocation receives an instruction budget. Each decoded instruction consumes one unit, including jumps. At zero the VM returns `BudgetExceeded { function_id, executed }`. Runtime reports the failure, discards the remaining work for that event, and continues processing later host events. Tests may construct a backward-jump function directly even if a particular source loop is optimized during compilation.

## 7. Runtime data flow

Initialization proceeds as follows:

1. Decode and validate the entire MBC container.
2. Create `StateStore` slots from state declarations.
3. Materialize the static `MicroUiTree` with stable node IDs.
4. Evaluate bindings while recording state reads.
5. Send a full initial tree to `RenderPort`.

A button click proceeds as follows:

1. SDL3 reports pointer input to the host bridge; LVGL resolves it to a click on a button object.
2. The adapter converts the button's registered handler ID into `PlatformEvent::Activate` and enqueues it in Core's FIFO `EventQueue`.
3. Runtime dequeues one event and invokes the handler function with a fresh 10,000-instruction budget.
4. `StoreState` changes the slot only when the value differs and marks its subscribed bindings dirty.
5. After the handler returns, Runtime reevaluates dirty bindings in stable binding-ID order, refreshing their dependency sets.
6. Changed binding results become typed patches such as `RenderPatch::SetText { node_id, text }`.
7. `micro-lvgl` applies the patch to the already existing label object.

Events created while an event is executing are appended and handled later. Rendering and Runtime remain single-threaded in the MVP, avoiding LVGL cross-thread access and making ordering deterministic.

## 8. LVGL 9 and SDL3 native integration

The native CMake project uses pinned source archives or tags with integrity/pin information and builds static SDL3 and LVGL targets. Cargo invokes this build through the host crate's build script only when its `native` feature is enabled. `cargo test --workspace` uses the default feature set and therefore does not download or build native dependencies; `cargo build -p micro-host-sdl --features native` and the demo do.

The bridge owns:

- SDL3 initialization, window, renderer, streaming texture, event polling, and presentation;
- LVGL initialization, display buffers, display flush callback, pointer input callback, timer handling, and teardown;
- a map from stable UI node IDs to LVGL object pointers;
- narrow C-callable functions for creating a column, label, and button and setting label text;
- delivery of LVGL click handler IDs to the Rust host adapter without exposing LVGL pointers.

The LVGL display flush callback uploads the changed pixel region to the SDL3 texture and acknowledges the flush. The pointer callback reads the latest SDL3 coordinates and pressed/released state. A monotonic host clock drives LVGL ticks. All FFI calls stay on the main thread; no Rust panic or C++ exception may cross the C ABI.

The first window uses a fixed logical size and a minimal built-in theme. Resizing, keyboard input, accessibility integration, GPU-specific rendering, and multi-window support are outside this MVP.

## 9. Error handling

- Parser and subset failures are formatted as `path:line:column`, a stable error code, a short message, and an optional hint.
- MBC version, checksum, bounds, stack validation, and bad-reference failures reject the App before any state or native objects are created.
- Runtime errors identify the function/handler and event. They do not terminate the host loop unless initialization itself failed.
- Budget exhaustion aborts the current handler. State writes already performed by that handler remain committed; dirty bindings are flushed so UI and StateStore remain consistent. This non-transactional rule is explicit and testable.
- Renderer operations return `Result`. Initialization failure triggers orderly teardown; a later patch failure is logged with node ID and leaves Runtime able to process quit events.
- Native setup and dependency-build errors include the failed stage and the prerequisite hint for CMake or Xcode Command Line Tools.

## 10. Testing and acceptance

Tests follow the dependency boundaries:

- `micro-ir`: deterministic encode/decode round trips; corrupt magic, checksum, lengths, references, jumps, and versions are rejected.
- `micro-compiler`: accepted Counter source; one focused negative fixture per rejected construct; diagnostic snapshots include correct source locations; emitted UI/function/state references validate.
- `micro-vm`: scalar operations, branches, loops, local/state access, string templates, stack validation, and budget exhaustion.
- `micro-core`: FIFO event ordering, no-op state writes, dynamic dependency replacement, dirty coalescing, stable patch order, runtime-error recovery, and renderer-error propagation.
- Headless integration: compile Counter, load it into a recording renderer, activate the Add handler twice, and assert the initial text is `Count: 0`, patches are `Count: 1` then `Count: 2`, and the UI tree was created only once.
- Native integration: build the pinned C dependencies, create and destroy an SDL window/LVGL display under a smoke-test mode, and verify a synthetic activation reaches Runtime. A manual acceptance run confirms real pointer clicks.

MVP acceptance requires all of the following:

1. `npm install` installs only repository tooling metadata; it does not install a JS runtime for Apps.
2. `npm run build:app` produces a valid MBC from `apps/counter/app.ts` using the Rust compiler.
3. `cargo test --workspace` passes for the platform-neutral workspace tests.
4. `cargo build -p micro-host-sdl --features native` builds LVGL 9.5.0 and SDL 3.4.10 from pinned sources without Homebrew libraries.
5. `npm run demo` opens the Counter on macOS, and repeated clicks increment the displayed value through incremental text patches.
6. Unsupported TypeScript, corrupt MBC, and an over-budget handler fail in the documented, recoverable way.
7. README documents prerequisites, architecture, supported language subset, build/test/demo commands, first-build network behavior, and known limitations.

## 11. Prerequisites and reproducibility

The first native build requires macOS, Rust stable, Node.js/npm, CMake, Git or archive-download support, network access, and Xcode Command Line Tools. Later builds reuse Cargo and CMake caches. The README will state that `npm` is a convenient command front end and editor/declaration carrier; the App itself never executes in Node.js.

Dependency revisions and MBC version are committed. Generated build directories and fetched sources are ignored. The Counter source is committed; generated `app.mbc` may be regenerated deterministically and is not treated as source of truth.

## 12. Deferred work

ESP32-S3 hosting, a binary-size-optimized no-std VM, asynchronous effects, timers exposed to Apps, persistence, packages/runtime imports, full TypeScript checking, dynamic UI trees, additional widgets and styles, accessibility, hot reload, debugger/source maps, signing/sandbox permissions, and production distribution are intentionally deferred. The MVP interfaces must not prevent these additions, but no speculative implementation is included.
