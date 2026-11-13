# READINESS — evidence log for the ESP32-C3 Environment Dashboard

This log records what was actually run, with what result, and what remains unverified. It
makes no readiness claim and does not score itself. **No physical hardware was built,
flashed or measured.** Every firmware and dashboard result below is either a host
unit/interface test or a simulation, and is labelled as such.

**Run environment:** Windows 11, Git Bash, Rust 1.99.1 (`x86_64-pc-windows-gnu`, target
`riscv32imc-unknown-none-elf`), Node 24.21.0 / npm 11.19.0 (dashboard UI build only), EAK at `bedd9b5` (M1), 2026-11-13.

**Status values**
- **PASS** — the command ran and its checks passed.
- **FAIL** — the command ran and a check failed.
- **NOT VERIFIED** — not tested, or not testable with the available tools or hardware.
- **PASS (unverified inputs)** — the computation passed, but some inputs are assumptions rather than values read from a datasheet.

## 1. Design authored through EAK

| # | Test | Command | Result | Evidence | Limitations |
|---|---|---|---|---|---|
| D1 | Board authored only through `Engine::execute` typed commands | `cd design && cargo run --release` | PASS: 222 commands, project revision 221 | `project/journal.jsonl`, `outputs/reports/build-log.json` | Driven through the Rust API: EAK has no CLI binary yet (gap G4). |
| D2 | Native project re-opens: parses, migrates, passes integrity | `cargo test --release` → `committed_project_matches_a_fresh_build` (via `eak_core::from_json`) | PASS | `project/project.eak.json` | — |
| D3 | Rebuilding gives the same connectivity and placement as the committed project | same test | PASS | — | Auto-routed copper differs between rebuilds. EAK iterates objects in random-id order, so routed paths are not reproducible, although every rebuild is fully routed (asserted). |
| D4 | Custom component definitions pass EAK library validation | `parts::tests::definitions_pass_eak_library_validation` | PASS | `design/src/parts.rs` | See §5 for geometry provenance. |

## 2. Electrical checks (EAK)

| # | Test | Command | Result | Evidence | Limitations |
|---|---|---|---|---|---|
| E1 | EAK ERC (13 rules) | `design` build / `erc_drc_mech_report_no_errors_and_router_completes` | PASS: 0 errors, 0 warnings | `outputs/reports/erc.json` | ERC-010 (decoupling) keys on supply pin names. The module's supply pin is named "3V3" (datasheet), so ERC-010 does **not** check it; decoupling was reviewed by hand (C3/C4 at the module, C2 at the LDO). The ERC report lists its own not-implemented checks (pin-compatibility matrix, simulation, derating). |
| E2 | Firmware pin map = EAK netlist = datasheet pin names | `firmware_pin_map_matches_the_eak_netlist` | PASS | `firmware/dashboard-core/src/pins.rs`, `outputs/reports/netlist.json` | Checks connectivity and resistor values, not electrical behaviour. |
| E3 | PCB pad nets = schematic (forward annotation complete) | `pcb_pad_nets_follow_the_schematic` | PASS | — | — |
| E4 | Engineering calculations, computed with `eak-engineering` | `design` build | PASS for track width/capacity, EN RC, divider current, SHT40 supply and logic level. PASS (unverified inputs) for LDO dissipation/Tj, dropout headroom, 3V3 range, VBUS-sense range, LED, I2C rise time and sink current. | `outputs/reports/engineering.json` | AMS1117 values (dropout, Iq, θJA, Tj max) are assumptions: its datasheet could not be fetched. The ESP32 DC levels (VIH/VOL/IOH) come from a misaligned text extraction. I2C bus capacitance (100 pF) is assumed. |
| E5 | SPICE / analog simulation | — | NOT VERIFIED | — | EAK has no circuit simulator; none was used. |
| E6 | USB 2.0 signal integrity / 90 Ω differential impedance | — | NOT VERIFIED | — | EAK DRC lists impedance as not implemented. The D± pair is about 45 mm of 0.2 mm track with no impedance control (full-speed 12 Mbit/s). |

## 3. PCB, mechanical and manufacturing checks

