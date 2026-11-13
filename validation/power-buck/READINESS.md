# Power-buck board: readiness evidence log

This log records what was actually run, with what result, and what is **NOT VERIFIED**. It
is not a self-assessment score; an independent verifier will grade it. Nothing here claims
manufacturing readiness, physical performance, thermal compliance or isolation safety.

Conventions:
- `PASS (EAK)`: an EAK check ran and passed.
- `PASS (calc)`: an EAK calculation met its stated requirement.
- `PASS (independent)`: a non-EAK reviewer tool ran and passed.
- `FAIL`: the check failed.
- `NOT VERIFIED`: no evidence exists.
- `PARTIAL`: some evidence exists, with a stated gap.
- `INFO`: recorded for reference, no pass/fail.

All commands run from the repository root. Platform: Windows 11, Rust 1.99.1
(`x86_64-pc-windows-gnu`), 2026-11-13. The design revision is in `reports/summary.txt`
(217 at the time of writing).

## A. Authoring through EAK

| # | Test | Command | Result | Evidence | Limitations |
|---|---|---|---|---|---|
| A1 | Whole design authored through `Engine::execute` typed commands (requirements, library definitions, schematic, rules, placement, routing, vias, pours) | `cargo run --release --manifest-path validation/power-buck/Cargo.toml` | PASS: 217 commands, each journalled as pending then committed (434 journal lines); a rejected command aborts the build | `project/journal.jsonl`, `project/project.eak.json`, `src/design.rs` | Driven by a Rust program, because EAK has no CLI yet (GAPS G7). The desktop UI is not exercised |
| A2 | Native project re-opens with schema and integrity checks | Implicit: the engine writes through `store::write_project` and the integrity gate on every commit | PASS (EAK) | `project/project.eak.json` | A separate `Engine::open` round-trip test was not added: NOT VERIFIED |
| A3 | Unit/regression tests of the validation crate | `cargo test --release --manifest-path validation/power-buck/Cargo.toml` | PASS: 5 passed, 0 failed | `src/lib.rs`, `src/parts.rs` tests | Windows only. Not part of workspace CI (stand-alone package), so Linux/macOS are NOT VERIFIED |
| A4 | Negative control: an undersized SW track (0.3 mm at 3.1 A) is rejected | Same as A3 (`injected_narrow_power_track_is_caught_by_drc`) | PASS (EAK DRC-008 error raised) | `src/lib.rs` | Shows that the current rule is live for this design, not merely configured |

## B. Schematic and electrical rules

| # | Test | Command | Result | Evidence | Limitations |
|---|---|---|---|---|---|
| B1 | ERC (13 rules) | A1 | PASS (EAK): 0 errors, 0 warnings | `reports/erc.json`, `project/evidence/erc-r000217.json` | ERC `not_implemented`: pin-compatibility matrix, hierarchy, analog simulation, stress beyond capacitor voltage |
| B2 | Netlist matches the intended circuit (10 nets) | A1 | PASS by manual review of `reports/netlist.txt` against the DESIGN.md circuit | `reports/netlist.txt` | Manual review by the author, not independent |
| B3 | Capacitor voltage ratings vs declared rail voltages (ERC-011) | A1 | PASS (EAK) | `reports/erc.json` | Capacitor ratings themselves are UNVERIFIED (datasheets not read) |
| B4 | Schematic drawing | none | NOT VERIFIED: EAK cannot render or export a schematic (GAPS G5) | none | The schematic exists as EAK-IR symbols plus labels only |

## C. Electrical design calculations

| # | Test | Command | Result | Evidence | Limitations |
|---|---|---|---|---|---|
| C1 | Output set point and tolerance | A1 (`calc::divider_vout`) | PASS (calc): 4.972 V typ, 4.763-5.188 V worst case | `reports/engineering-calcs.md` | Excludes ripple, load/line regulation and transients |
| C2 | Datasheet divider (100k/13.3k) re-checked | A1 history / DESIGN.md §3.4 | FAIL for that divider (worst case 5.298 V > 5.25 V). The design was changed to 105k/14.3k | DESIGN.md §3.4 | Real finding produced by the EAK calculation |
| C3 | Inductor ripple, peak and RMS current; margin to current limit | Hand calculation | NOT VERIFIED (by EAK; GAPS G1). Hand calc: I_peak 3.86 A worst vs 4.0 A ILIM min. That is a thin margin | DESIGN.md §3.1 | No EAK function exists |
| C4 | Output ripple, output-capacitor RMS current, load-step capacitance | Hand calculation | NOT VERIFIED (GAPS G1/G2). Hand calc: 22.5 mV worst; 33 µF effective (assumed) vs 30 µF minimum | DESIGN.md §3.2 | DC-bias derating assumed |
| C5 | Input-capacitor RMS current and input ripple | Hand calculation | NOT VERIFIED (GAPS G1). Hand calc: 1.49 A RMS, 0.12 V | DESIGN.md §3.3 | MLCC ripple ratings not read |
| C6 | Loop stability / phase margin | none | NOT VERIFIED | DESIGN.md §3.4 | Internal compensation; L/C match TI's example; no Bode data |
| C7 | Minimum on-time at max input | Hand calculation | NOT VERIFIED (min t_on not retrieved) | DESIGN.md §3.4 | |
| C8 | Feedback, LED and gate-resistor dissipation | A1 (`calc::resistor_power_w`, `led_resistor_ohm`) | PASS (calc): 0.18 mW, 5.1 mW, 1.7 mW | `reports/engineering-calcs.md` | Resistor 0.1 W rating UNVERIFIED |

