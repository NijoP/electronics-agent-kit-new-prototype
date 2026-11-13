//! Electrical rules check over the canonical schematic connectivity.

use crate::report::{finding, rule, Finding, Report, Severity, Severity::*};
use eak_core::{ComponentDef, PinType, Project, SymbolInstance, SymbolPin};
use eak_engineering::{calc::parse_value, netlist};
use std::collections::{BTreeMap, BTreeSet};

/// Largest |declared value| over `nets`; `None` when none of them is declared.
fn declared_max(nets: &[String], declared: &BTreeMap<String, f64>) -> Option<f64> {
    nets.iter()
        .filter_map(|n| declared.get(n))
        .map(|v| v.abs())
        .reduce(f64::max)
}

/// Shared component rating check (ERC-011, ERC-014..016).
///
/// `options` are (rating name, warning margin, margin note); the first rating the part has
/// is used. Rating below the stress is an error, below `margin` × stress a warning. A part
/// without the rating, a rating in another unit or a missing declared stress is reported
/// as an Info "not checked" finding, and a pass on an unverified rating is reported as
/// such: neither is ever silently treated as passing.
#[allow(clippy::too_many_arguments)]
fn check_rating(
    f: &mut Vec<Finding>,
    rule_id: &str,
    s: &SymbolInstance,
    def: &ComponentDef,
    options: &[(&str, f64, &str)],
    stress: Option<f64>,
    quantity: &str,
    unit: &str,
) {
    let (severity, message) = match rating_verdict(s, def, options, stress, quantity, unit) {
        Some(v) => v,
        None => return,
    };
    f.push(finding(
        rule_id,
        severity,
        message,
        vec![s.id.clone()],
        None,
    ));
}

/// Severity and message for [`check_rating`]; `None` only for a pass on a verified rating.
fn rating_verdict(
    s: &SymbolInstance,
    def: &ComponentDef,
    options: &[(&str, f64, &str)],
    stress: Option<f64>,
    quantity: &str,
    unit: &str,
) -> Option<(Severity, String)> {
    let Some((rating, margin, note)) = options.iter().find_map(|(name, margin, note)| {
        def.ratings
            .iter()
            .find(|r| r.name == *name)
            .map(|r| (r, *margin, *note))
    }) else {
        let names: Vec<&str> = options.iter().map(|o| o.0).collect();
        return Some((
            Info,
            format!(
                "{} not checked: library part {} has no {} rating",
                s.refdes,
                def.id,
                names.join(" or ")
            ),
        ));
    };
    if rating.unit != unit {
        return Some((
            Info,
            format!(
                "{} {} not checked: rating unit '{}' is not {unit}",
                s.refdes, rating.name, rating.unit
            ),
        ));
    }
    let Some(stress) = stress else {
        return Some((
            Info,
            format!(
                "{} {} not checked: no declared {quantity} on its nets",
                s.refdes, rating.name
            ),
        ));
    };
    let tag = if rating.verified {
        ""
    } else {
        " (rating unverified)"
    };
    let what = format!(
        "{} {} {} {unit} vs {stress} {unit} {quantity}",
        s.refdes, rating.name, rating.value
    );
    if rating.value < stress {
        Some((Error, format!("{what}: rating exceeded{tag}")))
    } else if rating.value < margin * stress {
        Some((Warning, format!("{what}: < {margin}x margin, {note}{tag}")))
    } else if !rating.verified {
        Some((
            Info,
            format!(
                "{what}: within rating, but the rating is unverified ({})",
                rating.source
            ),
        ))
    } else {
        None
    }
}

