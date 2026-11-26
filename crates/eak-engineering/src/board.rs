//! The board as fabricated, derived once from the EAK-IR: outline, cut-outs, per-layer copper
//! (pads, track chords, via barrels), zone fills after clearance, keep-outs, thermal reliefs and
//! dead-island removal, drilled holes and the mask-clipped silkscreen legend.
//!
//! DRC, connectivity and the 3D scene read this one value instead of re-deriving geometry, so
//! the geometry that is checked is the geometry that is drawn (ADR-0052). Derived on request,
//! never stored.

use crate::copper::{self, CopperItem, Core, ItemKind, Shape, ZoneFill};
use crate::fill_view::{self, FillView};
use crate::silk::{self, Stroke};
use eak_core::{Id, Nm, PadKind, Point, Project, Side};
use serde::Serialize;

/// One drilled hole: a via barrel or a plated through-hole pad.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hole {
    /// Owning object: the via id or the footprint id.
    pub object_id: Id,
    /// Pad number for pad holes, empty for vias.
    pub pad: String,
    /// Human-readable name for messages (`via v1`, `J1.2`).
    pub label: String,
    pub center: Point,
    pub drill: Nm,
    pub net: String,
}

impl Hole {
    pub fn shape(&self) -> Shape {
        Shape {
            core: Core::Segment(self.center, self.center),
            radius: self.drill as f64 / 2.0,
        }
    }
    /// True when `it` is this hole's own copper (its via barrel or its pad).
    pub fn is_owner(&self, it: &CopperItem) -> bool {
        it.object_id == self.object_id
            && match it.kind {
                ItemKind::Via => self.pad.is_empty(),
                ItemKind::Pad => it.sub == self.pad,
                _ => false,
            }
    }
}

/// Every drilled hole of the board (vias, then THT pads), in a deterministic order.
pub fn holes(p: &Project) -> Vec<Hole> {
    let mut out: Vec<Hole> = p
        .pcb
        .vias
        .values()
        .map(|v| Hole {
            object_id: v.id.clone(),
            pad: String::new(),
            label: format!("via {}", v.id),
            center: v.position,
            drill: v.drill,
            net: v.net.clone(),
        })
        .collect();
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        for pad in def.footprint.pads.iter().filter(|p| p.kind == PadKind::Tht) {
            let Some(drill) = pad.drill else { continue };
            out.push(Hole {
                object_id: fp.id.clone(),
                pad: pad.number.clone(),
                label: format!("{}.{}", fp.refdes, pad.number),
                center: p.pad_position(fp, pad),
                drill,
                net: fp.pad_nets.get(&pad.number).cloned().unwrap_or_default(),
            });
        }
    }
    out
}

/// The silkscreen legend of both sides, as fabricated (`silk::legend`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Legend {
    pub top: Vec<Stroke>,
    pub bottom: Vec<Stroke>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardGeometry {
    /// Project revision this geometry was derived from.
    pub revision: u64,
    /// Board outline with arc edges flattened (`Board::outline_path`), nm.
    pub outline: Vec<Point>,
    pub cutouts: Vec<Vec<Point>>,
    pub thickness: Nm,
    /// Every copper item except zones: the shapes DRC checks and connectivity joins.
    pub copper: Vec<CopperItem>,
    /// Zone fills as fabricated, in zone-id order; DRC and connectivity read these cells.
    #[serde(skip)]
    pub fills: Vec<ZoneFill>,
    /// `fills` as rectangles for drawing: the very same cells, one entry per fill.
    pub fill_views: Vec<FillView>,
    pub holes: Vec<Hole>,
    pub silk: Legend,
}

impl BoardGeometry {
    pub fn build(p: &Project) -> BoardGeometry {
        let copper = copper::items(p);
        let fills: Vec<ZoneFill> = p
            .pcb
            .zones
            .keys()
            .filter_map(|z| copper::fill_zone(p, z, &copper))
            .collect();
        BoardGeometry {
            revision: p.revision,
            outline: p.pcb.board.outline_path(),
            cutouts: p.pcb.cutouts.values().map(|c| c.outline.clone()).collect(),
            thickness: p.pcb.board.stackup.board_thickness,
            fill_views: fills.iter().map(fill_view::view_of).collect(),
            fills,
            holes: holes(p),
            silk: Legend {
                top: silk::legend(p, Side::Top),
                bottom: silk::legend(p, Side::Bottom),
            },
            copper,
        }
    }

