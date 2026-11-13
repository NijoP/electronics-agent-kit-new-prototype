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
| CMD-01 | All mutations go through typed commands, with revision checks and approval policy | `eak-runtime` (`command.rs`, `engine.rs`), `apps/cli` | `tests/engine.rs` (stale revision, agent approval); `apps/cli/tests/cli.rs` (CLI applies the same typed commands through `Engine::execute`, stale `--expect-rev` rejected) | PASS (Windows, Linux and macOS: CI run 43431745631, all 4 jobs) |
| CLI-01 | Headless manual/automation client on the shared command API (Exec. Instr. Phase 3 "manual test client and UI use same API") | `apps/cli` (binary `eak`) | `apps/cli/tests/cli.rs` (5 end-to-end tests, each step a new process: new/demo/open/show, apply file+stdin with `@REFDES`, undo/redo, ERC/DRC/MECH evidence, compile + re-verify, checkpoints/restore, lock, recovery acknowledgement, JSON output and exit codes) | PASS (Windows, Linux and macOS: CI run 43431745631, all 4 jobs). Gaps: batches are not atomic; `@REFDES` resolves only symbol/footprint id fields. Owner: eak-core |
| UNDO-01 | Reliable undo/redo, checkpoints, crash recovery | `eak-runtime` (`delta.rs`, `store.rs`, `engine.rs`) | `tests/engine.rs` (undo of every command kind, checkpoint restore, pending-journal reconciliation, UNKNOWN_OUTCOME block that survives reopen until acknowledged); CLI tests (undo/redo across processes, block across invocations) | PASS (Windows, Linux and macOS: CI run 43431745631, all 4 jobs) |
| PROC-01/02 | Supervised processes with timeout, cancel (whole process tree) and bounded output | `eak-runtime/process.rs` | `tests/process.rs` (6, incl. grandchild killed on cancel and fast-exit output capture). Windows path and Unix `killpg` path both passed in CI run 43431745631 (windows-latest, ubuntu-latest, macos-latest, macos-15-intel) | PARTIAL (process-group escape via `setsid` not covered; no crash/two-project app-level test) |
| EDA-01 | Native schematic capture and ERC | `eak-core`, `eak-engineering/netlist.rs`, `eak-validation/erc.rs` | e2e: reference schematic with 0 ERC errors | PARTIAL (no editor UI) |
| EDA-02 | Native PCB layout, placement, routing, DRC | `copper.rs`, `placer.rs`, `router.rs`, `drc.rs` | e2e: 20/20 routed, 0 DRC errors; injected short and clearance violation detected | PARTIAL (2 layers only, no editor UI) |
| PCB-01 | Current- and voltage-aware rules | `ipc.rs`, DRC-001/002 | DESK reference vectors | PARTIAL (tables not verified against the standards) |
| 3D-01 | 3D view and mechanical checks | `scene3d.rs`, `mech.rs` | MECH run in e2e | PARTIAL (no viewport, parametric bodies only) |
| MFG-01 | Revision-bound manufacturing package with release gate | `eak-manufacturing` | e2e: release candidate status, re-verification, tamper detection | PASS (internal verification only; no fabricator DFM) |
| UI-01 | Desktop workspace (Tauri 2 + React) | `apps/desktop` | — | NOT STARTED |
| AI-01, AG-01..03 | Lead agent using the same typed commands under approval | `eak-intelligence` | Approval policy exists in the engine | NOT STARTED (agent) |
| BROW-01 | Optional research browser | — | — | NOT STARTED |
| SEC-01 | No secrets in the repo; approval for external or irreversible actions | `.gitignore`, approval policy | Manual review | PARTIAL |
| BOOT-01 | Cross-platform build and installers | `.github/workflows/ci.yml` | Matrix: windows-latest, ubuntu-latest, macos-latest, macos-15-intel (replaces retired macos-13). The first M1 run passed on Windows and failed on Linux/macOS at the Python-free step (scripts were not executable; fixed). CI run 43431745631 is green on all 4 jobs (build, clippy, 43 tests each) | PARTIAL (no installers or packaging; Linux/macOS exercised in CI only) |
| PERF-01 | Performance targets | — | — | UNKNOWN |
