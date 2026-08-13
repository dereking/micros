# Micro App Platform MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a macOS Counter App written in a restricted TypeScript subset, compiled by Rust/SWC to MBC, executed by a budgeted Rust runtime, and rendered through LVGL 9.5.0 and SDL 3.4.10.

**Architecture:** A Cargo workspace separates binary IR, VM, runtime Core, compiler, LVGL adapter, and SDL host. Platform-neutral tests use a recording renderer; a feature-gated CMake bridge fetches and links native libraries only for the real macOS demo.

**Tech Stack:** Rust stable, SWC Rust crates, CRC-32, TypeScript declaration files, Cargo, npm scripts, CMake FetchContent, LVGL 9.5.0, SDL 3.4.10, macOS/Xcode Command Line Tools.

---

## File map

- `Cargo.toml`: workspace members, common dependency versions, default members.
- `package.json`: `build:app`, `demo`, and `test` command front end.
- `sdk/index.d.ts`: supported App SDK declarations only.
- `apps/counter/app.ts`: Counter source.
- `crates/micro-ir/src/{lib,codec,model,opcode}.rs`: MBC model and validated binary codec.
- `crates/micro-vm/src/{lib,error,value,vm}.rs`: typed stack interpreter and budgets.
- `crates/micro-core/src/{lib,event,state,ui,runtime}.rs`: state, bindings, queue, UI tree, patches, orchestration.
- `crates/micro-compiler/src/{lib,diagnostic,parse,lower}.rs`: SWC front end and lowering.
- `crates/micro-compiler/src/bin/microc.rs`: compiler CLI.
- `crates/micro-lvgl/src/lib.rs`: `RenderPort` implementation over the bridge.
- `crates/micro-host-sdl/{Cargo.toml,build.rs,src/main.rs}`: feature-gated native build and host loop.
- `native/{CMakeLists.txt,lv_conf.h,include/micro_native.h,src/micro_native.c}`: LVGL/SDL3 bridge.
- `crates/micro-compiler/tests/counter_e2e.rs`: headless compile/load/click/update integration.
- `README.md`: prerequisites, architecture, commands, subset, failures, limitations.

## Task 1: Workspace and command surface

**Files:** Create `Cargo.toml`, `package.json`, `sdk/index.d.ts`, `apps/counter/app.ts`, and six crate manifests plus minimal `src/lib.rs`/`src/main.rs` roots.

- [ ] **Step 1: Add a failing metadata smoke check**

Create `tests/workspace_layout.sh` that checks every planned manifest, SDK declaration, and Counter source exists and checks `package.json` contains `build:app`, `demo`, and `test`.

- [ ] **Step 2: Run the check and see the expected failure**

Run: `zsh tests/workspace_layout.sh`
Expected: non-zero exit at the first missing `Cargo.toml` or `package.json`.

- [ ] **Step 3: Create the minimal workspace**

Use resolver 2 and edition 2024. Add members for all six crates. Keep every native dependency behind the `micro-host-sdl/native` feature. Define npm commands exactly as:

```json
{
  "private": true,
  "scripts": {
    "build:app": "cargo run -p micro-compiler --bin microc -- apps/counter/app.ts apps/counter/dist/app.mbc",
    "demo": "npm run build:app && cargo run -p micro-host-sdl --features native -- apps/counter/dist/app.mbc",
    "test": "cargo test --workspace"
  }
}
```

Declare the SDK surface:

```ts
type Scalar = number | string | boolean | null;
interface State<T extends Scalar> { value: T }
interface Binding<T extends Scalar> { readonly __binding: T }
interface UiNode { readonly __node: unique symbol }
declare function state<T extends Scalar>(initial: T): State<T>;
declare function bind<T extends Scalar>(read: () => T): Binding<T>;
declare const ui: {
  column(children: UiNode[]): UiNode;
  text(value: string | Binding<string>): UiNode;
  button(label: string, options: { onClick: () => void }): UiNode;
  mount(root: UiNode): void;
};
```

- [ ] **Step 4: Add the approved Counter source and pass the check**

