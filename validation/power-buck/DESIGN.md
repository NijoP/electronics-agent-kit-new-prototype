# Power-buck validation board: design record

A 12-24 V to 5 V / 3 A synchronous buck PCB, authored end to end in EAK.
Source of truth: `src/design.rs`, a sequence of typed `Command`s executed by
`eak_runtime::Engine`. The native project it produces is `project/project.eak.json`, with
its journal and evidence. Nothing here is a manufacturing-readiness claim; see READINESS.md
for what is and is not verified.

Labels used below:
- **[EAK]**: computed by an `eak-engineering` function. Reproduce it with `cargo run --release`;
  the result is written to `reports/engineering-calcs.md`.
- **[DS]**: a datasheet value that was read during this session, with the section cited.
- **[ASSUMED]**: a datasheet value that was *not* read in this session (the PDF could not be
  rendered), or an engineering assumption. These are NOT VERIFIED.
- **[HAND]**: a hand calculation. EAK has no function for it yet (GAPS.md G1-G3). NOT VERIFIED by EAK.

## 1. Specification and operating conditions

| Parameter | Value | Basis |
|---|---|---|
| Input voltage | 11.4-25.2 V DC (12 V −5 % to 24 V +5 %) | Typical regulated 12 V / 24 V adapters. 25.2 V ≤ SMAJ26A VRWM 26 V [ASSUMED] and ≤ TPS54302 28 V operating max [DS §5.5] |
| Input source | SELV / IEC 62368-1 ES1 DC source, current-limited adapter | The board provides no isolation (§6) |
| Output | 5.0 V nominal (4.972 V typ), 0-3 A continuous, 15 W | |
| Output set-point tolerance | 4.763-5.188 V worst case (VFB 0.581-0.611 V [DS §5.5], ±1 % resistors) [EAK] | Within ±5 % |
| Topology | Non-isolated synchronous buck, TI TPS54302, fixed 400 kHz (290-510 kHz) [DS §5.5] | Integrated 85 mΩ / 40 mΩ FETs [DS §5.5], internal compensation |
| Ambient | 0-40 °C, natural convection, no enclosure model | |
| Environment | Pollution degree 2, altitude ≤ 2000 m, no conformal coating | Selects IPC-2221B condition B2 |
| Protection | 3 A slow-blow fuse; P-MOSFET reverse polarity; SMAJ26A TVS; TPS54302 cycle-by-cycle current limit (HS 4-6 A, LS valley 3.1-5.5 A) and thermal shutdown 160 °C [DS §5.5] | §5 |

## 2. Voltage stress (declared to EAK with `SetNetVoltage`)

| Net | Peak V to GND | Why |
|---|---|---|
| VIN_RAW, VIN_F, VIN | 25.2 | Max input. Under reverse polarity VIN_RAW/VIN_F reach −25.2 V; EAK uses the magnitude |
| SW | 25.2 | Swings 0 V to VIN, with a ≤ 20 ns undershoot to −5 V allowed by the abs max [DS §5.1] |
| BOOT | 30.2 | VIN + ~5 V bootstrap. BOOT-SW abs max 7 V [DS §5.1] |
| Q1_G | 13.2 | VIN − 12 V zener clamp |
| VOUT | 5.3 | Above the 5.188 V worst-case set point |
| FB | 0.611 | VFB max |

EAK's ERC-011 checks every capacitor's voltage rating against these values. All capacitors
are rated ≥ 1.5× their rail (rating values [ASSUMED]).

## 3. Power-stage calculations

### 3.1 Inductor: Bourns SRP1265A-100M, 10 µH ±20 % [ASSUMED]
L follows the TI typical application [DS §7.2.3.5.1], which uses K_IND = 0.35 to compute
L_MIN = 9.78 µH and then selects 10 µH.

[HAND] V_OUT = 4.972 V, f_sw = 400 kHz nominal:

| Case | D = Vo/Vin | ΔI_L = (Vin−Vo)·D/(f·L) | I_peak = Io + ΔI/2 |
|---|---|---|---|
| Vin 25.2 V, nominal L and f | 0.197 | 0.998 A | 3.50 A |
| Vin 25.2 V, L −20 %, f 290 kHz (worst) | 0.197 | 1.720 A | 3.86 A |
| Vin 11.4 V, nominal | 0.436 | 0.701 A | 3.35 A |

