//! Pin-to-pad compatibility verification (PP-001..PP-005, ADR-0052 addendum).
//!
//! Checks one component definition's symbol pins against its footprint pads, by number and by
//! electrical function: every pin lands on real pads, no pad joins two different pins, a large
//! exposed or thermal pad is driven by a power, ground or passive pin (never a logic signal),
//! no-connect pins stay isolated, mechanical pads carry no signal and multi-pad pins are of a
//! kind that may be split over pads. Each rule has one explicit state; a rule that could not be
//! decided is `UNKNOWN` or `REQUIRES_HUMAN_APPROVAL`, never `PASS`. Nothing here mutates a
//! design or a library.

use crate::footprint::mechanical;
use crate::state::{CheckState, RuleResult};
use eak_core::{ComponentDef, PadDef, PinType, SymbolPin};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Rules evaluated (PP-006 is listed in `not_implemented` instead).
pub const RULES: [&str; 5] = ["PP-001", "PP-002", "PP-003", "PP-004", "PP-005"];

/// A pad whose copper area is at least this many times the median pad is treated as an
/// exposed (thermal) pad.
pub const EXPOSED_AREA_RATIO: f64 = 3.0;

/// Result of checking one definition's pin map. Same shape as
/// [`crate::footprint::FootprintReport`]: about a library definition, not a project revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinPadReport {
    pub check: String,
    pub definition_id: String,
    pub footprint: String,
    /// Pads this check treated as exposed / thermal pads.
    pub exposed_pads: Vec<String>,
    pub rule_results: Vec<RuleResult>,
    pub overall: CheckState,
    pub not_implemented: Vec<String>,
    pub limitations: Vec<String>,
}

impl PinPadReport {
    /// State of one rule; a rule that is not in the report is `UNKNOWN`.
    pub fn state_of(&self, rule_id: &str) -> CheckState {
        self.rule_results
            .iter()
            .find(|r| r.rule_id == rule_id)
            .map(|r| r.state)
            .unwrap_or(CheckState::Unknown)
    }

    /// Rules PP-001..PP-005 that are missing or do not certify.
    pub fn blocking(&self) -> Vec<String> {
        RULES
            .iter()
            .filter(|id| !self.state_of(id).certifies())
            .map(
                |id| match self.rule_results.iter().find(|r| r.rule_id == *id) {
                    Some(r) => format!("{id} is {:?}: {}", r.state, r.reason),
                    None => format!("{id} is missing from the report"),
                },
            )
            .collect()
    }

    /// True only when every rule PP-001..PP-005 is present and certifies.
    pub fn verifies(&self) -> bool {
        self.blocking().is_empty()
    }
}

fn result(rule: &str, state: CheckState, reason: impl Into<String>) -> RuleResult {
    RuleResult {
        rule_id: rule.into(),
        state,
        reason: reason.into(),
        severity_override: None,
        default_state: None,
    }
}

fn verdict(rule: &str, problems: Vec<String>, review: Vec<String>) -> RuleResult {
    if !problems.is_empty() {
        result(rule, CheckState::Fail, problems.join("; "))
    } else if !review.is_empty() {
        result(rule, CheckState::RequiresHumanApproval, review.join("; "))
    } else {
        result(rule, CheckState::Pass, "")
    }
}

/// A logic signal pin: never on a thermal pad or a mechanical pad, and on several pads only
/// when a person confirms it against the datasheet.
fn signal(t: PinType) -> bool {
    matches!(
        t,
        PinType::Input | PinType::Output | PinType::Bidirectional | PinType::OpenCollector
    )
}

fn area(p: &PadDef) -> f64 {
    p.size_x as f64 * p.size_y as f64
}

