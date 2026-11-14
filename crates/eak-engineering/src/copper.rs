//! PCB copper model: exact shapes for pads, tracks and vias, shape-to-shape distance,
//! rasterised zone fill and physical copper connectivity.

use eak_core::geom::{point_in_polygon, point_segment_distance, segment_segment_distance};
use eak_core::{
    CopperLayer, Id, Nm, PadKind, PadShape, Point, Project, Rect, Rotation, Side, Zone,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Core geometry of a copper shape; the final shape is the core inflated by `radius`.
#[derive(Debug, Clone, PartialEq)]
pub enum Core {
    Segment(Point, Point),
    Polygon(Vec<Point>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub core: Core,
    pub radius: f64,
}

impl Shape {
    pub fn bbox(&self) -> Rect {
        let pts: Vec<Point> = match &self.core {
            Core::Segment(a, b) => vec![*a, *b],
            Core::Polygon(p) => p.clone(),
        };
        Rect::from_points(&pts)
            .unwrap_or(Rect {
                min: Point::default(),
                max: Point::default(),
            })
            .inflate(self.radius.ceil() as Nm)
    }
    pub fn contains(&self, p: Point) -> bool {
        match &self.core {
            Core::Segment(a, b) => point_segment_distance(p, *a, *b) <= self.radius,
            Core::Polygon(poly) => {
                point_in_polygon(p, poly) || poly_boundary_dist(p, poly) <= self.radius
            }
        }
    }
}

fn poly_boundary_dist(p: Point, poly: &[Point]) -> f64 {
    let mut best = f64::INFINITY;
    for i in 0..poly.len() {
        best = best.min(point_segment_distance(
            p,
            poly[i],
            poly[(i + 1) % poly.len()],
        ));
    }
    best
}

fn core_distance(a: &Core, b: &Core) -> f64 {
    match (a, b) {
        (Core::Segment(a1, a2), Core::Segment(b1, b2)) => {
            segment_segment_distance(*a1, *a2, *b1, *b2)
        }
        (Core::Segment(s1, s2), Core::Polygon(poly))
        | (Core::Polygon(poly), Core::Segment(s1, s2)) => {
            if point_in_polygon(*s1, poly) || point_in_polygon(*s2, poly) {
                return 0.0;
            }
            let mut best = f64::INFINITY;
            for i in 0..poly.len() {
                best = best.min(segment_segment_distance(
                    *s1,
                    *s2,
                    poly[i],
                    poly[(i + 1) % poly.len()],
                ));
            }
            best
        }
        (Core::Polygon(p1), Core::Polygon(p2)) => {
            if p1.iter().any(|v| point_in_polygon(*v, p2))
                || p2.iter().any(|v| point_in_polygon(*v, p1))
            {
                return 0.0;
            }
            let mut best = f64::INFINITY;
            for i in 0..p1.len() {
                for j in 0..p2.len() {
                    best = best.min(segment_segment_distance(
                        p1[i],
                        p1[(i + 1) % p1.len()],
                        p2[j],
                        p2[(j + 1) % p2.len()],
                    ));
                }
            }
            best
        }
    }
}

/// Edge-to-edge distance between two shapes (≤ 0 means touching/overlapping).
pub fn distance(a: &Shape, b: &Shape) -> f64 {
    core_distance(&a.core, &b.core) - a.radius - b.radius
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Pad,
    Track,
    Via,
    Zone,
}

/// One piece of copper on one layer.
#[derive(Debug, Clone)]
pub struct CopperItem {
    pub kind: ItemKind,
    /// Owning object id (footprint id for pads, track/via/zone id otherwise).
    pub object_id: Id,
    /// Pad number for pads.
    pub sub: String,
    pub layer: CopperLayer,
    pub net: String,
    pub shape: Shape,
}

impl CopperItem {
    pub fn label(&self) -> String {
        match self.kind {
            ItemKind::Pad => format!("pad {}.{}", self.object_id, self.sub),
            ItemKind::Track => format!("track {}", self.object_id),
            ItemKind::Via => format!("via {}", self.object_id),
            ItemKind::Zone => format!("zone {}", self.object_id),
        }
    }
}

pub fn pad_shape(center: Point, rot: Rotation, shape: PadShape, sx: Nm, sy: Nm) -> Shape {
    let (w, h) = rot.apply_size(sx, sy);
    match shape {
        PadShape::Circle => Shape {
            core: Core::Segment(center, center),
            radius: w.min(h) as f64 / 2.0,
        },
        PadShape::Oval => {
            let r = w.min(h) as f64 / 2.0;
            let (dx, dy) = if w >= h {
                ((w - h) / 2, 0)
            } else {
                (0, (h - w) / 2)
            };
            Shape {
                core: Core::Segment(
                    Point::new(center.x - dx, center.y - dy),
                    Point::new(center.x + dx, center.y + dy),
                ),
                radius: r,
            }
        }
        PadShape::Rect => {
            let r = Rect::centered(center, w, h);
            Shape {
                core: Core::Polygon(vec![
                    r.min,
                    Point::new(r.max.x, r.min.y),
                    r.max,
                    Point::new(r.min.x, r.max.y),
                ]),
                radius: 0.0,
            }
        }
    }
}

/// Every copper item of the design except zones (zones are handled via their fill).
pub fn items(p: &Project) -> Vec<CopperItem> {
    let mut out = Vec::new();
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        for pad in &def.footprint.pads {
            let c = p.pad_position(fp, pad);
            let shape = pad_shape(c, fp.rotation, pad.shape, pad.size_x, pad.size_y);
            let net = fp.pad_nets.get(&pad.number).cloned().unwrap_or_default();
            let layers: Vec<CopperLayer> = match pad.kind {
                PadKind::Tht => p.pcb.board.stackup.layer_ids(),
                PadKind::Smd => vec![if fp.side == Side::Top {
                    CopperLayer::Top
                } else {
                    CopperLayer::Bottom
                }],
            };
            for layer in layers {
                out.push(CopperItem {
                    kind: ItemKind::Pad,
                    object_id: fp.id.clone(),
                    sub: pad.number.clone(),
                    layer,
                    net: net.clone(),
                    shape: shape.clone(),
                });
            }
        }
    }
    for t in p.pcb.tracks.values() {
        out.push(CopperItem {
            kind: ItemKind::Track,
            object_id: t.id.clone(),
            sub: String::new(),
            layer: t.layer,
            net: t.net.clone(),
            shape: Shape {
                core: Core::Segment(t.a, t.b),
                radius: t.width as f64 / 2.0,
            },
        });
    }
    for v in p.pcb.vias.values() {
        for layer in v.layers(&p.pcb.board.stackup) {
            out.push(CopperItem {
                kind: ItemKind::Via,
                object_id: v.id.clone(),
                sub: String::new(),
                layer,
                net: v.net.clone(),
                shape: Shape {
                    core: Core::Segment(v.position, v.position),
                    radius: v.diameter as f64 / 2.0,
                },
            });
        }
    }
    out
}

/// Rasterised zone fill: zone polygon minus (other-net copper inflated by clearance), with
/// thermal reliefs around same-net pads (a copper-free gap bridged by up to four spokes).
#[derive(Debug, Clone)]
pub struct ZoneFill {
    pub zone_id: Id,
    pub layer: CopperLayer,
    pub net: String,
    pub origin: Point,
    pub pitch: Nm,
    pub cols: usize,
    pub rows: usize,
    /// Island index per cell (0 = empty, k = island k).
    pub cells: Vec<u32>,
    pub islands: u32,
    /// One entry per same-net pad on this layer that the zone surrounds or reaches.
    pub reliefs: Vec<ThermalRelief>,
}

impl ZoneFill {
    pub fn cell_center(&self, c: usize, r: usize) -> Point {
        Point::new(
            self.origin.x + c as Nm * self.pitch + self.pitch / 2,
            self.origin.y + r as Nm * self.pitch + self.pitch / 2,
        )
    }
    /// Islands touched by a shape on the same layer.
    pub fn islands_touching(&self, s: &Shape) -> BTreeSet<u32> {
        let mut out = BTreeSet::new();
        let bb = s.bbox();
        let c0 = ((bb.min.x - self.origin.x) / self.pitch).max(0) as usize;
        let r0 = ((bb.min.y - self.origin.y) / self.pitch).max(0) as usize;
        let c1 = (((bb.max.x - self.origin.x) / self.pitch) as usize + 1).min(self.cols);
        let r1 = (((bb.max.y - self.origin.y) / self.pitch) as usize + 1).min(self.rows);
        for r in r0..r1 {
            for c in c0..c1 {
                let k = self.cells[r * self.cols + c];
                if k != 0 && s.contains(self.cell_center(c, r)) {
                    out.insert(k);
                }
            }
        }
        out
    }
    pub fn filled_area_mm2(&self) -> f64 {
        let n = self.cells.iter().filter(|&&k| k != 0).count() as f64;
        n * (self.pitch as f64 / 1e6).powi(2)
    }
}

pub const ZONE_FILL_PITCH: Nm = 100_000; // 0.1 mm

/// Thermal-relief geometry applied to same-net pads inside a zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReliefRule {
    /// Copper-free ring kept between the pad and the surrounding fill.
    pub gap: Nm,
    /// Nominal width of each spoke.
    pub spoke_width: Nm,
}

