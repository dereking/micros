#!/bin/zsh
set -euo pipefail
repo_root=${0:A:h:h}
metadata="$repo_root/assets/fonts/noto-sans-sc.json"; license="$repo_root/assets/fonts/OFL-1.1.txt"
manifest="$repo_root/assets/fonts/ui-sans-common.txt"; generator="$repo_root/scripts/generate-font-assets.py"
lock="$repo_root/assets/fonts/lv-font-conv-lock.json"; web_font="$repo_root/products/micro-web-player/public/fonts/micro-ui-sans-common.woff2"
font_limit=$((0x240000)); reserve=$((0x40000)); test_mode=${MICRO_FONT_BUDGET_TEST_MODE:-0}
partitions=${MICRO_FONT_BUDGET_PARTITIONS:-"$repo_root/firmware/micro-os-esp32/partitions_8m.csv"}
sizes=${MICRO_FONT_BUDGET_SIZES:-"$repo_root/assets/fonts/lvgl/micro_ui_sans_sizes.json"}
map_file=${MICRO_FONT_BUDGET_MAP:-"$repo_root/firmware/micro-os-esp32/build/micro_os_esp32.map"}
app_bin=${MICRO_FONT_BUDGET_BIN:-"$repo_root/firmware/micro-os-esp32/build/micro_os_esp32.bin"}
fail() { print -u2 -- "esp32_font_budget: $1"; exit 1; }
if [[ $test_mode != 1 ]] && [[ -n ${MICRO_FONT_BUDGET_PARTITIONS:-}${MICRO_FONT_BUDGET_SIZES:-}${MICRO_FONT_BUDGET_MAP:-}${MICRO_FONT_BUDGET_BIN:-}${MICRO_FONT_SKIP_GENERATOR_CHECK:-} ]]; then fail "test overrides require MICRO_FONT_BUDGET_TEST_MODE=1"; fi
for required in "$metadata" "$license" "$manifest" "$generator" "$lock" "$sizes" "$web_font" "$partitions" "$map_file" "$app_bin"; do [[ -f "$required" ]] || fail "missing ${required#$repo_root/}"; done
if [[ ${MICRO_FONT_SKIP_GENERATOR_CHECK:-0} != 1 ]]; then python3 "$generator" fonts --check || fail "generated font assets are not deterministic"; elif [[ $test_mode != 1 ]]; then fail "generator check may only be skipped by mutation tests"; fi

python3 - "$repo_root" "$metadata" "$license" "$lock" "$sizes" "$web_font" "$partitions" "$map_file" "$app_bin" "$font_limit" "$reserve" <<'PY'
import csv, hashlib, json, pathlib, re, sys
root, metadata_path, license_path, lock_path, sizes_path, web_path, partitions_path, map_path, bin_path = map(pathlib.Path, sys.argv[1:10])
font_limit, reserve = map(int, sys.argv[10:12])
def fail(message): raise SystemExit(f"esp32_font_budget: {message}")
metadata=json.loads(metadata_path.read_text())
required={"family":"Noto Sans CJK SC","weight":400,"version":"2.004","license":"SIL Open Font License 1.1","license_url":"https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/LICENSE","source_url":"https://raw.githubusercontent.com/notofonts/noto-cjk/Sans2.004/Sans/OTF/SimplifiedChinese/NotoSansCJKsc-Regular.otf"}
for key,value in required.items():
    if metadata.get(key)!=value: fail(f"font metadata {key!r} must equal {value!r}")
if metadata.get("source_sha256")!="2c76254f6fc379fddfce0a7e84fb5385bb135d3e399294f6eeb6680d0365b74b": fail("font source hash is not pinned upstream 2.004")
if metadata.get("license_sha256")!=hashlib.sha256(license_path.read_bytes()).hexdigest() or "SIL OPEN FONT LICENSE Version 1.1" not in license_path.read_text(): fail("tracked OFL-1.1 license/hash differs")
if metadata.get("license_file")!=license_path.relative_to(root).as_posix(): fail("metadata license path differs")
if metadata.get("glyph_manifest")!="assets/fonts/ui-sans-common.txt" or metadata.get("generator_lock")!="assets/fonts/lv-font-conv-lock.json": fail("metadata asset paths differ")
if metadata.get("lvgl_bpp")!=2 or metadata.get("sizes_px")!=[12,14,18,24,32]: fail("metadata must declare five 2bpp fonts")
if metadata.get("generation_tools")!={"fonttools":"4.59.1","brotli":"1.1.0","lv_font_conv":"1.5.3"}: fail("generator versions differ from pins")
package=json.loads(lock_path.read_text()).get("packages",{}).get("node_modules/lv_font_conv",{})
if package.get("version")!="1.5.3" or package.get("integrity")!="sha512-0xJQThBOw2iptFccSXrKDIUTQAwr/2zhKjCI1lATIRgZo8uvYRTmenKafW9yTw6G0y5AyW00tqGpUtYuTuBIbQ==": fail("lv_font_conv lock pin/integrity differs")
declared=json.loads(sizes_path.read_text()); fonts=declared.get("fonts")
if declared.get("source_sha256")!=metadata["source_sha256"] or declared.get("bpp")!=2: fail("font declaration source/bpp differs")
if not isinstance(fonts,list) or [f.get("size_px") for f in fonts]!=[12,14,18,24,32]: fail("font declaration sizes differ")
total=0
for font in fonts:
    path=root/font.get("path","")
    if not path.is_file(): fail(f"missing {path}")
    data=path.read_bytes(); payload=font.get("payload_bytes")
    if hashlib.sha256(data).hexdigest()!=font.get("sha256"): fail(f"asset hash differs for {path.relative_to(root)}")
    if not isinstance(payload,int) or f"MICRO_UI_SANS_PAYLOAD_BYTES {payload}".encode() not in data or b" * Bpp: 2" not in data: fail(f"asset declaration differs for {path.relative_to(root)}")
    total+=payload
