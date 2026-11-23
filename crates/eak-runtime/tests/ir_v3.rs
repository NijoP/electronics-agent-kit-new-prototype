//! Schema v3 schematic primitives (ADR-0040): the v2 → v3 migration keeps the netlist, and
//! every new command works through the engine (accepted, refused with a reason, undoable).

use eak_core::*;
use eak_engineering::netlist;
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;
use eak_runtime::{fixtures, CommandError};
use serde_json::Value;

fn run(e: &mut Engine, cmd: C) -> CommandResponse {
    let rev = e.project().revision;
    e.execute(CommandRequest {
        expected_revision: rev,
        actor: Actor::human(),
        command: cmd,
        approval: None,
    })
}

fn ok(e: &mut Engine, cmd: C) -> CommandResponse {
    let name = cmd.name();
    let r = run(e, cmd);
    assert!(r.accepted, "{name}: {:?}", r.error);
    r
}

fn refused(e: &mut Engine, cmd: C) -> CommandError {
    let name = cmd.name();
    let rev = e.project().revision;
    let r = run(e, cmd);
    assert!(!r.accepted, "{name} was accepted");
    assert_eq!(
        e.project().revision,
        rev,
        "{name}: a refusal changed the project"
    );
    r.error.expect("refusal carries an error")
}

fn undo(e: &mut Engine) {
    let rev = e.project().revision;
    let r = e.undo(rev, &Actor::human());
    assert!(r.accepted, "undo: {:?}", r.error);
}

/// Rewrite a current (v3) project document as schema v2 wrote it: no symbol graphics, no pin
/// geometry, no buses, bus entries or sheet graphics.
fn as_v2(p: &Project) -> String {
    let mut v: Value = serde_json::from_str(&to_json(p)).unwrap();
    v["schema_version"] = Value::from(2);
    for def in v["library"].as_object_mut().unwrap().values_mut() {
        let sym = def["symbol"].as_object_mut().unwrap();
        sym.remove("graphics");
        for pin in sym["pins"].as_array_mut().unwrap() {
            let pin = pin.as_object_mut().unwrap();
            for k in ["length", "orientation", "name_visible", "number_visible"] {
                pin.remove(k);
            }
        }
    }
    let sch = v["schematic"].as_object_mut().unwrap();
    for k in ["graphics", "buses", "bus_entries"] {
        sch.remove(k);
    }
    let text = v.to_string();
    for v3 in ["\"graphics\"", "\"orientation\"", "\"bus_entries\""] {
        assert!(!text.contains(v3), "v2 fixture still holds {v3}");
    }
    text
}

fn reference() -> Engine {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).unwrap();
    ok(&mut e, C::SyncSchematicToPcb);
    e
}

#[test]
fn v2_project_migrates_to_v3_with_the_same_netlist_and_pad_nets() {
    let e = reference();
    let original = e.project().clone();
    assert!(!original.schematic.labels.is_empty() && !original.schematic.wires.is_empty());
    assert!(original
        .schematic
        .labels
        .values()
        .any(|l| l.kind == LabelKind::Power));
    let migrated = from_json(&as_v2(&original)).expect("v2 project loads");
    assert_eq!(migrated.schema_version, SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 3);

    let (a, b) = (netlist::build(&original), netlist::build(&migrated));
    assert_eq!(a.nets, b.nets, "same nets");
    assert_eq!(a.pin_net, b.pin_net, "same pin -> net map");
    assert_eq!(a.dangling_wire_ends, b.dangling_wire_ends);
    for (id, fp) in &original.pcb.footprints {
        assert_eq!(
            fp.pad_nets, migrated.pcb.footprints[id].pad_nets,
            "{}",
            fp.refdes
        );
    }
    // Every box symbol now carries exactly its box as a rectangle graphic.
    for def in migrated.library.values() {
        assert_eq!(
            def.symbol.graphics,
            vec![Graphic::body_box(
                def.symbol.body_half_w,
                def.symbol.body_half_h
            )],
            "{}",
            def.id
        );
    }
    // The whole project is identical, and the migrated file is byte-stable on re-save.
    assert_eq!(migrated, original);
    let saved = to_json(&migrated);
    assert_eq!(saved, to_json(&from_json(&saved).unwrap()));
    // Labels stay global (omitted on disk); v2 sheets get no drawing format.
    assert!(migrated
        .schematic
        .labels
        .values()
        .all(|l| l.scope == NetScope::Global));
    assert!(migrated
        .schematic
        .sheets
        .values()
        .all(|s| s.format.is_none()));
}

