//! Routing intelligence used by the native router (`router`): congestion analysis, stagnation
//! detection, differential-pair awareness and per-net track width / via selection.
//!
//! Ported (re-written in Rust, no source copied) from the mental models of DESK
//! (`desk/routing/congestion.py`, `recovery.py`, `width.py`; MIT, Copyright (c) 2026 NijoP; licence
//! in `third_party/licenses/DESK/LICENSE`). Differences are deliberate:
//!
//! * congestion is measured on the router's own ownership grid (what is really blocked), not
//!   estimated from pad bounding boxes;
//! * widths come from `ipc` (IPC-2221, the model EAK's DRC also uses) and are always at least
//!   the net-class width; a net with no declared current keeps its class width and says so;
//! * everything here is a heuristic that steers the router. Nothing here certifies a layout:
//!   the router re-checks exact geometry and `eak-validation` verifies the result afterwards.

use crate::ipc;
use eak_core::{CopperLayer, Nm, Point, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

// ---------------------------------------------------------------- congestion

/// Extra A* step cost (a straight step costs 10) for entering a coarse cell at `utilization`
/// (0.0..=1.0). Non-linear like DESK's: free below 50 %, steep above 80 %, prohibitive at 95 %.
pub fn congestion_cost(utilization: f64) -> u8 {
    if utilization >= 0.95 {
        30
    } else if utilization >= 0.80 {
        (10.0 + 100.0 * (utilization - 0.80)).round() as u8
    } else if utilization >= 0.50 {
        (30.0 * (utilization - 0.50)).round() as u8
    } else {
        0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotspot {
    pub layer: String,
    pub center: Point,
    pub utilization_pct: f64,
}

/// Fraction of each coarse cell (`block` x `block` router cells) that is blocked, per copper layer.
#[derive(Debug, Clone)]
pub struct CongestionMap {
    block: usize,
    grid_cols: usize,
    grid_rows: usize,
    cols: usize,
    rows: usize,
    layers: Vec<CopperLayer>,
    util: Vec<Vec<f32>>,
}

/// Hotspot layer label: `top`, `bottom`, `in1` ...
fn layer_label(l: CopperLayer) -> String {
    match l {
        CopperLayer::Top => "top".into(),
        CopperLayer::Bottom => "bottom".into(),
        CopperLayer::Inner(n) => format!("in{n}"),
    }
}

impl CongestionMap {
    /// `occupied[l][r * grid_cols + c]` is true when router cell (c, r) of `layers[l]` is blocked.
    pub fn new(
        grid_cols: usize,
        grid_rows: usize,
        block: usize,
        occupied: &[&[bool]],
        layers: &[CopperLayer],
    ) -> CongestionMap {
        let block = block.max(1);
        let cols = grid_cols.div_ceil(block).max(1);
        let rows = grid_rows.div_ceil(block).max(1);
        let mut util = vec![vec![0f32; cols * rows]; occupied.len()];
        for (l, occ) in occupied.iter().enumerate() {
            let mut count = vec![0u32; cols * rows];
            let mut total = vec![0u32; cols * rows];
            for r in 0..grid_rows {
                for c in 0..grid_cols {
                    let k = (r / block) * cols + c / block;
                    total[k] += 1;
                    if occ[r * grid_cols + c] {
                        count[k] += 1;
                    }
                }
            }
            for k in 0..cols * rows {
                util[l][k] = if total[k] == 0 {
                    0.0
                } else {
                    count[k] as f32 / total[k] as f32
                };
            }
        }
        CongestionMap {
            block,
            grid_cols,
            grid_rows,
            cols,
            rows,
            layers: layers.to_vec(),
            util,
        }
    }

    /// Utilisation (0..=1) of the coarse cell holding router cell (c, r).
    pub fn utilization(&self, layer: usize, c: usize, r: usize) -> f64 {
        f64::from(self.util[layer][(r / self.block) * self.cols + c / self.block])
    }

    /// Per router cell extra step cost, every layer (`[layer][r * grid_cols + c]`).
    pub fn step_penalties(&self) -> Vec<Vec<u8>> {
        let mut out = vec![vec![0u8; self.grid_cols * self.grid_rows]; self.util.len()];
        for (l, plane) in out.iter_mut().enumerate() {
            for r in 0..self.grid_rows {
                for c in 0..self.grid_cols {
                    plane[r * self.grid_cols + c] = congestion_cost(self.utilization(l, c, r));
                }
            }
        }
        out
    }

    /// The `n` most utilised coarse cells (ties broken by position, so the list is stable).
    pub fn hotspots(&self, n: usize, origin: Point, pitch: Nm) -> Vec<Hotspot> {
        // (utilisation, layer, coarse column, coarse row)
        let mut all: Vec<(f32, usize, usize, usize)> = vec![];
        for l in 0..self.util.len() {
            for k in 0..self.cols * self.rows {
                if self.util[l][k] > 0.0 {
                    all.push((self.util[l][k], l, k % self.cols, k / self.cols));
                }
            }
        }
        all.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then(a.1.cmp(&b.1))
                .then(a.3.cmp(&b.3))
                .then(a.2.cmp(&b.2))
        });
        let span = (self.block as Nm) * pitch;
        all.into_iter()
            .take(n)
            .map(|(u, l, cx, cy)| Hotspot {
                layer: self
                    .layers
                    .get(l)
                    .map_or_else(|| format!("layer{l}"), |&x| layer_label(x)),
                center: Point::new(
                    origin.x + cx as Nm * span + span / 2,
                    origin.y + cy as Nm * span + span / 2,
                ),
                utilization_pct: (f64::from(u) * 1000.0).round() / 10.0,
            })
            .collect()
    }
}

// ---------------------------------------------------------------- stagnation

/// One routing pass as the stagnation detector sees it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PassRecord {
    pub unrouted: usize,
    pub wire_length_nm: i64,
    pub vias: usize,
}

