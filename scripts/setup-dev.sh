#!/usr/bin/env bash
# setup-dev.sh — one-time development environment initialization for the
# Micro App Platform. Idempotent: safe to re-run; already-present tools are
# skipped. Run from anywhere; the repo root is resolved from this script.
#
#   npm run setup:dev      # or:  bash scripts/setup-dev.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

step() { printf '\n\033[1m[setup] %s\033[0m\n' "$*"; }
info() { printf '  %s\n' "$*"; }

step "Platform"
case "$(uname -s)" in
  Darwin) info "macOS $(sw_vers -productVersion) — good";;
  *)      info "Not macOS (found $(uname -s)); the native host targets macOS, continuing anyway";;
esac
case "$(uname -m)" in
  arm64)  info "Apple Silicon (arm64) — good";;
  *)      info "Architecture is $(uname -m); Apple Silicon is the supported target";;
esac

step "Xcode Command Line Tools"
if xcode-select -p >/dev/null 2>&1; then
  info "already installed: $(xcode-select -p)"
else
  info "NOT installed — run manually (opens a GUI dialog) and rerun this script:"
  info "    xcode-select --install"
fi

step "Rust toolchain (rustup)"
if command -v cargo >/dev/null 2>&1; then
  info "cargo already installed: $(cargo --version)"
else
  info "installing Rust via rustup (non-interactive)..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env" 2>/dev/null || true
  info "Rust installed: $(cargo --version)"
fi

step "Rust components: rustfmt + clippy"
rustup component add rustfmt clippy
info "rustfmt $(rustfmt --version | awk '{print $2}'), clippy $(cargo clippy --version | awk '{print $2}')"

step "Rust target: wasm32-unknown-unknown"
if rustup target list --installed | grep -qx 'wasm32-unknown-unknown'; then
  info "already installed"
else
  rustup target add wasm32-unknown-unknown
  info "installed"
fi

step "wasm-pack"
if command -v wasm-pack >/dev/null 2>&1; then
  info "already installed: $(wasm-pack --version)"
else
  info "installing wasm-pack via cargo (first build compiles from source; takes a few minutes)"
  cargo install wasm-pack --locked
  info "wasm-pack installed: $(wasm-pack --version)"
fi

step "Node dependencies"
if [[ ! -d node_modules ]]; then
  info "installing npm packages..."
  npm install
elif [[ package-lock.json -nt node_modules ]]; then
  info "package-lock.json is newer than node_modules; refreshing..."
  npm install
else
  info "node_modules up to date"
fi

step "Playwright Chromium"
if compgen -G "$HOME/Library/Caches/ms-playwright/chromium-*" >/dev/null 2>&1; then
  info "Chromium already cached"
else
  info "installing Playwright Chromium..."
  npx playwright install chromium
fi

step "CMake"
if command -v cmake >/dev/null 2>&1; then
  info "cmake $(cmake --version | head -1 | awk '{print $3}')"
else
  info "cmake NOT found — install CMake 3.24+ (e.g. brew install cmake), then rerun this script"
fi

step "Verification"
printf '\n'
if bash scripts/check-env.sh; then
  info "Setup complete. Environment is ready."
else
  printf '\n\033[1m[setup] done, but check-env reported problems — see the report above.\033[0m\n'
fi
