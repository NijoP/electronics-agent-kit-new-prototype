//! Native two-layer grid router (EAK-owned; no Freerouting dependency).
//!
//! Algorithm: each layer is rasterised into an ownership grid where a cell records which
//! net's copper (inflated by clearance + half track width + a discretisation margin)
//! covers it. A* with 8-way moves and layer changes (vias) then connects each net's
//! pads along a minimum spanning tree. Every produced segment is re-checked with exact
//! geometry before it is accepted; failures are reported, never hidden.
//!
//! Routing intelligence on top of that (`routing_intel`, DESK-derived mental models):
//!
//! * connections are routed most-constrained first (`priority`), differential-pair partners
//!   back to back. A declared pair (`Constraints::diff_pairs`, ADR-0055) with a known gap is
//!   routed coupled (`diffpair`: one centreline, members offset at the gap, spread vias,
//!   serpentine length matching within `max_skew`); a name-detected pair only prefers to stay
//!   near its partner, and the report says spacing and length are not controlled;
//! * step costs include a congestion penalty measured on the ownership grid;
//! * routing is pass-based: a pass that leaves connections unrouted promotes them and routes
//!   everything again; the best pass wins; passes stop on stagnation (no change in unrouted
//!   count, wire length or vias) or `max_passes`;
//! * inside a pass, a connection that finds no path may rip up the minimal set of
//!   *lower-priority copper this run placed* that blocks it, route itself, then re-route the
//!   victims; if that fails the victims are restored. Pre-existing copper is never touched;
//! * track width comes from the net class and declared current (IPC-2221), the via from the
//!   class (annular-ring safe); a wide track that cannot clear a fine-pitch pad necks down to the
//!   class width rather than failing, and says so.
//!
//! The router is a heuristic. It reports a connection as routed only after exact geometric
//! clearance against all other-net copper; whether the resulting board is electrically or
//! manufacturing correct is decided afterwards by `eak-validation` (DRC, connectivity), never here.

use crate::copper::{distance, items, CopperItem, Core, ItemKind, Shape};
use crate::routing_intel::{
    self as intel, CongestionMap, Hotspot, NetSpec, PassRecord, Stagnation,
};
use eak_core::geom::{point_in_polygon, point_segment_distance, segment_segment_distance};
use eak_core::{CopperLayer, Nm, Point, Project, Rect};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

mod diffpair;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutedTrack {
    pub layer: CopperLayer,
    pub a: Point,
    pub b: Point,
    pub width: Nm,
    pub net: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutedVia {
    pub position: Point,
    pub drill: Nm,
    pub diameter: Nm,
    pub net: String,
}

/// What one routing pass achieved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassSummary {
    pub pass: usize,
    pub routed: usize,
    pub unrouted: usize,
    /// Routes ripped up (and re-routed or restored) during the pass.
    pub ripups: usize,
    pub vias: usize,
    pub wire_length_mm: f64,
}

/// Routed state of one differential pair: a declared one (`Constraints::diff_pairs`) or one
/// detected by name (`routing_intel`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairReport {
    pub positive: String,
    pub negative: String,
    pub length_positive_mm: f64,
    pub length_negative_mm: f64,
    pub skew_mm: f64,
    pub vias_positive: usize,
    pub vias_negative: usize,
    pub fully_routed: bool,
    pub note: String,
    /// Declared in the project (only declared pairs are routed coupled).
    #[serde(default)]
    pub declared: bool,
    /// Every connection of both members was routed coupled at `gap_mm`.
    #[serde(default)]
    pub coupled: bool,
    #[serde(default)]
    pub width_mm: Option<f64>,
    #[serde(default)]
    pub gap_mm: Option<f64>,
    #[serde(default)]
    pub max_skew_mm: Option<f64>,
    /// `None` when no skew limit is declared or the pair is not fully routed (UNKNOWN).
    #[serde(default)]
    pub skew_within_tolerance: Option<bool>,
    /// Differential impedance estimate per layer the pair uses (IPC-2141A, or UNKNOWN).
    #[serde(default)]
    pub impedance: Vec<LayerImpedance>,
    /// `None` when the target or tolerance is undeclared or an estimate is UNKNOWN.
    #[serde(default)]
    pub impedance_within_tolerance: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerImpedance {
    pub layer: String,
    pub zdiff: crate::impedance::Estimate,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteResult {
    pub tracks: Vec<RoutedTrack>,
    pub vias: Vec<RoutedVia>,
    pub routed_connections: usize,
    pub failed_connections: Vec<(String, Point, Point, String)>,
    pub grid_pitch: Nm,
    #[serde(default)]
    pub passes: Vec<PassSummary>,
    /// True when passes stopped because more passes would not have changed anything.
    #[serde(default)]
    pub stagnated: bool,
    /// Routes ripped up over all passes (of the winning and discarded passes).
    #[serde(default)]
    pub ripups: usize,
    /// Most utilised coarse cells of the final result (router-grid blockage, heuristic).
    #[serde(default)]
    pub congestion: Vec<Hotspot>,
    #[serde(default)]
    pub pairs: Vec<PairReport>,
    /// Width / via / neckdown decisions that differ from the plain net class.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Pre-existing tracks pushed aside (push-and-shove): delete these ids; their new
    /// polylines are among `tracks`.
    #[serde(default)]
    pub replaced_tracks: Vec<String>,
}

const FREE: u32 = 0;
const MULTI: u32 = u32::MAX;
/// Extra step cost for leaving a differential-pair partner's neighbourhood.
const AFFINITY_PENALTY: u64 = 4;

#[derive(Clone)]
struct Grid {
    origin: Point,
    pitch: Nm,
    cols: usize,
    rows: usize,
    owner: [Vec<u32>; 2],
}

impl Grid {
    fn idx(&self, c: usize, r: usize) -> usize {
        r * self.cols + c
    }
    fn center(&self, c: usize, r: usize) -> Point {
        Point::new(
            self.origin.x + c as Nm * self.pitch,
            self.origin.y + r as Nm * self.pitch,
        )
    }
    fn cell_of(&self, p: Point) -> (usize, usize) {
        let c = ((p.x - self.origin.x) as f64 / self.pitch as f64)
            .round()
            .clamp(0.0, (self.cols - 1) as f64) as usize;
        let r = ((p.y - self.origin.y) as f64 / self.pitch as f64)
            .round()
            .clamp(0.0, (self.rows - 1) as f64) as usize;
        (c, r)
    }
    /// Same geometry, nothing owned.
    fn blank(&self) -> Grid {
        let n = self.cols * self.rows;
        Grid {
            origin: self.origin,
            pitch: self.pitch,
            cols: self.cols,
            rows: self.rows,
            owner: [vec![FREE; n], vec![FREE; n]],
        }
    }
    fn mark(&mut self, layer: usize, shape: &Shape, inflate: f64, net: u32) {
        let s = Shape {
            core: shape.core.clone(),
            radius: shape.radius + inflate,
        };
        let bb = s.bbox();
        let c0 = ((bb.min.x - self.origin.x) / self.pitch - 1).max(0) as usize;
        let r0 = ((bb.min.y - self.origin.y) / self.pitch - 1).max(0) as usize;
        let c1 = (((bb.max.x - self.origin.x) / self.pitch + 2).max(0) as usize).min(self.cols);
        let r1 = (((bb.max.y - self.origin.y) / self.pitch + 2).max(0) as usize).min(self.rows);
        for r in r0..r1 {
            for c in c0..c1 {
                if s.contains(self.center(c, r)) {
                    let i = self.idx(c, r);
                    let o = &mut self.owner[layer][i];
                    *o = if *o == FREE || *o == net { net } else { MULTI };
                }
            }
        }
    }
    fn passable(&self, layer: usize, i: usize, net: u32) -> bool {
        let o = self.owner[layer][i];
        o == FREE || o == net
    }
}

/// Router grid layer for a copper layer; the autorouter works on the two outer layers only,
/// so inner-layer copper is not part of its grid (`None`).
fn layer_index(l: CopperLayer) -> Option<usize> {
    match l {
        CopperLayer::Top => Some(0),
        CopperLayer::Bottom => Some(1),
        CopperLayer::Inner(_) => None,
    }
}

fn layer_of(i: usize) -> CopperLayer {
    if i == 0 {
        CopperLayer::Top
    } else {
        CopperLayer::Bottom
    }
}

pub struct RouterOptions {
    pub pitch: Nm,
    /// Only route these nets (None = all nets with unrouted connections).
    pub nets: Option<Vec<String>>,
    pub via_cost: u64,
    pub max_expansions: usize,
    /// Upper bound on routing passes (a pass routes every connection once).
    pub max_passes: usize,
    /// Consecutive unchanged passes that count as stagnation (at least 2).
    pub patience: usize,
    /// Upper bound on routes ripped up per pass.
    pub max_ripups: usize,
    /// Most routes a single rip-up may remove; a connection needing more stays unrouted.
    pub max_victims: usize,
}

impl Default for RouterOptions {
    fn default() -> Self {
        RouterOptions {
            pitch: 250_000,
            nets: None,
            via_cost: 40,
            max_expansions: 2_000_000,
            max_passes: 3,
            patience: 2,
            max_ripups: 24,
            max_victims: 4,
        }
    }
}

/// One unrouted pad-to-pad connection.
#[derive(Clone)]
struct Conn {
    net: String,
    a: Point,
    b: Point,
}

/// A route this run has placed for one connection.
#[derive(Clone)]
struct Placed {
    conn: usize,
    net: String,
    id: u32,
    width: Nm,
    via_drill: Nm,
    via_diameter: Nm,
    segs: Vec<(usize, Point, Point)>,
    vias: Vec<Point>,
    shapes: Vec<(usize, Shape)>,
    note: Option<String>,
    /// Placed by the coupled pair router; never ripped up alone (its partner would lose it).
    pair: bool,
    /// The pre-existing track this copper replaces (moved aside by a shove; `conn` is
    /// `usize::MAX`). Never a rip-up victim.
    moved: Option<String>,
}

struct PassOutcome {
    placed: Vec<Placed>,
    failed: BTreeMap<usize, String>,
    ripups: usize,
    wire_nm: i64,
    vias: usize,
    /// Why a declared pair connection was not routed coupled, or how it was.
    pair_notes: BTreeMap<usize, String>,
}

/// Everything a connection attempt needs that does not change during a run.
struct Env<'a> {
    p: &'a Project,
    opts: &'a RouterOptions,
    copper: &'a [CopperItem],
    net_ids: &'a BTreeMap<String, u32>,
    specs: &'a BTreeMap<String, NetSpec>,
    pairs: &'a [(String, String)],
    base: &'a Grid,
    /// Cells outside the board or inside the edge keep-out (not "congestion").
    outside: &'a [bool],
    inflate: f64,
    /// Declared pairs by positive net: routable coupled, or why not.
    coupled: &'a BTreeMap<String, Result<diffpair::Coupled, String>>,
    /// For each connection, the partner connection it is routed coupled with (declared pairs).
    mates: &'a [Option<usize>],
    /// Routing priority of every net (`priority::net_priority`); a shove moves only
    /// pre-existing tracks of lower-priority nets.
    priority: &'a BTreeMap<String, u32>,
}

