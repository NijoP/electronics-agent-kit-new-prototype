//! Deterministic circuit calculations. AI-proposed numbers are recomputed here
//! (Execution Instructions §9.1); every function rejects non-physical inputs.

use crate::ipc::CalcError;

fn pos(name: &str, v: f64) -> Result<f64, CalcError> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(CalcError::InvalidInput(format!(
            "{name} must be a positive finite number"
        )))
    }
}

/// Parse an engineering value such as `10k`, `4k7`, `2R2`, `100n`, `4.7u`, `1M`.
pub fn parse_value(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches(['Ω', 'F', 'H', 'V', 'A']).trim();
    if s.is_empty() {
        return None;
    }
    let mult = |c: char| -> Option<f64> {
        Some(match c {
            'p' => 1e-12,
            'n' => 1e-9,
            'u' | 'µ' => 1e-6,
            'm' => 1e-3,
            'R' | 'r' => 1.0,
            'k' | 'K' => 1e3,
            'M' => 1e6,
            'G' => 1e9,
            _ => return None,
        })
    };
    if let Ok(v) = s.parse::<f64>() {
        return Some(v);
    }
    // Letter used as decimal point (4k7) or as suffix (10k).
    let idx = s
        .char_indices()
        .find(|(_, c)| c.is_alphabetic() || *c == 'µ')?;
    let m = mult(idx.1)?;
    let (head, tail) = (&s[..idx.0], &s[idx.0 + idx.1.len_utf8()..]);
    let num = if tail.is_empty() {
        head.to_string()
    } else {
        format!("{head}.{tail}")
    };
    let v: f64 = if num.is_empty() || num.starts_with('.') {
        format!("0{num}").parse().ok()?
    } else {
        num.parse().ok()?
    };
    Some(v * m)
}

/// Series resistor for an LED: R = (Vs − Vf) / If.
pub fn led_resistor_ohm(supply_v: f64, forward_v: f64, current_a: f64) -> Result<f64, CalcError> {
    pos("supply voltage", supply_v)?;
    pos("forward voltage", forward_v)?;
    pos("LED current", current_a)?;
    if forward_v >= supply_v {
        return Err(CalcError::InvalidInput(
            "supply must exceed LED forward voltage".into(),
        ));
    }
    Ok((supply_v - forward_v) / current_a)
}

/// Resistor dissipation P = I²R.
pub fn resistor_power_w(current_a: f64, resistance_ohm: f64) -> Result<f64, CalcError> {
    pos("resistance", resistance_ohm)?;
    Ok(current_a * current_a * resistance_ohm)
}

/// Unloaded divider output Vout = Vin · R2 / (R1 + R2).
pub fn divider_vout(vin: f64, r1: f64, r2: f64) -> Result<f64, CalcError> {
    pos("R1", r1)?;
    pos("R2", r2)?;
    Ok(vin * r2 / (r1 + r2))
}

/// Linear regulator dissipation P = (Vin − Vout) · Iout (+ Vin · Iq).
pub fn linear_reg_dissipation_w(vin: f64, vout: f64, iout: f64, iq: f64) -> Result<f64, CalcError> {
    if vin <= vout {
        return Err(CalcError::InvalidInput(
            "input voltage must exceed output voltage".into(),
        ));
    }
    Ok((vin - vout) * iout + vin * iq)
}

/// Junction temperature estimate Tj = Ta + P · θja.
pub fn junction_temp_c(
    ambient_c: f64,
    power_w: f64,
    theta_ja_c_per_w: f64,
) -> Result<f64, CalcError> {
    pos("thetaJA", theta_ja_c_per_w)?;
    Ok(ambient_c + power_w * theta_ja_c_per_w)
}

/// RC time constant τ = R·C (seconds).
pub fn rc_time_constant_s(r: f64, c: f64) -> Result<f64, CalcError> {
    Ok(pos("R", r)? * pos("C", c)?)
}

// ---------------------------------------------------------------------------------------
// Synchronous buck converter power stage (continuous conduction, fixed frequency).
// Sources (formula-by-formula list in docs/engineering/POWER_CALCULATIONS.md):
//   [SLVA477B] TI, "Basic Calculation of a Buck Converter's Power Stage", SLVA477B, 2015.
//   [SLVA390]  TI, "Calculating Efficiency", SLVA390, 2010.
//   [IFX-LOSS] Graovac, Pürschel, Kiep, "MOSFET Power Losses Calculation Using the
//              Data-Sheet Parameters", Infineon application note, 2006.
//   [E&M]      Erickson & Maksimović, "Fundamentals of Power Electronics", 2nd ed., ch. 2.
//   [IEC63]    IEC 60063, preferred number series for resistors (E24, E96).
// ---------------------------------------------------------------------------------------

