#!/bin/zsh
set -euo pipefail

required=(
  Cargo.toml
  package.json
  sdk/index.d.ts
  apps/counter/app.ts
  crates/micro-ir/Cargo.toml
  crates/micro-vm/Cargo.toml
  crates/micro-core/Cargo.toml
  crates/micro-compiler/Cargo.toml
  crates/micro-lvgl/Cargo.toml
  crates/micro-host-sdl/Cargo.toml
  crates/micro-renderer-web/Cargo.toml
  crates/micro-host-web/Cargo.toml
  products/micro-web-player/index.html
  products/micro-web-player/src/main.js
  tests/web/counter.spec.js
)

for path in $required; do
  test -f "$path" || { print -u2 "missing: $path"; exit 1; }
done

for name in '"build:app"' '"demo"' '"test"' '"dev:web"' '"test:web"'; do
  /usr/bin/grep -Fq "$name" package.json || {
    print -u2 "missing npm script: $name"
    exit 1
  }
done

ignored=(
  products/micro-web-player/src/generated/micro_web.js
  products/micro-web-player/public/apps/counter.mbc
  products/micro-web-player/dist/index.html
  test-results/result.json
  playwright-report/index.html
  node_modules/vite/package.json
)

for path in $ignored; do
  /usr/bin/git check-ignore -q "$path" || {
    print -u2 "not ignored: $path"
    exit 1
  }
done

if /usr/bin/git ls-files | /usr/bin/grep -Eq \
  '(^|/)(node_modules|dist|generated|test-results|playwright-report)(/|$)|\.mbc$|\.wasm$'; then
  print -u2 "generated Web artifact is tracked"
  exit 1
fi
