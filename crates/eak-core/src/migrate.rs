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
//! * v4 → v5 (ADR-0044): per-pad solder-mask and paste expansion (`PadDef::mask_expansion`,
//!   `PadDef::paste_expansion`, absent = board default and no paste change) and the saved route
//!   tool options `pcb.routing`, added with their defaults (45-degree corners, walkaround,
//!   net-class width and via, loop removal on). A v4 pad has no expansion of its own, which is
//!   exactly the v4 meaning.
//! * v5 → v6 (ADR-0045): object groups. The new collection `schematic.groups` is added empty;
//!   a v5 project had no groups, so nothing else changes.
//! * v6 → v7 (ADR-0046): rule configuration. `constraints.rules` (per-rule enable and severity),
//!   `constraints.clearance_matrix` and `constraints.net_pair_rules` are new and absent means
//!   empty: every rule enabled at its default severity and no extra clearances, exactly the v6
//!   meaning. Only the version number changes; a v7 file is refused by v6 builds, which would
//!   otherwise drop a disabled rule's configuration and run it as enabled.
//! * v7 → v8 (ADR-0055): routing and engineering-check inputs. `constraints.diff_pairs`,
//!   `constraints.via_spans`, `net_classes[].target_z0_ohm` and the engineering inputs
//!   (`ambient_temp_c`, `via_plating_um`, `net_max_drop_v`, `part_dissipation_w`, `insulation`,
//!   `enclosure`) are new and absent means undeclared: no pairs, through vias only, and every
//!   check that needs an input UNKNOWN, exactly the v7 meaning. Only the version number changes;
//!   a v8 file is refused by v7 builds, which would silently drop the declared rules.
//! * v8 → v9 (ADR-0057): the structured requirement record `requirement_set` is new and absent
//!   means empty: every requirement field UNKNOWN and no rails or interfaces declared, exactly the
//!   v8 meaning (v8 recorded none). Only the version number changes; a v9 file is refused by v8
//!   builds, which would silently drop the record.
//! * v9 → v10 (ADR-0059): the reference-plane inputs `constraints.return_via_max_distance` and
//!   `constraints.plane_min_neck_width` are new and absent means undeclared: DRC-030 and the plane
//!   neck check are UNKNOWN, never PASS, exactly the v9 meaning. Only the version number changes; a
//!   v10 file is refused by v9 builds, which would silently drop the declared limits.
//! * v10 → v11 (ADR-0060): a requirement entry's `conflicts` (competing values not yet resolved)
//!   is new and absent means none, and the `domain.*` classification entries are new catalogue
//!   keys; a v10 record has neither, exactly the v11 meaning. Only the version number changes; a
//!   v11 file is refused by v10 builds, which would silently drop the conflicts and so show a
//!   disputed value as settled.

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
    if found <= 4 {
        raw = v4_to_v5(raw);
    }
    if found <= 5 {
        raw = v5_to_v6(raw);
    }
    // v6 → v7 up to v9 → v10 and v10 → v11 only add optional data: the version number is all that
    // changes.
    if found <= 10 {
        raw["schema_version"] = Value::from(11);
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

fn v4_to_v5(mut raw: Value) -> Value {
    if let Some(pcb) = raw.get_mut("pcb").and_then(Value::as_object_mut) {
        pcb.entry("routing").or_insert_with(|| {
            serde_json::to_value(crate::ir::RoutingOptions::default())
                .expect("routing options are always serialisable")
        });
    }
    raw["schema_version"] = Value::from(5);
    raw
}

fn v5_to_v6(mut raw: Value) -> Value {
    if let Some(sch) = raw.get_mut("schematic").and_then(Value::as_object_mut) {
        sch.entry("groups")
            .or_insert_with(|| Value::Object(Default::default()));
    }
    raw["schema_version"] = Value::from(6);
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
            let mut def = crate::builtin::get(id).expect("built-in part");
            // A v2 file only held box symbols: reduce the drawn built-ins (slice B2) to the
            // box v2 could express, so the migrated project can equal this one.
            def.symbol.graphics.clear();
            for pin in &mut def.symbol.pins {
                pin.length = 0;
                pin.orientation = Default::default();
                pin.name_visible = true;
                pin.number_visible = true;
            }
            def.symbol.upgrade_box();
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

    /// A v5 project: one library part with per-pad mask and paste expansion, and saved route
    /// tool options that are not the defaults.
    fn v5_project() -> Project {
        use crate::{BendMode, ConflictMode, ViaSize};
        let mut p = v4_project();
        let mut def = crate::builtin::get("CONN_1x02_P2.54").expect("built-in connector");
        def.symbol.upgrade_box();
        def.footprint.pads[0].mask_expansion = Some(-20_000);
        let mut smd = crate::builtin::components()
            .into_iter()
            .find(|c| {
                c.footprint
                    .pads
                    .iter()
                    .any(|p| p.kind == crate::PadKind::Smd)
            })
            .expect("a built-in SMD part");
        smd.symbol.upgrade_box();
        let pad = smd
            .footprint
            .pads
            .iter_mut()
            .find(|p| p.kind == crate::PadKind::Smd)
            .unwrap();
        pad.mask_expansion = Some(75_000);
        pad.paste_expansion = Some(-50_000);
        p.library.insert(def.id.clone(), def);
        p.library.insert(smd.id.clone(), smd);
        p.pcb.routing = crate::RoutingOptions {
            bend: BendMode::Orthogonal90,
            conflict: ConflictMode::Push,
            width: Some(400_000),
            via: Some(ViaSize {
                drill: 300_000,
                diameter: 600_000,
            }),
            remove_loops: false,
        };
        p
    }

    #[test]
    fn v5_pad_expansion_and_routing_options_round_trip_byte_stable() {
        let p = v5_project();
        let a = to_json(&p);
        let q = from_json(&a).expect("v5 project loads");
        assert_eq!(q, p);
        assert_eq!(a, to_json(&q));
        assert!(a.contains("\"paste_expansion\": -50000"), "{a}");
        assert!(a.contains("\"conflict\": \"push\""));
    }

    #[test]
    fn v4_project_migrates_to_v5_with_identical_meaning() {
        let mut current = v5_project();
        // Remove what v4 cannot express; what remains is a v4 design.
        for def in current.library.values_mut() {
            for pad in &mut def.footprint.pads {
                pad.mask_expansion = None;
                pad.paste_expansion = None;
            }
        }
        current.pcb.routing = crate::RoutingOptions::default();
        let mut v: Value = serde_json::from_str(&to_json(&current)).unwrap();
        v["schema_version"] = Value::from(4);
        v["pcb"].as_object_mut().unwrap().remove("routing");
        let v4 = v.to_string();
        assert!(!v4.contains("\"routing\"") && !v4.contains("paste_expansion"));
        let p = from_json(&v4).expect("v4 project loads");
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        assert_eq!(p, current);
        assert!(p.pcb.routing.remove_loops);
        let a = to_json(&p);
        assert_eq!(a, to_json(&from_json(&a).unwrap()));
    }

    #[test]
    fn invalid_v5_expansion_and_routing_options_fail_integrity() {
        let mut p = v5_project();
        let conn = p.library.get_mut("CONN_1x02_P2.54").unwrap();
        // A through-hole pad has no paste; an expansion that closes the opening is refused.
        conn.footprint.pads[0].paste_expansion = Some(10_000);
        conn.footprint.pads[1].mask_expansion = Some(-5_000_000);
        p.pcb.routing.width = Some(0);
        let Err(CoreError::Integrity(problems)) = from_json(&to_json(&p)) else {
            panic!("integrity must refuse the project");
        };
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems.iter().any(|m| m.contains("SMD pads only")));
        assert!(problems.iter().any(|m| m.contains("1 mm limit")));
        assert!(problems.iter().any(|m| m.starts_with("routing options")));
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

    /// A project with one group of a wire and a net label (schema v6, ADR-0045).
    fn v6_project() -> Project {
        use crate::{LabelKind, NetLabel, NetScope, Point, SchematicGroup, Wire};
        let mut p = Project::new("v6-groups");
        let sheet = p.first_sheet_id().unwrap().clone();
        p.schematic.wires.insert(
            "wir_1".into(),
            Wire {
                id: "wir_1".into(),
                sheet_id: sheet.clone(),
                a: Point::new(0, 0),
                b: Point::new(2_540_000, 0),
            },
        );
        p.schematic.labels.insert(
            "lbl_1".into(),
            NetLabel {
                id: "lbl_1".into(),
                sheet_id: sheet.clone(),
                name: "SDA".into(),
                position: Point::new(0, 0),
                kind: LabelKind::Net,
                scope: NetScope::Global,
                power_style: None,
            },
        );
        p.schematic.groups.insert(
            "grp_1".into(),
            SchematicGroup {
                id: "grp_1".into(),
                sheet_id: sheet,
                name: "Group 1".into(),
                members: vec!["lbl_1".into(), "wir_1".into()],
            },
        );
        p
    }

    #[test]
    fn v6_groups_round_trip_byte_stable() {
        let p = v6_project();
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
        assert_eq!(
            v["schematic"]["groups"]["grp_1"]["members"],
            serde_json::json!(["lbl_1", "wir_1"])
        );
    }

    #[test]
    fn v5_project_migrates_to_v6_with_identical_meaning() {
        let mut current = v6_project();
        current.schematic.groups.clear();
        let mut v: Value = serde_json::from_str(&to_json(&current)).unwrap();
        v["schema_version"] = Value::from(5);
        v["schematic"].as_object_mut().unwrap().remove("groups");
        let v5 = v.to_string();
        assert!(!v5.contains("\"groups\""));
        let p = from_json(&v5).expect("v5 project loads");
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        assert_eq!(p, current);
        assert!(p.schematic.groups.is_empty());
        let a = to_json(&p);
        assert!(a.contains("\"groups\""), "v6 always writes the collection");
        assert_eq!(a, to_json(&from_json(&a).unwrap()));
    }

    #[test]
    fn invalid_groups_fail_integrity() {
        use crate::SchematicGroup;
        let mut p = v6_project();
        let sheet = p.first_sheet_id().unwrap().clone();
        // A member that does not exist, and a one-member group.
        p.schematic
            .groups
            .get_mut("grp_1")
            .unwrap()
            .members
            .push("sym_gone".into());
        p.schematic.groups.insert(
            "grp_2".into(),
            SchematicGroup {
                id: "grp_2".into(),
                sheet_id: sheet,
                name: "Lonely".into(),
                members: vec!["wir_1".into()],
            },
        );
        let Err(CoreError::Integrity(problems)) = from_json(&to_json(&p)) else {
            panic!("integrity must refuse the project");
        };
        // missing member; grp_2 too small; wir_1 in two groups.
        assert_eq!(problems.len(), 3, "{problems:?}");
    }

    #[test]
    fn prune_groups_drops_deleted_members_and_dissolves_small_groups() {
        let mut p = v6_project();
        p.schematic.labels.remove("lbl_1");
        assert_eq!(p.schematic.prune_groups(), vec!["Group 1".to_string()]);
        assert!(p.schematic.groups.is_empty());
    }

    /// v6 → v7 (ADR-0046): a v6 file loads with no rule configuration, a project without one
    /// saves without the new keys, and a configured one round-trips through save and open.
    #[test]
    fn v7_rule_configuration_migrates_and_round_trips() {
        use crate::{ClearanceCell, NetPairRule, ObjectClass, RuleConfig, RuleSeverity};
        let plain = Project::new("v5-board");
        let json = to_json(&plain);
        for k in ["\"rules\"", "clearance_matrix", "net_pair_rules"] {
            assert!(!json.contains(k), "empty {k} must not be written");
        }
        let mut v: Value = serde_json::from_str(&json).unwrap();
        v["schema_version"] = Value::from(6);
        let p = from_json(&v.to_string()).expect("v6 project loads");
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        assert_eq!(p, plain);

        let mut q = Project::new("configured");
        let c = &mut q.constraints;
        c.rules.insert(
            "ERC-001".into(),
            RuleConfig {
                enabled: false,
                severity: None,
            },
        );
        c.rules.insert(
            "DRC-002".into(),
            RuleConfig {
                enabled: true,
                severity: Some(RuleSeverity::Warning),
            },
        );
        c.clearance_matrix.push(ClearanceCell {
            a: ObjectClass::Track,
            b: ObjectClass::Via,
            clearance: 250_000,
        });
        c.net_pair_rules.push(NetPairRule {
            net_a: "GND".into(),
            net_b: "VIN".into(),
            clearance: 500_000,
        });
        let a = to_json(&q);
        let back = from_json(&a).expect("configured project loads");
        assert_eq!(back, q);
        assert_eq!(a, to_json(&back));
        assert_eq!(
            back.constraints
                .matrix_cell(ObjectClass::Via, ObjectClass::Track),
            Some(250_000)
        );
        assert_eq!(
            back.constraints.net_pair_clearance("VIN", "GND"),
            Some(500_000)
        );

        let mut bad = q.clone();
        bad.constraints.net_pair_rules[0].net_b = "GND".into();
        bad.constraints.clearance_matrix[0].clearance = 0;
        let Err(CoreError::Integrity(problems)) = from_json(&to_json(&bad)) else {
            panic!("integrity must refuse the rule data");
        };
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    /// v7 → v8 (ADR-0055): a v7 file loads with nothing declared, a project without v8 data
    /// saves without the new keys, a declared set round-trips, and bad values fail integrity.
    #[test]
    fn v8_engineering_inputs_migrate_and_round_trip() {
        use crate::{DiffPairRule, Enclosure, ViaSpanRule};
        let plain = Project::new("v7-board");
        let json = to_json(&plain);
        for k in [
            "diff_pairs",
            "via_spans",
            "target_z0_ohm",
            "ambient_temp_c",
            "via_plating_um",
            "net_max_drop_v",
            "part_dissipation_w",
            "insulation",
            "enclosure",
        ] {
            assert!(!json.contains(k), "undeclared {k} must not be written");
        }
        let mut v: Value = serde_json::from_str(&json).unwrap();
        v["schema_version"] = Value::from(7);
        let p = from_json(&v.to_string()).expect("v7 project loads");
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        assert_eq!(p, plain);

        let mut q = Project::new("declared");
        let c = &mut q.constraints;
        c.default_class.target_z0_ohm = Some(50.0);
        c.diff_pairs.push(DiffPairRule {
            positive: "USB_DP".into(),
            negative: "USB_DN".into(),
            width: None,
            gap: Some(150_000),
            target_zdiff_ohm: Some(90.0),
            zdiff_tolerance_pct: None,
            max_skew: Some(100_000),
        });
        c.via_spans.push(ViaSpanRule {
            from: CopperLayer::Top,
            to: CopperLayer::Bottom,
            drill: 300_000,
            diameter: 600_000,
        });
        c.via_plating_um = Some(25.0);
        c.part_dissipation_w.insert("U1".into(), 0.25);
        c.enclosure = Some(Enclosure::default());
        let a = to_json(&q);
        let back = from_json(&a).expect("declared project loads");
        assert_eq!(back, q);
        assert_eq!(a, to_json(&back));

        let mut bad = q.clone();
        bad.constraints.diff_pairs[0].negative = "USB_DP".into();
        bad.constraints.default_class.target_z0_ohm = Some(-1.0);
        let Err(CoreError::Integrity(problems)) = from_json(&to_json(&bad)) else {
            panic!("integrity must refuse the v8 data");
        };
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    /// v8 → v9 (ADR-0057): a v8 file loads with an empty requirement record (every field
    /// UNKNOWN), a project without one saves without the key, a recorded set round-trips, and
    /// entries that break the status rules fail integrity.
    #[test]
    fn v9_requirement_record_migrates_and_round_trips() {
        use crate::requirements::{evaluate, ReqItem, SourceKind};
        use crate::{ReqEntry, ReqItemKind, ReqSource, ReqStatus, ReqUnit, ReqValue};
        let plain = Project::new("v8-board");
        let json = to_json(&plain);
        assert!(
            !json.contains("requirement_set"),
            "an empty record is not written"
        );
        let mut v: Value = serde_json::from_str(&json).unwrap();
        v["schema_version"] = Value::from(8);
        let p = from_json(&v.to_string()).expect("v8 project loads");
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        assert_eq!(p, plain);
        let r = evaluate(&p.requirement_set);
        assert_eq!(r.completeness.unknown, r.completeness.total);

        let confirmed = |value: ReqValue| ReqEntry {
            status: ReqStatus::Confirmed,
            value: Some(value),
            source: Some(ReqSource {
                kind: SourceKind::Client,
                reference: "requirements review 2026-11-25".into(),
            }),
            derived_from: vec![],
            owner: Some("Client PM".into()),
            risk_if_wrong: None,
            note: None,
            updated_at_ms: 1_795_000_000_000,
            updated_revision: 4,
            conflicts: vec![],
        };
        let mut q = Project::new("recorded");
        let set = &mut q.requirement_set;
        set.fields.insert(
            "supply.input_voltage".into(),
            confirmed(ReqValue::Range {
                min: Some(4.5),
                nom: Some(5.0),
                max: Some(5.5),
                unit: ReqUnit::Volt,
            }),
        );
        let mut rail = ReqItem {
            id: "rail_1".into(),
            kind: ReqItemKind::Rail,
            name: "3V3".into(),
            parent: None,
            fields: Default::default(),
        };
        rail.fields.insert(
            "current_peak".into(),
            confirmed(ReqValue::Number {
                value: 300.0,
                unit: ReqUnit::Milliampere,
                tolerance_pct: Some(10.0),
            }),
        );
        set.items.insert("rail_1".into(), rail);
        let a = to_json(&q);
        assert!(a.contains("\"CONFIRMED\"") && a.contains("\"mA\""));
        let back = from_json(&a).expect("recorded project loads");
        assert_eq!(back, q);
        assert_eq!(a, to_json(&back));

        let mut bad = q.clone();
        let e = bad
            .requirement_set
            .fields
            .get_mut("supply.input_voltage")
            .unwrap();
        e.status = ReqStatus::Unknown; // UNKNOWN with a value
        bad.requirement_set.items.get_mut("rail_1").unwrap().parent = Some("rail_gone".into());
        let Err(CoreError::Integrity(problems)) = from_json(&to_json(&bad)) else {
            panic!("integrity must refuse the requirement record");
        };
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    /// v9 → v11 (ADR-0060): a v9 file loads with no conflicts and no classification, an entry's
    /// conflicting values and a `domain.*` entry round-trip, and a malformed conflict fails
    /// integrity.
    #[test]
    fn v11_conflicts_and_classification_migrate_and_round_trip() {
        use crate::requirements::SourceKind;
        use crate::{ReqEntry, ReqSource, ReqStatus, ReqUnit, ReqValue};
        let entry = |status: ReqStatus, value: ReqValue, kind: SourceKind| ReqEntry {
            status,
            value: Some(value),
            source: Some(ReqSource {
                kind,
                reference: "kick-off call 2026-11-20".into(),
            }),
            derived_from: vec![],
            owner: (kind != SourceKind::Calculation).then(|| "Client PM".to_string()),
            risk_if_wrong: (status == ReqStatus::NeedsConfirmation)
                .then(|| "the input stage is re-selected".to_string()),
            note: None,
            updated_at_ms: 1_795_000_000_000,
            updated_revision: 3,
            conflicts: vec![],
        };
        let volts = |nom: f64| ReqValue::Range {
            min: None,
            nom: Some(nom),
            max: None,
            unit: ReqUnit::Volt,
        };
        let mut q = Project::new("v11-board");
        let mut disputed = entry(ReqStatus::Confirmed, volts(12.0), SourceKind::Client);
        disputed.conflicts.push(entry(
            ReqStatus::NeedsConfirmation,
            volts(24.0),
            SourceKind::Datasheet,
        ));
        let set = &mut q.requirement_set;
        set.fields.insert("supply.input_voltage".into(), disputed);
        set.fields.insert(
            "domain.rf_wireless".into(),
            entry(
                ReqStatus::NeedsConfirmation,
                ReqValue::Flag { value: true },
                SourceKind::Calculation,
            ),
        );
        let json = to_json(&q);
        assert!(json.contains("\"conflicts\"") && json.contains("\"domain.rf_wireless\""));
        let back = from_json(&json).expect("a v11 project loads");
        assert_eq!(back, q);
        assert_eq!(json, to_json(&back));

        // A v9 file (no conflicts, no classification) loads unchanged at v11.
        let plain = Project::new("v9-board");
        let mut v: Value = serde_json::from_str(&to_json(&plain)).unwrap();
        v["schema_version"] = Value::from(9);
        let p = from_json(&v.to_string()).expect("v9 project loads");
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        assert_eq!(p, plain);

        // A conflicting value equal to the recorded one is refused on load.
        let mut bad = q.clone();
        let e = bad
            .requirement_set
            .fields
            .get_mut("supply.input_voltage")
            .unwrap();
        e.conflicts[0].value = e.value.clone();
        let Err(CoreError::Integrity(problems)) = from_json(&to_json(&bad)) else {
            panic!("integrity must refuse the conflict");
        };
        assert_eq!(problems.len(), 1, "{problems:?}");
    }

    #[test]
    fn corrupt_file_fails_safely() {
        assert!(matches!(from_json("{not json"), Err(CoreError::Parse(_))));
        assert!(matches!(from_json("{}"), Err(CoreError::Parse(_))));
    }

    /// v9 → v10 (ADR-0059): a v9 file loads with both reference-plane limits undeclared, a
    /// project without them saves without the keys, declared limits round-trip, and a limit that
    /// is not positive fails integrity.
    #[test]
    fn v10_reference_plane_limits_migrate_and_round_trip() {
        let plain = Project::new("v9-board");
        let json = to_json(&plain);
        for k in ["return_via_max_distance", "plane_min_neck_width"] {
            assert!(!json.contains(k), "an undeclared {k} is not written");
        }
        let mut v: Value = serde_json::from_str(&json).unwrap();
        v["schema_version"] = Value::from(9);
        let p = from_json(&v.to_string()).expect("v9 project loads");
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        assert_eq!(p.constraints.return_via_max_distance, None);
        assert_eq!(p.constraints.plane_min_neck_width, None);
        assert_eq!(p, plain);

        let mut q = Project::new("declared");
        q.constraints.return_via_max_distance = Some(1_500_000);
        q.constraints.plane_min_neck_width = Some(400_000);
        let back = from_json(&to_json(&q)).expect("declared limits load");
        assert_eq!(back, q);

        let mut bad = q.clone();
        bad.constraints.return_via_max_distance = Some(0);
        bad.constraints.plane_min_neck_width = Some(-1);
        let Err(CoreError::Integrity(problems)) = from_json(&to_json(&bad)) else {
            panic!("integrity must refuse a limit that is not positive");
        };
        assert_eq!(problems.len(), 2, "{problems:?}");
    }
}
