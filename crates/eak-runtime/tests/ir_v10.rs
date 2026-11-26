//! Schema v10 (ADR-0059) through the engine: `SetEngineeringInputs` declares the largest
//! return-via distance and the plane neck width; a limit that is not positive is refused without
//! changing the project, an omitted limit is cleared (the command carries the whole set), setting
//! them is one undo step, and they survive a save and load. Written without running (no local
//! builds): CI decides.

use eak_core::{from_json, to_json, Nm, Project};
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;

fn run(e: &mut Engine, cmd: C) -> CommandResponse {
    let rev = e.project().revision;
    e.execute(CommandRequest {
        expected_revision: rev,
        actor: Actor::human(),
        command: cmd,
        approval: None,
    })
}

/// `SetEngineeringInputs` with only the two reference-plane limits declared.
fn limits(return_via: Option<Nm>, neck: Option<Nm>) -> C {
    C::SetEngineeringInputs {
        ambient_temp_c: None,
        via_plating_um: None,
        net_max_drop_v: Default::default(),
        part_dissipation_w: Default::default(),
        insulation: None,
        enclosure: None,
        return_via_max_distance: return_via,
        plane_min_neck_width: neck,
    }
}

#[test]
fn reference_plane_limits_are_set_refused_cleared_undone_and_saved() {
    let mut e = Engine::in_memory(Project::new("v10"));
    for bad in [limits(Some(0), None), limits(None, Some(-5))] {
        let before = e.project().clone();
        let r = run(&mut e, bad);
        assert!(!r.accepted, "a limit that is not positive was accepted");
        assert_eq!(e.project(), &before, "a refusal changed the project");
    }

    let r = run(&mut e, limits(Some(1_500_000), Some(400_000)));
    assert!(r.accepted, "{:?}", r.error);
    let c = &e.project().constraints;
    assert_eq!(c.return_via_max_distance, Some(1_500_000));
    assert_eq!(c.plane_min_neck_width, Some(400_000));

    let saved = to_json(e.project());
    let back = from_json(&saved).expect("a v10 project loads");
    assert_eq!(&back, e.project());
    assert_eq!(saved, to_json(&back));

    // One undo step brings back the undeclared state.
    let rev = e.project().revision;
    assert!(e.undo(rev, &Actor::human()).accepted);
    assert_eq!(e.project().constraints.return_via_max_distance, None);
    let rev = e.project().revision;
    assert!(e.redo(rev, &Actor::human()).accepted);
    assert_eq!(e.project().constraints.plane_min_neck_width, Some(400_000));

    // An omitted field is cleared: the command carries the whole set.
    let r = run(&mut e, limits(None, None));
    assert!(r.accepted, "{:?}", r.error);
    let json = to_json(e.project());
    for k in ["return_via_max_distance", "plane_min_neck_width"] {
        assert!(!json.contains(k), "an undeclared {k} must not be written");
    }
}