/// Relief parameters used for zone `z`.
pub fn relief_rule(p: &Project, z: &Zone) -> ReliefRule {
    // shortcut: gap = zone clearance and spoke width = the zone net's class track width (never
    // below the minimum track width), because the EAK-IR `Zone` has no thermal-relief fields;
    // upgrade when an ADR adds per-zone/per-pad relief settings (gap, spoke width, count, solid).
    let c = &p.constraints;
    ReliefRule {
        gap: z.clearance.max(0),
        spoke_width: c.class_for(&z.net).track_width.max(c.min_track_width),
    }
}

/// Spoke directions, in order +X, +Y, -X, -Y (spokes are axis-aligned: pads rotate in
/// quarter turns only).
pub const SPOKE_DIRS: [(i64, i64); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spoke {
    pub dir: (i64, i64),
    /// True when the spoke's copper runs unbroken from the pad across the gap into zone fill.
    pub connected: bool,
    /// Narrowest copper width of the spoke across the gap as filled on the raster
    /// (0 when not connected).
    pub width: Nm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThermalRelief {
    pub footprint_id: Id,
    pub pad: String,
    pub center: Point,
    pub spokes: Vec<Spoke>,
}

impl ThermalRelief {
    pub fn connected_spokes(&self) -> usize {
        self.spokes.iter().filter(|s| s.connected).count()
    }
    /// Total copper width of the connected spokes.
    pub fn connected_width(&self) -> Nm {
        self.spokes
            .iter()
            .filter(|s| s.connected)
            .map(|s| s.width)
            .sum()
    }
}

fn core_center(core: &Core) -> Point {
    match core {
        Core::Segment(a, b) => Point::new((a.x + b.x) / 2, (a.y + b.y) / 2),
        Core::Polygon(poly) => Rect::from_points(poly)
            .map(|r| r.center())
            .unwrap_or_default(),
    }
}

/// Raster addressing shared by the fill passes.
struct Grid {
    origin: Point,
    pitch: Nm,
    cols: usize,
    rows: usize,
}

impl Grid {
    fn center(&self, c: usize, r: usize) -> Point {
        Point::new(
            self.origin.x + c as Nm * self.pitch + self.pitch / 2,
            self.origin.y + r as Nm * self.pitch + self.pitch / 2,
        )
    }
    /// Call `f` with the index of every cell whose centre lies inside `s`.
    fn mark(&self, s: &Shape, mut f: impl FnMut(usize)) {
        let ib = s.bbox();
        let p = self.pitch;
        let c0 = ((ib.min.x - self.origin.x) / p).max(0) as usize;
        let r0 = ((ib.min.y - self.origin.y) / p).max(0) as usize;
        let c1 = ((((ib.max.x - self.origin.x) / p).max(-1) + 1) as usize).min(self.cols);
        let r1 = ((((ib.max.y - self.origin.y) / p).max(-1) + 1) as usize).min(self.rows);
        for r in r0..r1 {
            for c in c0..c1 {
                if s.contains(self.center(c, r)) {
                    f(r * self.cols + c);
                }
            }
        }
    }
    fn cell_of(&self, pt: Point) -> Option<(i64, i64)> {
        let c = (pt.x - self.origin.x).div_euclid(self.pitch);
        let r = (pt.y - self.origin.y).div_euclid(self.pitch);
        self.inside(c, r).then_some((c, r))
    }
    fn inside(&self, c: i64, r: i64) -> bool {
        c >= 0 && r >= 0 && (c as usize) < self.cols && (r as usize) < self.rows
    }
    /// Walk a spoke's centre line from the pad centre outward. The spoke is connected when
    /// every cell inside the relief void is filled and the first cell past the void is
    /// filled zone copper. Returns (connected, narrowest raster width across the gap).
    fn trace_spoke(
        &self,
        center: Point,
        dir: (i64, i64),
        pad: &Shape,
        void: &Shape,
        filled: &dyn Fn(usize) -> bool,
    ) -> (bool, Option<Nm>) {
        let Some((mut c, mut r)) = self.cell_of(center) else {
            return (false, None);
        };
        let mut narrowest: Option<Nm> = None;
        while self.inside(c, r) {
            let i = r as usize * self.cols + c as usize;
            let pt = self.center(c as usize, r as usize);
            if !void.contains(pt) {
                return (filled(i), narrowest);
            }
            if !filled(i) {
                return (false, None);
            }
            if !pad.contains(pt) {
                // In the gap: count filled void cells across the spoke.
                let mut n = 1;
                for side in [1, -1] {
                    let (mut cc, mut rr) = (c + side * dir.1, r + side * dir.0);
                    while self.inside(cc, rr) {
                        let j = rr as usize * self.cols + cc as usize;
                        if !filled(j) || !void.contains(self.center(cc as usize, rr as usize)) {
                            break;
                        }
                        n += 1;
                        cc += side * dir.1;
                        rr += side * dir.0;
                    }
                }
                let w = n * self.pitch;
                narrowest = Some(narrowest.map_or(w, |m| m.min(w)));
            }
            c += dir.0;
            r += dir.1;
        }
        (false, None)
    }
}

pub fn fill_zone(p: &Project, zone_id: &str, copper: &[CopperItem]) -> Option<ZoneFill> {
    let z = p.pcb.zones.get(zone_id)?;
    let bb = Rect::from_points(&z.outline)?;
    let pitch = ZONE_FILL_PITCH;
    let grid = Grid {
        origin: bb.min,
        pitch,
        cols: (bb.width() / pitch) as usize + 1,
        rows: (bb.height() / pitch) as usize + 1,
    };
    let n = grid.cols * grid.rows;
    // A filled cell's copper reaches up to ~0.71 pitch from its centre, so every keep-out
    // is inflated by that much to keep the true clearance on the raster.
    let halo = pitch as f64 * 0.71;
    let board = &p.pcb.board.outline;
    let edge = p.constraints.board_edge_clearance as f64;
    // `allowed`: inside the zone and board-edge rule, clear of other-net copper.
    let mut allowed = vec![false; n];
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            let pt = grid.center(c, r);
            if !point_in_polygon(pt, &z.outline) {
                continue;
            }
            if board.len() >= 3
                && (!point_in_polygon(pt, board) || poly_boundary_dist(pt, board) < edge)
            {
                continue;
            }
            allowed[r * grid.cols + c] = true;
        }
    }
    for it in copper
        .iter()
        .filter(|i| i.layer == z.layer && i.net != z.net)
    {
        let keepout = Shape {
            core: it.shape.core.clone(),
            radius: it.shape.radius + z.clearance as f64 + halo,
        };
        grid.mark(&keepout, |i| allowed[i] = false);
    }
    // Thermal reliefs: a void of `gap` around every same-net pad, bridged by spokes.
    let rule = relief_rule(p, z);
    let pads: Vec<(&CopperItem, Point, Shape)> = copper
        .iter()
        .filter(|i| {
            i.kind == ItemKind::Pad && i.layer == z.layer && i.net == z.net && !z.net.is_empty()
        })
        .map(|pad| {
            let void = Shape {
                core: pad.shape.core.clone(),
                radius: pad.shape.radius + rule.gap as f64 + halo,
            };
            (pad, core_center(&pad.shape.core), void)
        })
        .collect();
    let mut in_void = vec![false; n];
    for (_, _, void) in &pads {
        grid.mark(void, |i| in_void[i] = true);
    }
    let spoke_shape = |center: Point, void: &Shape, dir: (i64, i64)| {
        let vb = void.bbox();
        let reach = match dir {
            (1, _) => vb.max.x - center.x,
            (-1, _) => center.x - vb.min.x,
            (_, 1) => vb.max.y - center.y,
            _ => center.y - vb.min.y,
        };
        Shape {
            core: Core::Segment(
                center,
                Point::new(center.x + dir.0 * reach, center.y + dir.1 * reach),
            ),
            // Paint every cell the nominal spoke overlaps, so the raster (fabricated) spoke
            // is never narrower than nominal (and less than two pitches wider).
            radius: (rule.spoke_width + pitch) as f64 / 2.0 - 1.0,
        }
    };
    // Start with every spoke, then drop spokes that do not reach fill (no dangling stubs)
    // until the set is stable; removing a stub can only disconnect, so this terminates.
    let mut active: Vec<Vec<bool>> = vec![vec![true; SPOKE_DIRS.len()]; pads.len()];
    let (filled, reliefs) = loop {
        let mut spoke_mask = vec![false; n];
        for ((_, center, void), on) in pads.iter().zip(&active) {
            for (dir, _) in SPOKE_DIRS.iter().zip(on).filter(|(_, on)| **on) {
                grid.mark(&spoke_shape(*center, void, *dir), |i| spoke_mask[i] = true);
            }
        }
        let filled: Vec<bool> = (0..n)
            .map(|i| allowed[i] && (!in_void[i] || spoke_mask[i]))
            .collect();
        let is_filled = |i: usize| filled[i];
        let mut next = active.clone();
        let mut reliefs = Vec::new();
        for (k, (pad, center, void)) in pads.iter().enumerate() {
            let mut spokes = Vec::new();
            for (d, dir) in SPOKE_DIRS.iter().enumerate() {
                let (connected, narrowest) = if active[k][d] {
                    grid.trace_spoke(*center, *dir, &pad.shape, void, &is_filled)
                } else {
                    (false, None)
                };
                next[k][d] = connected;
                spokes.push(Spoke {
                    dir: *dir,
                    connected,
                    // No raster cell inside the gap: the junction is as wide as the spoke.
                    width: if connected {
                        narrowest.unwrap_or(rule.spoke_width)
                    } else {
                        0
                    },
                });
            }
            if spokes.iter().any(|s| s.connected) || point_in_polygon(*center, &z.outline) {
                reliefs.push(ThermalRelief {
                    footprint_id: pad.object_id.clone(),
                    pad: pad.sub.clone(),
                    center: *center,
                    spokes,
                });
            }
        }
        if next == active {
            break (filled, reliefs);
        }
        active = next;
    };
    let mut cells: Vec<u32> = filled
        .iter()
        .map(|&f| if f { u32::MAX } else { 0 })
        .collect();
    // Label islands (4-connectivity flood fill).
    let cols = grid.cols;
    let rows = grid.rows;
    let mut islands = 0u32;
    let mut stack = Vec::new();
    for start in 0..cells.len() {
        if cells[start] != u32::MAX {
            continue;
        }
        islands += 1;
        cells[start] = islands;
        stack.push(start);
        while let Some(i) = stack.pop() {
            let (r, c) = (i / cols, i % cols);
            let mut push = |j: usize| {
                if cells[j] == u32::MAX {
                    cells[j] = islands;
                    stack.push(j);
                }
            };
            if c > 0 {
                push(i - 1);
            }
            if c + 1 < cols {
                push(i + 1);
            }
            if r > 0 {
                push(i - cols);
            }
            if r + 1 < rows {
                push(i + cols);
            }
        }
    }
    Some(ZoneFill {
        zone_id: z.id.clone(),
        layer: z.layer,
        net: z.net.clone(),
        origin: bb.min,
        pitch,
        cols,
        rows,
        cells,
        islands,
        reliefs,
    })
}

