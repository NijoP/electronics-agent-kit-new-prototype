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
