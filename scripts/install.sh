#!/usr/bin/env bash
# EAK desktop installer for Linux and macOS: check prerequisites, get the source, build the
# desktop app from source, install a launcher, start it.
#
#   curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.sh | bash
#   bash scripts/install.sh [--no-launch] [--dir <checkout>] [--repo <git-url>] [--ref <branch>]
#
# It never runs sudo. Missing system packages are reported with the exact command to run.
# Environment: EAK_DIR, EAK_REPO_URL, EAK_REF, CARGO_BUILD_JOBS, CARGO_TARGET_DIR.
set -euo pipefail

REPO_URL="${EAK_REPO_URL:-https://github.com/NijoP/electronics-agent-kit-new-prototype}"
REF="${EAK_REF:-main}"
DIR="${EAK_DIR:-}"
LAUNCH=1

STEP=0
step() { STEP=$((STEP + 1)); printf '\n==> [%s] %s\n' "$STEP" "$*"; }
info() { printf '    %s\n' "$*"; }
fail() {
  printf '\nERROR at step [%s]: %s\n' "$STEP" "$1" >&2
  shift
  for line in "$@"; do printf '       %s\n' "$line" >&2; done
  exit 1
}

while [ $# -gt 0 ]; do
  case "$1" in
    --no-launch) LAUNCH=0 ;;
    --dir) [ $# -ge 2 ] || fail "--dir needs a path"; DIR="$2"; shift ;;
    --repo) [ $# -ge 2 ] || fail "--repo needs a git URL"; REPO_URL="$2"; shift ;;
    --ref) [ $# -ge 2 ] || fail "--ref needs a branch or tag"; REF="$2"; shift ;;
    -h|--help) sed -n '2,9p' "$0" 2>/dev/null || true; exit 0 ;;
    *) fail "unknown option: $1 (try --help)" ;;
  esac
  shift
done

version_ge() { [ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n1)" = "$2" ]; }

# ---------------------------------------------------------------- 1. platform
step "Detecting platform"
OS="$(uname -s)"
case "$OS" in
  Linux) PLATFORM=linux ;;
  Darwin) PLATFORM=macos ;;
  *) fail "unsupported OS '$OS'" "On Windows use scripts/install.ps1 in PowerShell." ;;
esac
info "platform: $PLATFORM ($(uname -m))"
if [ "$PLATFORM" = macos ]; then info "macOS support is UNVERIFIED: this script has not been run on a Mac."; fi

# ---------------------------------------------------------------- 2. prerequisites
step "Checking prerequisites"
MISSING=()

if command -v git >/dev/null 2>&1; then info "git: $(git --version)"; else MISSING+=("git"); fi

if command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
  RUSTV="$(rustc --version | awk '{print $2}')"
  info "rust: rustc $RUSTV, $(cargo --version)"
  version_ge "$RUSTV" "1.80.0" || fail "Rust $RUSTV is too old (need 1.80 or newer)" "Run: rustup update stable"
else
  fail "Rust toolchain (cargo, rustc) not found" \
    "Install it with rustup: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" \
    "Then open a new shell (or: . \"\$HOME/.cargo/env\") and re-run this script."
fi

if command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1; then
  NODEV="$(node --version | sed 's/^v//')"
  info "node: v$NODEV, npm $(npm --version)"
  version_ge "$NODEV" "22.0.0" || fail "Node.js v$NODEV is too old (need 22 or newer)" \
    "Install a current Node.js from https://nodejs.org or via your package manager / nvm."
else
  fail "Node.js 22+ and npm not found" "Install from https://nodejs.org or via your package manager / nvm."
fi

if [ "$PLATFORM" = linux ]; then
  command -v cc >/dev/null 2>&1 || command -v gcc >/dev/null 2>&1 || MISSING+=("a C compiler")
  command -v pkg-config >/dev/null 2>&1 || MISSING+=("pkg-config")
  command -v curl >/dev/null 2>&1 || command -v wget >/dev/null 2>&1 || MISSING+=("curl")
else
  xcode-select -p >/dev/null 2>&1 || fail "Xcode command-line tools not found" "Run: xcode-select --install"
fi

