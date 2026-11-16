#!/usr/bin/env bash
# EAK one-command SOURCE installer for Linux and macOS: builds the requested EAK revision (default:
# current `main`) on THIS machine, installs it per-user, registers a launcher and starts it.
# Never substitutes an old prebuilt release; the prebuilt installer is scripts/install.sh
# (EAK_INSTALL_MODE=prebuilt on the root install.sh).
#
#   curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.sh | bash
#   curl -fsSL .../install.sh | bash -s -- --uninstall            (add --clean-cache to also drop build caches)
#   curl -fsSL .../install.sh | EAK_REF=v0.2.0 bash               (pin a tag, branch or full commit SHA)
#
# Options: --uninstall  --clean-cache  --no-launch  --yes  --force  --ref <ref>  -h
# Environment (all optional):
#   EAK_REF=main            tag, branch or 40-hex commit to build; always resolved to a commit SHA
#   EAK_REPO=owner/name     source repository (default NijoP/electronics-agent-kit-new-prototype)
#   EAK_BUILD_JOBS=N        cargo parallelism (default from RAM: <6 GB 1, <12 GB 2, <24 GB 3, else 4)
#   EAK_NO_LAUNCH=1         do not start EAK afterwards (headless machines)
#   EAK_ASSUME_YES=1        pre-approve installing system packages with sudo (otherwise it asks on the terminal)
#   EAK_FORCE=1             rebuild even when this exact commit is already installed
#   EAK_MIN_DISK_GB=N       free-space floor (default 10, 4 when a build cache exists)
#   EAK_ALLOW_LOW_MEMORY=1  attempt the build with under 3 GB of RAM+swap (not recommended)
#   EAK_SKIP_SYSTEM_DEPS=1  never touch system packages (you installed the Tauri prerequisites yourself)
# The first build compiles the entire Rust dependency tree: expect it to take a long time.
# Privileges: sudo is used only to install system packages, only after listing them and asking.
# Touches nothing outside EAK-managed paths; never your projects, settings, ~/.cargo or ~/.rustup contents.
# Log: Linux ~/.local/state/eak/install-source.log, macOS ~/Library/Logs/EAK/install-source.log
set -Eeuo pipefail

REPO="${EAK_REPO:-NijoP/electronics-agent-kit-new-prototype}"
OFFICIAL_REPO="NijoP/electronics-agent-kit-new-prototype"
REF="${EAK_REF:-main}"
UNINSTALL="${EAK_UNINSTALL:-0}"
CLEAN_CACHE="${EAK_CLEAN_CACHE:-0}"
NO_LAUNCH="${EAK_NO_LAUNCH:-0}"
ASSUME_YES="${EAK_ASSUME_YES:-0}"
FORCE="${EAK_FORCE:-0}"
SELF="${BASH_SOURCE[0]:-}"

usage() { if [ -f "$SELF" ]; then sed -n '2,25p' "$SELF"; else echo "See docs/INSTALL.md in the EAK repository."; fi; }
while [ $# -gt 0 ]; do
  case "$1" in
    --uninstall) UNINSTALL=1 ;;
    --clean-cache) CLEAN_CACHE=1 ;;
    --no-launch) NO_LAUNCH=1 ;;
    --yes|-y) ASSUME_YES=1 ;;
    --force) FORCE=1 ;;
    --ref) [ $# -ge 2 ] || { echo "--ref needs a value" >&2; exit 2; }; REF="$2"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1 (try --help)" >&2; exit 2 ;;
  esac
  shift
done

# ------------------------------------------------------------------ input validation (no eval anywhere)
[[ "$REPO" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || { echo "invalid EAK_REPO '$REPO' (want owner/name)" >&2; exit 2; }
[[ "$REF" =~ ^[A-Za-z0-9][A-Za-z0-9._/-]*$ ]] || { echo "invalid EAK_REF '$REF'" >&2; exit 2; }
case "${EAK_BUILD_JOBS:-1}" in ''|*[!0-9]*|0) echo "invalid EAK_BUILD_JOBS (want a positive integer)" >&2; exit 2 ;; esac

# ------------------------------------------------------------------ platform, paths, logging
case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  *) echo "This installer is for Linux and macOS. On Windows run: irm https://raw.githubusercontent.com/$OFFICIAL_REPO/main/install.ps1 | iex" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) ARCH=x86_64 ;;
  aarch64|arm64) ARCH=aarch64 ;;
  *) echo "unsupported CPU architecture '$(uname -m)' (see docs/PLATFORM_SUPPORT.md)" >&2; exit 1 ;;
esac
{ [ -n "${HOME:-}" ] && [ -d "$HOME" ]; } || { echo "HOME is not set to an existing directory" >&2; exit 1; }
BIN_DIR="$HOME/.local/bin"
if [ "$OS" = linux ]; then
  DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
  APP_ROOT="$DATA/eak"
  BUILD_CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/eak/source-build"
  LOG_DIR="${XDG_STATE_HOME:-$HOME/.local/state}/eak"
  INFO="$APP_ROOT/source-install.info"
else
  DATA="$HOME/Library/Application Support"
  APP_ROOT="$HOME/Applications"
  BUILD_CACHE="$HOME/Library/Caches/EAK/source-build"
  LOG_DIR="$HOME/Library/Logs/EAK"
  INFO="$HOME/Library/Application Support/EAK-installer/source-install.info"
fi
LOG="$LOG_DIR/install-source.log"
APP_LOG="$LOG_DIR/eak-desktop.log"
mkdir -p "$LOG_DIR"

