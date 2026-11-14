# Installing EAK

EAK ships as prebuilt, **unsigned** packages. **Today the newest release is v0.1.4 (prerelease) with Linux `.deb` and AppImage only, built before the workspace/terminal/harness refactor. No Windows or macOS binaries are published yet; those installers will say so.** You do not need Rust,
Node.js or any build tools. Installers always pick the newest release, including prereleases.

| OS | One command |
|---|---|
| Linux (x86_64) and macOS (Apple Silicon / Intel) | `curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.sh \| bash` |
| Windows 10/11 x64 (PowerShell) | `irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.ps1 \| iex` |
| Windows (CMD) | `powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.ps1 \| iex"` |

Each installer: detects OS and CPU, checks prerequisites, downloads over HTTPS, **verifies SHA-256 against the
release `SHA256SUMS`**, installs (idempotent; re-running upgrades in place), then
launches EAK and only reports success if it stays up. The log path is printed on failure.

Options (environment variables): `EAK_VERSION=v0.2.0` pin a release, `EAK_FORMAT=deb` (Linux, system-wide
`.deb` via sudo), `EAK_NO_LAUNCH=1`, `EAK_UNINSTALL=1`, `EAK_REPO=owner/name`.

## Where things go
- **Linux** (default, no root): app in `~/.local/share/eak/app`, commands `~/.local/bin/eak`,
  launcher `~/.local/share/applications/eak.desktop`, log `~/.local/state/eak/install.log`.
  Requires glibc 2.35+ (Ubuntu 22.04+, Debian 12+, Fedora 36+). `.deb` and `.AppImage` are also on the release page.
- **macOS**: `/Applications/EAK.app` (or `~/Applications` if not writable).
- **Windows**: per-user NSIS install in `%LOCALAPPDATA%\EAK`, registered in Settings > Apps,
  log `%LOCALAPPDATA%\EAK-install\install.log`.

## Unsigned builds
No code-signing certificate or Apple Developer credentials are available, so nothing is signed or notarized.
Windows SmartScreen may show "unknown publisher" (More info > Run anyway). On macOS, if Gatekeeper blocks the
app, right-click > Open once (the installer already clears the quarantine flag on the copy it installs).
The SHA-256 verification is the integrity guarantee.

## Upgrade and uninstall
Upgrade: run the same command again. Your projects and settings are never touched by install or uninstall.
- Linux/macOS: `curl -fsSL https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.sh | bash -s -- --uninstall` (system `.deb`: `sudo apt-get remove eak`).
- Windows: `$env:EAK_UNINSTALL=1; irm https://raw.githubusercontent.com/NijoP/electronics-agent-kit-new-prototype/main/scripts/install.ps1 | iex`, or Settings > Apps > EAK.

## Manual download
Assets on v0.1.4: `EAK_0.1.4_amd64.deb`, `EAK_0.1.4_amd64.AppImage`, `SHA256SUMS`. Intended, not yet published: `eak-windows-x86_64-setup.exe`, `eak-macos-aarch64.dmg`, `eak-macos-x86_64.dmg`. Verify with `sha256sum -c SHA256SUMS --ignore-missing`.
`eak-macos-aarch64.dmg`, `eak-macos-x86_64.dmg`, `SHA256SUMS`. Verify with `sha256sum -c SHA256SUMS --ignore-missing`.
