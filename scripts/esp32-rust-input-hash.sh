#!/bin/zsh

set -euo pipefail

repo_root=${0:A:h:h}
typeset output_file=""
typeset completion_file=""
typeset completion_archive=""
typeset completion_map=""
if [[ "${1:-}" == --write ]]; then
  [[ $# == 2 ]] || { print -u2 -- "usage: $0 [--write FILE]"; exit 2; }
  output_file=$2
elif [[ "${1:-}" == --write-completion ]]; then
  [[ $# == 4 ]] || {
    print -u2 -- "usage: $0 --write-completion FILE ARCHIVE MAP"
    exit 2
  }
  completion_file=$2
  completion_archive=$3
  completion_map=$4
elif [[ $# != 0 ]]; then
  print -u2 -- "usage: $0 [--write FILE]"
  exit 2
fi

typeset -a inputs=(
  Cargo.toml
  Cargo.lock
  scripts/esp32-rust-input-hash.sh
  firmware/micro-os-esp32/CMakeLists.txt
  firmware/micro-os-esp32/sdkconfig.defaults
  firmware/micro-os-esp32/dependencies.lock
  firmware/micro-os-esp32/main/CMakeLists.txt
  firmware/micro-os-esp32/main/idf_component.yml
  firmware/micro-os-esp32/main/main.c
  firmware/micro-os-esp32/components/micro_runtime_ffi/CMakeLists.txt
  firmware/micro-os-esp32/components/micro_runtime_ffi/include/micro_runtime_ffi.h
  firmware/micro-os-esp32/components/micro_runtime_ffi/placeholder.c
)

typeset crate
typeset source_path=""
for crate in micro-host-esp32 micro-core micro-lvgl micro-ir micro-vm micro-os-core; do
  inputs+=("crates/$crate/Cargo.toml")
  [[ -f "$repo_root/crates/$crate/build.rs" ]] && inputs+=("crates/$crate/build.rs")
  [[ -f "$repo_root/crates/$crate/rust-toolchain.toml" ]] && \
    inputs+=("crates/$crate/rust-toolchain.toml")
  for source_path in "$repo_root/crates/$crate"/src/**/*.rs(N) "$repo_root/crates/$crate"/src/*.rs(N); do
    inputs+=("${source_path#$repo_root/}")
  done
done

typeset manifest_file=$(/usr/bin/mktemp "${TMPDIR:-/tmp}/micro-rust-inputs.XXXXXX")
trap '/bin/rm -f -- "$manifest_file"' EXIT
for input in ${(ou)inputs}; do
  [[ -f "$repo_root/$input" ]] || { print -u2 -- "missing hash input: $input"; exit 1; }
  digest=$(shasum -a 256 "$repo_root/$input" | awk '{print $1}')
  print -r -- "$digest  $input" >> "$manifest_file"
done
aggregate=$(shasum -a 256 "$manifest_file" | awk '{print $1}')

if [[ -n "$completion_file" ]]; then
  [[ -f "$completion_archive" ]] || {
    print -u2 -- "missing Rust archive: $completion_archive"
    exit 1
  }
  [[ -f "$completion_map" ]] || {
    print -u2 -- "missing firmware map: $completion_map"
    exit 1
  }
  archive_hash=$(shasum -a 256 "$completion_archive" | awk '{print $1}')
  map_hash=$(shasum -a 256 "$completion_map" | awk '{print $1}')
  completion_tmp="${completion_file}.tmp.$$"
  trap '/bin/rm -f -- "$manifest_file" "$completion_tmp"' EXIT
  {
    print -r -- "inputs $aggregate"
    print -r -- "archive $archive_hash"
    print -r -- "map $map_hash"
  } > "$completion_tmp"
  /bin/mv -f -- "$completion_tmp" "$completion_file"
elif [[ -n "$output_file" ]]; then
  print -r -- "$aggregate" > "$output_file"
else
  print -r -- "$aggregate"
fi
