//! Reference planes and split planes (M7.6): which plane layer carries the return current of each
//! signal layer, which copper of that plane lies under a point, where a track's reference changes
//! (a split crossing) or is missing, and what a layer change needs to keep the return path.
//!
//! The model reads the stack-up (`LayerKind::Plane` layers, and neighbours carrying a pour) and
//! the zone fills of the one board geometry (`board::BoardGeometry`, ADR-0052), so the router
//! (which avoids splits) and the DRC (which reports them) judge the same copper. A point's
//! reference is the fill island of a zone under it; a void inside a zone outline (an anti-pad,
//! the clearance around other-net copper) close to fill counts as the nearest island, so drilled
//! holes are not splits, while a gap between zones, a slot cutting a fill in two, or leaving the
//! zone are.
//!
//! `power::return_path` stays the advisory any-signal ground-continuity estimate; this module is
//! the stack-up model for declared high-speed nets.

use crate::board::BoardGeometry;
use crate::copper::{ZoneFill, ZONE_FILL_PITCH};
use eak_core::geom::point_in_polygon;
use eak_core::{CopperLayer, LayerKind, Nm, Point, Project, StackupLayer};
use serde::Serialize;
use std::collections::BTreeSet;

/// A void inside a plane zone this close to fill belongs to the nearest fill island; a larger
/// void is a gap in the reference.
// shortcut: a fixed 1.5 mm reach (EAK heuristic, about a large THT anti-pad); upgrade to a declared limit when the IR has one.
pub const VOID_REACH: Nm = 1_500_000;

/// Nets the project declares high-speed: members of declared differential pairs
/// (`Constraints::diff_pairs`) and the nets of a net class with a target impedance
/// (`NetClass::target_z0_ohm`). Name heuristics (`priority::high_speed_nets`) do not count.
pub fn declared_high_speed(p: &Project) -> BTreeSet<String> {
    let c = &p.constraints;
    let pairs = c
        .diff_pairs
        .iter()
        .flat_map(|d| [d.positive.clone(), d.negative.clone()]);
    let classes = c
        .net_classes
        .iter()
        .filter(|nc| nc.target_z0_ohm.is_some())
        .flat_map(|nc| nc.nets.iter().cloned());
    pairs.chain(classes).collect()
}

/// The reference layers of `layer`: its stack-up neighbours that are plane layers or carry a zone
/// (a pour, as on a two-layer board), nearest dielectric first (the return current flows mostly in
/// the nearest one). Empty for a plane layer, a layer the stack-up lacks and a layer with neither.
pub fn reference_layers(p: &Project, layer: CopperLayer) -> Vec<CopperLayer> {
    let st = &p.pcb.board.stackup;
    let Some(i) = st.index_of(layer) else {
        return vec![];
    };
    if st.layers[i].kind == LayerKind::Plane {
        return vec![];
    }
    let reference = |l: &StackupLayer| {
        l.kind == LayerKind::Plane || p.pcb.zones.values().any(|z| z.layer == l.layer)
    };
    let mut out: Vec<(u32, CopperLayer)> = vec![];
    if let Some(above) = i.checked_sub(1).map(|j| &st.layers[j]) {
        if reference(above) {
            out.push((above.dielectric_after_um, above.layer));
        }
    }
    if let Some(below) = st.layers.get(i + 1) {
        if reference(below) {
            out.push((st.layers[i].dielectric_after_um, below.layer));
        }
    }
    out.sort_by_key(|(d, _)| *d);
    out.into_iter().map(|(_, l)| l).collect()
}

/// What a plane layer offers as return path at one point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reference {
    /// No zone fill of the plane under the point.
    None,
    /// Fill island `island` of zone `zone`, on net `net`.
    Copper {
        zone: String,
        net: String,
        island: u32,
    },
}

impl Reference {
    pub fn net(&self) -> Option<&str> {
        match self {
            Reference::Copper { net, .. } => Some(net),
            Reference::None => None,
        }
    }

