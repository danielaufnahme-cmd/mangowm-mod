#!/bin/sh
# MangoWM Mod installer (user-local, never uses sudo, never edits shell rc files).
# Builds from source with cargo, or installs a verified prebuilt static binary.
# Works from inside a clone, or piped: curl -fsSL <url>/install.sh | sh
set -eu

REPO_URL="${MANGOWM_MOD_REPO:-https://github.com/danielaufnahme-cmd/mangowm-mod}"
BIN_DIR="${MANGOWM_MOD_BIN_DIR:-$HOME/.local/bin}"
DOWNLOAD_BASE="${MANGOWM_MOD_DOWNLOAD_BASE:-}"
MIN_MAJOR=1
MIN_MINOR=80
MODE=auto # auto | source | prebuilt
VERSION=""
ASSUME_YES=0
TMP_DIR=""

usage() {
  cat <<EOF
Usage: install.sh [options]

  --prebuilt        install the prebuilt static binary (checksum verified)
  --source          build from source with cargo
  --version vX.Y.Z  prebuilt: install this release instead of the latest
  --bin-dir DIR     install into DIR (default: \$HOME/.local/bin)
  --yes             never prompt
  --help            show this help

Environment: MANGOWM_MOD_REPO, MANGOWM_MOD_BIN_DIR, MANGOWM_MOD_DOWNLOAD_BASE
EOF
}

say() { printf '%s\n' "$*"; }
err() { printf '✗ %s\n' "$*" >&2; }
die() {
  err "$*"
  exit 1
}

cleanup() {
  if [ -n "$TMP_DIR" ]; then rm -rf "$TMP_DIR"; fi
  return 0
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# ---- arguments ----
while [ $# -gt 0 ]; do
  case "$1" in
    --prebuilt) MODE=prebuilt ;;
    --source) MODE=source ;;
    --yes | -y) ASSUME_YES=1 ;;
    --version)
      [ $# -ge 2 ] || die "--version needs a value (e.g. v0.1.0)"
      VERSION="$2"
      shift
      ;;
    --bin-dir)
      [ $# -ge 2 ] || die "--bin-dir needs a directory"
      BIN_DIR="$2"
      shift
      ;;
    --help | -h)
      usage
      exit 0
      ;;
    *)
      err "Unknown option: $1"
      usage >&2
      exit 1
      ;;
  esac
  shift
done

# ---- platform ----
OS_NAME="$(uname -s)"
case "$OS_NAME" in
  Linux) OS=linux ;;
  FreeBSD | OpenBSD) OS=bsd ;;
  *) die "MangoWM is Wayland/Linux only; $OS_NAME is not supported." ;;
esac

case "$(uname -m)" in
  x86_64 | amd64) ARCH=x86_64 ;;
  aarch64 | arm64) ARCH=aarch64 ;;
  *) ARCH="" ;;
esac

CAN_PREBUILT=1
if [ "$OS" != linux ] || [ -z "$ARCH" ]; then CAN_PREBUILT=0; fi

# ---- helpers ----
have() { command -v "$1" >/dev/null 2>&1; }

# True when cargo exists and is >= MIN_MAJOR.MIN_MINOR
rust_ok() {
  have cargo || return 1
  _v="$(cargo --version 2>/dev/null)" || return 1
  _maj="$(printf '%s\n' "$_v" | sed -n 's/^cargo \([0-9][0-9]*\)\..*/\1/p')"
  _min="$(printf '%s\n' "$_v" | sed -n 's/^cargo [0-9][0-9]*\.\([0-9][0-9]*\).*/\1/p')"
  [ -n "$_maj" ] && [ -n "$_min" ] || return 1
  [ "$_maj" -gt "$MIN_MAJOR" ] && return 0
  [ "$_maj" -eq "$MIN_MAJOR" ] && [ "$_min" -ge "$MIN_MINOR" ] && return 0
  return 1
}

# Print how to install rust/cargo, git and a C linker on this distro
print_pkg_help() {
  say ""
  say "Rust >= $MIN_MAJOR.$MIN_MINOR (cargo), git and a C linker are needed to build from source."
  say "Install them with your package manager (run as root):"
  say ""
  if have dnf; then
    say "  sudo dnf install rust cargo git gcc"
  elif have apt-get; then
    say "  sudo apt install cargo rustc git build-essential"
  elif have pacman; then
    say "  sudo pacman -S rust git gcc"
  elif have zypper; then
    say "  sudo zypper install rust cargo git gcc" # package names: unverified on openSUSE
  elif have apk; then
    say "  doas apk add rust cargo git build-base" # unverified: also needs musl-dev (in build-base)
  elif have xbps-install; then
    say "  sudo xbps-install -S rust cargo git gcc" # unverified on Void
  elif have emerge; then
    say "  sudo emerge --ask dev-lang/rust-bin dev-vcs/git" # unverified: gcc ships in @system
  elif have nix-shell || have nix; then
    say "  nix-shell -p cargo rustc git gcc" # unverified; NixOS: add them to your config for permanence
  else
    say "  (package manager not recognised: install rust, cargo, git and gcc/cc)"
  fi
  say ""
  say "If your distro's Rust is older than $MIN_MAJOR.$MIN_MINOR, use rustup instead (works everywhere):"
  say "  https://rustup.rs"
  say ""
  if [ "$CAN_PREBUILT" -eq 1 ]; then
    say "Or run this installer with --prebuilt for a ready-made static binary."
  fi
}