/// Numbers of pads whose area is at least [`EXPOSED_AREA_RATIO`] times the median numbered pad.
/// Needs at least three numbered pads; fewer have no meaningful median.
pub fn exposed_pads(def: &ComponentDef) -> Vec<String> {
    let pads: Vec<&PadDef> = def
        .footprint
        .pads
        .iter()
        .filter(|p| !mechanical(&p.number))
        .collect();
    if pads.len() < 3 {
        return vec![];
    }
    let mut areas: Vec<f64> = pads.iter().map(|&p| area(p)).collect();
    areas.sort_by(f64::total_cmp);
    let median = areas[areas.len() / 2];
    if median <= 0.0 {
        return vec![];
    }
    pads.iter()
        .filter(|&&p| area(p) >= EXPOSED_AREA_RATIO * median)
        .map(|p| p.number.clone())
        .collect()
}

/// pad number -> pins that map onto it.
fn pins_by_pad(def: &ComponentDef) -> BTreeMap<&str, Vec<&SymbolPin>> {
    let mut out: BTreeMap<&str, Vec<&SymbolPin>> = BTreeMap::new();
    for pin in &def.symbol.pins {
        for pad in def.pin_map.get(&pin.number).into_iter().flatten() {
            out.entry(pad.as_str()).or_default().push(pin);
        }
    }
    out
}

/// PP-001: by number. Every pin-map key is a symbol pin, every non-NC pin reaches at least one
/// existing pad, and no pad joins two pins of different names (same-named pins, such as two
/// GND pins sharing a pad, are one net).
fn pp001(def: &ComponentDef, by_pad: &BTreeMap<&str, Vec<&SymbolPin>>) -> RuleResult {
    let pads: BTreeSet<&str> = def
        .footprint
        .pads
        .iter()
        .map(|p| p.number.as_str())
        .collect();
    let pins: BTreeSet<&str> = def.symbol.pins.iter().map(|p| p.number.as_str()).collect();
    let mut problems = Vec::new();
    for key in def.pin_map.keys() {
        if !pins.contains(key.as_str()) {
            problems.push(format!("pin map names pin {key}, which the symbol lacks"));
        }
    }
    for pin in &def.symbol.pins {
        let mapped = def.pin_map.get(&pin.number).cloned().unwrap_or_default();
        if mapped.is_empty() && pin.electrical_type != PinType::NoConnect {
            problems.push(format!("pin {} ({}) maps to no pad", pin.number, pin.name));
        }
        for pad in mapped {
            if !pads.contains(pad.as_str()) {
                problems.push(format!("pin {} maps to missing pad {pad}", pin.number));
            }
        }
    }
    for (pad, on) in by_pad {
        let names: BTreeSet<&str> = on.iter().map(|p| p.name.as_str()).collect();
        if names.len() > 1 {
            let list: Vec<String> = on
                .iter()
                .map(|p| format!("{} ({})", p.number, p.name))
                .collect();
            problems.push(format!("pad {pad} joins pins {}", list.join(", ")));
        }
    }
    verdict("PP-001", problems, vec![])
}

/// PP-002: exposed / thermal pads are driven by power, ground or passive pins.
fn pp002(exposed: &[String], by_pad: &BTreeMap<&str, Vec<&SymbolPin>>) -> RuleResult {
    if exposed.is_empty() {
        return result("PP-002", CheckState::NotApplicable, "no exposed pad");
    }
    let mut problems = Vec::new();
    let mut review = Vec::new();
    for pad in exposed {
        let Some(on) = by_pad.get(pad.as_str()) else {
            review.push(format!(
                "exposed pad {pad} is not mapped: whether it may float is a datasheet fact"
            ));
            continue;
        };
        for pin in on {
            if signal(pin.electrical_type) {
                problems.push(format!(
                    "exposed pad {pad} carries signal pin {} ({}, {:?})",
                    pin.number, pin.name, pin.electrical_type
                ));
            } else if pin.electrical_type == PinType::NoConnect {
                review.push(format!(
                    "exposed pad {pad} is mapped to no-connect pin {}",
                    pin.number
                ));
            }
        }
    }
    verdict("PP-002", problems, review)
}

