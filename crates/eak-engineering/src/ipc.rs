//! Electrical sizing rules: trace ampacity, conductor clearance, creepage, via current.
//!
//! Provenance: independently re-implemented in Rust from the rule set documented in
//! DESK `desk/ipc.py` (commit 5ad8492c37a5dac919d97f4eccd8250780490e7a), which cites
//! IPC-2221 / IPC-2221B Table 6-1 / IEC 60664-1 Table F.4. The DESK reference values are
//! reproduced as regression tests below.
//!
//! VERIFICATION STATUS: `VERIFIED_AGAINST_STANDARD = false`. The tabulated values were
//! transcribed from secondary sources and have not been checked against the purchased
//! standard text. Results derived from these tables are reported by EAK with this status.
//! See docs/engineering/ELECTRICAL_RULES.md for flagged concerns (notably the creepage
//! table's discontinuity at 50 V, which mixes printed-wiring and other-material columns).

pub const VERIFIED_AGAINST_STANDARD: bool = false;
pub const MIL_PER_MM: f64 = 39.37007874;
/// Copper thickness per ounce: 1 oz/ft² ≈ 34.8 µm ≈ 1.37 mil.
pub const MIL_PER_OZ: f64 = 1.37;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CalcError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    OutOfRange(String),
}

fn k(internal: bool) -> f64 {
    if internal {
        0.024
    } else {
        0.048
    }
}

/// Required copper cross-section (mil²) for `current_a` at `temp_rise_c` (IPC-2221:
/// I = k · ΔT^0.44 · A^0.725).
pub fn trace_area_mil2(current_a: f64, internal: bool, temp_rise_c: f64) -> Result<f64, CalcError> {
    if !(current_a > 0.0) {
        return Err(CalcError::InvalidInput(
            "current must be > 0: unknown current is never sized".into(),
        ));
    }
    if !(temp_rise_c > 0.0) {
        return Err(CalcError::InvalidInput(
            "temperature rise must be > 0".into(),
        ));
    }
    Ok((current_a / (k(internal) * temp_rise_c.powf(0.44))).powf(1.0 / 0.725))
}

/// Required trace width in mil.
pub fn trace_width_mil(
    current_a: f64,
    internal: bool,
    copper_oz: f64,
    temp_rise_c: f64,
) -> Result<f64, CalcError> {
    if !(copper_oz > 0.0) {
        return Err(CalcError::InvalidInput("copper weight must be > 0".into()));
    }
    Ok(trace_area_mil2(current_a, internal, temp_rise_c)? / (copper_oz * MIL_PER_OZ))
}

/// Required trace width in mm.
pub fn trace_width_mm(
    current_a: f64,
    internal: bool,
    copper_oz: f64,
    temp_rise_c: f64,
) -> Result<f64, CalcError> {
    Ok(trace_width_mil(current_a, internal, copper_oz, temp_rise_c)? / MIL_PER_MM)
}

/// Current capacity (A) of a trace of `width_mil`.
pub fn trace_current_a(width_mil: f64, internal: bool, copper_oz: f64, temp_rise_c: f64) -> f64 {
    if width_mil <= 0.0 || copper_oz <= 0.0 || temp_rise_c <= 0.0 {
        return 0.0;
    }
    let area = width_mil * copper_oz * MIL_PER_OZ;
    k(internal) * temp_rise_c.powf(0.44) * area.powf(0.725)
}

const CLEARANCE_BANDS_V: [f64; 9] = [15.0, 30.0, 50.0, 100.0, 150.0, 170.0, 250.0, 300.0, 500.0];

/// IPC-2221B Table 6-1 conductor spacing conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ClearanceCondition {
    /// Internal conductors.
    B1,
    /// External, uncoated, sea level to 3050 m.
    B2,
    /// External, uncoated, above 3050 m.
    B3,
    /// External, permanent polymer coating.
    B4,
    /// External component leads, uncoated.
    A5,
    /// External component leads, with coating.
    A6,
    /// External component leads, conformal coat.
    A7,
}

