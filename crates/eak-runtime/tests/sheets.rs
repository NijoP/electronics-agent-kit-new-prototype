//! Sheet management (ADR-0043): `RenameSheet`, `DeleteSheet` and `ReorderSheets`. Each is one
//! undo step; rename and reorder leave the connectivity across sheets as it was, and a rename
//! carries the names of its sheet-local nets onto the PCB and the constraints.

use eak_core::{LabelKind, NetScope, Point, Project};
use eak_engineering::netlist;
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;
use eak_runtime::CommandError;
use std::collections::BTreeSet;

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

fn label(e: &mut Engine, sym: &str, pin: &str, name: &str, scope: NetScope) {
    ok(
        e,
        C::LabelPin {
            symbol_id: sym.into(),
            pin: pin.into(),
            name: name.into(),
            kind: LabelKind::Net,
            scope,
            power_style: None,
        },
    );
}

/// Connectivity without names: the set of pin groups.
fn groups(p: &Project) -> BTreeSet<Vec<(String, String)>> {
    netlist::build(p)
        .nets
        .values()
        .map(|n| {
            let mut v: Vec<_> = n
                .pins
                .iter()
                .map(|r| (r.symbol_id.clone(), r.pin.clone()))
                .collect();
            v.sort();
            v
        })
        .collect()
}

fn order(p: &Project) -> Vec<String> {
    p.sheets_in_order().iter().map(|s| s.id.clone()).collect()
}

/// Two sheets: R1 on the first; R2 and R3 on "Sheet 2", R2.1 on local net SIG, R2.2 and R3.1
/// on global net VBUS (also labelled from R1.1 on the first sheet). Synced to the PCB, with a
/// current declared on the local net.
fn design() -> (Engine, String, String, String) {
    let mut e = Engine::in_memory(Project::new("sheets"));
    let s1 = e.project().first_sheet_id().unwrap().clone();
    let s2 = ok(
        &mut e,
        C::AddSheet {
            name: "Sheet 2".into(),
        },
    )
    .created[0]
        .clone();
    let r1 = place(&mut e, &s1, 0.0);
    let r2 = place(&mut e, &s2, 0.0);
    let r3 = place(&mut e, &s2, 20.0);
    label(&mut e, &r1, "1", "VBUS", NetScope::Global);
    label(&mut e, &r2, "1", "SIG", NetScope::Local);
    label(&mut e, &r2, "2", "VBUS", NetScope::Global);
    label(&mut e, &r3, "1", "VBUS", NetScope::Global);
    ok(&mut e, C::SyncSchematicToPcb);
    ok(
        &mut e,
        C::SetNetCurrent {
            net: "Sheet 2/SIG".into(),
            amps: Some(0.5),
        },
    );
    (e, s1, s2, r2)
}

fn pad_net(e: &Engine, sym: &str, pad: &str) -> Option<String> {
    let fp = e
        .project()
        .pcb
        .footprints
        .values()
        .find(|f| f.symbol_id == sym)?;
    fp.pad_nets.get(pad).cloned()
}

#[test]
fn new_sheets_go_last_in_the_order() {
    let (e, s1, s2, _) = design();
    assert_eq!(order(e.project()), vec![s1.clone(), s2.clone()]);
    assert_eq!(e.project().sheet_number(&s2), Some(2));
}

#[test]
fn rename_keeps_connectivity_and_carries_local_net_names_to_the_pcb() {
    let (mut e, _, s2, r2) = design();
    let before = e.project().clone();
    let pads = |e: &Engine| pad_net(e, &r2, "1");
    assert_eq!(pads(&e).as_deref(), Some("Sheet 2/SIG"));
    let r = ok(
        &mut e,
        C::RenameSheet {
            sheet_id: s2.clone(),
            name: "  Power  ".into(),
        },
    );
    let p = e.project();
    assert_eq!(p.schematic.sheets[&s2].name, "Power");
    assert_eq!(groups(p), groups(&before), "connectivity is unchanged");
    let nl = netlist::build(p);
    assert!(nl.nets.contains_key("Power/SIG"));
    assert!(nl.nets.contains_key("VBUS"), "global nets keep their name");
    assert_eq!(pads(&e).as_deref(), Some("Power/SIG"));
    let p = e.project();
    assert_eq!(p.constraints.net_currents_a.get("Power/SIG"), Some(&0.5));
    assert!(!p.constraints.net_currents_a.contains_key("Sheet 2/SIG"));
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("Sheet 2/SIG → Power/SIG")),
        "{:?}",
        r.warnings
    );

    // One undo step restores the name and every net reference.
    undo(&mut e);
    let p = e.project();
    assert_eq!(p.schematic, before.schematic);
    assert_eq!(p.pcb, before.pcb);
    assert_eq!(p.constraints, before.constraints);
}

#[test]
fn rename_refuses_blank_duplicate_unchanged_and_unknown() {
    let (mut e, s1, s2, _) = design();
    let first = e.project().schematic.sheets[&s1].name.clone();
    for name in ["  ", first.as_str(), "Sheet 2"] {
        let r = run(
            &mut e,
            C::RenameSheet {
                sheet_id: s2.clone(),
                name: name.into(),
            },
        );
        assert!(
            matches!(r.error, Some(CommandError::Invalid(_))),
            "{name:?}: {:?}",
            r.error
        );
    }
    let r = run(
        &mut e,
        C::RenameSheet {
            sheet_id: "sht_missing".into(),
            name: "X".into(),
        },
    );
    assert!(matches!(r.error, Some(CommandError::NotFound(_))));
}

