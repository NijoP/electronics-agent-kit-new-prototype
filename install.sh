#!/bin/sh
# EAK installer entry point. DEFAULT (auto): install the verified prebuilt release (built by the release
# workflow from a CI-green commit, SHA-256 checked) when one is published, otherwise build the current source.
#   curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.sh | bash
# Force a mode (the variable goes on the shell, AFTER the pipe):
#   curl -fsSL .../install.sh | bash -s -- --source        (or EAK_INSTALL_MODE=source)  build from source
#   curl -fsSL .../install.sh | bash -s -- --prebuilt      (or EAK_INSTALL_MODE=prebuilt) prebuilt only, fail if none
# Other arguments (--uninstall, --clean-cache, --no-launch, --yes, --force, --ref X) go to the chosen installer.
#
# Works piped from curl (no reliance on a script directory) and from a clone: a clone uses its own
# scripts/; a pipe downloads the matching script over HTTPS from the same repo (EAK_INSTALLER_REF, default
# main) into a temp dir and removes it afterwards. Source mode -> scripts/install-source.sh (builds the
# source; never installs an old release as a stand-in). Prebuilt mode -> scripts/install.sh.
set -eu
REPO="${EAK_REPO:-NijoP/electronics-agent-kit-new-prototype}"
REF="${EAK_INSTALLER_REF:-main}"
MODE="${EAK_INSTALL_MODE:-auto}"

# Pick the mode from flags and pass every other argument through (POSIX sh has no arrays: rotate "$@").
n=$#
while [ "$n" -gt 0 ]; do
  a="$1"; shift; n=$((n - 1))
  case "$a" in
    --prebuilt) MODE=prebuilt ;;
    --source) MODE=source ;;
    *) set -- "$@" "$a" ;;
  esac
done

case "$MODE" in
  auto|source|prebuilt) ;;
  *) echo "EAK_INSTALL_MODE must be 'auto' (default), 'source' or 'prebuilt', got '$MODE'" >&2; exit 2 ;;
esac
case "$REPO" in */*) ;; *) echo "invalid EAK_REPO '$REPO' (want owner/name)" >&2; exit 2 ;; esac

# auto: the verified prebuilt (built by the release workflow from a CI-green commit, SHA-256 checked) is
# the fast path when one is published; otherwise build the source, and say so. Never an old release.
if [ "$MODE" = auto ]; then
  MODE=source
  if command -v curl >/dev/null 2>&1 \
    && curl -fsSL --proto '=https' --tlsv1.2 "https://api.github.com/repos/$REPO/releases?per_page=30" 2>/dev/null | grep -q '"tag_name": *"prebuilt-'; then
    MODE=prebuilt
    echo "EAK: a verified prebuilt release is published: installing it (fast path). Pass --source to build from source instead." >&2
  else
    echo "EAK: no verified prebuilt release is published for this repository: building from source." >&2
  fi
fi
case "$MODE" in source) NAME=install-source.sh ;; *) NAME=install.sh ;; esac

command -v bash >/dev/null 2>&1 || { echo "The EAK installer needs bash; install it and re-run." >&2; exit 1; }

# Use a local clone only when this file really is a file on disk next to scripts/ (when piped, $0 is a shell name).
here=""
if [ -f "$0" ]; then here="$(cd "$(dirname -- "$0")" 2>/dev/null && pwd -P || true)"; fi
if [ -n "$here" ] && [ -f "$here/scripts/$NAME" ] && [ -f "$here/Cargo.toml" ] && [ -f "$here/apps/desktop/package.json" ]; then
  exec bash "$here/scripts/$NAME" "$@"
fi

command -v curl >/dev/null 2>&1 || { echo "The EAK installer needs curl; install it and re-run." >&2; exit 1; }
tmp="$(mktemp -d "${TMPDIR:-/tmp}/eak-bootstrap.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT INT TERM
curl -fsSL --proto '=https' --tlsv1.2 --retry 3 -o "$tmp/$NAME" "https://raw.githubusercontent.com/$REPO/$REF/scripts/$NAME" \
  || { echo "could not download scripts/$NAME from $REPO@$REF (check your network and EAK_INSTALLER_REF)" >&2; exit 1; }
# The child's stdin is detached: when this script itself came through a pipe, nothing the installer
# runs may read the rest of it. Prompts in the installer read /dev/tty directly.
rc=0
bash "$tmp/$NAME" "$@" < /dev/null || rc=$?
exit "$rc"
