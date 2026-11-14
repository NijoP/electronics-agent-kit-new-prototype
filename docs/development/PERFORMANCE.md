# Performance and installation engineering log

Source inspection only. Nothing here is built, run or measured; no improvement is claimed as measured.

## 1. Installer rollback, root entry points (P0, code-reviewed only)
- **Problem:** `scripts/install.sh` deleted the previous app directory as soon as the swap finished, so a new version that failed the launch check left the user with a broken install. The advertised root URLs (`/install.sh`, `/install.ps1`) did not exist. The `.deb` path copied the package to a predictable `/tmp/eak-install.deb`.
- **Change:** previous Linux AppImage dir / macOS `EAK.app` is kept as `*.old` until the launch check passes, `fail()` calls `rollback()` (restore dir and `VERSION`; fresh installs are removed). `.deb` installs straight from the mktemp dir. Thin root `install.sh` / `install.ps1` delegate to `scripts/` (local copy if present, else HTTPS fetch from the same repo/ref).
- **Expected benefit:** failed upgrade leaves the prior working version. No resource cost beyond one extra app directory during the install.
- **Correctness:** integrity is unchanged (SHA-256 against release `SHA256SUMS` before install). User config and projects are never touched. Windows rollback is not added: the NSIS installer upgrades in place and its own atomicity is unverified.
- **Modules:** `install.sh`, `install.ps1`, `scripts/install.sh`, `scripts/install.ps1`, `docs/INSTALL.md`, `docs/PLATFORM_SUPPORT.md`.
- **Needs real-machine verification:** all of it.

## 2. Terminal lifecycle commands off the UI thread (P1, code-reviewed only)
- **Problem:** the terminal Tauri commands were plain `#[tauri::command]`, which Tauri runs on the main thread. PTY spawn (create/restart/run), process-tree kill (close, space close; `taskkill /T` on Windows) and shell probing could freeze the window.
- **Change:** `terminal_shells/create/run/restart/close/space_close` are now `#[tauri::command(async)]` (thread pool, like the model/workspace/library commands). `terminal_write`/`terminal_resize` deliberately stay synchronous: async tasks may run out of order and keystrokes must stay ordered.
- **Benefit:** UI thread stays free during session start/stop. Not measured.
- **Correctness:** the JS side awaits each lifecycle call, so ordering is unchanged. Startup was inspected: every optional subsystem (index, harness, terminal API, model settings) already logs and continues; no change needed.
- **Modules:** `apps/desktop/src-tauri/src/terminal.rs`.

## Remaining (not started)
Execution profile setting (low/balanced/high), PTY leak review, cache bounds, P3 revision-keyed caching.
