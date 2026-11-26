//! Native multilayer grid router (EAK-owned; no Freerouting dependency).
//!
//! Algorithm: every copper layer of the stack-up is rasterised into an ownership grid where a
//! cell records which net's copper (inflated by clearance + half track width + a discretisation
//! margin) covers it. A* with 8-way moves on the signal layers and layer changes (vias) then
//! connects each net's pads along a minimum spanning tree. Every produced segment and via is
//! re-checked with exact geometry before it is accepted; failures are reported, never hidden.
//!
//! Obstacles come from the one board geometry (`board::BoardGeometry`, ADR-0052): pads, tracks and
//! vias on every layer, the flattened outline and the cut-outs (board edge clearance), keep-out
//! regions (tracks per layer, vias), and drilled holes (hole-to-hole spacing for new vias).
//!
//! Layers and vias:
//!
//! * routing runs on every *signal* layer of the stack-up; plane layers carry pours (reached by
//!   `stitch`) and are never routed on, but their copper is still an obstacle for vias;
//! * a layer change places one through via (top to bottom) of the net's class size, drill and
//!   annular ring raised to the board minimums (`routing_intel::net_spec`); a via must clear
//!   every layer it spans. Blind and buried spans are not used: EAK-IR has no allowed-span rule yet;
//! * vias exist only where the path changes layer.
//!
//! Routing intelligence on top of that (`routing_intel`, DESK-derived mental models):
//!
//! * connections are routed most-constrained first (`priority`), differential-pair partners
//!   back to back, and the second net of a pair prefers to stay near the first;
//! * step costs include a congestion penalty measured on the ownership grid;
//! * routing is pass-based: a pass that leaves connections unrouted promotes them and routes
//!   everything again; the best pass wins; passes stop on stagnation (no change in unrouted
//!   count, wire length or vias) or `max_passes`;
//! * inside a pass, a connection that finds no path may rip up the minimal set of
//!   *lower-priority copper this run placed* that blocks it, route itself, then re-route the
//!   victims; if that fails the victims are restored. Pre-existing copper is never touched;
//! * track width comes from the net class and declared current (IPC-2221, external on the outer
//!   layers, internal on inner layers); a connection whose width does not fit fails and says
//!   why, it is never necked below its current-sized width.
//!
//! The exact check applies the DRC's own rules: `clearance::required` (DRC-002), the clearance
//! matrix, board edge (DRC-005), copper-to-hole (DRC-018), hole-to-hole (DRC-019), keep-outs
//! (DRC-021) and constraint-region track widths (DRC-022). The router is still a heuristic: whether
//! the resulting board is electrically or manufacturing correct is decided afterwards by
//! `eak-validation` (DRC, connectivity), never here.

use crate::board::{BoardGeometry, Hole};
use crate::clearance;
use crate::copper::{distance, CopperItem, Core, ItemKind, Shape};
use crate::ipc::{self, ClearanceCondition};
use crate::routing_intel::{
    self as intel, CongestionMap, Hotspot, NetSpec, PassRecord, Stagnation,
};
use eak_core::geom::{point_in_polygon, point_segment_distance, segment_segment_distance};
use eak_core::{CopperLayer, LayerKind, Nm, ObjectClass, Point, Project, Rect, RegionObject};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutedTrack {
    pub layer: CopperLayer,
    pub a: Point,
    pub b: Point,
    pub width: Nm,
    pub net: String,
}

/// A via the router or the stitcher places: always a through via (top to bottom).
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

/// Routed state of one differential pair (name-based detection, see `routing_intel`).
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
    /// Width / via / layer decisions that differ from the plain net class.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Copper layers routed on (the signal layers of the stack-up), top to bottom.
    #[serde(default)]
    pub routing_layers: Vec<CopperLayer>,
}

const FREE: u32 = 0;
const MULTI: u32 = u32::MAX;
/// Extra step cost for leaving a differential-pair partner's neighbourhood.
const AFFINITY_PENALTY: u64 = 4;
/// Failure of the grid search; replaced by `explain_no_path` in the result.
const NO_PATH: &str = "NO_LEGAL_PATH";

#[derive(Clone)]
struct Grid {
    origin: Point,
    pitch: Nm,
    cols: usize,
    rows: usize,
    /// One plane per stack-up layer, top to bottom.
    owner: Vec<Vec<u32>>,
    /// Cells where no via may be drilled (keep-outs, hole-to-hole spacing).
    no_via: Vec<bool>,
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
            owner: vec![vec![FREE; n]; self.owner.len()],
            no_via: vec![false; n],
        }
    }
    /// Cells whose centre lies in `shape` grown by `inflate`.
    fn cells(&self, shape: &Shape, inflate: f64) -> Vec<usize> {
        let s = Shape {
            core: shape.core.clone(),
            radius: shape.radius + inflate,
        };
        let bb = s.bbox();
        let c0 = ((bb.min.x - self.origin.x) / self.pitch - 1).max(0) as usize;
        let r0 = ((bb.min.y - self.origin.y) / self.pitch - 1).max(0) as usize;
        let c1 = (((bb.max.x - self.origin.x) / self.pitch + 2).max(0) as usize).min(self.cols);
        let r1 = (((bb.max.y - self.origin.y) / self.pitch + 2).max(0) as usize).min(self.rows);
        let mut out = vec![];
        for r in r0..r1 {
            for c in c0..c1 {
                if s.contains(self.center(c, r)) {
                    out.push(self.idx(c, r));
                }
            }
        }
        out
    }
    fn mark(&mut self, layer: usize, shape: &Shape, inflate: f64, net: u32) {
        for i in self.cells(shape, inflate) {
            let o = &mut self.owner[layer][i];
            *o = if *o == FREE || *o == net { net } else { MULTI };
        }
    }
    fn forbid_via(&mut self, shape: &Shape, inflate: f64) {
        for i in self.cells(shape, inflate) {
            self.no_via[i] = true;
        }
    }
    fn passable(&self, layer: usize, i: usize, net: u32) -> bool {
        let o = self.owner[layer][i];
        o == FREE || o == net
    }
    /// A through via of `net` may sit at cell (c, r): drilling allowed and every layer free
    /// here and around (a via is larger than a track).
    fn via_ok(&self, c: usize, r: usize, net: u32) -> bool {
        !self.no_via[self.idx(c, r)]
            && (-1i64..=1).all(|dr| {
                (-1i64..=1).all(|dc| {
                    let (x, y) = (c as i64 + dc, r as i64 + dr);
                    x >= 0 && y >= 0 && x < self.cols as i64 && y < self.rows as i64 && {
                        let j = self.idx(x as usize, y as usize);
                        (0..self.owner.len()).all(|l| self.passable(l, j, net))
                    }
                })
            })
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
    /// Track width per stack-up layer.
    widths: Vec<Nm>,
    via_drill: Nm,
    via_diameter: Nm,
    segs: Vec<(usize, Point, Point)>,
    vias: Vec<Point>,
    shapes: Vec<(usize, Shape)>,
}

