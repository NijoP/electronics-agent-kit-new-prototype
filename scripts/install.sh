#!/usr/bin/env bash
# EAK installer for Linux and macOS: downloads a prebuilt release, verifies SHA-256, installs it,
# checks that the app starts. No Rust/Node/build tools needed.
#
#   curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.sh | bash
#   curl -fsSL .../install.sh | bash -s -- --uninstall
#
# Environment (all optional): EAK_VERSION=v0.2.0 (default: newest release incl. prereleases),
#   EAK_FORMAT=deb (Linux: system-wide .deb via sudo instead of the per-user AppImage),
#   EAK_NO_LAUNCH=1, EAK_UNINSTALL=1, EAK_REPO=owner/name,
#   EAK_ASSET_DIR=<dir> (install from local assets + SHA256SUMS instead of downloading; used by CI).
# Never runs sudo implicitly; never touches your projects or settings. Log: ~/.local/state/eak/install.log
set -euo pipefail

REPO="${EAK_REPO:-NijoP/electronics-agent-kit-new-prototype}"
LOG_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/eak"
LOG="$LOG_DIR/install.log"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
APP_ROOT="$DATA/eak"
BIN_DIR="$HOME/.local/bin"
UNINSTALL="${EAK_UNINSTALL:-0}"
for a in "$@"; do
  case "$a" in
    --uninstall) UNINSTALL=1 ;;
    --no-launch) EAK_NO_LAUNCH=1 ;;
    -h|--help) sed -n '2,12p' "${BASH_SOURCE[0]:-/dev/null}" 2>/dev/null || true; exit 0 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done

mkdir -p "$LOG_DIR"
exec > >(tee -a "$LOG") 2>&1
echo "=== EAK install $(date -u +%FT%TZ) ==="
STEP=0
step() { STEP=$((STEP + 1)); printf '\n==> [%s] %s\n' "$STEP" "$*"; }
info() { printf '    %s\n' "$*"; }
RB_DIR=""; RB_OLD=""; RB_VERSION=""
# Undo a half-finished upgrade: put the previous app directory back (only set once the swap happened).
rollback() {
  [ -n "$RB_DIR" ] || return 0
  rm -rf "$RB_DIR"
  if [ -d "$RB_OLD" ]; then
    mv "$RB_OLD" "$RB_DIR"
    echo "$RB_VERSION" > "$APP_ROOT/VERSION"
    printf '       Rolled back to the previous installation (%s).\n' "${RB_VERSION:-unknown version}" >&2
  else
    rm -f "$APP_ROOT/VERSION"   # fresh install that failed its launch check: leave nothing half-installed
  fi
  RB_DIR=""
}
fail() {
  printf '\nERROR at step [%s]: %s\n' "$STEP" "$1" >&2
  shift; for l in "$@"; do printf '       %s\n' "$l" >&2; done
  rollback
  printf '       Log file: %s\n' "$LOG" >&2
  exit 1
}
killapp() { pkill -f "$1" 2>/dev/null || true; }

# ------------------------------------------------------------------ platform
step "Detecting platform"
case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  *) fail "unsupported OS '$(uname -s)'" "On Windows run: irm https://raw.githubusercontent.com/$REPO/main/scripts/install.ps1 | iex" ;;
esac
case "$(uname -m)" in
  x86_64|amd64) ARCH=x86_64 ;;
  aarch64|arm64) ARCH=aarch64 ;;
  *) fail "unsupported CPU architecture '$(uname -m)'" "Prebuilt EAK packages exist for x86_64 (Linux, macOS) and aarch64 (macOS)." ;;