| # | Test | Command | Result | Evidence | Limitations |
|---|---|---|---|---|---|
| P1 | Routing complete | `design` build | PASS: AutoRoute 62/62 connections, 0 failed; DRC-004 none | `outputs/reports/summary.txt` | The USB-C escape and the D±/CC/VBUS long runs were hand-drawn with `AddTrack` (gap G1). |
| P2 | EAK DRC (13 rules) | `design` build | PASS: 0 errors, 2 warnings (DRC-013 isolated GND-zone islands) | `outputs/reports/drc.json` | Not implemented in EAK: impedance, thermal reliefs, creepage, silkscreen/mask/paste checks, copper-to-hole and hole-to-hole spacing, acid traps. Dead-copper islands remain in the Gerbers (gap G6). |
| P3 | EAK MECH (height limit 12 mm, body collisions, model presence) | `design` build | PASS: 0 errors, 0 warnings | `outputs/reports/mech.json` | Bodies are parametric boxes. No enclosure or STEP model. |
| P4 | Antenna keep-out | manual placement (module antenna overhangs board edge by 5.5 mm) | NOT VERIFIED by a tool | `design/src/lib.rs` `build_pcb` | EAK has no keep-out object or rule (gap G5). The Espressif hardware design guideline was not read. |
| P5 | Manufacturing compile and package re-verification | `manufacturing_package_compiles_and_reverifies`, `committed_reports_are_bound_to_the_committed_revision` | PASS: EAK status `INTERNALLY_VERIFIED_RELEASE_CANDIDATE`; 12 files; SHA-256 re-verification clean | `outputs/manufacturing/manifest.json` | EAK's own statement: not a manufacturing-ready claim. |
| P6 | Package is fabricable as-is | — | **FAIL / NOT VERIFIED** | `outputs/manufacturing/*-PTH.drl` | The USB4105 shell needs **slots** (0.6 × 1.7 and 0.6 × 1.4 mm), but EAK can only emit round holes, so they are drilled as 0.6 mm circles (gap G2). The connector will not seat. |
| P7 | Gerber/drill viewer or fabricator DFM review | — | NOT VERIFIED | — | No Gerber viewer was run and no fabricator review was done. |
| P8 | BOM / CPL suitability for assembly | inspection | NOT VERIFIED | `outputs/manufacturing/*-BOM.csv`, `*-CPL.csv` | Passives have no MPN/LCSC numbers. Mounting holes and THT parts appear in the CPL (EAK has no DNP/assembly flag, gap G7). CPL rotation convention not checked against a specific assembler. |
| P9 | Silkscreen | inspection | NOT VERIFIED | `*-F_Silkscreen.gto` | EAK emits courtyard outlines only: no reference designators (gap G8). The module outline extends past the board edge. |
| P10 | Thermal | `engineering.json` first-order estimates | PASS (unverified inputs): about 50 °C (shipped firmware) and about 106 °C (Wi-Fi TX continuously at peak) LDO junction, at 40 °C ambient | `outputs/reports/engineering.json` | No thermal simulation exists in EAK. θJA is assumed. |
| P11 | Components sourced through EAK research / LCSC / JLCPCB path (datasheets, footprints, ratings, LCSC part numbers, JLCPCB-ready BOM/CPL) | — | NOT VERIFIED | — | Capability not on `main` (checked 2026-11-13 12:10 UTC at `bedd9b5`): EAK has no research browser, LCSC or JLCPCB integration (gap G9). Datasheets were fetched manually outside EAK; LCSC numbers are absent except the built-in AMS1117 entry (C6186, not re-verified). |

## 4. Firmware and dashboard

