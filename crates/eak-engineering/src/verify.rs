//! Engineering verification engines: explicit typed inputs to a [`Check`] that is `PASS`,
//! `FAIL`, `UNKNOWN` (a named input is missing) or `BLOCKED` (the inputs are present but outside
//! what the method covers). Nothing here substitutes a default for a missing input, and every
//! result carries the formula or standard it rests on.

use crate::ipc;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    Pass,
    Fail,
    /// A required input is missing; `Check::reason` names it.
    Unknown,
    /// The inputs are present but invalid or outside the method's range; `Check::reason` says why.
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bound {
    /// `value` must not exceed `limit`.
    Max,
    /// `value` must reach `limit`.
    Min,
}

/// One verification result: `value` checked against `limit` (both in `unit`) by `bound`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub verdict: Verdict,
    pub value: Option<f64>,
    pub limit: Option<f64>,
    pub unit: String,
    pub bound: Bound,
    /// The missing inputs (UNKNOWN), why the method does not apply (BLOCKED), else empty.
    pub reason: String,
    pub source: String,
}

impl Check {
    fn new(verdict: Verdict, bound: Bound, unit: &str, reason: String, source: &str) -> Check {
        Check {
            verdict,
            value: None,
            limit: None,
            unit: unit.into(),
            bound,
            reason,
            source: source.into(),
        }
    }

    /// UNKNOWN naming each input whose flag is `true` (missing).
    pub fn unknown(missing: &[(&str, bool)], bound: Bound, unit: &str, source: &str) -> Check {
        let names: Vec<&str> = missing.iter().filter(|m| m.1).map(|m| m.0).collect();
        let reason = format!("missing input: {}", names.join(", "));
        Check::new(Verdict::Unknown, bound, unit, reason, source)
    }

    pub fn blocked(why: impl Into<String>, bound: Bound, unit: &str, source: &str) -> Check {
        Check::new(Verdict::Blocked, bound, unit, why.into(), source)
    }

    /// PASS or FAIL of `value` against `limit`; BLOCKED when either is not finite.
    pub fn compare(value: f64, limit: f64, bound: Bound, unit: &str, source: &str) -> Check {
        if !(value.is_finite() && limit.is_finite()) {
            return Check::blocked("computed value or limit is not finite", bound, unit, source);
        }
        let ok = match bound {
            Bound::Max => value <= limit,
            Bound::Min => value >= limit,
        };
        let verdict = if ok { Verdict::Pass } else { Verdict::Fail };
        Check {
            value: Some(value),
            limit: Some(limit),
            ..Check::new(verdict, bound, unit, String::new(), source)
        }
    }
}

fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}

pub const SOURCE_VIA: &str = "IPC-2221 conductor formula on the via barrel cross-section π·(d+t)·t with the internal constant k = 0.024 (eak_engineering::ipc::via_current_a); verified_against_standard = false; IPC-2152 not implemented";
pub const SOURCE_DROP: &str =
    "Ohm's law V = I·R; R from the copper resistivity model of eak_engineering::power";
pub const SOURCE_TJ: &str = "Tj = Ta + P·θJA (one-resistor steady-state estimate; θJA from the part's datasheet, board-dependent)";
pub const SOURCE_RATING: &str =
    "stress against rating × derating fraction (eak_engineering::rating::DERATING)";

/// Net current through `vias` parallel vias against their combined IPC-2221 capacity. Equal
/// current sharing is assumed (no derating), which the caller must state.
pub fn via_current(
    current_a: Option<f64>,
    drill_mm: f64,
    plating_um: Option<f64>,
    temp_rise_c: Option<f64>,
    vias: u32,
) -> Check {
    let (b, u, s) = (Bound::Max, "A", SOURCE_VIA);
    let (Some(i), Some(t), Some(rise)) = (current_a, plating_um, temp_rise_c) else {
        let missing = [
            ("net current", current_a.is_none()),
            ("via plating thickness", plating_um.is_none()),
            ("allowed temperature rise", temp_rise_c.is_none()),
        ];
        return Check::unknown(&missing, b, u, s);
    };
    if !(positive(i) && positive(t) && positive(rise) && positive(drill_mm) && vias > 0) {
        let why = "current, plating, temperature rise, drill and via count must all be positive";
        return Check::blocked(why, b, u, s);
    }
    match ipc::via_current_a(drill_mm * ipc::MIL_PER_MM, t / 25.4, rise) {
        Ok(per) => Check::compare(i, per * f64::from(vias), b, u, s),
        Err(e) => Check::blocked(e.to_string(), b, u, s),
    }
}

