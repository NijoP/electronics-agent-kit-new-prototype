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

/// The voltage part of the rule between nets `a` and `b` on `layer`: the IPC-2221B spacing (nm),
/// its condition and the voltage difference (V). `None` unless both nets declare a voltage.
pub fn voltage_clearance(
    c: &Constraints,
    a: &str,
    b: &str,
    layer: CopperLayer,
) -> Option<(f64, ClearanceCondition, f64)> {
    let (va, vb) = (c.net_voltages_v.get(a)?, c.net_voltages_v.get(b)?);
    let cond = if matches!(layer, CopperLayer::Top | CopperLayer::Bottom) {
        ClearanceCondition::B2
    } else {
        ClearanceCondition::B1
    };
    let dv = (va - vb).abs();
    Some((ipc::clearance_mm(dv, cond) * 1e6, cond, dv))
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

/// The spacing (nm) the declared insulation (`Constraints::insulation`, schema v8) requires
/// between nets `a` and `b` on `layer`, from their declared voltage difference: IPC-2221B B1
/// inner, B2/B3 (above 3050 m) outer uncoated, B4 outer coated; IEC 60664-1 creepage on outer
/// layers. `None` without an insulation declaration or both voltages, or where the transcribed
/// tables do not apply (IEC inner layers or a case they do not cover).
pub fn insulation_clearance(c: &Constraints, a: &str, b: &str, layer: CopperLayer) -> Option<f64> {
    let ins = c.insulation.as_ref()?;
    let v = (c.net_voltages_v.get(a)? - c.net_voltages_v.get(b)?).abs();
    let outer = matches!(layer, CopperLayer::Top | CopperLayer::Bottom);
    let mm = match ins.standard {
        InsulationStandard::Ipc2221b => {
            let cond = match (outer, ins.coated, ins.altitude_m > 3050.0) {
                (false, _, _) => ClearanceCondition::B1,
                (true, true, _) => ClearanceCondition::B4,
                (true, false, true) => ClearanceCondition::B3,
                (true, false, false) => ClearanceCondition::B2,
            };
            ipc::clearance_mm(v, cond)
        }
        InsulationStandard::Iec60664_1 if outer => {
            ipc::creepage_mm(v, ins.pollution_degree, &ins.material_group).ok()?
        }
        InsulationStandard::Iec60664_1 => return None,
    };
    Some(mm * 1e6)
}

/// The spacing (nm) the autorouter keeps between nets `a` and `b` on `layer`: the DRC-002 rule
/// raised to the declared insulation's spacing, so a route never needs an insulation finding.
pub fn routing_required(c: &Constraints, a: &str, b: &str, layer: CopperLayer) -> f64 {
    let r = required(c, a, b, layer);
    insulation_clearance(c, a, b, layer).map_or(r, |i| r.max(i))
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
    fn routing_spacing_follows_the_declared_insulation() {
        let mut c = constraints();
        c.net_voltages_v.insert("HV".into(), 300.0);
        c.net_voltages_v.insert("GND".into(), 0.0);
        let drc = required(&c, "GND", "HV", CopperLayer::Top);
        assert_eq!(routing_required(&c, "GND", "HV", CopperLayer::Top), drc);
        let ins = |standard, altitude_m, coated| eak_core::Insulation {
            standard,
            pollution_degree: 2,
            material_group: "IIIa".into(),
            altitude_m,
            coated,
        };
        let ipc_at = |cond| ipc::clearance_mm(300.0, cond) * 1e6;
        c.insulation = Some(ins(InsulationStandard::Ipc2221b, 4000.0, false));
        let b3 = routing_required(&c, "GND", "HV", CopperLayer::Top);
        assert_eq!(b3, drc.max(ipc_at(ClearanceCondition::B3)));
        assert!(b3 > drc, "B3 above 3050 m needs more than the B2 DRC rule");
        c.insulation = Some(ins(InsulationStandard::Ipc2221b, 0.0, true));
        let b4 = routing_required(&c, "GND", "HV", CopperLayer::Top);
        assert_eq!(b4, drc.max(ipc_at(ClearanceCondition::B4)));
        c.insulation = Some(ins(InsulationStandard::Iec60664_1, 0.0, false));
        let creep = ipc::creepage_mm(300.0, 2, "IIIa").unwrap() * 1e6;
        assert_eq!(
            routing_required(&c, "GND", "HV", CopperLayer::Top),
            drc.max(creep)
        );
        // IEC inner layers: no creepage path, the DRC rule alone.
        let inner = required(&c, "GND", "HV", CopperLayer::Inner(1));
        assert_eq!(
            routing_required(&c, "GND", "HV", CopperLayer::Inner(1)),
            inner
        );
    }
}
