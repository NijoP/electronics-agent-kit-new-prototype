//! Symbol-to-footprint pin/pad mapping check (ADR-0035, M3/M4).
//!
//! A component must not be placed on a PCB with a mapping that leaves a connected symbol pin
//! without a pad, points a pin at a pad the footprint does not have, or maps no pads at all.
//! Findings that are suspicious but legitimate (an unmapped thermal or mechanical pad, several
//! pins sharing one pad such as a ground pad) are warnings, never silently accepted as correct.

use crate::library::{ComponentDef, FootprintDef, PinType, SymbolDef};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MapReport {
    /// Symbol pins (not marked no-connect) with no pad assignment. Blocking.
    pub unmapped_pins: Vec<String>,
    /// (pin, pad) assignments where the footprint has no such pad. Blocking.
    pub missing_pads: Vec<(String, String)>,
    /// Map keys that are not pins of the symbol. Blocking.
    pub unknown_pins: Vec<String>,
    /// Footprint pads no pin maps to (thermal, mechanical or a mistake). Warning.
    pub unmapped_pads: Vec<String>,
    /// Pads that more than one pin maps to, with the pins. Warning.
    pub shared_pads: Vec<(String, Vec<String>)>,
    pub pin_count: usize,
    pub pad_count: usize,
}

impl MapReport {
    /// True when the component must not be placed with this mapping.
    pub fn blocking(&self) -> bool {
        self.pad_count == 0
            || !self.unmapped_pins.is_empty()
            || !self.missing_pads.is_empty()
            || !self.unknown_pins.is_empty()
    }

    /// Human-readable problems and warnings, blocking ones first.
    pub fn messages(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.pad_count == 0 {
            out.push("footprint has no pads".to_string());
        }
        for p in &self.unmapped_pins {
            out.push(format!("symbol pin {p} is not mapped to a pad"));
        }
        for (pin, pad) in &self.missing_pads {
            out.push(format!(
                "pin {pin} maps to pad {pad}, which the footprint does not have"
            ));
        }
        for p in &self.unknown_pins {
            out.push(format!(
                "mapping names pin {p}, which the symbol does not have"
            ));
        }
        for p in &self.unmapped_pads {
            out.push(format!("warning: pad {p} is not used by any pin"));
        }
        for (pad, pins) in &self.shared_pads {
            out.push(format!(
                "warning: pad {pad} is shared by pins {}",
                pins.join(", ")
            ));
        }
        out
    }
}

/// Check an explicit mapping.
pub fn check_map(
    symbol: &SymbolDef,
    footprint: &FootprintDef,
    pin_map: &BTreeMap<String, Vec<String>>,
) -> MapReport {
    let pads: BTreeSet<&str> = footprint.pads.iter().map(|p| p.number.as_str()).collect();
    let pins: BTreeSet<&str> = symbol.pins.iter().map(|p| p.number.as_str()).collect();
    let mut r = MapReport {
        pin_count: pins.len(),
        pad_count: pads.len(),
        ..Default::default()
    };
    let mut used: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for pin in &symbol.pins {
        match pin_map.get(&pin.number) {
            Some(v) if !v.is_empty() => {
                for pad in v {
                    if pads.contains(pad.as_str()) {
                        used.entry(pad.as_str())
                            .or_default()
                            .push(pin.number.clone());
                    } else {
                        r.missing_pads.push((pin.number.clone(), pad.clone()));
                    }
                }
            }
            _ => {
                if pin.electrical_type != PinType::NoConnect {
                    r.unmapped_pins.push(pin.number.clone());
                }
            }
        }
    }
    for key in pin_map.keys() {
        if !pins.contains(key.as_str()) {
            r.unknown_pins.push(key.clone());
        }
    }
    for pad in &footprint.pads {
        match used.get(pad.number.as_str()) {
            None => r.unmapped_pads.push(pad.number.clone()),
            Some(v) if v.len() > 1 => r.shared_pads.push((pad.number.clone(), v.clone())),
            _ => {}
        }
    }
    r
}

/// Check a component definition's own mapping.
pub fn check_component(def: &ComponentDef) -> MapReport {
    check_map(&def.symbol, &def.footprint, &def.pin_map)
}

