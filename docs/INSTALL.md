# Installing EAK

**Status: code-reviewed only.** The installers below were written and statically checked (`bash -n`); they have
never been run on real hardware, and no build has been performed under them. Treat the first run as a test and
report what breaks. Per-platform state: [PLATFORM_SUPPORT.md](PLATFORM_SUPPORT.md).

## What the default command does

The default install **builds the current source on your own computer**. It never installs an old prebuilt
package in place of `main`.

| OS | One command |
|---|---|
| Linux (Debian/Ubuntu first) and macOS | `curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.sh \| bash` |
| Windows 10/11 x64 (PowerShell) | `irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.ps1 \| iex` |

You need network access (source, crates, npm packages, toolchains) and patience: the **first build compiles the
whole Rust dependency tree and takes a long time**, much longer than installing a package. Updates reuse caches.

Steps (the same on every OS; the logs name each one):
1. Detect OS and CPU; check disk (about 10 GB free on Linux/macOS, 12 GB on Windows; an estimate, not measured) and
   memory (stops under 3 GB RAM+swap unless `EAK_ALLOW_LOW_MEMORY=1`).
2. Resolve `EAK_REF` (default `main`; tag, branch or full SHA) to an exact commit SHA and print it.
3. Provision prerequisites (below), asking before anything needs administrator rights.
4. Download that commit (git if present, otherwise the GitHub source archive; HTTPS only) into the build cache.
5. Run the shared build: `npm ci` from the lockfile, then **one** `npm run tauri -- build`
   (Tauri runs the frontend `npm run build` itself, so the frontend is built once; cargo runs `--locked`).
   Parallelism follows RAM: under 6 GB: 1 job, under 12 GB: 2, under 24 GB: 3, otherwise 4 (never more than the CPU
   threads); override with `EAK_BUILD_JOBS`.
6. Install per user from a staging copy, register a launcher, start EAK and require it to stay up for 6 seconds.
   The previous version is kept until the new one starts; a failed launch rolls back.
7. Print the commit, platform, install directory, launcher, log path, update and uninstall commands. It never
   prints a success line if a step failed.

## Prerequisites the installer provisions
- **Rust**: official rustup installer, stable, minimal profile, no shell-profile edits. If rustup exists, the
  stable toolchain is installed beside your default and used only for the build (your default is not changed).
  An existing non-rustup Rust is reused if it meets the workspace `rust-version`. Windows uses the MSVC host.
- **Node.js**: your own Node is reused if it satisfies Vite 8 (**20.19+ or 22.12+**, not 21.x). Otherwise a user-local
  Node 22 LTS is downloaded from nodejs.org over HTTPS, checked against nodejs.org's `SHASUMS256.txt`, and
  kept in the EAK build cache only.
