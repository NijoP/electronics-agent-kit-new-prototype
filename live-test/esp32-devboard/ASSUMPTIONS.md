# Assumptions — ESP32 dev board

Every deviation from the board specification and every assumption, with the reason.

## Project / tooling
- A1. Design lives in EAK project `C:\eak-dev\esp32-devboard` (prj_6f642417cc134504), not the terminal's bound project, because of B-002.

## Electrical choices (spec deviations have reasons)
- A2. USB-UART = **CH340C** (SOP-16, internal oscillator), not CP2102N. Reason: neither is in the EAK library (B-004); every pin map must be entered by hand, and the CH340C SOP-16 pinout (1 GND, 2 TXD, 3 RXD, 4 V3, 5 UD+, 6 UD-, 7/8 NC, 9 CTS#, 10 DSR#, 11 RI#, 12 DCD#, 13 DTR#, 14 RTS#, 15 R232, 16 VCC) has no exposed pad and is lower-risk to enter without a vendor footprint. The spec allows CH340C. Powered from +3V3 with V3 tied to VCC (3.3 V operation per WCH datasheet), so TXD levels match the ESP32.
- A3. LDO = **AMS1117-3.3** (SOT-223, built-in EAK part, LCSC C6186, 1 A). Dropout up to 1.3 V: with VBUS 4.75 V minus ~0.4 V Schottky drop the headroom is marginal at full load (same compromise as Espressif DevKitC). Dissipation from EAK `calc_linear_regulator`: 0.875 W at 5.0 V/0.5 A, 0.864 W at 4.7 V/0.6 A; junction 92 °C at 40 °C ambient **assuming** θJA = 60 °C/W (agent-supplied, not computed by EAK).
- A4. Back-feed diode = SS14 (SMA, built-in D_SMA footprint).
- A5. User LED on GPIO4 (not a strapping pin). Power LED red, user LED green; series resistors 1 kΩ (≈1.3 mA). EN RC 10 kΩ + 1 µF, τ = 10 ms (EAK `calc_rc_time_constant`).
- A6. Header pinout: DevKitC order, but the six flash positions (SD2, SD3, CMD, CLK, SD0, SD1) are replaced by GND / 3V3 because those module pins are NC on WROOM-32E and GPIO6-11 must not be brought out. Strapping pins IO0/2/5/12/15 stay on the headers (labelled) but carry no on-board load other than the BOOT button / auto-boot on IO0.
- A7. Power trace widths from EAK `calc_copper_trace_width` (IPC-2221, 34.8 µm, ΔT 10 K, external): 1.0 A → 0.302 mm, 0.6 A → 0.149 mm. POWER class uses 0.5 mm, GND 0.4 mm (margin). IPC table itself unverified per EAK.
- A8. USB pair: 0.3 mm tracks, 0.15 mm gap target; EAK cannot compute or check 90 Ω (B-005). On 1.6 mm 2-layer FR-4 a true 90 Ω microstrip pair over a far bottom plane is not achievable at these widths; USB full-speed (12 Mbit/s) over < 15 mm is tolerant of this.

## Sourcing (all LCSC numbers entered from the agent's knowledge, NOT verified by EAK — B-003)
| Ref | Part | MPN | LCSC (unverified) | LCSC class (agent belief) |
|---|---|---|---|---|
| U1 | ESP32-WROOM-32E 4 MB | ESP32-WROOM-32E-N4 | C701341 | extended |
| U2 | 3.3 V LDO | AMS1117-3.3 | C6186 | basic |
| U3 | USB-UART | CH340C | C84681 | extended |
| U4 | USB ESD | USBLC6-2SC6 | C7519 | extended |
| J1 | USB-C 16P | TYPE-C-31-M-12 | C165948 | extended |
| D1 | Schottky 40 V 3 A | SS34 | C8678 | basic |
| Q1,Q2 | NPN | MMBT3904 | C20526 | basic |
| R 10k / 5.1k / 1k | 0603 1 % | 0603WAF1002T5E / 0603WAF5101T5E / 0603WAF1001T5E | C25804 / C23186 / C21190 | basic |
| C 100n / 1u / 10u / 22u | MLCC | CC0603KRX7R9BB104 / CL10A105KA8NNNC / CL21A106KAYNNNE / CL21A226MAQNNNE | C14663 / C15849 / C15850 / C45783 | basic |
| D2 / D3 | LED red / green 0603 | KT-0603R / 19-217/GHC-YR1S2/3T | C2286 / C72043 | basic |
| SW1,SW2 | 6x6 THT tact | PTS645SL43-2 LFS | none known | — |
| J2,J3 | 1x19 header | PZ254V-11-19P | none known | — |

## Library geometry (entered by the agent, not verified by EAK)
- A9. ESP32-WROOM-32E land pattern: 1.5 x 0.9 mm pads, 1.27 mm pitch, rows at x = ±8.75 mm, bottom row y = -12.5 mm, 4 x 4 mm GND pad at (-1, -2). The exposed-pad size/position is approximate; compare with Espressif's footprint before ordering.
- A10. USB-C HRO TYPE-C-31-M-12: signal pads as the KiCad land pattern; shell slots replaced by 0.6 mm round plated holes and NPTH pegs by 0.65 mm plated holes in 1.0 mm pads (B-007). **The shell holes are NOT the datasheet slots; the connector legs will not seat in 0.6 mm round holes.** Before ordering, the drill file must be corrected or EAK must gain slot support.
- A11. Shield (S1-S4, pegs) tied directly to GND (common hobby practice; no RC/ferrite).
- A12. D1 changed SS14 → SS34 after the EAK derating finding (1 A × 0.75 < 0.8 A rail).
