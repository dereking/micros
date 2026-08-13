# Shared Web Runtime Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run the existing Counter MBC in a browser with the same Rust IR, VM, Runtime, EventQueue, StateStore, and instruction-budget semantics as the native host.

**Architecture:** Add a platform-neutral Web renderer behind a narrow DOM port, then add a `wasm-bindgen` host that owns the real browser DOM bridge and a FIFO activation queue. A minimal Web Player drives Runtime ticks; Playwright verifies the real browser path from DOM click to `Count: 2`.

**Tech Stack:** Rust 2024, `wasm32-unknown-unknown`, `wasm-bindgen`, `web-sys`, `wasm-pack`, browser ES modules, Vite, Playwright

---

## File map

- `crates/micro-renderer-web/`: maps `MicroUiTree` and `RenderPatch` to a testable `WebDom` port.
- `crates/micro-host-web/`: WebAssembly boundary, browser DOM implementation, click activation queue, and Runtime pump.
- `products/micro-web-player/`: minimal end-user page consuming the generated Wasm package.
- `tests/web/`: real-browser Counter acceptance test.
- `playwright.config.js`: builds/serves the Player for browser tests.
- Root `package.json`: direct build, development, preview, and test commands.

Generated paths remain untracked:

```text
products/micro-web-player/src/generated/
products/micro-web-player/public/apps/*.mbc
products/micro-web-player/dist/
```

### Task 1: Add a testable Web renderer

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/micro-renderer-web/Cargo.toml`
- Create: `crates/micro-renderer-web/src/lib.rs`
- Create: `crates/micro-renderer-web/tests/renderer.rs`

- [ ] **Step 1: Register the crate and write the failing renderer test**

Add `crates/micro-renderer-web` to the workspace. Create its manifest with `micro-core` and `micro-ir` path dependencies. Write a fake DOM test that constructs this tree:

```rust
let tree = MicroUiTree {
    root: NodeId(0),
    nodes: vec![
        MicroUiNode {
            id: NodeId(0),
            kind: UiKind::Column,
            children: vec![NodeId(1), NodeId(2)],
            text: String::new(),
            on_click: None,
        },
        MicroUiNode {
            id: NodeId(1),
            kind: UiKind::Text,
            children: vec![],
            text: "Count: 0".into(),
            on_click: None,
        },
        MicroUiNode {
            id: NodeId(2),
            kind: UiKind::Button,
            children: vec![],
            text: "Add".into(),
            on_click: Some(FunctionId(7)),
        },
    ],
};
```

The fake records `column`, `text`, `button`, and `set_text` operations. Assert creation order is preorder and applying `RenderPatch::SetText { node: NodeId(1), text: "Count: 1" }` produces exactly one `set_text` operation.

- [ ] **Step 2: Run the focused test and verify red**

Run:

```bash
cargo test -p micro-renderer-web --test renderer
```

Expected: compilation fails because `WebDom` and `WebRenderer` do not exist.

- [ ] **Step 3: Implement the minimal renderer port**

Define:

```rust
pub trait WebDom {
    fn create_column(&mut self, node: NodeId, parent: Option<NodeId>) -> Result<(), String>;
    fn create_text(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
    ) -> Result<(), String>;
    fn create_button(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
    ) -> Result<(), String>;
    fn set_text(&mut self, node: NodeId, text: &str) -> Result<(), String>;
}

pub struct WebRenderer<D> {
    dom: D,
}
```

Implement recursive preorder creation for Column, Text, and Button. Reject a missing node and a Button without a handler as `RenderError`. Implement `RenderPort::apply` by forwarding only `SetText` patches. Expose `dom()` and `dom_mut()` accessors for host integration and tests.

- [ ] **Step 4: Verify green and commit**

Run:

```bash
cargo fmt --all
cargo test -p micro-renderer-web --test renderer
```

Expected: one renderer test passes.

Commit:

```bash
git add Cargo.toml crates/micro-renderer-web
git commit -m "feat: add portable Web renderer"
```

### Task 2: Add the WebAssembly host and activation queue

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/micro-host-web/Cargo.toml`
- Create: `crates/micro-host-web/src/activation.rs`
- Create: `crates/micro-host-web/src/dom.rs`
- Create: `crates/micro-host-web/src/lib.rs`
- Create: `crates/micro-host-web/tests/activation.rs`

- [ ] **Step 1: Write the failing FIFO activation test**

The test uses the desired API:

```rust
use micro_host_web::ActivationQueue;
use micro_ir::FunctionId;

#[test]
fn activations_are_shared_and_fifo() {
    let producer = ActivationQueue::default();
    let mut consumer = producer.clone();
    producer.push(FunctionId(4));
    producer.push(FunctionId(9));
    assert_eq!(consumer.pop(), Some(FunctionId(4)));
    assert_eq!(consumer.pop(), Some(FunctionId(9)));
    assert_eq!(consumer.pop(), None);
}
```

