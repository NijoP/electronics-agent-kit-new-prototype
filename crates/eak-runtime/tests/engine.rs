//! Command engine contract tests: shared path, revisions, approvals, undo/redo,
//! persistence, crash reconciliation, project isolation.

use eak_core::{LabelKind, Point, Project, Stackup};
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;
use eak_runtime::store;
use eak_runtime::{CommandError, OperationClass};
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn run(e: &mut Engine, actor: Actor, cmd: C) -> CommandResponse {
    let rev = e.project().revision;
    e.execute(CommandRequest {
        expected_revision: rev,
        actor,
        command: cmd,
        approval: None,
    })
}

fn place(e: &mut Engine, comp: &str, x: f64, y: f64) -> String {
    let r = run(
        e,
        Actor::human(),
        C::PlaceSymbol {
            sheet_id: None,
            component_id: comp.into(),
            position: Point::mm(x, y),
            rotation: Default::default(),
            refdes: None,
            value: None,
        },
    );
    assert!(r.accepted, "{:?}", r.error);
    r.created[0].clone()
}

#[test]
fn human_and_agent_use_the_same_command_implementation() {
    // Same command applied by a human and (approved) agent produces identical designs.
    let base = Project::new("x");
    let mut h = Engine::in_memory(base.clone());
    let mut a = Engine::in_memory(base);
    let cmd = C::SetBoardOutline {
        points: vec![
            Point::mm(0.0, 0.0),
            Point::mm(30.0, 0.0),
            Point::mm(30.0, 20.0),
            Point::mm(0.0, 20.0),
        ],
    };
    let rh = run(&mut h, Actor::human(), cmd.clone());
    let ra = a.execute(CommandRequest {
        expected_revision: 0,
        actor: Actor::agent("lead"),
        command: cmd,
        approval: Some(Approval {
            approved_by: "user".into(),
            max_class: OperationClass::EngineeringImpacting,
            note: String::new(),
        }),
    });
    assert!(rh.accepted && ra.accepted);
    assert_eq!(h.project().pcb.board, a.project().pcb.board);
    assert_eq!(rh.class, ra.class);
}

#[test]
fn agent_without_approval_is_refused() {
    let mut e = Engine::in_memory(Project::new("x"));
    let r = run(
        &mut e,
        Actor::agent("lead"),
        C::SetBoardOutline {
            points: vec![
                Point::mm(0.0, 0.0),
                Point::mm(1.0, 0.0),
                Point::mm(1.0, 1.0),
            ],
        },
    );
    assert!(!r.accepted);
    assert!(matches!(r.error, Some(CommandError::ApprovalRequired(_))));
    assert_eq!(e.project().revision, 0);
}

#[test]
fn stale_revision_is_rejected_never_applied() {
    let mut e = Engine::in_memory(Project::new("x"));
    place(&mut e, "R_0603", 0.0, 0.0);
    let r = e.execute(CommandRequest {
        expected_revision: 0,
        actor: Actor::human(),
        command: C::SetProjectName { name: "y".into() },
        approval: None,
    });
    assert!(matches!(
        r.error,
        Some(CommandError::StaleRevision {
            expected: 0,
            actual: 1
        })
    ));
    assert_eq!(e.project().name, "x");
}

#[test]
fn failed_precondition_leaves_project_untouched() {
    let mut e = Engine::in_memory(Project::new("x"));
    let before = e.project().clone();
    let r = run(
        &mut e,
        Actor::human(),
        C::DeleteWire {
            wire_id: "nope".into(),
        },
    );
    assert!(!r.accepted);
    assert_eq!(&before, e.project());
}

#[test]
fn invalid_stackup_is_rejected_and_leaves_project_untouched() {
    let mut e = Engine::in_memory(Project::new("x"));
    let before = e.project().clone();
    let mut bad = Stackup::default();
    bad.layers.pop();
    let mut thin = Stackup::default();
    thin.layers[0].dielectric_after_um = 100;
    for stackup in [bad, thin] {
        let r = run(&mut e, Actor::human(), C::SetStackup { stackup });
        assert!(!r.accepted);
        assert!(
            matches!(r.error, Some(CommandError::Invalid(_))),
            "{:?}",
            r.error
        );
        assert_eq!(&before, e.project());
    }
}