# Tauri 2 Linux system libraries (pkg-config modules).
if [ "$PLATFORM" = linux ] && command -v pkg-config >/dev/null 2>&1; then
  LIB_MISSING=()
  for mod in webkit2gtk-4.1 javascriptcoregtk-4.1 libsoup-3.0 gtk+-3.0 librsvg-2.0; do
    pkg-config --exists "$mod" 2>/dev/null || LIB_MISSING+=("$mod")
  done
  if ! pkg-config --exists ayatana-appindicator3-0.1 2>/dev/null \
     && ! pkg-config --exists appindicator3-0.1 2>/dev/null; then
    LIB_MISSING+=("ayatana-appindicator3-0.1")
  fi
  if [ ${#LIB_MISSING[@]} -gt 0 ]; then
    MISSING+=("Tauri system libraries (${LIB_MISSING[*]})")
  else
    info "tauri system libraries: found"
  fi
fi

if [ ${#MISSING[@]} -gt 0 ]; then
  HINT=()
  if [ "$PLATFORM" = linux ]; then
    ID_LIKE_ALL=" $( (. /etc/os-release 2>/dev/null; echo "${ID:-} ${ID_LIKE:-}") ) "
    case "$ID_LIKE_ALL" in
      *" debian "*|*" ubuntu "*)
        HINT=("sudo apt-get update && sudo apt-get install -y git curl build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev") ;;
      *" fedora "*|*" rhel "*)
        HINT=("sudo dnf install -y git curl gcc pkgconf-pkg-config webkit2gtk4.1-devel gtk3-devel libsoup3-devel libappindicator-gtk3-devel librsvg2-devel openssl-devel") ;;
      *" arch "*)
        HINT=("sudo pacman -S --needed git curl base-devel pkgconf webkit2gtk-4.1 gtk3 libsoup3 libappindicator-gtk3 librsvg openssl") ;;
      *" suse "*|*" opensuse"*)
        HINT=("sudo zypper install git curl gcc pkg-config webkit2gtk3-devel gtk3-devel libsoup-devel libappindicator3-devel librsvg-devel libopenssl-devel") ;;
      *) HINT=("Install the equivalents of: git, curl, a C compiler, pkg-config, webkit2gtk-4.1, gtk3, libsoup3, ayatana-appindicator3, librsvg (dev packages).") ;;
    esac
  fi
  fail "missing prerequisites: ${MISSING[*]}" \
    "This script never runs sudo. Install them, then re-run it." \
    ${HINT[@]+"${HINT[@]}"}
fi
info "all prerequisites present"

# ---------------------------------------------------------------- 3. source
step "Locating the source checkout"
SELF_DIR=""
if [ -n "${BASH_SOURCE[0]:-}" ] && [ -f "${BASH_SOURCE[0]}" ]; then
  SELF_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." 2>/dev/null && pwd -P || true)"
fi
if [ -z "$DIR" ] && [ -n "$SELF_DIR" ] && [ -f "$SELF_DIR/apps/desktop/src-tauri/tauri.conf.json" ]; then
  DIR="$SELF_DIR"
  info "running from an existing checkout, reusing it: $DIR"
else
  DIR="${DIR:-$HOME/.local/share/eak/src}"
  if [ -d "$DIR/.git" ]; then
    info "reusing checkout: $DIR"
    git -C "$DIR" fetch --quiet origin "$REF" || fail "git fetch failed in $DIR" "Check your network, or remove $DIR to re-clone."
    if [ -n "$(git -C "$DIR" status --porcelain)" ]; then
      info "checkout has local changes; leaving it as is (no update)"
    else
      git -C "$DIR" checkout --quiet -B "$REF" FETCH_HEAD || fail "could not check out $REF in $DIR"
    fi
  elif [ -e "$DIR" ] && [ -n "$(ls -A "$DIR" 2>/dev/null)" ]; then
    fail "$DIR exists and is not a git checkout" "Choose another location with --dir, or remove it."
  else
    info "cloning $REPO_URL ($REF) into $DIR"
    mkdir -p "$(dirname "$DIR")"
    git clone --quiet --branch "$REF" "$REPO_URL" "$DIR" \
      || fail "git clone failed" "URL: $REPO_URL" "If the repository is private, authenticate git first (e.g. gh auth login)."
  fi
