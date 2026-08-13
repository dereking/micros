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
)

for path in $required; do
  test -f "$path" || { print -u2 "missing: $path"; exit 1; }
done

for name in '"build:app"' '"demo"' '"test"'; do
  /usr/bin/grep -Fq "$name" package.json || {
    print -u2 "missing npm script: $name"
    exit 1
  }
done
