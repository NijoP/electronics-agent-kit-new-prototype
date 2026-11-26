//! The copper-to-copper clearance rule (DRC-002), shared by the DRC and the interactive router
//! (ADR-0044), so a route the router proposes is never one the DRC then reports.
//!
//! Between copper of nets `a` and `b` the rule is the largest of the board minimum, the two net
//! classes' clearances and, when both nets declare a peak working voltage, the IPC-2221B
//! Table 6-1 spacing for their voltage difference (condition B2 on the outer layers, B1 on inner
//! layers; the table is not verified against the standard text).

use crate::ipc::{self, ClearanceCondition};
use eak_core::{Constraints, CopperLayer, InsulationStandard, Nm};

/// DRC-019 minimum drill-edge to drill-edge spacing (EAK default, common fabricator practice,
/// unverified), shared by the DRC and the autorouter.
pub const HOLE_TO_HOLE_MIN: Nm = 500_000;

fn outer(layer: CopperLayer) -> bool {
    matches!(layer, CopperLayer::Top | CopperLayer::Bottom)
}

/// The IPC-2221B condition on `layer`: B1 inner; outer B4 when coated, B3 above 3050 m, else B2
/// (also when no insulation environment is declared).
pub fn condition(c: &Constraints, layer: CopperLayer) -> ClearanceCondition {
    match &c.insulation {
        _ if !outer(layer) => ClearanceCondition::B1,
        Some(i) if i.coated => ClearanceCondition::B4,
        Some(i) if i.altitude_m > 3050.0 => ClearanceCondition::B3,
        _ => ClearanceCondition::B2,
    }
}

/// The voltage part of the rule between nets `a` and `b` on `layer`: the spacing (nm), what it
/// rests on and the voltage difference (V). `None` unless both nets declare a voltage. A declared
/// IEC 60664-1 creepage the transcribed table does not cover adds nothing here;
/// [`spacing_unknown`] reports it.
pub fn voltage_clearance(
    c: &Constraints,
    a: &str,
    b: &str,
    layer: CopperLayer,
) -> Option<(f64, String, f64)> {
    let (va, vb) = (c.net_voltages_v.get(a)?, c.net_voltages_v.get(b)?);
    let dv = (va - vb).abs();
    let cond = condition(c, layer);
    let mut nm = ipc::clearance_mm(dv, cond) * 1e6;
    let mut basis = format!("IPC-2221B {cond:?}");
    let iec = c
        .insulation
        .as_ref()
        .filter(|i| i.standard == InsulationStandard::Iec60664_1);
    if let Some(i) = iec.filter(|_| outer(layer)) {
        let (pd, mg) = (i.pollution_degree, &i.material_group);
        if let Ok(mm) = ipc::creepage_mm(dv, pd, mg) {
            if mm * 1e6 > nm {
                nm = mm * 1e6;
                basis = format!("IEC 60664-1 creepage PD{pd} {mg}");
            }
        }
    }
    Some((nm, basis, dv))
}

/// Why the voltage part of DRC-002 is not settled (`None` when it is): no insulation declared
/// while the declared voltages need more than the board minimum under the worst IPC-2221B outer
/// condition (B3), so altitude or coating would change the answer; or a declared IEC 60664-1
/// creepage the transcribed table does not cover. Uses the largest declared voltage difference.
pub fn spacing_unknown(c: &Constraints) -> Option<String> {
    let vs = c.net_voltages_v.values();
    let lo = vs.clone().fold(f64::MAX, |a, &b| a.min(b));
    let dv = vs.fold(f64::MIN, |a, &b| a.max(b)) - lo;
    if dv <= 0.0 {
        return None;
    }
    match &c.insulation {
        None => {
            let worst = ipc::clearance_mm(dv, ClearanceCondition::B3) * 1e6;
            let why = format!(
                "no insulation environment declared (Constraints.insulation): the spacing for {dv:.0} V depends on altitude and coating"
            );
            (worst > c.min_clearance as f64).then_some(why)
        }
        Some(i) if i.standard == InsulationStandard::Iec60664_1 => {
            let creep = ipc::creepage_mm(dv, i.pollution_degree, &i.material_group);
            creep
                .err()
                .map(|e| format!("IEC 60664-1 creepage for {dv:.0} V: {e}"))
        }
        Some(_) => None,
    }
}