    /// The return current can follow from `self` to `other` without crossing a split: the same
    /// island of one zone, two zones of the same net (assumed joined), or no copper on both.
    pub fn continues(&self, other: &Reference) -> bool {
        match (self, other) {
            (Reference::None, Reference::None) => true,
            (
                Reference::Copper {
                    zone: za,
                    net: na,
                    island: ia,
                },
                Reference::Copper {
                    zone: zb,
                    net: nb,
                    island: ib,
                },
            ) => na == nb && (za != zb || ia == ib),
            _ => false,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Reference::None => "no plane copper".into(),
            Reference::Copper { zone, net, island } => {
                format!("{net} (zone {zone}, island {island})")
            }
        }
    }
}

/// A point along a track where its reference changes in a way the return current cannot follow.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Crossing {
    pub at: Point,
    pub from: Reference,
    pub to: Reference,
}

/// What lies under a track on one reference plane, sampled along its centreline and both edges.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PathReference {
    pub plane: CopperLayer,
    /// No sample had plane copper under it.
    pub absent: bool,
    /// Distinct reference changes along the track, in path order.
    pub crossings: Vec<Crossing>,
}

/// What a signal layer change needs to keep its return path.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReturnNeed {
    /// Both layers have the same nearest reference plane (or one has none: a missing reference,
    /// reported on its own).
    Nothing,
    /// A via of `net` joining both reference planes near the transition carries the return.
    Via {
        net: String,
        planes: [CopperLayer; 2],
    },
    /// The reference changes net (for example ground to a power plane): no via can join them;
    /// only a capacitor between the two nets bridges the return.
    Capacitor {
        nets: [String; 2],
        planes: [CopperLayer; 2],
    },
    /// The reference plane of one layer has no copper at the transition.
    NoCopper { plane: CopperLayer },
}

/// The fill island nearest to `pt` within `reach` (cell centres), if any.
fn nearest_island(f: &ZoneFill, pt: Point, reach: Nm) -> Option<u32> {
    if f.pitch <= 0 {
        return None;
    }
    let n = (reach / f.pitch).max(1);
    let c0 = (pt.x - f.origin.x).div_euclid(f.pitch);
    let r0 = (pt.y - f.origin.y).div_euclid(f.pitch);
    let mut best: Option<(f64, u32)> = None;
    for d in 1..=n {
        for dr in -d..=d {
            for dc in -d..=d {
                if dr.abs() != d && dc.abs() != d {
                    continue; // only the ring at Chebyshev distance d
                }
                let (c, r) = (c0 + dc, r0 + dr);
                if !(0..f.cols as Nm).contains(&c) || !(0..f.rows as Nm).contains(&r) {
                    continue;
                }
                let k = f.cells[r as usize * f.cols + c as usize];
                let dist = f.cell_center(c as usize, r as usize).dist(pt);
                if k != 0 && dist <= reach as f64 && best.is_none_or(|(b, _)| dist < b) {
                    best = Some((dist, k));
                }
            }
        }
        // Every cell of a later ring is farther than this ring's inner edge.
        if best.is_some_and(|(b, _)| b <= (d * f.pitch) as f64) {
            break;
        }
    }
    best.map(|(_, k)| k)
}

/// The plane model of one board revision.
pub struct Planes<'a> {
    p: &'a Project,
    geo: &'a BoardGeometry,
}