fn is_drain_source_pin(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "D" | "DRAIN" | "S" | "SOURCE"
    )
}

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
        for pin in &net.pins {
            let marked_nc = nc_pins.contains(&(pin.symbol_id.clone(), pin.pin.clone()));
            if !connected && pin.electrical_type != PinType::NoConnect && !marked_nc {
                f.push(finding(
                    "ERC-001",
                    Error,
                    format!(
                        "{} pin {} ({}) is unconnected",
                        pin.refdes, pin.pin, pin.pin_name
                    ),
                    vec![pin.symbol_id.clone()],
                    None,
                ));
            }
            if connected && marked_nc {
                f.push(finding(
                    "ERC-002",
                    Warning,
                    format!(
                        "{} pin {} is marked no-connect but is connected to {}",
                        pin.refdes, pin.pin, net.name
                    ),
                    vec![pin.symbol_id.clone()],
                    None,
                ));
            }
            if connected && pin.electrical_type == PinType::NoConnect {
                f.push(finding(
                    "ERC-002",
                    Warning,
                    format!(
                        "{} pin {} is a no-connect pin but is wired to {}",
                        pin.refdes, pin.pin, net.name
                    ),
                    vec![pin.symbol_id.clone()],
                    None,
                ));
            }
            if is_ground_pin(&pin.pin_name) && connected && !is_ground_net(&net.name) {
                f.push(finding(
                    "ERC-012",
                    Error,
                    format!(
                        "{} ground pin {} is on non-ground net {}",
                        pin.refdes, pin.pin_name, net.name
                    ),
                    vec![pin.symbol_id.clone()],
                    None,
                ));
            }
            if is_supply_pin(&pin.pin_name) && is_ground_net(&net.name) {
                f.push(finding(
                    "ERC-012",
                    Error,
                    format!(
                        "{} supply pin {} is tied to ground net {}",
                        pin.refdes, pin.pin_name, net.name
                    ),
                    vec![pin.symbol_id.clone()],
                    None,
                ));
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
            f.push(finding(
                "ERC-003",
                Error,
                format!("net {} feeds power input pin(s) {} but nothing drives it (no power output pin or power port)", net.name, power_in.iter().map(|p| format!("{}.{}", p.refdes, p.pin_name)).collect::<Vec<_>>().join(", ")),
                power_in.iter().map(|p| p.symbol_id.clone()).collect(),
                None,
            ));
        }
        if outputs.len() > 1 {
            f.push(finding(
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
                None,
            ));
        }
        if net.label_names.len() > 1 {
            f.push(finding(
                "ERC-005",
                Warning,
                format!(
                    "one net carries several names: {}",
                    net.label_names.join(", ")
                ),
                net.label_ids.clone(),
                None,
            ));
        }
        if net.pins.is_empty() {
            f.push(finding(
                "ERC-009",
                Warning,
                format!("label {} is not attached to any pin", net.name),
                net.label_ids.clone(),
                None,
            ));
        }
        if !is_ground_net(&net.name)
            && net
                .pins
                .iter()
                .any(|p| p.electrical_type == PinType::PowerIn && is_supply_pin(&p.pin_name))
            && !decoupled.contains(&net.name)
        {
            f.push(finding(
                "ERC-010",
                Warning,
                format!("supply net {} has no capacitor to ground", net.name),
                net.pins.iter().map(|p| p.symbol_id.clone()).collect(),
                None,
            ));
        }
    }

    // Reference designators.
    let mut refs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for s in p.schematic.symbols.values() {
        refs.entry(s.refdes.as_str())
            .or_default()
            .push(s.id.as_str());
        if s.refdes.ends_with('?') || s.refdes.is_empty() {
            f.push(finding(
                "ERC-007",
                Error,
                format!("symbol {} is not annotated", s.refdes),
                vec![s.id.clone()],
                Some(s.position),
            ));
        }
    }
    for (r, ids) in refs {
        if ids.len() > 1 {
            f.push(finding(
                "ERC-006",
                Error,
                format!("reference designator {r} is used {} times", ids.len()),
                ids.iter().map(|s| s.to_string()).collect(),
                None,
            ));
        }
    }
    for (wire, pt) in &nl.dangling_wire_ends {
        f.push(finding(
            "ERC-008",
            Warning,
            "wire end is not connected to anything",
            vec![wire.clone()],
            Some(*pt),
        ));
    }

    // Component ratings vs declared net voltages/currents (ERC-011, ERC-014..016). The
    // declared values are peak working voltages / continuous currents (Constraints).
    for s in p.schematic.symbols.values() {
        let Some(def) = p.library.get(&s.component_id) else {
            continue;
        };
        // Nets on the pins matching `keep`.
        let nets_of = |keep: &dyn Fn(&SymbolPin) -> bool| -> Vec<String> {
            def.symbol
                .pins
                .iter()
                .filter(|pin| keep(pin))
                .filter_map(|pin| nl.pin_net.get(&(s.id.clone(), pin.number.clone())))
                .cloned()
                .collect()
        };
        let all_nets = nets_of(&|_| true);
        let volts = declared_max(&all_nets, &p.constraints.net_voltages_v);

        if def.category == "Capacitor" {
            check_rating(
                &mut f,
                "ERC-011",
                s,
                def,
                &[("voltage", 1.5, "check DC-bias derating")],
                volts,
                "rail voltage",
                "V",
            );
        }

        // Regulator / IC supply input: highest declared rail on its supply-named power inputs.
        let supply_nets =
            nets_of(&|pin| pin.electrical_type == PinType::PowerIn && is_supply_pin(&pin.name));
        if !supply_nets.is_empty() {
            check_rating(
                &mut f,
                "ERC-014",
                s,
                def,
                &[
                    (
                        "input_voltage_max",
                        1.2,
                        "no headroom for input transients; check the clamp",
                    ),
                    (
                        "supply_voltage_max",
                        1.05,
                        "no headroom for rail regulation tolerance",
                    ),
                ],
                declared_max(&supply_nets, &p.constraints.net_voltages_v),
                "supply rail voltage",
                "V",
            );
        }

        if def.category == "Inductor" {
            let amps = declared_max(&all_nets, &p.constraints.net_currents_a);
            check_rating(
                &mut f,
                "ERC-015",
                s,
                def,
                &[(
                    "saturation_current",
                    1.2,
                    "peak current is 1.2x DC at a 40 % ripple ratio",
                )],
                amps,
                "net current",
                "A",
            );
            check_rating(
                &mut f,
                "ERC-015",
                s,
                def,
                &[(
                    "current",
                    1.1,
                    "little headroom for ripple RMS and self-heating",
                )],
                amps,
                "net current",
                "A",
            );
        }

        // MOSFET drain-source: any pin named D/DRAIN or S/SOURCE.
        let ds_nets = nets_of(&|pin| is_drain_source_pin(&pin.name));
        let is_fet = def.ratings.iter().any(|r| r.name == "vds_max")
            || (def.category == "Transistor"
                && def
                    .symbol
                    .pins
                    .iter()
                    .any(|pin| is_drain_source_pin(&pin.name)));
        if is_fet {
            check_rating(
                &mut f,
                "ERC-016",
                s,
                def,
                &[(
                    "vds_max",
                    1.25,
                    "below the 80 % Vds derating for switch-node ringing",
                )],
                declared_max(&ds_nets, &p.constraints.net_voltages_v),
                "drain/source rail voltage",
                "V",
            );
        }
    }
    // Resistor values must be numeric.
    for s in p.schematic.symbols.values() {
        if let Some(def) = p.library.get(&s.component_id) {
            if def.category == "Resistor" && parse_value(&s.value).is_none() {
                f.push(finding(
                    "ERC-013",
                    Warning,
                    format!(
                        "{} value '{}' is not a numeric resistance",
                        s.refdes, s.value
                    ),
                    vec![s.id.clone()],
                    None,
                ));
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
            "rating >= rail (error), >= 1.5x rail (warning); uses declared net voltages; \
             missing rating or rail is reported as not checked",
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
        rule(
            "ERC-014",
            "Regulator/IC supply input voltage rating",
            "highest declared rail on supply-named PowerIn pins vs input_voltage_max \
             (warning < 1.2x) or supply_voltage_max (warning < 1.05x)",
            "EAK (margins are design heuristics, not from a standard)",
        ),
        rule(
            "ERC-015",
            "Inductor current rating",
            "highest declared net current vs saturation_current (warning < 1.2x: peak at \
             40 % ripple, TI SLVA477B) and rated current (warning < 1.1x)",
            "EAK; ripple guidance TI SLVA477B",
        ),
        rule(
            "ERC-016",
            "MOSFET drain-source voltage rating",
            "highest declared rail on D/S pins vs vds_max (warning < 1.25x, i.e. 80 % derating)",
            "EAK (80 % derating is a design heuristic, not from a standard)",
        ),
    ];
    let not_implemented = vec![
        "Pin-to-pin compatibility matrix beyond driver conflicts".into(),
        "Hierarchical sheet / bus consistency (hierarchy not yet supported)".into(),
        "Analog simulation, operating-point and tolerance analysis".into(),
        "Component stress beyond capacitor voltage, IC supply voltage, inductor current and \
         MOSFET Vds (diode reverse voltage, resistor power, MOSFET drain current and Vgs, \
         thermal/junction temperature)"
            .into(),
    ];
    let limitations = vec![
        "Pin-name heuristics for supply/ground pins can miss unusual naming".into(),
        "Ratings in the built-in library are unverified unless marked verified".into(),
        "Rating checks use declared net voltages/currents only: switch-node ringing, input \
         transients and inductor ripple peaks are not simulated (margins cover them \
         heuristically); undeclared nets are reported as not checked"
            .into(),
    ];
    Report::new("ERC", p, rules, f, not_implemented, limitations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::geom::mm;
    use eak_core::{
        FootprintDef, LabelKind, Model3d, NetLabel, Point, Rating, Rotation, SymbolDef,
    };

    fn rating(name: &str, value: f64, unit: &str, verified: bool) -> Rating {
        Rating {
            name: name.into(),
            value,
            unit: unit.into(),
            source: if verified {
                "test datasheet".into()
            } else {
                "unverified".into()
            },
            verified,
        }
    }

    fn part(category: &str, pins: &[(&str, PinType)], ratings: Vec<Rating>) -> ComponentDef {
        let pins: Vec<SymbolPin> = pins
            .iter()
            .enumerate()
            .map(|(i, (name, t))| SymbolPin {
                number: (i + 1).to_string(),
                name: (*name).into(),
                electrical_type: *t,
                position: Point::mm(2.54 * i as f64, 0.0),
            })
            .collect();
        ComponentDef {
            id: format!("TEST_{category}"),
            name: category.into(),
            category: category.into(),
            refdes_prefix: "X".into(),
            default_value: String::new(),
            manufacturer: None,
            mpn: None,
            lcsc: None,
            datasheet: None,
            pin_map: pins
                .iter()
                .map(|p| (p.number.clone(), vec![p.number.clone()]))
                .collect(),
            symbol: SymbolDef {
                pins,
                body_half_w: mm(2.0),
                body_half_h: mm(2.0),
            },
            footprint: FootprintDef {
                name: "TEST".into(),
                pads: vec![],
                courtyard_half_w: mm(1.0),
                courtyard_half_h: mm(1.0),
                body_height: None,
            },
            ratings,
            model3d: Model3d {
                kind: "box".into(),
                color: "#000".into(),
                external_ref: None,
            },
            provenance: "test".into(),
        }
    }

    /// One part with pin i labelled `nets[i]`, plus declared voltages/currents; returns the
    /// findings of `rule_id`.
    fn check(
        def: ComponentDef,
        nets: &[&str],
        volts: &[(&str, f64)],
        amps: &[(&str, f64)],
        rule_id: &str,
    ) -> Vec<(Severity, String)> {
        let mut p = Project::new("t");
        let sheet = p.first_sheet_id().unwrap().clone();
        let sym = SymbolInstance {
            id: "sym_1".into(),
            sheet_id: sheet.clone(),
            component_id: def.id.clone(),
            refdes: "U1".into(),
            value: String::new(),
            position: Point::mm(50.0, 50.0),
            rotation: Rotation::R0,
        };
        p.library.insert(def.id.clone(), def);
        for (i, net) in nets.iter().enumerate() {
            let pos = p.pin_position(&sym, &(i + 1).to_string()).unwrap();
            let id = format!("lbl_{i}");
            p.schematic.labels.insert(
                id.clone(),
                NetLabel {
                    id,
                    sheet_id: sheet.clone(),
                    name: (*net).into(),
                    position: pos,
                    kind: LabelKind::Power,
                },
            );
        }
        p.schematic.symbols.insert(sym.id.clone(), sym);
        for (n, v) in volts {
            p.constraints.net_voltages_v.insert((*n).into(), *v);
        }
        for (n, a) in amps {
            p.constraints.net_currents_a.insert((*n).into(), *a);
        }
        let r = run(&p);
        assert!(
            r.rules.iter().any(|x| x.id == rule_id),
            "{rule_id} not listed"
        );
        r.findings
            .into_iter()
            .filter(|f| f.rule_id == rule_id)
            .map(|f| (f.severity, f.message))
            .collect()
    }

    fn severities(f: &[(Severity, String)]) -> Vec<Severity> {
        let mut s: Vec<Severity> = f.iter().map(|x| x.0).collect();
        s.sort();
        s
    }

    fn regulator(r: Vec<Rating>) -> ComponentDef {
        part(
            "Power",
            &[
                ("VIN", PinType::PowerIn),
                ("GND", PinType::PowerIn),
                ("SW", PinType::Output),
            ],
            r,
        )
    }

    const REG_NETS: &[&str] = &["VIN", "GND", "SW"];
    const RAIL_24: &[(&str, f64)] = &[("VIN", 24.0)];

    #[test]
    fn erc014_regulator_input_voltage() {
        let ok = check(
            regulator(vec![rating("input_voltage_max", 40.0, "V", true)]),
            REG_NETS,
            RAIL_24,
            &[],
            "ERC-014",
        );
        assert!(ok.is_empty(), "{ok:?}");
        // 28 V part on 24 V: within rating but under the 1.2x transient headroom.
        let w = check(
            regulator(vec![rating("input_voltage_max", 28.0, "V", true)]),
            REG_NETS,
            RAIL_24,
            &[],
            "ERC-014",
        );
        assert_eq!(severities(&w), vec![Warning]);
        let e = check(
            regulator(vec![rating("input_voltage_max", 20.0, "V", false)]),
            REG_NETS,
            RAIL_24,
            &[],
            "ERC-014",
        );
        assert_eq!(severities(&e), vec![Error]);
        assert!(e[0].1.contains("rating unverified"), "{}", e[0].1);
    }

    #[test]
    fn erc014_missing_rating_or_rail_is_not_checked() {
        let none = check(regulator(vec![]), REG_NETS, RAIL_24, &[], "ERC-014");
        assert_eq!(severities(&none), vec![Info]);
        assert!(none[0].1.contains("not checked"), "{}", none[0].1);
        let no_rail = check(
            regulator(vec![rating("input_voltage_max", 40.0, "V", true)]),
            REG_NETS,
            &[],
            &[],
            "ERC-014",
        );
        assert_eq!(severities(&no_rail), vec![Info]);
        assert!(no_rail[0].1.contains("no declared"), "{}", no_rail[0].1);
        // MCU on 3.3 V with an unverified 3.6 V limit: passes, reported as unverified.
        let mcu = part(
            "MCU",
            &[("VDD", PinType::PowerIn), ("GND", PinType::PowerIn)],
            vec![rating("supply_voltage_max", 3.6, "V", false)],
        );
        let u = check(mcu, &["+3V3", "GND"], &[("+3V3", 3.3)], &[], "ERC-014");
        assert_eq!(severities(&u), vec![Info]);
        assert!(u[0].1.contains("unverified"), "{}", u[0].1);
    }

    fn inductor(r: Vec<Rating>) -> ComponentDef {
        part(
            "Inductor",
            &[("~", PinType::Passive), ("~", PinType::Passive)],
            r,
        )
    }

    const L_NETS: &[&str] = &["SW", "+5V"];
    const LOAD_3A: &[(&str, f64)] = &[("+5V", 3.0)];

    #[test]
    fn erc015_inductor_current() {
        let ok = check(
            inductor(vec![
                rating("saturation_current", 4.5, "A", true),
                rating("current", 4.0, "A", true),
            ]),
            L_NETS,
            &[],
            LOAD_3A,
            "ERC-015",
        );
        assert!(ok.is_empty(), "{ok:?}");
        // Isat 3.4 A < 1.2 x 3 A (warning); rated 2.5 A < 3 A (error).
        let bad = check(
            inductor(vec![
                rating("saturation_current", 3.4, "A", true),
                rating("current", 2.5, "A", true),
            ]),
            L_NETS,
            &[],
            LOAD_3A,
            "ERC-015",
        );
        assert_eq!(severities(&bad), vec![Warning, Error]);
        // Generic library inductor (rated current only): saturation is reported as not
        // checked, never as passing.
        let generic = check(
            inductor(vec![rating("current", 1.0, "A", false)]),
            L_NETS,
            &[],
            LOAD_3A,
            "ERC-015",
        );
        assert_eq!(severities(&generic), vec![Info, Error]);
        assert!(generic
            .iter()
            .any(|(s, m)| *s == Info && m.contains("saturation_current")));
        let undeclared = check(
            inductor(vec![rating("saturation_current", 4.5, "A", true)]),
            L_NETS,
            &[],
            &[],
            "ERC-015",
        );
        assert_eq!(severities(&undeclared), vec![Info, Info]);
        assert!(undeclared.iter().all(|(_, m)| m.contains("not checked")));
    }

    fn mosfet(r: Vec<Rating>) -> ComponentDef {
        part(
            "Transistor",
            &[
                ("G", PinType::Input),
                ("D", PinType::Passive),
                ("S", PinType::Passive),
            ],
            r,
        )
    }

    // High-side FET: drain on VIN (24 V), source on the switch node (undeclared).
    const HS_NETS: &[&str] = &["HG", "VIN", "SW"];

    #[test]
    fn erc016_mosfet_vds() {
        let ok = check(
            mosfet(vec![rating("vds_max", 40.0, "V", true)]),
            HS_NETS,
            RAIL_24,
            &[],
            "ERC-016",
        );
        assert!(ok.is_empty(), "{ok:?}");
        // 30 V is exactly 1.25 x 24 V: meets the 80 % derating.
        let edge = check(
            mosfet(vec![rating("vds_max", 30.0, "V", true)]),
            HS_NETS,
            RAIL_24,
            &[],
            "ERC-016",
        );
        assert!(edge.is_empty(), "{edge:?}");
        let w = check(
            mosfet(vec![rating("vds_max", 25.0, "V", true)]),
            HS_NETS,
            RAIL_24,
            &[],
            "ERC-016",
        );
        assert_eq!(severities(&w), vec![Warning]);
        let e = check(
            mosfet(vec![rating("vds_max", 20.0, "V", true)]),
            HS_NETS,
            RAIL_24,
            &[],
            "ERC-016",
        );
        assert_eq!(severities(&e), vec![Error]);
        // Missing rating and wrong unit are not checked.
        let none = check(mosfet(vec![]), HS_NETS, RAIL_24, &[], "ERC-016");
        assert_eq!(severities(&none), vec![Info]);
        let unit = check(
            mosfet(vec![rating("vds_max", 30_000.0, "mV", true)]),
            HS_NETS,
            RAIL_24,
            &[],
            "ERC-016",
        );
        assert_eq!(severities(&unit), vec![Info]);
        assert!(unit[0].1.contains("unit"), "{}", unit[0].1);
    }

    fn capacitor(r: Vec<Rating>) -> ComponentDef {
        part(
            "Capacitor",
            &[("~", PinType::Passive), ("~", PinType::Passive)],
            r,
        )
    }

    #[test]
    fn erc011_capacitor_voltage_uses_shared_rating_check() {
        let cases = [
            (50.0, true, vec![]),
            (25.0, true, vec![Warning]),
            (16.0, false, vec![Error]),
            (50.0, false, vec![Info]),
        ];
        for (v, verified, want) in cases {
            let got = check(
                capacitor(vec![rating("voltage", v, "V", verified)]),
                &["VIN", "GND"],
                RAIL_24,
                &[],
                "ERC-011",
            );
            assert_eq!(severities(&got), want, "{v} V: {got:?}");
        }
        let no_rail = check(
            capacitor(vec![rating("voltage", 50.0, "V", true)]),
            &["VIN", "GND"],
            &[],
            &[],
            "ERC-011",
        );
        assert_eq!(severities(&no_rail), vec![Info]);
    }
}