- **Linux system packages** (Tauri v2): Debian/Ubuntu `build-essential curl wget file pkg-config
  libwebkit2gtk-4.1-dev libgtk-3-dev libdbus-1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev
  librsvg2-dev`. Fedora/RHEL (dnf), Arch (pacman) and openSUSE (zypper) names come from the Tauri v2 prerequisites
  page and are **unrun**. The installer lists the missing packages and the exact `sudo` commands and asks on the
  terminal first (when piped from `curl` it reads your answer from `/dev/tty`; with no terminal it refuses and prints
  the commands, or use `EAK_ASSUME_YES=1`). Unsupported distributions stop with guidance; install the
  [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#linux) yourself and re-run with `EAK_SKIP_SYSTEM_DEPS=1`.
- **macOS**: Xcode Command Line Tools (`xcode-select --install` is opened for you if missing; finish it, re-run).
- **Windows**: Visual Studio Build Tools with the "Desktop development with C++" workload and the Windows SDK,
  and the WebView2 runtime. These are **large** and need administrator approval, so they are installed only after
  you confirm (winget, or Microsoft's signed bootstrapper). If Windows must restart, restart and run the command again.

## Options
Environment variables (Linux/macOS: put them on the shell after the pipe, e.g. `curl ... | EAK_REF=v0.2.0 bash`;
PowerShell: `$env:EAK_REF='v0.2.0'` first). Linux/macOS also accept `bash -s -- --flag`.

| Variable (flag) | Meaning |
|---|---|
| `EAK_REF` (`--ref`) | tag, branch or full commit to build (default `main`); the resolved SHA is always recorded |
| `EAK_BUILD_JOBS` | cargo parallel jobs (default from RAM, see above) |
| `EAK_NO_LAUNCH=1` (`--no-launch`) | do not start EAK (headless machines; launch is not verified) |
| `EAK_ASSUME_YES=1` (`--yes`) | pre-approve installing system packages / large components when there is no terminal |
| `EAK_FORCE=1` (`--force`) | rebuild even if this commit is already installed |
| `EAK_UNINSTALL=1` (`--uninstall`) | uninstall (below) |
| `EAK_CLEAN_CACHE=1` (`--clean-cache`) | delete EAK's build caches |
| `EAK_INSTALL_MODE=prebuilt` (`--prebuilt`) | use the prebuilt-release installer instead |
| `EAK_REPO`, `EAK_INSTALLER_REF` | repository and the ref the installer scripts themselves come from |
| `EAK_MIN_DISK_GB`, `EAK_ALLOW_LOW_MEMORY`, `EAK_SKIP_SYSTEM_DEPS` | safety overrides (Linux/macOS; Windows has the first two) |

## Where things go
- **Linux**: app `~/.local/share/eak/versions/<commit>-<time>/eak-desktop` with `~/.local/share/eak/current` pointing at it
  (executable, no bundle: `npm run tauri -- build --no-bundle`), command `~/.local/bin/eak-desktop`, menu entry
  `~/.local/share/applications/eak.desktop` + icon, record `~/.local/share/eak/source-install.info`, build cache
  `~/.cache/eak/source-build/` (source, `target/`, npm cache, user-local Node), log `~/.local/state/eak/install-source.log`.
- **macOS**: `~/Applications/EAK.app` (a real `.app` from `tauri build --bundles app`), cache `~/Library/Caches/EAK/source-build`,
  log `~/Library/Logs/EAK/install-source.log`. It is **ad-hoc signed locally only**: not developer-signed, not notarized.
- **Windows**: `%LOCALAPPDATA%\Programs\EAK\eak-desktop.exe` (no bundle) with a Start Menu `EAK` shortcut, cache
  `%LOCALAPPDATA%\EAK\Build`, logs `%LOCALAPPDATA%\EAK\Logs`. Not signed; SmartScreen may warn. (The optional
  prebuilt NSIS installer also uses `%LOCALAPPDATA%\EAK`; uninstalling that can remove the build cache.)
- Cargo's shared registry/git caches in `~/.cargo` are reused. EAK's own data folder (`dev.eak.desktop`) is never touched.

## Update
Run the same command. It resolves the newest requested commit; if it equals the installed commit and the install is
intact, nothing is rebuilt (`EAK_FORCE=1` forces it); otherwise it rebuilds and swaps in the new version safely.
Your projects, settings and credentials are never touched.

## Uninstall
Removes only EAK-managed app files, launcher/shortcut and install record (checked by marker before deleting), never
projects, datasheets, credentials, settings, `~/.cargo`, `~/.rustup` or any Node you installed yourself.
- Linux/macOS: `curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.sh | bash -s -- --uninstall`
- Windows: `$env:EAK_UNINSTALL=1; irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.ps1 | iex`
- Add `--clean-cache` / `$env:EAK_CLEAN_CACHE=1` to also delete the build cache (alone, it deletes only the cache).
- A prebuilt install is removed separately (below).

## Prebuilt release (optional)
Only when you ask for it, and only if the release has an asset for your platform; otherwise it stops with an
explanation. It never stands in for `main`. The newest release is v0.1.4 (prerelease): Linux `.deb` and AppImage,
built before the workspace/terminal/harness refactor; no Windows or macOS binaries are published.
- Linux/macOS: `curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.sh | EAK_INSTALL_MODE=prebuilt bash`
  (the variable goes after the pipe; `EAK_INSTALL_MODE=prebuilt curl ... | bash` would set it for `curl` only)
- Windows: `$env:EAK_INSTALL_MODE='prebuilt'; irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/install.ps1 | iex`
- Prebuilt options and uninstall: `EAK_VERSION`, `EAK_FORMAT=deb`, `EAK_NO_LAUNCH`, `EAK_UNINSTALL=1`, run through the same
  `EAK_INSTALL_MODE=prebuilt` entry. It verifies SHA-256 against the release `SHA256SUMS`; packages are unsigned.
  Its Linux app is in `~/.local/share/eak/app`, command `~/.local/bin/eak`, log `~/.local/state/eak/install.log`.

## Developers
Build without installing: `./scripts/build.sh` (Linux/macOS) or `.\scripts\build.ps1` (Windows); the installers run the same
script. See [DEVELOPMENT.md](DEVELOPMENT.md).

## Troubleshooting
- **Read the log** printed at the failure; the failing command and the last output lines are shown.
- **Out of memory / "killed" linker**: `EAK_BUILD_JOBS=1`, add swap, close other programs.
- **Not enough disk**: free space yourself (the installer never deletes your files); `--clean-cache` drops EAK's caches.
- **No network / proxy**: everything is downloaded over HTTPS (GitHub, crates.io, npm, nodejs.org, static.rust-lang.org).
- **Linux: missing `webkit2gtk-4.1` or `pkg-config` modules**: accept the package list, or install the Tauri prerequisites and use
  `EAK_SKIP_SYSTEM_DEPS=1`. Distributions without WebKitGTK 4.1 packages cannot build EAK.
- **"Cargo.lock needs to be updated"**: the build is deliberately `--locked`; this means the lockfile in that commit is
  inconsistent (the integrated tree has not been compiled since Phase 1, so this can happen). Do not work around it for an install; report it.
- **Windows linker errors / `link.exe` not found**: the C++ workload or Windows SDK is missing; re-run after installing it.
- **Windows "path too long"**: enable long paths (`LongPathsEnabled=1` under `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem`) or use a shorter `%LOCALAPPDATA%`.
- **EAK exits right after launch**: Linux needs the WebKitGTK runtime; Windows needs the WebView2 runtime; run the launcher from a terminal.
- **macOS refuses to open the app**: it is not notarized; right-click > Open once, or check Privacy & Security.