| # | Test | Command | Result | Evidence | Limitations |
|---|---|---|---|---|---|
| F1 | no_std core unit tests: CRC-8 datasheet vector (0xBEEF→0x92), SHT4x conversions, frame protocol, dashboard model, pin map, SSD1306 byte stream and rendering | `cd firmware && cargo test --release` | PASS: 19/19 | `firmware/dashboard-core/src/*.rs` | Host tests. |
| F2 | SIMULATED end-to-end: firmware `Sampler` → byte-level SHT40 model (NACK while converting, CRC) → frames → parser → model → HTTP API, compared with simulator ground truth | `dashboard-host` tests `end_to_end.rs` | PASS: 4/4 (clean link, all 30 frames; |ΔT| < 0.06 °C, |ΔRH| < 0.1 %; with fault injection, every frame is accounted for and faults never become values; UI bundle and assets served, traversal/hidden/unknown paths 404, POST 405, CSP without `unsafe-inline`; missing bundle gives 503 while the API works). Plus 4/4 `dashboard-host` unit tests (static path mapping refuses traversal) | `firmware/dashboard-host/tests/end_to_end.rs` | **Simulation.** The SHT40 model is written from the datasheet protocol, not from silicon. |
| F3 | Device firmware builds for ESP32-C3 (`riscv32imc-unknown-none-elf`, esp-hal 1.2.2, no ESP-IDF) | `cd firmware/esp32c3 && cargo build --release` | PASS: ELF with `esp_app_desc`, .text 25 KB | `firmware/esp32c3/src/main.rs` | Firmware pins are compile-time asserted against `pins.rs`. |
| F4 | Firmware runs on hardware (flash, boot, USB enumeration, I2C to a real SHT40/OLED, ADC accuracy) | — | NOT VERIFIED | — | No hardware available. `espflash` not run. |
| F5 | Live React + TypeScript dashboard in a browser | `cd dashboard-ui && npm ci && npm run build`, then `cd firmware && cargo run --release --bin dashboard -- --simulate --faults`, opened in Chrome (chrome-devtools-axi) at 1280×1100 | PASS (simulated data): values, statistics, link counters (CRC-rejected frame, sequence gap, sensor faults), alarms with the last link error, temperature and humidity history with gaps for failed reads, OLED mirror and the SIMULATED banner all update every second. The browser console is empty: no CSP violations and no failed requests. Network: index, JS and CSS assets, /api/state, /api/history and /display.txt all 200 | screenshot kept outside the repo by the task runner | Simulated data path. The `--serial` path is NOT VERIFIED against a device. |
| F6 | SSD1306 init sequence correct for real modules | — | NOT VERIFIED | `firmware/dashboard-core/src/display.rs` | Common sequence; not checked against the Solomon Systech datasheet or a module. |
| F7 | Lint and format | `cargo fmt --check` + `cargo clippy --all-targets -D warnings` in `design/`, `firmware/`, `firmware/esp32c3/` | PASS | — | — |
| F8 | Python-free | `bash scripts/check-no-python.sh` | PASS | — | — |
| F9 | Dashboard UI type-check and production build | `cd dashboard-ui && npm ci && npm run build` (`tsc --noEmit` strict, then `vite build`); then `cargo test --release -p dashboard-host --test end_to_end -- --ignored` serves the real bundle | PASS: 0 type errors; bundle = index.html + 1 JS (224.6 kB, 70.6 kB gzip) + 1 CSS + favicon; the ignored real-bundle test passes 1/1 | `dashboard-ui/src/*.tsx` | There are no UI unit tests: a test runner would be a further dependency that was not requested. UI behaviour is covered by F5 (browser) and the API tests in F2. |

## 5. Datasheet provenance

