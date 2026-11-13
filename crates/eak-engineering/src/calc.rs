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
}
