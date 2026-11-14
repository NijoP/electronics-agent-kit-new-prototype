//! Net routing priority: route the most constrained connections first.
//!
//! Ported (re-written, no source copied) from the "constrainedness" priority mental model of
//! DESK (`desk/routing/priority.py`, MIT, Copyright (c) 2026 NijoP; licence in
//! `third_party/licenses/DESK/LICENSE`):
//!
//! `priority = 3 * criticality + 1 * topology + 2 * urgency`
//!
//! * criticality: power nets (name or declared current above 0.5 A) 8, ground 7, other 4;
//! * topology: more than 10 terminals 8, more than 4 terminals 5, otherwise 2;
//! * urgency: declared current above 1.5 A 9, otherwise 5.
//!
//! DESK's escape-difficulty, congestion and layer-scarcity terms are not ported: they need
//! board geometry that EAK's router already owns on its grid. The desktop route tool's
//! "Route next" uses the same formula (`apps/desktop/src/routeTool.ts::netPriority`).

use eak_core::{Point, Project};
use std::collections::BTreeMap;

const GROUND: [&str; 5] = ["GND", "VSS", "AGND", "DGND", "PGND"];
const POWER_PREFIXES: [&str; 10] = [
    "+", "VCC", "VDD", "3V3", "5V", "1V8", "VBAT", "VSYS", "VBUS", "VIN",
];

fn is_ground(net: &str) -> bool {
    GROUND.contains(&net.to_ascii_uppercase().as_str())
}

fn is_power_name(net: &str) -> bool {
    let n = net.to_ascii_uppercase();
    POWER_PREFIXES.iter().any(|p| n.starts_with(p))
}

/// Priority score of one net (higher routes first).
pub fn net_priority(net: &str, terminals: usize, current_a: Option<f64>) -> u32 {
    let amps = current_a.unwrap_or(0.0);
    let crit = if is_ground(net) {
        7
    } else if is_power_name(net) || amps > 0.5 {
        8
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

/// Order unrouted connections: highest net priority first, then shortest first (stable).
pub fn order_connections(p: &Project, todo: &mut [(String, Point, Point)]) {
    let terminals = terminal_counts(p);
    let score = |net: &str| {
        let n = terminals.get(net).copied().unwrap_or(2);
        net_priority(net, n, p.constraints.net_currents_a.get(net).copied())
    };
    let len = |c: &(String, Point, Point)| c.1.dist(c.2);
    todo.sort_by(|a, b| {
        let by_priority = score(&b.0).cmp(&score(&a.0));
        by_priority.then(len(a).total_cmp(&len(b)))
    });
}

// Written without running (no local builds/tests in this phase): unrun.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_and_ground_outrank_signals() {
        assert!(net_priority("VCC", 2, None) > net_priority("SIG", 2, None));
        assert!(net_priority("GND", 2, None) > net_priority("SIG", 2, None));
        assert!(net_priority("SIG", 2, Some(2.0)) > net_priority("SIG", 2, None));
        assert!(net_priority("SIG", 12, None) > net_priority("SIG", 2, None));
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
