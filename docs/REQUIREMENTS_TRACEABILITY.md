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
| PCB-02 | Thermal reliefs on same-net pads in zones (fill-, connectivity- and DRC-aware) | `copper.rs` (`fill_zone`, `relief_rule`), DRC-014 | eak-engineering `copper::tests` (4 spokes and gap; blocked spoke removed and connectivity kept; no landing means no connection); eak-validation `drc::tests` (starved relief, current-vs-spoke warning, power class clears it); e2e: J1.2/J2.2 relieved, GND one group | PASS (relief gap and spoke width derived from zone clearance and net class; no per-zone settings in the IR) |
| PCB-03 | Silkscreen reference designators in the Gerber legend, clipped at soldermask openings; DRC for silkscreen over pads | `silk.rs`, `gerber.rs::silkscreen`, DRC-015 | `silk::tests` (glyph set, centring and mirroring, clip geometry); `drc::tests` (overlap error, near warning, clipped legend clean); e2e: written `.gto` matches the clipped model, DRC-015 silent | PASS (internal checks only; legibility, overlap and edge checks not implemented) |
| PCB-01 | Current- and voltage-aware rules | `ipc.rs`, DRC-001/002/008/014 | DESK reference vectors; every value listed with its status in `docs/engineering/IPC_RULE_VALUES.md` | PARTIAL (no IPC/IEC value verified against the standard text) |
| 3D-01 | 3D view and mechanical checks | `scene3d.rs`, `mech.rs` | MECH run in e2e | PARTIAL (no viewport, parametric bodies only) |
| MFG-01 | Revision-bound manufacturing package with release gate | `eak-manufacturing` | e2e: release candidate status, re-verification, tamper detection | PASS (internal verification only; no fabricator DFM) |
| UI-01 | Desktop workspace (Tauri 2 + React) | `apps/desktop` | — | NOT STARTED |
| AI-01, AG-01..03 | Lead agent using the same typed commands under approval | `eak-intelligence` | Approval policy exists in the engine | NOT STARTED (agent) |
| BROW-01 | Optional research browser | — | — | NOT STARTED |
| SEC-01 | No secrets in the repo; approval for external or irreversible actions | `.gitignore`, approval policy | Manual review | PARTIAL |
| BOOT-01 | Cross-platform build and installers | `.github/workflows/ci.yml` | CI matrix defined but not yet run | PARTIAL |
| PERF-01 | Performance targets | — | — | UNKNOWN |
