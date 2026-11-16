use eak_core::{
    ComponentDef, FootprintDef, PadDef, PadKind, PadShape, PinDef, PinType, Point, SymbolDef,
};
use eak_harness::{CallContext, EngineHarness};
use eak_runtime::Command;
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn validate_component_definition_and_stage_to_schematic_and_pcb() {
    let td = std::env::temp_dir().join(format!("acq_test_{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&td).unwrap();
    let dir = td.to_str().unwrap();

    let harness = EngineHarness::new();
    let ctx = CallContext {
        session_id: "s1".into(),
        agent: "agent:lead".into(),
        max_class: eak_runtime::ImpactClass::Destructive,
    };

    // 1. Create a project
    harness
        .call(
            "create_project",
            &json!({"dir": dir, "name": "AcquisitionTest"}),
            &ctx,
        )
        .unwrap();

    // 2. Build a candidate component definition (e.g. TPS5430 buck regulator)
    let pins = vec![
        PinDef {
            number: "1".into(),
            name: "BOOT".into(),
            electrical_type: PinType::Power,
            position: Point::new(-5_000_000, 2_540_000),
        },
        PinDef {
            number: "2".into(),
            name: "VIN".into(),
            electrical_type: PinType::Power,
            position: Point::new(-5_000_000, 0),
        },
        PinDef {
            number: "3".into(),
            name: "PH".into(),
            electrical_type: PinType::Output,
            position: Point::new(5_000_000, 2_540_000),
        },
        PinDef {
            number: "4".into(),
            name: "GND".into(),
            electrical_type: PinType::Power,
            position: Point::new(5_000_000, 0),
        },
    ];

    let pads = vec![
        PadDef {
            number: "1".into(),
            kind: PadKind::Smd,
            shape: PadShape::Rect,
            position: Point::new(-2_500_000, 1_270_000),
            size_x: 1_500_000,
            size_y: 600_000,
            drill: None,
        },
        PadDef {
            number: "2".into(),
            kind: PadKind::Smd,
            shape: PadShape::Rect,
            position: Point::new(-2_500_000, -1_270_000),
            size_x: 1_500_000,
            size_y: 600_000,
            drill: None,
        },
        PadDef {
            number: "3".into(),
            kind: PadKind::Smd,
            shape: PadShape::Rect,
            position: Point::new(2_500_000, 1_270_000),
            size_x: 1_500_000,
            size_y: 600_000,
            drill: None,
        },
        PadDef {
            number: "4".into(),
            kind: PadKind::Smd,
            shape: PadShape::Rect,
            position: Point::new(2_500_000, -1_270_000),
            size_x: 1_500_000,
            size_y: 600_000,
            drill: None,
        },
    ];

    let mut pin_map = BTreeMap::new();
    pin_map.insert("1".into(), vec!["1".into()]);
    pin_map.insert("2".into(), vec!["2".into()]);
    pin_map.insert("3".into(), vec!["3".into()]);
    pin_map.insert("4".into(), vec!["4".into()]);

    let def = ComponentDef {
        id: "TPS5430DDA".into(),
        name: "TPS5430 Step-Down Converter".into(),
        category: "Power".into(),
        refdes_prefix: "U".into(),
        default_value: "TPS5430".into(),
        manufacturer: Some("Texas Instruments".into()),
        mpn: Some("TPS5430DDA".into()),
        distributor: None,
        supplier_offers: vec![],
        datasheet: Some("https://www.ti.com/lit/ds/symlink/tps5430.pdf".into()),
        symbol: SymbolDef {
            pins,
            body_half_w: 5_000_000,
            body_half_h: 3_810_000,
        },
        footprint: FootprintDef {
            name: "SOIC-8-EP".into(),
            pads,
            courtyard_half_w: 3_500_000,
            courtyard_half_h: 3_000_000,
            body_height: None,
        },
        pin_map,
        ratings: vec![],
        model3d: eak_core::Model3d::default(),
        provenance: "Acquired by EAK agent from TI TPS5430 datasheet; unverified by engineer"
            .into(),
    };

    // 3. Test `validate_component_definition` tool
    let val_res = harness
        .call(
            "validate_component_definition",
            &json!({"definition": serde_json::to_value(&def).unwrap()}),
            &ctx,
        )
        .unwrap();

    assert_eq!(val_res["valid"], true, "{val_res}");
    assert_eq!(val_res["pinmap_blocking"], false);
    assert_eq!(val_res["requires_review"], true);

    // 4. Stage component into the project library via UpsertComponentDefinition
    let engine = harness.engine().unwrap();
    let r = engine
        .execute(
            0,
            Command::UpsertComponentDefinition {
                definition: Box::new(def.clone()),
            },
        )
        .unwrap();
    assert_eq!(r.revision, 1);
    assert!(engine.project().library.contains_key("TPS5430DDA"));

    // 5. Place symbol on schematic
    let r2 = engine
        .execute(
            1,
            Command::PlaceSymbol {
                sheet_id: None,
                component_id: "TPS5430DDA".into(),
                position: Point::new(10_000_000, 10_000_000),
                rotation: eak_core::Rotation::R90,
                refdes: None,
                value: None,
            },
        )
        .unwrap();
    assert_eq!(r2.revision, 2);

    let sym_inst = engine
        .project()
        .schematic
        .symbols
        .values()
        .find(|s| s.component_id == "TPS5430DDA")
        .unwrap();
    assert_eq!(sym_inst.refdes, "U1");
    assert_eq!(sym_inst.rotation, eak_core::Rotation::R90);

    // 6. Synchronize to PCB and verify footprint placement
    let r3 = engine.execute(2, Command::SyncSchematicToPcb).unwrap();
    assert_eq!(r3.revision, 3);

    let fp_inst = engine
        .project()
        .pcb
        .footprints
        .values()
        .find(|f| f.component_id == "TPS5430DDA")
        .unwrap();
    assert_eq!(fp_inst.refdes, "U1");
    assert_eq!(fp_inst.symbol_id, sym_inst.id);

    let _ = std::fs::remove_dir_all(&td);
}
