# EAK development status

Truthful record of what exists and what has been verified. "Implemented" means code exists and
the listed tests pass; it does not mean professionally validated.

## Interactive editors and Gerber export (2026-11-14, branch fm/eak-o1-editors)

**Platform tested:** Linux, Rust stable, Node (see PR body for exact verification output). No window was opened;
editors are verified by vitest (jsdom, mocked IPC) and by headless Rust IPC tests only. Not verified in a real WebView.

- **Implemented and tested:** schematic tools (place library part, wire with pin snap, connect pins, net/power label,
  move, delete, rotate `R`, edit ref/value), PCB tools (board outline rectangle/polygon, move/rotate/flip footprint,
  route tracks on top/bottom with 45° and L segments, vias, delete), undo/redo and per-command durable save
  (header shows "Saved · rN"), reopen, ERC/DRC, and "Export Gerbers" (Gerber, Excellon, BOM, CPL via `eak-manufacturing`
  into `<project>/output/rev-N`). Every edit is `Engine::execute` (ADR-0008); no new Command.
- **Tests:** `ipc.rs` `editor_flow_places_wires_routes_and_exports_gerbers`, `library_lists_builtin_parts_with_pins`,
  `export_without_outline_or_project_fails_loudly`; vitest `editors.test.tsx`, `editing.test.ts`.
- **Limitations (unsupported / unverified):** footprints only appear via "Sync schematic to PCB"; no zone, text or
  multi-select editing; no drag-to-move (two-click move); no rubber-banding of wires on move; no manual pad/footprint
  editor; export status is DRAFT unless ERC/DRC/MECH all pass for the exact revision and is never "manufacturing-ready";
  Gerbers have not been viewed in an external viewer or sent to a fabricator.

## Desktop shell: Tauri 2 + React bound to the command engine (2026-11-13)