/// PP-003: a pin split over several pads is power, ground or passive; a signal pin on several
/// pads needs a person to confirm it against the datasheet. A no-connect pin never has several.
fn pp003(def: &ComponentDef) -> RuleResult {
    let mut any = false;
    let mut problems = Vec::new();
    let mut review = Vec::new();
    for pin in &def.symbol.pins {
        let n = def.pin_map.get(&pin.number).map_or(0, Vec::len);
        if n < 2 {
            continue;
        }
        any = true;
        if pin.electrical_type == PinType::NoConnect {
            problems.push(format!("no-connect pin {} maps to {n} pads", pin.number));
        } else if signal(pin.electrical_type) {
            let (num, name) = (&pin.number, &pin.name);
            review.push(format!("signal pin {num} ({name}) maps to {n} pads"));
        }
    }
    if !any {
        return result("PP-003", CheckState::NotApplicable, "no pin with several pads");
    }
    verdict("PP-003", problems, review)
}

/// PP-004: a no-connect pin's pads belong to it alone.
fn pp004(def: &ComponentDef, by_pad: &BTreeMap<&str, Vec<&SymbolPin>>) -> RuleResult {
    let mut any = false;
    let mut problems = Vec::new();
    for pin in &def.symbol.pins {
        if pin.electrical_type != PinType::NoConnect {
            continue;
        }
        any = true;
        for pad in def.pin_map.get(&pin.number).into_iter().flatten() {
            let others: Vec<&str> = by_pad
                .get(pad.as_str())
                .into_iter()
                .flatten()
                .filter(|p| p.number != pin.number)
                .map(|p| p.number.as_str())
                .collect();
            if !others.is_empty() {
                problems.push(format!(
                    "no-connect pin {} shares pad {pad} with pin {}",
                    pin.number,
                    others.join(", ")
                ));
            }
        }
    }
    if !any {
        return result("PP-004", CheckState::NotApplicable, "no no-connect pin");
    }
    verdict("PP-004", problems, vec![])
}

/// PP-005: mechanical pads (empty or MP* numbers) carry no logic signal.
fn pp005(def: &ComponentDef, by_pad: &BTreeMap<&str, Vec<&SymbolPin>>) -> RuleResult {
    let mut any = false;
    let mut problems = Vec::new();
    for pad in &def.footprint.pads {
        if !mechanical(&pad.number) {
            continue;
        }
        any = true;
        for pin in by_pad.get(pad.number.as_str()).into_iter().flatten() {
            if signal(pin.electrical_type) {
                problems.push(format!(
                    "mechanical pad {} carries signal pin {} ({})",
                    pad.number, pin.number, pin.name
                ));
            }
        }
    }
    if !any {
        return result("PP-005", CheckState::NotApplicable, "no mechanical pad");
    }
    verdict("PP-005", problems, vec![])
}