- I_L,rms = √(Io² + ΔI²/12) = 3.014 A nominal, 3.04 A worst [HAND]. Inductor Irms 7.5 A [ASSUMED] gives a ×2.5 margin.
- Saturation must exceed the high-side current limit maximum, 6 A [DS §5.5], so that overload stays in cycle-by-cycle limit and not in saturation. Isat 12 A [ASSUMED] gives a ×2 margin.
- **Finding:** at the worst-case corner, I_peak (3.86 A) is only 0.14 A below the *minimum* high-side current limit (4.0 A [DS §5.5]). At a 25.2 V input, 3 A is the practical ceiling; this design has no extra current headroom.
- DCR loss: 3.04² × 20 mΩ [ASSUMED] = 0.18 W [HAND], plus core loss (not calculated, NOT VERIFIED).

### 3.2 Output capacitors: 2 × 22 µF 25 V X7R 1210 (Murata GRM32ER71E226KE15L) [ASSUMED]
This matches the TI example [DS §7.2.3.5.2], which uses two 22 µF 25 V X7R parts, needs at
least 30 µF for a ±5 % 1.5 A load step, and allows ESR up to 29.2 mΩ.
- Effective capacitance at 5 V bias: 33 µF. The 75 % retention is [ASSUMED] (GAPS.md G2). This is above the 30 µF minimum, with little margin.
- Output ripple ΔV ≈ ΔI/(8·f·C): 9.5 mV nominal, 22.5 mV worst case (1.72 A, 290 kHz) [HAND]. Both are below the 30 mVpp design target [DS Table 7-1].
- Capacitor RMS current ΔI/√12: 0.29 A total nominal, 0.50 A worst case [HAND], shared by two parts.
- Voltage: 25 V rating on a 5.3 V rail is ≥ 4.7× (ERC-011 passes) [EAK ERC].

### 3.3 Input capacitors: 2 × 10 µF 50 V X7R 1210 + 100 nF 50 V 0603 + 47 µF 50 V electrolytic
TI recommends at least 10 µF of ceramic decoupling plus an optional 0.1 µF [DS §7.2.3.1].
- I_Cin,rms = Io·√(D(1−D)): 1.49 A at 11.4 V and 1.19 A at 25.2 V [HAND], about 0.75 A per 1210 MLCC. The MLCC ripple rating is NOT VERIFIED.
- Input ripple: ≈ Io·D(1−D)/(f·C_eff). With an [ASSUMED] 10 µF effective at 25 V bias it is 0.12 V; with 16 µF at 11.4 V it is 0.12 V [HAND]. Both are below the 400 mV design target [DS Table 7-1].
- C1 (47 µF 50 V aluminium electrolytic, Panasonic EEE-FK1H470P [ASSUMED]) damps cable inductance against the low-ESR ceramics on hot-plug (§5.4). Its ESR and ripple rating are NOT VERIFIED.
- Voltage: 50 V on 25.2 V is 1.98× (ERC-011 passes) [EAK ERC].

### 3.4 Feedback
- The divider is R2 = 105 kΩ (top) and R3 = 14.3 kΩ (bottom), both ±1 %. The datasheet asks for R2 ≈ 100 kΩ [DS §7.2.3.3].
- **Design change:** the datasheet example (100k / 13.3k) gives 5.077 V typical and 4.863-5.298 V worst case [EAK]. That violates the ±5 % output requirement. The divider was re-centred to a 4.972 V typical, 4.763-5.188 V worst case [EAK].
- The loop compensation is internal. L and C_out match TI's validated 5 V example. Phase margin is NOT VERIFIED: there is no Bode measurement, and EAK has no small-signal model.
- EN is left floating, as the datasheet allows: "Float the EN pin to enable" [DS Table 4-1]. The input UVLO is 4.1 V typical rising [DS §5.5].
- Minimum on-time at 25.2 V is D/f = 493 ns (387 ns at 510 kHz) [HAND]. The TPS54302 minimum on-time was not retrieved, so this is NOT VERIFIED.