    /// The fill of `zone_id`, if the zone has a usable outline.
    pub fn fill(&self, zone_id: &str) -> Option<&ZoneFill> {
        self.fills.iter().find(|f| f.zone_id == zone_id)
    }

    pub fn legend(&self, side: Side) -> &[Stroke] {
        match side {
            Side::Top => &self.silk.top,
            Side::Bottom => &self.silk.bottom,
        }
    }

    /// Physical connectivity of this copper (`copper::analyse` on the same items and fills).
    pub fn connectivity(&self) -> copper::Connectivity {
        copper::analyse_with(&self.copper, &self.fills)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::{CopperLayer, FootprintInstance, Rotation, Track, Via, Zone};

    /// 20 × 20 mm board: a two-pin THT header (pin 1 GND) under a bottom GND zone, a top SIG
    /// track and a GND via.
    fn board() -> Project {
        let mut p = Project::new("board-geometry");
        let def = eak_core::builtin::get("CONN_1x02_P2.54").unwrap();
        p.library.insert(def.id.clone(), def.clone());
        p.pcb.board.outline = vec![
            Point::mm(0.0, 0.0),
            Point::mm(20.0, 0.0),
            Point::mm(20.0, 20.0),
            Point::mm(0.0, 20.0),
        ];
        p.pcb.footprints.insert(
            "fp".into(),
            FootprintInstance {
                id: "fp".into(),
                symbol_id: "sym".into(),
                component_id: def.id.clone(),
                refdes: "J1".into(),
                position: Point::mm(10.0, 10.0),
                rotation: Rotation::R0,
                side: Side::Top,
                pad_nets: [("1".to_string(), "GND".to_string())].into(),
                locked: false,
            },
        );
        p.pcb.tracks.insert(
            "t".into(),
            Track {
                id: "t".into(),
                layer: CopperLayer::Top,
                width: 250_000,
                a: Point::mm(2.0, 3.0),
                b: Point::mm(8.0, 3.0),
                net: "SIG".into(),
                arc_deg: None,
            },
        );
        p.pcb.vias.insert(
            "v".into(),
            Via {
                id: "v".into(),
                position: Point::mm(15.0, 15.0),
                drill: 300_000,
                diameter: 600_000,
                net: "GND".into(),
                from: CopperLayer::Top,
                to: CopperLayer::Bottom,
            },
        );
        p.pcb.zones.insert(
            "z".into(),
            Zone {
                id: "z".into(),
                layer: CopperLayer::Bottom,
                net: "GND".into(),
                outline: vec![
                    Point::mm(1.0, 1.0),
                    Point::mm(19.0, 1.0),
                    Point::mm(19.0, 19.0),
                    Point::mm(1.0, 19.0),
                ],
                clearance: 300_000,
            },
        );
        p
    }

    #[test]
    fn one_build_holds_every_consumer_input() {
        let p = board();
        let g = BoardGeometry::build(&p);
        assert_eq!(g.outline, p.pcb.board.outline_path());
        assert_eq!(g.copper.len(), copper::items(&p).len());
        assert_eq!(g.holes, holes(&p));
        assert_eq!(g.holes.len(), 3, "via + two THT pads");
        assert_eq!(g.silk.top, silk::legend(&p, Side::Top));
        assert_eq!(g.legend(Side::Bottom), silk::legend(&p, Side::Bottom));
        // The drawn fills are the checked fills, and equal an independent recomputation.
        assert_eq!(g.fills.len(), 1);
        assert_eq!(g.fill_views, fill_view::fill_views(&p));
        assert_eq!(g.fill_views[0].rects, fill_view::rects_of(&g.fills[0]));
        assert!(g.fill("z").is_some() && g.fill("nope").is_none());
        let s = crate::scene3d::build(&p, &g);
        assert_eq!(s.outline_mm.len(), g.outline.len());
        let a = g.connectivity();
        let b = copper::analyse(&p);
        assert_eq!(a.net_groups, b.net_groups);
        assert_eq!(a.ratsnest, b.ratsnest);
        assert_eq!(a.shorts, b.shorts);
    }
}