/// Result of physical connectivity analysis.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Connectivity {
    /// For each net: number of physically separate copper groups containing its pads
    /// (1 = fully connected, >1 = unrouted connections remain).
    pub net_groups: BTreeMap<String, usize>,
    /// Pairs of different nets whose copper touches (shorts): (net a, net b, item a, item b).
    pub shorts: Vec<(String, String, String, String)>,
    /// Ratsnest: unrouted pad-to-pad connections per net (from, to) in nm.
    pub ratsnest: Vec<(String, Point, Point)>,
}

struct Dsu(Vec<usize>);
impl Dsu {
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.0[r] != r {
            r = self.0[r];
        }
        self.0[x] = r;
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra] = rb;
        }
    }
}

pub fn analyse(p: &Project) -> Connectivity {
    let items = items(p);
    let fills: Vec<ZoneFill> = p
        .pcb
        .zones
        .keys()
        .filter_map(|z| fill_zone(p, z, &items))
        .collect();
    // Node layout: items, then one node per (fill, island).
    let mut island_node: BTreeMap<(usize, u32), usize> = BTreeMap::new();
    let mut n = items.len();
    for (fi, f) in fills.iter().enumerate() {
        for k in 1..=f.islands {
            island_node.insert((fi, k), n);
            n += 1;
        }
    }
    let mut dsu = Dsu((0..n).collect());
    let mut shorts = BTreeSet::new();
    // Same object across layers (THT pads, vias) is one conductor.
    let mut by_obj: BTreeMap<(ItemKind, &str, &str), usize> = BTreeMap::new();
    for (i, it) in items.iter().enumerate() {
        let key = (it.kind, it.object_id.as_str(), it.sub.as_str());
        if let Some(&j) = by_obj.get(&key) {
            dsu.union(i, j);
        } else {
            by_obj.insert(key, i);
        }
    }
    // Geometric contact on the same layer (bounding-box prefilter).
    let boxes: Vec<Rect> = items.iter().map(|i| i.shape.bbox()).collect();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| boxes[i].min.x);
    for (oi, &i) in order.iter().enumerate() {
        for &j in &order[oi + 1..] {
            if boxes[j].min.x > boxes[i].max.x {
                break;
            }
            let (a, b) = (&items[i], &items[j]);
            if a.layer != b.layer || !boxes[i].inflate(1).intersects(&boxes[j]) {
                continue;
            }
            if a.kind == b.kind && a.object_id == b.object_id && a.sub == b.sub {
                continue;
            }
            if distance(&a.shape, &b.shape) <= 0.0 {
                if a.net != b.net && !a.net.is_empty() && !b.net.is_empty() {
                    let (x, y) = if a.net < b.net { (a, b) } else { (b, a) };
                    shorts.insert((x.net.clone(), y.net.clone(), x.label(), y.label()));
                } else {
                    dsu.union(i, j);
                }
            }
        }
    }
    // Zone islands connect same-net items they touch.
    for (fi, f) in fills.iter().enumerate() {
        for (i, it) in items.iter().enumerate() {
            if it.layer != f.layer || it.net != f.net {
                continue;
            }
            for k in f.islands_touching(&it.shape) {
                dsu.union(i, island_node[&(fi, k)]);
            }
        }
    }
    // Count pad groups per net and compute ratsnest between groups (nearest pads).
    let mut net_pads: BTreeMap<String, Vec<(usize, Point)>> = BTreeMap::new();
    for (i, it) in items.iter().enumerate() {
        if it.kind == ItemKind::Pad && !it.net.is_empty() {
            let c = core_center(&it.shape.core);
            net_pads.entry(it.net.clone()).or_default().push((i, c));
        }
    }
    let mut conn = Connectivity {
        shorts: shorts.into_iter().collect(),
        ..Default::default()
    };
    for (net, pads) in &net_pads {
        let mut groups: BTreeMap<usize, Vec<Point>> = BTreeMap::new();
        for (i, c) in pads {
            let r = dsu.find(*i);
            groups.entry(r).or_default().push(*c);
        }
        conn.net_groups.insert(net.clone(), groups.len());
        // Prim over groups for a minimal ratsnest.
        let gs: Vec<Vec<Point>> = groups.into_values().collect();
        if gs.len() > 1 {
            let mut in_tree = vec![false; gs.len()];
            in_tree[0] = true;
            for _ in 1..gs.len() {
                let mut best: Option<(f64, usize, Point, Point)> = None;
                for (a, ga) in gs.iter().enumerate().filter(|(a, _)| in_tree[*a]) {
                    let _ = a;
                    for (b, gb) in gs.iter().enumerate().filter(|(b, _)| !in_tree[*b]) {
                        for pa in ga {
                            for pb in gb {
                                let d = pa.dist(*pb);
                                if best.is_none_or(|x| d < x.0) {
                                    best = Some((d, b, *pa, *pb));
                                }
                            }
                        }
                    }
                }
                if let Some((_, b, pa, pb)) = best {
                    in_tree[b] = true;
                    conn.ratsnest.push((net.clone(), pa, pb));
                }
            }
        }
    }
    conn
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::{FootprintInstance, Track};

    /// A 20 × 20 mm board with a bottom GND zone and one single-pad THT footprint per entry
    /// of `pads` (1.7 mm square pad, 1.0 mm drill) on net GND.
    fn board(pads: &[Point], zone: &[Point]) -> Project {
        let mut p = Project::new("relief");
        let mut def = eak_core::builtin::get("CONN_1x02_P2.54").unwrap();
        def.footprint.pads.truncate(1);
        def.footprint.pads[0].position = Point::default();
        p.library.insert(def.id.clone(), def.clone());
        p.pcb.board.outline = vec![
            Point::mm(0.0, 0.0),
            Point::mm(20.0, 0.0),
            Point::mm(20.0, 20.0),
            Point::mm(0.0, 20.0),
        ];
        for (k, at) in pads.iter().enumerate() {
            let id = format!("fp{k}");
            p.pcb.footprints.insert(
                id.clone(),
                FootprintInstance {
                    id,
                    symbol_id: format!("sym{k}"),
                    component_id: def.id.clone(),
                    refdes: format!("J{}", k + 1),
                    position: *at,
                    rotation: Rotation::R0,
                    side: Side::Top,
                    pad_nets: [("1".to_string(), "GND".to_string())].into(),
                    locked: false,
                },
            );
        }
        p.pcb.zones.insert(
            "z".into(),
            Zone {
                id: "z".into(),
                layer: CopperLayer::Bottom,
                net: "GND".into(),
                outline: zone.to_vec(),
                clearance: 300_000,
            },
        );
        p
    }

    fn full_zone() -> Vec<Point> {
        vec![
            Point::mm(1.0, 1.0),
            Point::mm(19.0, 1.0),
            Point::mm(19.0, 19.0),
            Point::mm(1.0, 19.0),
        ]
    }

    fn fill(p: &Project) -> ZoneFill {
        fill_zone(p, "z", &items(p)).unwrap()
    }

    fn cell_at(f: &ZoneFill, pt: Point) -> u32 {
        let c = ((pt.x - f.origin.x) / f.pitch) as usize;
        let r = ((pt.y - f.origin.y) / f.pitch) as usize;
        f.cells[r * f.cols + c]
    }

    #[test]
    fn same_net_pad_gets_four_spokes_and_a_relief_gap() {
        let p = board(&[Point::mm(10.0, 10.0)], &full_zone());
        let f = fill(&p);
        let rule = relief_rule(&p, &p.pcb.zones["z"]);
        assert_eq!(f.reliefs.len(), 1);
        let rel = &f.reliefs[0];
        assert_eq!((rel.footprint_id.as_str(), rel.pad.as_str()), ("fp0", "1"));
        assert_eq!(rel.connected_spokes(), 4);
        for s in &rel.spokes {
            assert!(s.width >= rule.spoke_width, "{s:?}");
            assert!(s.width < rule.spoke_width + 2 * ZONE_FILL_PITCH, "{s:?}");
        }
        // The diagonal of the gap is copper-free; the spoke axis and outer fill are copper.
        assert_eq!(cell_at(&f, Point::mm(11.05, 11.05)), 0);
        assert_eq!(cell_at(&f, Point::mm(8.95, 8.95)), 0);
        assert_ne!(cell_at(&f, Point::mm(11.05, 10.0)), 0);
        assert_ne!(cell_at(&f, Point::mm(10.0, 8.95)), 0);
        assert_ne!(cell_at(&f, Point::mm(12.0, 12.0)), 0);
        // One island: the pad is joined to the pour through its spokes.
        assert_eq!(f.islands, 1);
        assert_eq!(f.islands_touching(&items(&p)[0].shape).len(), 1);
    }

    #[test]
    fn blocked_spoke_is_removed_and_connectivity_follows_the_spokes() {
        let mut p = board(&[Point::mm(10.0, 10.0), Point::mm(4.0, 10.0)], &full_zone());
        // An other-net track just past the +X relief gap of J1 blocks that spoke's landing.
        p.pcb.tracks.insert(
            "t".into(),
            Track {
                id: "t".into(),
                layer: CopperLayer::Bottom,
                width: 250_000,
                a: Point::mm(11.6, 8.5),
                b: Point::mm(11.6, 11.5),
                net: "SIG".into(),
            },
        );
        let f = fill(&p);
        let rel = f.reliefs.iter().find(|r| r.footprint_id == "fp0").unwrap();
        let conn: Vec<bool> = rel.spokes.iter().map(|s| s.connected).collect();
        assert_eq!(conn, vec![false, true, true, true]);
        assert_eq!(rel.spokes[0].width, 0);
        // No dangling stub is left in the gap of the removed spoke.
        assert_eq!(cell_at(&f, Point::mm(11.05, 10.0)), 0);
        // Both GND pads still join through the remaining spokes and the pour.
        assert_eq!(analyse(&p).net_groups["GND"], 1);
    }

    #[test]
    fn pad_without_landing_fill_has_no_connected_spokes() {
        // The zone ends left of the pad: no spoke can land, so the pad is not connected.
        let zone = vec![
            Point::mm(1.0, 1.0),
            Point::mm(8.0, 1.0),
            Point::mm(8.0, 19.0),
            Point::mm(1.0, 19.0),
        ];
        let p = board(&[Point::mm(10.0, 10.0), Point::mm(4.0, 10.0)], &zone);
        let f = fill(&p);
        assert!(f.reliefs.iter().all(|r| r.footprint_id != "fp0"));
        assert_eq!(analyse(&p).net_groups["GND"], 2);
        // Pads at a zone edge keep only the spokes that land in fill.
        let zone = vec![
            Point::mm(9.0, 9.6),
            Point::mm(19.0, 9.6),
            Point::mm(19.0, 10.4),
            Point::mm(9.0, 10.4),
        ];
        let p = board(&[Point::mm(10.0, 10.0)], &zone);
        let f = fill(&p);
        let conn: Vec<bool> = f.reliefs[0].spokes.iter().map(|s| s.connected).collect();
        assert_eq!(conn, vec![true, false, false, false]);
    }
}