## 4. Losses and thermal

| Element | Loss at 25.2 V in, 3 A out | Basis |
|---|---|---|
| U1 conduction | 0.58 W | [HAND] I_rms,HS² · 85 mΩ + I_rms,LS² · 40 mΩ [DS], × 1.3 for hot R_DS(on) [ASSUMED] |
| U1 switching | 0.30 W | [HAND] ½·Vin·Io·(t_r+t_f)·f with t_r = t_f = 10 ns [ASSUMED]. Edge times were not retrieved |
| U1 dead time + gate + quiescent | 0.05 W | [HAND, ASSUMED] |
| **U1 total** | **0.93 W** | |
| L1 copper | 0.18 W | §3.1 |
| Q1 (reverse-polarity FET) | 0.034 W | [EAK] resistor_power_w(1.55 A, 14 mΩ [ASSUMED]) |

U1 junction temperature [EAK `junction_temp_c`] at Ta = 40 °C:
- With the JEDEC θJA of 118.9 °C/W [DS §5.4], Tj ≈ 150.6 °C. That is above the 150 °C
  absolute maximum and near the 160 °C thermal-shutdown threshold. **FAIL**, as an estimate.
- With the TI EVM θJA of 57.2 °C/W [DS §5.4], Tj ≈ 93.2 °C. This passes the 125 °C target.

**Conclusion (honest):** full 3 A at 24 V input and 40 °C is thermally viable only if this
board spreads heat about as well as TI's EVM. The board does give U1 2 oz copper, solid GND
pours on both layers, and three GND vias directly at the GND pin. However, EAK has no
thermal model (GAPS.md G3) and nothing has been measured. Thermal compliance is therefore
**NOT VERIFIED**. If a measured case temperature, converted with ψJT = 14 °C/W [DS §5.4],
gives Tj > 125 °C, de-rate the board to 2 A at 24 V or limit ambient to 25 °C.

## 5. Protection

### 5.1 Fuse F1: 3 A slow-blow, 1206 (Bourns SF-1206S300-2) [ASSUMED]
- The maximum steady input current is 1.55 A (11.4 V, η = 0.85 [ASSUMED]; the §4 estimate gives η ≈ 92 %). That is 52 % of rating.
- Slow-blow is chosen to survive the inrush into about 67 µF of input capacitance. The I²t check is NOT VERIFIED.
- The voltage rating must be ≥ 25.2 V. 32 V is [ASSUMED] and NOT VERIFIED.

### 5.2 Reverse polarity: Q1 AO4407A P-MOSFET, high side [ASSUMED ratings]
- Drain faces the input and source faces the load. At power-up the body diode conducts, then
  Vgs = −Vin turns Q1 fully on.
- Gate drive: R1 (100 kΩ) pulls the gate to GND, and D1 (12 V zener, MMSZ5242B) clamps Vgs
  between source and gate. Vgs is −11.4 V at the minimum input and −12 V at 25.2 V, inside
  the ±25 V VGS max [ASSUMED].
- At 0.13 mA [EAK, R1 dissipation 1.7 mW] D1 runs far below its test current. Its knee
  voltage at that current is NOT VERIFIED.
- Reverse input of −25.2 V: the body diode is reverse-biased and Vgs = 0, so Q1 is off.
  |Vds| = 25.2 V against a 30 V rating [ASSUMED] (84 %, thin). D2, C1-C4 and U1 never see
  reverse voltage, because they sit after Q1.
- Conduction loss is 34 mW (§4).

### 5.3 Overvoltage / transients: D2 SMAJ26A, after Q1
- VRWM 26 V ≥ 25.2 V. VBR min 28.9 V is below the U1 VIN abs max of 30 V [DS §5.1], so a slow
  overvoltage is clamped before U1's absolute rating, though only at the TVS knee.
- **Limitation (not hidden):** the clamping voltage at rated I_PP is 42.1 V [ASSUMED], above
  the U1 30 V abs max. The TVS does not make the board surge-immune. No surge, ESD or
  ISO 7637 / IEC 61000-4-5 requirement is claimed, and none is VERIFIED.

