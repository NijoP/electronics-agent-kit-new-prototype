//! Object groups and net-label rename (ADR-0045): `Group`, `Ungroup` and `RenameNetLabel`. Each
//! is one revision and one undo step; deleting a grouped object drops it from its group and
//! dissolves a group left with one object; a rename keeps the label's id and carries the net
//! name onto the PCB and the constraints.

use eak_core::{LabelKind, NetScope, Point, Project};
use eak_engineering::netlist;
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;
use eak_runtime::CommandError;

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
    let r = run(e, cmd);
    assert!(r.accepted, "{:?}", r.error);
    r
}

fn undo(e: &mut Engine) {
    let r = e.undo(e.project().revision, &Actor::human());
    assert!(r.accepted, "{:?}", r.error);
}

fn redo(e: &mut Engine) {
    let r = e.redo(e.project().revision, &Actor::human());
    assert!(r.accepted, "{:?}", r.error);
}

fn place(e: &mut Engine, sheet: &str, x: f64) -> String {
    ok(
        e,
        C::PlaceSymbol {
            sheet_id: Some(sheet.into()),
            component_id: "R_0603".into(),
            position: Point::mm(x, 0.0),
            rotation: Default::default(),
            refdes: None,
            value: None,
        },
    )
    .created[0]
        .clone()
}

fn wire(e: &mut Engine, sheet: &str, y: f64) -> String {
    ok(
        e,
        C::AddWire {
            sheet_id: Some(sheet.into()),
            a: Point::mm(0.0, y),
            b: Point::mm(10.0, y),
        },
    )
    .created[0]
        .clone()
}

fn label_pin(e: &mut Engine, sym: &str, pin: &str, name: &str) -> String {
    ok(
        e,
        C::LabelPin {
            symbol_id: sym.into(),
            pin: pin.into(),
            name: name.into(),
            kind: LabelKind::Net,
            scope: NetScope::Global,
            power_style: None,
        },
    )
    .created[0]
        .clone()
}

/// One sheet with a resistor, a wire and a label on the resistor's pin 1.
fn design() -> (Engine, String, String, String, String) {
    let mut e = Engine::in_memory(Project::new("groups"));
    let s1 = e.project().first_sheet_id().unwrap().clone();
    let r1 = place(&mut e, &s1, 0.0);
    let w = wire(&mut e, &s1, 40.0);
    let l = label_pin(&mut e, &r1, "1", "VIN");
    (e, s1, r1, w, l)
}

fn group(e: &mut Engine, members: &[&String]) -> String {
    ok(
        e,
        C::Group {
            members: members.iter().map(|m| m.to_string()).collect(),
            name: None,
        },
    )
    .created[0]
        .clone()
}

#[test]
fn group_and_ungroup_are_one_undo_step_each() {
    let (mut e, s1, r1, w, l) = design();
    let before = e.project().clone();
    let g = group(&mut e, &[&w, &r1, &l, &r1]);
    let p = e.project();
    assert_eq!(p.revision, before.revision + 1, "one revision");
    let grp = &p.schematic.groups[&g];
    assert_eq!(grp.name, "Group 1");
    assert_eq!(grp.sheet_id, s1);
    let mut sorted = vec![r1.clone(), w.clone(), l.clone()];
    sorted.sort();
    assert_eq!(grp.members, sorted, "members are sorted and unique");
    assert_eq!(p.schematic.group_of(&w).map(|x| &x.id), Some(&g));
    assert!(p.schematic.symbols == before.schematic.symbols);

    undo(&mut e);
    assert_eq!(e.project().schematic, before.schematic);
    redo(&mut e);
    assert!(e.project().schematic.groups.contains_key(&g));

    let grouped = e.project().clone();
    ok(
        &mut e,
        C::Ungroup {
            group_id: g.to_string(),
        },
    );
    assert!(e.project().schematic.groups.is_empty());
    assert_eq!(e.project().schematic.wires, grouped.schematic.wires);
    undo(&mut e);
    assert_eq!(e.project().schematic, grouped.schematic);

    // An explicit name is kept (trimmed).
    let r2 = place(&mut e, &s1, 30.0);
    let w2 = wire(&mut e, &s1, 60.0);
    let r = ok(
        &mut e,
        C::Group {
            members: vec![r2, w2],
            name: Some("  Filter  ".into()),
        },
    );
    assert_eq!(e.project().schematic.groups[&r.created[0]].name, "Filter");
}

#[test]
fn group_refuses_bad_member_sets() {
    let (mut e, s1, r1, w, l) = design();
    let s2 = ok(
        &mut e,
        C::AddSheet {
            name: "Sheet 2".into(),
        },
    )
    .created[0]
        .clone();
    let elsewhere = wire(&mut e, &s2, 0.0);
    let cases: Vec<(Vec<String>, &str)> = vec![
        (vec![r1.clone()], "invalid"),
        (vec![r1.clone(), r1.clone()], "invalid"),
        (vec![r1.clone(), "sym_missing".into()], "not_found"),
        (vec![r1.clone(), s1.clone()], "not_found"),
        (vec![r1.clone(), elsewhere.clone()], "invalid"),
    ];
    for (members, want) in cases {
        let r = run(
            &mut e,
            C::Group {
                members: members.clone(),
                name: None,
            },
        );
        let got = match r.error {
            Some(CommandError::Invalid(_)) => "invalid",
            Some(CommandError::NotFound(_)) => "not_found",
            ref err => panic!("{members:?}: {err:?}"),
        };
        assert_eq!(got, want, "{members:?}");
    }
    group(&mut e, &[&r1, &w]);
    let r = run(
        &mut e,
        C::Group {
            members: vec![w.clone(), l.clone()],
            name: None,
        },
    );
    assert!(
        matches!(r.error, Some(CommandError::Precondition(_))),
        "an object is in one group at most: {:?}",
        r.error
    );
    let r = run(
        &mut e,
        C::Ungroup {
            group_id: "grp_missing".into(),
        },
    );
    assert!(matches!(r.error, Some(CommandError::NotFound(_))));
}