# Paths end up inside generated launcher/desktop files: refuse characters that would need escaping.
safe_path() { case "$1" in *[\"\'\$\`\\]*|*$'\n'*) echo "unsupported character in path: $1" >&2; exit 1 ;; esac; }
for p in "$HOME" "$DATA" "$BUILD_CACHE" "$LOG_DIR"; do safe_path "$p"; done

STEP=0; FAILED_PRINTED=0; TMP=""; BUILD_PID=""; LOCK=""
RB_KIND=""; RB_NEW=""; RB_PREV=""
log() { printf '%s %s\n' "$(date -u +%FT%TZ)" "$*" >> "$LOG"; }
step() { STEP=$((STEP + 1)); printf '\n==> [%s] %s\n' "$STEP" "$*"; log "STEP $STEP: $*"; }
info() { printf '    %s\n' "$*"; log "$*"; }

rollback() {
  [ -n "$RB_KIND" ] || return 0
  if [ "$RB_KIND" = linux ]; then
    if [ -n "$RB_PREV" ] && [ -d "$APP_ROOT/$RB_PREV" ]; then
      ln -sfn "$RB_PREV" "$APP_ROOT/current.rb" && mv -T -f "$APP_ROOT/current.rb" "$APP_ROOT/current" || true
      printf '       Rolled back to the previous installation (%s).\n' "$RB_PREV" >&2
    else
      rm -f "$APP_ROOT/current" "$INFO"
    fi
    [ -z "$RB_NEW" ] || rm -rf -- "$RB_NEW"
  else
    if [ -d "$RB_PREV" ]; then
      rm -rf -- "$RB_NEW"
      if mv "$RB_PREV" "$RB_NEW"; then printf '       Rolled back to the previous EAK.app.\n' >&2; fi
    else
      rm -rf -- "$RB_NEW"; rm -f "$INFO"
    fi
  fi
  RB_KIND=""
}
fail() {
  FAILED_PRINTED=1
  printf '\nERROR at step [%s]: %s\n' "$STEP" "$1" >&2
  shift; for l in "$@"; do printf '       %s\n' "$l" >&2; done
  rollback
  printf '       Nothing was reported as installed. Log: %s\n' "$LOG" >&2
  exit 1
}
LAST_CMD=""
on_exit() {
  rc=$?
  if [ -n "$BUILD_PID" ]; then kill "$BUILD_PID" 2>/dev/null || true; fi
  if [ -n "$LOCK" ]; then rm -rf -- "$LOCK" 2>/dev/null || true; fi
  if [ -n "$TMP" ]; then rm -rf -- "$TMP" 2>/dev/null || true; fi
  if [ "$rc" -ne 0 ] && [ "$FAILED_PRINTED" = 0 ]; then
    rollback
    printf '\nERROR: installer stopped unexpectedly at step [%s] (exit %s).\n       Failing command: %s\n       Log: %s\n' "$STEP" "$rc" "${LAST_CMD:-unknown}" "$LOG" >&2
  fi
}
trap 'LAST_CMD="$BASH_COMMAND (line $LINENO)"' ERR
trap on_exit EXIT
trap 'exit 130' INT TERM

log "=== EAK source install $(date -u +%FT%TZ) repo=$REPO ref=$REF os=$OS arch=$ARCH uid=$(id -u) ==="

# run_logged "what" cmd args...: output goes to the log; on failure show the command and the log tail.
run_logged() {
  local what="$1"; shift
  log "RUN ($what): $*"
  local rc=0
  "$@" >> "$LOG" 2>&1 < /dev/null || rc=$?
  if [ "$rc" = 0 ]; then return 0; fi
  printf '\n    FAILED: %s (exit %s)\n    command: %s\n    last log lines:\n' "$what" "$rc" "$*" >&2
  tail -n 25 "$LOG" | sed 's/^/      | /' >&2
  FAILED_PRINTED=1; rollback
  printf '\nERROR at step [%s]: %s failed. Full log: %s\n' "$STEP" "$what" "$LOG" >&2
  exit 1
}
confirm() {   # 0 = yes, 1 = no, 2 = nobody to ask
  if [ "$ASSUME_YES" = 1 ]; then return 0; fi
  if { : < /dev/tty; } 2>/dev/null; then
    printf '%s [y/N] ' "$1" > /dev/tty
    local ans=""; read -r ans < /dev/tty || return 1
    case "$ans" in y|Y|yes|YES) return 0 ;; esac
    return 1
  fi
  return 2
}
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}'
  else return 1; fi
}
curl_get() { curl -fsSL --proto '=https' --tlsv1.2 --retry 3 "$@"; }