/// Per-cell extra step costs for one attempt.
struct Costs<'a> {
    congestion: &'a [Vec<u8>; 2],
    affinity: Option<&'a [Vec<bool>; 2]>,
}

impl Costs<'_> {
    fn extra(&self, layer: usize, i: usize) -> u64 {
        let mut e = u64::from(self.congestion[layer][i]);
        if let Some(a) = self.affinity {
            if !a[layer][i] {
                e += AFFINITY_PENALTY;
            }
        }
        e
    }
}

pub fn route(p: &Project, opts: &RouterOptions) -> RouteResult {
    let mut result = RouteResult {
        grid_pitch: opts.pitch,
        ..Default::default()
    };
    let outline = &p.pcb.board.outline;
    let Some(bb) = Rect::from_points(outline) else {
        result.failed_connections.push((
            String::new(),
            Point::default(),
            Point::default(),
            "board outline missing".into(),
        ));
        return result;
    };
    let pitch = opts.pitch;
    let mut base = Grid {
        origin: bb.min,
        pitch,
        cols: (bb.width() / pitch) as usize + 1,
        rows: (bb.height() / pitch) as usize + 1,
        owner: [vec![], vec![]],
    };
    let n = base.cols * base.rows;
    base.owner = [vec![FREE; n], vec![FREE; n]];

    // Net ids.
    let copper = items(p);
    let mut net_ids: BTreeMap<String, u32> = BTreeMap::new();
    for it in &copper {
        if !it.net.is_empty() {
            let next = net_ids.len() as u32 + 1;
            net_ids.entry(it.net.clone()).or_insert(next);
        }
    }
    let terminals = crate::priority::terminal_counts(p);
    let priority: BTreeMap<String, u32> = net_ids
        .keys()
        .map(|net| {
            let n = terminals.get(net).copied().unwrap_or(2);
            let amps = p.constraints.net_currents_a.get(net).copied();
            (net.clone(), crate::priority::net_priority(net, n, amps))
        })
        .collect();
    // Per-net width / via from class and declared current.
    let specs: BTreeMap<String, NetSpec> = net_ids
        .keys()
        .map(|net| (net.clone(), intel::net_spec(p, net)))
        .collect();
    let max_width = specs
        .values()
        .map(|s| s.track_width)
        .chain(std::iter::once(p.constraints.default_class.track_width))
        .max()
        .unwrap_or(250_000);
    let max_clear = specs
        .values()
        .map(|s| s.clearance)
        .chain(std::iter::once(p.constraints.default_class.clearance))
        .max()
        .unwrap_or(200_000)
        .max(p.constraints.min_clearance);
    let margin = pitch as f64 * 0.5;
    let inflate = max_clear as f64 + max_width as f64 / 2.0 + margin;

    // Board edge keep-out.
    let edge = p.constraints.board_edge_clearance as f64 + max_width as f64 / 2.0;
    let mut outside = vec![false; n];
    for r in 0..base.rows {
        for c in 0..base.cols {
            let pt = base.center(c, r);
            let mut d = f64::INFINITY;
            for i in 0..outline.len() {
                d = d.min(point_segment_distance(
                    pt,
                    outline[i],
                    outline[(i + 1) % outline.len()],
                ));
            }
            if !point_in_polygon(pt, outline) || d < edge {
                let i = base.idx(c, r);
                base.owner[0][i] = MULTI;
                base.owner[1][i] = MULTI;
                outside[i] = true;
            }
        }
    }
    // Existing copper. Unassigned-net pads block everyone.
    for it in &copper {
        let id = net_ids.get(&it.net).copied().unwrap_or(MULTI);
        if let Some(l) = layer_index(it.layer) {
            base.mark(l, &it.shape, inflate, id);
        }
    }

    // Unrouted connections per net, most constrained first, pair partners back to back.
    let conn = crate::copper::analyse(p);
    let mut todo: Vec<(String, Point, Point)> = conn.ratsnest.clone();
    if let Some(only) = &opts.nets {
        todo.retain(|(n, _, _)| only.contains(n));
    }
    crate::priority::order_connections(p, &mut todo);
    let todo_nets: BTreeSet<String> = todo.iter().map(|(n, _, _)| n.clone()).collect();
    // Declared pairs come first; name detection only pairs nets no declared pair names.
    let declared: Vec<&eak_core::DiffPairRule> = p
        .constraints
        .diff_pairs
        .iter()
        .filter(|d| todo_nets.contains(&d.positive) && todo_nets.contains(&d.negative))
        .collect();
    let undeclared: BTreeSet<String> = todo_nets
        .iter()
        .filter(|n| !p.constraints.diff_pairs.iter().any(|d| d.has(n)))
        .cloned()
        .collect();
    let mut pairs: Vec<(String, String)> = declared
        .iter()
        .map(|d| (d.positive.clone(), d.negative.clone()))
        .collect();
    pairs.extend(intel::diff_pairs(&undeclared));
    let coupled: BTreeMap<String, Result<diffpair::Coupled, String>> = declared
        .iter()
        .map(|d| (d.positive.clone(), diffpair::resolve(p, d)))
        .collect();
    let conns = group_pairs(
        todo.into_iter()
            .map(|(net, a, b)| Conn { net, a, b })
            .collect(),
        &pairs,
    );
    if conns.is_empty() {
        return result;
    }
    // A routable declared pair's connections were put back to back by `group_pairs`.
    let mut mates: Vec<Option<usize>> = vec![None; conns.len()];
    for i in 1..conns.len() {
        let (x, y) = (&conns[i - 1].net, &conns[i].net);
        let routable = |pos: &String| coupled.get(pos).is_some_and(|r| r.is_ok());
        let is_pair = (routable(x) && intel::partner_of(x, &pairs) == Some(y.as_str()))
            || (routable(y) && intel::partner_of(y, &pairs) == Some(x.as_str()));
        if is_pair && mates[i - 1].is_none() {
            mates[i - 1] = Some(i);
            mates[i] = Some(i - 1);
        }
    }

    let env = Env {
        p,
        opts,
        copper: &copper,
        net_ids: &net_ids,
        specs: &specs,
        pairs: &pairs,
        base: &base,
        outside: &outside,
        inflate,
        coupled: &coupled,
        mates: &mates,
        priority: &priority,
    };

    // Pass-based routing: failed connections are promoted and everything is routed again; the
    // best pass is kept; stagnation (no change) stops early.
    let mut key: Vec<i64> = (0..conns.len() as i64).collect();
    let mut best: Option<PassOutcome> = None;
    let mut stagnation = Stagnation::new(opts.patience);
    for pass in 1..=opts.max_passes.max(1) {
        let out = run_pass(&env, &conns, &key);
        result.ripups += out.ripups;
        result.passes.push(PassSummary {
            pass,
            routed: out.placed.iter().filter(|pl| pl.moved.is_none()).count(),
            unrouted: out.failed.len(),
            ripups: out.ripups,
            vias: out.vias,
            wire_length_mm: (out.wire_nm as f64 / 1e3).round() / 1e3,
        });
        let unrouted = out.failed.len();
        let keep_going = stagnation.record(PassRecord {
            unrouted,
            wire_length_nm: out.wire_nm,
            vias: out.vias,
        });
        for ci in out.failed.keys() {
            key[*ci] -= conns.len() as i64;
        }
        let better = match &best {
            None => true,
            Some(b) => (unrouted, out.vias, out.wire_nm) < (b.failed.len(), b.vias, b.wire_nm),
        };
        if better {
            best = Some(out);
        }
        if unrouted == 0 {
            break;
        }
        if !keep_going {
            result.stagnated = true;
            break;
        }
    }
    let Some(mut best) = best else {
        return result;
    };
    best.placed.sort_by_key(|pl| pl.conn);

    // Report the winning pass.
    let mut notes: BTreeSet<String> = BTreeSet::new();
    for pl in &best.placed {
        for (l, s, e) in &pl.segs {
            result.tracks.push(RoutedTrack {
                layer: layer_of(*l),
                a: *s,
                b: *e,
                width: pl.width,
                net: pl.net.clone(),
            });
        }
        for v in &pl.vias {
            result.vias.push(RoutedVia {
                position: *v,
                drill: pl.via_drill,
                diameter: pl.via_diameter,
                net: pl.net.clone(),
            });
        }
        if let Some(s) = specs.get(&pl.net) {
            for note in &s.notes {
                if !note.starts_with("no declared current") {
                    notes.insert(format!("{}: {note}", pl.net));
                }
            }
        }
        if let Some(note) = &pl.note {
            notes.insert(format!("{}: {note}", pl.net));
        }
    }
    result.routed_connections = best.placed.iter().filter(|pl| pl.moved.is_none()).count();
    result.replaced_tracks = moved_ids(best.placed.iter()).into_iter().collect();
    for (ci, why) in &best.failed {
        let c = &conns[*ci];
        result
            .failed_connections
            .push((c.net.clone(), c.a, c.b, why.clone()));
    }
    result.notes = notes.into_iter().collect();
    result.pairs = pair_reports(&env, &pairs, &best, &conns);
    let final_grid = grid_with(&env, best.placed.iter());
    let cong = congestion_map(&env, &final_grid);
    result.congestion = cong.hotspots(5, base.origin, pitch);
    result
}

