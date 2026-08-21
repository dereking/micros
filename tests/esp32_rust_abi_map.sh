#!/bin/zsh

set -euo pipefail

repo_root=${0:A:h:h}
build_dir="$repo_root/firmware/micro-os-esp32/build"
map_file=${MICRO_ESP32_MAP_FILE:-$build_dir/micro_os_esp32.map}
cmake_cache="$build_dir/CMakeCache.txt"
build_type=$(sed -n 's/^CMAKE_BUILD_TYPE:STRING=//p' "$cmake_cache" 2>/dev/null)
if [[ "$build_type" == Debug ]]; then
  cargo_profile=debug
else
  cargo_profile=release
fi
archive_suffix="esp-idf/micro_runtime_ffi/target/xtensa-esp32s3-espidf/$cargo_profile/libmicro_host_esp32.a"
cmake_file="$repo_root/firmware/micro-os-esp32/components/micro_runtime_ffi/CMakeLists.txt"
hash_script="$repo_root/scripts/esp32-rust-input-hash.sh"
hash_stamp="$build_dir/esp-idf/micro_runtime_ffi/rust-abi-complete.sha256"
archive_file="$build_dir/$archive_suffix"

fail() {
  print -u2 -- "esp32_rust_abi_map: $1"
  exit 1
}

[[ -f "$map_file" ]] || fail "missing firmware map: $map_file"
[[ -x "$hash_script" ]] || fail "missing executable Rust input hash script"
current_hash=$(zsh "$hash_script")
[[ "$current_hash" =~ '^[0-9a-f]{64}$' ]] || \
  fail "Rust input hash script must print exactly one 64-character lowercase hash"
hash_check=$(/usr/bin/mktemp "${TMPDIR:-/tmp}/micro-rust-hash.XXXXXX")
trap '/bin/rm -f -- "$hash_check"' EXIT
zsh "$hash_script" --write "$hash_check"
[[ "$(<"$hash_check")" == "$current_hash" ]] || \
  fail "Rust input hash --write output differs from stdout"
[[ -f "$archive_file" ]] || fail "missing Rust archive: $archive_file"
[[ -f "$hash_stamp" ]] || fail "missing completed Rust ABI build stamp; rebuild firmware"
stamped_inputs=$(awk '$1 == "inputs" { print $2 }' "$hash_stamp")
stamped_archive=$(awk '$1 == "archive" { print $2 }' "$hash_stamp")
stamped_map=$(awk '$1 == "map" { print $2 }' "$hash_stamp")
[[ "$stamped_inputs" == "$current_hash" ]] || \
  fail "Rust archive/map is stale for current tracked inputs; rebuild firmware"
[[ "$stamped_archive" == "$(shasum -a 256 "$archive_file" | awk '{print $1}')" ]] || \
  fail "Rust archive differs from the completed firmware build"
[[ "$stamped_map" == "$(shasum -a 256 "$map_file" | awk '{print $1}')" ]] || \
  fail "firmware map differs from the completed firmware build"
rg -q 'cargo build --manifest-path' "$cmake_file" || fail "Rust component cargo build is missing"
rg -q -- '--package micro-host-esp32 --target [$][{]RUST_TARGET[}] --locked' "$cmake_file" || \
  fail "Rust component cargo build is not locked"

typeset -a symbols=(
  micro_runtime_create
  micro_runtime_activate
  micro_runtime_tick
  micro_runtime_destroy
  micro_os_create
  micro_os_dispatch
  micro_os_state
  micro_os_destroy
)

for symbol in "${symbols[@]}"; do
  rg -q "^[[:space:]]+0x[0-9a-fA-F]+[[:space:]]+$symbol$" "$map_file" || \
    fail "$symbol has no retained non-zero address"
  rg -q "^${symbol}[[:space:]]+.*${archive_suffix}" "$map_file" || \
    fail "$symbol is not sourced from libmicro_host_esp32.a"
done

discarded_end=$(rg -n -m1 '^Memory Configuration$' "$map_file" | cut -d: -f1)
[[ -n "$discarded_end" ]] || fail "map has no Memory Configuration boundary"
for symbol in "${symbols[@]}"; do
  if sed -n "1,${discarded_end}p" "$map_file" | rg -q "[.]text[.]${symbol}([[:space:]]|$)"; then
    fail "$symbol appears in Discarded input sections"
  fi
done

print -- "ESP32 Rust ABI map contract passed"