fi
[ -f "$DIR/apps/desktop/src-tauri/tauri.conf.json" ] || fail "$DIR does not look like an EAK checkout"

# ---------------------------------------------------------------- 4. build
step "Building the desktop app (first build takes several minutes)"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
TARGET_DIR="${CARGO_TARGET_DIR:-$DIR/target}"
export CARGO_TARGET_DIR="$TARGET_DIR"
info "cargo jobs: $CARGO_BUILD_JOBS, target dir: $TARGET_DIR"
info "using 'tauri build --no-bundle': a release binary with the embedded frontend; installers are not produced"
( cd "$DIR/apps/desktop" && npm ci --no-audit --no-fund ) \
  || fail "npm ci failed in $DIR/apps/desktop" "Check your network and Node/npm versions."
( cd "$DIR/apps/desktop" && npm run tauri -- build --no-bundle ) \
  || fail "tauri build failed" "Scroll up for the first error. Common causes: missing system libraries, low RAM (try CARGO_BUILD_JOBS=1)."
BIN="$TARGET_DIR/release/eak-desktop"
[ -x "$BIN" ] || fail "build finished but the binary is missing: $BIN"
info "built: $BIN"

# ---------------------------------------------------------------- 5. install
step "Installing"
if [ "$PLATFORM" = linux ]; then
  BIN_DIR="$HOME/.local/bin"
  DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
  APP_DIR="$DATA_DIR/applications"
  ICON_DIR="$DATA_DIR/icons/hicolor/128x128/apps"
  mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"
  install -m 755 "$BIN" "$BIN_DIR/eak-desktop" || fail "could not copy the binary to $BIN_DIR"
  info "binary: $BIN_DIR/eak-desktop"
  ICON_LINE="Icon=utilities-terminal"
  if [ -f "$DIR/apps/desktop/src-tauri/icons/128x128.png" ]; then
    install -m 644 "$DIR/apps/desktop/src-tauri/icons/128x128.png" "$ICON_DIR/eak.png"
    ICON_LINE="Icon=$ICON_DIR/eak.png"
  fi
  cat > "$APP_DIR/eak.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=EAK
Comment=Electronics Agent Kit
Exec=$BIN_DIR/eak-desktop
$ICON_LINE
Terminal=false
Categories=Development;Electronics;
EOF
  if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$APP_DIR" 2>/dev/null || true; fi
  info "launcher: $APP_DIR/eak.desktop"
  case ":$PATH:" in *":$BIN_DIR:"*) ;; *) info "note: $BIN_DIR is not on your PATH; add it to run 'eak-desktop' from a shell." ;; esac
  RUN_BIN="$BIN_DIR/eak-desktop"
else
  APP="$HOME/Applications/EAK.app"
  mkdir -p "$APP/Contents/MacOS"
  install -m 755 "$BIN" "$APP/Contents/MacOS/eak-desktop" || fail "could not create $APP"
  cat > "$APP/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>EAK</string>
<key>CFBundleIdentifier</key><string>dev.eak.desktop</string>
<key>CFBundleExecutable</key><string>eak-desktop</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>0.1.0</string>
</dict></plist>
EOF
  info "app bundle: $APP (unsigned; on first open use right-click > Open)"
  RUN_BIN="$APP/Contents/MacOS/eak-desktop"
fi

# ---------------------------------------------------------------- 6. launch
if [ "$LAUNCH" -eq 1 ]; then
  step "Starting EAK"
  if [ "$PLATFORM" = linux ] && [ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
    info "no display detected (DISPLAY/WAYLAND_DISPLAY unset); skipping launch. Start it from a desktop session."
  else
    nohup "$RUN_BIN" >/dev/null 2>&1 &
    PID=$!
    sleep 3
    if kill -0 "$PID" 2>/dev/null; then
      info "EAK is running (pid $PID)"
    else
      fail "EAK exited immediately" "Run it in a terminal to see the error: $RUN_BIN"
    fi
  fi
else
  step "Skipping launch (--no-launch)"
fi

printf '\nDone. Launch EAK from your applications menu, or run: %s\n' "$RUN_BIN"
printf 'To add a model API key: open EAK, click "Model settings", choose a provider, enter the key (stored in the OS keyring).\n'