**Platform tested:** Windows 11, Rust 1.99.1 `x86_64-pc-windows-gnu`, Node 24.21 / npm 11.19.
**CI (commit 599bf91, pull-request run https://github.com/NijoP/electronics-agent-kit-new-prototype/actions/runs/43431886918):**
all 8 jobs passed: `rust` (policy script, fmt, clippy, `cargo test --workspace --release`) and `desktop` (`npm ci`, typecheck, vitest, `tauri build --no-bundle`) on `windows-latest`, `ubuntu-latest`, `macos-latest` (arm64) and `macos-15-intel` (x64). Linux and macOS are verified by CI only, not on a local machine; no window was opened on any runner.

### Implemented and tested
- `apps/desktop/src-tauri` (crate `eak-desktop`, a workspace member) holds the IPC layer, which is plain Rust functions over one `Session` (one open `Engine` plus the SQLite recent-project index):
  - create, open (with recovery and explicit stale-lock take-over) and close a project;
  - `execute_command`, which always uses the **Human** actor with the revision the UI rendered;
  - undo/redo, `events_since` and recovery acknowledgement;
  - ERC/DRC via `eak-validation`, with the report saved as revision-bound evidence;
  - recent projects.
  - Geometry for the views (absolute pins and pads) is derived on each request through `eak-core`. The UI keeps no project model.
- The React/TypeScript/Vite UI has these parts:
  - Project Home: recent projects, open and create.
  - Workspace: a toolbar, a command palette (Ctrl/Cmd+K, keyboard navigation, confirmation for destructive commands) and keyboard undo/redo.
  - Read-only SVG schematic and PCB views with wheel zoom and drag pan. Selecting an item cross-probes by `symbol_id` in both directions and from findings.
  - Engineering Monitor: ERC/DRC findings, outcome text, a stale-revision marker, the not-implemented checks and the limitations.

### Tests executed (exact results, Windows)
- `cargo test -p eak-desktop --release`: **8 passed, 0 failed**. Covered:
  - Human-actor execute, undo and redo, with stale-revision rejection;
  - events;
  - reopen with rebuilt history and the recent index;
  - invalid paths;
  - lock take-over;
  - unknown-outcome write block and its acknowledgement;
  - ERC/DRC revision binding and evidence;
  - geometry identical to `Project::pin_position`/`pad_position` and cross-probe ids;
  - frontend and Rust IPC command lists match.
- `npm test` (vitest, jsdom): **18 passed, 0 failed**.
- `npm run typecheck`: clean. `npm run build` (Vite): succeeds.
- `cargo test --workspace --release`: **43 passed, 0 failed** (M1 suite 35 + eak-desktop 8).
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`: clean.
- `npm run tauri -- build --no-bundle`: succeeds and produces `eak-desktop.exe` with the embedded frontend. A 6-second launch smoke check stayed running and created the SQLite index in the app data folder; no interactive use.

### Not verified / not implemented
- The app has not been exercised interactively in a real WebView in this change. Rendering, focus and layout are covered only by jsdom tests.
- No editing tools, 3D viewport, library browser, Terminal Center, Manufacturing Center or Agent Studio yet.
- No dockable, resizable or persisted panels (UI-01 is PARTIAL).
- Project folders are typed as absolute paths; there is no native folder picker.
- Installers are not produced (`bundle.active: false`).

## Milestone M1: engineering foundation and headless vertical slice (2026-11-13)

**Platform tested:** Windows 11 (Git Bash), Rust 1.99.1 `x86_64-pc-windows-gnu`, WinLibs GCC 16.2,
AMD Ryzen 7 7840HS, 15.3 GB RAM. Linux and macOS: not run at M1; the first CI run is recorded under "Desktop shell" above.

### Implemented and tested
- EAK-IR v1 (`eak-core`): project, library, schematic, PCB, constraints, manufacturing config;
  integer-nanometre geometry; byte-stable JSON; schema version gate (newer/corrupt files are
  rejected safely); integrity checker; 7 built-in generic components.
- Engineering core (`eak-engineering`):
  - IPC-2221 trace width, clearance, creepage and via current, re-implemented from the DESK
    audit, with DESK reference vectors as tests (tables **not verified against the standard text**).
  - Circuit calculations.
  - Schematic netlist resolution.
  - Exact copper geometry and connectivity, with zone fill on a 0.1 mm raster.
  - A native two-layer A* router.
  - A constructive placer.
  - 3D scene data.
- Verification (`eak-validation`):
  - ERC: 13 rules.
  - DRC: 13 rules, including current capacity and voltage clearance.
  - Mechanical: 3 rules.
  - Every report is bound to a revision and a SHA-256 input hash, and lists the checks that are not implemented.
  - Every finding carries a stable id, the affected objects and location, and a suggested fix; quantitative rules also report measured vs required values with units (ADR-0009). Desktop and AI display of the new fields is not yet wired.
- Runtime (`eak-runtime`):
  - Typed command set (38 commands).
  - Revision checks and the agent approval policy.
  - Transactional apply with integrity gate, diff-based undo/redo and an fsync'd journal with crash reconciliation (UNKNOWN_OUTCOME blocks writes).
  - Checkpoints, project lock, evidence store.
  - Project-scoped process supervisor (timeout, cancel, bounded output).
  - Task model and SQLite index.
- Manufacturing (`eak-manufacturing`):
  - Outputs: RS-274X Gerber (9 layers), Excellon, BOM, CPL.
  - SHA-256 manifest.
  - Release gate that requires revision-bound ERC/DRC/MECH with zero errors, otherwise the package is marked DRAFT.
  - Package re-verification that detects tampering.

### Tests executed (exact results)
`cargo test --workspace --release`: **35 passed, 0 failed** (eak-core 7, eak-engineering 11,
eak-runtime 10 engine + 4 process, eak-manufacturing 3 end-to-end).
`cargo clippy --workspace --all-targets -- -D warnings`: clean. `cargo fmt --check`: clean.

The key end-to-end test (`reference_design_end_to_end`) runs a 10-component ATtiny85 + AMS1117 board through
the command API:
1. Schematic, then ERC: 0 errors.
2. Sync, place, auto-route (**20/20 connections**) and a GND pour.
3. DRC: 0 errors; MECH: 0 errors.
4. Package status: InternallyVerifiedReleaseCandidate; the package re-verifies and tampering is detected.
Injected short and clearance violations are detected (`drc_detects_injected_short_and_clearance_violation`).

### Not yet implemented (acceptance gaps)
- Desktop application: superseded. See "Desktop shell" above (read-only views; editors not started).
- AI Intelligence Engine (agent loop, model providers): not started.
- Symbol/footprint editors, 3D viewport UI, Terminal Center UI, research browser, LCSC/JLCPCB integrations.
- Multi-layer (>2) boards, differential pairs, impedance, thermal reliefs, silkscreen text.
- Linux/macOS builds have not been executed yet; packaging and installers do not exist yet.
- Independent engineering review and fabricator DFM: not performed (required before any manufacturing claim).

## Coordination note
The Firstmate crew could not be started on this host: the Firstmate session lock does not
acquire under native Windows Git Bash (upstream fix pending), and no WSL distribution is
installed. Development is proceeding directly in this repository.

## Next dependency-ready work
1. Interactive schematic editor driving the same commands (the desktop shell above is the base).
2. PCB editor and 3D view; Manufacturing Center and installer bundles.
3. Intelligence Engine: provider abstraction, typed tools over commands, approval flow.
