//! SPICE netlist export from EAK-IR schematic connectivity.
//!
//! Mapping is by the library part's `refdes_prefix` (falling back to the reference letter):
//! `R`/`C`/`L` -> passive with the instance `value` (engineering notation, `10k`, `100n`);
//! `D` -> diode (anode/cathode by pin name `A`/`K`, else pin 1 = anode); `V`/`I` -> source
//! (pin 1 = positive / current enters, pin 2 = negative). Parts with any other prefix have no
//! SPICE model and are reported in [`Export::skipped`]; nothing is silently dropped.
//! Nets named like ground (`GND`, `VSS`, ...) become SPICE node 0.

use crate::SimError;
use eak_core::{ComponentDef, Project, SymbolInstance};
use eak_engineering::{calc::parse_value, netlist};
use eak_validation::erc::is_ground_net;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    Led,
    VoltageSource,
    CurrentSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Voltage,
    Current,
}

/// A stimulus attached to existing nets for this run only (not stored in the project).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtraSource {
    /// Reference name, e.g. `V1` (letters/digits only).
    pub name: String,
    pub kind: SourceKind,
    /// Positive net (current enters here for a current source).
    pub positive_net: String,
    /// Negative net; empty means ground.
    #[serde(default)]
    pub negative_net: String,
    /// `5`, `3.3V`, `DC 5`, `PULSE(0 5 0 1n 1n 1 2)`, `SIN(0 1 1k)`, `PWL(...)`.
    pub spec: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub refdes: String,
    pub kind: ElementKind,
    pub value: String,
    pub line: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skipped {
    pub refdes: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Export {
    /// Netlist body: title, elements, models. No `.end`, no control block (the runner adds it).
    pub text: String,
    /// SPICE node name -> EAK net name (ground is node `0`, not listed).
    pub nodes: BTreeMap<String, String>,
    pub elements: Vec<Element>,
    pub skipped: Vec<Skipped>,
    pub warnings: Vec<String>,
}

/// What the Simulation panel needs before a run: nets, mapped elements, skipped parts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Describe {
    pub nets: Vec<String>,
    pub elements: Vec<Element>,
    pub skipped: Vec<Skipped>,
}

const GENERIC_DIODE: &str = ".model EAK_D D(IS=1e-14 N=1)";
const GENERIC_LED: &str = ".model EAK_LED D(IS=1e-18 N=2 RS=5)";

fn alnum(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

fn sanitize_node(net: &str, used: &mut BTreeMap<String, String>) -> String {
    let base: String = net
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let mut cand = format!("n_{base}");
    let mut k = 2;
    // ngspice node names are case-insensitive, so uniqueness is checked on the lowered name.
    while used.get(&cand).is_some_and(|n| n != net) {
        cand = format!("n_{base}_{k}");
        k += 1;
    }
    used.insert(cand.clone(), net.to_string());
    cand
}

/// Validate a source specification and return the SPICE source text.
pub fn source_spec(spec: &str) -> Result<String, SimError> {
    let s = spec.trim();
    let bad = || SimError::InvalidRequest(format!("unsupported source specification '{spec}'"));
    let up = s.to_ascii_uppercase();
    let structured = ["PULSE(", "SIN(", "PWL(", "DC "]
        .iter()
        .any(|p| up.starts_with(p));
    if structured {
        let ok = s.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '+' | '-' | '(' | ')' | ',')
        });
        if !ok {
            return Err(bad());
        }
        return Ok(s.to_string());
    }
    match parse_value(s) {
        Some(v) if v.is_finite() => Ok(format!("DC {v:e}")),
        _ => Err(bad()),
    }
}

