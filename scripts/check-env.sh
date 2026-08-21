#!/usr/bin/env bash
# check-env.sh — verify the Micro App Platform development environment.
# Idempotent; safe to re-run at any time. Prints PASS / WARN / FAIL per item
# and exits 1 when a required tool is missing. Run from anywhere.
#
#   npm run check:env      # or:  bash scripts/check-env.sh

set -u

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

status_fail=0
hard=0
soft=0

pass() { printf '  \033[32m[PASS]\033[0m %s\n' "$*"; }
warn() { printf '  \033[33m[WARN]\033[0m %s\n' "$*"; soft=$((soft + 1)); }
fail() { printf '  \033[31m[FAIL]\033[0m %s\n' "$*"; status_fail=1; hard=$((hard + 1)); }

heading() { printf '\n\033[1m%s\033[0m\n' "$*"; }

echo "== Micro App Platform environment check =="
echo "   repo: $REPO_ROOT"

heading "- Platform"
case "$(uname -s)" in
  Darwin) pass "macOS $(sw_vers -productVersion)";;
  *)      fail "expected macOS, found $(uname -s)";;
esac
case "$(uname -m)" in
  arm64)   pass "Apple Silicon (arm64)";;
  x86_64)  warn "Intel Mac — Apple Silicon is the supported target";;
  *)       warn "unknown architecture $(uname -m)";;
esac

heading "- Xcode Command Line Tools"
if xcode-select -p >/dev/null 2>&1; then
  pass "xcode-select: $(xcode-select -p)"
else
  fail "missing — install with: xcode-select --install"
fi

heading "- Rust toolchain"
if command -v cargo >/dev/null 2>&1; then
  pass "cargo $(cargo --version 2>/dev/null | awk '{print $2}')"
else
  fail "cargo missing — install via https://rustup.rs (or run scripts/setup-dev.sh)"
fi
if command -v rustc >/dev/null 2>&1; then
  pass "rustc $(rustc --version 2>/dev/null | awk '{print $2}')"
else
  fail "rustc missing"
fi
if command -v rustfmt >/dev/null 2>&1; then
  pass "rustfmt $(rustfmt --version 2>/dev/null | awk '{print $2}')"
else
  fail "rustfmt missing — run: rustup component add rustfmt"
fi
if command -v cargo-clippy >/dev/null 2>&1 || cargo clippy --version >/dev/null 2>&1; then
  pass "clippy $(cargo clippy --version 2>/dev/null | awk '{print $2}')"
else
  fail "clippy missing — run: rustup component add clippy"
fi

heading "- Web (Wasm) toolchain"
if command -v rustup >/dev/null 2>&1; then
  if rustup target list --installed 2>/dev/null | grep -qx 'wasm32-unknown-unknown'; then
    pass "wasm32-unknown-unknown target"
  else
    fail "wasm32-unknown-unknown missing — run: rustup target add wasm32-unknown-unknown"
  fi
else
  warn "rustup missing — cannot verify wasm target"
fi
if command -v wasm-pack >/dev/null 2>&1; then
  pass "wasm-pack $(wasm-pack --version 2>/dev/null)"
else
  fail "wasm-pack missing — run: cargo install wasm-pack --locked"
fi
if command -v node >/dev/null 2>&1; then
  pass "node $(node --version 2>/dev/null)"
else
  fail "node missing — install Node.js LTS"
fi
if command -v npm >/dev/null 2>&1; then
  pass "npm $(npm --version 2>/dev/null)"
else
  fail "npm missing — install Node.js LTS"
fi
if [[ -d "$REPO_ROOT/node_modules" ]]; then
  pass "node_modules present"
else
  fail "node_modules missing — run: npm install"
fi
if compgen -G "$HOME/Library/Caches/ms-playwright/chromium-*" >/dev/null 2>&1; then
  pass "Playwright Chromium cached"
else
  fail "Playwright Chromium missing — run: npx playwright install chromium"
fi

heading "- Native build"
if command -v cmake >/dev/null 2>&1; then
  pass "cmake $(cmake --version 2>/dev/null | head -1 | awk '{print $3}')"
else
  fail "cmake missing — install CMake 3.24+"
fi
if command -v git >/dev/null 2>&1; then
  pass "git $(git --version 2>/dev/null | awk '{print $3}')"
else
  fail "git missing"
fi
if command -v rust-lldb >/dev/null 2>&1; then
  pass "rust-lldb available for native debugging"
else
  warn "rust-lldb missing — add with: rustup component add rust-lldb"
fi

heading ""
printf 'Result: %d warn, %d fail\n' "$soft" "$hard"

if [[ "$status_fail" -eq 1 ]]; then
  printf '\n\033[31mSome required tools are missing. Run: npm run setup:dev\033[0m\n'
  exit 1
fi
printf '\n\033[32mEnvironment looks ready.\033[0m\n'
