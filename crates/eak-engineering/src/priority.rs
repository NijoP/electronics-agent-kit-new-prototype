//! Net routing priority: route the most constrained connections first.
//!
//! Ported (re-written, no source copied) from the "constrainedness" priority mental model of
//! DESK (`desk/routing/priority.py`, MIT, Copyright (c) 2026 NijoP; licence in
//! `third_party/licenses/DESK/LICENSE`):
//!
//! `priority = 3 * criticality + 1 * topology + 2 * urgency`
//!
//! * criticality: power nets (name or declared current above 0.5 A) 8, ground 7, high-speed 6
//!   (a differential pair found by name, or a clock name), other 4;
//! * topology: more than 10 terminals 8, more than 4 terminals 5, otherwise 2;
//! * urgency: declared current above 1.5 A 9, otherwise 5.
//!
//! So power, ground and high-speed nets route first, before the board fills up. High-speed is
//! name-based (EAK-IR declares no net class for it yet). DESK's escape-difficulty, congestion and
//! layer-scarcity terms are not ported: they need board geometry that EAK's router already owns
//! on its grid. The desktop route tool's "Route next" (`apps/desktop/src/routeTool.ts::netPriority`)
//! uses the same formula without the high-speed class.

use eak_core::{Point, Project};
use std::collections::{BTreeMap, BTreeSet};

const GROUND: [&str; 5] = ["GND", "VSS", "AGND", "DGND", "PGND"];
const POWER_PREFIXES: [&str; 10] = [
    "+", "VCC", "VDD", "3V3", "5V", "1V8", "VBAT", "VSYS", "VBUS", "VIN",
];
/// Name tokens of clock nets (split on `_`, `-`, `.`, `/`; trailing digits ignored).
const CLOCK_TOKENS: [&str; 6] = ["CLK", "SCK", "SCLK", "MCLK", "XTAL", "OSC"];

fn is_ground(net: &str) -> bool {
    GROUND.contains(&net.to_ascii_uppercase().as_str())
}

fn is_power_name(net: &str) -> bool {
    let n = net.to_ascii_uppercase();
    POWER_PREFIXES.iter().any(|p| n.starts_with(p))
}

fn is_clock_name(net: &str) -> bool {
    net.to_ascii_uppercase()
        .split(['_', '-', '.', '/'])
        .any(|t| CLOCK_TOKENS.contains(&t.trim_end_matches(|c: char| c.is_ascii_digit())))
}

/// Priority score of one net (higher routes first).
pub fn net_priority(net: &str, terminals: usize, current_a: Option<f64>, high_speed: bool) -> u32 {
    let amps = current_a.unwrap_or(0.0);
    let crit = if is_ground(net) {
        7
    } else if is_power_name(net) || amps > 0.5 {
        8
    } else if high_speed {
        6
    } else {
        4
    };
    let topo = if terminals > 10 {
        8
    } else if terminals > 4 {
        5
    } else {
        2
    };
    let urgency = if amps > 1.5 { 9 } else { 5 };
    3 * crit + topo + 2 * urgency
}

/// Terminal (pad) count per net from the PCB footprints.
pub fn terminal_counts(p: &Project) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for fp in p.pcb.footprints.values() {
        for net in fp.pad_nets.values() {
            *out.entry(net.clone()).or_insert(0) += 1;
        }
    }
    out
}

/// High-speed nets of the board: both nets of every differential pair found by name
/// (`routing_intel::diff_pairs`) and clock nets by name.
pub fn high_speed_nets(p: &Project) -> BTreeSet<String> {
    let nets: BTreeSet<String> = terminal_counts(p).into_keys().collect();
    let mut out: BTreeSet<String> = crate::routing_intel::diff_pairs(&nets)
        .into_iter()
        .flat_map(|(a, b)| [a, b])
        .collect();
    out.extend(nets.into_iter().filter(|n| is_clock_name(n)));
    out
}

/// Order unrouted connections: highest net priority first, then shortest first (stable).
pub fn order_connections(p: &Project, todo: &mut [(String, Point, Point)]) {
    let terminals = terminal_counts(p);
    let fast = high_speed_nets(p);
    let score = |net: &str| {
        let n = terminals.get(net).copied().unwrap_or(2);
        net_priority(
            net,
            n,
            p.constraints.net_currents_a.get(net).copied(),
            fast.contains(net),
        )
    };
    let len = |c: &(String, Point, Point)| c.1.dist(c.2);
    todo.sort_by(|a, b| {
        let by_priority = score(&b.0).cmp(&score(&a.0));
        by_priority.then(len(a).total_cmp(&len(b)))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_and_ground_outrank_signals() {
        let sig = net_priority("SIG", 2, None, false);
        assert!(net_priority("VCC", 2, None, false) > sig);
        assert!(net_priority("GND", 2, None, false) > sig);
        assert!(net_priority("SIG", 2, Some(2.0), false) > sig);
        assert!(net_priority("SIG", 12, None, false) > sig);
    }

    #[test]
    fn high_speed_ranks_after_power_and_ground_before_signals() {
        let hs = net_priority("USB_DP", 2, None, true);
        assert!(hs < net_priority("GND", 2, None, false));
        assert!(hs < net_priority("VCC", 2, None, false));
        assert!(hs > net_priority("SIG", 2, None, false));
    }

    #[test]
    fn clock_names_and_named_pairs_are_high_speed() {
        assert!(is_clock_name("SPI_SCK") && is_clock_name("MCLK") && is_clock_name("CLK2"));
        assert!(is_clock_name("xtal_in") && !is_clock_name("BLOCK") && !is_clock_name("SIG"));
        let mut p = Project::new("hs");
        let mut fp = eak_core::FootprintInstance {
            id: "u".into(),
            symbol_id: "s".into(),
            component_id: "x".into(),
            refdes: "U1".into(),
            position: Point::default(),
            rotation: eak_core::Rotation::R0,
            side: eak_core::Side::Top,
            pad_nets: BTreeMap::new(),
            locked: false,
        };
        for (pad, net) in [
            ("1", "USB_DP"),
            ("2", "USB_DN"),
            ("3", "CLK_P"),
            ("4", "RESET_N"),
        ] {
            fp.pad_nets.insert(pad.into(), net.into());
        }
        p.pcb.footprints.insert("u".into(), fp);
        let fast = high_speed_nets(&p);
        assert!(fast.contains("USB_DP") && fast.contains("USB_DN"));
        assert!(fast.contains("CLK_P"), "a clock name");
        assert!(!fast.contains("RESET_N"), "_N alone is not a pair");
    }

    #[test]
    fn orders_by_priority_then_length() {
        let p = Project::new("prio");
        let mut todo = vec![
            ("SIG".to_string(), Point::new(0, 0), Point::new(1_000, 0)),
            ("VCC".to_string(), Point::new(0, 0), Point::new(9_000, 0)),
            ("SIG".to_string(), Point::new(0, 0), Point::new(500, 0)),
        ];
        order_connections(&p, &mut todo);
        assert_eq!(todo[0].0, "VCC");
        assert!(todo[1].1.dist(todo[1].2) < 600.0);
    }
}
