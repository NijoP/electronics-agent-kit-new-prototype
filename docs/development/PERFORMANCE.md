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

Bottleneck ledger and baseline map: `PERFORMANCE_LEDGER.md`.

## 3. Resource profile and workload classes (P0/P2, code-reviewed only, unrun)
- **Problem:** heavy IPC (ERC/DRC/DFM, export, simulation) ran as sync code on Tokio worker threads with no limit and held the single engine lock, so a check froze edits and route previews; nothing decided concurrency or 3D quality centrally (ledger L1–L4, L18).
- **Change:** `apps/desktop/src-tauri/src/resources.rs` is the one profile: logical CPUs from `std::thread::available_parallelism` (no `sysinfo`: CPU count is enough for these limits and memory/GPU cannot be read portably); preference AUTO / LOW_RESOURCE / BALANCED / HIGH_PERFORMANCE / CUSTOM in `performance.json` (app config dir); AUTO is resolved at start and on change only (no polling, no oscillation). It exposes `analysis_jobs`, `background_jobs`, `cache_budget_mb`, `pixel_ratio_cap`, `antialias`, `scrollback_lines` and its limitations. Classes: P0 interactive reads and P1 edits run on `spawn_blocking` without queueing; P2 checks/analysis and P3+ export/simulation wait on Tokio semaphores sized by the profile (queued work waits, it is never skipped). Analysis runs on an `Arc<Project>` cloned once per revision; evidence is saved only if the engine is still at that (folder, id, revision), otherwise the call fails "run it again". The UI reads the profile for the 3D renderer and xterm scrollback; Settings → Performance changes it.
- **Not done:** `cache_budget_mb` has no consumer yet (the 3D model cache belongs to the 3D-components lane); no memory-pressure adaptation; harness endpoint and terminal PTY creation are not yet bounded (ledger L12, L15).
- **Correctness:** no engineering value, geometry, rule or validation outcome depends on the profile.

## Remaining (not started)
Typed incremental snapshots to the UI (ADR needed, L5), selection store (L6), SVG culling (L16), PTY output batching (L15), PTY leak review. Verification plan: `PERFORMANCE_VERIFICATION_PLAN.md`.
