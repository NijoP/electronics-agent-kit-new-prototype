//! PCB copper model: exact shapes for pads, tracks and vias, shape-to-shape distance,
//! rasterised zone fill and physical copper connectivity.

use eak_core::geom::{point_in_polygon, point_segment_distance, segment_segment_distance};
use eak_core::{CopperLayer, Id, Nm, PadKind, PadShape, Point, Project, Rect, Rotation, Side};
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
                PadKind::Tht => CopperLayer::ALL.to_vec(),
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
        for layer in CopperLayer::ALL {
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

/// Rasterised zone fill: zone polygon minus (other-net copper inflated by clearance).
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

pub fn fill_zone(p: &Project, zone_id: &str, copper: &[CopperItem]) -> Option<ZoneFill> {
    let z = p.pcb.zones.get(zone_id)?;
    let bb = Rect::from_points(&z.outline)?;
    let pitch = ZONE_FILL_PITCH;
    let cols = (bb.width() / pitch) as usize + 1;
    let rows = (bb.height() / pitch) as usize + 1;
    let mut cells = vec![0u32; cols * rows];
    let board = &p.pcb.board.outline;
    let edge = p.constraints.board_edge_clearance as f64;
    for r in 0..rows {
        for c in 0..cols {
            let pt = Point::new(
                bb.min.x + c as Nm * pitch + pitch / 2,
                bb.min.y + r as Nm * pitch + pitch / 2,
            );
            if !point_in_polygon(pt, &z.outline) {
                continue;
            }
            if board.len() >= 3
                && (!point_in_polygon(pt, board) || poly_boundary_dist(pt, board) < edge)
            {
                continue;
            }
            cells[r * cols + c] = u32::MAX; // tentatively filled
        }
    }
    // Knock out clearance around other-net copper on this layer.
    for it in copper
        .iter()
        .filter(|i| i.layer == z.layer && i.net != z.net)
    {
        let inflated = Shape {
            core: it.shape.core.clone(),
            radius: it.shape.radius + z.clearance as f64 + pitch as f64 * 0.71,
        };
        let ib = inflated.bbox();
        let c0 = ((ib.min.x - bb.min.x) / pitch).max(0) as usize;
        let r0 = ((ib.min.y - bb.min.y) / pitch).max(0) as usize;
        let c1 = ((((ib.max.x - bb.min.x) / pitch).max(-1) + 1) as usize).min(cols);
        let r1 = ((((ib.max.y - bb.min.y) / pitch).max(-1) + 1) as usize).min(rows);
        for r in r0..r1 {
            for c in c0..c1 {
                let pt = Point::new(
                    bb.min.x + c as Nm * pitch + pitch / 2,
                    bb.min.y + r as Nm * pitch + pitch / 2,
                );
                if inflated.contains(pt) {
                    cells[r * cols + c] = 0;
                }
            }
        }
    }
    // Label islands (4-connectivity flood fill).
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
            let c = match &it.shape.core {
                Core::Segment(a, b) => Point::new((a.x + b.x) / 2, (a.y + b.y) / 2),
                Core::Polygon(poly) => Rect::from_points(poly)
                    .map(|r| r.center())
                    .unwrap_or_default(),
            };
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