fn sheet_id(e: &Engine) -> Id {
    e.project().first_sheet_id().unwrap().clone()
}

#[test]
fn set_field_visibility_stores_only_differences_and_refuses_no_ops() {
    let mut e = reference();
    let sym = e.project().schematic.symbols.keys().next().unwrap().clone();
    let set = |field, visible| C::SetFieldVisibility {
        symbol_id: sym.clone(),
        field,
        visible,
    };
    ok(&mut e, set(SymbolField::Mpn, true));
    ok(&mut e, set(SymbolField::Value, false));
    let s = &e.project().schematic.symbols[&sym];
    assert!(s.field_visible(SymbolField::Mpn) && !s.field_visible(SymbolField::Value));
    assert!(matches!(
        refused(&mut e, set(SymbolField::Mpn, true)),
        CommandError::NothingTo(_)
    ));
    // Back to the default removes the entry.
    ok(&mut e, set(SymbolField::Value, true));
    assert_eq!(
        e.project().schematic.symbols[&sym].field_visibility.len(),
        1
    );
    undo(&mut e);
    assert!(!e.project().schematic.symbols[&sym].field_visible(SymbolField::Value));
    assert!(matches!(
        refused(
            &mut e,
            C::SetFieldVisibility {
                symbol_id: "sym_missing".into(),
                field: SymbolField::Refdes,
                visible: false
            }
        ),
        CommandError::NotFound(_)
    ));
}

#[test]
fn net_label_scope_and_power_style_commands() {
    let mut e = Engine::in_memory(Project::new("ports"));
    let sh = sheet_id(&e);
    let l = ok(
        &mut e,
        C::AddNetLabel {
            sheet_id: Some(sh),
            name: "EN".into(),
            position: Point::mm(10.0, 10.0),
            kind: LabelKind::Net,
            scope: NetScope::Local,
            power_style: None,
        },
    )
    .created[0]
        .clone();
    assert_eq!(e.project().schematic.labels[&l].scope, NetScope::Local);
    ok(
        &mut e,
        C::SetNetLabelScope {
            label_id: l.clone(),
            scope: NetScope::OffPage,
        },
    );
    assert_eq!(e.project().schematic.labels[&l].scope, NetScope::OffPage);
    assert!(matches!(
        refused(
            &mut e,
            C::SetNetLabelScope {
                label_id: l.clone(),
                scope: NetScope::OffPage
            }
        ),
        CommandError::NothingTo(_)
    ));
    // A net label has no power style.
    assert!(matches!(
        refused(
            &mut e,
            C::SetPowerPortStyle {
                label_id: l.clone(),
                style: Some(PowerStyle::Ground)
            }
        ),
        CommandError::Invalid(_)
    ));
    assert!(matches!(
        refused(
            &mut e,
            C::AddNetLabel {
                sheet_id: None,
                name: "X".into(),
                position: Point::mm(0.0, 0.0),
                kind: LabelKind::Net,
                scope: NetScope::Global,
                power_style: Some(PowerStyle::Rail),
            }
        ),
        CommandError::Invalid(_)
    ));
    let gnd = ok(
        &mut e,
        C::AddNetLabel {
            sheet_id: None,
            name: "GND".into(),
            position: Point::mm(30.0, 10.0),
            kind: LabelKind::Power,
            scope: NetScope::Global,
            power_style: Some(PowerStyle::Ground),
        },
    )
    .created[0]
        .clone();
    ok(
        &mut e,
        C::SetPowerPortStyle {
            label_id: gnd.clone(),
            style: Some(PowerStyle::Chassis),
        },
    );
    assert_eq!(
        e.project().schematic.labels[&gnd].power_style,
        Some(PowerStyle::Chassis)
    );
    let r = ok(
        &mut e,
        C::SetNetLabelScope {
            label_id: gnd.clone(),
            scope: NetScope::Local,
        },
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("power port")),
        "{:?}",
        r.warnings
    );
    undo(&mut e);
    assert_eq!(e.project().schematic.labels[&gnd].scope, NetScope::Global);
}

