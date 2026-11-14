# Performance and installation engineering log

Source inspection only. Nothing here is built, run or measured; no improvement is claimed as measured.

## 1. Installer rollback, root entry points (P0, code-reviewed only)
- **Problem:** `scripts/install.sh` deleted the previous app directory as soon as the swap finished, so a new version that failed the launch check left the user with a broken install. The advertised root URLs (`/install.sh`, `/install.ps1`) did not exist. The `.deb` path copied the package to a predictable `/tmp/eak-install.deb`.
- **Change:** previous Linux AppImage dir / macOS `EAK.app` is kept as `*.old` until the launch check passes, `fail()` calls `rollback()` (restore dir and `VERSION`; fresh installs are removed). `.deb` installs straight from the mktemp dir. Thin root `install.sh` / `install.ps1` delegate to `scripts/` (local copy if present, else HTTPS fetch from the same repo/ref).
- **Expected benefit:** failed upgrade leaves the prior working version. No resource cost beyond one extra app directory during the install.
- **Correctness:** integrity is unchanged (SHA-256 against release `SHA256SUMS` before install). User config and projects are never touched. Windows rollback is not added: the NSIS installer upgrades in place and its own atomicity is unverified.
- **Modules:** `install.sh`, `install.ps1`, `scripts/install.sh`, `scripts/install.ps1`, `docs/INSTALL.md`, `docs/PLATFORM_SUPPORT.md`.
- **Needs real-machine verification:** all of it.

## Remaining (not started)
P1/P2 startup-path and lifecycle review, execution profile setting, P3 revision-keyed caching.