/// Put each pair partner's connection right after the matching connection of its mate.
fn group_pairs(conns: Vec<Conn>, pairs: &[(String, String)]) -> Vec<Conn> {
    let gap = |x: &Conn, y: &Conn| -> f64 {
        (x.a.dist(y.a) + x.b.dist(y.b)).min(x.a.dist(y.b) + x.b.dist(y.a))
    };
    let mut taken = vec![false; conns.len()];
    let mut out = Vec::with_capacity(conns.len());
    for i in 0..conns.len() {
        if taken[i] {
            continue;
        }
        taken[i] = true;
        out.push(conns[i].clone());
        if let Some(partner) = intel::partner_of(&conns[i].net, pairs) {
            let mate = (i + 1..conns.len())
                .filter(|&j| !taken[j] && conns[j].net == partner)
                .min_by(|&x, &y| gap(&conns[i], &conns[x]).total_cmp(&gap(&conns[i], &conns[y])));
            if let Some(j) = mate {
                taken[j] = true;
                out.push(conns[j].clone());
            }
        }
    }
    out
}

fn pair_reports(
    env: &Env,
    pairs: &[(String, String)],
    out: &PassOutcome,
    conns: &[Conn],
) -> Vec<PairReport> {
    let (placed, failed) = (&out.placed, &out.failed);
    let length_nm = |net: &str| -> f64 {
        placed
            .iter()
            .filter(|pl| pl.net == net)
            .flat_map(|pl| pl.segs.iter())
            .map(|(_, a, b)| a.dist(*b))
            .sum::<f64>()
    };
    let length_mm = |net: &str| length_nm(net) / 1e6;
    let vias = |net: &str| -> usize {
        placed
            .iter()
            .filter(|pl| pl.net == net)
            .map(|pl| pl.vias.len())
            .sum()
    };
    let complete = |net: &str| -> bool {
        conns.iter().any(|c| c.net == net) && !failed.keys().any(|ci| conns[*ci].net == net)
    };
    pairs
        .iter()
        .filter(|(a, b)| conns.iter().any(|c| &c.net == a || &c.net == b))
        .map(|(a, b)| {
            let (la, lb) = (length_mm(a), length_mm(b));
            let fully_routed = complete(a) && complete(b);
            let mut r = PairReport {
                positive: a.clone(),
                negative: b.clone(),
                length_positive_mm: (la * 1e3).round() / 1e3,
                length_negative_mm: (lb * 1e3).round() / 1e3,
                skew_mm: ((la - lb).abs() * 1e3).round() / 1e3,
                vias_positive: vias(a),
                vias_negative: vias(b),
                fully_routed,
                note: "routed adjacent (the second net prefers the first one's neighbourhood); spacing, coupled impedance and length matching are NOT controlled or verified".into(),
                declared: false,
                coupled: false,
                width_mm: None,
                gap_mm: None,
                max_skew_mm: None,
                skew_within_tolerance: None,
                impedance: vec![],
                impedance_within_tolerance: None,
            };
            let Some(rule) = env.coupled.get(a) else {
                return r;
            };
            r.declared = true;
            let rule = match rule {
                Ok(rule) => rule,
                Err(why) => {
                    r.note = format!("declared pair NOT routed coupled: {why}; routed as two nets, so spacing, impedance and length matching are not controlled");
                    return r;
                }
            };
            let members: Vec<&Placed> =
                placed.iter().filter(|pl| &pl.net == a || &pl.net == b).collect();
            r.coupled = fully_routed && !members.is_empty() && members.iter().all(|pl| pl.pair);
            r.width_mm = Some(rule.width as f64 / 1e6);
            r.gap_mm = Some(rule.gap as f64 / 1e6);
            r.max_skew_mm = rule.max_skew.map(|s| s as f64 / 1e6);
            if fully_routed {
                r.skew_within_tolerance = rule
                    .max_skew
                    .map(|s| (length_nm(a) - length_nm(b)).abs() <= s as f64 + 1.0);
            }
            if r.coupled {
                let layers: BTreeSet<usize> = members
                    .iter()
                    .flat_map(|pl| pl.segs.iter().map(|(l, _, _)| *l))
                    .collect();
                let (est, ok) = diffpair::impedance_on(env.p, rule, &layers);
                r.impedance = est
                    .into_iter()
                    .map(|(layer, zdiff)| LayerImpedance { layer, zdiff })
                    .collect();
                r.impedance_within_tolerance = ok;
            }
            let mut notes = rule.notes.clone();
            notes.extend(
                conns
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| &c.net == a)
                    .filter_map(|(ci, _)| out.pair_notes.get(&ci).cloned()),
            );
            if rule.max_skew.is_none() {
                notes.push("max_skew undeclared: length matching UNKNOWN".into());
            }
            if !r.coupled {
                notes.push("not every connection could be routed coupled: those parts are routed as single nets".into());
            }
            r.note = notes.join("; ");
            r
        })
        .collect()
}

