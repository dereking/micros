# Shared UI font assets

`MicroUiSans` is generated from Noto Sans CJK SC Regular 2.004. The IPA
supplement `MicroUiIpa` is generated from Noto Sans Regular 2.015 and is wired
as the fallback face for glyphs the CJK font does not carry. The immutable
upstream URLs, source SHA-256 values, license hashes, glyph manifest, sizes,
bpp, and tool versions are pinned in `noto-sans-sc.json`; the complete bundled
npm dependency tree and tarball integrity are pinned in
`lv-font-conv-lock.json`; the two SIL OFL texts are tracked as
`OFL-1.1.txt` (CJK) and `OFL-1.1-NotoSans.txt` (IPA). The original fonts and
generator packages stay under ignored `work/fonts/` and `work/tools/`;
generated WOFF2 and LVGL C subsets are tracked.

`ui-sans-common.txt` contains printable ASCII, common Chinese punctuation,
U+FFFD, the complete IPA Extensions, Spacing Modifier Letters, and Combining
Diacritics blocks, phonetic Latin/Greek letters, and exactly 3,755 GB2312
level-1 Han characters. The compiler accepts those literals and the runtime
only replaces glyphs outside that manifest.

Install the pinned tools locally and regenerate from the repository root:

```zsh
mkdir -p work/fonts work/tools/python work/tools/lv-font-conv
curl -fL -o work/fonts/NotoSansCJKsc-Regular.otf \
  https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf
curl -fL -o work/fonts/NotoSans-v2.015.zip \
  https://github.com/notofonts/latin-greek-cyrillic/releases/download/NotoSans-v2.015/NotoSans-v2.015.zip
unzip -o -j work/fonts/NotoSans-v2.015.zip \
  'NotoSans/full/otf/NotoSans-Regular.otf' -d work/fonts
python3 -m pip install --no-cache-dir --target work/tools/python \
  fonttools==4.59.1 brotli==1.1.0
npm install --prefix work/tools/lv-font-conv --no-save --ignore-scripts \
  lv_font_conv@1.5.3
# The generator verifies node_modules/.package-lock.json byte-for-byte as JSON
# against assets/fonts/lv-font-conv-lock.json before generating.
python3 scripts/generate-font-assets.py fonts
python3 scripts/generate-font-assets.py fonts --check
```

The LVGL assets use 2bpp (four grayscale levels) and RLE compression. Any LVGL
host embedding them must define `LV_USE_FONT_COMPRESSED 1` (see
`native/lv_conf.h`); without it LVGL returns empty glyph bitmaps and text
renders invisibly while still occupying layout space. The generated 12, 14, 18,
24, and 32px fonts declare intrinsic line heights of 14, 18, 24, 32, and 40px
respectively, matching the closed SDK/CSS metric pairs. The IPA supplement uses
the same line height and baseline as the primary face and is reached through
the generated `.fallback` pointer; the Web player declares it as
`MicroUiIpa` after `MicroUiSans` in the font stack.

The upstream CJK font does not encode U+FFFD, so the deterministic generator
aliases U+FFFD to that same pinned font's U+25A1 square outline before
subsetting. The declared payload is measured from `lv_font_conv`'s packed
binary output for the same glyphs and settings; `tests/esp32_font_budget.sh`
enforces the combined primary + IPA total.

## Icon subset fonts

The LVGL hosts do not embed the full Montserrat symbol faces for `ui.icon`.
`micro_icons_{12,14,18,24,32}.c` contain only the 42 Font Awesome glyphs named
in `crates/micro-ir/src/icons.rs`: 2 bpp at 12/14 px, 4 bpp at 18/24/32 px, all
RLE-compressed, ~21 KiB of flash in total instead of ~110 KiB. The source is
the Font Awesome 5 woff bundled with the pinned LVGL checkout (SHA-256
`f4e42f6c…95f4`); the license is tracked as `FontAwesome5-LICENSE.txt`.

Regenerate after changing the icon catalog (the native build fetches the woff
into `target/native-deps`):

```zsh
python3 scripts/generate-icon-fonts.py
python3 scripts/generate-icon-fonts.py --check
```

The fonts are compiled by `native/CMakeLists.txt` and the ESP32
`micro_runtime_ffi` component. Keep `LV_USE_FONT_COMPRESSED 1` enabled or the
RLE glyphs render empty.
