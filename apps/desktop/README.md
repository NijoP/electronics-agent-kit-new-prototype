# EAK desktop shell (Tauri 2 + React/TypeScript/Vite)

A thin presentation layer over the shared command engine. The Rust side
(`src-tauri/src/lib.rs`) wraps `eak_runtime::Engine`; the React side (`src/`) renders the
engine snapshot and sends typed commands. The UI keeps no project model of its own.

| IPC command | Engine call |
|---|---|
| `create_project`, `open_project`, `close_project`, `current_project` | `Engine::create` / `Engine::open` (lock, recovery), snapshot of the canonical EAK-IR plus geometry derived by `eak-core` |
| `execute_command` | `Engine::execute` with the **Human** actor and the revision the UI rendered (stale requests are rejected) |
| `undo`, `redo` | `Engine::undo` / `Engine::redo` (Human actor, revision-checked) |
| `events_since` | `Engine::events_since` (activity log) |
| `acknowledge_recovery` | `Engine::acknowledge_recovery` (unblocks writes after an unknown-outcome recovery) |
| `run_check` | `eak_validation::erc::run` / `drc::run`, and the report is saved as revision-bound evidence |
| `recent_projects` | SQLite index (`eak_runtime::index`) in the app data folder |

The UI provides:
- Project Home: recent projects, open (with explicit stale-lock take-over) and create.
- Workspace: a toolbar, a command palette (Ctrl/Cmd+K), and undo/redo (Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z or Ctrl+Y).
- Read-only schematic and PCB views (SVG, wheel zoom, drag pan, double-click to fit). Clicking a symbol or footprint selects it in both views; the cross-probe key is the shared `symbol_id`.
- Engineering Monitor: ERC/DRC findings with severity, rule, revision, a stale marker, the checks that are not implemented, and the limitations. Clicking a finding cross-probes it.

Not in this shell yet: editing tools, 3D viewport, library browser, Terminal Center, Manufacturing Center, Agent Studio, dockable or persisted panel layout, native folder picker, and installers.

## Prerequisites (all OSes)
- Stable Rust (see the root `Cargo.toml`) and Node.js 22 or newer with npm.
- **Windows 10/11:** the WebView2 runtime (preinstalled on Windows 11). Either Rust toolchain works. MSVC needs the Visual Studio Build Tools; GNU needs MinGW-w64 (`source scripts/env.sh` in Git Bash).
- **Linux (Ubuntu 22.04+):** `sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev`
- **macOS (arm64 or x64):** Xcode command-line tools (`xcode-select --install`).

## Run, test and build
Run these from `apps/desktop`. The commands are the same on every OS.

```sh
npm ci                       # install the locked frontend toolchain
npm run tauri dev            # launch the app (Vite dev server on port 1420)
npm test                     # vitest UI tests (jsdom, headless)
npm run typecheck            # TypeScript strict check
npm run tauri -- build --no-bundle   # release binary with the embedded frontend
```

The Rust IPC tests are plain functions with no window, so they run headless:
`cargo test -p eak-desktop --release` (also part of `cargo test --workspace --release`).
A plain `cargo build` of this crate does not need the frontend to be built first. Only
`tauri build` embeds `dist/`.

Installer bundles (MSI/NSIS, .deb/AppImage, .dmg) are disabled (`bundle.active: false`).
They are later work.