fn non_neg(name: &str, v: f64) -> Result<f64, CalcError> {
    if v.is_finite() && v >= 0.0 {
        Ok(v)
    } else {
        Err(CalcError::InvalidInput(format!(
            "{name} must be a non-negative finite number"
        )))
    }
}

fn unit_fraction(name: &str, v: f64) -> Result<f64, CalcError> {
    if v.is_finite() && v > 0.0 && v <= 1.0 {
        Ok(v)
    } else {
        Err(CalcError::InvalidInput(format!("{name} must be in (0, 1]")))
    }
}

fn tolerance(name: &str, v: f64) -> Result<f64, CalcError> {
    if v.is_finite() && (0.0..1.0).contains(&v) {
        Ok(v)
    } else {
        Err(CalcError::InvalidInput(format!(
            "{name} must be a fraction in [0, 1)"
        )))
    }
}

/// Buck duty cycle D = Vout / (Vin · η) [SLVA477B]. `efficiency` = 1.0 gives the
/// ideal lossless duty cycle.
pub fn buck_duty(vin: f64, vout: f64, efficiency: f64) -> Result<f64, CalcError> {
    pos("input voltage", vin)?;
    pos("output voltage", vout)?;
    unit_fraction("efficiency", efficiency)?;
    if vout >= vin {
        return Err(CalcError::InvalidInput(
            "buck output voltage must be below input voltage".into(),
        ));
    }
    let d = vout / (vin * efficiency);
    if d >= 1.0 {
        return Err(CalcError::OutOfRange(format!(
            "duty cycle {d:.3} >= 1: input too low for this output at the given efficiency"
        )));
    }
    Ok(d)
}

/// Peak-to-peak inductor ripple ΔI = (Vin − Vout) · D / (fsw · L) [SLVA477B].
pub fn buck_ripple_a(
    vin: f64,
    vout: f64,
    efficiency: f64,
    fsw_hz: f64,
    inductance_h: f64,
) -> Result<f64, CalcError> {
    let d = buck_duty(vin, vout, efficiency)?;
    Ok((vin - vout) * d / (pos("switching frequency", fsw_hz)? * pos("inductance", inductance_h)?))
}

/// Inductance for a target ripple ratio r = ΔI / Iout:
/// L = (Vin − Vout) · D / (fsw · r · Iout) [SLVA477B, with ΔI = r · Iout; the note
/// recommends r = 0.2…0.4]. Using D with efficiency slightly overstates the on-time
/// volt-seconds, so the inductance errs on the large (lower-ripple) side.
pub fn buck_inductor_h(
    vin: f64,
    vout: f64,
    efficiency: f64,
    iout_a: f64,
    ripple_ratio: f64,
    fsw_hz: f64,
) -> Result<f64, CalcError> {
    let d = buck_duty(vin, vout, efficiency)?;
    let ripple = pos("output current", iout_a)? * pos("ripple ratio", ripple_ratio)?;
    Ok((vin - vout) * d / (pos("switching frequency", fsw_hz)? * ripple))
}

/// Inductor current figures for rating selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InductorCurrents {
    pub ripple_a: f64,
    /// Ipk = Iout + ΔI/2: compare with the saturation current.
    pub peak_a: f64,
    /// Ivalley = Iout − ΔI/2. Negative means reverse current (forced-PWM synchronous
    /// buck) or, with diode emulation, discontinuous conduction.
    pub valley_a: f64,
    /// Irms = √(Iout² + ΔI²/12): compare with the thermal (rated) current.
    pub rms_a: f64,
}

/// Inductor peak, valley and RMS current of a triangular ripple on a DC level
/// [SLVA477B for the peak; RMS of a triangle on DC, E&M ch. 2].
pub fn buck_inductor_currents(iout_a: f64, ripple_a: f64) -> Result<InductorCurrents, CalcError> {
    pos("output current", iout_a)?;
    non_neg("ripple current", ripple_a)?;
    Ok(InductorCurrents {
        ripple_a,
        peak_a: iout_a + ripple_a / 2.0,
        valley_a: iout_a - ripple_a / 2.0,
        rms_a: (iout_a * iout_a + ripple_a * ripple_a / 12.0).sqrt(),
    })
}

