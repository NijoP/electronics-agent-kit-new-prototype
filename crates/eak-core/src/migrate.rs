//! Schema migration. Each step rewrites a document from version N to N+1 with identical
//! meaning; newer or missing versions are rejected rather than guessed.
//!
//! * v1 → v2: `pcb.board.stackup` changes from summary fields (`copper_layers`,
//!   `outer_copper_oz`) to per-layer data (`layers`). A v1 board always had two copper
//!   layers of `outer_copper_oz` each; the core thickness is what that copper leaves of
//!   `board_thickness`, and its er is the FR-4 nominal (v1 recorded none).
//! * v2 → v3 (ADR-0040): schematic primitives. Every project library symbol goes through
//!   [`SymbolDef::upgrade_box`] (its box becomes a rectangle graphic, its pins get the
//!   orientation and length that reach the box; no pin moves), and the new collections
//!   `schematic.graphics`, `schematic.buses` and `schematic.bus_entries` are added empty. Labels
//!   load as global, sheets without a drawing format and instances with default field
//!   visibility, which is exactly the v2 meaning.
//! * v3 → v4 (ADR-0042): PCB objects. The new collections `pcb.regions`, `pcb.texts`,
//!   `pcb.dimensions` and `pcb.cutouts` are added empty. Tracks without `arc_deg` are straight and
//!   a board without `outline_arcs` has straight edges, which is exactly the v3 meaning.

use crate::ir::{Stackup, FR4_ER, SCHEMA_VERSION};
use crate::library::SymbolDef;
use crate::CoreError;
use serde_json::Value;

pub fn migrate(mut raw: Value) -> Result<Value, CoreError> {
    let found = raw
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| CoreError::Parse("missing schema_version".into()))? as u32;
    if found == 0 || found > SCHEMA_VERSION {
        return Err(CoreError::UnsupportedSchema {
            found,
            supported: SCHEMA_VERSION,
        });
    }
    if found == 1 {
        raw = v1_to_v2(raw)?;
    }
    if found <= 2 {
        raw = v2_to_v3(raw)?;
    }
    if found <= 3 {
        raw = v3_to_v4(raw);
    }
    Ok(raw)
}

fn v1_to_v2(mut raw: Value) -> Result<Value, CoreError> {
    let bad = |m: &str| CoreError::Parse(format!("v1 stackup cannot be migrated: {m}"));
    if let Some(s) = raw.pointer_mut("/pcb/board/stackup") {
        let layers = s.get("copper_layers").and_then(Value::as_u64);
        if layers != Some(2) {
            return Err(bad("v1 boards have exactly 2 copper layers"));
        }
        let thickness = s
            .get("board_thickness")
            .and_then(Value::as_i64)
            .ok_or_else(|| bad("missing board_thickness"))?;
        let oz = s
            .get("outer_copper_oz")
            .and_then(Value::as_f64)
            .ok_or_else(|| bad("missing outer_copper_oz"))?;
        let dielectric = s
            .get("dielectric")
            .and_then(Value::as_str)
            .ok_or_else(|| bad("missing dielectric"))?;
        *s = serde_json::to_value(Stackup::two_layer(thickness, oz, dielectric, FR4_ER))
            .expect("stackup is always serialisable");
    }
    raw["schema_version"] = Value::from(2);
    Ok(raw)
}

fn v2_to_v3(mut raw: Value) -> Result<Value, CoreError> {
    let bad = |m: String| CoreError::Parse(format!("v2 project cannot be migrated: {m}"));
    if let Some(lib) = raw.get_mut("library").and_then(Value::as_object_mut) {
        for (id, def) in lib.iter_mut() {
            let Some(sym) = def.get_mut("symbol") else {
                continue;
            };
            let mut s: SymbolDef = serde_json::from_value(sym.clone())
                .map_err(|e| bad(format!("library symbol {id}: {e}")))?;
            s.upgrade_box();
            *sym = serde_json::to_value(s).expect("symbol is always serialisable");
        }
    }
    if let Some(sch) = raw.get_mut("schematic").and_then(Value::as_object_mut) {
        for key in ["graphics", "buses", "bus_entries"] {
            sch.entry(key)
                .or_insert_with(|| Value::Object(Default::default()));
        }
    }
    raw["schema_version"] = Value::from(3);
    Ok(raw)
}