/// DC voltage drop `current × resistance` against the allowed drop.
pub fn voltage_drop(
    current_a: Option<f64>,
    resistance_ohm: Option<f64>,
    allowed_v: Option<f64>,
) -> Check {
    let (b, u, s) = (Bound::Max, "V", SOURCE_DROP);
    let (Some(i), Some(r), Some(max)) = (current_a, resistance_ohm, allowed_v) else {
        let missing = [
            ("net current", current_a.is_none()),
            ("path resistance", resistance_ohm.is_none()),
            ("allowed voltage drop", allowed_v.is_none()),
        ];
        return Check::unknown(&missing, b, u, s);
    };
    if !(positive(i) && positive(r) && positive(max)) {
        let why = "current, resistance and allowed drop must be positive";
        return Check::blocked(why, b, u, s);
    }
    Check::compare(i * r, max, b, u, s)
}

/// Junction temperature estimate `Ta + P·θJA` against the part's maximum junction temperature.
pub fn junction_temperature(
    power_w: Option<f64>,
    theta_ja_c_per_w: Option<f64>,
    ambient_c: Option<f64>,
    tj_max_c: Option<f64>,
) -> Check {
    let (b, u, s) = (Bound::Max, "°C", SOURCE_TJ);
    let (Some(p), Some(th), Some(ta), Some(tj)) = (power_w, theta_ja_c_per_w, ambient_c, tj_max_c)
    else {
        let missing = [
            ("dissipation", power_w.is_none()),
            ("θJA", theta_ja_c_per_w.is_none()),
            ("ambient temperature", ambient_c.is_none()),
            ("maximum junction temperature", tj_max_c.is_none()),
        ];
        return Check::unknown(&missing, b, u, s);
    };
    if !(p.is_finite() && p >= 0.0 && positive(th) && ta.is_finite() && tj.is_finite()) {
        return Check::blocked("dissipation must be ≥ 0 and θJA positive", b, u, s);
    }
    Check::compare(ta + p * th, tj, b, u, s)
}

/// Applied stress (voltage, current or power, sign ignored) against `max_fraction` of the part
/// rating. Exceeding the derated limit is `FAIL`; the caller tells it from exceeding the rating.
pub fn part_rating(
    applied: Option<f64>,
    rating: Option<f64>,
    max_fraction: f64,
    unit: &str,
) -> Check {
    let (b, s) = (Bound::Max, SOURCE_RATING);
    let (Some(a), Some(r)) = (applied, rating) else {
        let missing = [
            ("applied stress", applied.is_none()),
            ("part rating", rating.is_none()),
        ];
        return Check::unknown(&missing, b, unit, s);
    };
    if !(positive(r) && max_fraction > 0.0 && max_fraction <= 1.0 && a.is_finite()) {
        let why = "rating must be positive and the derating in (0, 1]";
        return Check::blocked(why, b, unit, s);
    }
    Check::compare(a.abs(), r * max_fraction, b, unit, s)
}

/// Insulation standard a spacing rule is declared against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsulationStandard {
    /// IPC-2221B Table 6-1 conductor spacing (clearance).
    Ipc2221b,
    /// IEC 60664-1 creepage (Table F.4 as transcribed); its clearance table is not implemented.
    Iec60664_1,
}

/// The declared standard and environment for voltage spacing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Insulation {
    pub standard: InsulationStandard,
    pub pollution_degree: u8,
    /// IEC 60664-1 material group: I, II, IIIa or IIIb.
    pub material_group: String,
    pub altitude_m: f64,
    /// Permanent polymer coating (IPC-2221B condition B4).
    pub coated: bool,
}

/// Sources of the spacing [`voltage_spacing`] requires.
pub const SOURCE_IPC_SPACING: &str = "IPC-2221B Table 6-1 (B1 inner; B2 outer uncoated to 3050 m; B3 outer uncoated above 3050 m; B4 outer coated), via eak_engineering::ipc::clearance_mm; verified_against_standard = false";
pub const SOURCE_IEC_CREEPAGE: &str = "IEC 60664-1 Table F.4 creepage as transcribed (eak_engineering::ipc::creepage_mm); verified_against_standard = false";

