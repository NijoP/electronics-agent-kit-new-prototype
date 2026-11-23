//! SyncSchematicToPcbSelected (ADR-0042): apply only the chosen changes of the
//! schematic-to-PCB update, as one revision, refusing a selection that is out of date.

use eak_core::*;
use eak_runtime::command::{self, Command as C};
use eak_runtime::engine::*;
use eak_runtime::{fixtures, sync_changes, CommandError, SyncChange};
use std::collections::BTreeMap;

fn run(e: &mut Engine, cmd: C) -> CommandResponse {
    let rev = e.project().revision;
    e.execute(CommandRequest {
        expected_revision: rev,
        actor: Actor::human(),
        command: cmd,
        approval: None,
    })
}

fn schematic() -> Engine {
    let mut e = Engine::in_memory(Project::new("sel"));
    fixtures::build_schematic(&mut e).unwrap();
    let r = run(
        &mut e,
        C::SetBoardOutline {
            points: vec![
                Point::mm(0.0, 0.0),
                Point::mm(40.0, 0.0),
                Point::mm(40.0, 30.0),
                Point::mm(0.0, 30.0),
            ],
        },
    );
    assert!(r.accepted);
    e
}

/// What the full sync would change now (computed on a copy, like the ECO preview).
fn preview(e: &Engine) -> Vec<SyncChange> {
    let mut next = e.project().clone();
    command::apply(&mut next, &C::SyncSchematicToPcb).unwrap();
    sync_changes(e.project(), &next)
}

type FpView = (String, String, BTreeMap<String, String>);

/// Footprints keyed by symbol, without their random ids.
fn by_symbol(p: &Project) -> BTreeMap<Id, FpView> {
    p.pcb
        .footprints
        .values()
        .map(|f| {
            (
                f.symbol_id.clone(),
                (f.refdes.clone(), f.component_id.clone(), f.pad_nets.clone()),
            )
        })
        .collect()
}

fn first_pad_net(all: &[SyncChange]) -> (Id, String) {
    all.iter()
        .find_map(|c| match c {
            SyncChange::PadNet { symbol_id, pad } => Some((symbol_id.clone(), pad.clone())),
            _ => None,
        })
        .expect("a pad net to sync")
}

fn refused(e: &mut Engine, changes: Vec<SyncChange>) -> String {
    let before = e.project().clone();
    let r = run(e, C::SyncSchematicToPcbSelected { changes });
    assert!(!r.accepted);
    assert_eq!(e.project(), &before);
    match r.error.unwrap() {
        CommandError::Invalid(m) => m,
        other => panic!("expected Invalid, got {other:?}"),
    }
}

#[test]
fn selecting_every_change_equals_the_full_sync() {
    // Compare with the full sync of the same project (fixture ids and auto net names differ
    // between builds).
    let mut a = schematic();
    let mut full = a.project().clone();
    command::apply(&mut full, &C::SyncSchematicToPcb).unwrap();
    let all = preview(&a);
    assert!(all.len() > 3);
    let rev = a.project().revision;
    let r = run(&mut a, C::SyncSchematicToPcbSelected { changes: all });
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(a.project().revision, rev + 1, "one revision");
    assert!(
        !r.warnings.iter().any(|w| w.starts_with("applied ")),
        "{:?}",
        r.warnings
    );
    assert_eq!(by_symbol(a.project()), by_symbol(&full));
    assert!(preview(&a).is_empty(), "nothing left to sync");
}

#[test]
fn a_subset_applies_only_what_was_chosen_and_undoes_as_one_step() {
    let mut e = schematic();
    let all = preview(&e);
    let (sym, pad) = first_pad_net(&all);
    let chosen = vec![
        SyncChange::AddFootprint {
            symbol_id: sym.clone(),
        },
        SyncChange::PadNet {
            symbol_id: sym.clone(),
            pad: pad.clone(),
        },
    ];
    let r = run(&mut e, C::SyncSchematicToPcbSelected { changes: chosen });
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(r.created.len(), 1);
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains(&format!("applied 2 of {}", all.len()))),
        "{:?}",
        r.warnings
    );
    let fps = by_symbol(e.project());
    assert_eq!(fps.len(), 1);
    assert_eq!(fps[&sym].2.keys().collect::<Vec<_>>(), vec![&pad]);
    // The rest is still pending, and the footprint just added is no longer an addition.
    let rest = preview(&e);
    assert_eq!(rest.len(), all.len() - 2);
    assert!(!rest.contains(&SyncChange::AddFootprint {
        symbol_id: sym.clone()
    }));
    let rev = e.project().revision;
    assert!(e.undo(rev, &Actor::human()).accepted);
    assert!(e.project().pcb.footprints.is_empty());
}