# ------------------------------------------------------------------ uninstall / clean-cache
if [ "$UNINSTALL" = 1 ] || [ "$CLEAN_CACHE" = 1 ]; then
  if [ "$UNINSTALL" = 1 ]; then
    step "Uninstalling EAK (source install). Projects, settings and credentials are not touched"
    if [ "$OS" = linux ]; then
      rm -rf -- "$APP_ROOT/versions" "$APP_ROOT/current" "$APP_ROOT/current.new" "$APP_ROOT/current.rb" "$INFO"
      if [ -f "$BIN_DIR/eak-desktop" ] && grep -q '^# eak-source-install' "$BIN_DIR/eak-desktop" 2>/dev/null; then rm -f "$BIN_DIR/eak-desktop"; fi
      for b in eak eak-mcp; do
        if [ -f "$BIN_DIR/$b" ] && grep -q '^# eak-source-install' "$BIN_DIR/$b" 2>/dev/null; then rm -f "$BIN_DIR/$b"; fi
      done
      if [ -f "$DATA/applications/eak.desktop" ] && grep -q '^X-EAK-Install=source' "$DATA/applications/eak.desktop" 2>/dev/null; then
        rm -f "$DATA/applications/eak.desktop" "$DATA/icons/hicolor/128x128/apps/eak.png"
      fi
      rmdir "$APP_ROOT" 2>/dev/null || true
      info "removed the EAK-managed app files, launcher and menu entry (if they were created by this installer)"
      if [ -d "$APP_ROOT/app" ]; then info "a prebuilt install is still present: EAK_INSTALL_MODE=prebuilt ... --uninstall removes it"; fi
      info "quit any running EAK window yourself; it keeps working until closed"
    else
      APP="$APP_ROOT/EAK.app"
      if [ -d "$APP" ] && grep -q 'dev.eak.desktop' "$APP/Contents/Info.plist" 2>/dev/null; then rm -rf -- "$APP"; info "removed $APP"
      else info "no EAK.app from this installer at $APP"; fi
      rm -rf -- "$APP_ROOT/cli"
      for b in eak eak-mcp; do
        if [ -f "$BIN_DIR/$b" ] && grep -q '^# eak-source-install' "$BIN_DIR/$b" 2>/dev/null; then rm -f "$BIN_DIR/$b"; fi
      done
      rm -rf -- "$HOME/Library/Application Support/EAK-installer"
    fi
  fi
  if [ "$CLEAN_CACHE" = 1 ]; then
    step "Removing EAK build caches"
    case "$BUILD_CACHE" in
      */source-build) rm -rf -- "$BUILD_CACHE"; info "removed $BUILD_CACHE (source, build output, npm cache, user-local Node)" ;;
      *) fail "refusing to remove unexpected cache path $BUILD_CACHE" ;;
    esac
    info "left alone: ~/.cargo, ~/.rustup and your projects"
  fi
  printf '\nDone. Log: %s\n' "$LOG"
  exit 0
fi

# ------------------------------------------------------------------ 1. platform
step "Detecting operating system and architecture"
info "$OS-$ARCH ($(uname -sr))"
if [ "$OS" = linux ] && [ "$ARCH" = aarch64 ]; then info "note: Linux aarch64 is not verified (docs/PLATFORM_SUPPORT.md); continuing"; fi
if [ "$OS" = macos ] && [ "$(sysctl -in sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
  info "note: this shell runs under Rosetta, so an Intel build will be made; open a native Apple Silicon terminal for an arm64 build"
fi
command -v curl >/dev/null 2>&1 || fail "curl is required" "Install curl with your package manager and re-run."
command -v tar >/dev/null 2>&1 || fail "tar is required"
sha256_of /dev/null >/dev/null 2>&1 || fail "sha256sum or shasum is required"

# ------------------------------------------------------------------ 2. resources
step "Checking disk space and memory"
mkdir -p "$BUILD_CACHE"
if [ "$OS" = linux ]; then
  MEM_KB="$(awk '/^MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)"
  SWAP_KB="$(awk '/^SwapTotal:/ {print $2}' /proc/meminfo 2>/dev/null || echo 0)"
else
  MEM_KB="$(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1024 ))"; SWAP_KB=0
fi
MEM_GB=$(( ${MEM_KB:-0} / 1048576 ))
if [ -n "${EAK_BUILD_JOBS:-}" ]; then JOBS="$EAK_BUILD_JOBS"
elif [ "$MEM_GB" -lt 6 ]; then JOBS=1; elif [ "$MEM_GB" -lt 12 ]; then JOBS=2; elif [ "$MEM_GB" -lt 24 ]; then JOBS=3; else JOBS=4; fi
CPUS="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)"; case "$CPUS" in ''|*[!0-9]*) CPUS=1 ;; esac
if [ -z "${EAK_BUILD_JOBS:-}" ] && [ "$JOBS" -gt "$CPUS" ]; then JOBS="$CPUS"; fi
info "memory: about ${MEM_GB} GB RAM, $(( ${SWAP_KB:-0} / 1048576 )) GB swap, $CPUS CPU threads -> $JOBS parallel build job(s)"
if [ $(( ${MEM_KB:-0} + ${SWAP_KB:-0} )) -lt 3145728 ] && [ "${EAK_ALLOW_LOW_MEMORY:-0}" != 1 ]; then
  fail "under 3 GB of RAM+swap: the Rust build is likely to run out of memory" \
       "Add swap space or use a bigger machine; or set EAK_ALLOW_LOW_MEMORY=1 EAK_BUILD_JOBS=1 to try anyway."
fi
if [ -d "$BUILD_CACHE/target" ]; then NEED_GB="${EAK_MIN_DISK_GB:-4}"; else NEED_GB="${EAK_MIN_DISK_GB:-10}"; fi
FREE_GB="$(df -Pk "$BUILD_CACHE" | awk 'NR==2 {print int($4/1048576)}')"
info "disk: ${FREE_GB} GB free where builds go ($BUILD_CACHE); estimate needed: ${NEED_GB} GB (toolchain + dependencies + build output, not measured)"
[ "${FREE_GB:-0}" -ge "$NEED_GB" ] || fail "not enough free disk space (${FREE_GB} GB free, ~${NEED_GB} GB needed)" \
  "Free some space yourself (this installer never deletes your files), or set EAK_MIN_DISK_GB to override at your own risk."