fn clearance_table(c: ClearanceCondition) -> ([f64; 9], f64) {
    use ClearanceCondition::*;
    match c {
        B1 => (
            [0.05, 0.05, 0.10, 0.10, 0.20, 0.20, 0.20, 0.20, 0.25],
            0.0025,
        ),
        B2 => (
            [0.10, 0.10, 0.60, 0.60, 0.60, 1.25, 1.25, 1.25, 2.50],
            0.0050,
        ),
        B3 => (
            [0.10, 0.10, 0.60, 1.50, 3.20, 3.20, 6.40, 12.5, 12.5],
            0.0250,
        ),
        B4 => (
            [0.05, 0.05, 0.13, 0.13, 0.40, 0.40, 0.40, 0.80, 0.80],
            0.00305,
        ),
        A5 => (
            [0.13, 0.13, 0.13, 0.13, 0.40, 0.40, 0.40, 0.80, 0.80],
            0.00305,
        ),
        A6 => (
            [0.13, 0.25, 0.40, 0.50, 0.80, 0.80, 0.80, 0.80, 1.50],
            0.00305,
        ),
        A7 => (
            [0.13, 0.13, 0.13, 0.13, 0.40, 0.40, 0.40, 0.80, 0.80],
            0.00305,
        ),
    }
}

/// Minimum conductor spacing (mm) for a peak working voltage (IPC-2221B Table 6-1).
pub fn clearance_mm(voltage_v: f64, cond: ClearanceCondition) -> f64 {
    let v = voltage_v.abs();
    let (table, per_volt) = clearance_table(cond);
    for (band, mm) in CLEARANCE_BANDS_V.iter().zip(table) {
        if v <= *band {
            return mm;
        }
    }
    table[8] + per_volt * (v - 500.0)
}

const CREEPAGE_V: [f64; 21] = [
    10.0, 12.5, 16.0, 20.0, 25.0, 32.0, 40.0, 50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0,
    320.0, 400.0, 500.0, 630.0, 800.0, 1000.0,
];
const CREEPAGE_PD1: [f64; 21] = [
    0.025, 0.025, 0.025, 0.025, 0.025, 0.025, 0.025, 0.025, 0.040, 0.063, 0.100, 0.160, 0.250,
    0.400, 0.560, 0.750, 1.0, 1.3, 1.8, 2.4, 3.2,
];
const CREEPAGE_PD2_I: [f64; 21] = [
    0.04, 0.04, 0.04, 0.04, 0.04, 0.04, 0.04, 0.60, 0.63, 0.67, 0.71, 0.75, 0.80, 1.00, 1.25, 1.60,
    2.0, 2.5, 3.2, 4.0, 5.0,
];
const CREEPAGE_PD2_II: [f64; 21] = [
    0.04, 0.04, 0.04, 0.04, 0.04, 0.04, 0.04, 0.85, 0.90, 0.95, 1.00, 1.05, 1.10, 1.40, 1.80, 2.20,
    2.8, 3.6, 4.5, 5.6, 7.1,
];
const CREEPAGE_PD2_III: [f64; 21] = [
    0.04, 0.04, 0.04, 0.04, 0.04, 0.04, 0.04, 1.20, 1.25, 1.30, 1.40, 1.50, 1.60, 2.00, 2.50, 3.20,
    4.0, 5.0, 6.3, 8.0, 10.0,
];

/// Minimum creepage (mm), IEC 60664-1 Table F.4 as transcribed by DESK. Voltage rounds up
/// to the next table row. Pollution degree 3/4 and > 1000 V are refused, not guessed.
pub fn creepage_mm(
    voltage_v: f64,
    pollution_degree: u8,
    material_group: &str,
) -> Result<f64, CalcError> {
    let v = voltage_v.abs();
    if v > 1000.0 {
        return Err(CalcError::OutOfRange(
            "creepage above 1000 V is outside the implemented table".into(),
        ));
    }
    let table = match pollution_degree {
        1 => &CREEPAGE_PD1,
        2 => match material_group
            .trim_end_matches(['a', 'b', 'A', 'B'])
            .to_ascii_uppercase()
            .as_str()
        {
            "I" => &CREEPAGE_PD2_I,
            "II" => &CREEPAGE_PD2_II,
            "III" => &CREEPAGE_PD2_III,
            _ => {
                return Err(CalcError::InvalidInput(
                    "material group must be I, II, IIIa or IIIb".into(),
                ))
            }
        },
        _ => {
            return Err(CalcError::OutOfRange(
                "pollution degree 3 and 4 are not implemented".into(),
            ))
        }
    };
    for (row, mm) in CREEPAGE_V.iter().zip(table.iter()) {
        if v <= *row {
            return Ok(*mm);
        }
    }
    Ok(table[20])
}

