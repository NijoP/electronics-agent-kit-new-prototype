#!/usr/bin/env bash
# Keeps README.md's install commands honest against the release workflow.
#  1. The app version agrees in Cargo.toml, tauri.conf.json and package.json (a version tag must match it).
#  2. Every release download URL in README.md names an asset that .github/workflows/release.yml and
#     scripts/collect-release-assets.sh actually produce.
#  3. With GH_TOKEN set (CI), every release tag README.md links to exists and holds each asset named for it.
# Usage: bash scripts/check-release-docs.sh
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readme="$repo/README.md"
wf="$repo/.github/workflows/release.yml"
fail=0
err() { echo "check-release-docs: $*" >&2; fail=1; }

cargo_v="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$repo/Cargo.toml" | head -n1)"
tauri_v="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$repo/apps/desktop/src-tauri/tauri.conf.json" | head -n1)"
npm_v="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' "$repo/apps/desktop/package.json" | head -n1)"
[ -n "$cargo_v" ] && [ "$cargo_v" = "$tauri_v" ] && [ "$cargo_v" = "$npm_v" ] \
  || err "app versions disagree: Cargo.toml '$cargo_v', tauri.conf.json '$tauri_v', package.json '$npm_v'"

# Asset names the release matrix produces (names from collect-release-assets.sh and release.yml).
expected="SHA256SUMS provenance.json"
while read -r os arch; do
  case "$os" in
    linux) expected="$expected eak-linux-$arch.deb eak-linux-$arch.AppImage eak-cli-linux-$arch.tar.gz" ;;
    windows) expected="$expected eak-windows-$arch-setup.exe eak-cli-windows-$arch.zip" ;;
    macos) expected="$expected eak-macos-$arch.dmg eak-cli-macos-$arch.tar.gz" ;;
    *) err "unknown os '$os' in the release matrix" ;;
  esac
done < <(sed -n 's/.*{ name: [^,]*, runner: [^,]*, os: \([a-z]*\), arch: \([a-z0-9_]*\),.*/\1 \2/p' "$wf")
[ "$(echo "$expected" | wc -w)" -gt 2 ] || err "could not read the release matrix from $wf"

base='https://github.com/NijoP/electronics-agent-kit-new-prototype/releases/download/'
urls="$(grep -o "${base}[^ )\"'\`]*" "$readme" | sed -e 's/\$ARCH/aarch64/' -e 'p' -e 's/aarch64/x86_64/' | sort -u || true)"
[ -n "$urls" ] || err "README.md has no release download URL"
for u in $urls; do
  rest="${u#"$base"}"
  tag="${rest%%/*}"
  asset="${rest#*/}"
  [[ "$tag" =~ ^(v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?|prebuilt-[0-9]{8}-[0-9a-f]{7})$ ]] \
    || err "README.md links to an unexpected release tag '$tag' ($u)"
  case " $expected " in *" $asset "*) ;; *) err "README.md links to '$asset', which the release workflow does not produce ($u)" ;; esac
done

if [ -n "${GH_TOKEN:-}" ] && [ -n "$urls" ]; then
  for tag in $(for u in $urls; do r="${u#"$base"}"; echo "${r%%/*}"; done | sort -u); do
    assets="$(gh release view "$tag" -R NijoP/electronics-agent-kit-new-prototype --json assets --jq '.assets[].name')" \
      || { err "README.md links to release '$tag', which does not exist"; continue; }
    for u in $urls; do
      r="${u#"$base"}"
      [ "${r%%/*}" = "$tag" ] || continue
      grep -qx -- "${r#*/}" <<<"$assets" || err "release '$tag' has no asset '${r#*/}' ($u)"
    done
  done
else
  echo "check-release-docs: GH_TOKEN not set; skipped the live release check" >&2
fi

[ "$fail" = 0 ] && echo "check-release-docs: README release links and app versions are consistent"
exit "$fail"
