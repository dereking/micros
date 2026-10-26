#!/usr/bin/env bash
# test-native.sh — one-command macOS native test.
#   npm run test:native                # build + headless smoke + open demo window
#   npm run test:native -- --smoke     # headless smoke only (no window, exits)
#
# First run downloads and compiles SDL3 + LVGL into target/native-deps; later
# runs reuse the cache. Run from anywhere; the repo root is resolved here.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

only_smoke=false
[[ "${1:-}" == "--smoke" ]] && only_smoke=true

step() { printf '\n\033[1m[test-native] %s\033[0m\n' "$*"; }

# If the pinned sources were already fetched/cloned (common on a machine
# without GitHub access), build them from target/native-deps instead of
# letting FetchContent reach the network again.
if [[ -d "$REPO_ROOT/target/native-deps/sdl3-src" && -d "$REPO_ROOT/target/native-deps/lvgl-src" ]]; then
  export MICRO_NATIVE_SDL3_SOURCE="$REPO_ROOT/target/native-deps/sdl3-src"
  export MICRO_NATIVE_LVGL_SOURCE="$REPO_ROOT/target/native-deps/lvgl-src"
  export MICRO_NATIVE_OFFLINE=1
fi

step "Environment check"
bash scripts/check-env.sh

step "Compile Apps (TS -> MBC)"
npm run build:app

step "Build native host (LVGL + SDL3)"
cargo build -p micro-host-sdl --features native

step "Headless smoke test (Counter -> state 2)"
cargo run -q -p micro-host-sdl --features native -- --smoke apps/counter/dist/counter.mbc
echo "  smoke OK"

step "Headless momo flow through the real LVGL renderer"
cargo test -q -p micro-host-sdl --features native --test momo_native

step "Headless OS-shell smoke (shell -> app -> shell)"
cargo run -q -p micro-host-sdl --features native -- --os-smoke apps/shell/dist/shell.mbc apps/counter/dist/counter.mbc apps/settings/dist/settings.mbc apps/momo/dist/momo.mbc
echo "  OS smoke OK"

step "Headless OS-shell smoke with momo as the first app"
cargo run -q -p micro-host-sdl --features native -- --os-smoke apps/shell/dist/shell.mbc apps/momo/dist/momo.mbc apps/counter/dist/counter.mbc apps/settings/dist/settings.mbc
echo "  momo OS smoke OK"

step "Headless portrait OS-shell smoke (480x800, fit)"
cargo run -q -p micro-host-sdl --features native -- --orientation portrait --zoom fit --os-smoke apps/shell/dist/shell.mbc apps/counter/dist/counter.mbc apps/settings/dist/settings.mbc apps/momo/dist/momo.mbc
echo "  portrait OS smoke OK"

# Settings is the tallest App (six tab pages of anchored cards), so boot it
# first in portrait: its tree has to reflow into the narrower content box.
step "Headless portrait OS-shell smoke with Settings as the first app"
cargo run -q -p micro-host-sdl --features native -- --orientation portrait --zoom fit --os-smoke apps/shell/dist/shell.mbc apps/settings/dist/settings.mbc apps/counter/dist/counter.mbc apps/momo/dist/momo.mbc
echo "  portrait Settings OS smoke OK"

if "$only_smoke"; then
  echo "  --smoke: skipped the demo window."
else
  step "Launching OS shell window (toolbar: Landscape / Portrait / Rotate / Device menu) — close it to exit"
  cargo run -q -p micro-host-sdl --features native -- --os apps/shell/dist/shell.mbc apps/counter/dist/counter.mbc apps/settings/dist/settings.mbc apps/momo/dist/momo.mbc
fi
