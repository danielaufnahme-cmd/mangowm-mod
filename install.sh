#!/usr/bin/env bash
# MangoWM Mod installer (user-local, no sudo)
# Works from inside a clone, or piped from curl (clones to a temp dir first).
set -euo pipefail

REPO_URL="${MANGOWM_MOD_REPO:-https://github.com/danielaufnahme-cmd/mangowm-mod}"
BIN_DIR="$HOME/.local/bin"
LAUNCHER="$BIN_DIR/mangowm-mod"
TMP_DIR=""

cleanup() {
  [ -n "$TMP_DIR" ] && rm -rf "$TMP_DIR"
  return 0
}
trap cleanup EXIT

if ! command -v cargo >/dev/null 2>&1; then
  echo "✗ Cargo is required to build mangowm-mod." >&2
  exit 1
fi

# Use the local checkout if the script sits in one, otherwise fetch the source
SRC_DIR=""
if [ -n "${BASH_SOURCE[0]:-}" ] && [ -f "${BASH_SOURCE[0]}" ]; then
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  if [ -f "$SCRIPT_DIR/Cargo.toml" ] && grep -q '^name = "mangowm-mod"' "$SCRIPT_DIR/Cargo.toml"; then
    SRC_DIR="$SCRIPT_DIR"
  fi
fi

if [ -z "$SRC_DIR" ]; then
  if ! command -v git >/dev/null 2>&1; then
    echo "✗ git is required to fetch the mangowm-mod source." >&2
    exit 1
  fi
  TMP_DIR="$(mktemp -d)"
  echo "==> Cloning $REPO_URL"
  git clone --depth 1 "$REPO_URL" "$TMP_DIR/mangowm-mod"
  SRC_DIR="$TMP_DIR/mangowm-mod"
fi

mkdir -p "$BIN_DIR"

echo "==> Building native Rust binary for MangoWM Mod"
(cd "$SRC_DIR" && cargo build --release)
cp "$SRC_DIR/target/release/mangowm-mod" "$LAUNCHER"
chmod +x "$LAUNCHER"
echo "✓ Installed mangowm-mod to $LAUNCHER"