#[test]
fn reorder_changes_only_the_order_and_is_one_undo_step() {
    let (mut e, s1, s2, _) = design();
    let s3 = ok(
        &mut e,
        C::AddSheet {
            name: "Sheet 3".into(),
        },
    )
    .created[0]
        .clone();
    let nl_before = netlist::build(e.project());
    ok(
        &mut e,
        C::ReorderSheets {
            sheet_ids: vec![s3.clone(), s1.clone(), s2.clone()],
        },
    );
    let p = e.project();
    assert_eq!(order(p), vec![s3.clone(), s1.clone(), s2.clone()]);
    assert_eq!(p.first_sheet_id(), Some(&s3));
    assert_eq!(p.sheet_number(&s2), Some(3));
    assert_eq!(
        netlist::build(p),
        nl_before,
        "the netlist, names included, is unchanged"
    );
    // The order survives a save and load.
    let q = eak_core::from_json(&eak_core::to_json(p)).unwrap();
    assert_eq!(order(&q), vec![s3.clone(), s1.clone(), s2.clone()]);
    undo(&mut e);
    assert_eq!(order(e.project()), vec![s1, s2, s3]);
}

#[test]
fn reorder_refuses_lists_that_do_not_name_every_sheet_once() {
    let (mut e, s1, s2, _) = design();
    for ids in [
        vec![s2.clone()],
        vec![s2.clone(), s2.clone()],
        vec![s2.clone(), s1.clone(), "sht_x".into()],
        vec![s2.clone(), "sht_x".into()],
        vec![s1.clone(), s2.clone()],
    ] {
        let r = run(
            &mut e,
            C::ReorderSheets {
                sheet_ids: ids.clone(),
            },
        );
        assert!(
            matches!(r.error, Some(CommandError::Invalid(_))),
            "{ids:?}: {:?}",
            r.error
        );
    }
}

#[test]
fn delete_refuses_a_non_empty_or_last_sheet() {
    let (mut e, s1, s2, _) = design();
    let r = run(
        &mut e,
        C::DeleteSheet {
            sheet_id: s2.clone(),
        },
    );
    match r.error {
        Some(CommandError::Precondition(m)) => {
            assert!(
                m.contains("2 symbol(s)") && m.contains("net label(s)"),
                "{m}"
            )
        }
        other => panic!("expected a precondition refusal, got {other:?}"),
    }
    let r = run(
        &mut e,
        C::DeleteSheet {
            sheet_id: "sht_x".into(),
        },
    );
    assert!(matches!(r.error, Some(CommandError::NotFound(_))));
    let mut solo = Engine::in_memory(Project::new("one"));
    let only = solo.project().first_sheet_id().unwrap().clone();
    let r = run(&mut solo, C::DeleteSheet { sheet_id: only });
    assert!(matches!(r.error, Some(CommandError::Precondition(_))));
    assert!(e.project().schematic.sheets.contains_key(&s1));
}

#[test]
fn delete_with_contents_is_one_batch_and_one_undo_step() {
    let (mut e, s1, s2, _) = design();
    let before = e.project().clone();
    let sch = &before.schematic;
    let mut steps: Vec<C> = vec![];
    steps.extend(
        sch.labels
            .values()
            .filter(|l| l.sheet_id == s2)
            .map(|l| C::DeleteNetLabel {
                label_id: l.id.clone(),
            }),
    );
    steps.extend(
        sch.symbols
            .values()
            .filter(|s| s.sheet_id == s2)
            .map(|s| C::DeleteSymbol {
                symbol_id: s.id.clone(),
            }),
    );
    steps.push(C::DeleteSheet {
        sheet_id: s2.clone(),
    });
    ok(
        &mut e,
        C::Batch {
            commands: steps,
            label: Some("Delete sheet Sheet 2".into()),
        },
    );
    let p = e.project();
    assert_eq!(order(p), vec![s1.clone()]);
    assert!(p.schematic.symbols.values().all(|s| s.sheet_id == s1));
    assert!(p.schematic.labels.values().all(|l| l.sheet_id == s1));
    assert_eq!(
        p.pcb.footprints.len(),
        1,
        "footprints of deleted symbols go too"
    );
    undo(&mut e);
    let p = e.project();
    assert_eq!(p.schematic, before.schematic);
    assert_eq!(p.pcb, before.pcb);
}

#[test]
fn deleting_an_empty_sheet_and_undo() {
    let (mut e, s1, s2, _) = design();
    let s3 = ok(
        &mut e,
        C::AddSheet {
            name: "Spare".into(),
        },
    )
    .created[0]
        .clone();
    ok(
        &mut e,
        C::DeleteSheet {
            sheet_id: s3.clone(),
        },
    );
    assert_eq!(order(e.project()), vec![s1.clone(), s2.clone()]);
    undo(&mut e);
    assert_eq!(order(e.project()), vec![s1, s2, s3]);
}