| Part | Source read in this project | Verified values | Not verified |
|---|---|---|---|
| ESP32-C3-WROOM-02 | Espressif datasheet v1.7 (documentation.espressif.com) | Pin table 3-1; VDD 3.0–3.6 V, abs max 3.6 V, ≥ 0.5 A supply; 345 mA peak TX; 28 mA at 160 MHz; strapping table 4-3; EN RC 10 k / 1 µF; land pads 1.5 × 0.9 mm on 1.5 mm pitch, rows 17.5 mm apart; module 18 × 20 × 3.2 mm | Pin-1 row offset (6.4 mm from the antenna edge) and EPAD position/shape. Read from drawing text; the EPAD is modelled as one 2.9 mm pad, while the drawing shows a 3 × 3 array. **Check against the Espressif footprint file before fabrication.** Paste on the EPAD is a full pad (the datasheet warns about excess paste). |
| SHT40 | Sensirion SHT4x datasheet v7.3 | Address 0x44, command 0xFD, 6-byte response, CRC-8 parameters and example, conversion formulas, tMEAS,h max 8.3 ms, VDD 1.08–3.6 V, VIH 0.7·VDD | Breakout-board pinout on J2 (header order is this project's own definition). |
| USB4105-GF-A | GCT drawing rev B4 | Land order, 0.30 / 0.60 mm widths, 0.50 mm pitch, 1.15 mm length | Shell-tab positions (read from drawing text), body height, contact current. |
| AMS1117-3.3 | Not retrievable (403 / TLS failure) | — | Every rating. EAK built-in values and assumptions are used and marked. |
| Tact switch, LED, passives | None | — | Generic patterns and values. |

## 6. EAK capability gaps found (reported to Firstmate as `blocked:` status lines)

| ID | Gap | Owner | Effect here |
|---|---|---|---|
| G1 | The auto-router inflates all copper by the worst-case class clearance plus half the widest track, and anchors the ratsnest at pad centres, so it cannot leave 0.5 mm-pitch lands. | eak-electronics (`eak-engineering/router.rs`) | USB-C escape and runs hand-drawn with `AddTrack`. |
| G2 | No slotted or oval drill in `PadDef` and no Excellon slot output. | eak-core (IR) + eak-electronics (manufacturing) | USB-C shell slots emitted as round holes, so the package is not fabricable (P6). |
| G3 | No schematic or PCB drawing export (SVG/PDF). | eak-electronics / eak-desktop | Schematic and layout exist only as native EAK-IR JSON plus Gerbers; there is no human-readable schematic sheet. |
| G4 | No `eak` CLI binary (`apps/cli` not in the workspace). | eak-core | Design driven by a Rust program over the public API. |
| G5 | No keep-out area object or rule. | eak-core + eak-electronics | Antenna keep-out handled by overhang only (P4). |
| G6 | Zone fill keeps isolated islands. | eak-electronics | Dead copper in both GND layers (DRC-013 warnings). |
| G7 | No DNP / not-assembled flag for BOM and CPL. | eak-electronics | Mounting holes listed for assembly (P8). |
| G8 | No reference-designator silkscreen text. | eak-electronics | Silkscreen has outlines only (P9). |
| G9 | No research / datasheet retrieval, LCSC part lookup or JLCPCB export path. | eak-electronics (libraries) + eak-intelligence | P11 NOT VERIFIED; parts not sourced through EAK. |

## 7. Known design limitations (revision A)

- No USB ESD protection and no VBUS polyfuse.
- AMS1117 dropout margin at USB minimum VBUS is thin (1.45 V headroom against about 1.3 V assumed dropout). A low-dropout regulator is recommended.
- The SHT40 is off-board on a header. Its self-heating and placement relative to the board are not characterised.
- The telemetry link is USB only; Wi-Fi/BLE are not used by the firmware.
- The mounting holes are plated and tied to GND.

## 8. Stack compliance (captain's frozen stack, received 2026-11-13 12:07 UTC)

| Item | Status |
|---|---|
| Rust only, no Python, no Node/TS backend service | Compliant (`scripts/check-no-python.sh` PASS). Node is used only as the frontend build tool; at run time the Rust `dashboard-host` serves the static bundle. |
| New dependencies: `esp-hal`, `esp-bootloader-esp-idf`, `embedded-hal`, `nb` (and `serde_json`, already an EAK workspace dependency) | **Approved by the captain (2026-11-13 13:21 UTC), only for the `validation/esp32-dashboard` firmware.** They live in this project's own Cargo workspaces and are never an EAK core dependency. |
| Dashboard UI | **Compliant. React + TypeScript + Vite served by the Rust host, approved by the captain (2026-11-13 13:21 UTC) only for this acceptance project.** The UI lives in `dashboard-ui/`, separate from the EAK desktop app. The earlier inline-JavaScript page has been removed. npm dependencies are pinned exactly in `package.json`/`package-lock.json`: `react`, `react-dom` 19.3.1; dev: `typescript` 7.0.2, `vite` 8.3.8, `@vitejs/plugin-react` 6.3.0, `@types/react`, `@types/react-dom` 19.3.0. |