#[test]
fn undo_redo_round_trips_every_command_kind() {
    let mut e = Engine::in_memory(Project::new("x"));
    let mut snapshots = vec![e.project().clone()];
    let r1 = place(&mut e, "R_0603", 0.0, 0.0);
    snapshots.push(e.project().clone());
    let r2 = place(&mut e, "LED_0603", 20.0, 0.0);
    snapshots.push(e.project().clone());
    let cmds = vec![
        C::ConnectPins {
            a_symbol: r1.clone(),
            a_pin: "2".into(),
            b_symbol: r2.clone(),
            b_pin: "1".into(),
        },
        C::LabelPin {
            symbol_id: r1.clone(),
            pin: "1".into(),
            name: "VCC".into(),
            kind: LabelKind::Power,
            scope: Default::default(),
            power_style: None,
        },
        C::SetBoardOutline {
            points: vec![
                Point::mm(0.0, 0.0),
                Point::mm(30.0, 0.0),
                Point::mm(30.0, 20.0),
                Point::mm(0.0, 20.0),
            ],
        },
        C::SetStackup {
            stackup: Stackup::two_layer(1_600_000, 2.0, "FR-4", 4.5),
        },
        C::SyncSchematicToPcb,
        C::SetNetCurrent {
            net: "VCC".into(),
            amps: Some(0.5),
        },
        C::SetDesignRules {
            min_track_width: Some(200_000),
            min_clearance: None,
            min_drill: None,
            min_annular_ring: None,
            board_edge_clearance: None,
            default_track_width: None,
            default_clearance: None,
            temp_rise_c: Some(20.0),
            max_component_height: None,
            clear_max_component_height: false,
        },
        C::DeleteSymbol {
            symbol_id: r2.clone(),
        },
    ];
    for c in cmds {
        let r = run(&mut e, Actor::human(), c.clone());
        assert!(r.accepted, "{c:?}: {:?}", r.error);
        snapshots.push(e.project().clone());
    }
    let strip = |p: &Project| {
        let mut q = p.clone();
        q.revision = 0;
        q
    };
    // Undo everything, comparing against each earlier snapshot.
    for i in (0..snapshots.len() - 1).rev() {
        let rev = e.project().revision;
        assert!(e.undo(rev, &Actor::human()).accepted);
        assert_eq!(strip(e.project()), strip(&snapshots[i]), "undo to step {i}");
    }
    assert!(!e.can_undo());
    for snap in snapshots.iter().skip(1) {
        let rev = e.project().revision;
        assert!(e.redo(rev, &Actor::human()).accepted);
        assert_eq!(strip(e.project()), strip(snap));
    }
    // Revisions are monotonic: history is never rewritten.
    assert_eq!(e.project().revision as usize, (snapshots.len() - 1) * 3);
}