/// Output voltage ripple ΔV = ΔI / (8 · fsw · C) + ΔI · ESR [SLVA477B].
/// The capacitive and ESR terms are summed (worst case; they are not in phase). ESL is
/// not modelled.
pub fn buck_output_ripple_v(
    ripple_a: f64,
    fsw_hz: f64,
    capacitance_f: f64,
    esr_ohm: f64,
) -> Result<f64, CalcError> {
    non_neg("ripple current", ripple_a)?;
    non_neg("ESR", esr_ohm)?;
    Ok(
        ripple_a / (8.0 * pos("switching frequency", fsw_hz)? * pos("capacitance", capacitance_f)?)
            + ripple_a * esr_ohm,
    )
}

/// Minimum output capacitance for a target ripple, solving [`buck_output_ripple_v`] for C:
/// C = ΔI / (8 · fsw · (ΔV − ΔI · ESR)).
pub fn buck_output_cap_f(
    ripple_a: f64,
    fsw_hz: f64,
    target_ripple_v: f64,
    esr_ohm: f64,
) -> Result<f64, CalcError> {
    pos("ripple current", ripple_a)?;
    pos("target ripple voltage", target_ripple_v)?;
    non_neg("ESR", esr_ohm)?;
    let budget = target_ripple_v - ripple_a * esr_ohm;
    if budget <= 0.0 {
        return Err(CalcError::OutOfRange(format!(
            "ESR ripple {:.4} V alone exceeds the {target_ripple_v} V target: lower the ESR",
            ripple_a * esr_ohm
        )));
    }
    Ok(ripple_a / (8.0 * pos("switching frequency", fsw_hz)? * budget))
}

/// Output capacitor RMS ripple current ΔI / (2√3) (AC part of the triangular inductor
/// current, E&M ch. 2).
pub fn buck_output_cap_rms_a(ripple_a: f64) -> Result<f64, CalcError> {
    Ok(non_neg("ripple current", ripple_a)? / (2.0 * 3f64.sqrt()))
}

/// Input capacitor RMS current I = √(D·(Iout² + ΔI²/12) − (D·Iout)²): the switch current
/// (inductor current during the on-time) minus its DC average. With ΔI = 0 it reduces to
/// the familiar Iout·√(D(1−D)) [SLVA477B; E&M ch. 2].
pub fn buck_input_cap_rms_a(duty: f64, iout_a: f64, ripple_a: f64) -> Result<f64, CalcError> {
    if !(duty > 0.0 && duty < 1.0) {
        return Err(CalcError::InvalidInput(
            "duty cycle must be in (0, 1)".into(),
        ));
    }
    pos("output current", iout_a)?;
    non_neg("ripple current", ripple_a)?;
    let mean_sq = duty * (iout_a * iout_a + ripple_a * ripple_a / 12.0);
    Ok((mean_sq - (duty * iout_a).powi(2)).max(0.0).sqrt())
}

/// Operating point and device data for [`buck_losses`]. Times in seconds, charges in
/// coulombs; zero is allowed for any loss parameter you want excluded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuckLossInput {
    pub vin_v: f64,
    pub vout_v: f64,
    pub iout_a: f64,
    pub fsw_hz: f64,
    /// Peak-to-peak inductor ripple (from [`buck_ripple_a`]).
    pub ripple_a: f64,
    pub hs_rds_on_ohm: f64,
    pub ls_rds_on_ohm: f64,
    /// High-side current rise and voltage fall/rise transition times.
    pub rise_s: f64,
    pub fall_s: f64,
    pub hs_qg_c: f64,
    pub ls_qg_c: f64,
    pub gate_drive_v: f64,
    /// Total dead time per switching period (both edges).
    pub dead_time_s: f64,
    pub body_diode_vf_v: f64,
    /// Low-side body-diode reverse-recovery charge.
    pub ls_qrr_c: f64,
    pub inductor_dcr_ohm: f64,
}

/// Loss breakdown of a synchronous buck power stage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuckLosses {
    /// Ideal duty cycle Vout / Vin used for the loss split.
    pub duty: f64,
    pub hs_conduction_w: f64,
    pub hs_switching_w: f64,
    /// Low-side reverse recovery, dissipated in the high-side switch.
    pub reverse_recovery_w: f64,
    pub ls_conduction_w: f64,
    /// Low-side body-diode conduction during dead time.
    pub dead_time_w: f64,
    /// Gate charge loss, dissipated in the driver/controller and gate resistors.
    pub gate_drive_w: f64,
    pub inductor_dcr_w: f64,
    pub output_power_w: f64,
    pub total_loss_w: f64,
    pub efficiency: f64,
}

