#!/usr/bin/env bash
# Reproducible release build of the EAK desktop app (Linux: .deb + AppImage).
#
# Usage: bash scripts/release-build.sh [OUT_DIR]
#   OUT_DIR defaults to <repo>/target/release-artifacts. It receives the bundles and SHA256SUMS.
#
# Environment (all optional):
#   CARGO_TARGET_DIR   cargo target dir (default: <repo>/target)
#   CARGO_BUILD_JOBS   parallel rustc jobs (lower it on small-RAM machines)
#   EAK_BUNDLES        comma-separated Tauri bundle targets (default: deb,appimage on Linux)
#
# Steps: clean `npm ci` from package-lock.json, `tauri build` (frontend `npm run build` runs via
# beforeBuildCommand, then cargo --release with Cargo.lock), copy bundles, write SHA256SUMS.
# No Python is used. Requires the Linux prerequisites listed in README.md.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${1:-$repo/target/release-artifacts}"
target_dir="${CARGO_TARGET_DIR:-$repo/target}"

case "$(uname -s)" in
  Linux) bundles="${EAK_BUNDLES:-deb,appimage}" ;;
  *) echo "release-build.sh: only Linux packaging is scripted so far" >&2; exit 2 ;;
esac

version="$(node -p "require('$repo/apps/desktop/src-tauri/tauri.conf.json').version")"
pkg_version="$(node -p "require('$repo/apps/desktop/package.json').version")"
cargo_version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/Cargo.toml" | head -n1)"
if [ "$version" != "$pkg_version" ] || [ "$version" != "$cargo_version" ]; then
  echo "version mismatch: tauri.conf=$version package.json=$pkg_version Cargo.toml=$cargo_version" >&2
  exit 1
fi

echo "== EAK $version release build (bundles: $bundles) at $(git -C "$repo" rev-parse HEAD)"
(cd "$repo/apps/desktop" && npm ci --no-audit --no-fund)
(cd "$repo/apps/desktop" && npx tauri build --ci --bundles "$bundles")

bundle_dir="$target_dir/release/bundle"
mkdir -p "$out"
rm -f "$out"/*.deb "$out"/*.AppImage "$out"/SHA256SUMS
found=0
for f in "$bundle_dir"/deb/*_"$version"_*.deb "$bundle_dir"/appimage/*_"$version"_*.AppImage; do
  [ -f "$f" ] || continue
  cp "$f" "$out/"
  found=$((found + 1))
done
if [ "$found" -eq 0 ]; then
  echo "no bundles found under $bundle_dir" >&2
  exit 1
fi

(cd "$out" && sha256sum -- *.deb *.AppImage 2>/dev/null > SHA256SUMS || true)
[ -s "$out/SHA256SUMS" ] || { echo "SHA256SUMS is empty" >&2; exit 1; }
echo "== artifacts in $out"
(cd "$out" && ls -l && cat SHA256SUMS)