/// Layers an SMD/through pad at `pt` on `net` is available on (both outer layers when unknown).
fn pad_layers(copper: &[CopperItem], pt: Point, net: &str) -> Vec<usize> {
    let mut ls: Vec<usize> = copper
        .iter()
        .filter(|i| i.kind == ItemKind::Pad && i.net == net && i.shape.contains(pt))
        .filter_map(|i| layer_index(i.layer))
        .collect();
    ls.sort();
    ls.dedup();
    if ls.is_empty() {
        vec![0, 1]
    } else {
        ls
    }
}

fn mark_placed(grid: &mut Grid, pl: &Placed, inflate: f64) {
    for (l, s) in &pl.shapes {
        grid.mark(*l, s, inflate, pl.id);
    }
}

/// The base grid plus the given placed routes (rebuilt without the pre-existing tracks a shove
/// moved, when there are any).
fn grid_with<'a>(env: &Env, placed: impl Iterator<Item = &'a Placed>) -> Grid {
    let placed: Vec<&Placed> = placed.collect();
    let hidden = moved_ids(placed.iter().copied());
    if !hidden.is_empty() {
        return fresh_grid(env, &placed, &hidden, |id| id);
    }
    let mut g = env.base.clone();
    for pl in placed {
        mark_placed(&mut g, pl, env.inflate);
    }
    g
}

/// Ids of the pre-existing tracks the given placed copper replaces.
fn moved_ids<'a>(placed: impl Iterator<Item = &'a Placed>) -> BTreeSet<String> {
    placed.filter_map(|pl| pl.moved.clone()).collect()
}

/// Whether copper item `it` is (part of) one of the `hidden` pre-existing tracks.
fn is_hidden(it: &CopperItem, hidden: &BTreeSet<String>) -> bool {
    it.kind == ItemKind::Track && hidden.contains(&it.object_id)
}

/// The board grid built from scratch: outside cells, every pre-existing copper item except the
/// `hidden` tracks, and `placed`; `remap` renames net ids (the pair router merges two nets).
fn fresh_grid(
    env: &Env,
    placed: &[&Placed],
    hidden: &BTreeSet<String>,
    remap: impl Fn(u32) -> u32,
) -> Grid {
    let mut g = env.base.blank();
    for (i, &out) in env.outside.iter().enumerate() {
        if out {
            for o in g.owner.iter_mut() {
                o[i] = MULTI;
            }
        }
    }
    for it in env.copper.iter().filter(|it| !is_hidden(it, hidden)) {
        let id = env.net_ids.get(&it.net).copied().unwrap_or(MULTI);
        if let Some(l) = layer_index(it.layer) {
            g.mark(l, &it.shape, env.inflate, remap(id));
        }
    }
    for pl in placed {
        for (l, s) in &pl.shapes {
            g.mark(*l, s, env.inflate, remap(pl.id));
        }
    }
    g
}

