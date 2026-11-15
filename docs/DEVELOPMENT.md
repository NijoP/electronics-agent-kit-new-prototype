# Development

Rules: `AGENTS.md`. Tests by level: `TESTING.md`. Decisions: `docs/decisions/`.

## Prerequisites
- Stable Rust 1.80+ (the locked dependencies may need a newer stable), Node.js 20.19+ or 22.12+ with npm (Vite 8).
- Linux (Debian/Ubuntu): `build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libdbus-1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev curl wget file`.
  Other distributions: <https://v2.tauri.app/start/prerequisites/#linux> (`docs/INSTALL.md` lists the names the installer uses).
- macOS: Xcode command-line tools. Windows: WebView2 and, for `scripts/build.ps1` and the installer, the **MSVC** toolchain
  (Visual Studio Build Tools, "Desktop development with C++", Windows SDK, rustup `x86_64-pc-windows-msvc`). The older GNU/MinGW setup
  (`source scripts/env.sh` in Git Bash) is only for testing the core crates; it cannot build the desktop app.
- No Python anywhere (`scripts/check-no-python.sh`).

## Build the desktop app
```bash
./scripts/build.sh       # Linux / macOS
.\scripts\build.ps1      # Windows PowerShell
```
Checks the prerequisites, runs `npm ci` (lockfile), then **one** `npm run tauri -- build` (Tauri's `beforeBuildCommand`
builds the frontend, then cargo `--release --locked`), and prints the artifact path (Linux `target/release/eak-desktop`,
Windows `target\release\eak-desktop.exe`, macOS `target/release/bundle/macos/EAK.app`). `EAK_BUNDLES=deb,appimage`
(Linux) or `-Bundles nsis` (Windows) also produce packages; `EAK_BUILD_JOBS` limits cargo parallelism (default from RAM).
Do not use a bare `cargo build --release` for the app: it skips the frontend build. The one-command installers
(`install.sh`, `install.ps1`) run these same scripts on a fresh checkout; `scripts/release-build.sh` is the separate
Linux `.deb`/AppImage release build. Both are **code-reviewed only, never run**.

## Run and check
```bash
cd apps/desktop && npm ci && npm run tauri dev
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace --release
bash scripts/check-no-python.sh && (cd apps/desktop && npm test)
```
Low-memory machines: `docs/dev/LOW_MEMORY_DEVELOPMENT.md`, `scripts/ci-local.sh`. The only GitHub
workflow, `.github/workflows/ci.yml`, is disabled on GitHub; add no workflows.

## Adding a workspace
Drop `apps/desktop/src/workspaces/<name>.workspace.tsx` that calls `registerWorkspace`
(`apps/desktop/src/workspaces/README.md`). The Terminal workspace is registered this way.

## Third-party code
Only under `third_party/licenses/` with notices; see `docs/LICENSES_AND_ATTRIBUTION.md`.