# one installer at a time
LOCK="$BUILD_CACHE/lock.d"
if ! mkdir "$LOCK" 2>/dev/null; then
  OLDPID="$(cat "$LOCK/pid" 2>/dev/null || true)"
  if [ -n "$OLDPID" ] && kill -0 "$OLDPID" 2>/dev/null; then LOCK=""; fail "another EAK installer is running (pid $OLDPID)"; fi
  rm -rf -- "$LOCK"; mkdir "$LOCK"
fi
echo $$ > "$LOCK/pid"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/eak-install.XXXXXX")"

# ------------------------------------------------------------------ 3. source revision
step "Resolving the EAK source revision ($REPO @ $REF)"
URL="https://github.com/$REPO.git"
HAVE_GIT=0
if command -v git >/dev/null 2>&1; then HAVE_GIT=1; fi
SHA=""
if [[ "$REF" =~ ^[0-9a-f]{40}$ ]]; then SHA="$REF"
elif [ "$HAVE_GIT" = 1 ]; then
  LS="$(GIT_TERMINAL_PROMPT=0 git ls-remote "$URL" "refs/heads/$REF" "refs/tags/$REF" "refs/tags/$REF^{}" 2>>"$LOG" < /dev/null)" \
    || fail "could not reach $URL" "Check your network connection (source installation needs internet access)."
  SHA="$(printf '%s\n' "$LS" | awk -v r="refs/tags/$REF^{}" '$2==r {print $1}' | head -n1)"
  if [ -z "$SHA" ]; then SHA="$(printf '%s\n' "$LS" | awk -v t="refs/tags/$REF" -v h="refs/heads/$REF" '$2==t || $2==h {print $1}' | head -n1)"; fi
else
  SHA="$(curl_get -H 'Accept: application/vnd.github.sha' "https://api.github.com/repos/$REPO/commits/$REF" 2>>"$LOG")" \
    || fail "could not resolve '$REF' through the GitHub API" "Check the ref name and your network (git is not installed, so the API was used)."
fi
[[ "$SHA" =~ ^[0-9a-f]{40}$ ]] || fail "could not resolve '$REF' to a commit in $REPO" "Use an existing branch, tag, or a full 40-character commit SHA."
info "repository: https://github.com/$REPO"
info "ref: $REF"
info "commit: $SHA"
if [ "$REPO" != "$OFFICIAL_REPO" ]; then info "NOTE: building from a non-default repository ($REPO)"; fi

PREV_COMMIT="$(sed -n 's/^commit=//p' "$INFO" 2>/dev/null | head -n1 || true)"
UP_TO_DATE=0
if [ "$PREV_COMMIT" = "$SHA" ] && [ "$FORCE" != 1 ]; then
  if [ "$OS" = linux ] && [ -x "$APP_ROOT/current/eak-desktop" ]; then UP_TO_DATE=1; fi
  if [ "$OS" = macos ] && [ -d "$APP_ROOT/EAK.app" ]; then UP_TO_DATE=1; fi
fi
if [ "$UP_TO_DATE" = 1 ]; then info "this commit is already installed and intact; skipping rebuild (EAK_FORCE=1 / --force rebuilds)"; fi

# ------------------------------------------------------------------ 4. prerequisites
NODE_DIR="$BUILD_CACHE/node"

native_deps_present() {
  command -v pkg-config >/dev/null 2>&1 || return 1
  { command -v cc >/dev/null 2>&1 || command -v gcc >/dev/null 2>&1; } || return 1
  local m; for m in webkit2gtk-4.1 gtk+-3.0 dbus-1; do pkg-config --exists "$m" 2>/dev/null || return 1; done
}

