#!/usr/bin/env python3
"""Generate the subset icon fonts used by the LVGL hosts.

The full Montserrat symbol faces carry ~160 ASCII + Font Awesome glyphs per
size. The OS only ever paints the 42 icons in ``crates/micro-ir/src/icons.rs``,
so the device builds use these RLE-compressed subsets instead:

    12/14 px @ 2 bpp, 18/24/32 px @ 4 bpp (~21 KiB total vs ~110 KiB)

The source is the Font Awesome 5 file bundled with the pinned LVGL checkout.
Every generated file records the source hash and the glyph count; ``--check``
verifies the committed files are current (used by the font budget test).

    python3 scripts/generate-icon-fonts.py
    python3 scripts/generate-icon-fonts.py --check
"""

from __future__ import annotations

import argparse
import hashlib
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ICONS = ROOT / "crates/micro-ir/src/icons.rs"
OUTPUT_DIR = ROOT / "assets/fonts/lvgl"
CONVERTER = ROOT / "work/tools/lv-font-conv/node_modules/.bin/lv_font_conv"
DEFAULT_WOFF = (
    ROOT
    / "target/native-deps/lvgl-src/scripts/built_in_font"
    / "FontAwesome5-Solid+Brands+Regular.woff"
)
# Font Awesome 5 file shipped with the pinned LVGL v9.5.0 checkout.
WOFF_SHA256 = "f4e42f6cd69e5dbdcccc0f2f5be136cebde0e427641e45403bf9173a92da95f4"
# (size_px, bpp): 12 px icons are small enough that 2 bpp keeps up.
SIZES = ((12, 2), (14, 2), (18, 4), (24, 4), (32, 4))
HEADER = """/* Generated from FontAwesome5-Solid+Brands+Regular.woff
 * (SHA-256 {sha}); do not edit.
 * {count} shared OS icon glyphs from crates/micro-ir/src/icons.rs,
 * {size} px at {bpp} bpp, RLE-compressed.
 * MICRO_ICONS_PAYLOAD_BYTES {payload}
 * SPDX-License-Identifier: OFL-1.1 AND CC-BY-4.0
 */
"""


def fail(message: str) -> None:
    raise SystemExit(f"generate-icon-fonts: {message}")


def icon_codepoints() -> list[int]:
    text = ICONS.read_text()
    codepoints = [
        int(value, 16)
        for value in re.findall(
            r'icon!\(\s*"[^"]+"\s*,\s*"\\u\{([0-9a-f]+)\}"', text
        )
    ]
    if not codepoints:
        fail(f"no glyphs found in {ICONS.relative_to(ROOT)}")
    return codepoints


def font_body(codepoints: list[int], size: int, bpp: int, woff: Path) -> str:
    with tempfile.TemporaryDirectory() as directory:
        raw = Path(directory) / f"micro_icons_{size}.c"
        packed = Path(directory) / f"micro_icons_{size}.bin"
        base = [
            str(CONVERTER),
            "--font",
            str(woff),
            "--range",
            ",".join(hex(codepoint) for codepoint in codepoints),
            "--size",
            str(size),
            "--bpp",
            str(bpp),
            "--force-fast-kern-format",
        ]
        for output_format, destination in (("lvgl", raw), ("bin", packed)):
            subprocess.run(
                base + ["--format", output_format, "-o", str(destination)],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.PIPE,
                cwd=ROOT,
            )
        payload = packed.stat().st_size
        text = raw.read_text()
    # Drop lv_font_conv's provenance banner (it embeds machine-local paths) and
    # replace it with a stable header the check can compare.
    text = re.sub(r"\A/\*.*?\*/\n", "", text, count=1, flags=re.DOTALL)
    return (
        HEADER.format(
            sha=WOFF_SHA256,
            count=len(codepoints),
            size=size,
            bpp=bpp,
            payload=payload,
        )
        + text
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify the committed fonts instead of rewriting them",
    )
    parser.add_argument(
        "--font",
        type=Path,
        default=DEFAULT_WOFF,
        help="Font Awesome 5 woff (defaults to the pinned LVGL source tree)",
    )
    arguments = parser.parse_args()

    if not CONVERTER.is_file():
        fail(f"missing lv_font_conv at {CONVERTER.relative_to(ROOT)}")
    if not ICONS.is_file():
        fail(f"missing {ICONS.relative_to(ROOT)}")
    if not arguments.font.is_file():
        fail(
            f"missing {arguments.font}; run the native build once to fetch LVGL "
            "or pass --font"
        )
    digest = hashlib.sha256(arguments.font.read_bytes()).hexdigest()
    if digest != WOFF_SHA256:
        fail(f"{arguments.font} has SHA-256 {digest}, expected {WOFF_SHA256}")

    codepoints = icon_codepoints()
    stale: list[str] = []
    for size, bpp in SIZES:
        expected = font_body(codepoints, size, bpp, arguments.font)
        path = OUTPUT_DIR / f"micro_icons_{size}.c"
        if arguments.check:
            current = path.read_text() if path.is_file() else ""
            if current != expected:
                stale.append(str(path.relative_to(ROOT)))
            continue
        path.write_text(expected)
        print(f"wrote {path.relative_to(ROOT)} ({len(codepoints)} glyphs, {size} px)")

    if stale:
        fail("out of date (run scripts/generate-icon-fonts.py): " + ", ".join(stale))


if __name__ == "__main__":
    sys.exit(main())