/// DESK's stagnation rule: `patience` consecutive passes with the same non-zero unrouted count
/// and (almost) no change in wire length or via count mean more passes will not help.
#[derive(Debug, Clone)]
pub struct Stagnation {
    patience: usize,
    history: Vec<PassRecord>,
}

/// Wire length changes below this (0.1 mm) do not count as progress.
const LENGTH_EPS_NM: i64 = 100_000;

impl Stagnation {
    pub fn new(patience: usize) -> Stagnation {
        Stagnation {
            patience: patience.max(2),
            history: vec![],
        }
    }

    /// Record a pass; returns `true` while routing should continue, `false` once stagnated.
    pub fn record(&mut self, rec: PassRecord) -> bool {
        self.history.push(rec);
        if rec.unrouted == 0 || self.history.len() < self.patience {
            return true;
        }
        let recent = &self.history[self.history.len() - self.patience..];
        let first = recent[0];
        let flat = recent.iter().all(|r| {
            r.unrouted == first.unrouted
                && r.vias == first.vias
                && (r.wire_length_nm - first.wire_length_nm).abs() < LENGTH_EPS_NM
        });
        !flat
    }
}

// ---------------------------------------------------------------- differential pairs

const PAIR_SUFFIXES: [(&str, &str); 7] = [
    ("_P", "_N"),
    ("_DP", "_DN"),
    ("_DP", "_DM"),
    ("D+", "D-"),
    ("+", "-"),
    (".P", ".N"),
    ("_H", "_L"),
];

/// Differential pairs among `nets`: names that differ only by a conventional polarity suffix
/// (`USB_DP`/`USB_DN`, `CLK_P`/`CLK_N`, `D+`/`D-`, ...) **and** both exist. Returned as
/// `(positive, negative)`, sorted; a net belongs to at most one pair. Name-based: a declared
/// pair in the project would take precedence once EAK-IR has one.
pub fn diff_pairs(nets: &BTreeSet<String>) -> Vec<(String, String)> {
    let mut used: BTreeSet<&str> = BTreeSet::new();
    let mut out = vec![];
    for n in nets {
        if used.contains(n.as_str()) {
            continue;
        }
        let up = n.to_ascii_uppercase();
        for (p, m) in PAIR_SUFFIXES {
            let Some(base) = up.strip_suffix(p) else {
                continue;
            };
            let want = format!("{base}{m}");
            let partner = nets
                .iter()
                .find(|o| o.to_ascii_uppercase() == want && !used.contains(o.as_str()));
            if let Some(o) = partner {
                used.insert(n.as_str());
                used.insert(o.as_str());
                out.push((n.clone(), o.clone()));
                break;
            }
        }
    }
    out.sort();
    out
}