# Y/n question; yes when --yes or no terminal is available
confirm() {
  [ "$ASSUME_YES" -eq 1 ] && return 0
  if ! (: </dev/tty) 2>/dev/null; then return 0; fi
  printf '%s [Y/n] ' "$1" >/dev/tty
  _a=""
  read -r _a </dev/tty || _a=""
  case "$_a" in
    n | N | no | NO | No) return 1 ;;
    *) return 0 ;;
  esac
}

fetch() { # fetch URL DEST
  if have curl; then
    curl -fsSL -o "$2" "$1"
  elif have wget; then
    wget -q -O "$2" "$1"
  else
    die "curl or wget is required to download the prebuilt binary."
  fi
}

sha256_of() { # print the hex sha256 of a file
  if have sha256sum; then
    sha256sum "$1" | cut -d' ' -f1
  elif have shasum; then
    shasum -a 256 "$1" | cut -d' ' -f1
  elif have openssl; then
    openssl dgst -sha256 "$1" | sed 's/^.*= *//'
  else
    return 1
  fi
}

mk_tmp() {
  if [ -z "$TMP_DIR" ]; then TMP_DIR="$(mktemp -d)"; fi
}

# Copy a binary into BIN_DIR (atomic replace, mode 755)
place_binary() {
  mkdir -p "$BIN_DIR"
  cp "$1" "$BIN_DIR/.mangowm-mod.new"
  chmod 755 "$BIN_DIR/.mangowm-mod.new"
  mv -f "$BIN_DIR/.mangowm-mod.new" "$BIN_DIR/mangowm-mod"
}

# ---- install paths ----
install_source() {
  rust_ok || {
    err "cargo >= $MIN_MAJOR.$MIN_MINOR not found."
    print_pkg_help
    exit 1
  }
  SRC_DIR=""
  # Local checkout? ($0 is just "sh" when piped, so the file test fails)
  if [ -f "$0" ]; then
    _d="$(cd "$(dirname "$0")" && pwd)"
    if [ -f "$_d/Cargo.toml" ] && grep -q '^name = "mangowm-mod"' "$_d/Cargo.toml"; then
      SRC_DIR="$_d"
    fi
  fi
  if [ -z "$SRC_DIR" ]; then
    have git || {
      err "git is required to fetch the mangowm-mod source."
      print_pkg_help
      exit 1
    }
    mk_tmp
    say "==> Cloning $REPO_URL"
    if [ -n "$VERSION" ]; then
      git clone --depth 1 --branch "$VERSION" "$REPO_URL" "$TMP_DIR/mangowm-mod"
    else
      git clone --depth 1 "$REPO_URL" "$TMP_DIR/mangowm-mod"
    fi
    SRC_DIR="$TMP_DIR/mangowm-mod"
  fi
  say "==> Building mangowm-mod (release)"
  (cd "$SRC_DIR" && cargo build --release --locked)
  place_binary "$SRC_DIR/target/release/mangowm-mod"
}

install_prebuilt() {
  asset="mangowm-mod-$ARCH-unknown-linux-musl.tar.gz"
  if [ -n "$DOWNLOAD_BASE" ]; then
    base="$DOWNLOAD_BASE"
  elif [ -n "$VERSION" ]; then
    base="$REPO_URL/releases/download/$VERSION"
  else
    base="$REPO_URL/releases/latest/download"
  fi
  mk_tmp
  say "==> Downloading $asset"
  fetch "$base/$asset" "$TMP_DIR/$asset" || die "Download failed: $base/$asset"
  fetch "$base/$asset.sha256" "$TMP_DIR/$asset.sha256" || die "Download failed: $base/$asset.sha256"

  say "==> Verifying checksum"
  expected="$(cut -d' ' -f1 <"$TMP_DIR/$asset.sha256" | head -n 1)"
  actual="$(sha256_of "$TMP_DIR/$asset")" || die "No sha256sum, shasum or openssl found; refusing to install an unverified binary."
  if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
    die "Checksum mismatch for $asset (expected '$expected', got '$actual'). Nothing was installed."
  fi
  say "✓ Checksum OK"

  mkdir "$TMP_DIR/x"
  tar -xzf "$TMP_DIR/$asset" -C "$TMP_DIR/x"
  [ -f "$TMP_DIR/x/mangowm-mod" ] || die "Archive does not contain mangowm-mod."
  place_binary "$TMP_DIR/x/mangowm-mod"
}

# ---- decide ----
if [ "$MODE" = prebuilt ] && [ "$CAN_PREBUILT" -eq 0 ]; then
  die "No prebuilt binary for $OS_NAME/$(uname -m). Use --source."
fi

if [ "$MODE" = auto ]; then
  if rust_ok; then MODE=source; else MODE=prebuilt; fi
fi

if [ "$MODE" = prebuilt ]; then
  if [ "$CAN_PREBUILT" -eq 0 ]; then
    err "No prebuilt binary for $OS_NAME/$(uname -m); a source build is needed."
    print_pkg_help
    exit 1
  fi
  say "==> Prebuilt static binary ($ARCH)"
  if ! confirm "Download and install the prebuilt binary to $BIN_DIR?"; then
    print_pkg_help
    exit 1
  fi
  install_prebuilt
else
  install_source
fi

say "✓ Installed mangowm-mod to $BIN_DIR/mangowm-mod"

# ---- after install ----
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    say ""
    say "$BIN_DIR is not in your PATH. Add it:"
    say "  bash: echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.bashrc"
    say "  zsh:  echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.zshrc"
    say "  fish: fish_add_path $BIN_DIR"
    ;;
esac

if _ver="$("$BIN_DIR/mangowm-mod" --version 2>/dev/null)"; then
  say "$_ver"
fi
