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
| CMD-01 | All mutations go through typed commands, with revision checks and approval policy | `eak-runtime` (`command.rs`, `engine.rs`) | `tests/engine.rs` (stale revision, agent approval) | PASS |
| UNDO-01 | Reliable undo/redo, checkpoints, crash recovery | `eak-runtime` (`delta.rs`, `store.rs`, `engine.rs`) | `tests/engine.rs` (undo of every command kind, checkpoint restore, pending-journal reconciliation, UNKNOWN_OUTCOME block) | PASS |
| PROC-01/02 | Supervised processes with timeout, cancel and bounded output | `eak-runtime/process.rs` | `tests/process.rs` (4) on Windows only | PARTIAL (Unix path not yet executed) |
| EDA-01 | Native schematic capture and ERC | `eak-core`, `eak-engineering/netlist.rs`, `eak-validation/erc.rs` | e2e: reference schematic with 0 ERC errors | PARTIAL (no editor UI) |
| EDA-02 | Native PCB layout, placement, routing, DRC | `copper.rs`, `placer.rs`, `router.rs`, `drc.rs` | e2e: 20/20 routed, 0 DRC errors; injected short and clearance violation detected | PARTIAL (2 layers only, no editor UI) |
| PCB-01 | Current- and voltage-aware rules | `ipc.rs`, DRC-001/002 | DESK reference vectors | PARTIAL (tables not verified against the standards) |
| 3D-01 | 3D view and mechanical checks | `scene3d.rs`, `mech.rs` | MECH run in e2e | PARTIAL (no viewport, parametric bodies only) |
| MFG-01 | Revision-bound manufacturing package with release gate | `eak-manufacturing` | e2e: release candidate status, re-verification, tamper detection | PASS (internal verification only; no fabricator DFM) |
| UI-01 | Desktop workspace (Tauri 2 + React) | `apps/desktop` | — | NOT STARTED |
| AI-01, AG-01..03 | Lead agent using the same typed commands under approval | `eak-intelligence` | Approval policy exists in the engine | NOT STARTED (agent) |
| BROW-01 | Optional research browser | — | — | NOT STARTED |
| SRC-01 | LCSC sourcing records with provenance and confidence; a search hit is never a verified part | `eak-manufacturing/sourcing.rs` | `tests/sourcing.rs` (9 offline fixture tests): provenance and hash, missing and hostile input, datasheet hash, human-only verification | PARTIAL (offline only: no live search, stock, price or datasheet retrieval; LCSC field mapping unverified) |
| SRC-02 | Missing-data detection, BOM-to-LCSC mapping, symbol, footprint and datasheet compatibility | `eak-validation/sourcing.rs` | 6 unit tests; `lcsc_selection_reaches_the_library_bom_and_report_through_commands` | PARTIAL (textual package comparison; datasheet content is not extracted) |
| SRC-03 | Documented and permitted supplier access only; integration test matrix | `docs/engineering/sourcing.md`, `docs/engineering/INTEGRATION_TEST_MATRIX.md` | Access and terms research dated 2026-11-13 | PARTIAL (access route undecided; no network-dependent tests exist) |
| SEC-01 | No secrets in the repo; approval for external or irreversible actions | `.gitignore`, approval policy | Manual review | PARTIAL |
| BOOT-01 | Cross-platform build and installers | `.github/workflows/ci.yml` | CI matrix defined but not yet run | PARTIAL |
| PERF-01 | Performance targets | — | — | UNKNOWN |