struct PassOutcome {
    placed: Vec<Placed>,
    failed: BTreeMap<usize, String>,
    ripups: usize,
    wire_nm: i64,
    vias: usize,
}

/// A region polygon some objects may not touch.
struct KeepOut {
    label: String,
    shape: Shape,
    /// Per stack-up layer: does the region apply there.
    layers: Vec<bool>,
}

/// Everything a connection attempt needs that does not change during a run.
struct Env<'a> {
    p: &'a Project,
    opts: &'a RouterOptions,
    geo: &'a BoardGeometry,
    /// Stack-up layers, top to bottom; grid plane `l` is `layers[l]`.
    layers: &'a [CopperLayer],
    /// Grid planes that carry routing (signal layers).
    routable: &'a [usize],
    /// Clearance-matrix class of each `geo.copper` item.
    classes: &'a [ObjectClass],
    net_ids: &'a BTreeMap<String, u32>,
    specs: &'a BTreeMap<String, NetSpec>,
    pairs: &'a [(String, String)],
    base: &'a Grid,
    /// Cells outside the board or inside the edge keep-out (not "congestion").
    outside: &'a [bool],
    /// Board edge as segments: the outline and every cut-out.
    edges: &'a [(Point, Point)],
    track_keepouts: &'a [KeepOut],
    via_keepouts: &'a [KeepOut],
    /// Grid inflation per plane: largest clearance + half the widest track on that layer + margin.
    inflate: Vec<f64>,
    /// Minimum drill-edge to drill-edge spacing (DRC-019).
    hole_min: f64,
    /// Grid inflation of a drill so another via drill keeps `hole_min`.
    via_hole_inflate: f64,
}

/// Per-cell extra step costs for one attempt.
struct Costs<'a> {
    congestion: &'a [Vec<u8>],
    affinity: Option<&'a [Vec<bool>]>,
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

fn ring_segments(ring: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    (0..ring.len()).map(move |i| (ring[i], ring[(i + 1) % ring.len()]))
}

