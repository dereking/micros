# Unified Typography Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add deterministic logical-pixel text styles and one embedded common Chinese/English font family across ESP32 LVGL, macOS LVGL, and the fixed 800×480 Web simulator, while repairing the simulator's app layering.

**Architecture:** `TextStyle` is immutable UI IR metadata and travels in MBC with each Text/Button node. Compiler lowering accepts restricted literal style objects; Runtime text patches preserve node style. Renderers receive the style during node creation and select assets generated from the same pinned Noto Sans SC source. The ESP target is the capacity authority: generated assets must fit a stated app/font budget before they are accepted.

**Tech Stack:** Rust, SWC restricted TypeScript compiler, MBC codec, LVGL 9, DOM/WebAssembly, CSS, Noto Sans SC (SIL OFL), `lv_font_conv`, Vite, ESP-IDF.

---

### Task 1: Repair the Web device canvas before adding typography

**Files:**
- Modify: `products/micro-web-player/src/style.css`
- Modify: `tests/web/counter.spec.js`

- [ ] **Step 1: Write failing browser checks for layer exclusivity and fixed canvas typography**

Append Playwright assertions that launch Counter and require:

```js
await expect(page.locator("#system-screen")).toBeHidden();
await expect(page.locator("#app-shell")).toBeVisible();
await expect(page.locator(".micro-text")).toHaveCSS("font-size", "24px");
```

Also assert the launcher `Choose runtime` title has computed `font-size: 18px` at the 800×480 device viewport, and compare `#app-shell`/`[data-device-screen]` bounding rectangles so the app shell fills the device content box.

- [ ] **Step 2: Run the new browser check and capture RED**

Run `npx playwright test tests/web/counter.spec.js`.

Expected: failure because the author `.system-screen { display: grid }` overrides the browser hidden state and the current `.micro-text` and launcher title use viewport-relative `clamp(...vw...)` sizes.

- [ ] **Step 3: Make the device layers exclusive and use fixed logical CSS sizes**

Add a device-scoped hidden rule and explicit canvas fill:

```css
.device-screen > [hidden] { display: none !important; }
.app-shell, .app-screen { width: 100%; height: 100%; min-height: 0; }
.launcher-copy h2 { font-size: 18px; line-height: 24px; }
.micro-text { font-size: 24px; line-height: 32px; }
.micro-button { font-size: 14px; line-height: 18px; }
```

Remove only device-internal `vw`/`rem` typography rules. Keep outer simulator presentation responsive; it is not device UI.

- [ ] **Step 4: Verify green and commit**

Run `npx playwright test tests/web/counter.spec.js`, `npm run test:web:unit`, `npm run build:web`, and `git diff --check`.

Expected: all browser tests pass; Counter fills the device content rectangle; system layer is hidden while Counter runs.

Commit only the CSS and Playwright changes with `fix: constrain Web simulator to device canvas`.

### Task 2: Add versioned immutable text styles to UI IR/MBC

**Files:**
- Modify: `crates/micro-ir/src/model.rs`
- Modify: `crates/micro-ir/src/codec.rs`
- Modify: `crates/micro-ir/tests/codec.rs`
- Modify: `crates/micro-core/src/ui.rs`
- Modify: `crates/micro-core/tests/runtime.rs`

- [ ] **Step 1: Write MBC/core failures for a styled Text node**

Create a `TextStyle` test value and require encoded/decoded nodes and runtime tree nodes to preserve it:

```rust
let style = TextStyle::ui_sans(18, FontWeight::Regular, 24).unwrap();
assert_eq!(decode(&encode(&image).unwrap()).unwrap().nodes[0].text_style, Some(style));
```

Add malformed-byte cases for unsupported style tag, unsupported size, and line height smaller than size.

- [ ] **Step 2: Run focused tests for RED**

Run `cargo test -p micro-ir --test codec` and `cargo test -p micro-core --test runtime`.

Expected: compile failures because `TextStyle`, `FontWeight`, and `text_style` do not exist.

- [ ] **Step 3: Implement a closed style model and MBC v2 encoding**

In `model.rs`, define:

```rust
pub enum FontFamily { UiSans }
pub enum FontWeight { Regular }
pub struct TextStyle { pub family: FontFamily, pub size_px: u8, pub weight: FontWeight, pub line_height_px: u8 }
```