/// Coarse congestion of `grid` (2 mm cells at the default pitch); board-edge keep-out is ignored.
fn congestion_map(env: &Env, grid: &Grid) -> CongestionMap {
    let occ = |l: usize| -> Vec<bool> {
        grid.owner[l]
            .iter()
            .zip(env.outside.iter())
            .map(|(&o, &out)| o != FREE && !out)
            .collect()
    };
    let (top, bottom) = (occ(0), occ(1));
    CongestionMap::new(grid.cols, grid.rows, 8, [&top, &bottom])
}

/// Cells near the already routed copper of `conn`'s differential-pair partner.
fn affinity(env: &Env, conn: &Conn, placed: &[Placed]) -> Option<[Vec<bool>; 2]> {
    let partner = intel::partner_of(&conn.net, env.pairs)?;
    let mut scratch = env.base.blank();
    let mut any = false;
    for pl in placed.iter().filter(|pl| pl.net == partner) {
        for (l, s) in &pl.shapes {
            scratch.mark(*l, s, 4.0 * env.opts.pitch as f64, 1);
            any = true;
        }
    }
    any.then(|| {
        [
            scratch.owner[0].iter().map(|&o| o != FREE).collect(),
            scratch.owner[1].iter().map(|&o| o != FREE).collect(),
        ]
    })
}

/// Route every connection once in priority order, with selective rip-up of lower-priority
/// copper placed in this pass.
fn run_pass(env: &Env, conns: &[Conn], key: &[i64]) -> PassOutcome {
    let mut order: Vec<usize> = (0..conns.len()).collect();
    order.sort_by_key(|&i| key[i]);
    let mut queue: VecDeque<usize> = order.into();
    let mut grid = env.base.clone();
    let mut placed: Vec<Placed> = vec![];
    let mut failed: BTreeMap<usize, String> = BTreeMap::new();
    let mut ripups = 0usize;
    let mut rip_tries: BTreeMap<usize, usize> = BTreeMap::new();
    let mut cong = congestion_map(env, &grid).step_penalties();
    let mut since_cong = 0usize;

    let mut pair_notes: BTreeMap<usize, String> = BTreeMap::new();
    while let Some(ci) = queue.pop_front() {
        if placed.iter().any(|pl| pl.conn == ci) {
            continue; // routed with its pair partner
        }
        if since_cong >= 8 {
            cong = congestion_map(env, &grid).step_penalties();
            since_cong = 0;
        }
        let conn = &conns[ci];
        if let Some(cj) = env.mates[ci].filter(|cj| !placed.iter().any(|pl| pl.conn == *cj)) {
            // The positive member is the one whose net keys the declared pair.
            let (ip, inn) = if env.coupled.contains_key(&conn.net) {
                (ci, cj)
            } else {
                (cj, ci)
            };
            if let Some(Ok(rule)) = env.coupled.get(&conns[ip].net) {
                let costs = Costs {
                    congestion: &cong,
                    affinity: None,
                };
                match diffpair::attempt(
                    env,
                    rule,
                    (&conns[ip], ip),
                    (&conns[inn], inn),
                    &placed,
                    &costs,
                ) {
                    Ok(pr) => {
                        pair_notes.insert(ip, pr.note);
                        for pl in pr.placed {
                            mark_placed(&mut grid, &pl, env.inflate);
                            failed.remove(&pl.conn);
                            placed.push(pl);
                        }
                        since_cong += 2;
                        continue;
                    }
                    Err(why) => {
                        pair_notes.insert(
                            ip,
                            format!("one connection routed as two single nets: {why}"),
                        );
                    }
                }
            }
        }
        let aff = affinity(env, conn, &placed);
        let costs = Costs {
            congestion: &cong,
            affinity: aff.as_ref(),
        };
        match attempt(env, &grid, conn, ci, &placed, &costs) {
            Ok(pl) => {
                mark_placed(&mut grid, &pl, env.inflate);
                placed.push(pl);
                failed.remove(&ci);
                since_cong += 1;
            }
            Err(why) => {
                // Push-and-shove first: move lower-priority pre-existing tracks aside.
                if let Some((route, moved)) = shove(env, conn, ci, &placed, &costs) {
                    placed.push(route);
                    placed.extend(moved);
                    failed.remove(&ci);
                    grid = grid_with(env, placed.iter());
                    cong = congestion_map(env, &grid).step_penalties();
                    since_cong = 0;
                    continue;
                }
                // Bounded fallback: rip up lower-priority copper this pass placed.
                let tries = rip_tries.entry(ci).or_insert(0);
                let allowed = *tries < 1 && ripups < env.opts.max_ripups;
                *tries += 1;
                if allowed {
                    if let Some(victims) = find_victims(env, ci, key, conn, &placed, &costs) {
                        let mut removed: Vec<Placed> = vec![];
                        for &idx in victims.iter().rev() {
                            removed.push(placed.remove(idx));
                        }
                        let g2 = grid_with(env, placed.iter());
                        match attempt(env, &g2, conn, ci, &placed, &costs) {
                            Ok(pl) => {
                                grid = g2;
                                mark_placed(&mut grid, &pl, env.inflate);
                                placed.push(pl);
                                failed.remove(&ci);
                                ripups += removed.len();
                                removed.sort_by_key(|r| key[r.conn]);
                                for r in removed {
                                    queue.push_back(r.conn);
                                }
                                cong = congestion_map(env, &grid).step_penalties();
                                since_cong = 0;
                                continue;
                            }
                            Err(_) => placed.extend(removed), // restore: nothing was gained
                        }
                    }
                }
                failed.insert(ci, why);
            }
        }
    }
    let wire_nm: i64 = placed
        .iter()
        .flat_map(|pl| pl.segs.iter())
        .map(|(_, a, b)| a.dist(*b) as i64)
        .sum();
    let vias = placed.iter().map(|pl| pl.vias.len()).sum();
    PassOutcome {
        placed,
        failed,
        ripups,
        wire_nm,
        vias,
        pair_notes,
    }
}

/// Which already placed, lower-priority routes block `conn`? Routes it on a board where only
/// higher-priority copper exists; the lower-priority routes that path crosses are the victims.
/// `None` when nothing ripable is in the way, the path is blocked by fixed copper, or too many
/// routes would have to go.
fn find_victims(
    env: &Env,
    ci: usize,
    key: &[i64],
    conn: &Conn,
    placed: &[Placed],
    costs: &Costs,
) -> Option<Vec<usize>> {
    let my_key = key[ci];
    // Coupled pair copper is never a victim, so it stays in the way like higher-priority copper.
    let ghost = grid_with(
        env,
        placed
            .iter()
            .filter(|pl| pl.pair || pl.moved.is_some() || key[pl.conn] <= my_key),
    );
    let id = *env.net_ids.get(&conn.net)?;
    let la = pad_layers(env.copper, conn.a, &conn.net);
    let lb = pad_layers(env.copper, conn.b, &conn.net);
    let path = astar(&ghost, id, conn.a, &la, conn.b, &lb, env.opts, costs)?;
    let mut hit: BTreeSet<usize> = BTreeSet::new();
    for &(l, c, r) in &path {
        let pt = ghost.center(c, r);
        for (idx, pl) in placed.iter().enumerate() {
            if pl.net == conn.net || pl.pair || pl.moved.is_some() || key[pl.conn] <= my_key {
                continue;
            }
            let covers = pl.shapes.iter().any(|(sl, s)| {
                *sl == l
                    && Shape {
                        core: s.core.clone(),
                        radius: s.radius + env.inflate,
                    }
                    .contains(pt)
            });
            if covers {
                hit.insert(idx);
            }
        }
    }
    if hit.is_empty() || hit.len() > env.opts.max_victims {
        None
    } else {
        Some(hit.into_iter().collect())
    }
}

