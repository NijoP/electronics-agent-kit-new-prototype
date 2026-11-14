//! Cross-domain interaction rules (directive §10).
//!
//! Each rule says: when a product contains a `from` participant and a `to` participant, a
//! change in `from` (on one of `triggers`) requires re-evaluating `re_evaluate` in `to`.
//! Participants are engineering-domain ids or `category:<name>` (mechanical and thermal
//! concerns exist on every board, so those categories are always present).
//! The rules name *what to look at again*; they do not compute coupling. Every listed
//! re-evaluation that EAK cannot perform is named in `not_implemented`.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct InteractionRule {
    pub id: &'static str,
    pub from: &'static [&'static str],
    pub to: &'static [&'static str],
    pub mechanism: &'static str,
    /// Quantity-key prefixes whose change triggers the rule (empty = any change in `from`).
    pub triggers: &'static [&'static str],
    pub re_evaluate: &'static [&'static str],
    /// Re-evaluations EAK has no deterministic check for.
    pub not_implemented: &'static [&'static str],
}

/// Always-present participants.
pub const ALWAYS: &[&str] = &["category:mechanical", "category:thermal"];

#[rustfmt::skip]
pub const RULES: &[InteractionRule] = &[
    InteractionRule {
        id: "power_noise_into_rf",
        from: &["power", "motor_drive"],
        to: &["rf"],
        mechanism: "Switching edges and their harmonics couple into RF receive bands through shared ground, radiation from the switch-node loop, and supply ripple.",
        triggers: &["switching_frequency", "output_current", "input_voltage", "rail_current"],
        re_evaluate: &["placement separation of switch node and RF section", "RF ground-reference continuity", "supply filtering on RF rails", "radiated-emission pre-assessment"],
        not_implemented: &["radiated-emission pre-assessment", "conducted-noise estimate"],
    },
    InteractionRule {
        id: "heat_near_precision_analog",
        from: &["power", "battery", "motor_drive", "category:thermal"],
        to: &["analog"],
        mechanism: "Dissipating parts raise local temperature; references, gain resistors and ADC inputs drift with temperature.",
        triggers: &["output_current", "rail_current", "ambient_temperature", "charge_current", "phase_current"],
        re_evaluate: &["placement distance between heat sources and references", "temperature-drift error budget", "copper and thermal relief around the analog section"],
        not_implemented: &["temperature-drift error budget", "board thermal map"],
    },
    InteractionRule {
        id: "motor_transients_into_sensing",
        from: &["motor_drive"],
        to: &["analog", "embedded"],
        mechanism: "Phase switching injects dv/dt and di/dt transients into sense lines and ground returns, corrupting measurements and logic thresholds.",
        triggers: &["phase_current", "bus_voltage", "switching_frequency"],
        re_evaluate: &["current-sense routing (Kelvin, differential)", "ADC sampling relative to PWM edges", "separation of power and signal ground returns"],
        not_implemented: &["transient coupling estimate"],
    },
    InteractionRule {
        id: "connector_placement_limits_high_speed",
        from: &["category:mechanical"],
        to: &["digital_hs"],
        mechanism: "Connector position fixed by the enclosure sets trace length, layer transitions and stub lengths of high-speed links.",
        triggers: &["board_size", "connector_position", "enclosure"],
        re_evaluate: &["high-speed trace length and skew", "via transitions on differential pairs", "impedance continuity at the connector"],
        not_implemented: &["length/skew matching check", "impedance continuity check"],
    },
    InteractionRule {
        id: "battery_heat_in_enclosure",
        from: &["battery"],
        to: &["category:thermal", "category:mechanical"],
        mechanism: "Charging losses heat the cells and the enclosure; cell temperature limits shrink the charge window.",
        triggers: &["charge_current", "ambient_temperature", "enclosure"],
        re_evaluate: &["charger dissipation", "enclosure temperature rise", "cell temperature sensing and charge derating"],
        not_implemented: &["enclosure temperature rise"],
    },
    InteractionRule {
        id: "ground_split_breaks_return_paths",
        from: &["analog", "power", "mains_hv"],
        to: &["rf", "digital_hs"],
        mechanism: "A split or slotted reference plane forces return current around the gap, raising loop inductance, emissions and crosstalk for RF and fast edges.",
        triggers: &["ground_strategy", "plane_split", "isolation"],
        re_evaluate: &["reference-plane continuity under RF and high-speed nets", "stitching across splits", "return-path review"],
        not_implemented: &["return-path continuity check"],
    },
    InteractionRule {
        id: "isolation_boundary_constrains_layout",
        from: &["mains_hv"],
        to: &["category:mechanical", "power", "embedded"],
        mechanism: "An isolation barrier fixes creepage and clearance distances, slots and keepouts, component orientation across the barrier, and which routing may cross it.",
        triggers: &["isolation", "working_voltage", "pollution_degree", "altitude"],
        re_evaluate: &["creepage and clearance across the barrier", "board outline and slots", "routing that crosses the barrier"],
        not_implemented: &["creepage/clearance from a standard's tables (no standard tables are embedded)"],
    },
];

/// True when the product's participant set satisfies the rule.
pub fn active(rule: &InteractionRule, present: &[&str]) -> bool {
    let has = |p: &&str| ALWAYS.contains(p) || present.contains(p);
    rule.from.iter().any(has) && rule.to.iter().any(has)
}

/// True when a change of the quantity key triggers the rule.
pub fn triggered_by(rule: &InteractionRule, quantity: &str) -> bool {
    rule.triggers.is_empty() || rule.triggers.iter().any(|t| quantity.starts_with(t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::domains::domain;

    // Written, not run (code-reviewed only).
    #[test]
    fn participants_are_known_and_ids_unique() {
        let mut ids = std::collections::BTreeSet::new();
        for r in RULES {
            assert!(ids.insert(r.id));
            for p in r.from.iter().chain(r.to) {
                assert!(
                    p.starts_with("category:") || domain(p).is_some(),
                    "{} names {p}",
                    r.id
                );
            }
        }
    }

    #[test]
    fn power_and_rf_interact() {
        let r = RULES
            .iter()
            .find(|r| r.id == "power_noise_into_rf")
            .unwrap();
        assert!(active(r, &["power", "rf"]));
        assert!(!active(r, &["power"]));
        assert!(triggered_by(r, "switching_frequency"));
        assert!(!triggered_by(r, "ripple:VOUT"));
    }
}