setup_linux_packages() {
  if [ "${EAK_SKIP_SYSTEM_DEPS:-0}" = 1 ]; then info "EAK_SKIP_SYSTEM_DEPS=1: not touching system packages"; return 0; fi
  local id like pm="" pkgs=() missing=() p
  id="$( . /etc/os-release 2>/dev/null; printf '%s' "${ID:-}" )"
  like="$( . /etc/os-release 2>/dev/null; printf '%s' "${ID_LIKE:-}" )"
  case " $id $like " in
    *" debian "*|*" ubuntu "*) pm=apt ;;
    *" fedora "*|*" rhel "*|*" centos "*) pm=dnf ;;
    *" arch "*) pm=pacman ;;
    *" suse "*|*" opensuse "*) pm=zypper ;;
  esac
  # Package names: Tauri v2 prerequisites page (https://v2.tauri.app/start/prerequisites/) plus the
  # dbus development files the keyring crate links; compilers/pkg-config named explicitly per distro.
  case "$pm" in
    apt) pkgs=(build-essential curl wget file pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libdbus-1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev) ;;
    dnf) pkgs=(gcc gcc-c++ make pkgconf-pkg-config curl wget file webkit2gtk4.1-devel gtk3-devel dbus-devel openssl-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel) ;;
    pacman) pkgs=(base-devel curl wget file webkit2gtk-4.1 gtk3 dbus openssl libappindicator-gtk3 librsvg xdotool) ;;
    zypper) pkgs=(gcc gcc-c++ make pkg-config curl wget file webkit2gtk3-devel gtk3-devel dbus-1-devel libopenssl-devel libappindicator3-1 librsvg-devel) ;;
    *)
      if native_deps_present; then info "unrecognised distribution ('$id'), but the native build libraries are already present"; return 0; fi
      fail "unsupported Linux distribution '${id:-unknown}': EAK cannot install system packages for it" \
           "Install the Tauri v2 Linux prerequisites yourself: https://v2.tauri.app/start/prerequisites/#linux" \
           "(WebKitGTK 4.1 dev files, GTK3, D-Bus dev files, a C compiler, pkg-config), then re-run with EAK_SKIP_SYSTEM_DEPS=1." ;;
  esac
  for p in "${pkgs[@]}"; do
    case "$pm" in
      apt) dpkg-query -W -f='${db:Status-Abbrev}' "$p" 2>/dev/null | grep -q '^ii' || missing+=("$p") ;;
      pacman) pacman -Qq "$p" >/dev/null 2>&1 || pacman -Qg "$p" >/dev/null 2>&1 || missing+=("$p") ;;
      *) rpm -q --whatprovides "$p" >/dev/null 2>&1 || missing+=("$p") ;;
    esac
  done
  if [ ${#missing[@]} -eq 0 ]; then info "system packages ($pm): all required packages are installed"; return 0; fi
  info "missing system packages ($pm): ${missing[*]}"
  local cmds=()
  case "$pm" in
    apt) cmds=("apt-get update" "apt-get install -y ${missing[*]}") ;;
    dnf) cmds=("dnf install -y ${missing[*]}") ;;
    pacman) cmds=("pacman -S --needed --noconfirm ${missing[*]}") ;;
    zypper) cmds=("zypper --non-interactive install ${missing[*]}") ;;
  esac
  local sudo_cmd=""
  if [ "$(id -u)" != 0 ]; then
    command -v sudo >/dev/null 2>&1 || fail "root is needed to install: ${missing[*]}, and sudo is not available" \
      "As root run: ${cmds[*]}" "Then re-run this installer (or set EAK_SKIP_SYSTEM_DEPS=1)."
    sudo_cmd="sudo"
  fi
  printf '\n    EAK needs these system packages to compile. This will run (needs administrator rights):\n'
  local c; for c in "${cmds[@]}"; do printf '      %s %s\n' "$sudo_cmd" "$c"; done
  local rc=0; confirm "    Install them now?" || rc=$?
  if [ "$rc" = 2 ]; then
    fail "system packages are missing and there is no terminal to ask for permission" \
         "Run these yourself, then re-run: ${cmds[*]}" "Or pre-approve with EAK_ASSUME_YES=1 / --yes."
  elif [ "$rc" != 0 ]; then
    fail "system packages not installed (declined)" "Install them yourself: ${cmds[*]}" "Then re-run, or set EAK_SKIP_SYSTEM_DEPS=1 if you are sure they are present."
  fi
  if [ -n "$sudo_cmd" ]; then sudo -v || fail "could not obtain administrator rights with sudo"; fi
  for c in "${cmds[@]}"; do
    # $c is a fixed command plus validated package names; word splitting is intended.
    # shellcheck disable=SC2086
    if [ -n "$sudo_cmd" ]; then run_logged "system packages: $c" sudo $c; else run_logged "system packages: $c" $c; fi
  done
  info "system packages installed"
}

setup_rust() {
  export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
  if command -v rustup >/dev/null 2>&1; then
    info "rustup found; ensuring the stable toolchain (your default toolchain is left unchanged)"
    run_logged "rustup toolchain install stable" rustup toolchain install stable --profile minimal
    export RUSTUP_TOOLCHAIN=stable
  elif command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
    info "using the existing Rust (not managed by rustup); the build checks it against the workspace minimum"
  else
    info "Rust not found: installing rustup (official installer, https://sh.rustup.rs), stable, minimal profile, no shell-profile edits"
    curl_get -o "$TMP/rustup-init.sh" https://sh.rustup.rs || fail "could not download the rustup installer" "Check your network."
    run_logged "rustup install" sh "$TMP/rustup-init.sh" -y --profile minimal --default-toolchain stable --no-modify-path
  fi
  command -v cargo >/dev/null 2>&1 || fail "cargo is still not available after setup" "See $LOG"
  info "$(rustc --version) / $(cargo --version)"
}

node_ok_ver() {   # Vite 8: ^20.19.0 || >=22.12.0
  local maj="${1%%.*}" rest="${1#*.}" min=""; min="${rest%%.*}"
  case "$maj$min" in ''|*[!0-9]*) return 1 ;; esac
  if [ "$maj" -gt 22 ]; then return 0; fi
  if [ "$maj" -eq 22 ] && [ "$min" -ge 12 ]; then return 0; fi
  if [ "$maj" -eq 20 ] && [ "$min" -ge 19 ]; then return 0; fi
  return 1
}

