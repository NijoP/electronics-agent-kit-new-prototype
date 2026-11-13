//! IPC contract tests: the desktop command layer exercised as plain Rust functions
//! (no window, no WebView), so they run headless on every CI OS.

use eak_core::{Point, Project, Rotation};
use eak_desktop::*;
use eak_runtime::command::Command as C;
use eak_runtime::index::Index;
use eak_runtime::store::{self, EntryKind, EntryState, JournalEntry};
use eak_runtime::{fixtures, CommandError, Engine};
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-desktop-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn session() -> Session {
    Session::new(Index::in_memory())
}

fn path(d: &std::path::Path) -> String {
    d.display().to_string()
}

fn outline() -> C {
    C::SetBoardOutline {
        points: vec![
            Point::mm(0.0, 0.0),
            Point::mm(30.0, 0.0),
            Point::mm(30.0, 20.0),
            Point::mm(0.0, 20.0),
        ],
    }
}

#[test]
fn execute_undo_redo_go_through_the_engine_as_human_with_revision_checks() {
    let d = tmpdir("exec");
    let s = session();
    let snap = create_project(&s, &path(&d), "Shell test").unwrap();
    assert_eq!(snap.project.revision, 0);
    assert!(!snap.can_undo);

    let r = execute_command(&s, 0, outline()).unwrap();
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(r.revision_after, 1);

    // A request built from a stale render is rejected, never applied.
    let stale = execute_command(&s, 0, C::SetProjectName { name: "x".into() }).unwrap();
    assert!(!stale.accepted);
    assert!(matches!(
        stale.error,
        Some(CommandError::StaleRevision {
            expected: 0,
            actual: 1
        })
    ));
    assert!(
        !undo(&s, 0).unwrap().accepted,
        "stale undo must be rejected"
    );

    let u = undo(&s, 1).unwrap();
    assert!(u.accepted, "{:?}", u.error);
    assert!(current_project(&s)
        .unwrap()
        .project
        .pcb
        .board
        .outline
        .is_empty());
    let r2 = redo(&s, 2).unwrap();
    assert!(r2.accepted, "{:?}", r2.error);
    let snap = current_project(&s).unwrap();
    assert_eq!(snap.project.revision, 3);
    assert_eq!(snap.project.pcb.board.outline.len(), 4);

    let events = events_since(&s, 0).unwrap();
    let kinds: Vec<&str> = events.iter().map(|e| e.kind.as_str()).collect();
    for k in [
        "project_created",
        "command_applied",
        "command_rejected",
        "undo",
        "redo",
    ] {
        assert!(kinds.contains(&k), "missing event {k}: {kinds:?}");
    }
    let applied = events.iter().find(|e| e.kind == "command_applied").unwrap();
    assert_eq!(applied.actor, "human:user");
    let last = events.last().unwrap().seq;
    assert!(events_since(&s, last).unwrap().is_empty());

    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn reopen_restores_state_history_and_recent_index() {
    let d = tmpdir("reopen");
    let s = session();
    create_project(&s, &path(&d), "Reopen").unwrap();
    assert!(execute_command(&s, 0, outline()).unwrap().accepted);
    close_project(&s).unwrap();
    assert_eq!(
        current_project(&s).unwrap_err(),
        "no project is open",
        "closed session has no engine"
    );

    let snap = open_project(&s, &path(&d), false).unwrap();
    assert_eq!(snap.project.revision, 1);
    assert!(snap.can_undo, "undo history is rebuilt from the journal");
    // Re-opening the folder that is already open releases our own lock first.
    let again = open_project(&s, &path(&d), false).unwrap();
    assert_eq!(again.project.id, snap.project.id);

    let recent = recent_projects(&s, 10).unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].path, path(&d));
    assert_eq!(recent[0].name, "Reopen");
    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn invalid_requests_fail_safely() {
    let s = session();
    assert!(create_project(&s, "relative/dir", "x")
        .unwrap_err()
        .contains("absolute"));
    assert!(open_project(&s, "  ", false).is_err());
    assert_eq!(
        execute_command(&s, 0, outline()).unwrap_err(),
        "no project is open"
    );
    assert!(run_check(&s, Check::Erc).is_err());

    let d = tmpdir("dup");
    create_project(&s, &path(&d), "One").unwrap();
    close_project(&s).unwrap();
    assert!(create_project(&s, &path(&d), "Two")
        .unwrap_err()
        .contains("already contains"));
    assert!(create_project(&s, &path(&d), " ").is_err());

    // A missing index disables the recent list without blocking project work.
    let no_index = Session::new(Err("disk full".into()));
    assert!(recent_projects(&no_index, 5)
        .unwrap_err()
        .contains("disk full"));
    assert!(open_project(&no_index, &path(&d), false).is_ok());
    close_project(&no_index).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn locked_project_needs_explicit_take_over() {
    let d = tmpdir("lock");
    let holder = Engine::create(&d, "Held").unwrap();
    let s = session();
    assert!(open_project(&s, &path(&d), false)
        .unwrap_err()
        .contains("locked"));
    let snap = open_project(&s, &path(&d), true).unwrap();
    assert_eq!(snap.project.name, "Held");
    drop(holder);
    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn unknown_outcome_blocks_writes_until_acknowledged() {
    let d = tmpdir("recover");
    drop(Engine::create(&d, "Crash").unwrap());
    // A pending operation whose revisions match neither side of the on-disk project.
    store::append_journal(
        &d,
        &JournalEntry {
            op_id: "op_lost".into(),
            state: EntryState::Pending,
            kind: EntryKind::Do,
            target_op: None,
            revision_before: 5,
            revision_after: 6,
            actor: "human:user".into(),
            command: "SetProjectName".into(),
            changes: vec![],
            timestamp_ms: 0,
        },
    )
    .unwrap();
    let s = session();
    let snap = open_project(&s, &path(&d), false).unwrap();
    assert!(snap.blocked.is_some());
    assert_eq!(snap.recovery.reconciled.len(), 1);
    let r = execute_command(&s, 0, outline()).unwrap();
    assert!(matches!(r.error, Some(CommandError::Precondition(_))));
    let snap = acknowledge_recovery(&s).unwrap();
    assert!(snap.blocked.is_none());
    assert!(execute_command(&s, 0, outline()).unwrap().accepted);
    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn checks_are_revision_bound_and_saved_as_evidence() {
    let d = tmpdir("checks");
    let s = session();
    create_project(&s, &path(&d), "Checks").unwrap();
    let r = execute_command(
        &s,
        0,
        C::PlaceSymbol {
            sheet_id: None,
            component_id: "R_0603".into(),
            position: Point::mm(50.0, 50.0),
            rotation: Rotation::R0,
            refdes: None,
            value: None,
        },
    )
    .unwrap();
    assert!(r.accepted, "{:?}", r.error);
    let rev = current_project(&s).unwrap().project.revision;

    let erc = run_check(&s, Check::Erc).unwrap();
    assert_eq!(erc.report.check, "ERC");
    assert_eq!(erc.report.revision, rev);
    assert!(
        erc.report.errors > 0,
        "unconnected resistor must be flagged"
    );
    assert!(erc
        .report
        .findings
        .iter()
        .any(|f| f.objects.contains(&r.created[0])));
    let ev = PathBuf::from(erc.evidence_path.unwrap());
    assert!(ev.starts_with(&d) && ev.exists());

    let drc = run_check(&s, Check::Drc).unwrap();
    assert_eq!(drc.report.check, "DRC");
    assert_eq!(drc.report.revision, rev);
    assert!(!drc.report.not_implemented.is_empty());
    assert!(PathBuf::from(drc.evidence_path.unwrap()).exists());
    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn geometry_is_derived_from_eak_ir_with_cross_probe_ids() {
    let mut e = Engine::in_memory(Project::new("geom"));
    fixtures::build_schematic(&mut e).unwrap();
    fixtures::build_pcb(&mut e).unwrap();
    let p = e.project();
    let g = geometry(p);
    assert_eq!(g.symbols.len(), p.schematic.symbols.len());
    assert_eq!(g.footprints.len(), p.pcb.footprints.len());
    assert!(!g.footprints.is_empty());
    for f in &g.footprints {
        assert!(
            g.symbols.iter().any(|s| s.id == f.symbol_id),
            "footprint {} must cross-probe to a symbol",
            f.refdes
        );
        let inst = &p.pcb.footprints[&f.id];
        let def = &p.library[&inst.component_id];
        assert_eq!(f.pads.len(), def.footprint.pads.len());
        for (pad, pd) in f.pads.iter().zip(&def.footprint.pads) {
            assert_eq!(pad.position, p.pad_position(inst, pd));
        }
        assert!(
            f.pads.iter().any(|pad| pad.net.is_some()),
            "pad nets synced"
        );
    }
    for s in &g.symbols {
        let inst = &p.schematic.symbols[&s.id];
        for pin in &s.pins {
            assert_eq!(Some(pin.position), p.pin_position(inst, &pin.number));
        }
    }

    // Quarter turns swap the drawn extents.
    let sym = g.symbols[0].clone();
    let rev = p.revision;
    let r = e.execute(eak_runtime::CommandRequest {
        expected_revision: rev,
        actor: eak_runtime::Actor::human(),
        command: C::RotateSymbol {
            symbol_id: sym.id.clone(),
            rotation: Rotation::R90,
        },
        approval: None,
    });
    assert!(r.accepted, "{:?}", r.error);
    let turned = geometry(e.project())
        .symbols
        .into_iter()
        .find(|s| s.id == sym.id)
        .unwrap();
    assert_eq!((turned.half_w, turned.half_h), (sym.half_h, sym.half_w));

    // Wire shape consumed by src/types.ts.
    let v = serde_json::to_value(snapshot(&e)).unwrap();
    for key in [
        "project", "dir", "can_undo", "can_redo", "blocked", "recovery", "geometry",
    ] {
        assert!(v.get(key).is_some(), "snapshot key {key}");
    }
    let fp = &v["geometry"]["footprints"][0];
    for key in [
        "id",
        "symbol_id",
        "refdes",
        "side",
        "position",
        "half_w",
        "pads",
    ] {
        assert!(fp.get(key).is_some(), "footprint key {key}");
    }
}

#[test]
fn frontend_invokes_exactly_the_registered_commands() {
    let api = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/api.ts"),
    )
    .unwrap();
    let start = api
        .find("IPC_COMMANDS = [")
        .expect("api.ts declares IPC_COMMANDS");
    let list = &api[start..api[start..].find(']').unwrap() + start];
    let mut ts: Vec<&str> = list.split('"').skip(1).step_by(2).collect();
    ts.sort();
    let mut rust = IPC_COMMANDS.to_vec();
    rust.sort();
    assert_eq!(ts, rust);
}

#[test]
fn library_lists_builtin_parts_with_pins() {
    let lib = library_components();
    assert!(lib.len() >= 4);
    let r = lib.iter().find(|c| c.id == "R_0603").expect("R_0603");
    assert_eq!(r.refdes_prefix, "R");
    assert_eq!(r.pins.len(), 2);
}

/// The editor flow the UI drives: place, wire, outline, sync, route, check, export, reopen.
#[test]
fn editor_flow_places_wires_routes_and_exports_gerbers() {
    let d = tmpdir("editor");
    let s = session();
    create_project(&s, &path(&d), "Editor flow").unwrap();
    let mut rev = 0;
    let mut run = |c: C| {
        let r = execute_command(&s, rev, c).unwrap();
        assert!(r.accepted, "{:?}", r.error);
        rev = r.revision_after;
        r
    };
    let place = |x: f64| C::PlaceSymbol {
        sheet_id: None,
        component_id: "R_0603".into(),
        position: Point::mm(x, -50.0),
        rotation: Rotation::default(),
        refdes: None,
        value: None,
    };
    let a = run(place(40.0)).created[0].clone();
    let b = run(place(80.0)).created[0].clone();
    run(C::ConnectPins {
        a_symbol: a,
        a_pin: "2".into(),
        b_symbol: b,
        b_pin: "1".into(),
    });
    run(outline());
    run(C::SyncSchematicToPcb);
    run(C::AutoPlace { grid: None });
    run(C::AutoRoute { nets: None });
    let snap = current_project(&s).unwrap();
    assert_eq!(snap.geometry.footprints.len(), 2);

    let erc = run_check(&s, Check::Erc).unwrap();
    let drc = run_check(&s, Check::Drc).unwrap();
    assert_eq!(erc.report.revision, snap.project.revision);
    assert_eq!(drc.report.revision, snap.project.revision);

    let out = export_manufacturing(&s).unwrap();
    let names: Vec<&str> = out.manifest.files.iter().map(|f| f.name.as_str()).collect();
    for ext in [".gtl", ".gbl", ".gm1", ".drl"] {
        assert!(names.iter().any(|n| n.ends_with(ext)), "missing {ext}");
    }
    assert!(std::path::Path::new(&out.out_dir).join("manifest.json").exists());
    assert!(eak_manufacturing::verify_package(std::path::Path::new(&out.out_dir))
        .unwrap()
        .is_empty());
    assert!(!out.manifest.statement.to_lowercase().contains("manufacturing-ready claim: yes"));

    close_project(&s).unwrap();
    let again = open_project(&s, &path(&d), false).unwrap();
    assert_eq!(again.project.revision, snap.project.revision);
    assert_eq!(again.project.pcb.footprints.len(), 2);
    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn export_without_outline_or_project_fails_loudly() {
    let s = session();
    assert!(export_manufacturing(&s).is_err());
    let d = tmpdir("noexport");
    create_project(&s, &path(&d), "No outline").unwrap();
    assert!(export_manufacturing(&s).unwrap_err().contains("outline"));
    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}
