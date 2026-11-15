#!/usr/bin/env bash
# Native build of the EAK desktop app (Linux, macOS). This is THE build function: the one-command
# installer (scripts/install-source.sh) runs this same script on the downloaded source.
#
#   ./scripts/build.sh                       build with the host-appropriate bundle choice
#   EAK_BUNDLES=deb,appimage ./scripts/build.sh   Linux packages instead of the raw binary
#   ./scripts/build.sh --skip-npm-ci         reuse an existing apps/desktop/node_modules
#
# Steps: check prerequisites -> `npm ci` (lockfile, no floating updates) -> ONE `npm run tauri -- build`.
# Tauri runs the configured beforeBuildCommand (`npm run build`: tsc + vite) itself, then cargo
# --release --locked, so the frontend is built exactly once and embedded in the executable.
# (A bare `cargo build --release` would skip that hook and produce a binary that loads the dev URL.)
#
# Options: --bundles <list|auto|none>  --jobs <n>  --skip-npm-ci  --result-file <path>  -h
# Environment (all optional):
#   EAK_BUNDLES        auto (default) | none | comma list of Tauri bundles (deb,appimage,rpm,app,dmg)
#                      auto = Linux: raw binary (no bundle); macOS: EAK.app
#   EAK_BUILD_JOBS     parallel rustc jobs; default comes from total RAM (<6 GB: 1, <12: 2, <24: 3, else 4)
#   EAK_UNLOCKED=1     do not pass --locked to cargo (developers only; the installer never sets this)
#   EAK_COMMIT         commit id to record (set by the installer; default: git rev-parse HEAD)
#   CARGO_TARGET_DIR   cargo target dir (default: <repo>/target)
# On success the last lines name the artifact; --result-file writes them as key=value for scripts.
# Windows: use scripts/build.ps1. No Python is used anywhere.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
APP="$ROOT/apps/desktop"
BIN_NAME="eak-desktop"
BUNDLES="${EAK_BUNDLES:-auto}"
JOBS="${EAK_BUILD_JOBS:-}"
SKIP_NPM_CI=0
RESULT_FILE=""

die() { printf 'build.sh: %s\n' "$1" >&2; shift; for l in "$@"; do printf '          %s\n' "$l" >&2; done; exit 1; }
say() { printf '==> %s\n' "$*"; }

while [ $# -gt 0 ]; do
  case "$1" in
    --bundles) [ $# -ge 2 ] || die "--bundles needs a value"; BUNDLES="$2"; shift 2 ;;
    --jobs) [ $# -ge 2 ] || die "--jobs needs a value"; JOBS="$2"; shift 2 ;;
    --result-file) [ $# -ge 2 ] || die "--result-file needs a path"; RESULT_FILE="$2"; shift 2 ;;
    --skip-npm-ci) SKIP_NPM_CI=1; shift ;;
    -h|--help) sed -n '2,24p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
done

case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  *) die "unsupported OS '$(uname -s)'" "On Windows use: powershell -File scripts\\build.ps1" ;;
esac
ARCH="$(uname -m)"
{ [ -f "$APP/package.json" ] && [ -f "$APP/src-tauri/tauri.conf.json" ]; } || die "not an EAK checkout: $ROOT"

# ---------------------------------------------------------------- bundle choice
if [ "$BUNDLES" = auto ]; then
  if [ "$OS" = macos ]; then BUNDLES=app; else BUNDLES=none; fi
fi
case "$BUNDLES" in *[!A-Za-z0-9,_-]*|'') die "invalid EAK_BUNDLES value '$BUNDLES'" ;; esac
if [ "$BUNDLES" = none ]; then BUNDLE_ARGS=(--no-bundle); else BUNDLE_ARGS=(--bundles "$BUNDLES"); fi
if [ "$OS" = macos ] && [ "$BUNDLES" = none ]; then die "macOS needs the 'app' bundle; EAK_BUNDLES=none is not supported here"; fi