/// Push-and-shove for a connection that found no path: route it as if the pre-existing tracks
/// of lower-priority nets (`Env::priority`; never a declared pair's) were not there, then move
/// each of those tracks the route crowds sideways out of the way with the route tool's 45-degree
/// bump that keeps both of its ends (`interactive::bump`), and a moved track that then crowds a
/// further one pushes it too, up to `MAX_SHOVE_LEVELS` levels and `MAX_SHOVED` tracks. Accepted
/// only when the route and every moved track pass the exact clearance check against all copper
/// (each other included) and the moved tracks keep the board-edge clearance; the moved tracks
/// come back as placed copper that replaces the originals. `None` when nothing movable blocks,
/// a blocking track cannot be bumped (crossed, an arc, joined inside the bump) or a check fails:
/// the caller then falls back to rip-up.
fn shove(
    env: &Env,
    conn: &Conn,
    ci: usize,
    placed: &[Placed],
    costs: &Costs,
) -> Option<(Placed, Vec<Placed>)> {
    use crate::interactive::{bump, Pusher, MAX_SHOVED, MAX_SHOVE_LEVELS};
    let c = &env.p.constraints;
    let score = |net: &str| env.priority.get(net).copied().unwrap_or(0);
    let mine = score(&conn.net);
    let already = moved_ids(placed.iter());
    let movable: BTreeSet<String> = env
        .copper
        .iter()
        .filter(|i| {
            i.kind == ItemKind::Track
                && !i.net.is_empty()
                && i.net != conn.net
                && score(&i.net) < mine
                && layer_index(i.layer).is_some()
                && !c.diff_pairs.iter().any(|d| d.has(&i.net))
                && !already.contains(&i.object_id)
        })
        .map(|i| i.object_id.clone())
        .collect();
    if movable.is_empty() {
        return None;
    }
    let id = *env.net_ids.get(&conn.net)?;
    let spec = &env.specs[&conn.net];
    let mut hide = movable.clone();
    hide.extend(already);
    let refs: Vec<&Placed> = placed.iter().collect();
    let ghost = fresh_grid(env, &refs, &hide, |x| x);
    let la = pad_layers(env.copper, conn.a, &conn.net);
    let lb = pad_layers(env.copper, conn.b, &conn.net);
    let path = astar(&ghost, id, conn.a, &la, conn.b, &lb, env.opts, costs)?;
    let (segs, vias) = path_to_segments(&ghost, &path, conn.a, conn.b);
    let shapes = shapes_for(&segs, &vias, spec.track_width, spec.via_diameter);
    let clearance = spec.clearance as f64;
    if violation_hiding(env, &shapes, &conn.net, clearance, placed, &movable).is_some() {
        return None;
    }

    // The route pushes; then each level's moved tracks push.
    let mut pushers: Vec<Pusher> = shapes
        .iter()
        .filter_map(|(l, s)| match s.core {
            Core::Segment(a, b) => Some(Pusher {
                a,
                b,
                half_width: s.radius,
                layer: layer_of(*l),
                net: conn.net.clone(),
            }),
            _ => None,
        })
        .collect();
    let crowded = |pushers: &[Pusher], done: &[crate::interactive::ShovedTrack]| -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for it in env.copper.iter().filter(|i| movable.contains(&i.object_id)) {
            if done.iter().any(|m| m.id == it.object_id) || out.contains(&it.object_id) {
                continue;
            }
            let hit = pushers.iter().any(|q| {
                q.layer == it.layer
                    && q.net != it.net
                    && distance(
                        &Shape {
                            core: Core::Segment(q.a, q.b),
                            radius: q.half_width,
                        },
                        &it.shape,
                    ) < crate::clearance::required(c, &q.net, &it.net, it.layer) - 1.0
            });
            if hit {
                out.push(it.object_id.clone());
            }
        }
        out
    };
    let mut moved: Vec<crate::interactive::ShovedTrack> = vec![];
    let mut next = crowded(&pushers, &moved);
    for _ in 0..MAX_SHOVE_LEVELS {
        if next.is_empty() {
            break;
        }
        let first = moved.len();
        for t in &next {
            moved.push(bump(env.p, c, env.copper, &pushers, t)?);
        }
        if moved.len() > MAX_SHOVED {
            return None;
        }
        let level: Vec<Pusher> = moved[first..]
            .iter()
            .flat_map(|s| {
                s.points
                    .windows(2)
                    .filter(|w| w[0] != w[1])
                    .map(|w| Pusher {
                        a: w[0],
                        b: w[1],
                        half_width: s.width as f64 / 2.0,
                        layer: s.layer,
                        net: s.net.clone(),
                    })
            })
            .collect();
        next = crowded(&level, &moved);
        pushers.extend(level);
    }
    if !next.is_empty() {
        return None; // more levels would be needed
    }

    let route = Placed {
        conn: ci,
        net: conn.net.clone(),
        id,
        width: spec.track_width,
        via_drill: spec.via_drill,
        via_diameter: spec.via_diameter,
        segs,
        vias,
        shapes,
        note: None,
        pair: false,
        moved: None,
    };
    let moved: Vec<Placed> = moved
        .into_iter()
        .map(|s| {
            let l = layer_index(s.layer).expect("movable tracks are on router layers");
            let segs: Vec<(usize, Point, Point)> = s
                .points
                .windows(2)
                .filter(|w| w[0] != w[1])
                .map(|w| (l, w[0], w[1]))
                .collect();
            Placed {
                conn: usize::MAX,
                net: s.net.clone(),
                id: env.net_ids.get(&s.net).copied().unwrap_or(MULTI),
                width: s.width,
                via_drill: 0,
                via_diameter: 0,
                shapes: shapes_for(&segs, &[], s.width, 0),
                segs,
                vias: vec![],
                note: Some(format!("pushed aside for {} (push-and-shove)", conn.net)),
                pair: false,
                moved: Some(s.id),
            }
        })
        .collect();
    // Exact check of the route and of every moved track against everything, moved tracks at
    // their new places and the originals gone.
    let mut after: Vec<Placed> = placed.to_vec();
    after.extend(moved.iter().cloned());
    if violation(env, &route.shapes, &route.net, clearance, &after).is_some() {
        return None;
    }
    after.push(route.clone());
    for m in &moved {
        let own = env.specs.get(&m.net).map_or(0.0, |s| s.clearance as f64);
        let hw = m.width as f64 / 2.0;
        if violation(env, &m.shapes, &m.net, own, &after).is_some()
            || m.segs
                .iter()
                .any(|(_, a, b)| !edge_clear(env.p, *a, *b, hw))
        {
            return None;
        }
    }
    Some((route, moved))
}