pub fn route(p: &Project, opts: &RouterOptions) -> RouteResult {
    let mut result = RouteResult {
        grid_pitch: opts.pitch,
        ..Default::default()
    };
    let geo = BoardGeometry::build(p);
    let outline = &geo.outline;
    let Some(bb) = Rect::from_points(outline).filter(|_| outline.len() >= 3) else {
        result.failed_connections.push((
            String::new(),
            Point::default(),
            Point::default(),
            "board outline missing".into(),
        ));
        return result;
    };
    let c = &p.constraints;
    let stack = &p.pcb.board.stackup;
    let layers = stack.layer_ids();
    let nl = layers.len();
    let routable: Vec<usize> = (0..nl)
        .filter(|&l| stack.kind_of(layers[l]) != Some(LayerKind::Plane))
        .collect();
    result.routing_layers = routable.iter().map(|&l| layers[l]).collect();
    let pitch = opts.pitch;
    let cols = (bb.width() / pitch) as usize + 1;
    let rows = (bb.height() / pitch) as usize + 1;
    let n = cols * rows;
    let mut base = Grid {
        origin: bb.min,
        pitch,
        cols,
        rows,
        owner: vec![vec![FREE; n]; nl],
        no_via: vec![false; n],
    };

    // Net ids.
    let copper = &geo.copper;
    let mut net_ids: BTreeMap<String, u32> = BTreeMap::new();
    for it in copper {
        if !it.net.is_empty() {
            let next = net_ids.len() as u32 + 1;
            net_ids.entry(it.net.clone()).or_insert(next);
        }
    }
    // Per-net width / via from class and declared current.
    let specs: BTreeMap<String, NetSpec> = net_ids
        .keys()
        .map(|net| (net.clone(), intel::net_spec(p, net)))
        .collect();
    // Widest track on each plane (inner layers carry the IPC internal width).
    let width_on: Vec<f64> = layers
        .iter()
        .map(|l| {
            specs
                .values()
                .map(|s| {
                    if l.is_outer() {
                        s.track_width
                    } else {
                        s.inner_track_width
                    }
                })
                .chain(std::iter::once(c.default_class.track_width))
                .max()
                .unwrap_or(250_000) as f64
        })
        .collect();
    let max_via_d = specs
        .values()
        .map(|s| s.via_diameter)
        .chain(std::iter::once(c.default_class.via_diameter))
        .max()
        .unwrap_or(600_000) as f64;
    let max_via_drill = specs
        .values()
        .map(|s| s.via_drill)
        .chain(std::iter::once(c.default_class.via_drill))
        .max()
        .unwrap_or(300_000) as f64;
    // shortcut: one grid inflation per layer for every net (the largest clearance anywhere and
    // that layer's widest track), so one high-voltage or high-current net makes every net route
    // with its spacing on that layer; per-net grids would recover the space when dense HV /
    // high-current boards need it.
    let vmax = c
        .net_voltages_v
        .values()
        .fold(0.0f64, |a, &b| a.max(b.abs()));
    let mut max_clear = specs
        .values()
        .map(|s| s.clearance)
        .chain([c.default_class.clearance, c.min_clearance])
        .chain(c.net_pair_rules.iter().map(|r| r.clearance))
        .chain(c.clearance_matrix.iter().map(|m| m.clearance))
        .max()
        .unwrap_or(200_000) as f64;
    if c.net_voltages_v.len() >= 2 {
        max_clear = max_clear.max(ipc::clearance_mm(vmax * 2.0, ClearanceCondition::B2) * 1e6);
    }
    let margin = pitch as f64 * 0.5;
    let inflate: Vec<f64> = width_on
        .iter()
        .map(|w| max_clear + w / 2.0 + margin)
        .collect();

    // Board edge keep-out: outline and cut-outs.
    let mut edges: Vec<(Point, Point)> = ring_segments(outline).collect();
    for cut in &geo.cutouts {
        edges.extend(ring_segments(cut));
    }
    let edge_min = c.board_edge_clearance.max(
        c.matrix_cell(ObjectClass::Track, ObjectClass::Outline)
            .unwrap_or(0),
    ) as f64;
    // Outer width: an inner track nearer the edge than this is caught by the exact check.
    let edge = edge_min + width_on[0] / 2.0;
    let mut outside = vec![false; n];
    for r in 0..rows {
        for cc in 0..cols {
            let pt = base.center(cc, r);
            let near = edges
                .iter()
                .any(|&(a, b)| point_segment_distance(pt, a, b) < edge);
            let out = !point_in_polygon(pt, outline)
                || geo.cutouts.iter().any(|cut| point_in_polygon(pt, cut));
            if out || near {
                let i = base.idx(cc, r);
                for plane in base.owner.iter_mut() {
                    plane[i] = MULTI;
                }
                outside[i] = true;
            }
        }
    }
    // Keep-out regions (DRC-021): tracks per layer, vias wherever the barrel would touch them.
    let mut track_keepouts = vec![];
    let mut via_keepouts = vec![];
    for reg in p.pcb.regions.values().filter(|r| r.outline.len() >= 3) {
        let ko = || KeepOut {
            label: reg.label(),
            shape: Shape {
                core: Core::Polygon(reg.outline.clone()),
                radius: 0.0,
            },
            layers: layers.iter().map(|&l| reg.applies_to(l)).collect(),
        };
        if reg.forbids(RegionObject::Tracks) {
            let k = ko();
            for (l, on) in k.layers.iter().enumerate() {
                if *on {
                    base.mark(l, &k.shape, width_on[l] / 2.0 + margin, MULTI);
                }
            }
            track_keepouts.push(k);
        }
        if reg.forbids(RegionObject::Vias) {
            let k = ko();
            base.forbid_via(&k.shape, max_via_d / 2.0 + margin);
            via_keepouts.push(k);
        }
    }
    // Hole-to-hole spacing (DRC-019) around every drilled hole.
    let hole_min = c
        .matrix_cell(ObjectClass::Hole, ObjectClass::Hole)
        .map_or(clearance::HOLE_TO_HOLE_MIN, |m| {
            m.max(clearance::HOLE_TO_HOLE_MIN)
        }) as f64;
    let via_hole_inflate = hole_min + max_via_drill / 2.0 + margin;
    for h in &geo.holes {
        base.forbid_via(&h.shape(), via_hole_inflate);
    }
    // Existing copper on every layer. Unassigned-net pads block everyone.
    for it in copper {
        let id = net_ids.get(&it.net).copied().unwrap_or(MULTI);
        if let Some(l) = stack.index_of(it.layer) {
            base.mark(l, &it.shape, inflate[l], id);
        }
    }
    let classes: Vec<ObjectClass> = copper.iter().map(|it| class_of(it, &geo.holes)).collect();

    // Unrouted connections per net, most constrained first, pair partners back to back.
    let mut todo: Vec<(String, Point, Point)> = geo.connectivity().ratsnest;
    if let Some(only) = &opts.nets {
        todo.retain(|(n, _, _)| only.contains(n));
    }
    crate::priority::order_connections(p, &mut todo);
    let todo_nets: BTreeSet<String> = todo.iter().map(|(n, _, _)| n.clone()).collect();
    let pairs = intel::diff_pairs(&todo_nets);
    let conns = group_pairs(
        todo.into_iter()
            .map(|(net, a, b)| Conn { net, a, b })
            .collect(),
        &pairs,
    );
    if conns.is_empty() {
        return result;
    }

    let env = Env {
        p,
        opts,
        geo: &geo,
        layers: &layers,
        routable: &routable,
        classes: &classes,
        net_ids: &net_ids,
        specs: &specs,
        pairs: &pairs,
        base: &base,
        outside: &outside,
        edges: &edges,
        track_keepouts: &track_keepouts,
        via_keepouts: &via_keepouts,
        inflate,
        hole_min,
        via_hole_inflate,
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
            routed: out.placed.len(),
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
                layer: layers[*l],
                a: *s,
                b: *e,
                width: pl.widths[*l],
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
    }
    for l in (0..nl).filter(|l| !routable.contains(l)) {
        notes.insert(format!(
            "{} is a plane layer: not routed on (it carries a pour; StitchZones connects plane nets)",
            layers[l].name()
        ));
    }
    if nl > 2 && !result.vias.is_empty() {
        notes.insert(
            "vias are through vias (F.Cu to B.Cu): EAK-IR has no allowed blind/buried span rule yet"
                .into(),
        );
    }
    result.routed_connections = best.placed.len();
    let final_grid = grid_with(&env, best.placed.iter());
    let flat = vec![vec![0u8; n]; nl];
    let no_costs = Costs {
        congestion: &flat,
        affinity: None,
    };
    for (ci, why) in &best.failed {
        let c = &conns[*ci];
        let why = match env.net_ids.get(&c.net) {
            Some(&id) if why == NO_PATH => {
                let ends = (pad_layers(&env, c.a, &c.net), pad_layers(&env, c.b, &c.net));
                explain_no_path(
                    &env,
                    &final_grid,
                    c,
                    id,
                    (&ends.0, &ends.1),
                    &best.placed,
                    &no_costs,
                )
            }
            _ => why.clone(),
        };
        result
            .failed_connections
            .push((c.net.clone(), c.a, c.b, why));
    }
    result.notes = notes.into_iter().collect();
    result.pairs = pair_reports(&pairs, &best.placed, &conns, &best.failed);
    let cong = congestion_map(&env, &final_grid);
    result.congestion = cong.hotspots(5, base.origin, pitch);
    result
}

