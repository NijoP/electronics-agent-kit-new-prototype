//! Native two-layer grid router (EAK-owned; no Freerouting dependency).
//!
//! Algorithm: each layer is rasterised into an ownership grid where a cell records which
//! net's copper (inflated by clearance + half track width + a discretisation margin)
//! covers it. A* with 8-way moves and layer changes (vias) then connects each net's
//! pads along a minimum spanning tree. Every produced segment is re-checked with exact
//! geometry before it is accepted; failures are reported, never hidden.

use crate::copper::{distance, items, Core, ItemKind, Shape};
use eak_core::geom::{point_in_polygon, point_segment_distance};
use eak_core::{CopperLayer, Nm, Point, Project, Rect};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteResult {
    pub tracks: Vec<RoutedTrack>,
    pub vias: Vec<RoutedVia>,
    pub routed_connections: usize,
    pub failed_connections: Vec<(String, Point, Point, String)>,
    pub grid_pitch: Nm,
}

const FREE: u32 = 0;
const MULTI: u32 = u32::MAX;

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
}

impl Default for RouterOptions {
    fn default() -> Self {
        RouterOptions {
            pitch: 250_000,
            nets: None,
            via_cost: 40,
            max_expansions: 2_000_000,
        }
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
    let mut grid = Grid {
        origin: bb.min,
        pitch,
        cols: (bb.width() / pitch) as usize + 1,
        rows: (bb.height() / pitch) as usize + 1,
        owner: [vec![], vec![]],
    };
    let n = grid.cols * grid.rows;
    grid.owner = [vec![FREE; n], vec![FREE; n]];

    // Net ids.
    let copper = items(p);
    let mut net_ids: BTreeMap<String, u32> = BTreeMap::new();
    for it in &copper {
        if !it.net.is_empty() {
            let next = net_ids.len() as u32 + 1;
            net_ids.entry(it.net.clone()).or_insert(next);
        }
    }
    let max_width = std::iter::once(&p.constraints.default_class)
        .chain(p.constraints.net_classes.iter())
        .map(|c| c.track_width)
        .max()
        .unwrap_or(250_000);
    let max_clear = std::iter::once(&p.constraints.default_class)
        .chain(p.constraints.net_classes.iter())
        .map(|c| c.clearance)
        .max()
        .unwrap_or(200_000)
        .max(p.constraints.min_clearance);
    let margin = pitch as f64 * 0.5;
    let inflate = max_clear as f64 + max_width as f64 / 2.0 + margin;

    // Board edge keep-out.
    let edge = p.constraints.board_edge_clearance as f64 + max_width as f64 / 2.0;
    for r in 0..grid.rows {
        for c in 0..grid.cols {
            let pt = grid.center(c, r);
            let mut d = f64::INFINITY;
            for i in 0..outline.len() {
                d = d.min(point_segment_distance(
                    pt,
                    outline[i],
                    outline[(i + 1) % outline.len()],
                ));
            }
            if !point_in_polygon(pt, outline) || d < edge {
                let i = grid.idx(c, r);
                grid.owner[0][i] = MULTI;
                grid.owner[1][i] = MULTI;
            }
        }
    }
    // Existing copper. Unassigned-net pads block everyone.
    for it in &copper {
        let id = net_ids.get(&it.net).copied().unwrap_or(MULTI);
        if let Some(l) = layer_index(it.layer) {
            grid.mark(l, &it.shape, inflate, id);
        }
    }

    // Gather pad terminals per net, plus existing connectivity groups.
    let conn = crate::copper::analyse(p);
    let mut todo: Vec<(String, Point, Point)> = conn.ratsnest.clone();
    if let Some(only) = &opts.nets {
        todo.retain(|(n, _, _)| only.contains(n));
    }
    // Most constrained nets first (power/ground, high current, many terminals), then shortest.
    crate::priority::order_connections(p, &mut todo);
|a, b| {
        a.1.dist(a.2)
            .partial_cmp(&b.1.dist(b.2))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // SMD pad layer lookup for terminals.
    let pad_layers = |pt: Point, net: &str| -> Vec<usize> {
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
    };

    let mut placed_shapes: Vec<(usize, Shape, String)> = Vec::new();
    for (net, a, b) in todo {
        let id = net_ids[&net];
        let class = p.constraints.class_for(&net);
        let width = class.track_width;
        let la = pad_layers(a, &net);
        let lb = pad_layers(b, &net);
        match astar(&grid, id, a, &la, b, &lb, opts) {
            Some(path) => {
                // Convert to segments: stubs from pad centres, collinear merge, vias.
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
                // Exact clearance verification against existing other-net copper.
                let clearance = class.clearance.max(p.constraints.min_clearance) as f64;
                let mut violation = None;
                let new_shapes: Vec<(usize, Shape)> = segs
                    .iter()
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
                                    radius: class.via_diameter as f64 / 2.0,
                                },
                            )
                        })
                    }))
                    .collect();
                'check: for (l, s) in &new_shapes {
                    for it in copper
                        .iter()
                        .filter(|i| layer_index(i.layer) == Some(*l) && i.net != net)
                    {
                        if distance(s, &it.shape) < clearance - 1.0 {
                            violation =
                                Some(format!("clearance to {} would be violated", it.label()));
                            break 'check;
                        }
                    }
                    for (pl, ps, pn) in &placed_shapes {
                        if pl == l && pn != &net && distance(s, ps) < clearance - 1.0 {
                            violation =
                                Some("clearance to a newly routed track would be violated".into());
                            break 'check;
                        }
                    }
                }
                if let Some(v) = violation {
                    result.failed_connections.push((net.clone(), a, b, v));
                    continue;
                }
                for (l, s) in &new_shapes {
                    grid.mark(*l, s, inflate - width as f64 / 2.0, id);
                    placed_shapes.push((*l, s.clone(), net.clone()));
                }
                for (l, s, e) in segs {
                    result.tracks.push(RoutedTrack {
                        layer: layer_of(l),
                        a: s,
                        b: e,
                        width,
                        net: net.clone(),
                    });
                }
                for v in vias {
                    result.vias.push(RoutedVia {
                        position: v,
                        drill: class.via_drill,
                        diameter: class.via_diameter,
                        net: net.clone(),
                    });
                }
                result.routed_connections += 1;
            }
            None => result.failed_connections.push((
                net.clone(),
                a,
                b,
                "no path found within clearance constraints".into(),
            )),
        }
    }
    result
}

fn astar(
    grid: &Grid,
    net: u32,
    a: Point,
    la: &[usize],
    b: Point,
    lb: &[usize],
    opts: &RouterOptions,
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
            relax(state(l, ni), if diag { 14 } else { 10 }, &mut heap);
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
            relax(state(ol, i), opts.via_cost, &mut heap);
        }
    }
    None
}
