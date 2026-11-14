//! Rust-produced 3D scene: board slab and component bodies derived from EAK-IR.
//! The frontend renders this; it is never electrical truth. Components with unknown
//! body height are listed explicitly rather than drawn with an invented height.

use eak_core::{geom::to_mm, Id, Project, Rect, Side};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Body {
    /// Footprint id — the same canonical id the PCB editor selects (cross-probing).
    pub footprint_id: Id,
    pub symbol_id: Id,
    pub refdes: String,
    pub side: Side,
    /// Centre and size in millimetres; z is measured up from the board top surface
    /// (negative for bottom-side parts).
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub sx: f64,
    pub sy: f64,
    pub sz: f64,
    pub kind: String,
    pub color: String,
    /// Real 3D model reference from the library, if any; the box above is the fallback
    /// and still carries the footprint-derived envelope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<eak_core::ModelRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub outline_mm: Vec<(f64, f64)>,
    pub thickness_mm: f64,
    pub bodies: Vec<Body>,
    pub missing_models: Vec<String>,
}

pub fn body_rect(p: &Project, fp_id: &str) -> Option<(Rect, f64)> {
    let fp = p.pcb.footprints.get(fp_id)?;
    let def = p.library.get(&fp.component_id)?;
    let h = def.footprint.body_height? as f64;
    let (w, d) = fp.rotation.apply_size(
        def.footprint.courtyard_half_w * 2,
        def.footprint.courtyard_half_h * 2,
    );
    // Body footprint approximated as 85 % of the courtyard.
    Some((Rect::centered(fp.position, w * 85 / 100, d * 85 / 100), h))
}

pub fn build(p: &Project) -> Scene {
    let t = to_mm(p.pcb.board.stackup.board_thickness);
    let mut bodies = vec![];
    let mut missing = vec![];
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        match body_rect(p, &fp.id) {
            Some((r, h)) => {
                let hz = to_mm(h as i64);
                let c = r.center();
                bodies.push(Body {
                    footprint_id: fp.id.clone(),
                    symbol_id: fp.symbol_id.clone(),
                    refdes: fp.refdes.clone(),
                    side: fp.side,
                    x: to_mm(c.x),
                    y: to_mm(c.y),
                    z: if fp.side == Side::Top {
                        hz / 2.0
                    } else {
                        -t - hz / 2.0
                    },
                    sx: to_mm(r.width()),
                    sy: to_mm(r.height()),
                    sz: hz,
                    kind: def.model3d.kind.clone(),
                    color: def.model3d.color.clone(),
                    model: def.model3d.model_ref.clone(),
                });
            }
            None => missing.push(fp.refdes.clone()),
        }
    }
    Scene {
        outline_mm: p
            .pcb
            .board
            .outline
            .iter()
            .map(|q| (to_mm(q.x), to_mm(q.y)))
            .collect(),
        thickness_mm: t,
        bodies,
        missing_models: missing,
    }
}