/// The partner of `net` in `pairs`, if any.
pub fn partner_of<'a>(net: &str, pairs: &'a [(String, String)]) -> Option<&'a str> {
    pairs.iter().find_map(|(a, b)| {
        if a == net {
            Some(b.as_str())
        } else if b == net {
            Some(a.as_str())
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------- width and via selection

/// Plating thickness assumed for via barrel current (25 um, standard Class 2 minimum average).
const VIA_PLATING_MIL: f64 = 0.984;

/// Track width and via for one net, with the reasons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetSpec {
    pub net: String,
    pub class: String,
    /// Track width on the outer layers.
    pub track_width: Nm,
    /// Track width on the inner signal layers (IPC-2221 internal; equal to `track_width` without
    /// a declared current or without inner signal layers).
    #[serde(default)]
    pub inner_track_width: Nm,
    pub clearance: Nm,
    pub via_drill: Nm,
    pub via_diameter: Nm,
    /// Declared continuous current (A), if any.
    pub current_a: Option<f64>,
    /// Single-via current capacity (A) at the chosen drill, when a current is declared.
    pub via_current_a: Option<f64>,
    /// Parallel vias a layer change needs for the declared current (1 when it fits one via).
    pub vias_needed: u32,
    pub notes: Vec<String>,
}

fn nm_ceil(mm: f64) -> Nm {
    (mm * 1_000_000.0).ceil() as Nm
}

/// Track width and via size for `net`.
///
/// * width = max(net class width, board minimum, IPC-2221 width for the declared current);
/// * via = the class via, with the diameter raised to keep the minimum annular ring;
/// * a net with no declared current keeps its class width and the note says the width is not
///   current-sized (it never invents a current).
pub fn net_spec(p: &Project, net: &str) -> NetSpec {
    let c = &p.constraints;
    let class = c.class_for(net);
    let mut notes = vec![];
    let class_width = class.track_width.max(c.min_track_width);
    let current = c.net_currents_a.get(net).copied().filter(|a| *a > 0.0);
    let stack = &p.pcb.board.stackup;
    // The thinnest copper of each group needs the widest track.
    let thinnest = |outer: bool| {
        stack
            .layers
            .iter()
            .filter(|l| l.layer.is_outer() == outer && l.kind == eak_core::LayerKind::Signal)
            .map(|l| l.copper_oz)
            .reduce(f64::min)
    };
    let mut sized = |internal: bool, oz: Option<f64>| -> Nm {
        let (Some(a), Some(oz)) = (current, oz) else {
            return class_width;
        };
        let kind = if internal { "internal" } else { "external" };
        match ipc::trace_width_mm(a, internal, oz, c.temp_rise_c) {
            Ok(mm) => {
                let need = nm_ceil(mm);
                if need > class_width {
                    notes.push(format!(
                        "{kind} width raised {:.3} -> {:.3} mm for {a} A (IPC-2221 {kind}, {oz} oz, {} C rise)",
                        class_width as f64 / 1e6,
                        need as f64 / 1e6,
                        c.temp_rise_c
                    ));
                }
                need.max(class_width)
            }
            Err(e) => {
                notes.push(format!("{kind} width not current-sized: {e}"));
                class_width
            }
        }
    };
    let width = sized(false, thinnest(true).or(Some(1.0)));
    let inner_width = match thinnest(false) {
        Some(oz) => sized(true, Some(oz)),
        None => width,
    };
    if current.is_none() {
        notes.push(
            "no declared current: class width kept (not current-sized; declare net_currents_a to size it)".into(),
        );
    }
    let drill = class.via_drill.max(c.min_drill);
    let diameter = class.via_diameter.max(drill + 2 * c.min_annular_ring);
    if diameter > class.via_diameter {
        notes.push(format!(
            "via diameter raised to {:.3} mm for the {:.3} mm minimum annular ring",
            diameter as f64 / 1e6,
            c.min_annular_ring as f64 / 1e6
        ));
    }
    let (via_i, needed) = match current {
        Some(a) => {
            let drill_mil = drill as f64 / 1e6 * ipc::MIL_PER_MM;
            match ipc::via_current_a(drill_mil, VIA_PLATING_MIL, c.temp_rise_c) {
                Ok(i) => {
                    let n = ipc::vias_required(a, drill_mil, VIA_PLATING_MIL, c.temp_rise_c, 0.8)
                        .unwrap_or(1);
                    if n > 1 {
                        notes.push(format!(
                            "one via carries about {i:.2} A; {a} A needs {n} parallel vias at each layer change (the router places one: review or add vias)"
                        ));
                    }
                    (Some(i), n)
                }
                Err(_) => (None, 1),
            }
        }
        None => (None, 1),
    };
    NetSpec {
        net: net.into(),
        class: class.name.clone(),
        track_width: width,
        inner_track_width: inner_width,
        clearance: class.clearance.max(c.min_clearance),
        via_drill: drill,
        via_diameter: diameter,
        current_a: current,
        via_current_a: via_i,
        vias_needed: needed,
        notes,
    }
}

// Written without running (no local builds/tests in this phase): unrun.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn congestion_curve_is_monotonic() {
        let mut last = 0;
        for i in 0..=100 {
            let c = congestion_cost(f64::from(i) / 100.0);
            assert!(c >= last, "cost fell at {i} %");
            last = c;
        }
        assert_eq!(congestion_cost(0.2), 0);
        assert!(congestion_cost(0.99) >= 30);
    }

    #[test]
    fn hotspot_is_the_blocked_cell() {
        let blocked: Vec<bool> = (0..16).map(|i| i % 4 < 2 && i / 4 < 2).collect();
        let free = vec![false; 16];
        let m = CongestionMap::new(4, 4, 2, &[&blocked, &free], &CopperLayer::OUTER);
        assert!((m.utilization(0, 0, 0) - 1.0).abs() < 1e-6);
        assert!(m.utilization(0, 3, 3).abs() < 1e-6);
        let h = m.hotspots(1, Point::new(0, 0), 1_000);
        assert_eq!(h[0].layer, "top");
        assert!((h[0].utilization_pct - 100.0).abs() < 1e-6);
        assert_eq!(m.step_penalties()[0][0], congestion_cost(1.0));
    }

    #[test]
    fn stagnation_needs_patience_flat_passes() {
        let mut s = Stagnation::new(2);
        let rec = |u, w| PassRecord {
            unrouted: u,
            wire_length_nm: w,
            vias: 3,
        };
        assert!(s.record(rec(4, 1_000_000)));
        assert!(s.record(rec(3, 1_500_000))); // progress
        assert!(!s.record(rec(3, 1_500_000))); // flat twice
        let mut done = Stagnation::new(2);
        assert!(done.record(rec(0, 1)) && done.record(rec(0, 1))); // nothing left is not stagnation
    }

    #[test]
    fn pairs_need_both_nets() {
        let nets: BTreeSet<String> = ["USB_DP", "USB_DN", "CLK_P", "GND", "D+", "D-"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let pairs = diff_pairs(&nets);
        assert!(pairs.contains(&("USB_DP".into(), "USB_DN".into())));
        assert!(pairs.contains(&("D+".into(), "D-".into())));
        assert!(!pairs.iter().any(|(a, _)| a == "CLK_P"));
        assert_eq!(partner_of("USB_DN", &pairs), Some("USB_DP"));
    }

    #[test]
    fn inner_signal_layers_get_the_ipc_internal_width_of_their_copper() {
        let mut p = Project::new("w4");
        // 4 layers, 1 oz outside, 0.5 oz inside; In2 made a signal layer (In1 stays a plane).
        let mut st = eak_core::Stackup::multilayer(4, 1_600_000, 1.0, 0.5, "FR-4", 4.5);
        st.layers[2].kind = eak_core::LayerKind::Signal;
        p.pcb.board.stackup = st;
        p.constraints.net_currents_a.insert("VBUS".into(), 3.0);
        let s = net_spec(&p, "VBUS");
        let t = p.constraints.temp_rise_c;
        let outer = nm_ceil(ipc::trace_width_mm(3.0, false, 1.0, t).unwrap());
        let inner = nm_ceil(ipc::trace_width_mm(3.0, true, 0.5, t).unwrap());
        assert_eq!(s.track_width, outer);
        assert_eq!(s.inner_track_width, inner);
        assert!(
            s.inner_track_width > s.track_width,
            "internal, thinner copper: wider"
        );
        assert!(s
            .notes
            .iter()
            .any(|n| n.starts_with("internal width raised")));
    }

    #[test]
    fn current_widens_the_track_and_flags_vias() {
        let mut p = Project::new("w");
        p.constraints.net_currents_a.insert("VBUS".into(), 3.0);
        let s = net_spec(&p, "VBUS");
        assert!(s.track_width > p.constraints.default_class.track_width);
        assert!(s.notes.iter().any(|n| n.contains("raised")));
        // Two layers: no inner signal layer, the inner width is the outer one.
        assert_eq!(s.inner_track_width, s.track_width);
        let plain = net_spec(&p, "SIG");
        assert_eq!(plain.track_width, p.constraints.default_class.track_width);
        assert_eq!(plain.inner_track_width, plain.track_width);
        assert!(plain
            .notes
            .iter()
            .any(|n| n.contains("no declared current")));
    }
}
