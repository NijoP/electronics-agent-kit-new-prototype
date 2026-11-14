#!/usr/bin/env bash
# Fetch the pinned Herdr runtime for a Rust target triple and stage it as a Tauri sidecar:
#   apps/desktop/src-tauri/binaries/herdr-<triple>[.exe]   (+ binaries/conpty/ on Windows)
# Usage: bash scripts/fetch-herdr.sh <target-triple>     Works in Git Bash on Windows too.
set -euo pipefail
triple="${1:?usage: fetch-herdr.sh <target-triple>}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck disable=SC1091
. "$repo/packaging/herdr.pin"
case "$triple" in
  x86_64-unknown-linux-gnu) key=linux_x86_64; asset=herdr-linux-x86_64 ;;
  aarch64-unknown-linux-gnu) key=linux_aarch64; asset=herdr-linux-aarch64 ;;
  aarch64-apple-darwin) key=macos_aarch64; asset=herdr-macos-aarch64 ;;
  x86_64-apple-darwin) key=macos_x86_64; asset=herdr-macos-x86_64 ;;
  x86_64-pc-windows-msvc) key=windows_x86_64; asset=herdr-windows-x86_64.zip ;;
  *) echo "fetch-herdr: no upstream Herdr build for $triple" >&2; exit 2 ;;
esac
want="$(eval echo "\$HERDR_SHA256_$key")"
out="$repo/apps/desktop/src-tauri/binaries"
mkdir -p "$out"
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
curl -fsSL --proto '=https' --tlsv1.2 --retry 3 -o "$tmp/$asset" \
  "https://github.com/herdrdev/herdr/releases/download/$HERDR_TAG/$asset"
if command -v sha256sum >/dev/null 2>&1; then got="$(sha256sum "$tmp/$asset" | cut -d' ' -f1)"
else got="$(shasum -a 256 "$tmp/$asset" | cut -d' ' -f1)"; fi
[ "$got" = "$want" ] || { echo "fetch-herdr: SHA-256 mismatch for $asset (got $got, want $want)" >&2; exit 1; }
case "$asset" in
  *.zip)
    unzip -q -o "$tmp/$asset" -d "$tmp/x"
    cp "$tmp/x/herdr.exe" "$out/herdr-$triple.exe"
    rm -rf "$out/conpty"; cp -r "$tmp/x/conpty" "$out/conpty" ;;
  *) install -m 755 "$tmp/$asset" "$out/herdr-$triple" ;;
esac
echo "fetch-herdr: staged Herdr $HERDR_VERSION for $triple in $out"