/// Whether a track segment of half-width `hw` lies inside the board outline and keeps the
/// board-edge clearance (no outline: nothing to keep).
fn edge_clear(p: &Project, a: Point, b: Point, hw: f64) -> bool {
    let o = &p.pcb.board.outline;
    if o.len() < 3 {
        return true;
    }
    let gap = (0..o.len())
        .map(|i| segment_segment_distance(a, b, o[i], o[(i + 1) % o.len()]))
        .fold(f64::INFINITY, f64::min);
    point_in_polygon(a, o)
        && point_in_polygon(b, o)
        && gap - hw >= p.constraints.board_edge_clearance as f64 - 1.0
}

/// Route one connection on `grid`; exact clearance is verified before it is accepted.
fn attempt(
    env: &Env,
    grid: &Grid,
    conn: &Conn,
    ci: usize,
    placed: &[Placed],
    costs: &Costs,
) -> Result<Placed, String> {
    let Some(&id) = env.net_ids.get(&conn.net) else {
        return Err("NO_NET: connection has no net".into());
    };
    let spec = &env.specs[&conn.net];
    let la = pad_layers(env.copper, conn.a, &conn.net);
    let lb = pad_layers(env.copper, conn.b, &conn.net);
    let Some(path) = astar(grid, id, conn.a, &la, conn.b, &lb, env.opts, costs) else {
        return Err("NO_LEGAL_PATH: no path found within clearance constraints".into());
    };
    let (segs, vias) = path_to_segments(grid, &path, conn.a, conn.b);

    // Preferred width first; a wide (current-sized) track that cannot clear a fine-pitch pad
    // necks down to the class width.
    let class_w = env
        .p
        .constraints
        .class_for(&conn.net)
        .track_width
        .max(env.p.constraints.min_track_width);
    let mut widths = vec![spec.track_width];
    if class_w < spec.track_width {
        widths.push(class_w);
    }
    let clearance = spec.clearance as f64;
    let mut last = String::new();
    for (k, &width) in widths.iter().enumerate() {
        let shapes = shapes_for(&segs, &vias, width, spec.via_diameter);
        match violation(env, &shapes, &conn.net, clearance, placed) {
            None => {
                let note = (k > 0).then(|| {
                    format!(
                        "necked down {:.3} -> {:.3} mm on one connection (the wider track could not clear nearby copper)",
                        spec.track_width as f64 / 1e6,
                        width as f64 / 1e6
                    )
                });
                return Ok(Placed {
                    conn: ci,
                    net: conn.net.clone(),
                    id,
                    width,
                    via_drill: spec.via_drill,
                    via_diameter: spec.via_diameter,
                    segs,
                    vias,
                    shapes,
                    note,
                    pair: false,
                    moved: None,
                });
            }
            Some(v) => last = v,
        }
    }
    Err(format!("CLEARANCE_CONFLICT: {last}"))
}

fn shapes_for(
    segs: &[(usize, Point, Point)],
    vias: &[Point],
    width: Nm,
    via_diameter: Nm,
) -> Vec<(usize, Shape)> {
    segs.iter()
        .map(|(l, s, e)| {
            (
                *l,
                Shape {
                    core: Core::Segment(*s, *e),
                    radius: width as f64 / 2.0,
                },
            )
        })
        .chain(vias.iter().flat_map(|v| {
            [0usize, 1].map(|l| {
                (
                    l,
                    Shape {
                        core: Core::Segment(*v, *v),
                        radius: via_diameter as f64 / 2.0,
                    },
                )
            })
        }))
        .collect()
}

/// Exact clearance of new shapes against all other-net copper (existing and placed): at least
/// `clearance` and the DRC-002 rule for each pair of nets (classes, net pairs, voltage).
/// Pre-existing tracks that placed copper replaces are not there any more.
fn violation(
    env: &Env,
    shapes: &[(usize, Shape)],
    net: &str,
    clearance: f64,
    placed: &[Placed],
) -> Option<String> {
    violation_hiding(env, shapes, net, clearance, placed, &BTreeSet::new())
}

/// [`violation`], also ignoring the pre-existing tracks in `hidden`.
fn violation_hiding(
    env: &Env,
    shapes: &[(usize, Shape)],
    net: &str,
    clearance: f64,
    placed: &[Placed],
    hidden: &BTreeSet<String>,
) -> Option<String> {
    let mut hidden = hidden.clone();
    hidden.extend(moved_ids(placed.iter()));
    let c = &env.p.constraints;
    let clearance = clearance.max(c.min_clearance as f64);
    let need = |other: &str, l: usize| {
        clearance.max(crate::clearance::required(c, net, other, layer_of(l)))
    };
    for (l, s) in shapes {
        for it in env
            .copper
            .iter()
            .filter(|i| layer_index(i.layer) == Some(*l) && i.net != net && !is_hidden(i, &hidden))
        {
            if distance(s, &it.shape) < need(&it.net, *l) - 1.0 {
                return Some(format!("clearance to {} would be violated", it.label()));
            }
        }
        for pl in placed.iter().filter(|pl| pl.net != net) {
            for (pl_l, ps) in &pl.shapes {
                if pl_l == l && distance(s, ps) < need(&pl.net, *l) - 1.0 {
                    return Some("clearance to a newly routed track would be violated".into());
                }
            }
        }
    }
    None
}

/// Grid path to segments: stubs from the pad centres, collinear merge, vias at layer changes.
fn path_to_segments(
    grid: &Grid,
    path: &[(usize, usize, usize)],
    a: Point,
    b: Point,
) -> (Vec<(usize, Point, Point)>, Vec<Point>) {
    let mut segs: Vec<(usize, Point, Point)> = Vec::new();
    let mut vias: Vec<Point> = Vec::new();
    let mut pts: Vec<(usize, Point)> = path
        .iter()
        .map(|&(l, c, r)| (l, grid.center(c, r)))
        .collect();
    pts.insert(0, (pts[0].0, a));
    let last_l = pts.last().map(|x| x.0).unwrap_or(0);
    pts.push((last_l, b));
    let mut run_start = pts[0];
    for w in 1..pts.len() {
        let (prev, cur) = (pts[w - 1], pts[w]);
        if cur.0 != prev.0 {
            if run_start.1 != prev.1 {
                segs.push((prev.0, run_start.1, prev.1));
            }
            vias.push(prev.1);
            run_start = cur;
            continue;
        }
        let next = pts.get(w + 1);
        let collinear = next.is_some_and(|n| {
            n.0 == cur.0 && {
                let (d1x, d1y) = (cur.1.x - prev.1.x, cur.1.y - prev.1.y);
                let (d2x, d2y) = (n.1.x - cur.1.x, n.1.y - cur.1.y);
                d1x as i128 * d2y as i128 == d1y as i128 * d2x as i128
                    && (d1x.signum(), d1y.signum()) == (d2x.signum(), d2y.signum())
            }
        });
        if !collinear && run_start.1 != cur.1 {
            segs.push((cur.0, run_start.1, cur.1));
            run_start = cur;
        }
    }
    (segs, vias)
}