setup_node() {
  local v=""
  if command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1; then
    v="$(node -p 'process.versions.node' 2>/dev/null || true)"
    if node_ok_ver "$v"; then info "reusing Node.js $v and npm $(npm --version)"; return 0; fi
    info "Node.js ${v:-unknown} found but Vite 8 needs 20.19+ or 22.12+"
  fi
  local plat=""
  case "$OS-$ARCH" in
    linux-x86_64) plat=linux-x64 ;; linux-aarch64) plat=linux-arm64 ;;
    macos-x86_64) plat=darwin-x64 ;; macos-aarch64) plat=darwin-arm64 ;;
  esac
  local base="https://nodejs.org/dist/latest-v22.x"
  info "installing a user-local Node.js 22 LTS from nodejs.org into $NODE_DIR (your system Node is not touched)"
  curl_get -o "$TMP/SHASUMS256.txt" "$base/SHASUMS256.txt" || fail "could not download the Node.js checksum list" "Check your network."
  local line file want got
  line="$(grep -E " node-v[0-9.]+-$plat\.tar\.gz\$" "$TMP/SHASUMS256.txt" | head -n1 || true)"
  [ -n "$line" ] || fail "no Node.js 22 build for $plat in the nodejs.org listing"
  want="${line%% *}"; file="${line##* }"
  if [ ! -x "$NODE_DIR/${file%.tar.gz}/bin/node" ]; then
    curl_get -o "$TMP/$file" "$base/$file" || fail "could not download $file"
    got="$(sha256_of "$TMP/$file")"
    [ "$got" = "$want" ] || fail "SHA-256 mismatch for $file" "expected $want" "got      $got" "Nothing was installed."
    mkdir -p "$NODE_DIR"
    tar -xzf "$TMP/$file" -C "$NODE_DIR" || fail "could not unpack $file"
  fi
  export PATH="$NODE_DIR/${file%.tar.gz}/bin:$PATH"
  v="$(node -p 'process.versions.node')"
  node_ok_ver "$v" || fail "installed Node.js $v does not satisfy the Vite requirement"
  info "Node.js $v, npm $(npm --version) (checksum verified against nodejs.org's SHASUMS256.txt over HTTPS)"
}

setup_toolchain() {
  if [ "$OS" = linux ]; then setup_linux_packages; else
    if ! xcode-select -p >/dev/null 2>&1; then
      xcode-select --install >/dev/null 2>&1 || true
      fail "Xcode Command Line Tools are not installed" \
           "A system dialog was opened (xcode-select --install). Finish that install, then run this installer again."
    fi
    info "Xcode Command Line Tools: present"
  fi
  setup_rust
  setup_node
}

if [ "$UP_TO_DATE" = 1 ]; then
  step "Checking build prerequisites"; info "skipped: nothing to build"
else
  step "Checking and installing build prerequisites"
  setup_toolchain
fi

# ------------------------------------------------------------------ 5. source download
SRC="$BUILD_CACHE/src"
KIND=""; ART=""; VERSION=""; CLI_DIR=""
if [ "$UP_TO_DATE" = 0 ]; then
  step "Downloading EAK source at $SHA"
  FETCHED=0
  if [ "$HAVE_GIT" = 1 ]; then
    if [ ! -d "$SRC/.git" ]; then rm -rf -- "$SRC"; git init -q "$SRC" >> "$LOG" 2>&1; fi
    if GIT_TERMINAL_PROMPT=0 git -C "$SRC" fetch -q --depth 1 "$URL" "$SHA" >> "$LOG" 2>&1 < /dev/null \
       && git -C "$SRC" checkout -q --force --detach FETCH_HEAD >> "$LOG" 2>&1; then
      [ "$(git -C "$SRC" rev-parse HEAD)" = "$SHA" ] || fail "checked-out commit does not match $SHA"
      FETCHED=1; info "fetched with git into $SRC (cache reused on updates)"
    else
      info "git fetch of that commit failed; falling back to the GitHub source archive"
    fi
  fi
  if [ "$FETCHED" = 0 ]; then
    curl_get -o "$TMP/src.tar.gz" "https://codeload.github.com/$REPO/tar.gz/$SHA" || fail "could not download the source archive for $SHA" "Check your network and that the commit exists in $REPO."
    mkdir "$TMP/src"; tar -xzf "$TMP/src.tar.gz" -C "$TMP/src" --strip-components=1 || fail "could not unpack the source archive"
    case "$SRC" in "$BUILD_CACHE"/src) rm -rf -- "$SRC" ;; *) fail "refusing to replace unexpected path $SRC" ;; esac
    mv "$TMP/src" "$SRC"; info "downloaded the source archive for $SHA into $SRC"
  fi
  for f in apps/desktop/package.json apps/desktop/package-lock.json apps/desktop/src-tauri/tauri.conf.json Cargo.lock scripts/build.sh; do
    [ -f "$SRC/$f" ] || fail "downloaded source is missing $f" "This does not look like an EAK checkout; nothing was built."
  done

  # ---------------------------------------------------------------- 6. build
  step "Building EAK (frontend dependencies, frontend build, Rust/Tauri build)"
  info "this can take a long time on the first run; progress is in the log: tail -f $LOG"
  RESULT="$TMP/build-result.env"
  : > "$RESULT"
  log "RUN: bash scripts/build.sh --result-file (EAK_BUILD_JOBS=$JOBS)"
  # npm cache and cargo target live in EAK's own cache dir; ~/.cargo registry caches are reused.
  EAK_COMMIT="$SHA" EAK_BUILD_JOBS="$JOBS" CARGO_TARGET_DIR="$BUILD_CACHE/target" npm_config_cache="$BUILD_CACHE/npm" \
    bash "$SRC/scripts/build.sh" --result-file "$RESULT" >> "$LOG" 2>&1 < /dev/null &
  BUILD_PID=$!
  T0="$(date +%s)"
  while kill -0 "$BUILD_PID" 2>/dev/null; do
    sleep 20
    N=$(( $(date +%s) - T0 ))
    if [ $(( N % 120 )) -lt 20 ] && kill -0 "$BUILD_PID" 2>/dev/null; then info "still building (${N}s elapsed)"; fi
  done
  BRC=0; wait "$BUILD_PID" || BRC=$?
  BUILD_PID=""
  if [ "$BRC" != 0 ]; then
    printf '\n    build failed (exit %s). Last log lines:\n' "$BRC" >&2
    tail -n 30 "$LOG" | sed 's/^/      | /' >&2
    fail "the EAK build failed" "Command: bash scripts/build.sh (in $SRC)" \
         "Common causes: out of memory (retry with EAK_BUILD_JOBS=1), missing system libraries, no network. See docs/INSTALL.md#troubleshooting"
  fi
  KIND="$(sed -n 's/^kind=//p' "$RESULT")"; ART="$(sed -n 's/^path=//p' "$RESULT")"; VERSION="$(sed -n 's/^version=//p' "$RESULT")"; CLI_DIR="$(sed -n 's/^cli_dir=//p' "$RESULT")"
  { [ -n "$KIND" ] && [ -e "$ART" ]; } || fail "the build did not report a usable artifact" "See $LOG"
  info "built EAK $VERSION: $ART"
