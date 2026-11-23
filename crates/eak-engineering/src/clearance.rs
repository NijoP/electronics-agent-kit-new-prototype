//! The copper-to-copper clearance rule (DRC-002), shared by the DRC and the interactive router
//! (ADR-0044), so a route the router proposes is never one the DRC then reports.
//!
//! Between copper of nets `a` and `b` the rule is the largest of the board minimum, the two net
//! classes' clearances and, when both nets declare a peak working voltage, the IPC-2221B
//! Table 6-1 spacing for their voltage difference (condition B2 on the outer layers, B1 on inner
//! layers; the table is not verified against the standard text).

use crate::ipc::{self, ClearanceCondition};
use eak_core::{Constraints, CopperLayer};

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
    let k = c
        .min_clearance
        .max(c.class_for(a).clearance)
        .max(c.class_for(b).clearance) as f64;
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
}