if declared.get("total_payload_bytes")!=total or total>font_limit: fail(f"LVGL payload {total:#x} exceeds {font_limit:#x} or declaration differs")
newest_asset=max((root/f["path"]).stat().st_mtime_ns for f in fonts)
if map_path.stat().st_mtime_ns<newest_asset or bin_path.stat().st_mtime_ns<newest_asset: fail("ESP map/binary are older than generated font assets; rebuild required")
web=declared.get("web",{})
if web.get("path")!=web_path.relative_to(root).as_posix() or web.get("sha256")!=hashlib.sha256(web_path.read_bytes()).hexdigest(): fail("web asset declaration differs")
factory=[]
with partitions_path.open(newline="") as source:
    for row in csv.reader(line for line in source if not line.lstrip().startswith("#")):
        fields=[field.strip() for field in row]
        if len(fields)>=5 and fields[:3]==["factory","app","factory"]: factory.append(int(fields[4],0))
if len(factory)!=1: fail("partition table must contain exactly one factory app partition")
factory_size=factory[0]; app_size=bin_path.stat().st_size
if app_size>factory_size-reserve: fail(f"ESP app is {app_size:#x}; factory {factory_size:#x} cannot retain {reserve:#x} free")
map_text=map_path.read_text(errors="replace")
for size in (12,14,18,24,32):
    symbol=f"micro_ui_sans_{size}"; source=f"esp-idf/micro_bsp_lcd7/libmicro_bsp_lcd7.a(micro_ui_sans_{size}.c.obj)"
    if not re.search(rf"\b{symbol}\b.*{re.escape(source)}", map_text): fail(f"linked map lacks {symbol} from BSP font asset")
print(f"ESP32 font budget passed: LVGL={total:#x}<={font_limit:#x}; app={app_size:#x}; factory={factory_size:#x}; free={factory_size-app_size:#x}")
PY

if [[ $test_mode != 1 ]]; then
  mutation_dir=
  cleanup_mutation_dir() {
    [[ -n $mutation_dir ]] || return 0
    rm -f -- "$mutation_dir/tiny.csv" "$mutation_dir/missing.map" "$mutation_dir/oversize.bin"
    [[ ! -d $mutation_dir ]] || rmdir -- "$mutation_dir"
  }
  trap cleanup_mutation_dir EXIT
  mutation_dir=$(mktemp -d "${TMPDIR:-/tmp}/micro-font-budget.XXXXXX")
  expect_failure() { if "$@" >/dev/null 2>&1; then fail "negative mutation unexpectedly passed"; fi; }
  sed 's/0x380000/0x080000/' "$partitions" > "$mutation_dir/tiny.csv"
  expect_failure env MICRO_FONT_BUDGET_TEST_MODE=1 MICRO_FONT_SKIP_GENERATOR_CHECK=1 MICRO_FONT_BUDGET_PARTITIONS="$mutation_dir/tiny.csv" zsh "$0"
  sed '/micro_ui_sans_32/d' "$map_file" > "$mutation_dir/missing.map"
  expect_failure env MICRO_FONT_BUDGET_TEST_MODE=1 MICRO_FONT_SKIP_GENERATOR_CHECK=1 MICRO_FONT_BUDGET_MAP="$mutation_dir/missing.map" zsh "$0"
  dd if=/dev/zero of="$mutation_dir/oversize.bin" bs=1 count=0 seek=$((0x340001)) 2>/dev/null
  expect_failure env MICRO_FONT_BUDGET_TEST_MODE=1 MICRO_FONT_SKIP_GENERATOR_CHECK=1 MICRO_FONT_BUDGET_BIN="$mutation_dir/oversize.bin" zsh "$0"
  cleanup_mutation_dir
  [[ ! -e $mutation_dir ]] || fail "mutation directory cleanup failed: $mutation_dir"
  mutation_dir=
  trap - EXIT
  print -- "ESP32 font budget negative mutations passed"
fi