#[test]
fn deleting_members_shrinks_then_dissolves_the_group_in_the_same_step() {
    let (mut e, _, r1, w, l) = design();
    let g = group(&mut e, &[&r1, &w, &l]);
    ok(
        &mut e,
        C::DeleteNetLabel {
            label_id: l.to_string(),
        },
    );
    assert_eq!(e.project().schematic.groups[&g].members.len(), 2);
    let two = e.project().clone();
    let r = ok(
        &mut e,
        C::DeleteWire {
            wire_id: w.to_string(),
        },
    );
    assert!(e.project().schematic.groups.is_empty());
    assert!(
        r.warnings.iter().any(|x| x.contains("Group 1")),
        "{:?}",
        r.warnings
    );
    // One undo brings back the wire and the group together.
    undo(&mut e);
    assert_eq!(e.project().schematic, two.schematic);
}

#[test]
fn moving_a_group_is_one_batch_of_member_moves() {
    let (mut e, _, r1, w, _) = design();
    group(&mut e, &[&r1, &w]);
    let p = e.project().clone();
    let (s, wi) = (&p.schematic.symbols[&r1], &p.schematic.wires[&w]);
    let by = Point::mm(5.0, 5.0);
    ok(
        &mut e,
        C::Batch {
            commands: vec![
                C::MoveSymbol {
                    symbol_id: r1.clone(),
                    position: s.position.plus(by),
                },
                C::MoveWire {
                    wire_id: w.clone(),
                    a: wi.a.plus(by),
                    b: wi.b.plus(by),
                },
            ],
            label: Some("Move Group 1".into()),
        },
    );
    assert_eq!(e.project().schematic.groups, p.schematic.groups);
    undo(&mut e);
    assert_eq!(e.project().schematic, p.schematic);
}

#[test]
fn rename_net_label_keeps_its_id_and_carries_the_net_to_the_pcb() {
    let (mut e, _, r1, _, l) = design();
    ok(&mut e, C::SyncSchematicToPcb);
    ok(
        &mut e,
        C::SetNetCurrent {
            net: "VIN".into(),
            amps: Some(1.5),
        },
    );
    let before = e.project().clone();
    let pad = |e: &Engine| {
        e.project()
            .pcb
            .footprints
            .values()
            .find(|f| f.symbol_id == r1)
            .and_then(|f| f.pad_nets.get("1").cloned())
    };
    assert_eq!(pad(&e).as_deref(), Some("VIN"));
    let r = ok(
        &mut e,
        C::RenameNetLabel {
            label_id: l.clone(),
            name: "  VBAT ".into(),
        },
    );
    let p = e.project();
    assert_eq!(p.revision, before.revision + 1, "one revision");
    let lbl = &p.schematic.labels[&l];
    let old = &before.schematic.labels[&l];
    assert_eq!(lbl.name, "VBAT");
    assert_eq!(
        (lbl.position, lbl.kind, lbl.scope, lbl.power_style),
        (old.position, old.kind, old.scope, old.power_style),
        "only the name changes"
    );
    assert!(netlist::build(p).nets.contains_key("VBAT"));
    assert_eq!(pad(&e).as_deref(), Some("VBAT"));
    let p = e.project();
    assert_eq!(p.constraints.net_currents_a.get("VBAT"), Some(&1.5));
    assert!(!p.constraints.net_currents_a.contains_key("VIN"));
    assert!(
        r.warnings.iter().any(|w| w.contains("VIN → VBAT")),
        "{:?}",
        r.warnings
    );

    undo(&mut e);
    let p = e.project();
    assert_eq!(p.schematic, before.schematic);
    assert_eq!(p.pcb, before.pcb);
    assert_eq!(p.constraints, before.constraints);
}

#[test]
fn rename_into_an_existing_net_name_warns_that_the_nets_join() {
    let (mut e, s1, _, _, l) = design();
    let r2 = place(&mut e, &s1, 30.0);
    label_pin(&mut e, &r2, "1", "GND_SENSE");
    let r = ok(
        &mut e,
        C::RenameNetLabel {
            label_id: l,
            name: "GND_SENSE".into(),
        },
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("joins")),
        "{:?}",
        r.warnings
    );
}

#[test]
fn rename_refuses_blank_unchanged_and_unknown() {
    let (mut e, _, _, _, l) = design();
    let r = run(
        &mut e,
        C::RenameNetLabel {
            label_id: l.clone(),
            name: "   ".into(),
        },
    );
    assert!(matches!(r.error, Some(CommandError::Invalid(_))));
    let r = run(
        &mut e,
        C::RenameNetLabel {
            label_id: l,
            name: "VIN".into(),
        },
    );
    assert!(matches!(r.error, Some(CommandError::NothingTo(_))));
    let r = run(
        &mut e,
        C::RenameNetLabel {
            label_id: "lbl_missing".into(),
            name: "X".into(),
        },
    );
    assert!(matches!(r.error, Some(CommandError::NotFound(_))));
}