## D. Component ratings and selection

| # | Test | Result | Evidence | Limitations |
|---|---|---|---|---|
| D1 | TPS54302 pinout, abs max, VFB, fsw, RDS(on), current limits, θJA, Tj, EN behaviour | Read from the TI datasheet SLVSDG6C (HTML viewer) in this session; recorded as `verified: true` with section citations | `src/parts.rs` `tps54302()`, DESIGN.md | Read by the authoring agent; not independently reviewed |
| D2 | Ratings of L1, C1-C7, Q1, D1, D2, F1, R1-R4, D3, J1/J2 | NOT VERIFIED: manufacturer PDFs could not be rendered in this session. Values are designer assumptions marked `verified: false` with the document to check | `src/parts.rs` | GAPS G10 (no EAK research/LCSC path) |
| D3 | Land patterns of every custom footprint vs manufacturer recommendations | NOT VERIFIED (generic IPC-7351-style approximations) | `src/parts.rs` provenance strings | |
| D4 | BOM completeness (MPN on every line) | PASS (independent) | `reports/independent-inspection.txt` (`bom-mpn: none`) | LCSC part numbers empty (GAPS G10) |

## E. Protection

| # | Test | Result | Evidence | Limitations |
|---|---|---|---|---|
| E1 | Reverse polarity (Q1 off, downstream never reverse biased) | NOT VERIFIED: circuit analysis only (DESIGN.md §5.2) | DESIGN.md §5.2 | No simulation or bench test. Q1 ratings unverified. |Vds| = 84 % of the assumed 30 V |
| E2 | Overvoltage / surge (TVS) | NOT VERIFIED. **No surge immunity is claimed**: the assumed TVS clamp of 42.1 V exceeds the U1 30 V abs max | DESIGN.md §5.3 | |
| E3 | Hot-plug overshoot | NOT VERIFIED | DESIGN.md §5.4 | Needs bench measurement |
| E4 | Fuse rating and I²t vs inrush | NOT VERIFIED (52 % steady loading by calculation; fuse data unread) | DESIGN.md §5.1 | |
| E5 | Output overload / short behaviour | NOT VERIFIED (datasheet current-limit values only) | DESIGN.md §5.5 | |

## F. PCB layout and physical verification