/// The clearance (nm) DRC-002 requires between copper of nets `a` and `b` on `layer`.
pub fn required(c: &Constraints, a: &str, b: &str, layer: CopperLayer) -> f64 {
    // A net-pair rule (Phase 1 C5, ADR-0046) raises it like a net class does.
    let k = c
        .min_clearance
        .max(c.class_for(a).clearance)
        .max(c.class_for(b).clearance)
        .max(c.net_pair_clearance(a, b).unwrap_or(0)) as f64;
    voltage_clearance(c, a, b, layer).map_or(k, |(v, _, _)| k.max(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::NetClass;

    fn constraints() -> Constraints {
        let mut c = Constraints {
            min_clearance: 150_000,
            ..Default::default()
        };
        c.default_class.clearance = 200_000;
        c.net_classes.push(NetClass {
            name: "HV".into(),
            track_width: 500_000,
            clearance: 400_000,
            via_drill: 400_000,
            via_diameter: 800_000,
            nets: vec!["HV".into()],
            target_z0_ohm: None,
        });
        c
    }

    #[test]
    fn net_classes_and_the_board_minimum_without_voltages() {
        let c = constraints();
        assert_eq!(required(&c, "A", "B", CopperLayer::Top), 200_000.0);
        assert_eq!(required(&c, "A", "HV", CopperLayer::Top), 400_000.0);
        assert!(voltage_clearance(&c, "A", "HV", CopperLayer::Top).is_none());
    }

    #[test]
    fn declared_voltages_raise_the_rule_by_layer_condition() {
        let mut c = constraints();
        c.net_voltages_v.insert("HV".into(), 300.0);
        c.net_voltages_v.insert("GND".into(), 0.0);
        // Only one side declared: the voltage part does not apply.
        assert_eq!(required(&c, "A", "HV", CopperLayer::Top), 400_000.0);
        let outer = required(&c, "GND", "HV", CopperLayer::Top);
        let inner = required(&c, "GND", "HV", CopperLayer::Inner(1));
        let b2 = ipc::clearance_mm(300.0, ClearanceCondition::B2) * 1e6;
        let b1 = ipc::clearance_mm(300.0, ClearanceCondition::B1) * 1e6;
        assert_eq!(outer, b2.max(400_000.0));
        assert_eq!(inner, b1.max(400_000.0));
        assert!(
            outer > 400_000.0,
            "300 V on an outer layer needs more than the class"
        );
        // The difference counts, not each voltage.
        c.net_voltages_v.insert("GND".into(), 300.0);
        assert_eq!(
            required(&c, "GND", "HV", CopperLayer::Top),
            400_000.0_f64.max(ipc::clearance_mm(0.0, ClearanceCondition::B2) * 1e6)
        );
    }

    #[test]
    fn declared_insulation_picks_the_condition_and_iec_creepage() {
        use eak_core::Insulation;
        let mut c = constraints();
        c.net_voltages_v.insert("HV".into(), 300.0);
        c.net_voltages_v.insert("GND".into(), 0.0);
        // 300 V needs more than the 0.15 mm minimum at altitude: UNKNOWN until declared.
        assert!(spacing_unknown(&c).is_some());
        let mut ins = Insulation {
            standard: InsulationStandard::Ipc2221b,
            pollution_degree: 2,
            material_group: "IIIb".into(),
            altitude_m: 4000.0,
            coated: false,
        };
        c.insulation = Some(ins.clone());
        assert_eq!(spacing_unknown(&c), None);
        assert_eq!(condition(&c, CopperLayer::Top), ClearanceCondition::B3);
        assert_eq!(condition(&c, CopperLayer::Inner(1)), ClearanceCondition::B1);
        let b3 = ipc::clearance_mm(300.0, ClearanceCondition::B3) * 1e6;
        let top = required(&c, "GND", "HV", CopperLayer::Top);
        assert_eq!(top, b3.max(400_000.0));
        // IEC 60664-1: the outer-layer rule is at least the creepage.
        ins.standard = InsulationStandard::Iec60664_1;
        ins.altitude_m = 0.0;
        c.insulation = Some(ins.clone());
        let creep = ipc::creepage_mm(300.0, 2, "IIIb").unwrap() * 1e6;
        let v = voltage_clearance(&c, "GND", "HV", CopperLayer::Top);
        let (nm, basis, _) = v.unwrap();
        assert!(nm >= creep, "{nm} < {creep}");
        assert!(basis.starts_with("IEC 60664-1"), "{basis}");
        // Pollution degree 3 is outside the transcribed creepage table: UNKNOWN.
        ins.pollution_degree = 3;
        c.insulation = Some(ins);
        assert!(spacing_unknown(&c).is_some());
    }

    /// Restored from 3c9901f (dropped in the #140 merge, 500e938), ported onto `required`, which
    /// the router and the DRC now share: the declared insulation picks the outer condition (B3 at
    /// altitude, B4 coated) and IEC 60664-1 adds creepage on outer layers only.
    #[test]
    fn routing_spacing_follows_the_declared_insulation() {
        let mut c = constraints();
        c.net_voltages_v.insert("HV".into(), 300.0);
        c.net_voltages_v.insert("GND".into(), 0.0);
        let ipc_at = |cond| ipc::clearance_mm(300.0, cond) * 1e6;
        let b2 = required(&c, "GND", "HV", CopperLayer::Top);
        assert_eq!(b2, 400_000.0_f64.max(ipc_at(ClearanceCondition::B2)));
        let ins = |standard, altitude_m, coated| eak_core::Insulation {
            standard,
            pollution_degree: 2,
            material_group: "IIIa".into(),
            altitude_m,
            coated,
        };
        c.insulation = Some(ins(InsulationStandard::Ipc2221b, 4000.0, false));
        let b3 = required(&c, "GND", "HV", CopperLayer::Top);
        assert_eq!(b3, b2.max(ipc_at(ClearanceCondition::B3)));
        assert!(b3 > b2, "B3 above 3050 m needs more than B2");
        c.insulation = Some(ins(InsulationStandard::Ipc2221b, 0.0, true));
        let b4 = required(&c, "GND", "HV", CopperLayer::Top);
        assert_eq!(b4, 400_000.0_f64.max(ipc_at(ClearanceCondition::B4)));
        c.insulation = Some(ins(InsulationStandard::Iec60664_1, 0.0, false));
        let creep = ipc::creepage_mm(300.0, 2, "IIIa").unwrap() * 1e6;
        assert_eq!(required(&c, "GND", "HV", CopperLayer::Top), b2.max(creep));
        // IEC inner layers: no creepage path, the B1 rule alone.
        assert_eq!(
            required(&c, "GND", "HV", CopperLayer::Inner(1)),
            400_000.0_f64.max(ipc_at(ClearanceCondition::B1))
        );
    }
}