esac
if [ "$OS" = macos ] && [ "$ARCH" = x86_64 ] && [ "$(sysctl -in sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
  ARCH=aarch64; info "running under Rosetta; using the native Apple Silicon build"
fi
if [ "$OS" = linux ] && [ "$ARCH" != x86_64 ]; then
  fail "no prebuilt Linux $ARCH package yet" "Only linux-x86_64 is published. See docs/PLATFORM_SUPPORT.md."
fi
info "platform: $OS-$ARCH"
FORMAT=""; [ "$OS" = linux ] && FORMAT="${EAK_FORMAT:-appimage}"

# ------------------------------------------------------------------ uninstall
if [ "$UNINSTALL" = 1 ]; then
  step "Uninstalling EAK (your projects and settings are not touched)"
  if [ "$OS" = linux ]; then
    killapp "$APP_ROOT/app/"
    rm -rf "$APP_ROOT/app" "$APP_ROOT/VERSION" "$BIN_DIR/eak" \
      "$DATA/applications/eak.desktop" "$DATA/icons/hicolor/128x128/apps/eak.png"
    if dpkg -s eak >/dev/null 2>&1; then info "a system .deb is also installed: sudo apt-get remove eak"; fi
  else
    killapp "EAK.app/Contents/MacOS"
    rm -rf /Applications/EAK.app "$HOME/Applications/EAK.app" 2>/dev/null \
      || info "could not remove /Applications/EAK.app; run: sudo rm -rf /Applications/EAK.app"
    rm -f "$APP_ROOT/VERSION"
  fi
  info "EAK removed. User project folders were left untouched."
  exit 0
fi

# ------------------------------------------------------------------ prerequisites
step "Checking prerequisites"
MISSING=()
if [ -z "${EAK_ASSET_DIR:-}" ]; then command -v curl >/dev/null 2>&1 || MISSING+=("curl"); fi
SHA=()
if command -v sha256sum >/dev/null 2>&1; then SHA=(sha256sum); elif command -v shasum >/dev/null 2>&1; then SHA=(shasum -a 256); else MISSING+=("sha256sum or shasum"); fi
if [ "$OS" = linux ]; then
  GLIBC="$(ldd --version 2>&1 | head -n1 | grep -oE '[0-9]+\.[0-9]+$' || echo 0)"
  info "glibc: $GLIBC (packages are built on Ubuntu 22.04, glibc 2.35)"
  [ "$(printf '%s\n2.35\n' "$GLIBC" | sort -V | head -n1)" = "2.35" ] \
    || fail "glibc $GLIBC is older than 2.35" "Use Ubuntu 22.04+/Debian 12+/Fedora 36+ or equivalent."
else
  command -v hdiutil >/dev/null 2>&1 || MISSING+=("hdiutil (macOS)")
  command -v ditto >/dev/null 2>&1 || MISSING+=("ditto (macOS)")
fi
[ ${#MISSING[@]} -eq 0 ] || fail "missing prerequisites: ${MISSING[*]}" "Install them with your package manager and re-run."
info "prerequisites present"

# ------------------------------------------------------------------ resolve release
step "Resolving release"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
case "$OS-$FORMAT" in
  linux-deb) ASSET="eak-linux-$ARCH.deb" ;;
  linux-*) ASSET="eak-linux-$ARCH.AppImage" ;;
  *) ASSET="eak-macos-$ARCH.dmg" ;;
esac
if [ -n "${EAK_ASSET_DIR:-}" ]; then
  TAG="local"; SRC="$EAK_ASSET_DIR"; info "using local assets in $SRC"
else
  TAG="${EAK_VERSION:-}"
  if [ -z "$TAG" ]; then
    TAG="$(curl -fsSL --proto '=https' --tlsv1.2 "https://api.github.com/repos/$REPO/releases?per_page=10" \
      | grep -m1 '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')" \
      || fail "could not query GitHub releases" "Check your network, or set EAK_VERSION=vX.Y.Z."
    [ -n "$TAG" ] || fail "no published EAK release found in $REPO" "Releases appear after a v* tag is pushed; see docs/INSTALL.md."
  fi
  SRC="https://github.com/$REPO/releases/download/$TAG"
fi
info "release: $TAG, asset: $ASSET"
OLD="$(cat "$APP_ROOT/VERSION" 2>/dev/null || true)"
if [ -n "$OLD" ]; then info "currently installed: $OLD (upgrading/reinstalling to $TAG)"; fi