Run: `zsh tests/workspace_layout.sh && cargo metadata --no-deps --format-version 1`
Expected: both exit 0 and metadata lists six workspace packages.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock package.json sdk apps crates tests/workspace_layout.sh
git commit -m "chore: scaffold micro app workspace"
```

## Task 2: Versioned MBC model and codec

**Files:** Create `crates/micro-ir/src/{model,opcode,codec}.rs`; modify `crates/micro-ir/src/lib.rs`; create `crates/micro-ir/tests/codec.rs`.

- [ ] **Step 1: Write failing round-trip and corruption tests**

Define a fixture with one number state, one `Text` node, and one binding function. Assert `decode(encode(app)) == app`. Add independent cases for bad `MBC1` magic, version 2, flipped payload/checksum byte, out-of-range constant/function/node references, invalid jump targets, and declared stack depth below the verified maximum.

```rust
#[test]
fn rejects_checksum_corruption() {
    let mut bytes = encode(&fixture()).unwrap();
    *bytes.last_mut().unwrap() ^= 0xff;
    assert!(matches!(decode(&bytes), Err(DecodeError::ChecksumMismatch)));
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p micro-ir`
Expected: compile failure because `encode`, `decode`, and MBC model types do not exist.

- [ ] **Step 3: Implement model and deterministic codec**

Define ID newtypes, `ScalarType`, `Constant`, `AppImage`, `StateDecl`, `Function { kind, locals, max_stack, code }`, `UiNodeSpec`, `UiKind`, and `Opcode`. Encode fixed-width integers little-endian, UTF-8 strings as `u32 length + bytes`, sections as `kind + u32 length + bytes`, and CRC-32 over the payload. Validate the whole image before returning it.

- [ ] **Step 4: Verify GREEN and malformed-input safety**

Run: `cargo test -p micro-ir`
Expected: all codec and validation tests pass; no panic on truncated inputs from lengths 0 through fixture length.

- [ ] **Step 5: Commit**

```bash
git add crates/micro-ir
git commit -m "feat: add validated MBC format"
```

## Task 3: Budgeted stack VM

**Files:** Create `crates/micro-vm/src/{value,error,vm}.rs`; modify `crates/micro-vm/src/lib.rs`; create `crates/micro-vm/tests/vm.rs`.

- [ ] **Step 1: Write failing behavior tests**

Test number arithmetic, comparison/jumps, local load/store, state load/store, scalar-to-string and concatenation, division-by-zero, type mismatch, stack underflow, and a backward jump with a budget of three.

```rust
#[test]
fn backward_jump_exhausts_budget() {
    let function = function(vec![Opcode::Jump(0)]);
    let error = Vm::new(&fixture(function), TestState::default())
        .invoke(FunctionId(0), 3).unwrap_err();
    assert_eq!(error, VmError::BudgetExceeded { function: FunctionId(0), executed: 3 });
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p micro-vm`
Expected: compile failure for missing `Vm` and `VmError`.

- [ ] **Step 3: Implement the interpreter**

Implement `Value::{Number,String,Bool,Null}`, a `StateAccess` trait, checked instruction-pointer movement, exact stack effects, and one budget decrement before every instruction. Return errors; never panic on App-controlled bytes.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p micro-vm`
Expected: all VM tests pass, including exactly three executed instructions for the loop.

- [ ] **Step 5: Commit**

```bash
git add crates/micro-vm
git commit -m "feat: execute MBC with instruction budgets"
```

## Task 4: EventQueue, StateStore, bindings, and UI patches

**Files:** Create `crates/micro-core/src/{event,state,ui,runtime}.rs`; modify `crates/micro-core/src/lib.rs`; create `crates/micro-core/tests/runtime.rs`.

- [ ] **Step 1: Write failing Core tests**

Test FIFO handler ordering, unchanged state writes producing no dirtiness, two writes coalescing to one patch, dynamic binding dependency replacement, stable binding-ID patch order, partial writes surviving budget exhaustion, and renderer errors being returned without panic.

```rust
#[derive(Default)]
struct RecordingRenderer { created: usize, patches: Vec<RenderPatch> }

#[test]
fn two_writes_emit_one_final_text_patch() {
    let mut runtime = counter_runtime(RecordingRenderer::default());
    runtime.enqueue(Event::Activate(HandlerId(0)));
    runtime.tick().unwrap();
    assert_eq!(runtime.renderer().patches,
        [RenderPatch::SetText { node: NodeId(1), text: "Count: 2".into() }]);
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p micro-core`
Expected: compile failure for missing runtime types.

- [ ] **Step 3: Implement Core**

Use `VecDeque<Event>` for FIFO, indexed `Vec<Value>` state slots, `BTreeSet<BindingId>` for deterministic dirty order, and two-way dependency sets. Define `RenderPort::create_tree(&MicroUiTree)` and `RenderPort::apply(&[RenderPatch])`. Flush dirty bindings after both successful and failed handlers.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p micro-core`
Expected: all Core tests pass and the recording renderer reports one initial tree creation.

- [ ] **Step 5: Commit**

```bash
git add crates/micro-core
git commit -m "feat: add event driven reactive runtime"
```

## Task 5: SWC parser, subset validator, and diagnostics

**Files:** Create `crates/micro-compiler/src/{diagnostic,parse}.rs`; modify `crates/micro-compiler/src/lib.rs`; create `crates/micro-compiler/tests/diagnostics.rs` and `crates/micro-compiler/tests/fixtures/rejected/*.ts`.

- [ ] **Step 1: Write failing parser/diagnostic tests**

Accept the Counter syntax. Add focused fixtures for JSX, class, async, general function, runtime import, spread, destructuring, dynamic property, unknown UI prop, multiple mounts, and unsupported global. Assert stable error codes and exact line/column, for example `MTS001 unsupported syntax: class declaration`.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p micro-compiler diagnostics`
Expected: compile failure for missing `compile_source` and diagnostic types.

- [ ] **Step 3: Implement parsing and allow-list validation**

Parse with `Syntax::Typescript(TsSyntax { tsx: false, ..Default::default() })`. Walk every statement and expression; accept only the constructs in the design spec. Convert SWC spans through `SourceMap` into `Diagnostic { code, path, line, column, message, hint }`.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p micro-compiler diagnostics`
Expected: Counter parses and every rejection fixture matches its diagnostic snapshot.

- [ ] **Step 5: Commit**

```bash
git add crates/micro-compiler
git commit -m "feat: validate the micro TypeScript subset"
```

## Task 6: Compiler lowering and CLI

**Files:** Create `crates/micro-compiler/src/lower.rs`, `crates/micro-compiler/src/bin/microc.rs`, and `crates/micro-compiler/tests/lowering.rs`; modify `crates/micro-compiler/src/lib.rs`.

- [ ] **Step 1: Write failing lowering tests**

Assert the Counter produces one numeric state, a `Column(Text, Button)` tree, one text binding, one click handler, and handler bytecode equivalent to `LoadState 0; PushConst 1; Add; StoreState 0; Return`. Test `if`, `while`, scalar locals, and template lowering.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p micro-compiler lowering`
Expected: failure because the validated AST is not lowered.

- [ ] **Step 3: Implement symbol/type analysis and lowering**

Use distinct tables for scalar locals, state handles, binding functions, handlers, and UI nodes. Infer only supported scalar types, reject incompatible assignments/operators, intern constants, calculate maximum stack depth, and finish by calling `micro_ir::validate` and `encode`.

- [ ] **Step 4: Implement and exercise the CLI**

The CLI accepts exactly input and output paths, creates the output parent, writes only after successful compilation, prints source diagnostics to stderr, and exits 2 for source errors or 1 for I/O/internal errors.

Run: `cargo run -p micro-compiler --bin microc -- apps/counter/app.ts /tmp/micro-counter.mbc`
Expected: exit 0 and a non-empty file beginning with `MBC1`.

- [ ] **Step 5: Commit**

```bash
git add crates/micro-compiler
git commit -m "feat: compile micro TypeScript to MBC"
```

## Task 7: Headless Counter end-to-end test

**Files:** Create `crates/micro-compiler/tests/counter_e2e.rs`; add `micro-core` as a `dev-dependency` of `micro-compiler`.

- [ ] **Step 1: Write the failing real-source integration test**

Read `apps/counter/app.ts`, compile it through `micro_compiler`, decode it, start `Runtime<RecordingRenderer>`, activate the button handler twice, and assert initial `Count: 0`, patches `Count: 1` and `Count: 2`, and `create_tree` called once.

- [ ] **Step 2: Verify RED for an integration mismatch**

Temporarily expect `Count: 99`; run `cargo test -p micro-compiler --test counter_e2e`; confirm the assertion fails with actual `Count: 2`, then restore the correct expectation.

- [ ] **Step 3: Verify GREEN**

Run: `cargo test -p micro-compiler --test counter_e2e && npm run build:app`
Expected: test passes and `apps/counter/dist/app.mbc` is created.

- [ ] **Step 4: Commit**

```bash
git add crates/micro-compiler/Cargo.toml crates/micro-compiler/tests/counter_e2e.rs Cargo.lock apps/counter/app.ts
git commit -m "test: prove the counter pipeline headlessly"
```

## Task 8: Feature-gated LVGL 9.5.0 / SDL 3.4.10 bridge

**Files:** Create `native/CMakeLists.txt`, `native/lv_conf.h`, `native/include/micro_native.h`, `native/src/micro_native.c`, `crates/micro-host-sdl/build.rs`, and native bridge smoke tests.

- [ ] **Step 1: Write a failing native build/smoke target**

Expose opaque `micro_native_t` and functions for create/destroy, poll, tick, create column/label/button, set label text, and retrieve activated handler IDs. Add a Rust test gated by `native` that creates the bridge in SDL's dummy/offscreen mode and destroys it.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p micro-host-sdl --features native native_create_destroy`
Expected: link/build failure because the bridge and CMake project are absent.

- [ ] **Step 3: Implement pinned FetchContent and bridge**

In `build.rs`, return immediately unless `CARGO_FEATURE_NATIVE` is set. Otherwise use LVGL tag `v9.5.0` and SDL tag `release-3.4.10`, disable examples/tests, build static targets, and configure LVGL for 32-bit color with custom display/input callbacks. The flush callback updates only the dirty rectangle of an SDL3 streaming texture. Store `(node_id, lv_obj_t*)` only inside C. Queue integer handler IDs from LVGL click callbacks.

- [ ] **Step 4: Verify GREEN**

Run: `cargo test -p micro-host-sdl --features native native_create_destroy`
Expected: first run downloads/builds pinned sources and the offscreen create/destroy test passes.

- [ ] **Step 5: Commit**

```bash
git add native crates/micro-host-sdl/build.rs crates/micro-host-sdl/Cargo.toml Cargo.lock
git commit -m "feat: add vendored LVGL SDL3 bridge"
```

## Task 9: LVGL renderer and macOS host loop

**Files:** Implement `crates/micro-lvgl/src/lib.rs` and `crates/micro-host-sdl/src/main.rs`; create `crates/micro-lvgl/tests/renderer.rs`.

- [ ] **Step 1: Write failing renderer mapping tests**

Use a fake bridge trait to assert preorder tree creation, stable node/handler IDs, and `RenderPatch::SetText` calling only `set_label_text` for the matching node.

- [ ] **Step 2: Verify RED**

Run: `cargo test -p micro-lvgl`
Expected: failure because `LvglRenderer` is missing.

- [ ] **Step 3: Implement renderer and host**

Keep unsafe FFI in one private bridge module. `main` reads and decodes the supplied MBC, creates Runtime and native renderer, then loops: poll SDL/quit, drain activated IDs into Core events, run Core until its queue is empty, call LVGL timer handling, and sleep for the returned bounded delay. Print structured errors and always destroy native state.

- [ ] **Step 4: Verify automated and manual paths**

Run: `cargo test -p micro-lvgl`
Expected: mapping tests pass.

Run: `npm run demo`
Expected: a macOS window shows `Count: 0`; each real click on `Add` increments once; closing the window exits 0.

- [ ] **Step 5: Commit**

```bash
git add crates/micro-lvgl crates/micro-host-sdl
git commit -m "feat: render and run micro apps on macOS"
```

## Task 10: Documentation and final verification

**Files:** Create `README.md`; modify `.gitignore` only if verification reveals generated paths not covered.

- [ ] **Step 1: Write README acceptance checklist first**

Include prerequisites, Quick Start, first-build network warning, architecture/crate table, SDK Counter example, exact supported/rejected syntax, MBC/runtime flow, instruction-budget behavior including partial state commits, test commands, native versions, troubleshooting, and deferred work.

- [ ] **Step 2: Run formatting, lint, tests, compiler, and native build fresh**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run build:app
cargo build -p micro-host-sdl --features native
```

Expected: every command exits 0; no warnings; generated MBC begins with `MBC1`.

- [ ] **Step 3: Perform final manual acceptance**

Run `npm run demo`, click Add at least three times, confirm `Count: 3`, close the window, and confirm the process exits 0.

- [ ] **Step 4: Check requirement coverage and repository cleanliness**

Run: `git status --short && git log --oneline --decorate -12`
Expected: only intended README/ignore changes before final commit; history contains each component commit.

- [ ] **Step 5: Commit**

```bash
git add README.md .gitignore Cargo.lock
git commit -m "docs: document the runnable MVP"
```