/// Clearance-matrix class of a copper item (a pad that owns a drilled hole is a TH pad).
fn class_of(it: &CopperItem, holes: &[Hole]) -> ObjectClass {
    match it.kind {
        ItemKind::Track => ObjectClass::Track,
        ItemKind::Via => ObjectClass::Via,
        ItemKind::Zone => ObjectClass::Zone,
        ItemKind::Pad if holes.iter().any(|h| h.is_owner(it)) => ObjectClass::ThPad,
        ItemKind::Pad => ObjectClass::SmdPad,
    }
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
    pairs: &[(String, String)],
    placed: &[Placed],
    conns: &[Conn],
    failed: &BTreeMap<usize, String>,
) -> Vec<PairReport> {
    let length_mm = |net: &str| -> f64 {
        placed
            .iter()
            .filter(|pl| pl.net == net)
            .flat_map(|pl| pl.segs.iter())
            .map(|(_, a, b)| a.dist(*b))
            .sum::<f64>()
            / 1e6
    };
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
            PairReport {
                positive: a.clone(),
                negative: b.clone(),
                length_positive_mm: (la * 1e3).round() / 1e3,
                length_negative_mm: (lb * 1e3).round() / 1e3,
                skew_mm: ((la - lb).abs() * 1e3).round() / 1e3,
                vias_positive: vias(a),
                vias_negative: vias(b),
                fully_routed: complete(a) && complete(b),
                note: "routed adjacent (the second net prefers the first one's neighbourhood); spacing, coupled impedance and length matching are NOT controlled or verified".into(),
            }
        })
        .collect()
}

/// Routable grid planes a pad at `pt` on `net` is available on (every routable plane when no
/// pad is found there, e.g. a via end of a partly routed net).
fn pad_layers(env: &Env, pt: Point, net: &str) -> Vec<usize> {
    let stack = &env.p.pcb.board.stackup;
    let mut ls: Vec<usize> = env
        .geo
        .copper
        .iter()
        .filter(|i| i.kind == ItemKind::Pad && i.net == net && i.shape.contains(pt))
        .filter_map(|i| stack.index_of(i.layer))
        .filter(|l| env.routable.contains(l))
        .collect();
    ls.sort();
    ls.dedup();
    if ls.is_empty() {
        env.routable.to_vec()
    } else {
        ls
    }
}

fn mark_placed(grid: &mut Grid, pl: &Placed, env: &Env) {
    for (l, s) in &pl.shapes {
        grid.mark(*l, s, env.inflate[*l], pl.id);
    }
    for v in &pl.vias {
        let drill = Shape {
            core: Core::Segment(*v, *v),
            radius: pl.via_drill as f64 / 2.0,
        };
        grid.forbid_via(&drill, env.via_hole_inflate);
    }
}

/// The base grid plus the given placed routes.
fn grid_with<'a>(env: &Env, placed: impl Iterator<Item = &'a Placed>) -> Grid {
    let mut g = env.base.clone();
    for pl in placed {
        mark_placed(&mut g, pl, env);
    }
    g
}

/// Coarse congestion of `grid` (2 mm cells at the default pitch); board-edge keep-out is ignored.
fn congestion_map(env: &Env, grid: &Grid) -> CongestionMap {
    let occ: Vec<Vec<bool>> = grid
        .owner
        .iter()
        .map(|plane| {
            plane
                .iter()
                .zip(env.outside.iter())
                .map(|(&o, &out)| o != FREE && !out)
                .collect()
        })
        .collect();
    let refs: Vec<&[bool]> = occ.iter().map(|v| v.as_slice()).collect();
    CongestionMap::new(grid.cols, grid.rows, 8, &refs, env.layers)
}