fn v3_to_v4(mut raw: Value) -> Value {
    if let Some(pcb) = raw.get_mut("pcb").and_then(Value::as_object_mut) {
        for key in ["regions", "texts", "dimensions", "cutouts"] {
            pcb.entry(key)
                .or_insert_with(|| Value::Object(Default::default()));
        }
    }
    raw["schema_version"] = Value::from(4);
    raw
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{from_json, to_json, CopperLayer, Project};

    /// A schema-v1 project as written by EAK 0.1 (2-layer, 1.6 mm, 2 oz copper). The
    /// stackup is the only part of the schema that changed in v2, so a v1 file is a
    /// current document with the v1 stackup object and version number.
    fn v1_fixture() -> String {
        let mut v: Value = serde_json::from_str(&to_json(&Project::new("v1-board"))).unwrap();
        v["schema_version"] = Value::from(1);
        v["revision"] = Value::from(3);
        v["pcb"]["board"]["stackup"] = serde_json::json!({
            "copper_layers": 2,
            "board_thickness": 1600000,
            "outer_copper_oz": 2.0,
            "dielectric": "FR-4"
        });
        v.to_string()
    }

    #[test]
    fn round_trip_is_byte_stable() {
        let p = Project::new("demo");
        let a = to_json(&p);
        let q = from_json(&a).unwrap();
        assert_eq!(p, q);
        assert_eq!(a, to_json(&q));
    }

    #[test]
    fn v1_stackup_migrates_with_identical_meaning() {
        let p = from_json(&v1_fixture()).unwrap();
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        assert_eq!(p.revision, 3);
        let s = &p.pcb.board.stackup;
        assert_eq!(s.copper_layers(), 2);
        assert_eq!(s.board_thickness, 1_600_000);
        assert_eq!(s.dielectric, "FR-4");
        assert_eq!(s.copper_oz(CopperLayer::Top), Some(2.0));
        assert_eq!(s.copper_oz(CopperLayer::Bottom), Some(2.0));
        // 1.6 mm − 2 × 69.6 µm of 2 oz copper.
        assert_eq!(s.layers[0].dielectric_after_um, 1461);
        assert_eq!(s.layers[1].dielectric_after_um, 0);
        assert!(s.validate().is_empty(), "{:?}", s.validate());
        // The migrated file saves as v2 and is then byte-stable.
        let a = to_json(&p);
        assert_eq!(a, to_json(&from_json(&a).unwrap()));
    }

    #[test]
    fn v1_default_stackup_equals_v2_default() {
        let mut v: Value = serde_json::from_str(&v1_fixture()).unwrap();
        v["pcb"]["board"]["stackup"]["outer_copper_oz"] = Value::from(1.0);
        let p = from_json(&v.to_string()).unwrap();
        assert_eq!(p.pcb.board.stackup, crate::Stackup::default());
    }

    #[test]
    fn unmigratable_v1_stackup_is_rejected() {
        let mut v: Value = serde_json::from_str(&v1_fixture()).unwrap();
        v["pcb"]["board"]["stackup"]["copper_layers"] = Value::from(4);
        assert!(matches!(
            from_json(&v.to_string()),
            Err(CoreError::Parse(_))
        ));
    }

    #[test]
    fn invalid_v2_stackup_fails_integrity() {
        let mut p = Project::new("x");
        p.pcb.board.stackup.layers[0].copper_oz = 0.0;
        assert!(matches!(
            from_json(&to_json(&p)),
            Err(CoreError::Integrity(_))
        ));
    }

    #[test]
    fn newer_schema_is_rejected() {
        let mut v: Value = serde_json::from_str(&to_json(&Project::new("x"))).unwrap();
        v["schema_version"] = Value::from(99);
        let err = from_json(&v.to_string()).unwrap_err();
        assert!(matches!(
            err,
            CoreError::UnsupportedSchema { found: 99, .. }
        ));
    }

    /// A schema-v2 project with library box symbols: the symbol fields added in v3 are absent.
    fn v2_fixture() -> (Project, String) {
        let mut p = Project::new("v2-board");
        for id in ["R_0603", "LED_0603", "CONN_1x04_P2.54"] {
            let def = crate::builtin::get(id).expect("built-in part");
            p.library.insert(def.id.clone(), def);
        }
        let mut v: Value = serde_json::from_str(&to_json(&p)).unwrap();
        v["schema_version"] = Value::from(2);
        for def in v["library"].as_object_mut().unwrap().values_mut() {
            let sym = def["symbol"].as_object_mut().unwrap();
            sym.remove("graphics");
            for pin in sym["pins"].as_array_mut().unwrap() {
                for k in ["length", "orientation", "name_visible", "number_visible"] {
                    pin.as_object_mut().unwrap().remove(k);
                }
            }
        }
        for k in ["graphics", "buses", "bus_entries"] {
            v["schematic"].as_object_mut().unwrap().remove(k);
        }
        (p, v.to_string())
    }

    #[test]
    fn v2_box_symbols_migrate_to_their_rectangle_with_pins_unmoved() {
        let (current, v2) = v2_fixture();
        let p = from_json(&v2).unwrap();
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        for (id, def) in &p.library {
            let s = &def.symbol;
            assert_eq!(
                s.graphics,
                vec![crate::Graphic::body_box(s.body_half_w, s.body_half_h)],
                "{id}"
            );
            assert!(s
                .pins
                .iter()
                .all(|pin| pin.name_visible && pin.number_visible));
        }
        // Identical to the current built-ins: same pins, same positions, same body.
        assert_eq!(p, current);
        assert!(p.schematic.buses.is_empty() && p.schematic.bus_entries.is_empty());
        let a = to_json(&p);
        assert_eq!(a, to_json(&from_json(&a).unwrap()));
    }

    #[test]
    fn v2_symbol_that_cannot_be_read_is_rejected() {
        let (_, v2) = v2_fixture();
        let mut v: Value = serde_json::from_str(&v2).unwrap();
        v["library"]["R_0603"]["symbol"]["pins"] = Value::from("not pins");
        assert!(matches!(
            from_json(&v.to_string()),
            Err(CoreError::Parse(_))
        ));
    }

    /// A v4 project holding one of every schema-v4 PCB object (arc track, arc outline edge,
    /// region, text, dimension, cut-out).
    fn v4_project() -> Project {
        use crate::{
            Cutout, DimensionUnit, DrawLayer, OutlineArc, PcbDimension, PcbText, Point, Region,
            RegionObject, Rotation, Track,
        };
        let mut p = Project::new("v4-board");
        let mm = Point::mm;
        p.pcb.board.outline = vec![mm(0.0, 0.0), mm(40.0, 0.0), mm(40.0, 30.0), mm(0.0, 30.0)];
        p.pcb.board.outline_arcs = vec![OutlineArc {
            edge: 1,
            sweep_deg: 60.0,
        }];
        p.pcb.tracks.insert(
            "trk_arc".into(),
            Track {
                id: "trk_arc".into(),
                layer: CopperLayer::Top,
                width: 250_000,
                a: mm(5.0, 5.0),
                b: mm(15.0, 5.0),
                net: "N1".into(),
                arc_deg: Some(90.0),
            },
        );
        p.pcb.regions.insert(
            "rgn_1".into(),
            Region {
                id: "rgn_1".into(),
                name: "antenna".into(),
                outline: vec![mm(20.0, 20.0), mm(30.0, 20.0), mm(30.0, 28.0)],
                layers: vec![CopperLayer::Top],
                forbid: [RegionObject::Tracks, RegionObject::Zones].into(),
                min_track_width: None,
                max_track_width: None,
            },
        );
        p.pcb.texts.insert(
            "txt_1".into(),
            PcbText {
                id: "txt_1".into(),
                layer: DrawLayer::TopSilk,
                position: mm(20.0, 3.0),
                text: "REV A".into(),
                size: 1_000_000,
                stroke_width: 0,
                rotation: Rotation::R0,
            },
        );
        p.pcb.dimensions.insert(
            "dim_1".into(),
            PcbDimension {
                id: "dim_1".into(),
                layer: DrawLayer::Documentation,
                a: mm(0.0, 0.0),
                b: mm(40.0, 0.0),
                offset: -3_000_000,
                unit: DimensionUnit::Mm,
                precision: 1,
                text_size: 0,
            },
        );
        p.pcb.cutouts.insert(
            "cut_1".into(),
            Cutout {
                id: "cut_1".into(),
                outline: vec![mm(8.0, 15.0), mm(12.0, 15.0), mm(12.0, 19.0), mm(8.0, 19.0)],
            },
        );
        p
    }

    #[test]
    fn v4_objects_round_trip_byte_stable() {
        let p = v4_project();
        assert!(
            crate::integrity::check(&p).is_empty(),
            "{:?}",
            crate::integrity::check(&p)
        );
        let a = to_json(&p);
        let q = from_json(&a).unwrap();
        assert_eq!(p, q);
        assert_eq!(a, to_json(&q));
        let v: Value = serde_json::from_str(&a).unwrap();
        assert_eq!(v["pcb"]["tracks"]["trk_arc"]["arc_deg"], 90.0);
        assert_eq!(v["pcb"]["board"]["outline_arcs"][0]["edge"], 1);
        assert_eq!(
            v["pcb"]["regions"]["rgn_1"]["forbid"],
            serde_json::json!(["tracks", "zones"])
        );
        // A straight track and a board without arc edges keep their v3 form.
        let plain = to_json(&Project::new("plain"));
        assert!(!plain.contains("arc_deg") && !plain.contains("outline_arcs"));
    }

    #[test]
    fn v3_project_migrates_to_v4_with_identical_meaning() {
        let mut current = v4_project();
        // Remove what v3 cannot express; what remains is a v3 design.
        current.pcb.board.outline_arcs.clear();
        for t in current.pcb.tracks.values_mut() {
            t.arc_deg = None;
        }
        current.pcb.regions.clear();
        current.pcb.texts.clear();
        current.pcb.dimensions.clear();
        current.pcb.cutouts.clear();
        let mut v: Value = serde_json::from_str(&to_json(&current)).unwrap();
        v["schema_version"] = Value::from(3);
        for k in ["regions", "texts", "dimensions", "cutouts"] {
            v["pcb"].as_object_mut().unwrap().remove(k);
        }
        let v3 = v.to_string();
        assert!(!v3.contains("\"cutouts\"") && !v3.contains("\"regions\""));
        let p = from_json(&v3).expect("v3 project loads");
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        assert_eq!(p, current);
        let a = to_json(&p);
        assert_eq!(a, to_json(&from_json(&a).unwrap()));
    }

    #[test]
    fn invalid_v4_objects_fail_integrity() {
        let mut p = v4_project();
        p.pcb.tracks.get_mut("trk_arc").unwrap().arc_deg = Some(360.0);
        p.pcb.regions.get_mut("rgn_1").unwrap().forbid.clear();
        p.pcb.board.outline_arcs[0].edge = 9;
        let Err(CoreError::Integrity(problems)) = from_json(&to_json(&p)) else {
            panic!("integrity must refuse the project");
        };
        assert_eq!(problems.len(), 3, "{problems:?}");
    }

    #[test]
    fn corrupt_file_fails_safely() {
        assert!(matches!(from_json("{not json"), Err(CoreError::Parse(_))));
        assert!(matches!(from_json("{}"), Err(CoreError::Parse(_))));
    }
}