# ------------------------------------------------------------------ download + verify
step "Downloading and verifying (SHA-256)"
fetch() {
  if [ -n "${EAK_ASSET_DIR:-}" ]; then cp "$SRC/$1" "$TMP/$1"
  else curl -fsSL --proto '=https' --tlsv1.2 --retry 3 -o "$TMP/$1" "$SRC/$1"; fi
}
fetch SHA256SUMS || fail "release $TAG has no SHA256SUMS ($SRC/SHA256SUMS is missing)" "Nothing was installed; the release may be incomplete."
# Releases up to v0.1.4 name Linux packages with the Tauri bundler's pattern (EAK_<version>_amd64.*).
if [ "$OS" = linux ] && [ "$ARCH" = x86_64 ] && ! grep -qE "[ *]$ASSET\$" "$TMP/SHA256SUMS"; then
  ALT="EAK_${TAG#v}_amd64.${ASSET##*.}"
  if grep -qE "[ *]$ALT\$" "$TMP/SHA256SUMS"; then info "using legacy asset name $ALT"; ASSET="$ALT"; fi
fi
fetch "$ASSET" || fail "release $TAG has no asset named $ASSET for $OS-$ARCH" "Missing artifact: $SRC/$ASSET" "See docs/PLATFORM_SUPPORT.md; nothing was installed."
WANT="$(grep -E "[ *]$ASSET\$" "$TMP/SHA256SUMS" | head -n1 | awk '{print $1}')"
[ -n "$WANT" ] || fail "release $TAG has no checksum entry for $ASSET" "Missing artifact: $ASSET"
GOT="$("${SHA[@]}" "$TMP/$ASSET" | awk '{print $1}')"
[ "$GOT" = "$WANT" ] || fail "SHA-256 mismatch for $ASSET" "expected $WANT" "got      $GOT" "The download is corrupt or tampered with; nothing was installed."
info "sha256 OK: $GOT"

# ------------------------------------------------------------------ install
step "Installing"
mkdir -p "$BIN_DIR" "$APP_ROOT"
if [ "$FORMAT" = deb ]; then
  if [ "$(id -u)" = 0 ]; then SUDO=(); elif sudo -n true 2>/dev/null; then SUDO=(sudo); else
    fail "installing the .deb needs root" "Run: sudo apt-get install -y ./$ASSET   (or omit EAK_FORMAT=deb for a per-user install)"; fi
  chmod 755 "$TMP"   # let apt's sandbox user read the verified package (no predictable /tmp path)
  "${SUDO[@]}" apt-get install -y "$TMP/$ASSET" || fail "apt-get install failed"
  APP_BIN=/usr/bin/eak-desktop
elif [ "$OS" = linux ]; then
  chmod +x "$TMP/$ASSET"
  ( cd "$TMP" && "./$ASSET" --appimage-extract >/dev/null ) || fail "could not unpack the AppImage"
  [ -d "$TMP/squashfs-root" ] || fail "AppImage unpack produced no files"
  rm -rf "$APP_ROOT/app.new" "$APP_ROOT/app.old"
  mv "$TMP/squashfs-root" "$APP_ROOT/app.new"
  if [ -d "$APP_ROOT/app" ]; then mv "$APP_ROOT/app" "$APP_ROOT/app.old"; fi
  mv "$APP_ROOT/app.new" "$APP_ROOT/app"
  # app.old is kept until the launch check passes so a failed upgrade can be rolled back.
  RB_DIR="$APP_ROOT/app"; RB_OLD="$APP_ROOT/app.old"; RB_VERSION="$OLD"
  APP_BIN="$BIN_DIR/eak"
  printf '#!/bin/sh\nexec "%s/app/AppRun" "$@"\n' "$APP_ROOT" > "$APP_BIN"; chmod 755 "$APP_BIN"
  ICON="$(find "$APP_ROOT/app" -maxdepth 6 -name '*.png' -path '*128x128*' 2>/dev/null | head -n1)"
  mkdir -p "$DATA/applications" "$DATA/icons/hicolor/128x128/apps"
  if [ -n "$ICON" ]; then cp "$ICON" "$DATA/icons/hicolor/128x128/apps/eak.png"; fi
  cat > "$DATA/applications/eak.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=EAK