/// Cells near the already routed copper of `conn`'s differential-pair partner.
fn affinity(env: &Env, conn: &Conn, placed: &[Placed]) -> Option<Vec<Vec<bool>>> {
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
        scratch
            .owner
            .iter()
            .map(|plane| plane.iter().map(|&o| o != FREE).collect())
            .collect()
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

    while let Some(ci) = queue.pop_front() {
        if since_cong >= 8 {
            cong = congestion_map(env, &grid).step_penalties();
            since_cong = 0;
        }
        let conn = &conns[ci];
        let aff = affinity(env, conn, &placed);
        let costs = Costs {
            congestion: &cong,
            affinity: aff.as_deref(),
        };
        match attempt(env, &grid, conn, ci, &placed, &costs) {
            Ok(pl) => {
                mark_placed(&mut grid, &pl, env);
                placed.push(pl);
                failed.remove(&ci);
                since_cong += 1;
            }
            Err(why) => {
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
                                mark_placed(&mut grid, &pl, env);
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
    let ghost = grid_with(env, placed.iter().filter(|pl| key[pl.conn] <= my_key));
    let id = *env.net_ids.get(&conn.net)?;
    let la = pad_layers(env, conn.a, &conn.net);
    let lb = pad_layers(env, conn.b, &conn.net);
    let path = astar(
        &ghost,
        id,
        (conn.a, &la),
        (conn.b, &lb),
        env.routable,
        env.opts,
        costs,
    )?;
    let mut hit: BTreeSet<usize> = BTreeSet::new();
    for &(l, c, r) in &path {
        let pt = ghost.center(c, r);
        for (idx, pl) in placed.iter().enumerate() {
            if pl.net == conn.net || key[pl.conn] <= my_key {
                continue;
            }
            let covers = pl.shapes.iter().any(|(sl, s)| {
                *sl == l
                    && Shape {
                        core: s.core.clone(),
                        radius: s.radius + env.inflate[l],
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
    let la = pad_layers(env, conn.a, &conn.net);
    let lb = pad_layers(env, conn.b, &conn.net);
    let Some(path) = astar(
        grid,
        id,
        (conn.a, &la),
        (conn.b, &lb),
        env.routable,
        env.opts,
        costs,
    ) else {
        // Explained once, on the final board, for connections that stay unrouted (`route`).
        return Err(NO_PATH.into());
    };
    let (segs, vias) = path_to_segments(grid, &path, conn.a, conn.b);
    let widths: Vec<Nm> = env
        .layers
        .iter()
        .map(|l| {
            if l.is_outer() {
                spec.track_width
            } else {
                spec.inner_track_width
            }
        })
        .collect();
    let shapes = shapes_for(&segs, &vias, &widths, spec.via_diameter, env.layers.len());
    if let Some(v) = violation(env, &shapes, &vias, spec.via_drill, &conn.net, placed) {
        return Err(format!("CLEARANCE_CONFLICT: {v}"));
    }
    Ok(Placed {
        conn: ci,
        net: conn.net.clone(),
        id,
        widths,
        via_drill: spec.via_drill,
        via_diameter: spec.via_diameter,
        segs,
        vias,
        shapes,
    })
}

fn at(env: &Env, l: usize, pt: Point) -> String {
    format!(
        "({:.2}, {:.2}) mm on {}",
        pt.x as f64 / 1e6,
        pt.y as f64 / 1e6,
        env.layers[l].name()
    )
}

/// A grid with only some obstacle classes: the board edge always; fixed copper and drills when
/// `fixed`; and the routes of `placed`. Keep-outs are never in it.
fn partial_grid(env: &Env, fixed: bool, placed: &[Placed]) -> Grid {
    let mut g = env.base.blank();
    for (i, _) in env.outside.iter().enumerate().filter(|(_, o)| **o) {
        for plane in g.owner.iter_mut() {
            plane[i] = MULTI;
        }
    }
    if fixed {
        let stack = &env.p.pcb.board.stackup;
        for it in &env.geo.copper {
            let id = env.net_ids.get(&it.net).copied().unwrap_or(MULTI);
            if let Some(l) = stack.index_of(it.layer) {
                g.mark(l, &it.shape, env.inflate[l], id);
            }
        }
        for h in &env.geo.holes {
            g.forbid_via(&h.shape(), env.via_hole_inflate);
        }
    }
    for pl in placed {
        mark_placed(&mut g, pl, env);
    }
    g
}

/// Why `conn` has no path, as which objects of which rule close it. The obstacle classes are
/// dropped one at a time: if a path exists without the keep-outs, the keep-outs it crosses are
/// the cause; else if one exists with fixed copper only, the earlier routes it crosses are; else
/// fixed copper (pads, existing tracks and vias) closes every path, and the first such obstacle
/// on the shortest edge-only path is named.
fn explain_no_path(
    env: &Env,
    grid: &Grid,
    conn: &Conn,
    id: u32,
    (la, lb): (&[usize], &[usize]),
    placed: &[Placed],
    costs: &Costs,
) -> String {
    let search = |g: &Grid| {
        astar(
            g,
            id,
            (conn.a, la),
            (conn.b, lb),
            env.routable,
            env.opts,
            costs,
        )
    };
    // The real-board obstacles along `path`, distinct, in path order.
    let blockers = |path: &[(usize, usize, usize)], first_only: bool| -> Vec<String> {
        let mut out: Vec<String> = vec![];
        let mut prev_layer = None;
        for &(l, c, r) in path {
            let pt = grid.center(c, r);
            let why = if prev_layer.is_some_and(|pl| pl != l) && !grid.via_ok(c, r, id) {
                Some(format!(
                    "no via fits at {}: {}",
                    at(env, l, pt),
                    via_blocker(env, grid, c, r, id, &conn.net, placed)
                ))
            } else if !grid.passable(l, grid.idx(c, r), id) {
                Some(format!(
                    "at {}: {}",
                    at(env, l, pt),
                    blocker(env, l, pt, &conn.net, placed)
                ))
            } else {
                None
            };
            prev_layer = Some(l);
            if let Some(w) = why {
                // One line per object, not per grid cell it covers.
                let object = w.split_once(": ").map_or(w.as_str(), |x| x.1).to_string();
                if !out.iter().any(|o| o.ends_with(&object)) {
                    out.push(w);
                }
                if first_only || out.len() >= 3 {
                    break;
                }
            }
        }
        out
    };
    if let Some(path) = search(&partial_grid(env, true, placed)) {
        let b = blockers(&path, false);
        if !b.is_empty() {
            return format!(
                "NO_LEGAL_PATH: only keep-out regions close the path; {}",
                b.join("; ")
            );
        }
    }
    if let Some(path) = search(&partial_grid(env, true, &[])) {
        let b = blockers(&path, false);
        if !b.is_empty() {
            return format!(
                "NO_LEGAL_PATH: routes placed earlier (higher priority, not ripped up) close the path; {}",
                b.join("; ")
            );
        }
    }
    match search(&partial_grid(env, false, &[])) {
        None => "NO_LEGAL_PATH: no path even on an empty board: an end lies outside the board or within the edge / cut-out clearance [DRC-005]".into(),
        Some(path) => match blockers(&path, true).pop() {
            Some(b) => format!(
                "NO_LEGAL_PATH: fixed copper and drills close every path; first obstacle on the shortest path {b}"
            ),
            None => format!(
                "NO_LEGAL_PATH: search budget exhausted ({} expansions) before a path was found",
                env.opts.max_expansions
            ),
        },
    }
}

/// The object that makes grid cell `pt` of plane `l` impassable for `net`.
fn blocker(env: &Env, l: usize, pt: Point, net: &str, placed: &[Placed]) -> String {
    let layer = env.layers[l];
    let dot = Shape {
        core: Core::Segment(pt, pt),
        radius: 0.0,
    };
    let near = env
        .geo
        .copper
        .iter()
        .filter(|it| it.layer == layer && (it.net != net || it.net.is_empty()))
        .map(|it| (distance(&dot, &it.shape), it))
        .filter(|(d, _)| *d <= env.inflate[l])
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((_, it)) = near {
        let net_name = if it.net.is_empty() { "no net" } else { &it.net };
        return format!(
            "{} ({net_name}): tracks keep {} from it [DRC-002]",
            it.label(),
            mm(clearance::required(&env.p.constraints, net, &it.net, layer))
        );
    }
    let regions: Vec<&str> = env
        .track_keepouts
        .iter()
        .filter(|k| k.layers[l] && distance(&dot, &k.shape) <= env.inflate[l])
        .map(|k| k.label.as_str())
        .collect();
    if !regions.is_empty() {
        return format!(
            "{}, which keep(s) tracks out of {} [DRC-021]",
            regions.join(" and "),
            layer.name()
        );
    }
    if let Some(pl) = placed.iter().find(|pl| {
        pl.net != net
            && pl
                .shapes
                .iter()
                .any(|(sl, s)| *sl == l && distance(&dot, s) <= env.inflate[l])
    }) {
        return format!(
            "a route of {} placed earlier (higher priority) [DRC-002]",
            pl.net
        );
    }
    "the board edge or a cut-out (edge clearance) [DRC-005]".into()
}

/// Why no through via of `net` fits at cell (c, r).
fn via_blocker(
    env: &Env,
    grid: &Grid,
    c: usize,
    r: usize,
    id: u32,
    net: &str,
    placed: &[Placed],
) -> String {
    let pt = grid.center(c, r);
    let dot = Shape {
        core: Core::Segment(pt, pt),
        radius: 0.0,
    };
    if grid.no_via[grid.idx(c, r)] {
        if let Some(k) = env
            .via_keepouts
            .iter()
            .find(|k| distance(&dot, &k.shape) <= env.inflate.iter().copied().fold(0.0, f64::max))
        {
            return format!("{}, which keeps vias out [DRC-021]", k.label);
        }
        let reach = env.via_hole_inflate;
        if let Some(h) = env
            .geo
            .holes
            .iter()
            .find(|h| distance(&dot, &h.shape()) <= reach)
        {
            return format!(
                "the drill of {} ({} hole-to-hole spacing) [DRC-019]",
                h.label,
                mm(env.hole_min)
            );
        }
        return format!(
            "a new via drill nearby ({} hole-to-hole spacing) [DRC-019]",
            mm(env.hole_min)
        );
    }
    // Some layer around the via site is taken: name it.
    for dr in -1i64..=1 {
        for dc in -1i64..=1 {
            let (x, y) = (c as i64 + dc, r as i64 + dr);
            if x < 0 || y < 0 || x >= grid.cols as i64 || y >= grid.rows as i64 {
                return "the board edge (edge clearance) [DRC-005]".into();
            }
            let j = grid.idx(x as usize, y as usize);
            if let Some(l) = (0..grid.owner.len()).find(|&l| !grid.passable(l, j, id)) {
                return blocker(env, l, grid.center(x as usize, y as usize), net, placed);
            }
        }
    }
    "an obstacle on another layer of the via span".into()
}

fn shapes_for(
    segs: &[(usize, Point, Point)],
    vias: &[Point],
    widths: &[Nm],
    via_diameter: Nm,
    nlayers: usize,
) -> Vec<(usize, Shape)> {
    segs.iter()
        .map(|(l, s, e)| {
            (
                *l,
                Shape {
                    core: Core::Segment(*s, *e),
                    radius: widths[*l] as f64 / 2.0,
                },
            )
        })
        .chain(vias.iter().flat_map(|v| {
            (0..nlayers).map(move |l| {
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

fn mm(v: f64) -> String {
    format!("{:.3} mm", v / 1e6)
}

/// Exact check of new shapes with the DRC's rules: clearance to other-net copper (DRC-002 incl.
/// voltage, net-pair and matrix rules), board edge and cut-outs (DRC-005), other-net holes
/// (DRC-018), hole-to-hole spacing of new vias (DRC-019) and keep-outs (DRC-021). Returns the
/// first problem: which object, which rule.
fn violation(
    env: &Env,
    shapes: &[(usize, Shape)],
    vias: &[Point],
    via_drill: Nm,
    net: &str,
    placed: &[Placed],
) -> Option<String> {
    let c = &env.p.constraints;
    // Router shapes: a zero-length core is a via pad, anything else a track.
    let class = |s: &Shape| match s.core {
        Core::Segment(a, b) if a == b => ObjectClass::Via,
        _ => ObjectClass::Track,
    };
    for (l, s) in shapes {
        let layer = env.layers[*l];
        let me = class(s);
        let rule = |other_net: &str, other: ObjectClass| -> f64 {
            let r = clearance::required(c, net, other_net, layer);
            c.matrix_cell(me, other).map_or(r, |m| r.max(m as f64))
        };
        for (it, oc) in env.geo.copper.iter().zip(env.classes) {
            if it.layer != layer || (it.net == net && !net.is_empty()) {
                continue;
            }
            let need = rule(&it.net, *oc);
            let d = distance(s, &it.shape);
            if d < need - 1.0 {
                return Some(format!(
                    "{} to {} ({}) on {} would be {}, below {} [DRC-002]",
                    me.label(),
                    it.label(),
                    it.net,
                    layer.name(),
                    mm(d.max(0.0)),
                    mm(need)
                ));
            }
        }
        for pl in placed.iter().filter(|pl| pl.net != net) {
            for (pl_l, ps) in &pl.shapes {
                if pl_l != l {
                    continue;
                }
                let need = rule(&pl.net, class(ps));
                if distance(s, ps) < need - 1.0 {
                    return Some(format!(
                        "{} would come within {} of a new {} route on {} [DRC-002]",
                        me.label(),
                        mm(need),
                        pl.net,
                        layer.name()
                    ));
                }
            }
        }
        let edge = c
            .board_edge_clearance
            .max(c.matrix_cell(me, ObjectClass::Outline).unwrap_or(0)) as f64;
        let (a, b) = match s.core {
            Core::Segment(a, b) => (a, b),
            Core::Polygon(_) => continue,
        };
        let gap = env
            .edges
            .iter()
            .map(|&(e0, e1)| segment_segment_distance(a, b, e0, e1))
            .fold(f64::INFINITY, f64::min)
            - s.radius;
        let in_cutout = env
            .geo
            .cutouts
            .iter()
            .any(|cut| point_in_polygon(a, cut) || point_in_polygon(b, cut));
        if gap < edge - 1.0 || in_cutout || !point_in_polygon(a, &env.geo.outline) {
            return Some(format!(
                "{} on {} would be within {} of the board edge or a cut-out [DRC-005]",
                me.label(),
                layer.name(),
                mm(edge)
            ));
        }
        for h in &env.geo.holes {
            if h.net == net && !net.is_empty() {
                continue;
            }
            let need = c
                .matrix_cell(ObjectClass::Hole, me)
                .map_or(c.min_clearance, |m| m.max(c.min_clearance)) as f64;
            if distance(s, &h.shape()) < need - 1.0 {
                return Some(format!(
                    "{} on {} would be within {} of the hole of {} [DRC-018]",
                    me.label(),
                    layer.name(),
                    mm(need),
                    h.label
                ));
            }
        }
        let keepouts = if me == ObjectClass::Via {
            env.via_keepouts
        } else {
            env.track_keepouts
        };
        for k in keepouts.iter().filter(|k| k.layers[*l]) {
            if distance(s, &k.shape) <= 0.0 {
                return Some(format!(
                    "{} on {} would enter {}, which keeps it out [DRC-021]",
                    me.label(),
                    layer.name(),
                    k.label
                ));
            }
        }
        if me == ObjectClass::Track {
            let w = (s.radius * 2.0).round() as Nm;
            for reg in env.p.pcb.regions.values() {
                let narrow = reg.min_track_width.is_some_and(|m| w < m);
                let wide = reg.max_track_width.is_some_and(|m| w > m);
                if !(narrow || wide) || !reg.applies_to(layer) || reg.outline.len() < 3 {
                    continue;
                }
                let area = Shape {
                    core: Core::Polygon(reg.outline.clone()),
                    radius: 0.0,
                };
                if distance(s, &area) <= 0.0 {
                    return Some(format!(
                        "a {} track on {} would enter {}, outside its track width limits [DRC-022]",
                        mm(w as f64),
                        layer.name(),
                        reg.label()
                    ));
                }
            }
        }
    }
    // Hole-to-hole spacing of the new vias against every drill: existing, placed, and each other.
    let r_new = via_drill as f64 / 2.0;
    let mut drills: Vec<(Point, f64, String)> = env
        .geo
        .holes
        .iter()
        .map(|h| (h.center, h.drill as f64 / 2.0, h.label.clone()))
        .collect();
    for pl in placed {
        for v in &pl.vias {
            drills.push((
                *v,
                pl.via_drill as f64 / 2.0,
                format!("a new {} via", pl.net),
            ));
        }
    }
    for (k, v) in vias.iter().enumerate() {
        let mine = vias[..k]
            .iter()
            .map(|o| (*o, r_new, "another via of this route".to_string()));
        for (at, r, label) in drills.iter().cloned().chain(mine) {
            let gap = v.dist(at) - r - r_new;
            if gap < env.hole_min - 1.0 {
                return Some(format!(
                    "via drill would be {} from {label}, below the {} hole-to-hole spacing [DRC-019]",
                    mm(gap.max(0.0)),
                    mm(env.hole_min)
                ));
            }
        }
    }
    None
}

/// Grid path to segments: stubs from the pad centres, collinear merge, one via per layer change.
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
            // A through via already joins every layer at this point.
            if vias.last() != Some(&prev.1) {
                vias.push(prev.1);
            }
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

/// Cheapest grid path from `start` (point, allowed planes) to `goal` on the `routable` planes;
/// a layer change is one through via straight to any other routable plane.
fn astar(
    grid: &Grid,
    net: u32,
    start: (Point, &[usize]),
    goal: (Point, &[usize]),
    routable: &[usize],
    opts: &RouterOptions,
    costs: &Costs,
) -> Option<Vec<(usize, usize, usize)>> {
    let (sc, sr) = grid.cell_of(start.0);
    let (gc, gr) = grid.cell_of(goal.0);
    let n = grid.cols * grid.rows;
    // States exist only for the routable planes: slot k is plane routable[k].
    let mut slot = vec![usize::MAX; grid.owner.len()];
    for (k, &l) in routable.iter().enumerate() {
        slot[l] = k;
    }
    let state = |l: usize, i: usize| slot[l] * n + i;
    let mut g = vec![u64::MAX; routable.len() * n];
    let mut parent = vec![u32::MAX; routable.len() * n];
    let mut heap = BinaryHeap::new();
    let h = |c: usize, r: usize| -> u64 {
        let dx = (c as i64 - gc as i64).unsigned_abs();
        let dy = (r as i64 - gr as i64).unsigned_abs();
        let (mn, mx) = (dx.min(dy), dx.max(dy));
        mn * 14 + (mx - mn) * 10
    };
    for &l in start.1.iter().filter(|l| routable.contains(l)) {
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
        let (l, i) = (routable[s / n], s % n);
        if i == goal_i && goal.1.contains(&l) {
            let mut path = vec![];
            let mut cur = s;
            loop {
                let (cl, ci) = (routable[cur / n], cur % n);
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
        // Layer change: one through via, straight to any other routing layer.
        if routable.len() > 1 && grid.via_ok(c, r, net) {
            for &ol in routable.iter().filter(|&&ol| ol != l) {
                relax(state(ol, i), opts.via_cost + costs.extra(ol, i), &mut heap);
            }
        }
    }
    None
}

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

    fn grid(cols: usize, rows: usize, layers: usize) -> Grid {
        Grid {
            origin: Point::new(0, 0),
            pitch: 1_000,
            cols,
            rows,
            owner: vec![vec![FREE; cols * rows]; layers],
            no_via: vec![false; cols * rows],
        }
    }

    fn no_costs(layers: usize, cells: usize) -> Vec<Vec<u8>> {
        vec![vec![0u8; cells]; layers]
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
        let g = grid(4, 1, 2);
        let path = vec![(0, 0, 0), (0, 1, 0), (0, 2, 0), (0, 3, 0)];
        let (segs, vias) = path_to_segments(&g, &path, Point::new(0, 0), Point::new(3_000, 0));
        assert_eq!(segs.len(), 1);
        assert!(vias.is_empty());
    }

    #[test]
    fn a_layer_change_places_a_via() {
        let g = grid(4, 1, 2);
        let path = vec![(0, 0, 0), (0, 1, 0), (1, 1, 0), (1, 2, 0)];
        let (segs, vias) = path_to_segments(&g, &path, Point::new(0, 0), Point::new(2_000, 0));
        assert_eq!(vias, vec![Point::new(1_000, 0)]);
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn two_layer_changes_at_one_point_are_one_via() {
        let g = grid(4, 1, 4);
        let path = vec![(0, 0, 0), (0, 1, 0), (2, 1, 0), (3, 1, 0), (3, 2, 0)];
        let (segs, vias) = path_to_segments(&g, &path, Point::new(0, 0), Point::new(2_000, 0));
        assert_eq!(vias, vec![Point::new(1_000, 0)]);
        assert_eq!(segs.iter().map(|s| s.0).collect::<Vec<_>>(), vec![0, 3]);
    }

    #[test]
    fn astar_avoids_a_congested_cell_when_a_free_detour_exists() {
        let mut g = grid(5, 3, 2);
        g.owner[1] = vec![MULTI; 15];
        let mut pen = no_costs(2, 15);
        pen[0][g.idx(2, 1)] = 30; // the straight line passes through a congested cell
        let costs = Costs {
            congestion: &pen,
            affinity: None,
        };
        let opts = RouterOptions::default();
        let path = astar(
            &g,
            1,
            (Point::new(0, 1_000), &[0]),
            (Point::new(4_000, 1_000), &[0]),
            &[0, 1],
            &opts,
            &costs,
        )
        .unwrap();
        assert!(path.iter().all(|&(_, c, r)| !(c == 2 && r == 1)));
    }

    #[test]
    fn astar_drops_to_an_inner_layer_past_an_outer_wall_and_skips_planes() {
        // Top (0) has a full-height wall at column 3; In1 (1) is a plane (not routable, free);
        // In2 (2) is free; Bottom (3) is blocked away from the via sites. The path must use
        // plane 2, never plane 1.
        let mut g = grid(9, 5, 4);
        for r in 0..5 {
            let i = g.idx(4, r);
            g.owner[0][i] = MULTI;
            g.owner[3][i] = MULTI;
        }
        let pen = no_costs(4, 45);
        let costs = Costs {
            congestion: &pen,
            affinity: None,
        };
        let opts = RouterOptions::default();
        let path = astar(
            &g,
            1,
            (Point::new(0, 2_000), &[0]),
            (Point::new(8_000, 2_000), &[0]),
            &[0, 2],
            &opts,
            &costs,
        )
        .unwrap();
        assert!(path.iter().any(|&(l, _, _)| l == 2), "uses In2");
        assert!(
            path.iter().all(|&(l, _, _)| l != 1),
            "never routes on the plane"
        );
        let (_, vias) = path_to_segments(&g, &path, Point::new(0, 2_000), Point::new(8_000, 2_000));
        assert_eq!(vias.len(), 2, "down and back up");
    }

    #[test]
    fn a_via_needs_every_layer_free_and_drilling_allowed() {
        let mut g = grid(9, 5, 4);
        for r in 0..5 {
            let i = g.idx(4, r);
            g.owner[0][i] = MULTI;
        }
        let pen = no_costs(4, 45);
        let costs = Costs {
            congestion: &pen,
            affinity: None,
        };
        let opts = RouterOptions::default();
        let route = |g: &Grid| {
            astar(
                g,
                1,
                (Point::new(0, 2_000), &[0]),
                (Point::new(8_000, 2_000), &[0]),
                &[0, 2],
                &opts,
                &costs,
            )
        };
        assert!(route(&g).is_some());
        // Other-net copper on the (unroutable) plane layer everywhere: no via can be placed.
        let mut blocked = g.clone();
        blocked.owner[1] = vec![MULTI; 45];
        assert!(route(&blocked).is_none());
        // A drilling ban everywhere has the same effect.
        let mut no_drill = g.clone();
        no_drill.no_via = vec![true; 45];
        assert!(route(&no_drill).is_none());
    }
}