else
  VERSION="$(sed -n 's/^version=//p' "$INFO" 2>/dev/null | head -n1 || true)"; VERSION="${VERSION:-unknown}"
fi

# ------------------------------------------------------------------ 7. staged install
APP_BIN=""; LAUNCHER=""; APP_DIR=""
write_info() {
  mkdir -p "$(dirname "$INFO")"
  {
    printf 'repo=%s\nref=%s\ncommit=%s\nversion=%s\nos=%s\narch=%s\n' "$REPO" "$REF" "$SHA" "$VERSION" "$OS" "$ARCH"
    printf 'installed_at=%s\nlog=%s\n' "$(date -u +%FT%TZ)" "$LOG"
    printf 'cli=%s\n' "$( [ -n "$CLI_DIR" ] && echo "eak eak-mcp" || echo none )"
    printf 'rustc=%s\nnode=%s\n' "$(rustc --version 2>/dev/null || echo unknown)" "$(node --version 2>/dev/null || echo unknown)"
  } > "$INFO.tmp" && mv "$INFO.tmp" "$INFO"
}

# EAK CLI + MCP harness (built from the same commit): copied into $1, launchers on PATH in $BIN_DIR.
# Terminal sessions in the app find them next to eak-desktop (or via these launchers).
install_cli() {
  local dest="$1" b
  [ -n "$CLI_DIR" ] || fail "the build did not report the eak harness directory" "See $LOG"
  mkdir -p "$BIN_DIR"
  for b in eak eak-mcp; do
    install -m 755 "$CLI_DIR/$b" "$dest/$b" || fail "could not install $b"
    [ -s "$dest/$b" ] || fail "staged $b is empty"
  done
}
link_cli() {
  local dir="$1" b
  for b in eak eak-mcp; do
    printf '#!/bin/sh\n# eak-source-install: managed by the EAK source installer\nexec "%s" "$@"\n' "$dir/$b" > "$BIN_DIR/$b.tmp"
    chmod 755 "$BIN_DIR/$b.tmp"; mv "$BIN_DIR/$b.tmp" "$BIN_DIR/$b"
  done
  info "CLI: $BIN_DIR/eak and $BIN_DIR/eak-mcp"
}

install_linux() {
  step "Installing into $APP_ROOT (previous version is kept until the new one starts)"
  local ts short vname stage prev
  ts="$(date +%Y%m%d%H%M%S)"; short="${SHA:0:12}"; vname="$short-$ts"
  mkdir -p "$APP_ROOT/versions" "$BIN_DIR" "$DATA/applications" "$DATA/icons/hicolor/128x128/apps"
  stage="$APP_ROOT/versions/.stage-$$"
  rm -rf -- "$stage"; mkdir "$stage"
  install -m 755 "$ART" "$stage/eak-desktop"
  [ -s "$stage/eak-desktop" ] || fail "staged executable is empty"
  [ "$(head -c4 "$stage/eak-desktop" | od -An -tx1 | tr -d ' \n')" = 7f454c46 ] || fail "staged file is not a Linux executable"
  install_cli "$stage"   # eak + eak-mcp next to eak-desktop; link_cli below points the launchers here
  prev="$(readlink "$APP_ROOT/current" 2>/dev/null || true)"
  mv "$stage" "$APP_ROOT/versions/$vname"
  RB_KIND=linux; RB_NEW="$APP_ROOT/versions/$vname"; RB_PREV="$prev"
  ln -sfn "versions/$vname" "$APP_ROOT/current.new"
  mv -T "$APP_ROOT/current.new" "$APP_ROOT/current"
  APP_BIN="$APP_ROOT/current/eak-desktop"; LAUNCHER="$BIN_DIR/eak-desktop"
  printf '#!/bin/sh\n# eak-source-install: managed by the EAK source installer\nexec "%s" "$@"\n' "$APP_BIN" > "$LAUNCHER.tmp"
  chmod 755 "$LAUNCHER.tmp"; mv "$LAUNCHER.tmp" "$LAUNCHER"
  link_cli "$APP_ROOT/current"
  cp "$SRC/apps/desktop/src-tauri/icons/128x128.png" "$DATA/icons/hicolor/128x128/apps/eak.png" 2>/dev/null \
    || info "warning: could not copy the application icon"
  cat > "$DATA/applications/eak.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=EAK
Comment=Electronics Agent Kit
Exec="$LAUNCHER"
Icon=$DATA/icons/hicolor/128x128/apps/eak.png
Terminal=false
Categories=Development;Electronics;
X-EAK-Install=source
DESK
  if command -v desktop-file-validate >/dev/null 2>&1; then desktop-file-validate "$DATA/applications/eak.desktop" >> "$LOG" 2>&1 || info "warning: desktop-file-validate reported issues (see log)"; fi
  if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$DATA/applications" >> "$LOG" 2>&1 || true; fi
  info "app: $APP_BIN"; info "command: $LAUNCHER"; info "menu entry: $DATA/applications/eak.desktop"
  case ":$PATH:" in *":$BIN_DIR:"*) ;; *) info "note: $BIN_DIR is not on your PATH; use the menu entry or the full path" ;; esac
}