### 5.4 Hot-plug
A long input cable can ring the ceramic input capacitors up to about 2 × Vin. C1's ESR
damping and the D2 clamp are the mitigation. Whether that is enough is NOT VERIFIED and
needs a bench measurement.

### 5.5 Output overload / short
The TPS54302 limits the high-side current cycle-by-cycle (4-6 A peak) and has a low-side
valley limit [DS §5.5]. Long-term short-circuit behaviour (hiccup vs. continuous limiting)
was not retrieved, so it is NOT VERIFIED. The fuse is not expected to open on an output short.

## 6. Insulation, clearance, creepage and isolation
- **No isolation barrier exists.** The converter is non-isolated: output 0 V = input 0 V = GND.
  The board does not create or maintain a safety barrier. It is safe to touch only because
  its source is SELV / ES1 (IEC 62368-1 ES1 DC limit 60 V; IEC 61140 / IEC 60449 band I).
  These limits are quoted from general knowledge; the standard text was not consulted in
  this session, so they are NOT VERIFIED.
- The highest voltage on the board is 30.2 V (BOOT). Everything is ≤ 60 V DC, so only
  functional insulation applies.
- Clearance (air) [EAK `ipc::clearance_mm`, IPC-2221B Table 6-1, table not verified against
  the standard text]:
  - 25.2 V, B2 (external, uncoated): 0.10 mm required.
  - 25.2 V, A5 (component leads): 0.13 mm required.
  - 30.2 V, B2: 0.60 mm required. 30.2 V falls in the 31-50 V band; EAK rounds by band.
  - The design uses ≥ 0.20 mm everywhere (`SetDesignRules`). BOOT-to-GND copper keeps
    ≥ 0.60 mm, enforced by the 0.6 mm pour clearance and checked by DRC-002 against declared
    voltages.
- Creepage [EAK `ipc::creepage_mm`, 30.2 V, PD2, material group IIIb]: the table gives 0.04 mm
  and the design minimum is 0.2 mm. EAK flags the ≤ 50 V rows of that table as unverified,
  and EAK performs no creepage path analysis (GAPS.md G9).

## 7. PCB implementation
- Stackup: 2 layers, FR-4, 1.6 mm, **2 oz** outer copper (`SetStackup`). Fabricator
  capability for 0.2 mm track and space on 2 oz copper is NOT VERIFIED.
- Rules (`SetDesignRules`): min track 0.20 mm, min clearance 0.20 mm, min drill 0.30 mm,
  min annular ring 0.15 mm, edge clearance 0.50 mm, ΔT = 10 °C. The "Power" net class is
  0.6 mm / 0.2 mm, with 0.3 mm / 0.6 mm vias.
- Declared currents (`SetNetCurrent`):
  - VIN_RAW, VIN_F, VIN: 2.0 A. That is ≥ the 1.55 A DC input current and ≥ the 2.0 A RMS
    U1 VIN-pin current at D = 0.44.
  - SW: 3.1 A (I_L,rms).
  - VOUT and GND: 3.0 A.
  - EAK requires 0.39 / 0.72 / 0.69 mm [EAK]. The narrowest routed tracks are 0.60 / 0.80 / 0.80 mm.
- Board: 66 × 33 mm, four M3 plated holes with 6 mm floating pads on a 58 × 25 mm pattern.
  The pour clearance of 3.67 mm radius keeps an M3 washer (7 mm OD [ASSUMED]) off copper.
  The tallest part is about 10 mm (J1/J2 [ASSUMED]); the declared height limit is 12 mm.
- Placement (all top side; coordinates in `src/design.rs`, shifted by `LAYOUT_DY`):
  - Power flows left to right: J1, F1, Q1 (+ D1/R1 gate network), D2, C1, input MLCCs C3/C2/C4, U1, L1, C6/C7, J2.
  - The feedback divider (R2/R3) sits below U1, away from SW and L1.
  - R2 senses VOUT at C6 through a dedicated 0.8 mm trace. This is a Kelvin-style sense, not the power path.
