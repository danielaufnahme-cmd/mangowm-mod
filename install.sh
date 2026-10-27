#!/usr/bin/env bash
# MangoWM Mod installer (user-local, no sudo)
set -euo pipefail

BIN_DIR="$HOME/.local/bin"
LAUNCHER="$BIN_DIR/mangowm-mod"

mkdir -p "$BIN_DIR"

if command -v cargo >/dev/null 2>&1; then
  echo "==> Building native Rust binary for MangoWM Mod"
  cargo build --release
  cp target/release/mangowm-mod "$LAUNCHER"
  chmod +x "$LAUNCHER"
  echo "✓ Installed mangowm-mod to $LAUNCHER"
else
  echo "✗ Cargo is required to build mangowm-mod." >&2
  exit 1
fi
