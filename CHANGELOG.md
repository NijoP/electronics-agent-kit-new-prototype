# Changelog

## Unreleased - desktop shell
- `apps/desktop`: Tauri 2 shell (`src-tauri`, workspace member `eak-desktop`) plus a React/TypeScript/Vite UI.
- IPC commands wrap `eak_runtime::Engine`:
  - open, create and close a project (with recovery and explicit stale-lock take-over);
  - `execute_command` with the Human actor and the rendered revision;
  - undo/redo, `events_since` and recovery acknowledgement;
  - ERC/DRC runs whose reports are saved as revision-bound evidence;
  - recent projects from the SQLite index.
- UI:
  - Project Home;
  - a workspace with toolbar, command palette and keyboard undo/redo;
  - read-only SVG schematic and PCB views with pan/zoom and cross-probing by `symbol_id`;
  - an Engineering Monitor that lists ERC/DRC findings, stale reports, not-implemented checks and limitations.
- Tests: Rust IPC contract tests (`apps/desktop/src-tauri/tests/ipc.rs`) and vitest UI tests (jsdom).
- CI changes:
  - new `desktop` job on Windows, Linux, macOS arm64 and macOS x64 (typecheck, vitest, `tauri build --no-bundle`);
  - the policy script now runs via `bash` because it lacked the executable bit on Unix;
  - `macos-13` (retired runner image) is replaced by `macos-15-intel`.

## 0.1.0 (unreleased) - M1 engineering foundation
- EAK-IR v1, built-in starter library, schema gate and integrity checks.
- Engineering core: IPC sizing rules, circuit calcs, netlist, copper connectivity, zone fill, router, placer.
- Verification engine: ERC (13 rules), DRC (13 rules), mechanical (3 rules) with revision-bound evidence.
- Runtime: typed command engine, approvals, transactions, undo/redo, journal + crash recovery, checkpoints,
  process supervisor, task model, SQLite index.
- Manufacturing compiler: Gerber, Excellon, BOM, CPL, SHA-256 manifest, release gate, package verification.
- Three-OS CI workflow and Python-free policy scan.
