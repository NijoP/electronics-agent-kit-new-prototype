//! Part stress (ERC-022..024): each part's ratings against the conditions the declared net
//! voltages and currents put on it, with the derating facts of `eak_engineering::rating`.
//! A part without the rating, or a pin net without the declared value, leaves the rule UNKNOWN
//! naming it; nothing is assumed. ERC-011 (capacitor voltage) gets the same fail-closed states.

use crate::report::{finding, rule, Bound, Finding, RuleInfo, Severity::*};
use eak_core::{ComponentDef, Project, SymbolInstance};
use eak_engineering::calc::parse_value;
use eak_engineering::netlist::Netlist;
use eak_engineering::rating::{derating, CURRENT_RATINGS, POWER_RATINGS, VOLTAGE_RATINGS};
use eak_engineering::verify::{part_rating, Verdict};
use std::collections::BTreeMap;

/// Parts whose rated current is a series (through) current: inductors, diodes, transistors, fuses.
const SERIES_PREFIXES: [&str; 4] = ["L", "D", "Q", "F"];

pub(crate) fn rules() -> Vec<RuleInfo> {
    let src = "EAK; derating facts from eak_engineering::rating (DESK component-selection skill)";
    vec![
        rule(
            "ERC-022",
            "Part voltage rating",
            "highest minus lowest declared voltage across the part's pin nets vs its lowest voltage rating: across two nets this is the voltage on the part: above the rating is an error, above the derated limit a warning; across more nets it is an upper bound, and exceeding it is UNKNOWN (capacitors: ERC-011)",
            src,
        ),
        rule(
            "ERC-023",
            "Part current rating",
            "L, D, Q and F parts: highest declared current on any pin net (upper bound: the part may carry less) vs its derated current rating: within it is a pass, above it UNKNOWN (never FAIL, the part current is not known)",
            src,
        ),
        rule(
            "ERC-024",
            "Resistor power rating",
            "P = ΔV² / R from the declared voltages of the two pin nets vs the power rating; no derating fact on record, so the limit is the rating",
            src,
        ),
    ]
}

/// The lowest rating of `names` in `unit`; `Err` names a matching rating in another unit.
fn lowest(def: &ComponentDef, names: &[&str], unit: &str) -> Result<Option<f64>, String> {
    let mut best: Option<f64> = None;
    for r in def.ratings.iter().filter(|r| names.contains(&r.name.as_str())) {
        if r.unit != unit {
            return Err(format!("rating {} is in {}, expected {unit}", r.name, r.unit));
        }
        best = Some(best.map_or(r.value, |b| b.min(r.value)));
    }
    Ok(best)
}

struct Stress {
    rule: &'static str,
    quantity: &'static str,
    unit: &'static str,
    names: &'static [&'static str],
}

const VOLTAGE: Stress = Stress {
    rule: "ERC-022",
    quantity: "voltage",
    unit: "V",
    names: VOLTAGE_RATINGS,
};
const CURRENT: Stress = Stress {
    rule: "ERC-023",
    quantity: "current",
    unit: "A",
    names: CURRENT_RATINGS,
};
const POWER: Stress = Stress {
    rule: "ERC-024",
    quantity: "power",
    unit: "W",
    names: POWER_RATINGS,
};

/// One part, one rule: a finding when the stress exceeds the derated limit, else the UNKNOWN or
/// BLOCKED reason (`None` on a pass). `exact` is false when `applied` is only an upper bound on
/// the part's own stress: exceeding it then proves nothing and is UNKNOWN, never FAIL.
fn evaluate(
    s: &SymbolInstance,
    def: &ComponentDef,
    k: &Stress,
    applied: Result<f64, String>,
    exact: bool,
    f: &mut Vec<Finding>,
) -> Option<String> {
    let rating = match lowest(def, k.names, k.unit) {
        Ok(Some(r)) => r,
        Ok(None) => return Some(format!("{}: no {} rating", s.refdes, k.quantity)),
        Err(e) => return Some(format!("{}: BLOCKED: {e}", s.refdes)),
    };
    let applied = match applied {
        Ok(a) => a,
        Err(why) => return Some(format!("{}: {why}", s.refdes)),
    };
    let (fraction, why) = match derating(&s.refdes, k.quantity) {
        Some(d) => (d.max_fraction, d.source),
        None => (1.0, "no derating fact on record"),
    };
    let c = part_rating(Some(applied), Some(rating), fraction, k.unit);
    match c.verdict {
        Verdict::Pass => None,
        Verdict::Unknown | Verdict::Blocked => Some(format!("{}: {}", s.refdes, c.reason)),
        Verdict::Fail if !exact => Some(format!(
            "{}: the upper bound {:.3} {} exceeds the derated limit {:.3} {}; the part's own {} is not known",
            s.refdes,
            applied.abs(),
            k.unit,
            rating * fraction,
            k.unit,
            k.quantity
        )),
        Verdict::Fail => {
            let (u, q, a) = (k.unit, k.quantity, applied.abs());
            let over = a > rating;
            let (severity, limit) = if over {
                (Error, rating)
            } else {
                (Warning, rating * fraction)
            };
            let message = if over {
                format!(
                    "{} sees {a:.3} {u} but is rated {rating} {u} ({q})",
                    s.refdes
                )
            } else {
                format!(
                    "{} sees {a:.3} {u}, above {:.0} % of its {rating} {u} {q} rating ({why})",
                    s.refdes,
                    fraction * 100.0
                )
            };
            f.push(
                finding(
                    k.rule,
                    severity,
                    message,
                    vec![s.id.clone()],
                    Some(s.position),
                )
                .measured(a, u)
                .required(Bound::Max, limit, u)
                .fix(format!(
                    "Choose a part for {} rated above {:.3} {u}, or correct the declared {q} of its nets if it is overstated.",
                    s.refdes,
                    a / fraction
                )),
            );
            None
        }
    }
}