# ---------------------------------------------------------------- prerequisites
say "Checking build prerequisites ($OS-$ARCH)"
MISSING=()
for t in cargo rustc node npm; do command -v "$t" >/dev/null 2>&1 || MISSING+=("$t"); done
[ ${#MISSING[@]} -eq 0 ] || die "missing tools: ${MISSING[*]}" \
  "Install Rust (https://rustup.rs) and Node.js 20.19+ or 22.12+, or run the one-command installer," \
  "which sets them up for you (see docs/INSTALL.md)."

# Node: Vite 8 (apps/desktop) requires ^20.19.0 || >=22.12.0.
NODE_V="$(node -p 'process.versions.node')"
NMAJ="${NODE_V%%.*}"; NREST="${NODE_V#*.}"; NMIN="${NREST%%.*}"
NODE_OK=0
if [ "$NMAJ" -eq 20 ] && [ "$NMIN" -ge 19 ]; then NODE_OK=1; fi
if [ "$NMAJ" -eq 22 ] && [ "$NMIN" -ge 12 ]; then NODE_OK=1; fi
if [ "$NMAJ" -gt 22 ]; then NODE_OK=1; fi
[ "$NODE_OK" = 1 ] || die "Node.js $NODE_V does not satisfy Vite 8" "Need 20.19+ or 22.12+ (21.x is not supported)."

# Rust: workspace rust-version is a floor; the locked dependency set may need a newer stable.
RUST_MIN="$(sed -n 's/^rust-version *= *"\([0-9.]*\)".*/\1/p' "$ROOT/Cargo.toml" | head -n1)"
RUST_V="$(rustc --version | sed -n 's/^rustc \([0-9][0-9.]*\).*/\1/p')"
rust_lt() {   # true when major.minor of $1 is lower than major.minor of $2
  local a="${1%%.*}" b="${2%%.*}" ar="${1#*.}" br="${2#*.}"
  ar="${ar%%.*}"; br="${br%%.*}"
  [ "$a" -lt "$b" ] || { [ "$a" -eq "$b" ] && [ "$ar" -lt "$br" ]; }
}
if [ -n "$RUST_MIN" ] && [ -n "$RUST_V" ] && rust_lt "$RUST_V" "$RUST_MIN"; then
  die "rustc $RUST_V is older than the workspace minimum $RUST_MIN" "Run: rustup update stable"
fi
printf '    node %s, npm %s, %s, %s\n' "$NODE_V" "$(npm --version)" "$(rustc --version)" "$(cargo --version)"

if [ "$OS" = linux ]; then
  command -v cc >/dev/null 2>&1 || command -v gcc >/dev/null 2>&1 || MISSING+=("a C compiler (build-essential)")
  if command -v pkg-config >/dev/null 2>&1; then
    for p in webkit2gtk-4.1 gtk+-3.0 dbus-1; do pkg-config --exists "$p" 2>/dev/null || MISSING+=("pkg-config module $p"); done
  else
    MISSING+=("pkg-config")
  fi
  [ ${#MISSING[@]} -eq 0 ] || die "missing native build prerequisites: ${MISSING[*]}" \
    "Debian/Ubuntu: sudo apt-get install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libdbus-1-dev \\" \
    "  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev curl wget file" \
    "Other distributions: see docs/INSTALL.md (Linux prerequisites)."
else
  xcode-select -p >/dev/null 2>&1 || die "Xcode Command Line Tools are missing" "Run: xcode-select --install   then re-run."
fi

# ---------------------------------------------------------------- build jobs (memory-aware)
if [ -z "$JOBS" ]; then JOBS="${CARGO_BUILD_JOBS:-}"; fi
if [ -z "$JOBS" ]; then
  KB=0
  if [ "$OS" = linux ]; then KB="$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)"
  else KB="$(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1024 ))"; fi
  GB=$(( ${KB:-0} / 1048576 ))
  if [ "$GB" -lt 6 ]; then JOBS=1; elif [ "$GB" -lt 12 ]; then JOBS=2; elif [ "$GB" -lt 24 ]; then JOBS=3; else JOBS=4; fi
  CPUS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)"
  case "$CPUS" in ''|*[!0-9]*) CPUS=1 ;; esac
  if [ "$JOBS" -gt "$CPUS" ]; then JOBS="$CPUS"; fi
  printf '    RAM about %s GB -> CARGO_BUILD_JOBS=%s (override with EAK_BUILD_JOBS)\n' "$GB" "$JOBS"
fi
case "$JOBS" in ''|*[!0-9]*|0) die "invalid build job count '$JOBS' (want a positive integer)" ;; esac
export CARGO_BUILD_JOBS="$JOBS"

# ---------------------------------------------------------------- build
TARGET="${CARGO_TARGET_DIR:-$ROOT/target}"
MARK="$(mktemp "${TMPDIR:-/tmp}/eak-build-mark.XXXXXX")"
trap 'rm -f "$MARK"' EXIT

if [ "$SKIP_NPM_CI" = 1 ]; then
  [ -d "$APP/node_modules" ] || die "--skip-npm-ci but $APP/node_modules does not exist"
  say "Frontend dependencies: reusing node_modules (--skip-npm-ci)"
else
  say "Installing frontend dependencies (npm ci, from package-lock.json)"
  ( cd "$APP" && npm ci --no-audit --no-fund ) || die "npm ci failed" "Check the network and that package-lock.json matches package.json."
fi

CARGO_ARGS=(-- --locked)
if [ "${EAK_UNLOCKED:-0}" = 1 ]; then CARGO_ARGS=(--); fi
say "Building the desktop app: npm run tauri -- build ${BUNDLE_ARGS[*]} (frontend build runs once via beforeBuildCommand)"
printf '    the first build compiles the whole Rust dependency tree and can take a long time\n'
( cd "$APP" && npm run tauri -- build --ci "${BUNDLE_ARGS[@]}" "${CARGO_ARGS[@]}" ) \
  || die "tauri build failed" "Command: (cd apps/desktop && npm run tauri -- build --ci ${BUNDLE_ARGS[*]} ${CARGO_ARGS[*]})" \
         "If the message mentions the lockfile: Cargo.lock needs a deliberate fix; do not build unlocked for an install." \
         "Out of memory or a killed linker: re-run with EAK_BUILD_JOBS=1."

CLI_ARGS=(--locked); if [ "${EAK_UNLOCKED:-0}" = 1 ]; then CLI_ARGS=(); fi
say "Building the EAK CLI and MCP harness: cargo build --release ${CLI_ARGS[*]} -p eak-harness --bin eak --bin eak-mcp"
( cd "$ROOT" && CARGO_TARGET_DIR="$TARGET" cargo build --release "${CLI_ARGS[@]}" -p eak-harness --bin eak --bin eak-mcp ) \
  || die "eak-harness build failed" "Command: cargo build --release ${CLI_ARGS[*]} -p eak-harness --bin eak --bin eak-mcp" \
         "Out of memory or a killed linker: re-run with EAK_BUILD_JOBS=1."
CLI_DIR="$TARGET/release"
for b in eak eak-mcp; do [ -s "$CLI_DIR/$b" ] || die "expected harness binary not found or empty: $CLI_DIR/$b"; done

# ---------------------------------------------------------------- verified artifact discovery
say "Locating the build artifact"
KIND=""; ART=""
if [ "$OS" = macos ]; then
  ART="$(find "$TARGET/release/bundle/macos" -maxdepth 1 -name '*.app' -type d -newer "$MARK" 2>/dev/null | head -n1 || true)"
  [ -n "$ART" ] || die "no .app bundle was produced under $TARGET/release/bundle/macos"
  { [ -s "$ART/Contents/MacOS/$BIN_NAME" ] && [ -f "$ART/Contents/Info.plist" ]; } \
    || die "$ART is not a complete app bundle (missing Contents/MacOS/$BIN_NAME or Info.plist)"
  KIND=app
else
  ART="$TARGET/release/$BIN_NAME"
  [ -s "$ART" ] || die "expected executable not found or empty: $ART"
  [ "$(head -c4 "$ART" | od -An -tx1 | tr -d ' \n')" = 7f454c46 ] || die "$ART is not an ELF executable"
  KIND=binary
fi
VERSION="$(node -p 'require(process.argv[1]).version' "$APP/src-tauri/tauri.conf.json")"
COMMIT="${EAK_COMMIT:-$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)}"
if [ "$BUNDLES" != none ]; then
  printf '    bundles written by this build:\n'
  { find "$TARGET/release/bundle" -type f -newer "$MARK" \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' -o -name '*.dmg' \) 2>/dev/null || true; } | sed 's/^/      /'
fi
if [ -n "$RESULT_FILE" ]; then
  printf 'kind=%s\npath=%s\nversion=%s\ncommit=%s\nos=%s\narch=%s\njobs=%s\ncli_dir=%s\n' \
    "$KIND" "$ART" "$VERSION" "$COMMIT" "$OS" "$ARCH" "$JOBS" "$CLI_DIR" > "$RESULT_FILE"
fi
printf '\nBuilt EAK %s (bundles: %s) for %s-%s, commit %s\nArtifact (%s): %s\n' "$VERSION" "$BUNDLES" "$OS" "$ARCH" "$COMMIT" "$KIND" "$ART"
