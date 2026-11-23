//! Zone stitching for the `StitchZones` command: the DESK `tools/stitch_orphans.py` method
//! (MIT, `third_party/licenses/DESK/`), re-implemented in Rust over the one EAK-IR.
//!
//! DESK pours the ground net on every copper layer, then:
//! 1. gives every SMD pad of the zone net that its own layer's fill does not reach an escape
//!    via found by a ring search (0.6-3.0 mm, 24 angles) plus a pad-to-via track, so the pad
//!    joins the pour on another layer (`stitch_orphans.py:79-90`);
//! 2. gives every fill island with no via of the net one stitching via at its deepest point
//!    (`stitch_orphans.py:55-61`, "pole of inaccessibility");
//! 3. refills, dropping islands no copper of the net touches (`copper::fill_zone`).
//!
//! EAK differences, all toward safety: a via is only placed where it lands in a fill island of
//! the same net on another layer (a via into nothing would only keep dead copper alive), a
//! stitching via must land in an island that already touches the net's copper, every via and
//! track is checked against all other-net copper with exact geometry at the net-class clearance,
//! vias never touch pads (no via-in-pad), and DESK's 10 mm bottom-layer link tracks are not
//! drawn (the refilled pour joins the via). The result is a proposal applied as one typed
//! command; connectivity is then judged by `eak-validation` (DRC-004/013), never here.

use crate::copper::{
    core_center, distance, fill_zone_raw, items, poly_boundary_dist, CopperItem, Core, ItemKind,
    Shape, ZoneFill,
};
use crate::router::{RoutedTrack, RoutedVia};
use crate::routing_intel::net_spec;
use eak_core::geom::point_in_polygon;
use eak_core::{CopperLayer, Point, Project};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Ring search for a pad's escape via (DESK `stitch_orphans.py`: 0.6-3.0 mm, 24 angles).
pub const RING_MIN_MM: f64 = 0.6;
pub const RING_MAX_MM: f64 = 3.0;
pub const RING_STEP_MM: f64 = 0.2;
pub const RING_ANGLES: usize = 24;
/// Deepest cells of an island tried as stitching-via positions.
const ISLAND_CANDIDATES: usize = 400;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StitchPlan {
    pub vias: Vec<RoutedVia>,
    pub tracks: Vec<RoutedTrack>,
    /// SMD pads that got an escape via and a pad-to-via track.
    pub pads_stitched: usize,
    /// Fill islands that got a stitching via.
    pub islands_stitched: usize,
    /// What could not be stitched and why (those islands are removed as dead copper on refill).
    pub notes: Vec<String>,
}

/// New copper this plan has placed: `(net, layer, shape)`, layer `None` for a through via.
type Placed = (String, Option<CopperLayer>, Shape);

struct Ctx<'a> {
    p: &'a Project,
    copper: &'a [CopperItem],
    placed: Vec<Placed>,
}

impl Ctx<'_> {
    fn inside_board(&self, pt: Point, keep: f64) -> bool {
        let b = &self.p.pcb.board.outline;
        b.len() >= 3
            && point_in_polygon(pt, b)
            && poly_boundary_dist(pt, b) >= self.p.constraints.board_edge_clearance as f64 + keep
    }

    /// A through via of `net` at `c`: clear of every pad, via and other-net copper on every
    /// layer, and of the board edge. Same-net tracks may touch it.
    fn via_ok(&self, net: &str, c: Point, r: f64, cl: f64) -> bool {
        if !self.inside_board(c, r) {
            return false;
        }
        let s = Shape {
            core: Core::Segment(c, c),
            radius: r,
        };
        self.copper
            .iter()
            .filter(|it| !(it.net == net && it.kind == ItemKind::Track))
            .all(|it| distance(&s, &it.shape) >= cl)
            && self
                .placed
                .iter()
                .filter(|(n, l, _)| !(n == net && l.is_some()))
                .all(|(_, _, o)| distance(&s, o) >= cl)
    }

    /// A track of `net` on `layer` from `a` to `b`: clear of other-net copper on that layer.
    fn track_ok(&self, net: &str, layer: CopperLayer, a: Point, b: Point, w: f64, cl: f64) -> bool {
        let mid = Point::new((a.x + b.x) / 2, (a.y + b.y) / 2);
        if !self.inside_board(b, w / 2.0) || !self.inside_board(mid, w / 2.0) {
            return false;
        }
        let s = Shape {
            core: Core::Segment(a, b),
            radius: w / 2.0,
        };
        self.copper
            .iter()
            .filter(|it| it.layer == layer && it.net != net)
            .all(|it| distance(&s, &it.shape) >= cl)
            && self
                .placed
                .iter()
                .filter(|(n, l, _)| n != net && l.is_none_or(|l| l == layer))
                .all(|(_, _, o)| distance(&s, o) >= cl)
    }
}