impl BuckLosses {
    /// Heat in the high-side MOSFET (for [`junction_temp_c`]).
    pub fn hs_dissipation_w(&self) -> f64 {
        self.hs_conduction_w + self.hs_switching_w + self.reverse_recovery_w
    }
    /// Heat in the low-side MOSFET (for [`junction_temp_c`]).
    pub fn ls_dissipation_w(&self) -> f64 {
        self.ls_conduction_w + self.dead_time_w
    }
}

/// Synchronous buck loss and efficiency estimate.
///
/// * Conduction: Rds(on) · I²rms per switch, I²rms = D·(Iout² + ΔI²/12) for the high side
///   and (1−D)·(…) for the low side [SLVA390; IFX-LOSS].
/// * High-side switching: ½ · Vin · Iout · (tr + tf) · fsw [SLVA390].
/// * Reverse recovery: Vin · Qrr · fsw [IFX-LOSS].
/// * Dead time: Vf · Iout · tdead · fsw [SLVA390].
/// * Gate drive: (Qg,hs + Qg,ls) · Vdrive · fsw [SLVA390; IFX-LOSS].
/// * Inductor copper: I²rms · DCR, I²rms = Iout² + ΔI²/12.
/// * η = Pout / (Pout + ΣP).
///
/// shortcut: losses use the ideal duty Vout/Vin and exclude Coss charging, inductor core
/// loss, capacitor ESR loss and controller quiescent current; upgrade when a design needs
/// an efficiency estimate tighter than these terms (light load or > 1 MHz).
pub fn buck_losses(i: &BuckLossInput) -> Result<BuckLosses, CalcError> {
    let duty = buck_duty(i.vin_v, i.vout_v, 1.0)?;
    let iout = pos("output current", i.iout_a)?;
    let fsw = pos("switching frequency", i.fsw_hz)?;
    for (name, v) in [
        ("ripple current", i.ripple_a),
        ("high-side Rds(on)", i.hs_rds_on_ohm),
        ("low-side Rds(on)", i.ls_rds_on_ohm),
        ("rise time", i.rise_s),
        ("fall time", i.fall_s),
        ("high-side gate charge", i.hs_qg_c),
        ("low-side gate charge", i.ls_qg_c),
        ("gate drive voltage", i.gate_drive_v),
        ("dead time", i.dead_time_s),
        ("body diode forward voltage", i.body_diode_vf_v),
        ("reverse recovery charge", i.ls_qrr_c),
        ("inductor DCR", i.inductor_dcr_ohm),
    ] {
        non_neg(name, v)?;
    }
    if i.dead_time_s * fsw >= 1.0 - duty {
        return Err(CalcError::InvalidInput(
            "dead time exceeds the low-side conduction interval".into(),
        ));
    }
    let i2 = iout * iout + i.ripple_a * i.ripple_a / 12.0;
    let hs_conduction_w = i.hs_rds_on_ohm * i2 * duty;
    let ls_conduction_w = i.ls_rds_on_ohm * i2 * (1.0 - duty);
    let hs_switching_w = 0.5 * i.vin_v * iout * (i.rise_s + i.fall_s) * fsw;
    let reverse_recovery_w = i.vin_v * i.ls_qrr_c * fsw;
    let dead_time_w = i.body_diode_vf_v * iout * i.dead_time_s * fsw;
    let gate_drive_w = (i.hs_qg_c + i.ls_qg_c) * i.gate_drive_v * fsw;
    let inductor_dcr_w = i2 * i.inductor_dcr_ohm;
    let total_loss_w = hs_conduction_w
        + ls_conduction_w
        + hs_switching_w
        + reverse_recovery_w
        + dead_time_w
        + gate_drive_w
        + inductor_dcr_w;
    let output_power_w = i.vout_v * iout;
    Ok(BuckLosses {
        duty,
        hs_conduction_w,
        hs_switching_w,
        reverse_recovery_w,
        ls_conduction_w,
        dead_time_w,
        gate_drive_w,
        inductor_dcr_w,
        output_power_w,
        total_loss_w,
        efficiency: output_power_w / (output_power_w + total_loss_w),
    })
}

/// Preferred-number series for resistors [IEC63].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ESeries {
    E24,
    E96,
}