/// Findings for ERC-022..024, and per rule (ERC-011 included) the reasons it is UNKNOWN.
pub(crate) fn check(
    p: &Project,
    nl: &Netlist,
    f: &mut Vec<Finding>,
) -> Vec<(&'static str, String)> {
    let volts = &p.constraints.net_voltages_v;
    let amps = &p.constraints.net_currents_a;
    let mut unknown: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for s in p.schematic.symbols.values() {
        let Some(def) = p.library.get(&s.component_id) else {
            continue;
        };
        let mut nets: Vec<&str> = def
            .symbol
            .pins
            .iter()
            .filter_map(|pin| nl.pin_net.get(&(s.id.clone(), pin.number.clone())))
            .map(String::as_str)
            .collect();
        nets.sort_unstable();
        nets.dedup();
        if nets.len() < 2 {
            continue;
        }
        let undeclared: Vec<&str> = nets
            .iter()
            .copied()
            .filter(|n| !volts.contains_key(*n))
            .collect();
        let vs = nets.iter().filter_map(|n| volts.get(*n).copied());
        let lo = vs.clone().fold(f64::MAX, f64::min);
        let hi = vs.fold(f64::MIN, f64::max);
        let across = if undeclared.is_empty() {
            Ok(hi - lo)
        } else {
            Err(format!("no declared voltage on {}", undeclared.join(", ")))
        };
        // Across exactly two nets the difference is the voltage on the part; with more it bounds
        // every pin pair, and the rating may cover only one of them.
        let exact = nets.len() == 2;
        let mut why: Vec<(&'static str, Option<String>)> = vec![];
        if def.category == "Capacitor" {
            // ERC-011 checks the value; here only whether it could.
            let rated = def.ratings.iter().any(|r| r.name == "voltage");
            let reason = match (&across, rated) {
                (_, false) => Some(format!("{}: no voltage rating", s.refdes)),
                (Err(e), true) => Some(format!("{}: {e}", s.refdes)),
                (Ok(_), true) => None,
            };
            why.push(("ERC-011", reason));
        } else {
            why.push(("ERC-022", evaluate(s, def, &VOLTAGE, across.clone(), exact, f)));
        }
        let prefix: String = s
            .refdes
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect();
        if SERIES_PREFIXES.contains(&prefix.as_str()) {
            let current = nets
                .iter()
                .filter_map(|n| amps.get(*n).copied())
                .reduce(f64::max)
                .ok_or_else(|| format!("no declared current on {}", nets.join(", ")));
            why.push(("ERC-023", evaluate(s, def, &CURRENT, current, false, f)));
        }
        if def.category == "Resistor" {
            let power = match (parse_value(&s.value).filter(|r| *r > 0.0), across) {
                (None, _) => Err(format!("value {:?} is not a positive resistance", s.value)),
                (Some(_), _) if nets.len() != 2 => {
                    Err("power is computed for two-terminal resistors only".into())
                }
                (Some(r), Ok(dv)) => Ok(dv * dv / r),
                (Some(_), Err(e)) => Err(e),
            };
            why.push(("ERC-024", evaluate(s, def, &POWER, power, true, f)));
        }
        for (rule, reason) in why {
            if let Some(r) = reason {
                unknown.entry(rule).or_default().push(r);
            }
        }
    }
    unknown
        .into_iter()
        .map(|(rule, mut why)| {
            let more = why.len().saturating_sub(10);
            why.truncate(10);
            let tail = match more {
                0 => String::new(),
                n => format!(" and {n} more"),
            };
            (rule, format!("not evaluated for {}{tail}", why.join("; ")))
        })
        .collect()
}
