//! Zone fills as the editor draws them (slice E5, AD-6): the cells [`copper::fill_zone`] leaves
//! filled, the same raster DRC and connectivity use, merged into axis-aligned rectangles.
//! Derived on request from the IR (which stores only the zone outline); never stored.

use crate::copper::{self, ZoneFill};
use eak_core::{CopperLayer, Id, Nm, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One zone's fill in board nm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FillView {
    pub zone_id: Id,
    pub layer: CopperLayer,
    pub net: String,
    /// Islands kept (each touches copper of the zone net on its layer).
    pub islands: u32,
    /// Islands dropped as dead copper (not drawn).
    pub removed_islands: u32,
    pub filled_area_mm2: f64,
    /// Raster pitch of the fill in nm.
    pub pitch: Nm,
    /// Filled copper as `[x0, y0, x1, y1]` rectangles (x0 < x1, y0 < y1) that tile exactly the
    /// filled cells: no overlap, no gap.
    pub rects: Vec<[Nm; 4]>,
}

/// Merge the filled cells of `f` into rectangles: runs per row, a run extended downwards while
/// the next row has the very same run.
pub fn rects_of(f: &ZoneFill) -> Vec<[Nm; 4]> {
    let mut out = Vec::new();
    // Open rectangles: (c0, c1) -> first row.
    let mut open: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let close = |out: &mut Vec<[Nm; 4]>, (c0, c1): (usize, usize), r0: usize, r1: usize| {
        out.push([
            f.origin.x + c0 as Nm * f.pitch,
            f.origin.y + r0 as Nm * f.pitch,
            f.origin.x + c1 as Nm * f.pitch,
            f.origin.y + r1 as Nm * f.pitch,
        ]);
    };
    for r in 0..=f.rows {
        let mut runs = Vec::new();
        if r < f.rows {
            let row = &f.cells[r * f.cols..(r + 1) * f.cols];
            let mut c = 0;
            while c < f.cols {
                if row[c] == 0 {
                    c += 1;
                    continue;
                }
                let c0 = c;
                while c < f.cols && row[c] != 0 {
                    c += 1;
                }
                runs.push((c0, c));
            }
        }
        let mut next = BTreeMap::new();
        for run in runs {
            next.insert(run, open.remove(&run).unwrap_or(r));
        }
        for (run, r0) in std::mem::take(&mut open) {
            close(&mut out, run, r0, r);
        }
        open = next;
    }
    out
}

/// Every zone's fill, in zone-id order. A zone without a usable outline has no entry.
pub fn fill_views(p: &Project) -> Vec<FillView> {
    let items = copper::items(p);
    let mut out = Vec::new();
    for z in p.pcb.zones.keys() {
        let Some(f) = copper::fill_zone(p, z, &items) else {
            continue;
        };
        out.push(FillView {
            rects: rects_of(&f),
            filled_area_mm2: f.filled_area_mm2(),
            zone_id: f.zone_id,
            layer: f.layer,
            net: f.net,
            islands: f.islands,
            removed_islands: f.removed_islands,
            pitch: f.pitch,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::{FootprintInstance, Point, Rotation, Side, Zone};

    /// A 20 × 20 mm board, a bottom GND zone and a GND THT pad in the middle (thermal relief).
    fn board() -> Project {
        let mut p = Project::new("fill-view");
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

    /// Fill equals the DRC copper model: the rectangles cover exactly the cells `fill_zone`
    /// (the raster DRC and connectivity read) leaves filled, each cell once.
    #[test]
    fn rectangles_tile_exactly_the_drc_fill() {
        let p = board();
        let f = copper::fill_zone(&p, "z", &copper::items(&p)).unwrap();
        let rects = rects_of(&f);
        let mut covered = vec![0u8; f.cells.len()];
        for [x0, y0, x1, y1] in &rects {
            assert!(x0 < x1 && y0 < y1);
            for r in ((y0 - f.origin.y) / f.pitch)..((y1 - f.origin.y) / f.pitch) {
                for c in ((x0 - f.origin.x) / f.pitch)..((x1 - f.origin.x) / f.pitch) {
                    covered[r as usize * f.cols + c as usize] += 1;
                }
            }
        }
        for (i, k) in f.cells.iter().enumerate() {
            assert_eq!(covered[i], u8::from(*k != 0), "cell {i}");
        }
        // The relief gap around the pad leaves holes, so it is more than one rectangle.
        assert!(rects.len() > 1);
        let v = &fill_views(&p)[0];
        assert_eq!(
            (v.zone_id.as_str(), v.islands, v.rects.len()),
            ("z", f.islands, rects.len())
        );
        assert!((v.filled_area_mm2 - f.filled_area_mm2()).abs() < 1e-9);
    }

    #[test]
    fn a_zone_without_an_outline_has_no_fill() {
        let mut p = board();
        p.pcb.zones.get_mut("z").unwrap().outline.clear();
        assert!(fill_views(&p).is_empty());
    }
}
