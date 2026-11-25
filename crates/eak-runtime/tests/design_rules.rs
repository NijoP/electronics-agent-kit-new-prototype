//! `SetDesignRules` and the optional max component height (ADR-0050, DoD audit S19): a `None`
//! field means "unchanged", so `clear_max_component_height` is the explicit way to remove the
//! limit. The clear is one revision and one undo step, is refused together with a new value, and
//! the flag round-trips through JSON (absent in older command JSON, omitted when false).

use eak_core::Project;
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

fn rules(height: Option<i64>, clear: bool) -> C {
    C::SetDesignRules {
        min_track_width: None,
        min_clearance: None,
        min_drill: None,
        min_annular_ring: None,
        board_edge_clearance: None,
        default_track_width: None,
        default_clearance: None,
        temp_rise_c: None,
        max_component_height: height,
        clear_max_component_height: clear,
    }
}

#[test]
fn the_max_component_height_is_cleared_only_explicitly_and_the_clear_undoes() {
    let mut e = Engine::in_memory(Project::new("rules"));
    assert!(run(&mut e, rules(Some(12_000_000), false)).accepted);
    assert_eq!(e.project().constraints.max_component_height, Some(12_000_000));

    // An empty (None) height leaves the stored limit alone: nothing changes.
    let r = run(&mut e, rules(None, false));
    assert!(matches!(r.error, Some(CommandError::NothingTo(_))), "{r:?}");
    assert_eq!(e.project().constraints.max_component_height, Some(12_000_000));

    // A value and a clear together are refused and change nothing.
    let rev = e.project().revision;
    let r = run(&mut e, rules(Some(5_000_000), true));
    assert!(matches!(r.error, Some(CommandError::Invalid(_))), "{r:?}");
    assert_eq!(e.project().revision, rev);
    assert_eq!(e.project().constraints.max_component_height, Some(12_000_000));

    // The explicit clear removes it, as one undo step.
    let r = run(&mut e, rules(None, true));
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(e.project().constraints.max_component_height, None);
    let u = e.undo(e.project().revision, &Actor::human());
    assert!(u.accepted, "{:?}", u.error);
    assert_eq!(e.project().constraints.max_component_height, Some(12_000_000));
}

#[test]
fn the_clear_flag_round_trips_and_older_command_json_still_decodes() {
    // Command JSON written before ADR-0050 has no flag: it decodes as "do not clear".
    let old = serde_json::json!({
        "type": "SetDesignRules",
        "min_track_width": null, "min_clearance": null, "min_drill": null,
        "min_annular_ring": null, "board_edge_clearance": null, "default_track_width": null,
        "default_clearance": null, "temp_rise_c": null, "max_component_height": 3000000
    });
    let c: C = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(c, rules(Some(3_000_000), false));
    // A false flag is not written, so that command serialises exactly as before.
    assert_eq!(serde_json::to_value(&c).unwrap(), old);

    let clear = rules(None, true);
    let v = serde_json::to_value(&clear).unwrap();
    assert_eq!(v["clear_max_component_height"], true);
    assert_eq!(serde_json::from_value::<C>(v).unwrap(), clear);
}