- **Hot loop (Cin → HS FET → LS FET → GND → Cin):** the TPS54302 places SW between VIN (pin 3)
  and GND (pin 1), so a single-layer loop is impossible without running switching current
  under the IC, which TI forbids [DS §7.4.1]. The loop is closed instead as follows:
  - C4 (100 nF) VIN pad 1.6 mm from the VIN pin.
  - U1 GND pin → 3 vias at 1.35-1.7 mm.
  - A solid bottom GND plane 1.6 mm below.
  - 5 vias back at C2/C3/C4 GND, 4.7-10 mm away.
  - A parallel top-layer GND pour path.

  The loop inductance has not been calculated (no field solver in EAK): **NOT VERIFIED**.
- SW node: a 0.85 mm neck between the VIN and GND pads, ≥ 0.2 mm clear of both, widening to
  2.0 mm over a 4.5 mm run to L1 pad 1. This is the minimum practical SW copper. The bottom
  layer under SW is unbroken GND.
- Return currents: the bottom layer is a continuous GND plane. No signal or power track runs
  on the bottom layer, so every top-layer current has an unbroken image path.
- Via current [EAK `via_current_a` / `vias_required`, 20 µm plating ASSUMED]: 0.80 A per via;
  5 vias are needed for 3 A.
  - The C_in and C_out clusters have 5 vias each.
  - **The U1 GND-pin cluster has only 3.** Space between the BOOT keep-out and SW limits it.
    The parallel top pour carries the remainder, but pour current density is not checked by
    EAK (GAPS.md G3). Status: PARTIAL / NOT VERIFIED.
- Thermal paths: U1 heat leaves through its pins into 2 oz copper: the GND exit to the via
  cluster, the VIN plane, and the SW copper. Q1 has a drain bar and a source bar. Both pours
  are solid. There is no thermal relief: EAK zone fill makes solid connections, which helps
  heat spreading but makes hand soldering harder.

## 8. Component selection summary (justification)

| Ref | Part | Why | Key rating basis |
|---|---|---|---|
| U1 | TI TPS54302DDCR | 4.5-28 V, 3 A sync buck with integrated FETs, internal comp, few parts, widely stocked | [DS] §5.1/5.4/5.5 |
| L1 | Bourns SRP1265A-100M 10 µH | L per TI K_IND 0.35 method; shielded; Isat ≫ 6 A ILIM max | [ASSUMED] |
| C6, C7 | 22 µF 25 V X7R 1210 | Same as TI's validated example; ≥ 30 µF effective | [ASSUMED] |
| C2, C3, C4 | 10 µF 50 V X7R 1210 ×2, 100 nF 50 V 0603 | ≥ 10 µF ceramic per TI; 2× voltage margin | [ASSUMED] |
| C1 | 47 µF 50 V electrolytic | Hot-plug damping | [ASSUMED] |
| C5 | 100 nF 50 V 0603 | BOOT capacitor per [DS Table 4-1] | [ASSUMED] |
| R2, R3 | 105 kΩ / 14.3 kΩ 1 % 0603 | Re-centred set point (§3.4) | [EAK] |
| Q1 | AOS AO4407A | −30 V, low-RDS(on) P-FET; |VGS| clamped to 12 V | [ASSUMED] |
| D1 | MMSZ5242B 12 V zener | Gate clamp | [ASSUMED] |
| R1 | 100 kΩ 0603 | Gate pull-down; 1.7 mW [EAK] | |
| D2 | Vishay SMAJ26A | VRWM ≥ 25.2 V; VBR min < 30 V U1 abs max | [ASSUMED] |
| F1 | Bourns SF-1206S300-2 3 A slow-blow | 52 % loading, inrush tolerant | [ASSUMED] |
| R4, D3 | 1.5 kΩ + green 0603 LED | 2 mA power indicator [EAK led_resistor_ohm] | built-in generic |
| J1, J2 | Phoenix MKDS 1,5/2-5,08 | 5.08 mm screw terminal, ≫ 3 A | [ASSUMED] |
| H1-H4 | M3 plated holes | Mounting | |

The land patterns are generic IPC-7351-style approximations authored for this board. They
have not been checked against the manufacturers' recommended land patterns: NOT VERIFIED.
