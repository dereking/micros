#!/bin/zsh
set -euo pipefail

repo_root=${0:A:h:h}
fixture="$repo_root/tests/fixtures/esp32_ui_bridge"
output_dir=
cleanup_output_dir() {
  [[ -n $output_dir && -d $output_dir ]] || return 0
  rm -rf -- "$output_dir"
}
trap cleanup_output_dir EXIT
output_dir=$(mktemp -d "${TMPDIR:-/tmp}/micro-esp-ui-bridge.XXXXXX")
output="$output_dir/test"

TMPDIR="$output_dir" cc -std=c17 -Wall -Wextra -Werror -DMICRO_UI_BRIDGE_HOST_TEST \
  -I"$fixture" -I"$repo_root/firmware/micro-os-esp32/components/micro_runtime_ffi/include" \
  "$repo_root/firmware/micro-os-esp32/components/micro_runtime_ffi/placeholder.c" \
  "$fixture/test.c" -o "$output"
"$output"