fn prefix_of(def: &ComponentDef, sym: &SymbolInstance) -> char {
    def.refdes_prefix
        .chars()
        .chain(sym.refdes.chars())
        .find(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('?')
}

/// Pin numbers of a two-terminal part ordered `(first, second)`; for diodes `(anode, cathode)`.
fn two_pins(def: &ComponentDef, diode: bool) -> Option<(String, String)> {
    if def.symbol.pins.len() != 2 {
        return None;
    }
    let mut pins: Vec<_> = def.symbol.pins.iter().collect();
    pins.sort_by(|a, b| {
        (a.number.parse::<u32>().unwrap_or(u32::MAX), &a.number)
            .cmp(&(b.number.parse::<u32>().unwrap_or(u32::MAX), &b.number))
    });
    if diode {
        let is = |n: &str, names: &[&str]| names.contains(&n.to_ascii_uppercase().as_str());
        for (i, j) in [(0, 1), (1, 0)] {
            if is(&pins[i].name, &["A", "ANODE", "+"])
                && is(&pins[j].name, &["K", "C", "CATHODE", "-"])
            {
                return Some((pins[i].number.clone(), pins[j].number.clone()));
            }
        }
    }
    Some((pins[0].number.clone(), pins[1].number.clone()))
}

pub fn export_netlist(p: &Project, extra: &[ExtraSource]) -> Result<Export, SimError> {
    let e = build_export(p, extra)?;
    if e.elements.is_empty() {
        return Err(SimError::EmptyCircuit(
            "no simulatable elements (place and connect R, C, L, diodes or sources)".into(),
        ));
    }
    Ok(e)
}

fn build_export(p: &Project, extra: &[ExtraSource]) -> Result<Export, SimError> {
    let nl = netlist::build(p);
    let mut node_of: BTreeMap<String, String> = BTreeMap::new(); // net -> spice node
    let mut nodes: BTreeMap<String, String> = BTreeMap::new(); // spice node -> net
    let mut node = |net: &str, nodes: &mut BTreeMap<String, String>| -> String {
        if is_ground_net(net) {
            return "0".into();
        }
        if let Some(n) = node_of.get(net) {
            return n.clone();
        }
        let n = sanitize_node(net, nodes);
        node_of.insert(net.to_string(), n.clone());
        n
    };

    let mut elements = Vec::new();
    let mut skipped = Vec::new();
    let mut warnings = Vec::new();
    let mut used_refs = BTreeSet::new();
    let (mut need_d, mut need_led) = (false, false);

    let mut syms: Vec<&SymbolInstance> = p.schematic.symbols.values().collect();
    syms.sort_by(|a, b| a.refdes.cmp(&b.refdes));
    for sym in syms {
        let skip = |reason: &str, skipped: &mut Vec<Skipped>| {
            skipped.push(Skipped {
                refdes: sym.refdes.clone(),
                reason: reason.to_string(),
            })
        };
        let Some(def) = p.library.get(&sym.component_id) else {
            skip(
                "component definition missing from the project library",
                &mut skipped,
            );
            continue;
        };
        let letter = prefix_of(def, sym);
        let kind = match letter {
            'R' => ElementKind::Resistor,
            'C' => ElementKind::Capacitor,
            'L' => ElementKind::Inductor,
            'D' if def.category.eq_ignore_ascii_case("LED") => ElementKind::Led,
            'D' => ElementKind::Diode,
            'V' => ElementKind::VoltageSource,
            'I' => ElementKind::CurrentSource,
            _ => {
                skip(
                    "no SPICE model for this part (only R, C, L, D/LED, V, I are simulated)",
                    &mut skipped,
                );
                continue;
            }
        };
        let Some((first, second)) =
            two_pins(def, matches!(kind, ElementKind::Diode | ElementKind::Led))
        else {
            skip("expected a two-terminal symbol", &mut skipped);
            continue;
        };
        let net_of = |pin: &str| nl.pin_net.get(&(sym.id.clone(), pin.to_string())).cloned();
        let (Some(n1), Some(n2)) = (net_of(&first), net_of(&second)) else {
            skip("a pin is not connected to any net", &mut skipped);
            continue;
        };
        let name = alnum(&sym.refdes);
        if name.is_empty() || !used_refs.insert(name.to_ascii_uppercase()) {
            skip("reference designator is empty or duplicated", &mut skipped);
            continue;
        }
        let (a, b) = (node(&n1, &mut nodes), node(&n2, &mut nodes));
        let id = format!(
            "{}{}",
            letter,
            name.trim_start_matches(|c: char| c.eq_ignore_ascii_case(&letter))
        );
        let line = match kind {
            ElementKind::Resistor | ElementKind::Capacitor | ElementKind::Inductor => {
                match parse_value(&sym.value).filter(|v| v.is_finite() && *v > 0.0) {
                    Some(v) => format!("{id} {a} {b} {v:e}"),
                    None => {
                        skip(
                            &format!(
                                "value '{}' is not a positive engineering value (e.g. 10k, 100n)",
                                sym.value
                            ),
                            &mut skipped,
                        );
                        continue;
                    }
                }
            }
            ElementKind::Diode => {
                need_d = true;
                format!("{id} {a} {b} EAK_D")
            }
            ElementKind::Led => {
                need_led = true;
                format!("{id} {a} {b} EAK_LED")
            }
            ElementKind::VoltageSource | ElementKind::CurrentSource => {
                match source_spec(&sym.value) {
                    Ok(s) => format!("{id} {a} {b} {s}"),
                    Err(_) => {
                        skip(
                            &format!(
                                "source value '{}' is not a supported specification",
                                sym.value
                            ),
                            &mut skipped,
                        );
                        continue;
                    }
                }
            }
        };
        elements.push(Element {
            refdes: sym.refdes.clone(),
            kind,
            value: sym.value.clone(),
            line,
        });
    }

    let all_nets: BTreeSet<&String> = nl.nets.keys().collect();
    for s in extra {
        let name = alnum(&s.name);
        if name.is_empty() || !used_refs.insert(name.to_ascii_uppercase()) {
            return Err(SimError::InvalidRequest(format!(
                "source name '{}' is empty or already used",
                s.name
            )));
        }
        let neg = if s.negative_net.trim().is_empty() {
            "GND"
        } else {
            s.negative_net.trim()
        };
        for n in [s.positive_net.as_str(), neg] {
            if !is_ground_net(n) && !all_nets.contains(&n.to_string()) {
                return Err(SimError::InvalidRequest(format!(
                    "source {} references unknown net '{n}'",
                    s.name
                )));
            }
        }
        let (letter, kind) = match s.kind {
            SourceKind::Voltage => ('V', ElementKind::VoltageSource),
            SourceKind::Current => ('I', ElementKind::CurrentSource),
        };
        let (a, b) = (node(&s.positive_net, &mut nodes), node(neg, &mut nodes));
        if a == b {
            return Err(SimError::InvalidRequest(format!(
                "source {} has both terminals on the same node",
                s.name
            )));
        }
        let id = format!(
            "{letter}{}",
            name.trim_start_matches(|c: char| c.eq_ignore_ascii_case(&letter))
        );
        elements.push(Element {
            refdes: s.name.clone(),
            kind,
            value: s.spec.clone(),
            line: format!("{id} {a} {b} {}", source_spec(&s.spec)?),
        });
    }

    for s in &skipped {
        warnings.push(format!("{} not simulated: {}", s.refdes, s.reason));
    }
    if !elements.is_empty()
        && !elements.iter().any(|e| {
            matches!(
                e.kind,
                ElementKind::VoltageSource | ElementKind::CurrentSource
            )
        })
    {
        warnings.push("no voltage or current source: all node voltages will be 0 V".into());
    }
    let mut text = String::from("* EAK-IR schematic export (generated; do not edit)\n");
    for e in &elements {
        text.push_str(&e.line);
        text.push('\n');
    }
    if need_d {
        text.push_str(GENERIC_DIODE);
        text.push('\n');
    }
    if need_led {
        text.push_str(GENERIC_LED);
        text.push('\n');
    }
    Ok(Export {
        text,
        nodes,
        elements,
        skipped,
        warnings,
    })
}

pub fn describe(p: &Project) -> Describe {
    let nl = netlist::build(p);
    let (elements, skipped) = match build_export(p, &[]) {
        Ok(e) => (e.elements, e.skipped),
        Err(_) => (vec![], vec![]),
    };
    Describe {
        nets: nl.nets.keys().cloned().collect(),
        elements,
        skipped,
    }
}
