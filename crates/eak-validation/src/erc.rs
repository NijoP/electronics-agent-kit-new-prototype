//! Electrical rules check over the canonical schematic connectivity.

use crate::report::{finding, rule, Bound, Finding, Report, Severity::*};
use eak_core::{PinType, Project};
use eak_engineering::{calc::parse_value, netlist};
use std::collections::{BTreeMap, BTreeSet};

pub fn is_ground_net(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    let n = n.trim_start_matches('/');
    matches!(
        n,
        "GND" | "AGND" | "DGND" | "PGND" | "SGND" | "VSS" | "0V" | "GNDA" | "GNDD"
    ) || n.starts_with("GND_")
        || n.ends_with("_GND")
}

fn is_ground_pin(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    let base = n.trim_end_matches(|c: char| c.is_ascii_digit());
    matches!(
        base,
        "GND" | "AGND" | "DGND" | "PGND" | "VSS" | "AVSS" | "DVSS"
    )
}

fn is_supply_pin(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    [
        "VCC", "VDD", "AVDD", "DVDD", "VDDIO", "VDDA", "VIN", "PVIN", "AVIN", "VBUS", "VBAT",
        "VCCIO",
    ]
    .iter()
    .any(|p| {
        n == *p
            || (n.starts_with(p) && n[p.len()..].chars().all(|c| c.is_ascii_digit() || c == '_'))
    })
}

