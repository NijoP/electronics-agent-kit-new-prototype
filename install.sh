#!/bin/sh
# Thin entry point: curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.sh | sh
# Delegates to scripts/install.sh (all logic, options and checks live there). From a clone it runs the
# local copy; otherwise it fetches scripts/install.sh over HTTPS from the same repo and ref. The
# release artifacts themselves are SHA-256-verified by scripts/install.sh before anything is installed.
set -eu
REPO="${EAK_REPO:-NijoP/electronics-agent-kit-new-prototype}"
REF="${EAK_INSTALLER_REF:-main}"
command -v bash >/dev/null 2>&1 || { echo "EAK installer needs bash; install it and re-run." >&2; exit 1; }
here="$(dirname -- "$0" 2>/dev/null || echo .)"
if [ -f "$here/scripts/install.sh" ]; then exec bash "$here/scripts/install.sh" "$@"; fi
command -v curl >/dev/null 2>&1 || { echo "EAK installer needs curl; install it and re-run." >&2; exit 1; }
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
curl -fsSL --proto '=https' --tlsv1.2 -o "$tmp/install.sh" "https://raw.githubusercontent.com/$REPO/$REF/scripts/install.sh" \
  || { echo "could not download scripts/install.sh from $REPO@$REF" >&2; exit 1; }
bash "$tmp/install.sh" "$@"
