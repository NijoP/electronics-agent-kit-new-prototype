# EAK development status

Truthful record of what exists and what has been verified. "Implemented" means code exists and
the listed tests pass; it does not mean professionally validated.

## Milestone M1: engineering foundation and headless vertical slice (2026-11-13)

**Platform tested:** Windows 11 (Git Bash), Rust 1.99.1 `x86_64-pc-windows-gnu`, WinLibs GCC 16.2,
AMD Ryzen 7 7840HS, 15.3 GB RAM. Linux and macOS: CI workflow added, **not yet run**.

### Implemented and tested
- EAK-IR v1 (`eak-core`): project, library, schematic, PCB, constraints, manufacturing config;
  integer-nanometre geometry; byte-stable JSON; schema version gate (newer/corrupt files are
  rejected safely); integrity checker; built-in library of 48 components (see below).
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

### Built-in component library (`eak-core/src/builtin.rs`, 48 parts)
- Generic parts (IPC-7351 nominal land patterns, approximate; every rating `verified: false`,
  source "unverified"):
  - R, C (MLCC) and L in 0402, 0603, 0805, 1206 and 1210.
  - LEDs in 0603 and 0805; diodes in SOD-123 and SMA.
  - NPN, PNP, N-MOSFET and P-MOSFET in SOT-23; a SOT-23-5 LDO (VIN/GND/EN/NC/VOUT).
  - 1x02 to 1x10 2.54 mm pin headers; crystals in HC-49/US and 3225; a 6x6 mm tactile switch.
- Parts for the two validation boards. Pinouts and land patterns come from the named manufacturer
  datasheets, and each part's provenance names its source. Ratings carry the datasheet as their
  source but stay `verified: false`, because EAK has not independently reviewed them.
  - Espressif ESP32-C3-MINI-1.
  - TI LMR33630 3 A synchronous buck (HSOIC-8 PowerPAD).
  - Sensirion SHT40.
  - AOS AO4406A SO-8 N-MOSFET.
  - Power inductors: Bourns SRP1038A (Isat 12 A), Würth WE-PD 7345 and WE-LQS 5040 (both saturate
    below 3 A).
  - Panasonic-land SMD electrolytics (6.3 mm and 8 mm).
  - Phoenix MKDS 5.08 mm 2-pin screw terminal.
  - The 8 M1 parts are pinned to a golden fixture, so their definitions cannot change silently.
- Tests check, for every part:
  - unique ids;
  - every symbol pin maps to a pad and every pad is mapped;
  - no overlapping pads;
  - pads lie inside the courtyard;
  - no rating claims verification.
- Known limits:
  - **No USB-C receptacle yet.** Its shell tabs and locating pegs need slotted and non-plated holes,
    which `FootprintDef` cannot express. This needs an IR type change.
  - Antenna and sensor copper keep-outs, and paste-window sizes, are noted in provenance only; they
    are not modelled.
  - AO4406A pin numbers follow the standard SO-8 assignment; they were not read from the drawing.
  - Generic part ratings are typical class values, not datasheet facts.

### Tests executed (exact results)
`cargo test --workspace --release`: **39 passed, 0 failed** (eak-core 11, eak-engineering 11,
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
- **Desktop application** (Tauri 2 + React): not started. There is no GUI yet.
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
1. Tauri 2 desktop shell + React workspace bound to the command engine (Phase 1/4).
2. Schematic and PCB editor canvases driving the same commands.
3. Intelligence Engine: provider abstraction, typed tools over commands, approval flow.
