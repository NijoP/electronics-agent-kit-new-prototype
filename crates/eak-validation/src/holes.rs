//! Drilled holes and the spacing rules around them, shared by DRC (project constraints) and
//! DFM (fabrication profile): copper-to-hole (DRC-018), hole-to-hole (DRC-019 / DFM-005)
//! and silkscreen-to-board-edge (DRC-020).

use crate::report::{finding, Finding, Severity::*};
use eak_core::geom::{point_in_polygon, to_mm};
use eak_core::{CopperLayer, Id, Nm, PadKind, Point, Project, Side};
use eak_engineering::copper::{self, distance, Core, ItemKind, Shape};
use eak_engineering::silk;
use std::collections::BTreeSet;

/// One drilled hole: a via barrel or a plated through-hole pad.
#[derive(Debug, Clone)]
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
    fn is_owner(&self, it: &copper::CopperItem) -> bool {
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

fn mm(v: f64) -> String {
    format!("{:.3} mm", v / 1e6)
}

/// DRC-018: copper of another net (or unconnected copper) closer than `min` to the edge of a
/// drilled hole, on the layers the hole's own copper spans.
pub fn copper_to_hole(p: &Project, min: Nm) -> Vec<Finding> {
    let items = copper::items(p);
    let mut f = Vec::new();
    for h in holes(p) {
        let layers: BTreeSet<CopperLayer> = items
            .iter()
            .filter(|it| h.is_owner(it))
            .map(|it| it.layer)
            .collect();
        let hs = h.shape();
        let reach = hs.bbox().inflate(min);
        let mut seen = BTreeSet::new();
        for it in &items {
            if h.is_owner(it) || !layers.contains(&it.layer) {
                continue;
            }
            if !h.net.is_empty() && it.net == h.net {
                continue;
            }
            if !reach.intersects(&it.shape.bbox()) {
                continue;
            }
            let d = distance(&hs, &it.shape);
            if d < min as f64 - 1.0 && seen.insert(it.label()) {
                f.push(
                    finding(
                        "DRC-018",
                        Error,
                        format!(
                            "{} ({}) is {} from the hole of {} on {} (minimum {})",
                            it.label(),
                            it.net,
                            mm(d.max(0.0)),
                            h.label,
                            it.layer.name(),
                            mm(min as f64)
                        ),
                        vec![h.object_id.clone(), it.object_id.clone()],
                        Some(h.center),
                    )
                    .key(&format!("{}|{}", h.pad, it.label()))
                    .mm_min(d.max(0.0) / 1e6, to_mm(min))
                    .fix(format!(
                        "Move {} or the hole of {} apart to leave at least {} between the copper and the drill edge.",
                        it.label(),
                        h.label,
                        mm(min as f64)
                    )),
                );
            }
        }
    }
    f
}

/// Hole-to-hole edge spacing below `min`; `rule_id` lets DRC and DFM share the geometry.
pub fn hole_to_hole(p: &Project, min: Nm, rule_id: &str, basis: &str) -> Vec<Finding> {
    let hs = holes(p);
    let mut f = Vec::new();
    for (i, a) in hs.iter().enumerate() {
        for b in &hs[i + 1..] {
            // Two holes of one net drilled at the same point (stacked vias left by routing) are
            // one physical hole: reported as a duplicate drill, not a spacing violation.
            if a.center == b.center && !a.net.is_empty() && a.net == b.net {
                f.push(
                    finding(
                        rule_id,
                        Warning,
                        format!(
                            "holes of {} and {} ({}) are drilled at the same point (duplicate drill)",
                            a.label, b.label, a.net
                        ),
                        vec![a.object_id.clone(), b.object_id.clone()],
                        Some(a.center),
                    )
                    .key(&format!("dup|{}|{}", a.label, b.label))
                    .fix(format!("Delete one of {} and {}.", a.label, b.label)),
                );
                continue;
            }
            let d = distance(&a.shape(), &b.shape());
            if d < min as f64 - 1.0 {
                f.push(
                    finding(
                        rule_id,
                        Error,
                        format!(
                            "holes of {} and {} are {} apart edge to edge (minimum {}, {basis})",
                            a.label,
                            b.label,
                            mm(d.max(0.0)),
                            mm(min as f64)
                        ),
                        vec![a.object_id.clone(), b.object_id.clone()],
                        Some(Point::new(
                            (a.center.x + b.center.x) / 2,
                            (a.center.y + b.center.y) / 2,
                        )),
                    )
                    .key(&format!("{}|{}", a.label, b.label))
                    .mm_min(d.max(0.0) / 1e6, to_mm(min))
                    .fix(format!(
                        "Move {} and {} apart so the drill edges are at least {} apart.",
                        a.label,
                        b.label,
                        mm(min as f64)
                    )),
                );
            }
        }
    }
    f
}

/// DRC-020: generated silkscreen legend closer than `min` to the board outline, or outside
/// it. One finding per side, at the worst stroke.
pub fn silk_to_edge(p: &Project, min: Nm) -> Vec<Finding> {
    let o = &p.pcb.board.outline;
    if o.len() < 3 {
        return vec![];
    }
    let edges: Vec<Shape> = (0..o.len())
        .map(|i| Shape {
            core: Core::Segment(o[i], o[(i + 1) % o.len()]),
            radius: 0.0,
        })
        .collect();
    let mut f = Vec::new();
    for side in [Side::Top, Side::Bottom] {
        let mut worst: Option<(f64, Point)> = None;
        for s in silk::legend(p, side) {
            let sh = s.shape();
            let d = if point_in_polygon(s.a, o) && point_in_polygon(s.b, o) {
                edges
                    .iter()
                    .map(|e| distance(&sh, e))
                    .fold(f64::INFINITY, f64::min)
            } else {
                0.0
            };
            let worse = match worst {
                None => true,
                Some((w, _)) => d < w,
            };
            if d < min as f64 - 1.0 && worse {
                worst = Some((d, s.a));
            }
        }
        if let Some((d, at)) = worst {
            f.push(
                finding(
                    "DRC-020",
                    Warning,
                    format!(
                        "{side:?} silkscreen comes {} from the board edge (minimum {})",
                        mm(d.max(0.0)),
                        mm(min as f64)
                    ),
                    vec![],
                    Some(at),
                )
                .key(&format!("{side:?}"))
                .mm_min(d.max(0.0) / 1e6, to_mm(min))
                .fix("Move the parts near the edge inward; legend this close to the edge is clipped or printed off the board."),
            );
        }
    }
    f
}