/// Run PP-001..PP-005 on one definition. PP-006 is reported as not implemented.
pub fn verify(def: &ComponentDef) -> PinPadReport {
    let by_pad = pins_by_pad(def);
    let exposed = exposed_pads(def);
    let rule_results = vec![
        pp001(def, &by_pad),
        pp002(&exposed, &by_pad),
        pp003(def),
        pp004(def, &by_pad),
        pp005(def, &by_pad),
    ];
    let overall = rule_results
        .iter()
        .fold(CheckState::NotApplicable, |acc, r| acc.worst(r.state));
    PinPadReport {
        check: "pin_pad".into(),
        definition_id: def.id.clone(),
        footprint: def.footprint.name.clone(),
        exposed_pads: exposed,
        rule_results,
        overall,
        not_implemented: vec![
            "PP-006 pin names and functions against the datasheet pinout table".into(),
        ],
        limitations: vec![
            "an exposed pad is one of at least 3x the median pad area".into(),
            "pin function comes from the symbol's electrical type; a wrong type is not caught"
                .into(),
            "pads numbered empty or MP* are treated as mechanical".into(),
            "pins of the same name sharing a pad are treated as one net".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(id: &str) -> ComponentDef {
        eak_core::builtin::components()
            .into_iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("builtin {id}"))
    }

    fn first_signal_pin(def: &ComponentDef) -> String {
        def.symbol
            .pins
            .iter()
            .find(|p| signal(p.electrical_type))
            .expect("a signal pin")
            .number
            .clone()
    }

    #[test]
    fn no_builtin_part_fails() {
        for def in eak_core::builtin::components() {
            let r = verify(&def);
            assert_ne!(r.overall, CheckState::Fail, "{}: {:?}", def.id, r.blocking());
            assert!(r.not_implemented.iter().any(|n| n.starts_with("PP-006")));
        }
    }

    #[test]
    fn correct_thermal_pad_map_passes() {
        let r = verify(&part("LMR33630_HSOIC-8"));
        assert_eq!(r.exposed_pads, ["9"]);
        assert_eq!(r.state_of("PP-002"), CheckState::Pass);
        assert!(r.verifies(), "{:?}", r.blocking());
    }

    #[test]
    fn multi_pad_ground_with_exposed_array_passes() {
        let r = verify(&part("ESP32-C3-MINI-1"));
        assert_eq!(r.exposed_pads.len(), 9, "{:?}", r.exposed_pads);
        assert_eq!(r.state_of("PP-002"), CheckState::Pass);
        assert_eq!(r.state_of("PP-003"), CheckState::Pass);
        assert_eq!(r.state_of("PP-004"), CheckState::Pass);
        assert!(r.verifies(), "{:?}", r.blocking());
    }

    #[test]
    fn signal_pin_on_thermal_pad_fails() {
        let mut def = part("LMR33630_HSOIC-8");
        // FB onto the thermal pad, AGND onto FB's pad: a swapped map.
        def.pin_map.insert("5".into(), vec!["9".into()]);
        def.pin_map.insert("9".into(), vec!["5".into()]);
        let r = verify(&def);
        assert_eq!(r.state_of("PP-002"), CheckState::Fail);
        assert_eq!(r.overall, CheckState::Fail);
        assert!(!r.verifies());
    }

    #[test]
    fn nc_pin_shorted_to_supply_fails() {
        let mut def = part("ESP32-C3-MINI-1");
        def.pin_map.insert("4".into(), vec!["3".into()]);
        let r = verify(&def);
        assert_eq!(r.state_of("PP-001"), CheckState::Fail);
        assert_eq!(r.state_of("PP-004"), CheckState::Fail);
    }

    #[test]
    fn missing_pad_and_unknown_pin_fail_by_number() {
        let mut def = part("LDO_SOT-23-5");
        def.pin_map.insert("1".into(), vec!["7".into()]);
        def.pin_map.insert("99".into(), vec!["2".into()]);
        let r = verify(&def);
        assert_eq!(r.state_of("PP-001"), CheckState::Fail);
    }

    #[test]
    fn unmapped_exposed_pad_and_split_signal_are_never_pass() {
        let mut def = part("LMR33630_HSOIC-8");
        def.pin_map.insert("9".into(), vec![]);
        let r = verify(&def);
        assert_eq!(r.state_of("PP-002"), CheckState::RequiresHumanApproval);
        assert!(!r.verifies());

        let mut def = part("LDO_SOT-23-5");
        let sig = first_signal_pin(&def);
        def.pin_map.get_mut(&sig).unwrap().push("2".into());
        let r = verify(&def);
        assert_eq!(r.state_of("PP-003"), CheckState::RequiresHumanApproval);
        assert!(!r.verifies());
    }

    #[test]
    fn signal_on_mechanical_pad_fails() {
        let mut def = part("LDO_SOT-23-5");
        let mut mp = def.footprint.pads[0].clone();
        mp.number = "MP1".into();
        def.footprint.pads.push(mp);
        let sig = first_signal_pin(&def);
        def.pin_map.get_mut(&sig).unwrap().push("MP1".into());
        let r = verify(&def);
        assert_eq!(r.state_of("PP-005"), CheckState::Fail);
    }
}
