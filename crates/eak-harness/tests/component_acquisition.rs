use eak_core::{
    ComponentDef, FootprintDef, Model3d, PadDef, PadKind, PadShape, PinType, Point, Project,
    Rotation, SymbolDef, SymbolPin,
};
use eak_harness::{handle, CallContext};
use eak_runtime::{Actor, Command, CommandRequest, Engine};
use serde_json::json;
use std::collections::BTreeMap;

#[test]
fn validate_component_definition_and_stage_to_schematic_and_pcb() {
    let mut engine = Engine::in_memory(Project::new("AcquisitionTest"));
    let ctx = CallContext::from_env();

    // 1. Build a candidate component definition (e.g. TPS5430 buck regulator)
    let pins = vec![
        SymbolPin {
            length: 0,
            orientation: Default::default(),
            name_visible: true,
            number_visible: true,
            number: "1".into(),
            name: "BOOT".into(),
            electrical_type: PinType::PowerIn,
            position: Point::new(-5_000_000, 2_540_000),
        },
        SymbolPin {
            length: 0,
            orientation: Default::default(),
            name_visible: true,
            number_visible: true,
            number: "2".into(),
            name: "VIN".into(),
            electrical_type: PinType::PowerIn,
            position: Point::new(-5_000_000, 0),
        },
        SymbolPin {
            length: 0,
            orientation: Default::default(),
            name_visible: true,
            number_visible: true,
            number: "3".into(),
            name: "PH".into(),
            electrical_type: PinType::Output,
            position: Point::new(5_000_000, 2_540_000),
        },
        SymbolPin {
            length: 0,
            orientation: Default::default(),
            name_visible: true,
            number_visible: true,
            number: "4".into(),
            name: "GND".into(),
            electrical_type: PinType::PowerIn,
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
            graphics: Vec::new(),
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
        model3d: Model3d {
            kind: "box".into(),
            color: "#333333".into(),
            external_ref: None,
            model_ref: None,
        },
        provenance: "Acquired by EAK agent from TI TPS5430 datasheet; unverified by engineer"
            .into(),
    };

    // 2. Test `validate_component_definition` tool
    let val_res = handle(
        &mut engine,
        &ctx,
        "direct",
        "validate_component_definition",
        &json!({"definition": serde_json::to_value(&def).unwrap()}),
    );

    assert_eq!(val_res["ok"], true, "{val_res}");
    let res = &val_res["result"];
    assert_eq!(res["valid"], true, "{res}");
    assert_eq!(res["pinmap_blocking"], false);
    assert_eq!(res["requires_review"], true);

    // 3. Stage component into the project library via UpsertComponentDefinition
    let r = engine.execute(CommandRequest {
        expected_revision: 0,
        actor: Actor::human(),
        command: Command::UpsertComponentDefinition {
            definition: Box::new(def.clone()),
        },
        approval: None,
    });
    assert!(
        r.accepted,
        "UpsertComponentDefinition failed: {:?}",
        r.error
    );
    assert_eq!(r.revision_after, 1);
    assert!(engine.project().library.contains_key("TPS5430DDA"));

    // 4. Place symbol on schematic
    let r2 = engine.execute(CommandRequest {
        expected_revision: 1,
        actor: Actor::human(),
        command: Command::PlaceSymbol {
            sheet_id: None,
            component_id: "TPS5430DDA".into(),
            position: Point::new(10_000_000, 10_000_000),
            rotation: Rotation::R90,
            refdes: None,
            value: None,
        },
        approval: None,
    });
    assert!(r2.accepted, "PlaceSymbol failed: {:?}", r2.error);
    assert_eq!(r2.revision_after, 2);

    let sym_id = {
        let sym_inst = engine
            .project()
            .schematic
            .symbols
            .values()
            .find(|s| s.component_id == "TPS5430DDA")
            .unwrap();
        assert_eq!(sym_inst.refdes, "U1");
        assert_eq!(sym_inst.rotation, Rotation::R90);
        sym_inst.id.clone()
    };

    // 5. Synchronize to PCB and verify footprint placement
    let r3 = engine.execute(CommandRequest {
        expected_revision: 2,
        actor: Actor::human(),
        command: Command::SyncSchematicToPcb,
        approval: None,
    });
    assert!(r3.accepted, "SyncSchematicToPcb failed: {:?}", r3.error);
    assert_eq!(r3.revision_after, 3);

    let fp_inst = engine
        .project()
        .pcb
        .footprints
        .values()
        .find(|f| f.component_id == "TPS5430DDA")
        .unwrap();
    assert_eq!(fp_inst.refdes, "U1");
    assert_eq!(fp_inst.symbol_id, sym_id);
}