impl<'a> Planes<'a> {
    pub fn new(p: &'a Project, geo: &'a BoardGeometry) -> Planes<'a> {
        Planes { p, geo }
    }

    fn fills_on(&self, plane: CopperLayer) -> impl Iterator<Item = &'a ZoneFill> {
        let geo: &'a BoardGeometry = self.geo;
        geo.fills.iter().filter(move |f| f.layer == plane)
    }

    /// The reference copper of `plane` under `pt`.
    pub fn reference_at(&self, plane: CopperLayer, pt: Point) -> Reference {
        let copper = |f: &ZoneFill, island: u32| Reference::Copper {
            zone: f.zone_id.clone(),
            net: f.net.clone(),
            island,
        };
        for f in self.fills_on(plane) {
            match f.cell_at(pt) {
                0 => {}
                k => return copper(f, k),
            }
        }
        for f in self.fills_on(plane) {
            let inside = self
                .p
                .pcb
                .zones
                .get(&f.zone_id)
                .is_some_and(|z| z.outline.len() >= 3 && point_in_polygon(pt, &z.outline));
            if !inside {
                continue;
            }
            if let Some(k) = nearest_island(f, pt, VOID_REACH) {
                return copper(f, k);
            }
        }
        Reference::None
    }

    /// The reference of a track (`path`, `width`) on `plane`: its centreline and both edges are
    /// sampled at the fill pitch.
    pub fn along(&self, plane: CopperLayer, path: &[Point], width: Nm) -> PathReference {
        let step = ZONE_FILL_PITCH as f64;
        let half = width as f64 / 2.0;
        let mut crossings: Vec<Crossing> = vec![];
        let mut any = false;
        for lane in [0.0, half, -half] {
            let mut prev: Option<Reference> = None;
            for w in path.windows(2) {
                let (a, b) = (w[0], w[1]);
                let len = a.dist(b);
                if len <= 0.0 {
                    continue;
                }
                let (ux, uy) = ((b.x - a.x) as f64 / len, (b.y - a.y) as f64 / len);
                let n = (len / step).ceil().max(1.0) as usize;
                for k in 0..=n {
                    let t = len * k as f64 / n as f64;
                    let pt = Point::new(
                        (a.x as f64 + ux * t - uy * lane).round() as Nm,
                        (a.y as f64 + uy * t + ux * lane).round() as Nm,
                    );
                    let r = self.reference_at(plane, pt);
                    any |= r != Reference::None;
                    if let Some(q) = prev.as_ref().filter(|q| !q.continues(&r)) {
                        if !crossings.iter().any(|c| c.from == *q && c.to == r) {
                            crossings.push(Crossing {
                                at: pt,
                                from: q.clone(),
                                to: r.clone(),
                            });
                        }
                    }
                    prev = Some(r);
                }
            }
        }
        PathReference {
            plane,
            absent: !any,
            crossings,
        }
    }

    /// What a change from signal layer `a` to `b` at `at` needs to keep its return path
    /// (nearest reference plane of each layer).
    pub fn return_need(&self, at: Point, a: CopperLayer, b: CopperLayer) -> ReturnNeed {
        let pa = reference_layers(self.p, a).first().copied();
        let pb = reference_layers(self.p, b).first().copied();
        let (Some(pa), Some(pb)) = (pa, pb) else {
            return ReturnNeed::Nothing;
        };
        if pa == pb {
            return ReturnNeed::Nothing;
        }
        let (ra, rb) = (self.reference_at(pa, at), self.reference_at(pb, at));
        match (ra.net(), rb.net()) {
            (None, _) => ReturnNeed::NoCopper { plane: pa },
            (_, None) => ReturnNeed::NoCopper { plane: pb },
            (Some(na), Some(nb)) if na == nb => ReturnNeed::Via {
                net: na.to_string(),
                planes: [pa, pb],
            },
            (Some(na), Some(nb)) => ReturnNeed::Capacitor {
                nets: [na.to_string(), nb.to_string()],
                planes: [pa, pb],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::{DiffPairRule, NetClass, Stackup, Track, Via, Zone};

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
        vec![
            Point::mm(x0, y0),
            Point::mm(x1, y0),
            Point::mm(x1, y1),
            Point::mm(x0, y1),
        ]
    }

    fn via(p: &mut Project, id: &str, x: f64, y: f64, net: &str) {
        p.pcb.vias.insert(
            id.into(),
            Via {
                id: id.into(),
                position: Point::mm(x, y),
                drill: 300_000,
                diameter: 600_000,
                net: net.into(),
                from: CopperLayer::Top,
                to: CopperLayer::Bottom,
            },
        );
    }

    fn zone(p: &mut Project, id: &str, layer: CopperLayer, net: &str, outline: Vec<Point>) {
        p.pcb.zones.insert(
            id.into(),
            Zone {
                id: id.into(),
                layer,
                net: net.into(),
                outline,
                clearance: 300_000,
            },
        );
    }

    /// 40 x 20 mm 4-layer board: In1 a solid GND plane; In2 split into `left` (x 1..19 mm) and
    /// +5V (x 21..39 mm). Each zone has a through via of its net so its fill is kept.
    fn board(left: &str) -> Project {
        let mut p = Project::new("planes");
        p.pcb.board.outline = rect(0.0, 0.0, 40.0, 20.0);
        p.pcb.board.stackup = Stackup::multilayer(4, 1_600_000, 1.0, 0.5, "FR-4", 4.5);
        let (in1, in2) = (CopperLayer::Inner(1), CopperLayer::Inner(2));
        zone(&mut p, "gnd", in1, "GND", rect(1.0, 1.0, 39.0, 19.0));
        zone(&mut p, "left", in2, left, rect(1.0, 1.0, 19.0, 19.0));
        zone(&mut p, "p5", in2, "+5V", rect(21.0, 1.0, 39.0, 19.0));
        via(&mut p, "vg", 20.0, 10.0, "GND");
        via(&mut p, "vl", 5.0, 5.0, left);
        via(&mut p, "v5", 35.0, 5.0, "+5V");
        p
    }

    fn track(layer: CopperLayer, a: Point, b: Point) -> Track {
        Track {
            id: "t".into(),
            layer,
            width: 250_000,
            a,
            b,
            net: "D_P".into(),
            arc_deg: None,
        }
    }

    fn along(p: &Project, t: &Track) -> PathReference {
        let geo = BoardGeometry::build(p);
        let plane = reference_layers(p, t.layer)[0];
        Planes::new(p, &geo).along(plane, &t.path(), t.width)
    }

    #[test]
    fn each_outer_layer_references_its_adjacent_plane() {
        let p = board("+3V3");
        assert_eq!(
            reference_layers(&p, CopperLayer::Top),
            vec![CopperLayer::Inner(1)]
        );
        assert_eq!(
            reference_layers(&p, CopperLayer::Bottom),
            vec![CopperLayer::Inner(2)]
        );
        assert!(reference_layers(&p, CopperLayer::Inner(1)).is_empty());
        let mut q = Project::new("two");
        q.pcb.board.stackup = Stackup::two_layer(1_600_000, 1.0, "FR-4", 4.5);
        assert!(reference_layers(&q, CopperLayer::Top).is_empty());
        // A pour on the other side is the reference on a two-layer board.
        zone(
            &mut q,
            "pour",
            CopperLayer::Bottom,
            "GND",
            rect(1.0, 1.0, 9.0, 9.0),
        );
        assert_eq!(
            reference_layers(&q, CopperLayer::Top),
            vec![CopperLayer::Bottom]
        );
    }

    #[test]
    fn declared_high_speed_is_pairs_and_impedance_classes_only() {
        let mut p = board("+3V3");
        p.constraints.diff_pairs.push(DiffPairRule {
            positive: "D_P".into(),
            negative: "D_N".into(),
            width: None,
            gap: None,
            target_zdiff_ohm: None,
            zdiff_tolerance_pct: None,
            max_skew: None,
        });
        let mut class = NetClass {
            name: "rf".into(),
            nets: vec!["RF_OUT".into()],
            target_z0_ohm: Some(50.0),
            ..p.constraints.default_class.clone()
        };
        p.constraints.net_classes.push(class.clone());
        class.name = "slow".into();
        class.nets = vec!["LED".into()];
        class.target_z0_ohm = None;
        p.constraints.net_classes.push(class);
        let want: BTreeSet<String> = ["D_P", "D_N", "RF_OUT"].map(String::from).into();
        assert_eq!(declared_high_speed(&p), want);
    }

    #[test]
    fn a_track_over_the_split_crosses_it_and_one_beside_it_does_not() {
        let p = board("+3V3");
        let (a, b) = (Point::mm(5.0, 15.0), Point::mm(35.0, 15.0));
        let cross = along(&p, &track(CopperLayer::Bottom, a, b));
        assert!(!cross.absent);
        let first = &cross.crossings[0];
        assert_eq!(first.from.net(), Some("+3V3"));
        assert_eq!(first.to, Reference::None, "the gap between the zones");
        assert!(first.at.x > eak_core::geom::mm(18.0) && first.at.x < eak_core::geom::mm(20.0));
        assert!(cross.crossings.iter().any(|c| c.to.net() == Some("+5V")));
        let (c, d) = (Point::mm(3.0, 15.0), Point::mm(17.0, 15.0));
        let beside = along(&p, &track(CopperLayer::Bottom, c, d));
        assert!(beside.crossings.is_empty() && !beside.absent);
        // The same run on the top layer is over the solid GND plane.
        let top = along(&p, &track(CopperLayer::Top, a, b));
        assert!(top.crossings.is_empty() && !top.absent);
    }

    #[test]
    fn an_anti_pad_is_not_a_split_but_leaving_the_plane_is() {
        let p = board("+3V3");
        // Passes over the +3V3 via's anti-pad in the GND plane.
        let (a, b) = (Point::mm(2.0, 5.0), Point::mm(12.0, 5.0));
        let over = along(&p, &track(CopperLayer::Top, a, b));
        assert!(over.crossings.is_empty(), "{:?}", over.crossings);
        // Runs off the GND zone (x 39 mm) towards the board edge.
        let (c, d) = (Point::mm(30.0, 10.0), Point::mm(39.8, 10.0));
        let off = along(&p, &track(CopperLayer::Top, c, d));
        assert_eq!(off.crossings.len(), 1, "{:?}", off.crossings);
        assert_eq!(off.crossings[0].to, Reference::None);
        let (e, f) = (Point::mm(19.6, 2.0), Point::mm(19.6, 18.0));
        let nowhere = along(&p, &track(CopperLayer::Bottom, e, f));
        assert!(nowhere.absent, "the run lies in the split itself");
    }

    #[test]
    fn a_layer_change_needs_a_via_of_a_shared_reference_net_else_a_capacitor() {
        let p = board("+3V3");
        let geo = BoardGeometry::build(&p);
        let planes = Planes::new(&p, &geo);
        let (top, bottom) = (CopperLayer::Top, CopperLayer::Bottom);
        let at = Point::mm(10.0, 10.0);
        assert_eq!(
            planes.return_need(at, top, bottom),
            ReturnNeed::Capacitor {
                nets: ["GND".into(), "+3V3".into()],
                planes: [CopperLayer::Inner(1), CopperLayer::Inner(2)],
            }
        );
        assert_eq!(
            planes.return_need(Point::mm(20.0, 15.0), top, bottom),
            ReturnNeed::NoCopper {
                plane: CopperLayer::Inner(2)
            }
        );
        assert_eq!(planes.return_need(at, top, top), ReturnNeed::Nothing);
        let g = board("GND");
        let geo = BoardGeometry::build(&g);
        assert_eq!(
            Planes::new(&g, &geo).return_need(at, bottom, top),
            ReturnNeed::Via {
                net: "GND".into(),
                planes: [CopperLayer::Inner(2), CopperLayer::Inner(1)],
            }
        );
    }
}