pub fn run(p: &Project) -> Report {
    let nl = netlist::build(p);
    let mut f: Vec<Finding> = Vec::new();
    let nc_pins: BTreeSet<(String, String)> = p
        .schematic
        .no_connects
        .values()
        .map(|n| (n.symbol_id.clone(), n.pin.clone()))
        .collect();
    // Schematic position of a symbol pin, for cross-probing.
    let pin_at = |symbol_id: &str, pin: &str| {
        p.schematic
            .symbols
            .get(symbol_id)
            .and_then(|s| p.pin_position(s, pin))
    };
    let label_at = |ids: &[String]| {
        ids.iter()
            .find_map(|id| p.schematic.labels.get(id).map(|l| l.position))
    };

    // Capacitors from each net to ground (for decoupling checks).
    let mut decoupled: BTreeSet<String> = BTreeSet::new();
    for s in p.schematic.symbols.values() {
        let Some(def) = p.library.get(&s.component_id) else {
            continue;
        };
        if def.category != "Capacitor" || def.symbol.pins.len() != 2 {
            continue;
        }
        let a = nl
            .pin_net
            .get(&(s.id.clone(), def.symbol.pins[0].number.clone()));
        let b = nl
            .pin_net
            .get(&(s.id.clone(), def.symbol.pins[1].number.clone()));
        if let (Some(a), Some(b)) = (a, b) {
            if is_ground_net(b) {
                decoupled.insert(a.clone());
            }
            if is_ground_net(a) {
                decoupled.insert(b.clone());
            }
        }
    }

    for net in nl.nets.values() {
        let connected = net.pins.len() + net.label_ids.len() > 1;
        let net_key = format!("net:{}", net.name);
        for pin in &net.pins {
            let marked_nc = nc_pins.contains(&(pin.symbol_id.clone(), pin.pin.clone()));
            let at = pin_at(&pin.symbol_id, &pin.pin);
            let pin_key = format!("pin:{}", pin.pin);
            if !connected && pin.electrical_type != PinType::NoConnect && !marked_nc {
                f.push(
                    finding(
                        "ERC-001",
                        Error,
                        format!(
                            "{} pin {} ({}) is unconnected",
                            pin.refdes, pin.pin, pin.pin_name
                        ),
                        vec![pin.symbol_id.clone()],
                        at,
                    )
                    .key(&pin_key)
                    .fix(format!(
                        "Wire {} pin {} ({}) to the net it belongs on, or place a no-connect marker on it if it is intentionally unused (check the datasheet for pins that must not float).",
                        pin.refdes, pin.pin, pin.pin_name
                    )),
                );
            }
            if connected && marked_nc {
                f.push(
                    finding(
                        "ERC-002",
                        Warning,
                        format!(
                            "{} pin {} is marked no-connect but is connected to {}",
                            pin.refdes, pin.pin, net.name
                        ),
                        vec![pin.symbol_id.clone()],
                        at,
                    )
                    .key(&format!("{pin_key}:marker"))
                    .fix(format!(
                        "Either remove the no-connect marker from {} pin {} or disconnect the pin from {}, whichever matches the intended circuit.",
                        pin.refdes, pin.pin, net.name
                    )),
                );
            }
            if connected && pin.electrical_type == PinType::NoConnect {
                f.push(
                    finding(
                        "ERC-002",
                        Warning,
                        format!(
                            "{} pin {} is a no-connect pin but is wired to {}",
                            pin.refdes, pin.pin, net.name
                        ),
                        vec![pin.symbol_id.clone()],
                        at,
                    )
                    .key(&format!("{pin_key}:nc-pin"))
                    .fix(format!(
                        "Disconnect {} pin {} from {}: the part defines it as no-connect. Check the datasheet before using the pin.",
                        pin.refdes, pin.pin, net.name
                    )),
                );
            }
            if is_ground_pin(&pin.pin_name) && connected && !is_ground_net(&net.name) {
                f.push(
                    finding(
                        "ERC-012",
                        Error,
                        format!(
                            "{} ground pin {} is on non-ground net {}",
                            pin.refdes, pin.pin_name, net.name
                        ),
                        vec![pin.symbol_id.clone()],
                        at,
                    )
                    .key(&pin_key)
                    .fix(format!(
                        "Connect {} pin {} ({}) to the ground net instead of {}, or rename the net if {} is a ground.",
                        pin.refdes, pin.pin, pin.pin_name, net.name, net.name
                    )),
                );
            }
            if is_supply_pin(&pin.pin_name) && is_ground_net(&net.name) {
                f.push(
                    finding(
                        "ERC-012",
                        Error,
                        format!(
                            "{} supply pin {} is tied to ground net {}",
                            pin.refdes, pin.pin_name, net.name
                        ),
                        vec![pin.symbol_id.clone()],
                        at,
                    )
                    .key(&pin_key)
                    .fix(format!(
                        "Connect {} pin {} ({}) to its supply rail instead of {}.",
                        pin.refdes, pin.pin, pin.pin_name, net.name
                    )),
                );
            }
        }
        let drivers: Vec<_> = net
            .pins
            .iter()
            .filter(|p| p.electrical_type == PinType::PowerOut)
            .collect();
        let outputs: Vec<_> = net
            .pins
            .iter()
            .filter(|p| matches!(p.electrical_type, PinType::Output | PinType::PowerOut))
            .collect();
        let power_in: Vec<_> = net
            .pins
            .iter()
            .filter(|p| p.electrical_type == PinType::PowerIn)
            .collect();
        if !power_in.is_empty() && drivers.is_empty() && !net.has_power_port && connected {
            f.push(
                finding(
                    "ERC-003",
                    Error,
                    format!("net {} feeds power input pin(s) {} but nothing drives it (no power output pin or power port)", net.name, power_in.iter().map(|p| format!("{}.{}", p.refdes, p.pin_name)).collect::<Vec<_>>().join(", ")),
                    power_in.iter().map(|p| p.symbol_id.clone()).collect(),
                    pin_at(&power_in[0].symbol_id, &power_in[0].pin),
                )
                .key(&net_key)
                .fix(format!(
                    "Connect {} to a source (a regulator or connector power output pin), or add a power port for {} if the rail is supplied from outside this schematic.",
                    net.name, net.name
                )),
            );
        }
        if outputs.len() > 1 {
            f.push(
                finding(
                    "ERC-004",
                    Error,
                    format!(
                        "net {} has {} driving outputs ({})",
                        net.name,
                        outputs.len(),
                        outputs
                            .iter()
                            .map(|p| format!("{}.{}", p.refdes, p.pin_name))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    outputs.iter().map(|p| p.symbol_id.clone()).collect(),
                    pin_at(&outputs[0].symbol_id, &outputs[0].pin),
                )
                .key(&net_key)
                .fix(format!(
                    "Leave one output driving {}: split the net, or, if the outputs are meant to share it, use parts or circuitry made for that (open-drain/tri-state outputs, diode-OR or ideal-diode power sharing).",
                    net.name
                )),
            );
        }
        if net.label_names.len() > 1 {
            f.push(
                finding(
                    "ERC-005",
                    Warning,
                    format!(
                        "one net carries several names: {}",
                        net.label_names.join(", ")
                    ),
                    net.label_ids.clone(),
                    label_at(&net.label_ids),
                )
                .key(&net_key)
                .fix(format!(
                    "Keep one name for this net: rename or remove the extra labels ({}), or separate the wires if they were not meant to be joined.",
                    net.label_names.join(", ")
                )),
            );
        }
        if net.pins.is_empty() {
            f.push(
                finding(
                    "ERC-009",
                    Warning,
                    format!("label {} is not attached to any pin", net.name),
                    net.label_ids.clone(),
                    label_at(&net.label_ids),
                )
                .key(&net_key)
                .fix(format!(
                    "Attach label {} to a wire that reaches a pin, or delete it if it is unused.",
                    net.name
                )),
            );
        }
        let supply_pins: Vec<_> = net
            .pins
            .iter()
            .filter(|p| p.electrical_type == PinType::PowerIn && is_supply_pin(&p.pin_name))
            .collect();
        if !is_ground_net(&net.name) && !supply_pins.is_empty() && !decoupled.contains(&net.name) {
            f.push(
                finding(
                    "ERC-010",
                    Warning,
                    format!("supply net {} has no capacitor to ground", net.name),
                    net.pins.iter().map(|p| p.symbol_id.clone()).collect(),
                    pin_at(&supply_pins[0].symbol_id, &supply_pins[0].pin),
                )
                .key(&net_key)
                .fix(format!(
                    "Add a decoupling capacitor from {} to ground, placed close to the supply pins; take its value and voltage rating from the IC datasheet.",
                    net.name
                )),
            );
        }
    }

    // Reference designators.
    let mut refs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for s in p.schematic.symbols.values() {
        refs.entry(s.refdes.as_str())
            .or_default()
            .push(s.id.as_str());
        if s.refdes.ends_with('?') || s.refdes.is_empty() {
            f.push(
                finding(
                    "ERC-007",
                    Error,
                    format!("symbol {} is not annotated", s.refdes),
                    vec![s.id.clone()],
                    Some(s.position),
                )
                .fix("Assign a unique reference designator to the symbol (run annotation)."),
            );
        }
    }
    for (r, ids) in refs {
        if ids.len() > 1 {
            f.push(
                finding(
                    "ERC-006",
                    Error,
                    format!("reference designator {r} is used {} times", ids.len()),
                    ids.iter().map(|s| s.to_string()).collect(),
                    p.schematic.symbols.get(ids[0]).map(|s| s.position),
                )
                .fix(format!(
                    "Re-annotate so each of the {} symbols named {r} has its own reference designator.",
                    ids.len()
                )),
            );
        }
    }
    for (wire, pt) in &nl.dangling_wire_ends {
        f.push(
            finding(
                "ERC-008",
                Warning,
                "wire end is not connected to anything",
                vec![wire.clone()],
                Some(*pt),
            )
            .key(&format!("end:{},{}", pt.x, pt.y))
            .fix("Extend the wire end to a pin, wire or label, or delete the unused wire segment."),
        );
    }

    // Capacitor voltage rating vs declared rail voltage (DESK-derived rule).
    for s in p.schematic.symbols.values() {
        let Some(def) = p.library.get(&s.component_id) else {
            continue;
        };
        if def.category != "Capacitor" {
            continue;
        }
        let Some(rating) = def.ratings.iter().find(|r| r.name == "voltage") else {
            continue;
        };
        let vmax = def
            .symbol
            .pins
            .iter()
            .filter_map(|pin| nl.pin_net.get(&(s.id.clone(), pin.number.clone())))
            .filter_map(|n| p.constraints.net_voltages_v.get(n))
            .fold(0.0f64, |a, &b| a.max(b.abs()));
        if vmax <= 0.0 {
            continue;
        }
        let tag = if rating.verified {
            ""
        } else {
            " (rating unverified)"
        };
        // 1.5x rail, rounded to 1 mV so values such as 1.5 x 3.3 V print cleanly.
        let v_margin = (1.5 * vmax * 1000.0).round() / 1000.0;
        let fix = format!(
            "Use a capacitor rated at least {v_margin} V (1.5x the {vmax} V rail, the margin this rule checks) and confirm the DC-bias derating in its datasheet."
        );
        if rating.value < vmax {
            f.push(
                finding(
                    "ERC-011",
                    Error,
                    format!(
                        "{} rated {} V on a {} V rail{tag}",
                        s.refdes, rating.value, vmax
                    ),
                    vec![s.id.clone()],
                    Some(s.position),
                )
                .measured(rating.value, "V")
                .required(Bound::Min, vmax, "V")
                .fix(fix),
            );
        } else if rating.value < v_margin {
            f.push(
                finding(
                    "ERC-011",
                    Warning,
                    format!(
                        "{} rated {} V on {} V (< 1.5x margin; check DC-bias derating){tag}",
                        s.refdes, rating.value, vmax
                    ),
                    vec![s.id.clone()],
                    Some(s.position),
                )
                .measured(rating.value, "V")
                .required(Bound::Min, v_margin, "V")
                .fix(fix),
            );
        }
    }
    // Resistor values must be numeric.
    for s in p.schematic.symbols.values() {
        if let Some(def) = p.library.get(&s.component_id) {
            if def.category == "Resistor" && parse_value(&s.value).is_none() {
                f.push(
                    finding(
                        "ERC-013",
                        Warning,
                        format!(
                            "{} value '{}' is not a numeric resistance",
                            s.refdes, s.value
                        ),
                        vec![s.id.clone()],
                        Some(s.position),
                    )
                    .fix(format!(
                        "Enter the resistance of {} as a number with an optional SI prefix, for example 10k, 4k7 or 100R.",
                        s.refdes
                    )),
                );
            }
        }
    }

    let rules = vec![
        rule(
            "ERC-001",
            "Unconnected pin",
            "single-pin nets without a no-connect marker",
            "EAK",
        ),
        rule("ERC-002", "No-connect conflict", "", "EAK"),
        rule(
            "ERC-003",
            "Undriven power input",
            "PowerIn pins need a PowerOut pin or power port on the net",
            "EAK",
        ),
        rule(
            "ERC-004",
            "Output conflict",
            "more than one Output/PowerOut on a net",
            "EAK",
        ),
        rule("ERC-005", "Multiple net names", "", "EAK"),
        rule("ERC-006", "Duplicate reference designator", "", "EAK"),
        rule("ERC-007", "Unannotated symbol", "", "EAK"),
        rule("ERC-008", "Dangling wire", "", "EAK"),
        rule("ERC-009", "Label without pins", "", "EAK"),
        rule(
            "ERC-010",
            "Missing decoupling capacitor",
            "supply pin nets need a capacitor to a ground net",
            "DESK erc.py (reimplemented)",
        ),
        rule(
            "ERC-011",
            "Capacitor voltage rating",
            "rating >= rail (error), >= 1.5x rail (warning); uses declared net voltages",
            "DESK erc.py (reimplemented)",
        ),
        rule(
            "ERC-012",
            "Ground/supply pin mis-connection",
            "pin-name patterns",
            "DESK erc.py (reimplemented)",
        ),
        rule(
            "ERC-013",
            "Non-numeric resistor value",
            "",
            "DESK erc.py (reimplemented)",
        ),
    ];
    let not_implemented = vec![
        "Pin-to-pin compatibility matrix beyond driver conflicts".into(),
        "Hierarchical sheet / bus consistency (hierarchy not yet supported)".into(),
        "Analog simulation, operating-point and tolerance analysis".into(),
        "Component stress/derating beyond capacitor voltage".into(),
    ];
    let limitations = vec![
        "Pin-name heuristics for supply/ground pins can miss unusual naming".into(),
        "Ratings in the built-in library are unverified unless marked verified".into(),
    ];
    Report::new("ERC", p, rules, f, not_implemented, limitations)
}