Comment=Electronics Agent Kit
Exec=$APP_BIN
Icon=$DATA/icons/hicolor/128x128/apps/eak.png
Terminal=false
Categories=Development;Electronics;
DESK
  if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$DATA/applications" 2>/dev/null || true; fi
  info "launcher: $DATA/applications/eak.desktop, command: $APP_BIN"
else
  MNT="$TMP/mnt"; mkdir -p "$MNT"
  hdiutil attach -nobrowse -readonly -quiet -mountpoint "$MNT" "$TMP/$ASSET" || fail "could not mount the DMG"
  SRCAPP="$(find "$MNT" -maxdepth 1 -name '*.app' | head -n1)"
  [ -n "$SRCAPP" ] || { hdiutil detach -quiet "$MNT" || true; fail "no .app inside the DMG"; }
  DEST=/Applications; [ -w "$DEST" ] || { DEST="$HOME/Applications"; mkdir -p "$DEST"; }
  killapp "EAK.app/Contents/MacOS"
  rm -rf "$DEST/EAK.app.new"; ditto "$SRCAPP" "$DEST/EAK.app.new"
  hdiutil detach -quiet "$MNT" || true
  rm -rf "$DEST/EAK.app.old"
  if [ -d "$DEST/EAK.app" ]; then mv "$DEST/EAK.app" "$DEST/EAK.app.old"; fi
  mv "$DEST/EAK.app.new" "$DEST/EAK.app"
  RB_DIR="$DEST/EAK.app"; RB_OLD="$DEST/EAK.app.old"; RB_VERSION="$OLD"
  xattr -dr com.apple.quarantine "$DEST/EAK.app" 2>/dev/null || true
  APP_DIR="$DEST/EAK.app"
  info "installed: $APP_DIR (unsigned build: if macOS blocks it, right-click > Open once)"
fi
echo "$TAG" > "$APP_ROOT/VERSION"

# ------------------------------------------------------------------ verify

if [ "${EAK_NO_LAUNCH:-0}" = 1 ]; then step "Skipping launch check"; else
  step "Launch check (app must stay up for 6 seconds)"
  if [ "$OS" = macos ]; then
    open -n "$APP_DIR" || fail "could not open the app" "Check System Settings > Privacy & Security for a blocked-app notice."
    sleep 6
    pgrep -f "EAK.app/Contents/MacOS/eak-desktop" >/dev/null || fail "EAK exited right after launch"
    killapp "EAK.app/Contents/MacOS"
    info "EAK started and stayed up"
  else
    probe='"$0" & p=$!; sleep 6; if kill -0 $p 2>/dev/null; then kill $p; exit 0; else exit 1; fi'
    RUNNER=()
    if [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then RUNNER=(sh -c "$probe" "$APP_BIN")
    elif command -v xvfb-run >/dev/null 2>&1; then RUNNER=(xvfb-run -a sh -c "$probe" "$APP_BIN"); info "no display: using xvfb"; fi
    if [ ${#RUNNER[@]} -eq 0 ]; then
      info "no display and no xvfb-run: launch NOT verified (install xvfb or run from a desktop session)"
    else
      "${RUNNER[@]}" || fail "EAK exited right after launch" "Run '$APP_BIN' in a terminal to see the error."
      info "EAK started and stayed up"
    fi
  fi
fi

if [ -n "$RB_OLD" ]; then rm -rf "$RB_OLD"; RB_DIR=""; fi   # upgrade confirmed: drop the rollback copy
printf '\nEAK %s installed (%s).\n' "$TAG" "$OS-$ARCH"
if [ "$OS" = linux ]; then printf 'Start it from your applications menu or run: %s\n' "$APP_BIN"
else printf 'Open it from Applications or run: open -a EAK\n'; fi
printf 'Uninstall: curl -fsSL https://raw.githubusercontent.com/%s/main/scripts/install.sh | bash -s -- --uninstall\n' "$REPO"
printf 'Log: %s\n' "$LOG"
