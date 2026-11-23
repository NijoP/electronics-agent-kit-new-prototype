//! Schema v5 (ADR-0044) through the engine: `SetPadExpansion` (per-pad solder-mask and paste
//! expansion) and `SetRoutingOptions` (the route tool's options saved with the board). Each is
//! one undo step, a refusal changes nothing, and both survive a save and load. Written without
//! running (no local builds): CI decides.

use eak_core::{
    from_json, to_json, BendMode, ConflictMode, Point, Project, RoutingOptions, ViaSize,
};
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

fn expansion(component: &str, pad: &str, mask: Option<i64>, paste: Option<i64>) -> C {
    C::SetPadExpansion {
        component_id: component.into(),
        pad: pad.into(),
        mask_expansion: mask,
        paste_expansion: paste,
    }
}

/// A project with an R_0603 (two SMD pads) on the schematic and its footprint on the PCB.
fn placed() -> Engine {
    let mut e = Engine::in_memory(Project::new("v5"));
    ok(
        &mut e,
        C::PlaceSymbol {
            sheet_id: None,
            component_id: "R_0603".into(),
            position: Point::mm(10.0, 10.0),
            rotation: Default::default(),
            refdes: None,
            value: None,
        },
    );
    ok(&mut e, C::SyncSchematicToPcb);
    assert_eq!(e.project().pcb.footprints.len(), 1);
    e
}

#[test]
fn set_pad_expansion_works_while_the_part_is_placed_and_undoes() {
    let mut e = placed();
    // AssignComponentFootprint is refused here; a pad's openings are not.
    ok(
        &mut e,
        expansion("R_0603", "1", Some(75_000), Some(-30_000)),
    );
    let pad = &e.project().library["R_0603"].footprint.pads[0];
    assert_eq!(pad.number, "1");
    assert_eq!(
        (pad.mask_expansion, pad.paste_expansion),
        (Some(75_000), Some(-30_000))
    );
    let other = &e.project().library["R_0603"].footprint.pads[1];
    assert_eq!((other.mask_expansion, other.paste_expansion), (None, None));
    // Clearing goes back to the board default.
    ok(&mut e, expansion("R_0603", "1", None, None));
    assert_eq!(
        e.project().library["R_0603"].footprint.pads[0].mask_expansion,
        None
    );
    undo(&mut e);
    assert_eq!(
        e.project().library["R_0603"].footprint.pads[0].mask_expansion,
        Some(75_000)
    );
    undo(&mut e);
    assert_eq!(
        e.project().library["R_0603"].footprint.pads[0].mask_expansion,
        None
    );
}

#[test]
fn set_pad_expansion_refuses_bad_values_and_unknown_targets() {
    let mut e = placed();
    assert!(matches!(
        refused(&mut e, expansion("NOPE", "1", Some(0), None)),
        CommandError::NotFound(_)
    ));
    assert!(matches!(
        refused(&mut e, expansion("R_0603", "9", Some(0), None)),
        CommandError::NotFound(_)
    ));
    // Beyond the 1 mm limit, and an expansion that closes the opening.
    let e1 = refused(&mut e, expansion("R_0603", "1", Some(2_000_000), None));
    assert!(e1.to_string().contains("1 mm"), "{e1}");
    let e2 = refused(&mut e, expansion("R_0603", "1", None, Some(-900_000)));
    assert!(matches!(e2, CommandError::Invalid(_)), "{e2:?}");
}

#[test]
fn set_routing_options_saves_with_the_board_and_undoes() {
    let mut e = Engine::in_memory(Project::new("routing"));
    assert_eq!(e.project().pcb.routing, RoutingOptions::default());
    let opts = RoutingOptions {
        bend: BendMode::Orthogonal90,
        conflict: ConflictMode::Push,
        width: Some(400_000),
        via: Some(ViaSize {
            drill: 300_000,
            diameter: 650_000,
        }),
        remove_loops: false,
    };
    ok(
        &mut e,
        C::SetRoutingOptions {
            options: opts.clone(),
        },
    );
    assert_eq!(e.project().pcb.routing, opts);
    // The options survive a save and load (the project file holds them).
    let saved = to_json(e.project());
    assert_eq!(from_json(&saved).unwrap().pcb.routing, opts);
    undo(&mut e);
    assert_eq!(e.project().pcb.routing, RoutingOptions::default());

    let bad_via = RoutingOptions {
        via: Some(ViaSize {
            drill: 400_000,
            diameter: 400_000,
        }),
        ..opts.clone()
    };
    refused(&mut e, C::SetRoutingOptions { options: bad_via });
    let bad_width = RoutingOptions {
        width: Some(0),
        ..opts
    };
    refused(&mut e, C::SetRoutingOptions { options: bad_width });
}