/// Two sheets, each with a resistor whose pin 1 carries a label `EN` of the given scope.
/// Returns the engine, the two symbols and their two labels.
fn two_sheet_en(scope_a: NetScope, scope_b: NetScope) -> (Engine, [Id; 2], [Id; 2]) {
    let mut e = Engine::in_memory(Project::new("scopes"));
    let a = sheet_id(&e);
    let b = ok(
        &mut e,
        C::AddSheet {
            name: "Sheet 2".into(),
        },
    )
    .created[0]
        .clone();
    let (mut syms, mut labels) = (vec![], vec![]);
    for (sheet, scope) in [(a, scope_a), (b, scope_b)] {
        let s = ok(
            &mut e,
            C::PlaceSymbol {
                sheet_id: Some(sheet),
                component_id: "R_0603".into(),
                position: Point::mm(50.0, 50.0),
                rotation: Rotation::R0,
                refdes: None,
                value: None,
            },
        )
        .created[0]
            .clone();
        let l = ok(
            &mut e,
            C::LabelPin {
                symbol_id: s.clone(),
                pin: "1".into(),
                name: "EN".into(),
                kind: LabelKind::Net,
                scope,
                power_style: None,
            },
        )
        .created[0]
            .clone();
        syms.push(s);
        labels.push(l);
    }
    (
        e,
        [syms[0].clone(), syms[1].clone()],
        [labels[0].clone(), labels[1].clone()],
    )
}

fn net(e: &Engine, sym: &str) -> String {
    netlist::build(e.project()).pin_net[&(sym.to_string(), "1".to_string())].clone()
}

#[test]
fn netlist_local_ports_stay_on_their_sheet_and_global_ports_join() {
    let (e, [r1, r2], _) = two_sheet_en(NetScope::Global, NetScope::Global);
    assert_eq!(net(&e, &r1), "EN");
    assert_eq!(net(&e, &r2), "EN");

    let (mut e, [r1, r2], [l1, l2]) = two_sheet_en(NetScope::Local, NetScope::Local);
    assert_ne!(net(&e, &r1), net(&e, &r2));
    assert_eq!(net(&e, &r1), "Sheet 1/EN");
    assert_eq!(net(&e, &r2), "Sheet 2/EN");
    // Making one side off-page is not enough; both must reach across.
    ok(
        &mut e,
        C::SetNetLabelScope {
            label_id: l1,
            scope: NetScope::OffPage,
        },
    );
    assert_ne!(net(&e, &r1), net(&e, &r2));
    ok(
        &mut e,
        C::SetNetLabelScope {
            label_id: l2,
            scope: NetScope::Global,
        },
    );
    assert_eq!(net(&e, &r1), "EN");
    assert_eq!(net(&e, &r2), "EN");
}