/// Measured spacing (mm) between conductors at `working_v` volts against the spacing the
/// declared standard requires. No declaration is UNKNOWN; a case the transcribed tables do not
/// cover (pollution degree 3 and 4, creepage above 1000 V, inner layers under IEC) is BLOCKED.
/// IEC 60664-1 is applied as creepage only: its clearance table needs the rated impulse voltage
/// and is not implemented, so an IEC declaration never certifies the air gap.
pub fn voltage_spacing(
    measured_mm: f64,
    working_v: Option<f64>,
    insulation: Option<&Insulation>,
    outer_layer: bool,
) -> Check {
    let (b, u) = (Bound::Min, "mm");
    let (Some(v), Some(ins)) = (working_v, insulation) else {
        let missing = [
            ("working voltage", working_v.is_none()),
            ("insulation standard and environment", insulation.is_none()),
        ];
        return Check::unknown(&missing, b, u, SOURCE_IPC_SPACING);
    };
    if !(v.is_finite() && ins.altitude_m.is_finite()) {
        let why = "working voltage and altitude must be finite";
        return Check::blocked(why, b, u, SOURCE_IPC_SPACING);
    }
    match ins.standard {
        InsulationStandard::Ipc2221b => {
            let cond = match (outer_layer, ins.coated, ins.altitude_m > 3050.0) {
                (false, _, _) => ipc::ClearanceCondition::B1,
                (true, true, _) => ipc::ClearanceCondition::B4,
                (true, false, true) => ipc::ClearanceCondition::B3,
                (true, false, false) => ipc::ClearanceCondition::B2,
            };
            let req = ipc::clearance_mm(v, cond);
            Check::compare(measured_mm, req, b, u, SOURCE_IPC_SPACING)
        }
        InsulationStandard::Iec60664_1 if !outer_layer => Check::blocked(
            "inner layers: solid insulation, not an IEC 60664-1 creepage path",
            b,
            u,
            SOURCE_IEC_CREEPAGE,
        ),
        InsulationStandard::Iec60664_1 => {
            let req = ipc::creepage_mm(v, ins.pollution_degree, &ins.material_group);
            match req {
                Ok(req) => Check::compare(measured_mm, req, b, u, SOURCE_IEC_CREEPAGE),
                Err(e) => Check::blocked(e.to_string(), b, u, SOURCE_IEC_CREEPAGE),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn via_current_outcomes() {
        let per = ipc::via_current_a(0.3 * ipc::MIL_PER_MM, 25.0 / 25.4, 10.0).unwrap();
        let ok = via_current(Some(per * 1.9), 0.3, Some(25.0), Some(10.0), 2);
        assert_eq!(ok.verdict, Verdict::Pass);
        assert!((ok.limit.unwrap() - 2.0 * per).abs() < 1e-9);
        let over = via_current(Some(per * 1.1), 0.3, Some(25.0), Some(10.0), 1);
        assert_eq!(over.verdict, Verdict::Fail);
        let unknown = via_current(Some(1.0), 0.3, None, Some(10.0), 1);
        assert_eq!(unknown.verdict, Verdict::Unknown);
        assert_eq!(unknown.reason, "missing input: via plating thickness");
        let zero = via_current(Some(1.0), 0.3, Some(25.0), Some(10.0), 0);
        assert_eq!(zero.verdict, Verdict::Blocked);
    }

    #[test]
    fn voltage_drop_outcomes() {
        let ok = voltage_drop(Some(2.0), Some(0.05), Some(0.15));
        assert_eq!(ok.verdict, Verdict::Pass);
        let f = voltage_drop(Some(2.0), Some(0.1), Some(0.15));
        assert_eq!(f.verdict, Verdict::Fail);
        assert!((f.value.unwrap() - 0.2).abs() < 1e-12);
        let u = voltage_drop(Some(2.0), Some(0.1), None);
        assert_eq!(u.verdict, Verdict::Unknown);
        assert_eq!(u.reason, "missing input: allowed voltage drop");
        let bad = voltage_drop(Some(-1.0), Some(0.1), Some(1.0));
        assert_eq!(bad.verdict, Verdict::Blocked);
    }

    #[test]
    fn junction_outcomes() {
        // 0.85 W at 90 °C/W from 25 °C: Tj = 101.5 °C.
        let ok = junction_temperature(Some(0.85), Some(90.0), Some(25.0), Some(125.0));
        assert_eq!(ok.verdict, Verdict::Pass);
        assert!((ok.value.unwrap() - 101.5).abs() < 1e-9);
        let hot = junction_temperature(Some(0.85), Some(90.0), Some(50.0), Some(125.0));
        assert_eq!(hot.verdict, Verdict::Fail);
        let u = junction_temperature(Some(0.85), None, None, Some(125.0));
        assert_eq!(u.verdict, Verdict::Unknown);
        assert_eq!(u.reason, "missing input: θJA, ambient temperature");
        let bad = junction_temperature(Some(0.85), Some(0.0), Some(25.0), Some(125.0));
        assert_eq!(bad.verdict, Verdict::Blocked);
    }

    fn env(standard: InsulationStandard, pd: u8, altitude_m: f64, coated: bool) -> Insulation {
        Insulation {
            standard,
            pollution_degree: pd,
            material_group: "IIIb".into(),
            altitude_m,
            coated,
        }
    }

    #[test]
    fn voltage_spacing_follows_the_declared_standard_and_environment() {
        use InsulationStandard::*;
        let ipc = env(Ipc2221b, 2, 0.0, false);
        // 230 V, outer uncoated (B2): 1.25 mm.
        let ok = voltage_spacing(1.3, Some(230.0), Some(&ipc), true);
        assert_eq!((ok.verdict, ok.limit), (Verdict::Pass, Some(1.25)));
        let close = voltage_spacing(1.0, Some(230.0), Some(&ipc), true);
        assert_eq!(close.verdict, Verdict::Fail);
        // Above 3050 m (B3) the same gap fails: 6.4 mm.
        let mountain = env(Ipc2221b, 2, 4000.0, false);
        let high = voltage_spacing(1.3, Some(230.0), Some(&mountain), true);
        assert_eq!((high.verdict, high.limit), (Verdict::Fail, Some(6.4)));
        // Coated (B4) and inner (B1) need less.
        let coat = env(Ipc2221b, 2, 0.0, true);
        let coated = voltage_spacing(0.5, Some(230.0), Some(&coat), true);
        assert_eq!((coated.verdict, coated.limit), (Verdict::Pass, Some(0.4)));
        let inner = voltage_spacing(0.2, Some(230.0), Some(&ipc), false);
        assert_eq!(inner.verdict, Verdict::Pass);
        // IEC 60664-1 PD2, material group IIIb, 250 V: 2.5 mm creepage.
        let iec = env(Iec60664_1, 2, 0.0, false);
        let creep = voltage_spacing(2.0, Some(250.0), Some(&iec), true);
        assert_eq!((creep.verdict, creep.limit), (Verdict::Fail, Some(2.5)));
        // No declaration: UNKNOWN; outside the transcribed table: BLOCKED.
        let none = voltage_spacing(5.0, Some(250.0), None, true);
        assert!(none.reason.contains("insulation"), "{}", none.reason);
        let dirty = env(Iec60664_1, 3, 0.0, false);
        let pd3 = voltage_spacing(5.0, Some(250.0), Some(&dirty), true);
        assert_eq!(pd3.verdict, Verdict::Blocked);
    }

    #[test]
    fn part_rating_outcomes() {
        let ok = part_rating(Some(3.3), Some(6.0), 0.8, "V");
        assert_eq!(ok.verdict, Verdict::Pass);
        let f = part_rating(Some(-5.0), Some(6.0), 0.8, "V");
        assert_eq!(f.verdict, Verdict::Fail);
        assert!((f.limit.unwrap() - 4.8).abs() < 1e-12);
        let u = part_rating(Some(5.0), None, 0.8, "V");
        assert_eq!(u.reason, "missing input: part rating");
        let bad = part_rating(Some(5.0), Some(6.0), 1.5, "V");
        assert_eq!(bad.verdict, Verdict::Blocked);
    }
}
