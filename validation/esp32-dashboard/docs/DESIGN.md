# ESP32-C3 Environment Dashboard — design description

This is the design record for the board and software in this folder. Every hardware object
(symbols, labels, footprints, tracks, zones, rules) was created by `design/` through
`eak_runtime::Engine::execute` typed commands. The native project is `project/project.eak.json`,
and its command journal is `project/journal.jsonl` (222 commands, revision 221).
Verification evidence and its limits are in [`../READINESS.md`](../READINESS.md).

## 1. Requirements

The same text is stored in the EAK project (`AddRequirement`):

| ID | Requirement |
|---|---|
| R1 | Power from one USB Type-C 5 V input (USB 2.0 default power, ≤ 500 mA), with 5.1 kΩ CC pull-downs (sink). |
| R2 | 3.3 V ± 5 % logic rail for the ESP32-C3 module (datasheet range 3.0–3.6 V) that can supply ≥ 0.5 A (datasheet Table 6-2). |
| R3 | ESP32-C3 (RISC-V) module. Its on-chip USB Serial/JTAG is used for both programming and the telemetry link (no USB-UART bridge). |
| R4 | SHT40 temperature/humidity sensor (I2C address 0x44) on an external 4-pin header, away from the heat of the module and LDO. VBUS voltage monitor on ADC1. |
| R5 | Local UI: status LED, BOOT/user button (GPIO9), reset button (EN), and a 4-pin header for a 0.96" SSD1306 I2C OLED. |
| R6 | Firmware streams CRC-protected telemetry frames at 1 Hz. A host dashboard shows live values, history, statistics, link health and alarms. |
| R7 | Mechanical: 52 × 44 mm, 2-layer FR-4, 1.6 mm. Four M3 mounting holes. The module antenna overhangs the board edge. Maximum component height 12 mm. |
| R8 | Fabrication: generic low-cost 2-layer rules (0.15 mm track/space, 0.3 mm drill), with GND pours on both layers. |

## 2. Architecture

```
USB-C J1 ──VBUS──► AMS1117-3.3 U2 ──+3V3──► ESP32-C3-WROOM-02 U1 ──I2C(IO4/IO5)──► J2 SHT40 breakout
   │  CC1/CC2: 5.1k R1/R2                     │   │   │              └──────────► J3 SSD1306 OLED
   │  D+/D- ──────────────────────────────────┘   │   └─ IO10 ─► R7 ─► D1 status LED
   └─VBUS─► R10 100k / R11 47k ─► IO3 (ADC1_CH3)  └─ IO9 BOOT/user button SW2, EN RC + reset SW1

Firmware (esp32c3/) ── uses ── dashboard-core (no_std: pins, SHT4x driver, frame protocol, model, OLED)
      │ USB Serial/JTAG CDC: "$EAKD,1,seq,uptime,T,RH,VBUS,btn,faults*CRC\r\n" at 1 Hz
      ▼
dashboard-host (std): serial reader or simulated firmware ─► Dashboard model ─► JSON API + static UI
      │ GET /api/state, /api/history, /display.txt (1 Hz poll)
      ▼
dashboard-ui (React + TypeScript, Vite build → static dist/ served by dashboard-host)
```

There is a single source of truth for pins:
[`firmware/dashboard-core/src/pins.rs`](../firmware/dashboard-core/src/pins.rs).
- The design driver labels the schematic from it.
- The firmware checks its peripheral GPIOs against it at compile time (`const` asserts).
- `design/tests/design.rs::firmware_pin_map_matches_the_eak_netlist` re-derives the connectivity from the saved EAK project and compares it with the pin map and with the datasheet pin names.

## 3. Component selection

