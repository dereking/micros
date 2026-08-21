# Unified typography design

## Goal

Give Micro Apps one deterministic typography model that starts within the ESP32-S3's 8 MiB resource limits and produces matching text metrics, line breaks, and layout on ESP32 LVGL, macOS LVGL, and the Web simulator.

## Logical typography contract

The UI SDK gains an optional text-style argument for `ui.text` and the existing `ui.button` options object:

```ts
ui.text("欢迎", { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 });
ui.button("确认", { onClick: () => {}, textStyle: { font: "uiSans", size: 18, weight: "regular", lineHeight: 24 } });
```

`font` is initially the single `uiSans` family and `regular` is the only public weight. `size` and `lineHeight` are required integer logical pixels when a style is supplied; supported sizes are 12, 14, 18, 24, and 32. The compiler lowers the style into the versioned UI IR/MBC. All platforms use the device profile's logical 800×480 coordinates, not browser-relative CSS units. A device profile for a different physical resolution must define one documented integral scale factor for the entire logical canvas.

## Font resources

Noto Sans SC, pinned to one checked-in upstream release and its SIL Open Font License, becomes the canonical `uiSans` source. The build pipeline generates:

- LVGL font assets for ESP32 and macOS at the supported sizes.
- A Web font resource from the same source for the browser renderer.

The base glyph set contains printable ASCII, common Chinese punctuation, U+FFFD, and exactly the 3,755 GB2312 level-1 Han characters in Unicode order. All five 2bpp LVGL assets are embedded in the ESP application so no network or system font is required. A measured 4bpp build required `0x2b1db0` bytes, exceeding the `0x240000` font budget by `0x71db0`; the approved 2bpp build requires `0x1946dc` bytes while preserving every glyph and size. The compiler validates literal text against the selected target font manifest and reports missing glyphs. Runtime-bound text replaces an unsupported character with U+FFFD and emits a host diagnostic rather than silently failing.

The shared source font gives equivalent glyph selection and metrics. Raster anti-aliasing may differ slightly between browser and LVGL; pixel-identical raster output is explicitly out of scope for this phase.

## Renderer behavior

`MicroUiNode` stores its immutable text style. Text patches update text only and preserve the assigned style. LVGL and DOM renderer traits gain style application at node creation. The browser maps logical pixel values directly within the fixed device viewport and loads the generated Web resource. LVGL selects the matching generated LVGL font.

## Simulator layering repair

The simulator enforces `[hidden]` on system and app layers so only one participates in device layout. The active Counter app shell fills the full device content rectangle beneath its Back control. Launcher titles are assigned a compact token rather than the current viewport-relative heading rule.

## Verification

Tests cover MBC style encode/decode and backward-version rejection, compiler lowering and glyph diagnostics, style propagation through core/LVGL/Web renderer fakes, and browser end-to-end checks for fixed device typography and full-screen Counter layering. ESP32 build checks validate generated font assets are present, linked or packaged, and remain within the declared partition budget.
