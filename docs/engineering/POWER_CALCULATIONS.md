# Synchronous buck power-stage calculations

Implementation: `crates/eak-engineering/src/calc.rs`. Every function rejects non-physical input
with a typed `CalcError` (`InvalidInput` or `OutOfRange`). The model is continuous conduction at a
fixed frequency. The numeric tests use a hand-computed reference case: 24 V → 5 V, 3 A, 500 kHz,
ripple ratio 0.3.

## Sources
- **[SLVA477B]** Texas Instruments, *Basic Calculation of a Buck Converter's Power Stage*, SLVA477B (2015).
- **[SLVA390]** Texas Instruments, *Calculating Efficiency*, SLVA390 (2010).
- **[IFX-LOSS]** D. Graovac, M. Pürschel, A. Kiep, *MOSFET Power Losses Calculation Using the
  Data-Sheet Parameters*, Infineon application note (2006).
- **[E&M]** R. W. Erickson, D. Maksimović, *Fundamentals of Power Electronics*, 2nd ed., ch. 2
  (inductor volt-second balance, triangular ripple waveforms).
- **[IEC63]** IEC 60063, preferred number series (E24, E96).

The formulas below are written from these public references. No code was copied.

## Formulas
| Function | Formula | Source | Reference case (24 V → 5 V, 3 A, 500 kHz) |
|---|---|---|---|
| `buck_duty` | D = Vout / (Vin · η) | SLVA477B | η = 1: 0.208333; η = 0.9: 0.231481 |
| `buck_ripple_a` | ΔI = (Vin − Vout) · D / (fsw · L) | SLVA477B | L = 10 µH: 0.791667 A |
| `buck_inductor_h` | L = (Vin − Vout) · D / (fsw · r · Iout); guidance r = 0.2…0.4 | SLVA477B | r = 0.3: 8.796 µH |
| `buck_inductor_currents` | Ipk = Iout + ΔI/2; Ivalley = Iout − ΔI/2; Irms = √(Iout² + ΔI²/12) | SLVA477B (peak); E&M (RMS of a triangle on DC) | ΔI = 0.9 A: Ipk 3.45 A, Irms 3.0112 A |
| `buck_output_ripple_v` | ΔV = ΔI / (8 · fsw · C) + ΔI · ESR (worst-case sum) | SLVA477B | 22 µF, 5 mΩ: 14.73 mV |
| `buck_output_cap_f` | C = ΔI / (8 · fsw · (ΔV − ΔI · ESR)); error when ΔI·ESR ≥ ΔV | SLVA477B (solved for C) | 50 mV, 5 mΩ: 4.945 µF |
| `buck_output_cap_rms_a` | I = ΔI / (2√3) | E&M | 0.2598 A |
| `buck_input_cap_rms_a` | I = √(D · (Iout² + ΔI²/12) − (D · Iout)²); with ΔI = 0 it is Iout·√(D(1−D)) | SLVA477B; E&M | 1.2241 A (1.2183 A without ripple) |
| `buck_losses`: conduction | P = Rds(on) · D · (Iout² + ΔI²/12) (high side); (1 − D) for the low side | SLVA390; IFX-LOSS | 10 / 5 mΩ: 18.9 / 35.9 mW |
| `buck_losses`: switching | P = ½ · Vin · Iout · (tr + tf) · fsw (high side) | SLVA390 | 10 + 10 ns: 0.36 W |
| `buck_losses`: reverse recovery | P = Vin · Qrr · fsw (dissipated in the high side) | IFX-LOSS | 10 nC: 0.12 W |
| `buck_losses`: dead time | P = Vf · Iout · tdead · fsw (low-side body diode) | SLVA390 | 0.7 V, 40 ns: 42 mW |
| `buck_losses`: gate drive | P = (Qg,hs + Qg,ls) · Vdrive · fsw (dissipated in driver and gate resistors) | SLVA390; IFX-LOSS | 2 × 10 nC, 5 V: 50 mW |
| `buck_losses`: inductor copper | P = (Iout² + ΔI²/12) · DCR | E&M | 15 mΩ: 136 mW |
| `buck_losses`: efficiency | η = Pout / (Pout + ΣP) | — | 95.16 % |
| Junction temperature | Tj = Ta + P · θJA (`junction_temp_c`, applied to `hs_dissipation_w` / `ls_dissipation_w`) | — | θJA 40 °C/W, Ta 25 °C: HS 44.96 °C, LS 28.12 °C |
| `feedback_divider` | Vout = Vref · (1 + Rtop / Rbot); exhaustive search over E24/E96 pairs in [Rmin, Rmax]; worst case Vref(1 ± tv) · (1 + Rtop(1 ± t) / (Rbot(1 ∓ t))) | SLVA477B; IEC63 | 0.6 V → 5 V, E24: 11k / 1.5k exact; 0.8 V → 3.3 V, E96: 3.57k / 1.15k, −0.50 % |

## Known limits
- `buck_losses` uses the ideal duty Vout/Vin for the loss split. It leaves out Coss charging,
  inductor core loss, capacitor ESR loss and controller quiescent current, which is marked as a
  `shortcut:` in the code. Above about 1 MHz, or at light load, these terms matter.
- The ripple formulas assume continuous conduction. A negative valley current is reported as is:
  it means reverse current in forced PWM, or discontinuous conduction with diode emulation.
- ESL and capacitor DC-bias derating are not modelled. Use the derated (effective) capacitance as
  the input.
- The calculations are library functions. They are not yet exposed as a typed runtime command or
  through a CLI.

## Rating checks (ERC)
The rules `ERC-011` (capacitor voltage), `ERC-014` (regulator/IC supply voltage), `ERC-015`
(inductor current) and `ERC-016` (MOSFET Vds) share one rating check
(`eak-validation/src/erc.rs::check_rating`):
- Rating below the declared stress: **error**.
- Rating below margin × stress: **warning**.
- Missing rating, rating in a different unit, or no declared net voltage/current: **info, "not
  checked"**. This is never counted as a pass.
- Pass on an unverified rating: **info**, so the pass is reported as resting on an unverified
  rating.

| Rule | Library rating name(s) | Stress | Warning margin | Margin basis |
|---|---|---|---|---|
| ERC-011 | `voltage` | highest declared voltage on any pin net | 1.5× | DESK `erc.py` heuristic (DC-bias derating) |
| ERC-014 | `input_voltage_max`, else `supply_voltage_max` | highest declared rail on PowerIn pins with supply names (VIN, VCC, VDD…) | 1.2× / 1.05× | EAK heuristic: input-transient headroom / rail regulation tolerance |
| ERC-015 | `saturation_current` and `current` (rated), each checked | highest declared net current on its pins | 1.2× / 1.1× | Peak at a 40 % ripple ratio (SLVA477B guidance); rated-current margin is an EAK heuristic |
| ERC-016 | `vds_max` | highest declared rail on D/S pins | 1.25× (80 % derating) | EAK heuristic for switch-node ringing |

Declared values come from `Constraints.net_voltages_v` (peak working voltage) and
`Constraints.net_currents_a` (continuous current). Transients, ringing and the inductor ripple
peak are not simulated; the margins cover them heuristically.
