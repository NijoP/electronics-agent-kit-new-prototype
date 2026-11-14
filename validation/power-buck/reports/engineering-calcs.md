# Engineering calculations (EAK-computed)

Project revision 217. IPC tables verified against the standard text: **false**.

| Item | EAK function | Inputs | Result | Requirement | Status |
|---|---|---|---|---|---|
| Trace width, net VIN_RAW | `eak_engineering::ipc::trace_width_mm` | I = 2 A, external, 2 oz, ΔT = 10 °C | required ≥ 0.393 mm; narrowest routed track 1.00 mm | every track on the net ≥ required width | PASS (calculation) |
| Trace width, net VIN_F | `eak_engineering::ipc::trace_width_mm` | I = 2 A, external, 2 oz, ΔT = 10 °C | required ≥ 0.393 mm; narrowest routed track 1.00 mm | every track on the net ≥ required width | PASS (calculation) |
| Trace width, net VIN | `eak_engineering::ipc::trace_width_mm` | I = 2 A, external, 2 oz, ΔT = 10 °C | required ≥ 0.393 mm; narrowest routed track 0.60 mm | every track on the net ≥ required width | PASS (calculation) |
| Trace width, net SW | `eak_engineering::ipc::trace_width_mm` | I = 3.1 A, external, 2 oz, ΔT = 10 °C | required ≥ 0.719 mm; narrowest routed track 0.80 mm | every track on the net ≥ required width | PASS (calculation) |
| Trace width, net VOUT | `eak_engineering::ipc::trace_width_mm` | I = 3 A, external, 2 oz, ΔT = 10 °C | required ≥ 0.687 mm; narrowest routed track 0.80 mm | every track on the net ≥ required width | PASS (calculation) |
| Trace width, net GND | `eak_engineering::ipc::trace_width_mm` | I = 3 A, external, 2 oz, ΔT = 10 °C | required ≥ 0.687 mm; narrowest routed track 0.80 mm | every track on the net ≥ required width | PASS (calculation) |
| Via current capacity (0.3 mm drill) | `eak_engineering::ipc::via_current_a / vias_required` | drill 11.8 mil, plating 0.79 mil (assumed), ΔT 10 °C, sharing derating 0.8, 3.0 A | 0.80 A per via; 5 vias needed for 3 A; placed: U1 GND 3, input caps 5, output caps 5 | each GND via cluster that carries the full 3 A return ≥ required count | PARTIAL: clusters below 5 vias rely on parallel top-layer GND pour (pour current density NOT checked by EAK) |
| Electrical clearance VIN/SW to GND, external uncoated | `eak_engineering::ipc::clearance_mm` | 25.2 V, B2 | required ≥ 0.10 mm; design minimum 0.20 mm | design clearance ≥ required | PASS (calculation) |
| Electrical clearance VIN/SW to GND, component leads | `eak_engineering::ipc::clearance_mm` | 25.2 V, A5 | required ≥ 0.13 mm; design minimum 0.20 mm | design clearance ≥ required | PASS (calculation) |
| Electrical clearance BOOT to GND, external uncoated | `eak_engineering::ipc::clearance_mm` | 30.2 V, B2 | required ≥ 0.60 mm; design minimum 0.60 mm | design clearance ≥ required | PASS (calculation) |
| Creepage (functional insulation only, no isolation barrier) | `eak_engineering::ipc::creepage_mm` | 30.2 V, pollution degree 2, material group IIIb (FR-4 assumed) | table value 0.04 mm; design minimum surface distance 0.2 mm | functional creepage ≥ table value (IEC 60664-1 table as transcribed; EAK flags the ≤50 V rows as unverified) | PASS (calculation) |
| Output voltage set point | `eak_engineering::calc::divider_vout` | R2 = 105 kΩ, R3 = 14.3 kΩ (±1 %), VFB = 0.581/0.596/0.611 V (TI SLVSDG6C §5.5) | divider ratio 0.11987; VOUT typ 4.972 V, worst-case 4.763-5.188 V | 5.0 V ±5 % (4.75-5.25 V) and below the 5.3 V declared for clearance/rating checks | PASS (calculation) |
| Feedback divider dissipation (R2) | `eak_engineering::calc::resistor_power_w` | I = 41.9 µA, R = 105 kΩ | 0.184 mW | ≤ 50 % of 0603 rating (Yageo RC0603 0.1 W, unverified) | PASS (calculation) |
| Power LED series resistor | `eak_engineering::calc::led_resistor_ohm / resistor_power_w` | VOUT 4.972 V, Vf 2.2 V (Kingbright APT1608SGC, unverified), target If 2 mA | 1386 Ω → 1.5 kΩ chosen → 1.85 mA; 5.1 mW in R4 | R4 ≤ 50 % of rating | PASS (calculation) |
| Q1 gate pull-down R1 dissipation (zener clamping at 12 V) | `eak_engineering::calc::resistor_power_w` | I = (25.2 V - 12 V) / 100 kΩ | 1.74 mW | ≤ 50 % of 0603 rating | PASS (calculation) |
| U1 junction temperature (JEDEC 2s2p θJA) | `eak_engineering::calc::junction_temp_c` | Ta 40 °C, P 0.93 W (hand estimate), θJA 118.9 °C/W (TI SLVSDG6C §5.4) | Tj ≈ 150.6 °C (abs max 150 °C, shutdown 160 °C) | Tj ≤ 125 °C design target (25 °C below abs max) | FAIL against 125 °C target (estimate) - board copper must perform like TI EVM; NOT VERIFIED |
| U1 junction temperature (TI EVM θJA) | `eak_engineering::calc::junction_temp_c` | Ta 40 °C, P 0.93 W (hand estimate), θJA 57.2 °C/W (TI SLVSDG6C §5.4) | Tj ≈ 93.2 °C (abs max 150 °C, shutdown 160 °C) | Tj ≤ 125 °C design target (25 °C below abs max) | PASS (estimate only - NOT VERIFIED without measurement) |
| Q1 reverse-polarity MOSFET dissipation and Tj | `eak_engineering::calc::resistor_power_w / junction_temp_c` | Iin 1.55 A (11.4 V, η 0.85 assumed), RDS(on) 0.014 Ω (unverified), θJA 75 °C/W (unverified) | P ≈ 34 mW, Tj ≈ 42.5 °C | Tj ≤ 125 °C | PASS (estimate on unverified ratings - NOT VERIFIED) |
| GND pour fill, Top layer | `eak_engineering::copper::fill_zone` | clearance 0.60 mm, 0.1 mm raster | 1522 mm² filled in 2 island(s) | informational | INFO |
| GND pour fill, Bottom layer | `eak_engineering::copper::fill_zone` | clearance 0.60 mm, 0.1 mm raster | 1870 mm² filled in 1 island(s) | informational | INFO |

## Not computable in EAK today (NOT VERIFIED by EAK)

- Switching-converter operating point (generic, topology-parameterised; this board is a buck): duty cycle, inductor ripple/peak/RMS current (eak-engineering)
- Output voltage ripple and output-capacitor RMS current (eak-engineering)
- Input-capacitor RMS ripple current and input voltage ripple (eak-engineering)
- MOSFET conduction + switching loss and converter efficiency (eak-engineering)
- Loop stability / phase margin (internally compensated part; datasheet-defined only)
- MLCC DC-bias capacitance derating (eak-engineering / library data)
- Copper-pour current density and PCB thermal (spreading) model (eak-engineering / eak-validation)
