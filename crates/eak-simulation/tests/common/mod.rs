//! Builds small EAK-IR schematics directly (read-only simulation tests need no engine).
#![allow(dead_code)]
use eak_core::{builtin, new_id, NetLabel, Point, Project, Rotation, SymbolInstance};

pub struct Circuit {
    pub p: Project,
    sheet: String,
}

impl Circuit {
    pub fn new() -> Circuit {
        let mut p = Project::new("sim-test");
        for c in builtin::components() {
            p.library.insert(c.id.clone(), c);
        }
        let sheet = p.first_sheet_id().unwrap().clone();
        Circuit { p, sheet }
    }

    /// Place a two-pin part at `x` mm (pins at x±5.08 mm) and label pin 1 / pin 2 nets.
    pub fn part(&mut self, comp: &str, refdes: &str, value: &str, x: f64, net1: &str, net2: &str) {
        let id = new_id("sym");
        self.p.schematic.symbols.insert(
            id.clone(),
            SymbolInstance {
                id,
                sheet_id: self.sheet.clone(),
                component_id: comp.into(),
                refdes: refdes.into(),
                value: value.into(),
                position: Point::mm(x, 20.0),
                rotation: Rotation::R0,
                mirror: false,
                field_visibility: Default::default(),
            },
        );
        self.label(net1, x - 5.08);
        self.label(net2, x + 5.08);
    }

    fn label(&mut self, name: &str, x: f64) {
        let id = new_id("lbl");
        self.p.schematic.labels.insert(
            id.clone(),
            NetLabel {
                id,
                sheet_id: self.sheet.clone(),
                name: name.into(),
                position: Point::mm(x, 20.0),
                kind: Default::default(),
                scope: Default::default(),
                power_style: None,
            },
        );
    }
}

pub fn volts(r: &eak_simulation::SimResult, net: &str) -> f64 {
    r.nodes
        .iter()
        .find(|n| n.net == net)
        .unwrap_or_else(|| panic!("no node {net}: {:?}", r.nodes))
        .volts
}