| Ref | Part | Why | Source of key values |
|---|---|---|---|
| U1 | Espressif ESP32-C3-WROOM-02-N4 | RISC-V core, so the stock Rust target and esp-hal build without ESP-IDF or Python. Native USB Serial/JTAG. Pre-certified module with PCB antenna. 1.5 mm-pitch castellated pads. | ESP32-C3-WROOM-02 datasheet v1.7: Table 3-1 (pins), Tables 6-1/6-2/6-4/6-5 (ratings, currents), Figures 10-1/11-1 (geometry) |
| U2 | AMS1117-3.3 (EAK built-in library) | Rated ≥ 0.8 A (module needs ≥ 0.5 A of supply capability). Reused from EAK's library. | The datasheet could **not** be fetched, so its ratings are marked unverified. A lower-dropout part is recommended for revision B (§6). |
| J1 | GCT USB4105-GF-A | USB 2.0 Type-C receptacle, SMT. | GCT drawing rev B4: land order, widths, pitch |
| J2 | 1×4 2.54 mm header | Connects the SHT40 breakout. Keeps the sensor away from module and LDO heat. Avoids the DFN-4 land pattern, which is only a figure in the datasheet. | SHT4x datasheet v7.3 (protocol, VDD range) |
| J3 | 1×4 2.54 mm header | Common GND-VCC-SCL-SDA SSD1306 OLED module. | Module pin order: common market convention (not verified) |
| R1/R2 | 5.1 kΩ | USB Type-C sink Rd on CC1/CC2 | USB Type-C spec (not re-read in this project) |
| R3, C5 | 10 kΩ, 1 µF | EN RC delay | ESP32-C3-WROOM-02 datasheet v1.7, section 8 note (R = 10 k, C = 1 µF) |
| R4/R5/R6 | 10 kΩ | Pull-ups on GPIO9 (BOOT/user), GPIO8 and GPIO2 strapping pins | Datasheet Table 4-3 (SPI boot needs GPIO9 = 1, GPIO8 = 1 for download, GPIO2 pull-up recommended) |
| R8/R9 | 4.7 kΩ | I2C pull-ups (100 kHz) | See `outputs/reports/engineering.json` |
| R10/R11, C6 | 100 kΩ / 47 kΩ, 100 nF | VBUS sense: 5.25 V → 1.68 V at the ADC pin | `engineering.json` |
| R7, D1 | 1 kΩ, green 0603 LED | About 1.3 mA status LED | `engineering.json` |
| C1/C2/C3/C4 | 10 µF / 22 µF / 10 µF / 100 nF | LDO input/output and module decoupling (10 µF + 0.1 µF, per the module datasheet reference) | |
| SW1/SW2 | 6×6 mm THT tact switch | Reset, BOOT/user | Generic pattern (not verified) |
| H1–H4 | M3 plated mounting holes | Mechanical | ISO 273 medium-fit clearance hole |

## 4. Pin assignment and interface agreement

| Signal | GPIO | Module pin | Net | Electrical agreement |
|---|---|---|---|---|
| I2C SDA | 4 | 3 (IO4) | I2C_SDA | Open drain, 4.7 kΩ to +3V3. The SHT40 (VDD 1.08–3.6 V) and the OLED both run on +3V3, so there is no level shifting. VIH(SHT40) = 0.7·VDD = 2.31 V < 3.3 V. |
| I2C SCL | 5 | 4 (IO5) | I2C_SCL | Same as SDA. The firmware runs at 100 kHz (`pins::I2C_FREQ_HZ`). |
| Status LED | 10 | 10 (IO10) | LED_DRV | Push-pull, about 1.3 mA through R7. |
| BOOT/user button | 9 | 8 (IO9) | BOOT_BTN | Active low. 10 kΩ pull-up keeps SPI boot (strapping value 1). |
| VBUS sense | 3 | 15 (IO3, ADC1_CH3) | VBUS_SENSE | ≤ 1.68 V at 5.25 V VBUS, inside the 11 dB attenuation range the firmware uses. |
| USB D− / D+ | 18 / 19 | 13 / 14 | USB_DM / USB_DP | USB Serial/JTAG PHY wired directly to the receptacle. Both plug orientations are joined (A6/B6, A7/B7). |
| Strap GPIO2 / GPIO8 | 2 / 8 | 16 / 7 | STRAP_IO2 / STRAP_IO8 | 10 kΩ pull-ups (Table 4-3). |
| EN | — | 2 | EN | 10 kΩ / 1 µF RC; reset switch to GND. |
| Not used (no-connect) | 0, 1, 6, 7, 20, 21 | 18, 17, 5, 6, 11, 12 | — | Marked no-connect in the schematic. |

Protocol agreement:
- SHT40: address 0x44, command 0xFD, 6-byte response with CRC-8 (polynomial 0x31, init 0xFF). The firmware waits 10 ms (datasheet maximum 8.3 ms).
- SSD1306: address 0x3C.
- Telemetry: `dashboard_core::frame`. The firmware and the host share the same encoder and parser.

## 5. PCB

- **Board.** 52 × 44 mm, 2 layers, 1.6 mm FR-4, 1 oz.
- **Rules.**
  - Minimum track and clearance 0.15 mm; minimum drill 0.3 mm; annular ring ≥ 0.13 mm; 0.3 mm edge clearance.
  - Net classes: Default (0.25 mm track / 0.2 mm clearance), Power (VBUS, +3V3, GND: 0.30 mm track / 0.2 mm clearance), USB (D±, CC: 0.2 mm track / 0.15 mm clearance).