const E24: [u16; 24] = [
    10, 11, 12, 13, 15, 16, 18, 20, 22, 24, 27, 30, 33, 36, 39, 43, 47, 51, 56, 62, 68, 75, 82, 91,
];
const E96: [u16; 96] = [
    100, 102, 105, 107, 110, 113, 115, 118, 121, 124, 127, 130, 133, 137, 140, 143, 147, 150, 154,
    158, 162, 165, 169, 174, 178, 182, 187, 191, 196, 200, 205, 210, 215, 221, 226, 232, 237, 243,
    249, 255, 261, 267, 274, 280, 287, 294, 301, 309, 316, 324, 332, 340, 348, 357, 365, 374, 383,
    392, 402, 412, 422, 432, 442, 453, 464, 475, 487, 499, 511, 523, 536, 549, 562, 576, 590, 604,
    619, 634, 649, 665, 681, 698, 715, 732, 750, 768, 787, 806, 825, 845, 866, 887, 909, 931, 953,
    976,
];

impl ESeries {
    /// Every series value in [lo, hi] ohms, ascending.
    pub fn values_in(self, lo: f64, hi: f64) -> Vec<f64> {
        let (mantissas, digits): (&[u16], i32) = match self {
            ESeries::E24 => (&E24, 2),
            ESeries::E96 => (&E96, 3),
        };
        let mut out = Vec::new();
        if !(lo > 0.0 && hi >= lo && hi.is_finite()) {
            return out;
        }
        let (first, last) = (lo.log10().floor() as i32, hi.log10().ceil() as i32);
        for decade in first..=last {
            for &m in mantissas {
                let v = f64::from(m) * 10f64.powi(decade - digits + 1);
                if v >= lo * (1.0 - 1e-9) && v <= hi * (1.0 + 1e-9) {
                    out.push(v);
                }
            }
        }
        out
    }
}

/// Selected feedback divider (Rtop from Vout to FB, Rbottom from FB to ground).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeedbackDivider {
    pub r_top_ohm: f64,
    pub r_bottom_ohm: f64,
    /// Nominal output with these resistors.
    pub vout_v: f64,
    /// Nominal error vout_v − target.
    pub error_v: f64,
    pub error_pct: f64,
    /// Worst-case output over resistor and reference tolerance.
    pub vout_min_v: f64,
    pub vout_max_v: f64,
}

