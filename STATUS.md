# EAK development status

Truthful record of what exists and what has been verified. "Implemented" means code exists and
the listed tests pass; it does not mean professionally validated.

## Milestone M1: engineering foundation and headless vertical slice (2026-11-13)

**Platform tested:** Windows 11 (Git Bash), Rust 1.99.1 `x86_64-pc-windows-gnu`, WinLibs GCC 16.2,
AMD Ryzen 7 7840HS, 15.3 GB RAM. Linux and macOS: CI workflow added, **not yet run**.

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

### LCSC sourcing, offline only (2026-11-13)
Implemented and tested on Windows 11 only. Details: `docs/engineering/sourcing.md`; test
matrix: `docs/engineering/INTEGRATION_TEST_MATRIX.md`.
- Sourcing records (`eak-manufacturing/src/sourcing.rs`): built from raw response bytes, a source
  URL and a retrieval time. Each record keeps the SHA-256 of the raw bytes, the fields exactly
  as parsed, and lists of missing and rejected fields.
- Verification states: search hit, datasheet-matched (hashed PDF that names the MPN), verified
  (named human reviewer against that datasheet hash). A search hit is never a verified part.
- `SOURCING` report (`eak-validation/src/sourcing.rs`): 12 rules covering missing MPN, LCSC number,
  datasheet and ratings, BOM-to-LCSC mapping, record state, and symbol, footprint and package
  consistency. It is not part of the manufacturing release gate.
- Tests: 9 offline fixture tests and 6 unit tests, all passing. Fixtures are synthetic.
  Full run with these added, `cargo test --workspace --release`: **50 passed, 0 failed**
  (eak-core 7, eak-engineering 11, eak-validation 6, eak-runtime 10 engine + 4 process,
  eak-manufacturing 3 end-to-end + 9 sourcing). Clippy with `-D warnings` and `cargo fmt --check`: clean.

Not done, and not to be read as done:
- **No network access exists.** No live LCSC search, stock, price or datasheet retrieval, and no
  network-dependent test. Neither LCSC nor JLCPCB offers documented machine access without
  private credentials; the choice of access route is an open product-owner decision.
- **The LCSC field mapping is unverified.** LCSC's site refused the automated browser, so its
  API documentation could not be read; the mapping follows third-party reports.
- Datasheet matching reads uncompressed PDF bytes only. Ratings, pinouts and land patterns are
  not extracted from datasheets.
- No real part has been sourced or verified with this module.

### Not yet implemented (acceptance gaps)
- **Desktop application** (Tauri 2 + React): not started. There is no GUI yet.
- AI Intelligence Engine (agent loop, model providers): not started.
- Symbol/footprint editors, 3D viewport UI, Terminal Center UI, research browser.
- Live LCSC and JLCPCB access (see the sourcing section above); JLCPCB preparation checks.
- Multi-layer (>2) boards, differential pairs, impedance, thermal reliefs, silkscreen text.
- Linux/macOS builds have not been executed yet; packaging and installers do not exist yet.
- Independent engineering review and fabricator DFM: not performed (required before any manufacturing claim).

## Coordination note
The Firstmate crew could not be started on this host: the Firstmate session lock does not
acquire under native Windows Git Bash (upstream fix pending), and no WSL distribution is
installed. Development is proceeding directly in this repository.

## Next dependency-ready work
1. Tauri 2 desktop shell + React workspace bound to the command engine (Phase 1/4).
2. Schematic and PCB editor canvases driving the same commands.
3. Intelligence Engine: provider abstraction, typed tools over commands, approval flow.