`TextStyle::new` accepts only size 12, 14, 18, 24, or 32 and requires `line_height_px >= size_px`. Add `pub text_style: Option<TextStyle>` to `UiNodeSpec` and `MicroUiNode`. Encode the optional style after each node's existing click metadata. Bump `VERSION` from 1 to 2; v1 input must deterministically return `DecodeError::UnsupportedVersion(1)` rather than being misdecoded.

- [ ] **Step 4: Verify green and commit**

Run `cargo test -p micro-ir --test codec`, `cargo test -p micro-core --test runtime`, `cargo test --workspace`, `cargo fmt --all -- --check`, and `cargo clippy --workspace --all-targets -- -D warnings`.

Commit with `feat: encode immutable UI text styles`.

### Task 3: Lower restricted TypeScript style objects and validate static glyphs

**Files:**
- Modify: `crates/micro-compiler/src/lower.rs`
- Modify: `crates/micro-compiler/src/lib.rs`
- Modify: `crates/micro-compiler/tests/counter_e2e.rs`
- Create: `crates/micro-compiler/tests/typography.rs`
- Create: `assets/fonts/ui-sans-common.txt`

- [ ] **Step 1: Add failing compiler fixtures**

Create compiler tests for valid calls:

```ts
ui.text("欢迎", { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 });
ui.button("确认", { onClick: () => {}, textStyle: { font: "uiSans", size: 14, weight: "regular", lineHeight: 18 } });
```

Require the lowered node's `text_style`. Add rejection fixtures for unknown font, non-literal style values, size `17`, `lineHeight: 14` with `size: 18`, and literal Chinese text absent from `ui-sans-common.txt` with a stable `MTS` diagnostic code.

- [ ] **Step 2: Verify RED**

Run `cargo test -p micro-compiler --test typography`.

Expected: valid calls are rejected because current known option schemas support only `onClick` and text takes one argument.

- [ ] **Step 3: Implement style-object parsing and glyph manifest validation**

Extend the existing literal object-property helpers. `ui.text(value, style?)` accepts exactly `font`, `size`, `weight`, and `lineHeight`; `ui.button(label, options)` accepts `onClick` and optional `textStyle` with the same exact fields. Convert literal values through `TextStyle::new`. Load `assets/fonts/ui-sans-common.txt` at compiler build time with `include_str!`; accept ASCII plus every newline-delimited listed character. Validate static text/button literals. Dynamic `bind` output remains valid and is rendered with replacement glyph + host diagnostic if a character is not present.

`ui-sans-common.txt` contains ASCII printable characters, Chinese punctuation, and exactly the GB2312 level-1 Chinese range (3,755 characters) emitted in Unicode order by the font-generation script in Task 5. The script and test compare the generated list byte-for-byte with the checked-in manifest.

- [ ] **Step 4: Verify green and commit**

Run `cargo test -p micro-compiler --test typography`, `cargo test -p micro-compiler --test counter_e2e`, and `cargo test --workspace`.

Commit with `feat: lower typed typography styles`.

### Task 4: Propagate styles through DOM and LVGL renderer boundaries

**Files:**
- Modify: `crates/micro-renderer-web/src/lib.rs`
- Modify: `crates/micro-renderer-web/tests/renderer.rs`
- Modify: `crates/micro-host-web/src/lib.rs`
- Modify: `crates/micro-lvgl/src/lib.rs`
- Modify: `crates/micro-lvgl/tests/renderer.rs`
- Modify: `crates/micro-host-sdl/src/native.rs`

- [ ] **Step 1: Write renderer fake-port failures**

Update both renderer fake ports so a styled label/button creation records the `TextStyle`:

```rust
Call::Text(NodeId(1), "欢迎".into(), Some(TextStyle::ui_sans(18, FontWeight::Regular, 24).unwrap()))
```

Assert subsequent `RenderPatch::SetText` changes only text and emits no second style application.

- [ ] **Step 2: Verify RED**

Run `cargo test -p micro-renderer-web --test renderer` and `cargo test -p micro-lvgl --test renderer`.

Expected: trait signatures cannot accept the style argument.

- [ ] **Step 3: Extend renderer ports and concrete hosts**