/// Propose a mapping by identical pin and pad numbers. Only exact matches are proposed; the
/// result is a suggestion for the user to confirm, never applied automatically.
pub fn suggest_by_number(
    symbol: &SymbolDef,
    footprint: &FootprintDef,
) -> BTreeMap<String, Vec<String>> {
    let pads: BTreeSet<&str> = footprint.pads.iter().map(|p| p.number.as_str()).collect();
    symbol
        .pins
        .iter()
        .filter(|p| pads.contains(p.number.as_str()))
        .map(|p| (p.number.clone(), vec![p.number.clone()]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Point;
    use crate::library::{PadDef, PadKind, PadShape, SymbolPin};

    fn pin(n: &str, t: PinType) -> SymbolPin {
        SymbolPin {
            length: 0,
            orientation: Default::default(),
            name_visible: true,
            number_visible: true,
            number: n.into(),
            name: n.into(),
            electrical_type: t,
            position: Point::new(0, 0),
        }
    }

    fn pad(n: &str) -> PadDef {
        PadDef {
            number: n.into(),
            kind: PadKind::Smd,
            shape: PadShape::Rect,
            position: Point::new(0, 0),
            size_x: 1_000_000,
            size_y: 1_000_000,
            drill: None,
        }
    }

    fn sym(pins: Vec<SymbolPin>) -> SymbolDef {
        SymbolDef {
            graphics: Vec::new(),
            pins,
            body_half_w: 1_000_000,
            body_half_h: 1_000_000,
        }
    }

    fn fp(pads: Vec<PadDef>) -> FootprintDef {
        FootprintDef {
            name: "F".into(),
            pads,
            courtyard_half_w: 2_000_000,
            courtyard_half_h: 2_000_000,
            body_height: None,
        }
    }

    fn map(pairs: &[(&str, &[&str])]) -> BTreeMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    #[test]
    fn complete_one_to_one_mapping_is_clean() {
        let s = sym(vec![pin("1", PinType::Passive), pin("2", PinType::Passive)]);
        let f = fp(vec![pad("1"), pad("2")]);
        let m = suggest_by_number(&s, &f);
        let r = check_map(&s, &f, &m);
        assert!(!r.blocking(), "{:?}", r.messages());
        assert!(r.messages().is_empty());
    }

    #[test]
    fn unmapped_pin_missing_pad_and_unknown_pin_block_placement() {
        let s = sym(vec![
            pin("1", PinType::Passive),
            pin("2", PinType::Input),
            pin("3", PinType::NoConnect),
        ]);
        let f = fp(vec![pad("1")]);
        let r = check_map(&s, &f, &map(&[("1", &["1"]), ("9", &["1"])]));
        assert!(r.blocking());
        assert_eq!(
            r.unmapped_pins,
            vec!["2".to_string()],
            "no-connect pin 3 needs no pad"
        );
        assert_eq!(r.unknown_pins, vec!["9".to_string()]);
        let r = check_map(&s, &f, &map(&[("1", &["7"]), ("2", &["1"])]));
        assert_eq!(r.missing_pads, vec![("1".to_string(), "7".to_string())]);
        assert!(r.blocking());
    }

    #[test]
    fn unused_and_shared_pads_warn_but_do_not_block() {
        let s = sym(vec![pin("1", PinType::PowerIn), pin("2", PinType::PowerIn)]);
        let f = fp(vec![pad("EP"), pad("X")]);
        let r = check_map(&s, &f, &map(&[("1", &["EP"]), ("2", &["EP"])]));
        assert!(!r.blocking());
        assert_eq!(r.unmapped_pads, vec!["X".to_string()]);
        assert_eq!(r.shared_pads.len(), 1);
        assert_eq!(
            r.messages()
                .iter()
                .filter(|m| m.starts_with("warning"))
                .count(),
            2
        );
    }

    #[test]
    fn a_footprint_without_pads_always_blocks() {
        let s = sym(vec![pin("1", PinType::NoConnect)]);
        assert!(check_map(&s, &fp(vec![]), &BTreeMap::new()).blocking());
    }

    #[test]
    fn suggestion_only_proposes_exact_number_matches() {
        let s = sym(vec![pin("1", PinType::Passive), pin("A", PinType::Passive)]);
        let m = suggest_by_number(&s, &fp(vec![pad("1"), pad("2")]));
        assert_eq!(m, map(&[("1", &["1"])]));
    }
}