#[test]
fn reference_changes_can_be_applied_alone() {
    let mut e = schematic();
    assert!(run(&mut e, C::SyncSchematicToPcb).accepted);
    // SetRefdes keeps the footprint in step itself, so make the PCB drift the way an older file
    // or an external edit would: load a project whose footprint still has an old reference.
    let s = e
        .project()
        .schematic
        .symbols
        .values()
        .next()
        .unwrap()
        .clone();
    let mut drifted = e.project().clone();
    drifted
        .pcb
        .footprints
        .values_mut()
        .find(|f| f.symbol_id == s.id)
        .unwrap()
        .refdes = "OLD9".into();
    let mut e = Engine::in_memory(drifted);
    let pending = preview(&e);
    assert_eq!(pending.len(), 1, "{pending:?}");
    let refdes = SyncChange::Refdes {
        symbol_id: s.id.clone(),
    };
    assert!(pending.contains(&refdes), "{pending:?}");
    let r = run(
        &mut e,
        C::SyncSchematicToPcbSelected {
            changes: vec![refdes],
        },
    );
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(by_symbol(e.project())[&s.id].0, s.refdes);
}

#[test]
fn out_of_date_or_inconsistent_selections_are_refused() {
    let mut e = schematic();
    let (sym, pad) = first_pad_net(&preview(&e));
    let net = SyncChange::PadNet {
        symbol_id: sym.clone(),
        pad,
    };
    assert!(refused(&mut e, vec![]).contains("at least one"));
    assert!(refused(&mut e, vec![net.clone()]).contains("footprint"));
    assert!(refused(&mut e, vec![net.clone(), net]).contains("twice"));
    // Nothing to rename before the first sync: a stale preview row is refused.
    let m = refused(&mut e, vec![SyncChange::Refdes { symbol_id: sym }]);
    assert!(m.contains("preview the update again"), "{m}");
    assert!(refused(
        &mut e,
        vec![SyncChange::AddFootprint {
            symbol_id: "no-such-symbol".into()
        }]
    )
    .contains("not part of"));
}

#[test]
fn the_wire_form_is_tagged_by_kind() {
    let c = C::SyncSchematicToPcbSelected {
        changes: vec![SyncChange::PadNet {
            symbol_id: "s1".into(),
            pad: "2".into(),
        }],
    };
    let v = serde_json::to_value(&c).unwrap();
    assert_eq!(
        v,
        serde_json::json!({
            "type": "SyncSchematicToPcbSelected",
            "changes": [{"kind": "pad_net", "symbol_id": "s1", "pad": "2"}]
        })
    );
}

/// The ECO dialog's Apply (C2): a `Batch` holding either the full sync or only the checked
/// changes, decoded from the JSON the desktop sends. Either way one revision and one undo step.
fn dialog_apply(e: &mut Engine, changes: Option<&[SyncChange]>) -> CommandResponse {
    let inner = match changes {
        Some(c) => serde_json::json!({"type": "SyncSchematicToPcbSelected", "changes": c}),
        None => serde_json::json!({"type": "SyncSchematicToPcb"}),
    };
    let label = "Update PCB from schematic";
    let batch = serde_json::json!({"type": "Batch", "commands": [inner], "label": label});
    run(e, serde_json::from_value(batch).unwrap())
}

#[test]
fn the_dialogs_partial_apply_is_one_revision_and_one_undo() {
    let mut e = schematic();
    let all = preview(&e);
    let before = e.project().clone();
    // Check two footprints to add (without their pad nets), leave the rest unchecked.
    let adds: Vec<SyncChange> = all
        .iter()
        .filter(|c| matches!(c, SyncChange::AddFootprint { .. }))
        .take(2)
        .cloned()
        .collect();
    assert_eq!(adds.len(), 2, "{all:?}");
    let rev = e.project().revision;
    let r = dialog_apply(&mut e, Some(&adds));
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(e.project().revision, rev + 1, "one revision");
    assert_eq!(e.project().pcb.footprints.len(), 2);
    let fps = &e.project().pcb.footprints;
    assert!(fps.values().all(|f| f.pad_nets.is_empty()));
    // What was left unchecked is what the next preview lists.
    let rest = preview(&e);
    assert_eq!(rest.len(), all.len() - 2);
    assert!(adds.iter().all(|a| !rest.contains(a)));
    // One undo restores the project as it was before Apply.
    assert!(e.undo(rev + 1, &Actor::human()).accepted);
    assert_eq!(by_symbol(e.project()), by_symbol(&before));
    assert_eq!(preview(&e), all);
}

#[test]
fn the_dialogs_full_apply_is_one_revision_and_one_undo() {
    let mut e = schematic();
    let all = preview(&e);
    let rev = e.project().revision;
    let r = dialog_apply(&mut e, None);
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(e.project().revision, rev + 1, "one revision");
    assert!(preview(&e).is_empty(), "nothing left to sync");
    assert!(e.undo(rev + 1, &Actor::human()).accepted);
    assert!(e.project().pcb.footprints.is_empty());
    assert_eq!(preview(&e), all);
}