/// Choose standard resistors for Vout = Vref · (1 + Rtop / Rbottom) [SLVA477B].
/// Searches every (Rtop, Rbottom) pair of the series within [r_min, r_max] and keeps the
/// smallest nominal error (ties keep the smaller Rbottom, then the smaller Rtop). Worst
/// case applies `resistor_tol` (e.g. 0.01) to both resistors in the adverse direction and
/// `vref_tol` to the reference.
pub fn feedback_divider(
    vref: f64,
    vout_target: f64,
    series: ESeries,
    r_min_ohm: f64,
    r_max_ohm: f64,
    resistor_tol: f64,
    vref_tol: f64,
) -> Result<FeedbackDivider, CalcError> {
    pos("reference voltage", vref)?;
    pos("target output voltage", vout_target)?;
    pos("minimum resistance", r_min_ohm)?;
    pos("maximum resistance", r_max_ohm)?;
    tolerance("resistor tolerance", resistor_tol)?;
    tolerance("reference tolerance", vref_tol)?;
    if vout_target <= vref {
        return Err(CalcError::InvalidInput(
            "target output must exceed the reference voltage".into(),
        ));
    }
    if r_max_ohm < r_min_ohm {
        return Err(CalcError::InvalidInput(
            "maximum resistance is below minimum resistance".into(),
        ));
    }
    let values = series.values_in(r_min_ohm, r_max_ohm);
    let mut best: Option<(f64, f64, f64)> = None; // (|error|, rtop, rbot)
    for &rb in &values {
        for &rt in &values {
            let err = (vref * (1.0 + rt / rb) - vout_target).abs();
            // Improvement must exceed float noise so exact ties stay deterministic.
            let better = match best {
                None => true,
                Some((e, _, _)) => err < e - 1e-12,
            };
            if better {
                best = Some((err, rt, rb));
            }
        }
    }
    let (_, rt, rb) = best.ok_or_else(|| {
        CalcError::OutOfRange("no series values in the given resistance range".into())
    })?;
    let vout = vref * (1.0 + rt / rb);
    let t = resistor_tol;
    Ok(FeedbackDivider {
        r_top_ohm: rt,
        r_bottom_ohm: rb,
        vout_v: vout,
        error_v: vout - vout_target,
        error_pct: (vout - vout_target) / vout_target * 100.0,
        vout_min_v: vref * (1.0 - vref_tol) * (1.0 + rt * (1.0 - t) / (rb * (1.0 + t))),
        vout_max_v: vref * (1.0 + vref_tol) * (1.0 + rt * (1.0 + t) / (rb * (1.0 - t))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_engineering_notation() {
        assert_eq!(parse_value("10k"), Some(10_000.0));
        assert_eq!(parse_value("4k7"), Some(4_700.0));
        assert_eq!(parse_value("2R2"), Some(2.2));
        assert!((parse_value("100n").unwrap() - 1e-7).abs() < 1e-18);
        assert!((parse_value("4.7u").unwrap() - 4.7e-6).abs() < 1e-18);
        assert_eq!(parse_value("1M"), Some(1e6));
        assert_eq!(parse_value("330"), Some(330.0));
        assert_eq!(parse_value("abc"), None);
        assert_eq!(parse_value(""), None);
    }

    #[test]
    fn led_and_power() {
        let r = led_resistor_ohm(5.0, 2.0, 0.01).unwrap();
        assert!((r - 300.0).abs() < 1e-9);
        assert!((resistor_power_w(0.01, 300.0).unwrap() - 0.03).abs() < 1e-12);
        assert!(led_resistor_ohm(3.0, 3.3, 0.01).is_err());
    }

    #[test]
    fn regulator_thermal() {
        let p = linear_reg_dissipation_w(5.0, 3.3, 0.5, 0.0).unwrap();
        assert!((p - 0.85).abs() < 1e-12);
        assert!((junction_temp_c(25.0, p, 90.0).unwrap() - 101.5).abs() < 1e-9);
        assert!(linear_reg_dissipation_w(3.3, 5.0, 0.1, 0.0).is_err());
    }

    #[test]
    fn divider() {
        assert!((divider_vout(5.0, 10e3, 10e3).unwrap() - 2.5).abs() < 1e-12);
        assert!((rc_time_constant_s(10e3, 100e-9).unwrap() - 1e-3).abs() < 1e-15);
    }

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() <= tol, "{a} != {b} (tol {tol})");
    }

    // Reference case for the validation buck board: 24 V -> 5 V, 3 A, 500 kHz.
    // Every expected value below is hand-computed from the documented formula.

    #[test]
    fn buck_duty_cycle() {
        close(buck_duty(24.0, 5.0, 1.0).unwrap(), 5.0 / 24.0, 1e-15); // 0.208333
        close(buck_duty(24.0, 5.0, 0.9).unwrap(), 5.0 / 21.6, 1e-15); // 0.231481
        close(buck_duty(12.0, 5.0, 0.9).unwrap(), 5.0 / 10.8, 1e-15); // 0.462963
        assert!(buck_duty(5.0, 5.0, 1.0).is_err());
        assert!(buck_duty(24.0, 5.0, 0.0).is_err());
        assert!(buck_duty(24.0, 5.0, 1.2).is_err());
        assert!(buck_duty(f64::NAN, 5.0, 1.0).is_err());
        // 5.2 V in, 5 V out at 90 % efficiency would need D > 1.
        assert!(matches!(
            buck_duty(5.2, 5.0, 0.9),
            Err(CalcError::OutOfRange(_))
        ));
    }

    #[test]
    fn buck_inductor_and_ripple() {
        // r = 0.3 -> ΔI = 0.9 A; L = 19 V · (5/24) / (500 kHz · 0.9 A) = 8.796296 µH.
        let l = buck_inductor_h(24.0, 5.0, 1.0, 3.0, 0.3, 500e3).unwrap();
        close(l, 8.796_296_296e-6, 1e-15);
        // Round trip: that inductance gives back 0.9 A of ripple.
        close(buck_ripple_a(24.0, 5.0, 1.0, 500e3, l).unwrap(), 0.9, 1e-12);
        // Standard 10 µH: ΔI = 19 V · (5/24) / (500 kHz · 10 µH) = 0.791667 A.
        close(
            buck_ripple_a(24.0, 5.0, 1.0, 500e3, 10e-6).unwrap(),
            0.791_666_667,
            1e-9,
        );
        assert!(buck_inductor_h(24.0, 5.0, 1.0, 3.0, 0.0, 500e3).is_err());
        assert!(buck_ripple_a(24.0, 5.0, 1.0, 0.0, 10e-6).is_err());
    }

    #[test]
    fn buck_inductor_current_ratings() {
        let c = buck_inductor_currents(3.0, 0.9).unwrap();
        close(c.peak_a, 3.45, 1e-12);
        close(c.valley_a, 2.55, 1e-12);
        close(c.rms_a, 9.0675f64.sqrt(), 1e-12); // √(9 + 0.81/12) = 3.011229
        close(c.rms_a, 3.011_229, 1e-6);
        // Forced-PWM light load: valley goes negative, formulas still hold.
        assert!(buck_inductor_currents(0.3, 0.9).unwrap().valley_a < 0.0);
        assert!(buck_inductor_currents(3.0, -0.1).is_err());
        assert!(buck_inductor_currents(0.0, 0.9).is_err());
    }

    #[test]
    fn buck_output_capacitor() {
        // 50 mV target, 5 mΩ ESR: C = 0.9 / (8 · 500 kHz · (0.05 − 0.0045)) = 4.945055 µF.
        let c = buck_output_cap_f(0.9, 500e3, 0.05, 0.005).unwrap();
        close(c, 0.9 / 182_000.0, 1e-18);
        close(c, 4.945_055e-6, 1e-12);
        close(
            buck_output_ripple_v(0.9, 500e3, c, 0.005).unwrap(),
            0.05,
            1e-12,
        );
        // 22 µF, 5 mΩ: 0.9/(8·0.5M·22µ) + 4.5 mV = 10.227 mV + 4.5 mV.
        close(
            buck_output_ripple_v(0.9, 500e3, 22e-6, 0.005).unwrap(),
            0.9 / 88.0 + 0.0045,
            1e-15,
        );
        // ESR alone (0.9 A · 60 mΩ = 54 mV) busts a 50 mV budget.
        assert!(matches!(
            buck_output_cap_f(0.9, 500e3, 0.05, 0.06),
            Err(CalcError::OutOfRange(_))
        ));
        close(buck_output_cap_rms_a(0.9).unwrap(), 0.259_807_6, 1e-7);
    }

    #[test]
    fn buck_input_capacitor_rms() {
        let d = 5.0 / 24.0;
        // 9 · D(1−D) + D · 0.81/12 = 1.484375 + 0.0140625 = 1.4984375 A².
        close(
            buck_input_cap_rms_a(d, 3.0, 0.9).unwrap(),
            1.498_437_5f64.sqrt(),
            1e-12,
        );
        // Without ripple: 3 · √(D(1−D)) = 1.218349 A.
        close(buck_input_cap_rms_a(d, 3.0, 0.0).unwrap(), 1.218_349, 1e-6);
        // Worst case D = 0.5: Iout / 2.
        close(buck_input_cap_rms_a(0.5, 3.0, 0.0).unwrap(), 1.5, 1e-12);
        assert!(buck_input_cap_rms_a(1.0, 3.0, 0.0).is_err());
    }

    fn reference_losses() -> BuckLossInput {
        BuckLossInput {
            vin_v: 24.0,
            vout_v: 5.0,
            iout_a: 3.0,
            fsw_hz: 500e3,
            ripple_a: 0.9,
            hs_rds_on_ohm: 0.010,
            ls_rds_on_ohm: 0.005,
            rise_s: 10e-9,
            fall_s: 10e-9,
            hs_qg_c: 10e-9,
            ls_qg_c: 10e-9,
            gate_drive_v: 5.0,
            dead_time_s: 40e-9,
            body_diode_vf_v: 0.7,
            ls_qrr_c: 10e-9,
            inductor_dcr_ohm: 0.015,
        }
    }

    #[test]
    fn buck_loss_breakdown() {
        let l = buck_losses(&reference_losses()).unwrap();
        // I²rms over a period = 9 + 0.81/12 = 9.0675 A².
        close(l.hs_conduction_w, 0.010 * 9.0675 * 5.0 / 24.0, 1e-15); // 0.018891
        close(l.ls_conduction_w, 0.005 * 9.0675 * 19.0 / 24.0, 1e-15); // 0.035892
        close(l.hs_switching_w, 0.36, 1e-12); // ½·24·3·20 ns·500 kHz
        close(l.reverse_recovery_w, 0.12, 1e-12); // 24·10 nC·500 kHz
        close(l.dead_time_w, 0.042, 1e-12); // 0.7·3·40 ns·500 kHz
        close(l.gate_drive_w, 0.05, 1e-12); // 20 nC·5 V·500 kHz
        close(l.inductor_dcr_w, 0.136_012_5, 1e-12); // 9.0675·15 mΩ
        close(l.total_loss_w, 0.762_795_312_5, 1e-12);
        close(l.output_power_w, 15.0, 1e-12);
        close(l.efficiency, 15.0 / 15.762_795_312_5, 1e-12); // 95.16 %
        close(l.hs_dissipation_w(), 0.498_890_625, 1e-12);
        close(l.ls_dissipation_w(), 0.077_892_187_5, 1e-12);
        // Junction temperatures from dissipation (θJA 40 °C/W, 25 °C ambient).
        close(
            junction_temp_c(25.0, l.hs_dissipation_w(), 40.0).unwrap(),
            44.955_625,
            1e-9,
        );
        close(
            junction_temp_c(25.0, l.ls_dissipation_w(), 40.0).unwrap(),
            28.115_687_5,
            1e-9,
        );
    }

    #[test]
    fn buck_losses_reject_bad_input() {
        let mut i = reference_losses();
        i.hs_rds_on_ohm = -0.01;
        assert!(buck_losses(&i).is_err());
        let mut i = reference_losses();
        i.dead_time_s = 2e-6; // 1 period
        assert!(buck_losses(&i).is_err());
        let mut i = reference_losses();
        i.vout_v = 30.0;
        assert!(buck_losses(&i).is_err());
    }

    #[test]
    fn e_series_tables() {
        assert_eq!(ESeries::E24.values_in(10.0, 99.0).len(), 24);
        assert_eq!(ESeries::E96.values_in(100.0, 999.0).len(), 96);
        let e24 = ESeries::E24.values_in(1e3, 10e3);
        assert_eq!(e24.first(), Some(&1e3));
        assert_eq!(e24.last(), Some(&10e3));
        assert!(e24.contains(&4.7e3) && e24.contains(&9.1e3));
        assert!(e24.windows(2).all(|w| w[0] < w[1]));
        assert!(ESeries::E96.values_in(1e3, 10e3).contains(&4.99e3));
        assert!(ESeries::E24.values_in(10.0, 1.0).is_empty());
    }

    #[test]
    fn feedback_divider_picks_series_values() {
        // Vref 0.6 V, 5 V target: Rtop/Rbot = 7.3333; first exact E24 pair is 11k/1.5k.
        let d = feedback_divider(0.6, 5.0, ESeries::E24, 1e3, 1e6, 0.01, 0.0).unwrap();
        assert_eq!((d.r_top_ohm, d.r_bottom_ohm), (11e3, 1.5e3));
        close(d.vout_v, 5.0, 1e-12);
        close(d.error_pct, 0.0, 1e-9);
        // 1 % resistors: 0.6·(1 + 7.3333·1.01/0.99) and 0.6·(1 + 7.3333·0.99/1.01).
        close(d.vout_max_v, 0.6 * (1.0 + 11.0 / 1.5 * 1.01 / 0.99), 1e-12); // 5.088889
        close(d.vout_min_v, 0.6 * (1.0 + 11.0 / 1.5 * 0.99 / 1.01), 1e-12); // 4.912871

        // A 1 % reference widens the band.
        let w = feedback_divider(0.6, 5.0, ESeries::E24, 1e3, 1e6, 0.01, 0.01).unwrap();
        assert!(w.vout_max_v > d.vout_max_v && w.vout_min_v < d.vout_min_v);

        // 0.8 V -> 3.3 V in E96: Rtop/Rbot = 3.125 lies midway between the E96 ratio steps
        // 10^(47/96) = 3.090 and 10^(48/96) = 3.162, so no near-exact pair exists. The search
        // must still beat the naive Rbot = 1k pick (3.16k: +1.12 %) and report its real error.
        let d = feedback_divider(0.8, 3.3, ESeries::E96, 1e3, 1e6, 0.01, 0.0).unwrap();
        let e96 = ESeries::E96.values_in(1e3, 1e6);
        assert!(e96.contains(&d.r_top_ohm) && e96.contains(&d.r_bottom_ohm));
        close(d.vout_v, 0.8 * (1.0 + d.r_top_ohm / d.r_bottom_ohm), 1e-12);
        close(d.error_v, d.vout_v - 3.3, 1e-12);
        assert!(d.error_pct.abs() < 1.12);
        // Regression pin of the deterministic search: 3.57k / 1.15k -> 3.2835 V (-0.50 %).
        assert_eq!((d.r_top_ohm, d.r_bottom_ohm), (3570.0, 1150.0));
        close(d.error_pct, -0.500_658_76, 1e-6);
        // Range too narrow for any pair, and non-physical targets.
        assert!(feedback_divider(0.8, 3.3, ESeries::E96, 1.001e3, 1.019e3, 0.01, 0.0).is_err());
        assert!(feedback_divider(0.8, 0.8, ESeries::E24, 1e3, 1e6, 0.01, 0.0).is_err());
        assert!(feedback_divider(0.8, 5.0, ESeries::E24, 1e3, 1e6, 1.0, 0.0).is_err());
    }
}