- [ ] **Step 2: Run the focused test and verify red**

Run:

```bash
cargo test -p micro-host-web --test activation
```

Expected: compilation fails because the crate and queue do not exist.

- [ ] **Step 3: Implement the queue and Wasm crate boundary**

Implement the queue as a cloneable single-thread structure shared by DOM callbacks and the Runtime owner:

```rust
#[derive(Clone, Default)]
pub struct ActivationQueue(Rc<RefCell<VecDeque<FunctionId>>>);

impl ActivationQueue {
    pub fn push(&self, handler: FunctionId) {
        self.0.borrow_mut().push_back(handler);
    }

    pub fn pop(&mut self) -> Option<FunctionId> {
        self.0.borrow_mut().pop_front()
    }
}
```

Create `micro-host-web` as `crate-type = ["cdylib", "rlib"]`. Use normal path dependencies for `micro-core`, `micro-ir`, `micro-renderer-web`, and `micro-vm`. Put `wasm-bindgen = "0.2"` and `web-sys = "0.3"` behind `cfg(target_arch = "wasm32")`, enabling `Window`, `Document`, `Element`, `Node`, `Event`, and `EventTarget`.

- [ ] **Step 4: Implement the real DOM bridge only for Wasm**

`DomBridge` owns the container `Element`, a `BTreeMap<u32, Element>`, an `ActivationQueue`, and retained `Closure<dyn FnMut(Event)>` values. It implements `WebDom` as follows:

- Column: create a `div.micro-column`.
- Text: create a `span.micro-text` and set `text_content`.
- Button: create a `button.micro-button`, set `type="button"`, and register a click closure that pushes the supplied `FunctionId`.
- Every element receives `data-micro-node="<id>"` and is appended to its declared parent or the root container.
- `set_text` finds the recorded element and updates only `text_content`.
- `clear` removes all container children, clears the element map, and drops retained event closures.

- [ ] **Step 5: Expose `MicroWebRuntime` to JavaScript**

For `wasm32`, export:

```rust
#[wasm_bindgen]
pub struct MicroWebRuntime {
    runtime: Runtime<WebRenderer<DomBridge>>,
    activations: ActivationQueue,
}
```

The constructor accepts `(container_id: &str, mbc: &[u8], event_budget: u64)`, finds the container, decodes and validates MBC through existing APIs, creates the DOM bridge and Runtime, and returns JavaScript errors with stable prefixes such as `WEB_CONTAINER`, `WEB_MBC`, and `WEB_RUNTIME`.

Expose:

```rust
pub fn tick(&mut self) -> Result<u32, JsValue>;
pub fn dispose(&mut self);
```

`tick` drains activation IDs into `Event::Activate`, then calls `Runtime::tick` until no event remains and returns the number processed. `dispose` clears the DOM bridge. No browser timer is created inside Rust in this phase.

- [ ] **Step 6: Verify native tests and Wasm compilation**

Run:

```bash
cargo test -p micro-host-web --test activation
rustup target add wasm32-unknown-unknown
cargo check -p micro-host-web --target wasm32-unknown-unknown
```

Expected: the FIFO test passes and the Wasm target compiles without warnings or host-only symbols.

Commit:

```bash
git add Cargo.toml Cargo.lock crates/micro-host-web
git commit -m "feat: expose Micro Runtime to WebAssembly"
```

### Task 3: Build the minimal Web Player and real-browser test

**Files:**
- Modify: `.gitignore`
- Modify: `package.json`
- Modify: `package-lock.json`
- Create: `products/micro-web-player/index.html`
- Create: `products/micro-web-player/src/main.js`
- Create: `products/micro-web-player/src/style.css`
- Create: `playwright.config.js`
- Create: `tests/web/counter.spec.js`

- [ ] **Step 1: Add Web tooling and ignored generated paths**

Run:

```bash
npm install --save-dev vite @playwright/test
```

Add these ignore rules:

```gitignore
products/micro-web-player/src/generated/
products/micro-web-player/public/apps/
products/micro-web-player/dist/
test-results/
playwright-report/
```

Install local prerequisites once:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
npx playwright install chromium
```

- [ ] **Step 2: Add the browser acceptance test before the Player**

Configure Playwright with `testDir: "tests/web"`, Chromium, base URL `http://127.0.0.1:4173`, and a web server command of `npm run preview:web` on port `4173`.

Write:

```javascript
import { expect, test } from "@playwright/test";

test("Counter executes the real MBC in WebAssembly", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Count: 0")).toBeVisible();
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("Count: 1")).toBeVisible();
  await page.getByRole("button", { name: "Add" }).click();
  await expect(page.getByText("Count: 2")).toBeVisible();
  await expect(page.locator("[data-runtime-error]")).toHaveCount(0);
});
```

- [ ] **Step 3: Run the acceptance test and verify red**

Run:

```bash
npx playwright test tests/web/counter.spec.js
```

Expected: FAIL because the Web Player page and build output do not exist.

- [ ] **Step 4: Add direct root scripts**

Add these scripts to root `package.json`:

```json
{
  "build:web:app": "cargo run -p micro-compiler --bin microc -- apps/counter/app.ts products/micro-web-player/public/apps/counter.mbc",
  "build:web:wasm": "wasm-pack build crates/micro-host-web --target web --out-dir ../../products/micro-web-player/src/generated --out-name micro_web",
  "build:web": "npm run build:web:app && npm run build:web:wasm && vite build products/micro-web-player",
  "dev:web": "npm run build:web:app && npm run build:web:wasm && vite products/micro-web-player --host 127.0.0.1",
  "preview:web": "vite preview products/micro-web-player --host 127.0.0.1 --port 4173",
  "test:web": "npm run build:web && playwright test"
}
```

- [ ] **Step 5: Implement the Player page**

`index.html` contains a status heading, a `#micro-device` viewport, and a hidden `[data-runtime-error]` element. `main.js` performs:

```javascript
import init, { MicroWebRuntime } from "./generated/micro_web.js";
import "./style.css";

async function start() {
  await init();
  const response = await fetch("/apps/counter.mbc");
  if (!response.ok) throw new Error(`MBC download failed: ${response.status}`);
  const bytes = new Uint8Array(await response.arrayBuffer());
  const runtime = new MicroWebRuntime("micro-device", bytes, 10_000n);

  function frame() {
    runtime.tick();
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

start().catch((error) => {
  const output = document.querySelector("[data-runtime-error]");
  output.hidden = false;
  output.textContent = String(error);
});
```

CSS presents a centered 480×320 device viewport, Column flex layout, readable Counter text, and a clear Add button. Use plain CSS and no frontend framework.

- [ ] **Step 6: Build and verify the real browser path**

Run:

```bash
npm run test:web
```

Expected: Chromium loads the generated Wasm and MBC, and the Counter acceptance test passes through `Count: 2`.

Commit:

```bash
git add .gitignore package.json package-lock.json products/micro-web-player playwright.config.js tests/web
git commit -m "feat: run Counter in the Web Player"
```

### Task 4: Document and verify the shared foundation

**Files:**
- Modify: `README.md`
- Modify: `tests/workspace_layout.sh`

- [ ] **Step 1: Expand the workspace layout test**

Require these source paths:

```text
crates/micro-renderer-web/Cargo.toml
crates/micro-host-web/Cargo.toml
products/micro-web-player/index.html
products/micro-web-player/src/main.js
tests/web/counter.spec.js
```

Also assert generated Wasm, MBC, `dist`, Playwright reports, and `node_modules` are ignored and not tracked.

- [ ] **Step 2: Document prerequisites and commands**

Add a Web Player section to `README.md` covering:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
npm install
npx playwright install chromium
npm run dev:web
npm run test:web
```

Explain that TypeScript is compiled on the development host, Rust IR/VM/Core run inside WebAssembly, DOM click handlers enqueue the same `Event::Activate`, and generated `.wasm`/`.mbc`/`dist` artifacts remain untracked.

- [ ] **Step 3: Run all verification gates**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo check -p micro-host-web --target wasm32-unknown-unknown
cargo test -p micro-host-sdl --features native
npm run test:web
git diff --check
git status --short --ignored
```

Expected: all Rust/native/browser tests pass; the Wasm target compiles; only intended source files are tracked; generated Web files appear as ignored.

- [ ] **Step 4: Commit the documentation and verification updates**

```bash
git add README.md tests/workspace_layout.sh docs/superpowers/plans/2026-08-13-shared-web-runtime-foundation.md
git commit -m "docs: document the shared Web Runtime"
```

## Completion criteria

- The existing Counter TypeScript compiles to one MBC used by native and browser hosts.
- The browser executes the existing Rust decoder, VM, Runtime, EventQueue, StateStore, bindings, and instruction budget in WebAssembly.
- A DOM click queues a handler ID and updates only the existing bound text element.
- `npm run dev:web` opens a working browser Player.
- `npm run test:web` proves `Count: 0 -> 1 -> 2` in Chromium.
- No generated Wasm, MBC, Vite output, Playwright report, dependency directory, or browser cache enters Git.
