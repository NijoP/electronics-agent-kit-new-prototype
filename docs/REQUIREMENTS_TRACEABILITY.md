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
| PWR-01 | Deterministic power-design calculations for the synchronous buck (duty, inductor, ripple, peak/RMS current, output/input capacitors, MOSFET/inductor losses, efficiency, junction temperature, E24/E96 feedback divider) | `eak-engineering/src/calc.rs`; `docs/engineering/POWER_CALCULATIONS.md` | eak-engineering tests: hand-computed 24 V → 5 V, 3 A, 500 kHz reference case; invalid-input rejection | PARTIAL (library functions only, not yet a typed runtime command or CLI; losses exclude Coss, core and ESR loss; formulas from TI/Infineon app notes, not independently reviewed) |
| ERC-02 | Component rating checks: capacitor voltage, regulator/IC supply voltage, inductor saturation/rated current, MOSFET Vds | `eak-validation/src/erc.rs` (ERC-011, ERC-014..016, shared `check_rating`) | eak-validation tests: pass, warning, error, missing rating, missing rail/current, unit mismatch and unverified-rating cases | PARTIAL (uses declared net voltages/currents only; built-in ratings are unverified; diode reverse voltage, resistor power, Id/Vgs and thermal limits not checked) |
| PCB-01 | Current- and voltage-aware rules | `ipc.rs`, DRC-001/002 | DESK reference vectors | PARTIAL (tables not verified against the standards) |
| 3D-01 | 3D view and mechanical checks | `scene3d.rs`, `mech.rs` | MECH run in e2e | PARTIAL (no viewport, parametric bodies only) |
| MFG-01 | Revision-bound manufacturing package with release gate | `eak-manufacturing` | e2e: release candidate status, re-verification, tamper detection | PASS (internal verification only; no fabricator DFM) |
| UI-01 | Desktop workspace (Tauri 2 + React) | `apps/desktop` | — | NOT STARTED |
| AI-01, AG-01..03 | Lead agent using the same typed commands under approval | `eak-intelligence` | Approval policy exists in the engine | NOT STARTED (agent) |
| BROW-01 | Optional research browser | — | — | NOT STARTED |
| SEC-01 | No secrets in the repo; approval for external or irreversible actions | `.gitignore`, approval policy | Manual review | PARTIAL |
| BOOT-01 | Cross-platform build and installers | `.github/workflows/ci.yml` | CI matrix defined but not yet run | PARTIAL |
| PERF-01 | Performance targets | — | — | UNKNOWN |