install_macos() {
  step "Installing into $APP_ROOT/EAK.app (previous version is kept until the new one starts)"
  local dest="$APP_ROOT/EAK.app"
  mkdir -p "$APP_ROOT"
  rm -rf -- "$APP_ROOT/.EAK.app.new" "$APP_ROOT/.EAK.app.old"
  ditto "$ART" "$APP_ROOT/.EAK.app.new" || fail "could not copy the app bundle"
  { [ -s "$APP_ROOT/.EAK.app.new/Contents/MacOS/eak-desktop" ] && [ -f "$APP_ROOT/.EAK.app.new/Contents/Info.plist" ]; } || fail "staged app bundle is incomplete"
  # Local ad-hoc signature only (no developer identity, not notarized): lets Apple Silicon run the locally built app.
  codesign --force --deep --sign - "$APP_ROOT/.EAK.app.new" >> "$LOG" 2>&1 || info "note: ad-hoc codesign failed; macOS may refuse to open the app (see log)"
  if [ -d "$dest" ]; then mv "$dest" "$APP_ROOT/.EAK.app.old"; fi
  mv "$APP_ROOT/.EAK.app.new" "$dest"
  RB_KIND=macos; RB_NEW="$dest"; RB_PREV="$APP_ROOT/.EAK.app.old"
  APP_DIR="$dest"; LAUNCHER="$dest"
  rm -rf -- "$APP_ROOT/cli"; mkdir -p "$APP_ROOT/cli"; install_cli "$APP_ROOT/cli"; link_cli "$APP_ROOT/cli"
  info "installed: $dest (not developer-signed, not notarized: built locally on this Mac)"
}

launch_check() {
  if [ "$NO_LAUNCH" = 1 ]; then step "Launch skipped (EAK_NO_LAUNCH=1)"; info "launch NOT verified"; return 0; fi
  if [ "$OS" = linux ]; then
    if [ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
      step "Launch skipped"; info "no graphical session detected; launch NOT verified. Start EAK later from the menu or with: $LAUNCHER"; return 0
    fi
    step "Launching EAK (it must stay up for 6 seconds)"
    nohup "$LAUNCHER" >> "$APP_LOG" 2>&1 < /dev/null &
    local pid=$!
    sleep 6
    kill -0 "$pid" 2>/dev/null || fail "EAK exited right after launch" "Run '$LAUNCHER' in a terminal to see the error; app log: $APP_LOG" \
      "Missing WebKitGTK runtime libraries are the usual cause."
    info "EAK started and stayed up"
  else
    step "Launching EAK (it must stay up for 6 seconds)"
    open -n "$APP_DIR" || fail "could not open the app" "Check System Settings > Privacy & Security for a blocked-app notice."
    sleep 6
    pgrep -f "EAK.app/Contents/MacOS/eak-desktop" >/dev/null || fail "EAK exited right after launch"
    info "EAK started and stayed up"
  fi
}

if [ "$UP_TO_DATE" = 1 ]; then
  step "Installation"; info "unchanged: $SHA"
  if [ "$OS" = linux ]; then APP_BIN="$APP_ROOT/current/eak-desktop"; LAUNCHER="$BIN_DIR/eak-desktop"; else APP_DIR="$APP_ROOT/EAK.app"; LAUNCHER="$APP_DIR"; fi
else
  if [ "$OS" = linux ]; then install_linux; else install_macos; fi
fi
launch_check

# Upgrade confirmed: finalize and drop old copies (keep the immediately previous Linux version for manual rollback).
if [ "$UP_TO_DATE" = 0 ]; then
  write_info
  if [ "$OS" = linux ]; then
    KEEP_NEW="$(basename "$RB_NEW")"; KEEP_PREV="$(basename "${RB_PREV:-none}")"
    for d in "$APP_ROOT"/versions/*; do
      [ -d "$d" ] || continue
      case "$(basename "$d")" in "$KEEP_NEW"|"$KEEP_PREV") ;; *) rm -rf -- "$d" ;; esac
    done
  else
    rm -rf -- "$APP_ROOT/.EAK.app.old"
  fi
  RB_KIND=""
fi

# ------------------------------------------------------------------ 8. report (only reached on success)
step "Done"
printf '\nEAK %s installed from source.\n' "$VERSION"
printf '  source commit : %s (%s @ %s)\n' "$SHA" "$REPO" "$REF"
printf '  platform      : %s-%s\n' "$OS" "$ARCH"
if [ "$OS" = linux ]; then printf '  install dir   : %s\n' "$APP_ROOT/current -> $(readlink "$APP_ROOT/current")"; else printf '  install dir   : %s\n' "$APP_DIR"; fi
printf '  launcher      : %s\n' "$LAUNCHER"
printf '  build log     : %s\n' "$LOG"
printf '  update        : curl -fsSL https://raw.githubusercontent.com/%s/main/install.sh | bash\n' "$OFFICIAL_REPO"
printf '  uninstall     : curl -fsSL https://raw.githubusercontent.com/%s/main/install.sh | bash -s -- --uninstall\n' "$OFFICIAL_REPO"
printf '  (installer code-reviewed only; see STATUS.md for what is verified)\n'
