#!/usr/bin/env bash
# Copy Tauri bundles to stable, version-free names and write SHA256SUMS.
# Usage: bash scripts/collect-release-assets.sh <linux|windows|macos> <x86_64|aarch64> <out-dir>
set -euo pipefail
os="${1:?os}"; arch="${2:?arch}"; out="${3:?out}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
b="${CARGO_TARGET_DIR:-$repo/target}/release/bundle"
mkdir -p "$out"; rm -f "$out"/*
pick() { # pick <glob-dir> <pattern> <dest-name>
  local f; f="$(find "$1" -maxdepth 1 -name "$2" 2>/dev/null | head -n1)"
  [ -n "$f" ] || { echo "collect: missing $2 in $1" >&2; exit 1; }
  cp "$f" "$out/$3"
}
case "$os" in
  linux)
    pick "$b/deb" '*.deb' "eak-linux-$arch.deb"
    pick "$b/appimage" '*.AppImage' "eak-linux-$arch.AppImage" ;;
  windows) pick "$b/nsis" '*-setup.exe' "eak-windows-$arch-setup.exe" ;;
  macos) pick "$b/dmg" '*.dmg' "eak-macos-$arch.dmg" ;;
  *) echo "collect: bad os $os" >&2; exit 2 ;;
esac
cd "$out"
if command -v sha256sum >/dev/null 2>&1; then sha256sum -- * > SHA256SUMS; else shasum -a 256 -- * > SHA256SUMS; fi
cat SHA256SUMS
