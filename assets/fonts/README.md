# Shared UI font assets

`MicroUiSans` is generated from Noto Sans CJK SC Regular 2.004. The immutable
upstream URL, source SHA-256, license hash, glyph manifest, sizes, bpp, and
tool versions are pinned in `noto-sans-sc.json`; the complete bundled npm
dependency tree and tarball integrity are pinned in `lv-font-conv-lock.json`.
The original OTF and generator
packages stay under ignored `work/fonts/` and `work/tools/`; generated WOFF2
and LVGL C subsets are tracked.

Install the pinned tools locally and regenerate from the repository root:

```zsh
mkdir -p work/fonts work/tools/python work/tools/lv-font-conv
curl -fL -o work/fonts/NotoSansCJKsc-Regular.otf \
  https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf
python3 -m pip install --no-cache-dir --target work/tools/python \
  fonttools==4.59.1 brotli==1.1.0
npm install --prefix work/tools/lv-font-conv --no-save --ignore-scripts \
  lv_font_conv@1.5.3
# The generator verifies node_modules/.package-lock.json byte-for-byte as JSON
# against assets/fonts/lv-font-conv-lock.json before generating.
python3 scripts/generate-font-assets.py fonts
python3 scripts/generate-font-assets.py fonts --check
```

The LVGL assets use 2bpp (four grayscale levels) and RLE compression. The
upstream font does not encode U+FFFD, so the deterministic generator aliases
U+FFFD to that same pinned font's U+25A1 square outline before subsetting.
The declared payload is measured from `lv_font_conv`'s packed binary output for
the same glyphs and settings; `tests/esp32_font_budget.sh` enforces the total.