#[allow(clippy::too_many_arguments)]
fn astar(
    grid: &Grid,
    net: u32,
    a: Point,
    la: &[usize],
    b: Point,
    lb: &[usize],
    opts: &RouterOptions,
    costs: &Costs,
) -> Option<Vec<(usize, usize, usize)>> {
    let (sc, sr) = grid.cell_of(a);
    let (gc, gr) = grid.cell_of(b);
    let n = grid.cols * grid.rows;
    let state = |l: usize, i: usize| l * n + i;
    let mut g = vec![u64::MAX; 2 * n];
    let mut parent = vec![u32::MAX; 2 * n];
    let mut heap = BinaryHeap::new();
    let h = |c: usize, r: usize| -> u64 {
        let dx = (c as i64 - gc as i64).unsigned_abs();
        let dy = (r as i64 - gr as i64).unsigned_abs();
        let (mn, mx) = (dx.min(dy), dx.max(dy));
        mn * 14 + (mx - mn) * 10
    };
    for &l in la {
        let i = grid.idx(sc, sr);
        if grid.passable(l, i, net) {
            g[state(l, i)] = 0;
            heap.push(Reverse((h(sc, sr), 0u64, state(l, i))));
        }
    }
    let goal_i = grid.idx(gc, gr);
    let mut expansions = 0;
    while let Some(Reverse((_, cost, s))) = heap.pop() {
        if cost > g[s] {
            continue;
        }
        let (l, i) = (s / n, s % n);
        if i == goal_i && lb.contains(&l) {
            let mut path = vec![];
            let mut cur = s;
            loop {
                let (cl, ci) = (cur / n, cur % n);
                path.push((cl, ci % grid.cols, ci / grid.cols));
                if parent[cur] == u32::MAX {
                    break;
                }
                cur = parent[cur] as usize;
            }
            path.reverse();
            return Some(path);
        }
        expansions += 1;
        if expansions > opts.max_expansions {
            return None;
        }
        let (c, r) = (i % grid.cols, i / grid.cols);
        let mut relax =
            |ns: usize, step: u64, heap: &mut BinaryHeap<Reverse<(u64, u64, usize)>>| {
                let nc = cost + step;
                if nc < g[ns] {
                    g[ns] = nc;
                    parent[ns] = s as u32;
                    let ni = ns % n;
                    heap.push(Reverse((nc + h(ni % grid.cols, ni / grid.cols), nc, ns)));
                }
            };
        for (dc, dr) in [
            (-1i64, 0i64),
            (1, 0),
            (0, -1),
            (0, 1),
            (-1, -1),
            (-1, 1),
            (1, -1),
            (1, 1),
        ] {
            let (nc_, nr_) = (c as i64 + dc, r as i64 + dr);
            if nc_ < 0 || nr_ < 0 || nc_ >= grid.cols as i64 || nr_ >= grid.rows as i64 {
                continue;
            }
            let ni = grid.idx(nc_ as usize, nr_ as usize);
            if !grid.passable(l, ni, net) {
                continue;
            }
            let diag = dc != 0 && dr != 0;
            if diag {
                // No corner cutting past blocked cells.
                let a1 = grid.idx(nc_ as usize, r);
                let a2 = grid.idx(c, nr_ as usize);
                if !grid.passable(l, a1, net) || !grid.passable(l, a2, net) {
                    continue;
                }
            }
            let base_step: u64 = if diag { 14 } else { 10 };
            let step = base_step + costs.extra(l, ni);
            relax(state(l, ni), step, &mut heap);
        }
        // Via: both layers free here and around (via is larger than a track).
        let ol = 1 - l;
        let around_free = (-1i64..=1).all(|dr| {
            (-1i64..=1).all(|dc| {
                let (x, y) = (c as i64 + dc, r as i64 + dr);
                x >= 0 && y >= 0 && x < grid.cols as i64 && y < grid.rows as i64 && {
                    let j = grid.idx(x as usize, y as usize);
                    grid.passable(0, j, net) && grid.passable(1, j, net)
                }
            })
        });
        if around_free {
            relax(state(ol, i), opts.via_cost + costs.extra(ol, i), &mut heap);
        }
    }
    None
}

// Written without running (no local builds/tests in this phase): unrun.
#[cfg(test)]
mod tests {
    use super::*;

    fn conn(net: &str, ax: Nm, bx: Nm) -> Conn {
        Conn {
            net: net.into(),
            a: Point::new(ax, 0),
            b: Point::new(bx, 0),
        }
    }

    #[test]
    fn pair_partners_are_routed_back_to_back() {
        let pairs = vec![("USB_DP".to_string(), "USB_DN".to_string())];
        let conns = vec![
            conn("USB_DP", 0, 10_000),
            conn("SIG", 0, 5_000),
            conn("USB_DN", 100, 10_100),
        ];
        let out = group_pairs(conns, &pairs);
        let names: Vec<&str> = out.iter().map(|c| c.net.as_str()).collect();
        assert_eq!(names, ["USB_DP", "USB_DN", "SIG"]);
    }

    #[test]
    fn straight_path_becomes_one_segment() {
        let g = Grid {
            origin: Point::new(0, 0),
            pitch: 1_000,
            cols: 4,
            rows: 1,
            owner: [vec![FREE; 4], vec![FREE; 4]],
        };
        let path = vec![(0, 0, 0), (0, 1, 0), (0, 2, 0), (0, 3, 0)];
        let (segs, vias) = path_to_segments(&g, &path, Point::new(0, 0), Point::new(3_000, 0));
        assert_eq!(segs.len(), 1);
        assert!(vias.is_empty());
    }

    #[test]
    fn a_layer_change_places_a_via() {
        let g = Grid {
            origin: Point::new(0, 0),
            pitch: 1_000,
            cols: 4,
            rows: 1,
            owner: [vec![FREE; 4], vec![FREE; 4]],
        };
        let path = vec![(0, 0, 0), (0, 1, 0), (1, 1, 0), (1, 2, 0)];
        let (segs, vias) = path_to_segments(&g, &path, Point::new(0, 0), Point::new(2_000, 0));
        assert_eq!(vias, vec![Point::new(1_000, 0)]);
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn astar_avoids_a_congested_cell_when_a_free_detour_exists() {
        let g = Grid {
            origin: Point::new(0, 0),
            pitch: 1_000,
            cols: 5,
            rows: 3,
            owner: [vec![FREE; 15], vec![MULTI; 15]],
        };
        let mut pen = [vec![0u8; 15], vec![0u8; 15]];
        pen[0][g.idx(2, 1)] = 30; // the straight line passes through a congested cell
        let costs = Costs {
            congestion: &pen,
            affinity: None,
        };
        let opts = RouterOptions::default();
        let path = astar(
            &g,
            1,
            Point::new(0, 1_000),
            &[0],
            Point::new(4_000, 1_000),
            &[0],
            &opts,
            &costs,
        )
        .unwrap();
        assert!(path.iter().all(|&(_, c, r)| !(c == 2 && r == 1)));
    }
}