fn cell_value(f: &ZoneFill, pt: Point) -> u32 {
    if pt.x < f.origin.x || pt.y < f.origin.y {
        return 0;
    }
    let c = ((pt.x - f.origin.x) / f.pitch) as usize;
    let r = ((pt.y - f.origin.y) / f.pitch) as usize;
    if c >= f.cols || r >= f.rows {
        return 0;
    }
    f.cells[r * f.cols + c]
}

/// Cells from each filled cell to the nearest empty cell or grid border (4-connected BFS).
/// Islands are never 4-adjacent, so one pass gives each island's own depth map.
fn depth(f: &ZoneFill) -> Vec<u32> {
    let n = f.cells.len();
    let mut d = vec![u32::MAX; n];
    let mut q = VecDeque::new();
    for (i, &k) in f.cells.iter().enumerate() {
        let (r, c) = (i / f.cols, i % f.cols);
        if k == 0 {
            d[i] = 0;
            q.push_back(i);
        } else if r == 0 || c == 0 || r + 1 == f.rows || c + 1 == f.cols {
            d[i] = 1;
            q.push_back(i);
        }
    }
    while let Some(i) = q.pop_front() {
        let (r, c) = (i / f.cols, i % f.cols);
        let next = d[i] + 1;
        let mut visit = |j: usize| {
            if d[j] == u32::MAX {
                d[j] = next;
                q.push_back(j);
            }
        };
        if c > 0 {
            visit(i - 1);
        }
        if c + 1 < f.cols {
            visit(i + 1);
        }
        if r > 0 {
            visit(i - f.cols);
        }
        if r + 1 < f.rows {
            visit(i + f.cols);
        }
    }
    d
}

/// Islands of `f` touched by copper of the fill's net on its layer (original or placed).
fn live_islands(f: &ZoneFill, copper: &[CopperItem], placed: &[Placed]) -> BTreeSet<u32> {
    let mut live = BTreeSet::new();
    for it in copper
        .iter()
        .filter(|it| it.layer == f.layer && it.net == f.net)
    {
        live.extend(f.islands_touching(&it.shape));
    }
    for (n, l, s) in placed {
        if *n == f.net && l.is_none_or(|l| l == f.layer) {
            live.extend(f.islands_touching(s));
        }
    }
    live
}

/// Size (cells) of the island of `f` at `pt`; 0 when `pt` is not in fill.
fn island_size_at(f: &ZoneFill, sizes: &[usize], pt: Point) -> usize {
    match cell_value(f, pt) {
        0 => 0,
        k => sizes[k as usize],
    }
}

/// True when a same-net track already joins `pad` to a same-net via (an earlier escape).
fn escaped(copper: &[CopperItem], pad: &CopperItem) -> bool {
    let via_at = |pt: Point| {
        copper.iter().any(|v| {
            v.kind == ItemKind::Via
                && v.net == pad.net
                && v.layer == pad.layer
                && v.shape.contains(pt)
        })
    };
    copper.iter().any(|t| {
        if t.kind != ItemKind::Track || t.net != pad.net || t.layer != pad.layer {
            return false;
        }
        let Core::Segment(a, b) = &t.shape.core else {
            return false;
        };
        (pad.shape.contains(*a) && via_at(*b)) || (pad.shape.contains(*b) && via_at(*a))
    })
}

