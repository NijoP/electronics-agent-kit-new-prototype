//! Constructive placement: places unlocked footprints inside the board outline,
//! highest-connectivity first, each at the free candidate position that minimises
//! total distance to already-placed connected pads. Courtyards never overlap and stay
//! inside the board edge clearance; components that do not fit are reported.
//! Role-aware (see `placement_intel`): mounting holes go to the corners, connectors to the
//! board edge, decoupling/crystal/ESD parts next to the pad they serve.

use crate::placement_intel::{self as intel, Role};
use eak_core::geom::{mm, point_in_polygon, point_segment_distance};
use eak_core::{Id, Nm, Point, Project, Rect, Rotation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Placement {
    pub moves: Vec<(Id, Point, Rotation)>,
    pub unplaced: Vec<(Id, String)>,
}

fn courtyard(p: &Project, id: &str, at: Point, rot: Rotation) -> Option<Rect> {
    let fp = p.pcb.footprints.get(id)?;
    let def = p.library.get(&fp.component_id)?;
    let (w, h) = rot.apply_size(
        def.footprint.courtyard_half_w * 2,
        def.footprint.courtyard_half_h * 2,
    );
    Some(Rect::centered(at, w, h))
}

fn rect_inside(r: &Rect, outline: &[Point], edge: Nm) -> bool {
    let corners = [
        r.min,
        Point::new(r.max.x, r.min.y),
        r.max,
        Point::new(r.min.x, r.max.y),
    ];
    corners.iter().all(|c| {
        point_in_polygon(*c, outline)
            && (0..outline.len()).all(|i| {
                point_segment_distance(*c, outline[i], outline[(i + 1) % outline.len()])
                    >= edge as f64
            })
    })
}

pub fn place(p: &Project, grid: Nm, spacing: Nm) -> Placement {
    let mut out = Placement::default();
    let outline = &p.pcb.board.outline;
    let Some(bb) = Rect::from_points(outline) else {
        for id in p.pcb.footprints.keys() {
            out.unplaced
                .push((id.clone(), "board outline missing".into()));
        }
        return out;
    };
    // Net adjacency between footprints.
    let mut fp_nets: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for fp in p.pcb.footprints.values() {
        for net in fp.pad_nets.values() {
            fp_nets
                .entry(fp.id.as_str())
                .or_default()
                .insert(net.as_str());
        }
    }
    let degree = |id: &str| fp_nets.get(id).map_or(0, |s| s.len());
    let plan = intel::plan(p);
    let mut placed: Vec<(Id, Rect, Point, Rotation)> = p
        .pcb
        .footprints
        .values()
        .filter(|f| f.locked)
        .filter_map(|f| {
            courtyard(p, &f.id, f.position, f.rotation)
                .map(|r| (f.id.clone(), r, f.position, f.rotation))
        })
        .collect();
    let mut order: Vec<&str> = p
        .pcb
        .footprints
        .values()
        .filter(|f| !f.locked)
        .map(|f| f.id.as_str())
        .collect();
    order.sort_by_key(|id| {
        (
            plan.role(id).phase(),
            std::cmp::Reverse(degree(id)),
            p.pcb.footprints[*id].refdes.clone(),
        )
    });

    let center = bb.center();
    let corners = intel::corner_targets(&bb, mm(intel::MOUNT_HOLE_INSET_MM));
    for id in order {
        let mut best: Option<(f64, Point, Rotation)> = None;
        let mut y = bb.min.y + grid;
        while y < bb.max.y {
            let mut x = bb.min.x + grid;
            while x < bb.max.x {
                let at = Point::new(x, y);
                for rot in [Rotation::R0, Rotation::R90] {
                    let Some(r) = courtyard(p, id, at, rot) else {
                        continue;
                    };
                    if !rect_inside(&r, outline, p.constraints.board_edge_clearance) {
                        continue;
                    }
                    if placed
                        .iter()
                        .any(|(_, o, _, _)| o.inflate(spacing).intersects(&r))
                    {
                        continue;
                    }
                    // Cost: distance to connected placed parts; otherwise distance to centre.
                    let my = fp_nets.get(id);
                    let mut cost = 0.0;
                    let mut linked = false;
                    for (oid, _, opos, _) in &placed {
                        let shared = match (my, fp_nets.get(oid.as_str())) {
                            (Some(a), Some(b)) => a.intersection(b).count(),
                            _ => 0,
                        };
                        if shared > 0 {
                            linked = true;
                            cost += shared as f64 * at.dist(*opos);
                        }
                    }
                    if !linked {
                        cost = at.dist(center) * 0.5;
                    }
                    match plan.role(id) {
                        Role::MountingHole => {
                            cost = corners
                                .iter()
                                .map(|c| at.dist(*c))
                                .fold(f64::INFINITY, f64::min);
                        }
                        Role::Connector => cost = 20.0 * intel::edge_distance(at, outline),
                        _ => {}
                    }
                    // Electrical snap: stay next to the pad this part serves (decoupling,
                    // crystal load, ESD); weighted so it dominates general connectivity.
                    if let Some(cands) = plan.anchors.get(id) {
                        let d = cands
                            .iter()
                            .filter_map(|c| {
                                let (_, _, apos, arot) = placed
                                    .iter()
                                    .find(|(pid, _, _, _)| *pid == c.footprint_id)?;
                                intel::pad_at(p, &c.footprint_id, *apos, *arot, &c.pad)
                            })
                            .map(|pp| at.dist(pp))
                            .fold(f64::INFINITY, f64::min);
                        if d.is_finite() {
                            cost += 10.0 * d;
                        }
                    }
                    if rot != Rotation::R0 {
                        cost += 1.0; // prefer the library orientation on ties
                    }
                    if best.is_none_or(|b| cost < b.0) {
                        best = Some((cost, at, rot));
                    }
                }
                x += grid;
            }
            y += grid;
        }
        match best {
            Some((_, at, rot)) => {
                let r = courtyard(p, id, at, rot).expect("checked above");
                let keep = if plan.role(id) == Role::MountingHole {
                    Rect::centered(
                        at,
                        mm(intel::MOUNT_HOLE_KEEPOUT_MM * 2.0),
                        mm(intel::MOUNT_HOLE_KEEPOUT_MM * 2.0),
                    )
                    .union(&r)
                } else {
                    r
                };
                placed.push((id.to_string(), keep, at, rot));
                out.moves.push((id.to_string(), at, rot));
            }
            None => out
                .unplaced
                .push((id.to_string(), "no free position inside the board".into())),
        }
    }
    out
}
