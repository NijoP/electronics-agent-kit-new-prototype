# Requirements traceability

Status values: **PASS** means implemented and covered by an executed test. **PARTIAL** means some
code exists but the requirement is not met. **NOT STARTED**. **UNKNOWN** means not yet measured.
Unknown is never reported as pass. The IDs follow the acceptance areas in the source-of-truth
documents (`docs/source-of-truth/`).

| ID | Requirement (summary) | Module(s) | Evidence | Status |
|---|---|---|---|---|
| ARCH-01 | Modular monolith with one-directional layering | workspace `Cargo.toml`, all crates | Crate dependency graph; `docs/ARCHITECTURE.md` | PASS |
| LANG-01 | Rust-first and Python-free | all | `scripts/check-no-python.sh` (CI step) | PASS |
| IR-01 | One canonical, versioned EAK-IR with safe migration | `eak-core` (`ir.rs`, `migrate.rs`, `integrity.rs`) | eak-core tests: round-trip byte-stability, rejection of newer, corrupt and missing versions | PASS |
| CMD-01 | All mutations go through typed commands, with revision checks and approval policy | `eak-runtime` (`command.rs`, `engine.rs`); desktop IPC `apps/desktop/src-tauri/src/lib.rs` | `tests/engine.rs` (stale revision, agent approval); `apps/desktop/src-tauri/tests/ipc.rs` (UI path is `Engine::execute`/`undo`/`redo` as Human with the rendered revision; stale requests rejected) | PASS |
| UNDO-01 | Reliable undo/redo, checkpoints, crash recovery | `eak-runtime` (`delta.rs`, `store.rs`, `engine.rs`) | `tests/engine.rs` (undo of every command kind, checkpoint restore, pending-journal reconciliation, UNKNOWN_OUTCOME block) | PASS |
| PROC-01/02 | Supervised processes with timeout, cancel and bounded output | `eak-runtime/process.rs` | `tests/process.rs` (4) on Windows only | PARTIAL (Unix path not yet executed) |
| EDA-01 | Native schematic capture and ERC | `eak-core`, `eak-engineering/netlist.rs`, `eak-validation/erc.rs` | e2e: reference schematic with 0 ERC errors; `eak-validation/tests/actionable_findings.rs` (finding ids, pin locations, fixes, ERC-011 measured vs required) | PARTIAL (read-only schematic view only; no editor UI; new finding fields not yet shown in the desktop) |
| EDA-02 | Native PCB layout, placement, routing, DRC | `copper.rs`, `placer.rs`, `router.rs`, `drc.rs` | e2e: 20/20 routed, 0 DRC errors; injected short and clearance violation detected; `actionable_findings.rs` (measured vs required for width, current, clearance, edge, drill, ring) | PARTIAL (2 layers only; read-only PCB view, no editor UI; new finding fields not yet shown in the desktop) |
| PCB-01 | Current- and voltage-aware rules | `ipc.rs`, DRC-001/002 | DESK reference vectors | PARTIAL (tables not verified against the standards) |
| 3D-01 | 3D view and mechanical checks | `scene3d.rs`, `mech.rs` | MECH run in e2e | PARTIAL (no viewport, parametric bodies only) |
| MFG-01 | Revision-bound manufacturing package with release gate | `eak-manufacturing` | e2e: release candidate status, re-verification, tamper detection | PASS (internal verification only; no fabricator DFM) |
| UI-01 | Desktop workspace (Tauri 2 + React): Project Home, workspace layout, command palette | `apps/desktop` | `src-tauri/tests/ipc.rs` (create/open/reopen, recent index, lock take-over, recovery block); vitest `App.test.tsx`, `components.test.tsx` (Project Home, palette keyboard flow and destructive confirm, IPC argument shapes) | PARTIAL (no dockable/resizable/persisted panels; layout and keyboard focus not verified in a real window; no screen tests) |
| UI-02 | Read-only schematic/PCB views from EAK-IR with pan/zoom and cross-probe by `symbol_id`; Engineering Monitor for ERC/DRC | `apps/desktop/src` (`SchematicView`, `PcbView`, `Canvas`, `EngineeringMonitor`), geometry from `eak-core` | `ipc.rs` (`geometry_is_derived_from_eak_ir_with_cross_probe_ids`, `checks_are_revision_bound_and_saved_as_evidence`); vitest (rendering, cross-probe both ways and from findings, stale report marker, not-implemented list, zoom/pan maths) | PARTIAL (jsdom tests only; rendering not verified in a WebView; no editing; 3D not started) |
| UI-03 | Interactive schematic and 2-layer PCB editors over typed commands; Gerber/Excellon export from the app | `apps/desktop/src` (`SchematicView`, `PcbView`, `editing.ts`, `ExportPanel`), `src-tauri` (`library_components`, `export_manufacturing`) | `ipc.rs` editor flow + export; vitest `editors.test.tsx`, `editing.test.ts` | PARTIAL (implemented and tested headless/jsdom; not exercised in a real WebView; see STATUS.md limitations) |
| AI-01, AG-01..03 | Lead agent using the same typed commands under approval | `eak-intelligence` | Approval policy exists in the engine | NOT STARTED (agent) |
| BROW-01 | Optional research browser | — | — | NOT STARTED |
| SEC-01 | No secrets in the repo; approval for external or irreversible actions | `.gitignore`, approval policy | Manual review | PARTIAL |
| BOOT-01 | Cross-platform build and installers | `.github/workflows/ci.yml` (`rust` + `desktop` jobs on Windows, Ubuntu, macOS arm64/x64) | Windows local build and tests; CI run on 599bf91 green on all four runners for both jobs (recorded in STATUS.md) | PARTIAL (no installers; Linux/macOS verified by CI only) |
| PERF-01 | Performance targets | — | — | UNKNOWN |