/// Plan escape and stitching vias for the zones of `net` (every zoned net when `None`).
pub fn plan(p: &Project, net: Option<&str>) -> StitchPlan {
    let copper = items(p);
    let mut out = StitchPlan::default();
    let nets: BTreeSet<String> = p
        .pcb
        .zones
        .values()
        .filter(|z| net.is_none_or(|n| z.net == n))
        .map(|z| z.net.clone())
        .collect();
    // Pads that exist on more than one layer (THT) already join every layer's pour.
    let mut pad_layers: BTreeMap<(String, String), usize> = BTreeMap::new();
    for it in copper.iter().filter(|it| it.kind == ItemKind::Pad) {
        *pad_layers
            .entry((it.object_id.clone(), it.sub.clone()))
            .or_default() += 1;
    }
    let layers_of = |it: &CopperItem| {
        pad_layers
            .get(&(it.object_id.clone(), it.sub.clone()))
            .copied()
            .unwrap_or(1)
    };
    let mut ctx = Ctx {
        p,
        copper: &copper,
        placed: vec![],
    };
    for net in &nets {
        let fills: Vec<ZoneFill> = p
            .pcb
            .zones
            .values()
            .filter(|z| &z.net == net)
            .filter_map(|z| fill_zone_raw(p, &z.id, &copper))
            .collect();
        let layers: BTreeSet<CopperLayer> = fills.iter().map(|f| f.layer).collect();
        if layers.len() < 2 {
            out.notes.push(format!(
                "{net} is poured on {} layer(s) only: escape and stitching vias need a {net} zone on another layer to land in (DESK pours ground on every copper layer; add one with AddZone)",
                layers.len()
            ));
            continue;
        }
        let spec = net_spec(p, net);
        let cl = spec.clearance.max(p.constraints.min_clearance) as f64;
        let r = spec.via_diameter as f64 / 2.0;
        let w = spec.track_width as f64;
        let sizes: Vec<Vec<usize>> = fills
            .iter()
            .map(|f| {
                let mut s = vec![0usize; f.islands as usize + 1];
                for &k in &f.cells {
                    s[k as usize] += 1;
                }
                s
            })
            .collect();
        let via_at = |ctx: &mut Ctx, out: &mut StitchPlan, c: Point| {
            ctx.placed.push((
                net.clone(),
                None,
                Shape {
                    core: Core::Segment(c, c),
                    radius: r,
                },
            ));
            out.vias.push(RoutedVia {
                position: c,
                drill: spec.via_drill,
                diameter: spec.via_diameter,
                net: net.clone(),
            });
        };

        // 1. Escape vias for SMD pads the fill of their own layer does not reach.
        let pads: Vec<&CopperItem> = copper
            .iter()
            .filter(|it| &it.net == net && it.kind == ItemKind::Pad && layers_of(it) == 1)
            .collect();
        let rings = ((RING_MAX_MM - RING_MIN_MM) / RING_STEP_MM).round() as usize;
        for pad in pads {
            let reached = fills
                .iter()
                .filter(|f| f.layer == pad.layer)
                .any(|f| !f.islands_touching(&pad.shape).is_empty());
            if reached || escaped(&copper, pad) {
                continue;
            }
            let c0 = core_center(&pad.shape.core);
            // Smallest ring with a legal position; on that ring, the largest landing island.
            let mut found: Option<(usize, Point)> = None;
            for ring in 0..=rings {
                let rad = (RING_MIN_MM + ring as f64 * RING_STEP_MM) * 1e6;
                for a in 0..RING_ANGLES {
                    let t = a as f64 * std::f64::consts::TAU / RING_ANGLES as f64;
                    let c = Point::new(
                        c0.x + (rad * t.cos()).round() as i64,
                        c0.y + (rad * t.sin()).round() as i64,
                    );
                    let land = fills
                        .iter()
                        .zip(&sizes)
                        .filter(|(f, _)| f.layer != pad.layer)
                        .map(|(f, s)| island_size_at(f, s, c))
                        .max()
                        .unwrap_or(0);
                    if land == 0 || found.is_some_and(|(best, _)| best >= land) {
                        continue;
                    }
                    if ctx.via_ok(net, c, r, cl) && ctx.track_ok(net, pad.layer, c0, c, w, cl) {
                        found = Some((land, c));
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            match found {
                Some((_, c)) => {
                    ctx.placed.push((
                        net.clone(),
                        Some(pad.layer),
                        Shape {
                            core: Core::Segment(c0, c),
                            radius: w / 2.0,
                        },
                    ));
                    out.tracks.push(RoutedTrack {
                        layer: pad.layer,
                        a: c0,
                        b: c,
                        width: spec.track_width,
                        net: net.clone(),
                    });
                    via_at(&mut ctx, &mut out, c);
                    out.pads_stitched += 1;
                }
                None => out.notes.push(format!(
                    "{net} pad {}.{}: no escape via within {RING_MAX_MM} mm lands in {net} fill on another layer with {:.2} mm clearance; route it with a track",
                    pad.object_id,
                    pad.sub,
                    cl / 1e6
                )),
            }
        }

        // 2. Stitching vias for islands with no cross-layer conductor of the net.
        for (fi, f) in fills.iter().enumerate() {
            let mut anchored = BTreeSet::new();
            for it in copper.iter().filter(|it| {
                &it.net == net
                    && it.layer == f.layer
                    && (it.kind == ItemKind::Via || (it.kind == ItemKind::Pad && layers_of(it) > 1))
            }) {
                anchored.extend(f.islands_touching(&it.shape));
            }
            for (n, l, s) in &ctx.placed {
                if n == net && l.is_none() {
                    anchored.extend(f.islands_touching(s));
                }
            }
            let d = depth(f);
            let mut by_island: Vec<Vec<usize>> = vec![vec![]; f.islands as usize + 1];
            for (i, &k) in f.cells.iter().enumerate() {
                if k != 0 {
                    by_island[k as usize].push(i);
                }
            }
            for k in 1..=f.islands {
                if anchored.contains(&k) {
                    continue;
                }
                // Recomputed per island: a via placed for one island makes the island it
                // lands in live for the next.
                let live_other: Vec<(&ZoneFill, BTreeSet<u32>)> = fills
                    .iter()
                    .enumerate()
                    .filter(|(j, g)| *j != fi && g.layer != f.layer)
                    .map(|(_, g)| (g, live_islands(g, &copper, &ctx.placed)))
                    .collect();
                let cells = &mut by_island[k as usize];
                cells.sort_by_key(|&i| std::cmp::Reverse(d[i]));
                let found = cells
                    .iter()
                    .take(ISLAND_CANDIDATES)
                    .map(|&i| f.cell_center(i % f.cols, i / f.cols))
                    .find(|&c| {
                        live_other
                            .iter()
                            .any(|(g, live)| live.contains(&cell_value(g, c)))
                            && ctx.via_ok(net, c, r, cl)
                    });
                match found {
                    Some(c) => {
                        via_at(&mut ctx, &mut out, c);
                        out.islands_stitched += 1;
                    }
                    None => out.notes.push(format!(
                        "{net} island {k} of zone {} on {}: no via position lands in connected {net} fill on another layer with {:.2} mm clearance; it is removed as dead copper on refill",
                        f.zone_id,
                        f.layer.name(),
                        cl / 1e6
                    )),
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Unrun: written while builds are frozen (code-reviewed only).
    use super::*;
    use crate::copper::analyse;
    use eak_core::{FootprintInstance, Rotation, Side, Track, Via, Zone};

    /// 30 x 20 mm two-layer board. GND is poured over the whole bottom layer and over
    /// x = 12..29 mm on top, where a top SIG track at x = 20 mm splits it into two islands.
    /// R1 (0603, both pads GND) sits on top at x = 8 mm, outside the top pour.
    fn board() -> Project {
        let mut p = Project::new("stitch");
        let def = eak_core::builtin::get("R_0603").expect("builtin 0603 resistor");
        p.library.insert(def.id.clone(), def.clone());
        p.pcb.board.outline = vec![
            Point::mm(0.0, 0.0),
            Point::mm(30.0, 0.0),
            Point::mm(30.0, 20.0),
            Point::mm(0.0, 20.0),
        ];
        p.pcb.footprints.insert(
            "r1".into(),
            FootprintInstance {
                id: "r1".into(),
                symbol_id: "s1".into(),
                component_id: def.id.clone(),
                refdes: "R1".into(),
                position: Point::mm(8.0, 10.0),
                rotation: Rotation::R0,
                side: Side::Top,
                pad_nets: def
                    .footprint
                    .pads
                    .iter()
                    .map(|pd| (pd.number.clone(), "GND".to_string()))
                    .collect(),
                locked: false,
            },
        );
        let rect = |x0: f64, x1: f64| {
            vec![
                Point::mm(x0, 1.0),
                Point::mm(x1, 1.0),
                Point::mm(x1, 19.0),
                Point::mm(x0, 19.0),
            ]
        };
        for (id, layer, outline) in [
            ("zt", CopperLayer::Top, rect(12.0, 29.0)),
            ("zb", CopperLayer::Bottom, rect(1.0, 29.0)),
        ] {
            p.pcb.zones.insert(
                id.into(),
                Zone {
                    id: id.into(),
                    layer,
                    net: "GND".into(),
                    outline,
                    clearance: 300_000,
                },
            );
        }
        p.pcb.tracks.insert(
            "split".into(),
            Track {
                id: "split".into(),
                layer: CopperLayer::Top,
                width: 250_000,
                a: Point::mm(20.0, 0.5),
                b: Point::mm(20.0, 19.5),
                net: "SIG".into(),
                arc_deg: None,
            },
        );
        p
    }

    fn apply(p: &Project, plan: &StitchPlan) -> Project {
        let mut q = p.clone();
        for (i, v) in plan.vias.iter().enumerate() {
            q.pcb.vias.insert(
                format!("v{i}"),
                Via {
                    id: format!("v{i}"),
                    position: v.position,
                    drill: v.drill,
                    diameter: v.diameter,
                    net: v.net.clone(),
                    from: CopperLayer::Top,
                    to: CopperLayer::Bottom,
                },
            );
        }
        for (i, t) in plan.tracks.iter().enumerate() {
            q.pcb.tracks.insert(
                format!("t{i}"),
                Track {
                    id: format!("t{i}"),
                    layer: t.layer,
                    width: t.width,
                    a: t.a,
                    b: t.b,
                    net: t.net.clone(),
                    arc_deg: None,
                },
            );
        }
        q
    }

    #[test]
    fn single_layer_pour_is_reported_not_stitched() {
        let mut p = board();
        p.pcb.zones.remove("zb");
        let plan = plan(&p, Some("GND"));
        assert!(plan.vias.is_empty());
        assert!(
            plan.notes[0].contains("1 layer(s) only"),
            "{:?}",
            plan.notes
        );
    }

    /// Regression for dev-board run B (GND unrouted, 43 pour islands): orphan SMD pads get
    /// escape vias into the bottom pour, both top islands get stitching vias, and the net ends
    /// as one copper group with no short and no via on the SIG track.
    #[test]
    fn stitching_joins_orphan_pads_and_split_pour() {
        let p = board();
        assert_eq!(analyse(&p).net_groups["GND"], 2);
        let plan = plan(&p, Some("GND"));
        assert_eq!(plan.pads_stitched, 2, "{:?}", plan.notes);
        assert_eq!(plan.islands_stitched, 2, "{:?}", plan.notes);
        let q = apply(&p, &plan);
        let c = analyse(&q);
        assert_eq!(c.net_groups["GND"], 1, "{:?}", plan.notes);
        assert!(c.shorts.is_empty(), "{:?}", c.shorts);
        for v in &plan.vias {
            assert!((v.position.x - Point::mm(20.0, 0.0).x).abs() > 300_000);
        }
        // Nothing left to do on a second run.
        let again = super::plan(&q, Some("GND"));
        assert!(again.vias.is_empty(), "{:?}", again.vias);
    }
}