| # | Test | Command | Result | Evidence | Limitations |
|---|---|---|---|---|---|
| F1 | DRC (13 rules) incl. clearance vs declared voltages and IPC-2221 current width | A1 | PASS (EAK): 0 errors, 0 warnings | `reports/drc.json`, `project/evidence/drc-r000217.json` | DRC `not_implemented`: zone current density and thermal, creepage paths, copper-to-hole and hole-to-hole spacing, silk/mask/paste slivers, acid traps, impedance. Zone-to-zone clearance is not checked (GAPS G4; avoided because GND is the only pour net) |
| F2 | Physical connectivity: each net is one copper group, no shorts | A1 (`copper::analyse`); A3 | PASS (EAK): 10 nets × 1 group, 0 shorts | `reports/connectivity.json`, `reports/summary.txt` | Zone fill on a 0.1 mm raster |
| F3 | Trace width vs IPC-2221 for each declared current | A1 (`ipc::trace_width_mm`) | PASS (calc): VIN 0.60 ≥ 0.39 mm, SW 0.80 ≥ 0.72 mm, VOUT/GND 0.80 ≥ 0.69 mm | `reports/engineering-calcs.md` | IPC tables not verified against the standard text (EAK flag). Pours are not current-checked |
| F4 | Via current | A1 (`ipc::via_current_a`, `vias_required`) | PARTIAL: 5 vias are needed for 3 A. C_in and C_out clusters have 5 each; the U1 GND cluster has 3, supplemented by the top pour | `reports/engineering-calcs.md` | 20 µm plating assumed; pour current density NOT VERIFIED (GAPS G3) |
| F5 | Clearance at declared voltages (25.2 V and 30.2 V BOOT) | A1 (`ipc::clearance_mm`) + DRC-002 | PASS (calc + EAK) | `reports/engineering-calcs.md`, `reports/drc.json` | B2 table unverified vs standard |
| F6 | Creepage | A1 (`ipc::creepage_mm`) | PASS (calc) as a table look-up only; path analysis NOT VERIFIED | `reports/engineering-calcs.md` | GAPS G9 |
| F7 | Isolation boundary | none | N/A by design: no isolation barrier. The SELV/ES1 source assumption is NOT VERIFIED against standard text | DESIGN.md §6 | |
| F8 | Hot loop, SW node, return path, Kelvin feedback, ground plane continuity | Visual review of rendered Gerbers (`tools/render.sh`) + coordinates in `src/design.rs` | INFO: as designed (DESIGN.md §7). Loop inductance NOT VERIFIED | DESIGN.md §7 | No field solver; renders are reproducible but not committed |
| F9 | Pour islands / slivers | A1 (DRC-013 + island locator) + visual review | PASS (EAK): 0 isolated islands. 4 corner islands and a 1-cell sliver were found and fixed during design | `reports/summary.txt` | 0.1 mm raster |
| F10 | MECH: height limit 12 mm, 3D body collisions | A1 | PASS (EAK): 0 errors, 0 warnings | `reports/mech.json` | Parametric boxes; no enclosure; component heights UNVERIFIED |
| F11 | Thermal: U1 junction temperature at 25.2 V / 3 A / 40 °C | A1 (`calc::junction_temp_c`) | **FAIL** with JEDEC θJA (Tj ≈ 150.6 °C vs 125 °C design target; abs max 150 °C). PASS with TI-EVM θJA (Tj ≈ 93.2 °C). Overall **NOT VERIFIED** | `reports/engineering-calcs.md`, DESIGN.md §4 | Loss is a hand estimate; no PCB thermal model (GAPS G3); needs measurement |
| F12 | Mounting: 4 × M3, pour keep-out 3.67 mm radius | A1 (DRC-005/007) | PASS (EAK) for edge and courtyard rules | DESIGN.md §7 | Courtyard-vs-edge not checked (GAPS G8) |

## G. Manufacturing outputs

| # | Test | Command | Result | Evidence | Limitations |
|---|---|---|---|---|---|
| G1 | Package compiled; gates bound to the same revision | A1 (`eak_manufacturing::compile`) | PASS (EAK): status `INTERNALLY_VERIFIED_RELEASE_CANDIDATE`, 12 files | `outputs/manifest.json` | The manifest itself says this is not manufacturing-ready |
| G2 | Package re-verification (hash and syntax) | A1 (`verify_package`) | PASS (EAK): 0 problems | `reports/summary.txt` | |
| G3 | Independent inspection: Gerber structure, aperture definitions, extents on board, drill tool table, BOM qty = CPL rows, CPL on board, manifest SHA-256 via system `sha256sum` | `sh validation/power-buck/tools/inspect.sh validation/power-buck/outputs 66 33` | PASS (independent): 28 PASS, 0 FAIL; 32 plated holes (24 × 0.3, 4 × 1.3, 4 × 3.2 mm) | `reports/independent-inspection.txt` | Syntax subset only; not a full Gerber viewer or fabricator CAM |
| G4 | Visual inspection of copper, mask, paste and drill | `sh validation/power-buck/tools/render.sh validation/power-buck/outputs <dir>`, viewed in a browser | INFO: images reviewed by the author | Renders outside the repo (reproducible) | Not an independent reviewer |
| G5 | Silkscreen content | Visual (G4) | NOT VERIFIED / gap: courtyard outlines and pin-1 dots only, no reference designators (GAPS G6) | | |
| G6 | CPL rotation and origin conventions for a specific assembler | none | NOT VERIFIED (GAPS G10) | | |
| G7 | Fabricator DFM review | none | NOT VERIFIED | | |

## H. Physical performance (needs hardware, which is out of scope)

| # | Test | Result |
|---|---|---|
| H1 | Efficiency, input current | NOT VERIFIED |
| H2 | Output ripple, load-step and line transient | NOT VERIFIED |
| H3 | Thermal (case temperature at 3 A / 24 V / 40 °C) | NOT VERIFIED |
| H4 | Reverse polarity, hot-plug, surge, short circuit | NOT VERIFIED |
| H5 | EMI | NOT VERIFIED |