#[test]
fn bus_and_bus_entry_commands_and_member_connectivity() {
    let mut e = Engine::in_memory(Project::new("bus"));
    let sh = sheet_id(&e);
    let bus = ok(
        &mut e,
        C::AddBus {
            sheet_id: Some(sh.clone()),
            name: "D[0..1]".into(),
            points: vec![Point::mm(0.0, 50.0), Point::mm(100.0, 50.0)],
        },
    )
    .created[0]
        .clone();
    // Refusals: bad polyline, empty name, entry off the bus, zero-length entry.
    for bad in [
        C::AddBus {
            sheet_id: None,
            name: "A[0..1]".into(),
            points: vec![Point::mm(0.0, 0.0)],
        },
        C::AddBus {
            sheet_id: None,
            name: " ".into(),
            points: vec![Point::mm(0.0, 0.0), Point::mm(1.0, 0.0)],
        },
        C::AddBusEntry {
            bus_id: bus.clone(),
            bus_point: Point::mm(10.0, 40.0),
            wire_point: Point::mm(12.54, 42.54),
        },
        C::AddBusEntry {
            bus_id: bus.clone(),
            bus_point: Point::mm(10.0, 50.0),
            wire_point: Point::mm(10.0, 50.0),
        },
    ] {
        assert!(matches!(refused(&mut e, bad), CommandError::Invalid(_)));
    }
    assert!(matches!(
        refused(
            &mut e,
            C::AddBusEntry {
                bus_id: "bus_missing".into(),
                bus_point: Point::mm(10.0, 50.0),
                wire_point: Point::mm(12.54, 52.54),
            }
        ),
        CommandError::NotFound(_)
    ));
    // An illegal bus name is accepted with a warning (ERC-017 reports it).
    let odd = ok(
        &mut e,
        C::AddBus {
            sheet_id: None,
            name: "DATA".into(),
            points: vec![Point::mm(0.0, 150.0), Point::mm(50.0, 150.0)],
        },
    );
    assert!(odd.warnings.iter().any(|w| w.contains("ERC-017")));

    let entry = ok(
        &mut e,
        C::AddBusEntry {
            bus_id: bus.clone(),
            bus_point: Point::mm(10.0, 50.0),
            wire_point: Point::mm(12.54, 52.54),
        },
    )
    .created[0]
        .clone();
    assert_eq!(e.project().schematic.bus_entries[&entry].sheet_id, sh);
    // A resistor whose pin 1 is wired to the entry and labelled D0.
    let r = ok(
        &mut e,
        C::PlaceSymbol {
            sheet_id: Some(sh.clone()),
            component_id: "R_0603".into(),
            position: Point::mm(40.0, 80.0),
            rotation: Rotation::R0,
            refdes: None,
            value: None,
        },
    )
    .created[0]
        .clone();
    let pin1 = e
        .project()
        .pin_position(&e.project().schematic.symbols[&r], "1")
        .unwrap();
    let corner = Point::new(Point::mm(12.54, 0.0).x, pin1.y);
    ok(
        &mut e,
        C::AddWire {
            sheet_id: Some(sh.clone()),
            a: Point::mm(12.54, 52.54),
            b: corner,
        },
    );
    ok(
        &mut e,
        C::AddWire {
            sheet_id: Some(sh.clone()),
            a: corner,
            b: pin1,
        },
    );
    ok(
        &mut e,
        C::LabelPin {
            symbol_id: r.clone(),
            pin: "1".into(),
            name: "D0".into(),
            kind: LabelKind::Net,
            scope: NetScope::Global,
            power_style: None,
        },
    );
    let nl = netlist::build(e.project());
    assert_eq!(nl.bus_entries[&entry], Some("D0".to_string()));
    assert_eq!(
        nl.buses[&bus].members,
        Some(vec!["D0".to_string(), "D1".to_string()])
    );

    // Reshaping the bus away from the entry is reported; moving the entry back on fixes it.
    let moved = ok(
        &mut e,
        C::MoveBus {
            bus_id: bus.clone(),
            points: vec![Point::mm(0.0, 60.0), Point::mm(100.0, 60.0)],
        },
    );
    assert!(moved.warnings.iter().any(|w| w.contains("ERC-019")));
    assert!(matches!(
        refused(
            &mut e,
            C::MoveBus {
                bus_id: bus.clone(),
                points: vec![Point::mm(0.0, 60.0), Point::mm(100.0, 60.0)],
            }
        ),
        CommandError::NothingTo(_)
    ));
    ok(
        &mut e,
        C::MoveBusEntry {
            entry_id: entry.clone(),
            bus_point: Point::mm(10.0, 60.0),
            wire_point: Point::mm(12.54, 52.54),
        },
    );
    assert!(matches!(
        refused(
            &mut e,
            C::MoveBusEntry {
                entry_id: entry.clone(),
                bus_point: Point::mm(10.0, 70.0),
                wire_point: Point::mm(12.54, 52.54),
            }
        ),
        CommandError::Invalid(_)
    ));
    // Delete the entry and undo it, then delete the bus: its entries go with it, and one undo
    // restores both.
    ok(
        &mut e,
        C::DeleteBusEntry {
            entry_id: entry.clone(),
        },
    );
    undo(&mut e);
    assert!(e.project().schematic.bus_entries.contains_key(&entry));
    let del = ok(
        &mut e,
        C::DeleteBus {
            bus_id: bus.clone(),
        },
    );
    assert!(del.warnings.iter().any(|w| w.contains("bus entr")));
    assert!(e.project().schematic.bus_entries.is_empty());
    undo(&mut e);
    assert!(e.project().schematic.buses.contains_key(&bus));
    assert!(e.project().schematic.bus_entries.contains_key(&entry));
    assert!(matches!(
        refused(
            &mut e,
            C::DeleteBus {
                bus_id: "nope".into()
            }
        ),
        CommandError::NotFound(_)
    ));
}