Add `style: Option<&TextStyle>` to `create_text`/`create_label` and `create_button` in the Web and LVGL narrow traits. During tree creation pass `node.text_style.as_ref()`; do not add a text-style render patch. In the browser host set `font-family: "MicroUiSans"`, `font-size: "{size_px}px"`, `font-weight`, and `line-height: "{line_height_px}px"` on created DOM nodes. In the LVGL native bridge select the corresponding generated `lv_font_t` and apply it with `lv_obj_set_style_text_font`; apply text line spacing from the same style.

- [ ] **Step 4: Verify green and commit**

Run renderer package tests, `cargo test --workspace`, `cargo fmt --all -- --check`, and `cargo clippy --workspace --all-targets -- -D warnings`.

Commit with `feat: render shared text styles on Web and LVGL`.

### Task 5: Generate, package, and budget the common CJK font assets

**Files:**
- Create: `assets/fonts/OFL-1.1.txt`
- Create: `assets/fonts/noto-sans-sc.json`
- Create: `assets/fonts/lv-font-conv-lock.json`
- Create: `scripts/generate-font-assets.py`
- Create: `products/micro-web-player/public/fonts/micro-ui-sans-common.woff2`
- Create: `assets/fonts/lvgl/micro_ui_sans_12.c`
- Create: `assets/fonts/lvgl/micro_ui_sans_14.c`
- Create: `assets/fonts/lvgl/micro_ui_sans_18.c`
- Create: `assets/fonts/lvgl/micro_ui_sans_24.c`
- Create: `assets/fonts/lvgl/micro_ui_sans_32.c`
- Modify: `firmware/micro-os-esp32/components/micro_bsp_lcd7/CMakeLists.txt`
- Modify: `firmware/micro-os-esp32/components/micro_bsp_lcd7/include/micro_bsp_lcd7.h`
- Modify: `firmware/micro-os-esp32/partitions_8m.csv`
- Modify: `firmware/micro-os-esp32/main/CMakeLists.txt`
- Modify: `products/micro-web-player/src/style.css`
- Create: `tests/esp32_font_budget.sh`

- [ ] **Step 1: Write the asset and budget contract test**

`tests/esp32_font_budget.sh` must verify all five generated LVGL source files and the WOFF2 file exist, their declared font sizes are exactly 12/14/18/24/32, `ui-sans-common.txt` and the generator output match, the total generated LVGL font payload is at most `0x240000` bytes, and the compiled ESP app (which already contains those C font objects) fits the exact `factory` partition length from `partitions_8m.csv` with at least `0x40000` bytes free.

- [ ] **Step 2: Verify RED**

Run `zsh tests/esp32_font_budget.sh`.

Expected: fail because no generated font sources, WOFF2 resource, or font budget contract exists.

- [ ] **Step 3: Add reproducible same-source generation and CMake packaging**

Pin the Noto Sans CJK SC source file SHA-256, OFL text, FontTools/Brotli versions, and `lv_font_conv` dependency lock in `scripts/generate-font-assets.py` and `assets/fonts/noto-sans-sc.json`. The script derives the exact printable-ASCII, Chinese-punctuation, U+FFFD, and 3,755 GB2312 level-1 Han manifest in deterministic order. Because the pinned upstream font does not encode U+FFFD, generation deterministically aliases U+FFFD to its U+25A1 square outline before subsetting. It invokes pinned `lv_font_conv` for each size at the user-approved 2bpp, produces the WOFF2 from the same derived source/glyph list, and fails when any generated file differs from its tracked output. The full 4bpp set measured `0x2b1db0`, exceeding the `0x240000` budget by `0x71db0`; the same five sizes and glyphs at 2bpp measure `0x1946dc`. CMake compiles the LVGL C fonts into `micro_bsp_lcd7`; the Web stylesheet declares:

```css
@font-face { font-family: "MicroUiSans"; src: url("/fonts/micro-ui-sans-common.woff2") format("woff2"); font-display: block; }
```

If the exact payload crosses the Task 5 budget, the test stays red and the implementation stops for a user decision; no silent subset reduction or partition expansion is permitted.

- [ ] **Step 4: Verify target artifacts and commit**

Run `zsh tests/esp32_font_budget.sh`, `npm run test:web`, `zsh tests/esp32_layout.sh`, and the project-local ESP-IDF build documented in `firmware/micro-os-esp32/TOOLCHAIN.md`.

Expected: generated resources are deterministic, app/font capacity test passes, Web loads `MicroUiSans`, and ESP-IDF links LVGL font assets.

Commit with `feat: embed unified common CJK typography assets`.