- **Placement.** Every part was placed by hand with `MoveFootprint` and then locked.
  - The module antenna overhangs the bottom edge by 5.5 mm. Only 0.5 mm of the 6 mm antenna area is over the board, inside the edge clearance, so no copper lies under it. This follows the module datasheet's "keepout zone" figure. The Espressif hardware design guideline itself was not read.
  - The USB-C receptacle sits on the top edge.
  - Headers sit on the left and right edges, buttons in the lower corners, and passives in the open area above the module.
- **Routing.**
  - The USB-C fine-pitch escape and the long runs of D±, CC1/CC2 and VBUS were drawn as `AddTrack` commands (`usb_escape` in `design/src/lib.rs`). EAK's auto-router cannot start at a 0.5 mm-pitch land (gap G1).
  - Everything else (62 connections) was routed by EAK's native `AutoRoute`, with 0 failures.
  - Totals: 363 tracks and 36 vias.
- **Copper.** GND zones on both layers with 0.3 mm clearance. DRC-013 reports isolated islands (dead copper) in both zones. That is a warning only; EAK has no island removal (gap G6).

## 6. Power and thermal (first-order; see `outputs/reports/engineering.json`)

| Case | Load | P(LDO) | Tj at 40 °C ambient |
|---|---|---|---|
| Shipped firmware (USB telemetry, Wi-Fi off) | 28 mA module (Table 6-5) + 4.3 mA | 0.12 W | about 50 °C |
| Worst case: Wi-Fi TX continuously at peak current | 345 mA (Table 6-4) + 4.3 mA | 0.73 W | about 106 °C |

θJA = 90 °C/W and Tj(max) = 125 °C are **assumptions**, because the AMS1117 datasheet could not be retrieved.

Dropout headroom at VBUS = 4.75 V is 1.45 V, against an assumed maximum dropout of 1.3 V. The margin is thin. **Recommendation for revision B:** use a 3.3 V LDO with ≤ 0.4 V dropout at 500 mA, and add USB ESD protection (e.g. a TVS array on D±/VBUS) and a VBUS polyfuse. This revision has neither (see READINESS limitations).

## 7. Mechanical

- **Outline and holes.** 52 × 44 mm outline. M3 holes at (3.5, 3.5), (48.5, 3.5), (3.5, 40.5) and (48.5, 40.5) mm.
- **Heights.** The tallest parts are the headers at 8.5 mm, under the 12 mm limit (EAK MECH-001).
- **Collisions.** EAK's MECH check found no 3D body collisions, using parametric boxes.
- **Not checked.** There is no enclosure model.

## 8. Firmware and software

- **`firmware/dashboard-core`** (no_std, no alloc): pin map, SHT4x driver over `embedded-hal` 1.0, CRC-8, telemetry frame encoder/parser, dashboard model (240-sample ring, statistics, alarms, sequence-gap detection), SSD1306 renderer and driver.
  - A sensor failure produces `NA` fields and increments a fault counter. Old values are never repeated.
- **`firmware/esp32c3`**: esp-hal 1.2 bare-metal binary for `riscv32imc-unknown-none-elf`, with an ESP-IDF app descriptor from `esp-bootloader-esp-idf`.
  - It samples at 1 Hz and writes frames to USB Serial/JTAG with a bounded spin budget, so a missing host cannot stall sampling.
  - It drives the status LED and the optional OLED.
- **`firmware/dashboard-host`**:
  - `dashboard --serial <device>` reads the real board.
  - `dashboard --simulate [--faults]` runs the same `Sampler` code against a byte-level SHT40 model, with optional NACK, CRC and line-noise fault injection.
  - Both modes serve `http://127.0.0.1:8080/`.
    - The JSON API: `/api/state`, `/api/history` and `/display.txt`. `/display.txt` is the OLED frame buffer, drawn by the firmware's own renderer.
    - The built dashboard page from `dashboard-ui/dist`, which `--ui-dir` can override.
  - Only GET is served. Static paths are limited to at most two safe segments with known file types, so there is no traversal.
  - The Content-Security-Policy allows only same-origin script and style, with no inline code.
  - If the page has not been built, `/` answers 503 with build instructions, while the API keeps working.
  - Simulated data is labelled SIMULATED in the UI and in the API.
- **`dashboard-ui`**: React 19 + TypeScript (strict) + Vite. Approved by the captain for this acceptance project only; it is kept separate from the EAK desktop app.
  - It polls the API once a second.
  - It shows the latest values with statistics, link health (CRC-rejected frames, sequence gaps, sensor faults), alarms, and per-quantity history charts with the alarm threshold. A failed sensor read appears as a gap in the line; it is never interpolated.
  - It also shows a canvas mirror of the OLED.
  - There is no Node server: `npm run build` (`tsc --noEmit` then `vite build`) writes static files only.