#[test]
fn sheet_graphics_format_and_project_properties() {
    let mut e = Engine::in_memory(Project::new("sheet"));
    let sh = sheet_id(&e);
    let text = Graphic::Text {
        position: Point::mm(10.0, 10.0),
        text: "Power".into(),
        size: geom::mm(2.0),
        rotation: Rotation::R0,
    };
    let g = ok(
        &mut e,
        C::AddSheetGraphic {
            sheet_id: None,
            graphic: text.clone(),
        },
    )
    .created[0]
        .clone();
    assert_eq!(e.project().schematic.graphics[&g].sheet_id, sh);
    assert!(matches!(
        refused(
            &mut e,
            C::AddSheetGraphic {
                sheet_id: None,
                graphic: Graphic::Circle {
                    center: Point::mm(0.0, 0.0),
                    radius: 0,
                    fill: false,
                    stroke: Stroke::default(),
                },
            }
        ),
        CommandError::Invalid(_)
    ));
    ok(
        &mut e,
        C::MoveSheetGraphic {
            graphic_id: g.clone(),
            by: Point::mm(5.0, 0.0),
        },
    );
    assert_eq!(
        e.project().schematic.graphics[&g].graphic,
        text.translated(Point::mm(5.0, 0.0))
    );
    assert!(matches!(
        refused(
            &mut e,
            C::MoveSheetGraphic {
                graphic_id: g.clone(),
                by: Point::default(),
            }
        ),
        CommandError::NothingTo(_)
    ));
    ok(
        &mut e,
        C::DeleteSheetGraphic {
            graphic_id: g.clone(),
        },
    );
    undo(&mut e);
    assert!(e.project().schematic.graphics.contains_key(&g));

    // Drawing format with a title-block field bound to a project property.
    let mut format = DrawingFormat::standard();
    format.title_block.push(TitleBlockField {
        label: "Company".into(),
        value: FieldSource::Property {
            key: "company".into(),
        },
    });
    ok(
        &mut e,
        C::SetSheetFormat {
            sheet_id: sh.clone(),
            format: Some(format.clone()),
        },
    );
    ok(
        &mut e,
        C::SetProjectProperty {
            key: "company".into(),
            value: Some("ACME".into()),
        },
    );
    {
        let p = e.project();
        let sheet = &p.schematic.sheets[&sh];
        let company = &sheet
            .format
            .as_ref()
            .unwrap()
            .title_block
            .last()
            .unwrap()
            .value;
        assert_eq!(p.title_block_value(sheet, company), "ACME");
    }
    assert!(matches!(
        refused(
            &mut e,
            C::SetProjectProperty {
                key: "company".into(),
                value: Some(" ACME ".into()),
            }
        ),
        CommandError::NothingTo(_)
    ));
    let mut bad = format.clone();
    bad.zone_columns = 99;
    assert!(matches!(
        refused(
            &mut e,
            C::SetSheetFormat {
                sheet_id: sh.clone(),
                format: Some(bad),
            }
        ),
        CommandError::Invalid(_)
    ));
    // The project with every v3 object saves and loads unchanged.
    let saved = to_json(e.project());
    assert_eq!(&from_json(&saved).unwrap(), e.project());
    // Undo restores the property section exactly (absent before it was set).
    undo(&mut e);
    assert!(e.project().properties.is_empty());
    ok(
        &mut e,
        C::SetSheetFormat {
            sheet_id: sh.clone(),
            format: None,
        },
    );
    assert!(e.project().schematic.sheets[&sh].format.is_none());
    assert!(matches!(
        refused(
            &mut e,
            C::SetProjectProperty {
                key: "company".into(),
                value: None,
            }
        ),
        CommandError::NothingTo(_)
    ));
}

#[test]
fn upsert_of_a_v2_shaped_definition_gets_its_box_graphic() {
    let mut e = Engine::in_memory(Project::new("lib"));
    let mut def = builtin::get("R_0603").unwrap();
    def.id = "MY_R".into();
    def.symbol.graphics.clear();
    for p in &mut def.symbol.pins {
        p.length = 0;
    }
    ok(
        &mut e,
        C::UpsertComponentDefinition {
            definition: Box::new(def.clone()),
        },
    );
    let stored = &e.project().library["MY_R"];
    assert_eq!(
        stored.symbol.graphics,
        vec![Graphic::body_box(
            def.symbol.body_half_w,
            def.symbol.body_half_h
        )]
    );
    assert!(stored.symbol.pins.iter().all(|p| p.length > 0));
    // A definition with a broken graphic is refused.
    let mut broken = def;
    broken.id = "BROKEN".into();
    broken.symbol.graphics = vec![Graphic::Polygon {
        points: vec![Point::mm(0.0, 0.0)],
        fill: true,
        stroke: Stroke::default(),
    }];
    assert!(matches!(
        refused(
            &mut e,
            C::UpsertComponentDefinition {
                definition: Box::new(broken),
            }
        ),
        CommandError::Invalid(_)
    ));
}
