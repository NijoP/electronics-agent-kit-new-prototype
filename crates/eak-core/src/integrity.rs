//! Structural integrity of EAK-IR: every reference resolves, ids match their keys,
//! geometry values are sane. This is data integrity, not electrical verification
//! (that lives in `eak-validation`).

use crate::geom::polygon_self_intersects;
use crate::ir::Project;

pub fn check(p: &Project) -> Vec<String> {
    let mut e = Vec::new();
    for (k, def) in &p.library {
        if &def.id != k {
            e.push(format!(
                "library key {k} does not match component id {}",
                def.id
            ));
        }
        e.extend(def.validate());
    }
    for (k, s) in &p.schematic.sheets {
        if &s.id != k {
            e.push(format!("sheet key {k} != id {}", s.id));
        }
    }
    for (k, s) in &p.schematic.symbols {
        if &s.id != k {
            e.push(format!("symbol key {k} != id {}", s.id));
        }
        if !p.schematic.sheets.contains_key(&s.sheet_id) {
            e.push(format!(
                "symbol {} on missing sheet {}",
                s.refdes, s.sheet_id
            ));
        }
        if !p.library.contains_key(&s.component_id) {
            e.push(format!(
                "symbol {} uses missing component {}",
                s.refdes, s.component_id
            ));
        }
    }
    for (k, w) in &p.schematic.wires {
        if &w.id != k {
            e.push(format!("wire key {k} != id {}", w.id));
        }
        if !p.schematic.sheets.contains_key(&w.sheet_id) {
            e.push(format!("wire {k} on missing sheet"));
        }
        if w.a == w.b {
            e.push(format!("wire {k} has zero length"));
        }
    }
    for (k, l) in &p.schematic.labels {
        if &l.id != k {
            e.push(format!("label key {k} != id {}", l.id));
        }
        if l.name.trim().is_empty() {
            e.push(format!("label {k} has an empty net name"));
        }
        if !p.schematic.sheets.contains_key(&l.sheet_id) {
            e.push(format!("label {k} on missing sheet"));
        }
    }
    for (k, nc) in &p.schematic.no_connects {
        if !p.schematic.symbols.contains_key(&nc.symbol_id) {
            e.push(format!("no-connect {k} references missing symbol"));
        }
    }
    for (k, f) in &p.pcb.footprints {
        if &f.id != k {
            e.push(format!("footprint key {k} != id {}", f.id));
        }
        if !p.library.contains_key(&f.component_id) {
            e.push(format!(
                "footprint {} uses missing component {}",
                f.refdes, f.component_id
            ));
        }
        if !p.schematic.symbols.contains_key(&f.symbol_id) {
            e.push(format!(
                "footprint {} realises missing symbol {}",
                f.refdes, f.symbol_id
            ));
        }
    }
    for (k, t) in &p.pcb.tracks {
        if &t.id != k {
            e.push(format!("track key {k} != id {}", t.id));
        }
        if t.width <= 0 {
            e.push(format!("track {k} has non-positive width"));
        }
        if t.a == t.b {
            e.push(format!("track {k} has zero length"));
        }
    }
    for (k, v) in &p.pcb.vias {
        if &v.id != k {
            e.push(format!("via key {k} != id {}", v.id));
        }
        if v.drill <= 0 || v.diameter <= v.drill {
            e.push(format!("via {k} has invalid drill/diameter"));
        }
    }
    for (k, z) in &p.pcb.zones {
        if &z.id != k {
            e.push(format!("zone key {k} != id {}", z.id));
        }
        if z.outline.len() < 3 || polygon_self_intersects(&z.outline) {
            e.push(format!("zone {k} outline is not a simple polygon"));
        }
    }
    let o = &p.pcb.board.outline;
    if !o.is_empty() && (o.len() < 3 || polygon_self_intersects(o)) {
        e.push("board outline is not a simple polygon".into());
    }
    e.extend(p.pcb.board.stackup.validate());
    e
}
