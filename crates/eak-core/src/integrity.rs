//! Structural integrity of EAK-IR: every reference resolves, ids match their keys,
//! geometry values are sane. This is data integrity, not electrical verification
//! (that lives in `eak-validation`).

use crate::geom::polygon_self_intersects;
use crate::ir::Project;
use crate::pcb_objects::{valid_sweep, validate_outline_arcs};

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
    for (k, s) in &p.schematic.sheets {
        if let Some(f) = &s.format {
            for problem in f.validate() {
                e.push(format!("sheet {k} drawing format: {problem}"));
            }
        }
    }
    for (k, g) in &p.schematic.graphics {
        if &g.id != k {
            e.push(format!("sheet graphic key {k} != id {}", g.id));
        }
        if !p.schematic.sheets.contains_key(&g.sheet_id) {
            e.push(format!("sheet graphic {k} on missing sheet"));
        }
        for problem in g.graphic.validate() {
            e.push(format!("sheet graphic {k}: {problem}"));
        }
    }
    for (k, b) in &p.schematic.buses {
        if &b.id != k {
            e.push(format!("bus key {k} != id {}", b.id));
        }
        if !p.schematic.sheets.contains_key(&b.sheet_id) {
            e.push(format!("bus {k} on missing sheet"));
        }
        if b.name.trim().is_empty() {
            e.push(format!("bus {k} has an empty name"));
        }
        if b.points.len() < 2 || b.points.windows(2).any(|w| w[0] == w[1]) {
            e.push(format!(
                "bus {k} needs at least 2 points and no zero-length segment"
            ));
        }
    }
    for (k, be) in &p.schematic.bus_entries {
        if &be.id != k {
            e.push(format!("bus entry key {k} != id {}", be.id));
        }
        match p.schematic.buses.get(&be.bus_id) {
            None => e.push(format!(
                "bus entry {k} references missing bus {}",
                be.bus_id
            )),
            Some(b) if b.sheet_id != be.sheet_id => {
                e.push(format!("bus entry {k} is not on its bus's sheet"))
            }
            Some(_) => {}
        }
        if be.bus_point == be.wire_point {
            e.push(format!("bus entry {k} has zero length"));
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
        if t.arc_deg.is_some_and(|s| !valid_sweep(s)) {
            e.push(format!(
                "track {k} arc sweep must be finite, non-zero and below a full turn"
            ));
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
    e.extend(validate_outline_arcs(o, &p.pcb.board.outline_arcs));
    for (k, r) in &p.pcb.regions {
        if &r.id != k {
            e.push(format!("region key {k} != id {}", r.id));
        }
        e.extend(r.validate().into_iter().map(|m| format!("region {k}: {m}")));
    }
    for (k, t) in &p.pcb.texts {
        if &t.id != k {
            e.push(format!("PCB text key {k} != id {}", t.id));
        }
        e.extend(
            t.validate()
                .into_iter()
                .map(|m| format!("PCB text {k}: {m}")),
        );
    }
    for (k, d) in &p.pcb.dimensions {
        if &d.id != k {
            e.push(format!("dimension key {k} != id {}", d.id));
        }
        e.extend(
            d.validate()
                .into_iter()
                .map(|m| format!("dimension {k}: {m}")),
        );
    }
    for (k, c) in &p.pcb.cutouts {
        if &c.id != k {
            e.push(format!("cut-out key {k} != id {}", c.id));
        }
        e.extend(
            c.validate()
                .into_iter()
                .map(|m| format!("cut-out {k}: {m}")),
        );
    }
    e.extend(p.pcb.board.stackup.validate());
    let c = &p.constraints;
    for cell in &c.clearance_matrix {
        if cell.clearance <= 0 || cell.a > cell.b {
            e.push(format!(
                "clearance matrix cell {:?}/{:?} must have a <= b and a positive clearance",
                cell.a, cell.b
            ));
        }
    }
    for (i, cell) in c.clearance_matrix.iter().enumerate() {
        if c.clearance_matrix[..i]
            .iter()
            .any(|o| o.a == cell.a && o.b == cell.b)
        {
            e.push(format!(
                "clearance matrix cell {:?}/{:?} appears twice",
                cell.a, cell.b
            ));
        }
    }
    for r in &c.net_pair_rules {
        if r.clearance <= 0 || r.net_a >= r.net_b {
            e.push(format!(
                "net pair rule {}/{} must name two different nets in order with a positive clearance",
                r.net_a, r.net_b
            ));
        }
    }
    e
}
