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

step "Environment check"
bash scripts/check-env.sh

step "Compile Counter App (TS -> MBC)"
npm run build:app

step "Build native host (LVGL + SDL3)"
cargo build -p micro-host-sdl --features native

step "Headless smoke test (Counter -> state 2)"
cargo run -q -p micro-host-sdl --features native -- --smoke apps/counter/dist/counter.mbc
echo "  smoke OK"

step "Headless OS-shell smoke (shell -> app -> shell)"
cargo run -q -p micro-host-sdl --features native -- --os-smoke apps/shell/dist/shell.mbc apps/counter/dist/counter.mbc apps/settings/dist/settings.mbc
echo "  OS smoke OK"

if "$only_smoke"; then
  echo "  --smoke: skipped the demo window."
else
  step "Launching OS shell window (480x320) — close it to exit"
  cargo run -q -p micro-host-sdl --features native -- --os apps/shell/dist/shell.mbc apps/counter/dist/counter.mbc apps/settings/dist/settings.mbc
fi