/// Copper cross-section of a plated via barrel, approximated as π·(d+t)·t (mil²).
pub fn via_wall_area_mil2(drill_mil: f64, plating_mil: f64) -> Result<f64, CalcError> {
    if !(drill_mil > 0.0 && plating_mil > 0.0) {
        return Err(CalcError::InvalidInput(
            "drill and plating must be > 0".into(),
        ));
    }
    Ok(std::f64::consts::PI * (drill_mil + plating_mil) * plating_mil)
}

/// Current capacity (A) of one via, using the IPC-2221 *internal* constant because the
/// barrel is buried in laminate. (DESK's module header says "external k" but its code and
/// tests use the internal constant; EAK follows the conservative internal constant.)
pub fn via_current_a(drill_mil: f64, plating_mil: f64, temp_rise_c: f64) -> Result<f64, CalcError> {
    let area = via_wall_area_mil2(drill_mil, plating_mil)?;
    Ok(0.024 * temp_rise_c.powf(0.44) * area.powf(0.725))
}

/// Number of parallel vias for `current_a` with a current-sharing derating (default 0.8).
pub fn vias_required(
    current_a: f64,
    drill_mil: f64,
    plating_mil: f64,
    temp_rise_c: f64,
    derating: f64,
) -> Result<u32, CalcError> {
    if !(current_a > 0.0) {
        return Err(CalcError::InvalidInput("current must be > 0".into()));
    }
    let per = via_current_a(drill_mil, plating_mil, temp_rise_c)? * derating;
    Ok(((current_a / per).ceil() as u32).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ClearanceCondition::*;

    // Reference values reproduced from DESK tests/test_ipc_models.py.
    #[test]
    fn reference_widths() {
        let w = |i, inner| trace_width_mil(i, inner, 1.0, 10.0).unwrap();
        assert!((w(1.0, false) - 11.9).abs() < 0.3);
        assert!((w(3.0, false) - 54.1).abs() < 1.0);
        assert!((w(10.0, false) - 284.9).abs() < 4.0);
        assert!((w(1.0, true) - 30.9).abs() < 0.7);
    }

    #[test]
    fn two_ounce_halves_width() {
        let a = trace_width_mil(2.0, false, 1.0, 10.0).unwrap();
        let b = trace_width_mil(2.0, false, 2.0, 10.0).unwrap();
        assert!((a / b - 2.0).abs() < 1e-9);
    }

    #[test]
    fn inverse_round_trip() {
        for i in [0.5, 1.0, 2.0, 5.0] {
            let w = trace_width_mil(i, false, 1.0, 10.0).unwrap();
            assert!((trace_current_a(w, false, 1.0, 10.0) - i).abs() < 1e-6);
        }
    }

    #[test]
    fn unknown_current_is_error() {
        assert!(trace_width_mil(0.0, false, 1.0, 10.0).is_err());
        assert!(trace_width_mil(f64::NAN, false, 1.0, 10.0).is_err());
        assert!(trace_width_mil(1.0, false, 1.0, 0.0).is_err());
    }

    #[test]
    fn clearance_table_rows() {
        assert_eq!(clearance_mm(5.0, B2), 0.10);
        assert_eq!(clearance_mm(48.0, B2), 0.60);
        assert_eq!(clearance_mm(230.0, B2), 1.25);
        assert_eq!(clearance_mm(400.0, B2), 2.50);
        assert!((clearance_mm(600.0, B2) - 3.0).abs() < 1e-9);
        assert!((clearance_mm(1000.0, B1) - (0.25 + 0.0025 * 500.0)).abs() < 1e-9);
        assert_eq!(clearance_mm(300.0, B1), 0.20);
        assert_eq!(clearance_mm(300.0, B3), 12.5);
    }

    #[test]
    fn creepage_rows() {
        assert_eq!(creepage_mm(250.0, 2, "IIIb").unwrap(), 2.5);
        assert_eq!(creepage_mm(230.0, 2, "IIIa").unwrap(), 2.5);
        assert_eq!(creepage_mm(320.0, 2, "III").unwrap(), 3.2);
        assert!(creepage_mm(250.0, 1, "I").unwrap() < creepage_mm(250.0, 2, "I").unwrap());
        assert!(creepage_mm(1200.0, 2, "I").is_err());
        assert!(creepage_mm(100.0, 3, "I").is_err());
    }

    #[test]
    fn single_via_reference() {
        let i = via_current_a(12.0, 1.0, 10.0).unwrap();
        assert!(i > 0.9 && i < 1.3, "{i}");
        assert_eq!(vias_required(3.0, 12.0, 1.0, 10.0, 0.8).unwrap(), 4);
    }
}