#[test]
fn persisted_project_reopens_with_history() {
    let dir = tmpdir("reopen");
    let rev;
    {
        let mut e = Engine::create(&dir, "Board").unwrap();
        place(&mut e, "R_0603", 0.0, 0.0);
        place(&mut e, "C_0603", 10.0, 0.0);
        rev = e.project().revision;
        // A second session cannot open while the lock is held.
        assert!(Engine::open(&dir, false).is_err());
    }
    let mut e = Engine::open(&dir, false).unwrap();
    assert_eq!(e.project().revision, rev);
    assert_eq!(e.project().schematic.symbols.len(), 2);
    assert!(e.can_undo());
    let r = e.undo(rev, &Actor::human());
    assert!(r.accepted);
    assert_eq!(e.project().schematic.symbols.len(), 1);
    drop(e);
    let e = Engine::open(&dir, false).unwrap();
    assert_eq!(e.project().schematic.symbols.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn interrupted_operation_is_reconciled_on_open() {
    let dir = tmpdir("crash");
    {
        let mut e = Engine::create(&dir, "Board").unwrap();
        place(&mut e, "R_0603", 0.0, 0.0);
    }
    // Simulate a crash after the pending journal record but before the project write.
    let p = store::read_project(&dir).unwrap();
    store::append_journal(
        &dir,
        &store::JournalEntry {
            op_id: "op_crashed".into(),
            state: store::EntryState::Pending,
            kind: store::EntryKind::Do,
            target_op: None,
            revision_before: p.revision,
            revision_after: p.revision + 1,
            actor: "human:user".into(),
            command: "PlaceSymbol".into(),
            changes: vec![],
            timestamp_ms: 0,
        },
    )
    .unwrap();
    std::fs::write(dir.join("project.eak.tmp"), b"{torn").unwrap();
    let _ = std::fs::remove_file(dir.join(".lock"));
    let e = Engine::open(&dir, false).unwrap();
    assert!(e.recovery.torn_temp_file_removed);
    assert_eq!(
        e.recovery.reconciled,
        vec![("op_crashed".to_string(), store::EntryState::Aborted)]
    );
    assert!(e.blocked_reason().is_none());
    assert_eq!(e.project().schematic.symbols.len(), 1);
    drop(e);

    // An operation whose recorded revisions match neither side is UNKNOWN and blocks writes.
    let p = store::read_project(&dir).unwrap();
    store::append_journal(
        &dir,
        &store::JournalEntry {
            op_id: "op_weird".into(),
            state: store::EntryState::Pending,
            kind: store::EntryKind::Do,
            target_op: None,
            revision_before: p.revision + 5,
            revision_after: p.revision + 6,
            actor: "human:user".into(),
            command: "X".into(),
            changes: vec![],
            timestamp_ms: 0,
        },
    )
    .unwrap();
    let mut e = Engine::open(&dir, false).unwrap();
    assert!(e.blocked_reason().is_some());
    let r = run(
        &mut e,
        Actor::human(),
        C::SetProjectName { name: "z".into() },
    );
    assert!(!r.accepted);
    // Regression (old PR #11): the block used to live only in memory, so simply reopening
    // the project (the next CLI or agent invocation) lifted it without any acknowledgement.
    drop(e);
    let mut e = Engine::open(&dir, false).unwrap();
    assert!(e.recovery.reconciled.is_empty(), "already reconciled");
    let reason = e
        .blocked_reason()
        .expect("block survives reopen")
        .to_string();
    assert!(reason.contains("op_weird"), "{reason}");
    assert!(
        !run(
            &mut e,
            Actor::human(),
            C::SetProjectName { name: "z".into() }
        )
        .accepted
    );
    e.acknowledge_recovery().unwrap();
    assert!(
        run(
            &mut e,
            Actor::human(),
            C::SetProjectName { name: "z".into() }
        )
        .accepted
    );
    drop(e);
    // Acknowledgement is durable too.
    assert!(Engine::open(&dir, false)
        .unwrap()
        .blocked_reason()
        .is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn checkpoint_restore_is_undoable() {
    let dir = tmpdir("ckpt");
    let mut e = Engine::create(&dir, "Board").unwrap();
    place(&mut e, "R_0603", 0.0, 0.0);
    let cp = e.create_checkpoint("one resistor").unwrap();
    place(&mut e, "R_0603", 10.0, 0.0);
    assert_eq!(e.list_checkpoints(), vec![cp.clone()]);
    let rev = e.project().revision;
    assert!(e.restore_checkpoint(&cp, rev, &Actor::human()).accepted);
    assert_eq!(e.project().schematic.symbols.len(), 1);
    let rev = e.project().revision;
    assert!(e.undo(rev, &Actor::human()).accepted);
    assert_eq!(e.project().schematic.symbols.len(), 2);
    drop(e);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn connect_pins_refuses_routes_through_other_pins() {
    let mut e = Engine::in_memory(Project::new("x"));
    let a = place(&mut e, "R_0603", 0.0, 0.0); // pins at x = -5.08, 5.08
    let b = place(&mut e, "R_0603", 30.0, 0.0);
    let _blocker = place(&mut e, "R_0603", 15.0, 0.0); // its pins sit on the straight route
    let r = run(
        &mut e,
        Actor::human(),
        C::ConnectPins {
            a_symbol: a,
            a_pin: "2".into(),
            b_symbol: b,
            b_pin: "1".into(),
        },
    );
    assert!(!r.accepted);
    assert!(matches!(r.error, Some(CommandError::Precondition(_))));
}

#[test]
fn sqlite_index_records_projects_and_events() {
    let idx = eak_runtime::index::Index::in_memory().unwrap();
    idx.touch_project("/a", "prj_1", "A", 3).unwrap();
    idx.touch_project("/a", "prj_1", "A2", 4).unwrap();
    let r = idx.recent(10).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].name, "A2");
    let mut e = Engine::in_memory(Project::new("x"));
    place(&mut e, "R_0603", 0.0, 0.0);
    for ev in e.events_since(0) {
        idx.log_event(&ev).unwrap();
    }
    assert!(idx.event_count(&e.project().id).unwrap() >= 1);
}

// ---- Phase 5: library properties, footprint assignment, placement

fn props(id: &str, mfr: Option<&str>, mpn: Option<&str>, distributor: Option<&str>) -> C {
    C::SetComponentProperties {
        component_id: id.into(),
        manufacturer: mfr.map(Into::into),
        mpn: mpn.map(Into::into),
        distributor: distributor.map(Into::into),
        datasheet: None,
    }
}

#[test]
fn placing_a_library_part_imports_it_and_syncs_a_linked_footprint() {
    let mut e = Engine::in_memory(Project::new("lib"));
    let sym = place(&mut e, "R_0603", 0.0, 0.0);
    assert!(e.project().library.contains_key("R_0603"));
    let r = run(&mut e, Actor::human(), C::SyncSchematicToPcb);
    assert!(r.accepted, "{:?}", r.error);
    let fp = e
        .project()
        .pcb
        .footprints
        .values()
        .find(|f| f.symbol_id == sym)
        .expect("footprint linked to the placed symbol");
    assert_eq!(fp.component_id, "R_0603");
}

#[test]
fn set_component_properties_edits_fields_and_preserves_provenance() {
    let mut e = Engine::in_memory(Project::new("lib"));
    place(&mut e, "R_0603", 0.0, 0.0);
    let before = e.project().library["R_0603"].clone();
    let r = run(
        &mut e,
        Actor::human(),
        props(
            "R_0603",
            Some(" Yageo "),
            Some("RC0603FR-0710KL"),
            Some("C25804"),
        ),
    );
    assert!(r.accepted, "{:?}", r.error);
    let after = &e.project().library["R_0603"];
    assert_eq!(after.manufacturer.as_deref(), Some("Yageo"));
    assert_eq!(after.mpn.as_deref(), Some("RC0603FR-0710KL"));
    assert_eq!(after.distributor.as_deref(), Some("C25804"));
    // Provenance, ratings, geometry and pin map are untouched.
    assert_eq!(after.provenance, before.provenance);
    assert_eq!(after.ratings, before.ratings);
    assert_eq!(after.symbol, before.symbol);
    assert_eq!(after.footprint, before.footprint);
    assert_eq!(after.pin_map, before.pin_map);
    // Blank clears a field.
    assert!(
        run(
            &mut e,
            Actor::human(),
            props("R_0603", Some("  "), None, None)
        )
        .accepted
    );
    assert_eq!(e.project().library["R_0603"].mpn, None);
    // Undo restores the previous values exactly.
    let rev = e.project().revision;
    assert!(e.undo(rev, &Actor::human()).accepted);
    assert_eq!(
        e.project().library["R_0603"].mpn.as_deref(),
        Some("RC0603FR-0710KL")
    );
}

#[test]
fn set_component_properties_rejects_unknown_component() {
    let mut e = Engine::in_memory(Project::new("lib"));
    let r = run(&mut e, Actor::human(), props("NOPE", None, None, None));
    assert!(!r.accepted);
}

#[test]
fn assign_footprint_validates_records_provenance_and_undoes() {
    let mut e = Engine::in_memory(Project::new("lib"));
    place(&mut e, "R_0603", 0.0, 0.0);
    let donor = eak_core::builtin::components()
        .into_iter()
        .find(|c| c.id != "R_0603" && c.pin_map.len() == 2 && c.footprint.name != "R_0603")
        .expect("another two-pin part");
    let before = e.project().library["R_0603"].clone();
    let r = run(
        &mut e,
        Actor::human(),
        C::AssignComponentFootprint {
            component_id: "R_0603".into(),
            footprint: donor.footprint.clone(),
            pin_map: donor.pin_map.clone(),
        },
    );
    assert!(r.accepted, "{:?}", r.error);
    let after = &e.project().library["R_0603"];
    assert_eq!(after.footprint.name, donor.footprint.name);
    assert!(after.provenance.starts_with(&before.provenance));
    assert!(after.provenance.contains(&donor.footprint.name));
    // A pin map that points at a missing pad is rejected and changes nothing.
    let mut bad = donor.pin_map.clone();
    bad.insert("1".into(), vec!["99".into()]);
    let rev = e.project().revision;
    let r = run(
        &mut e,
        Actor::human(),
        C::AssignComponentFootprint {
            component_id: "R_0603".into(),
            footprint: donor.footprint.clone(),
            pin_map: bad,
        },
    );
    assert!(!r.accepted);
    assert_eq!(e.project().revision, rev);
    let rev = e.project().revision;
    assert!(e.undo(rev, &Actor::human()).accepted);
    assert_eq!(e.project().library["R_0603"], before);
}

#[test]
fn assign_footprint_is_refused_once_pcb_footprints_exist() {
    let mut e = Engine::in_memory(Project::new("lib"));
    place(&mut e, "R_0603", 0.0, 0.0);
    assert!(run(&mut e, Actor::human(), C::SyncSchematicToPcb).accepted);
    let def = e.project().library["R_0603"].clone();
    let r = run(
        &mut e,
        Actor::human(),
        C::AssignComponentFootprint {
            component_id: "R_0603".into(),
            footprint: def.footprint,
            pin_map: def.pin_map,
        },
    );
    assert!(!r.accepted);
}
