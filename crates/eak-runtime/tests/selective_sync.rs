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
    let mut a = schematic();
    let mut b = schematic();
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
    assert!(run(&mut b, C::SyncSchematicToPcb).accepted);
    assert_eq!(by_symbol(a.project()), by_symbol(b.project()));
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
    let s = e
        .project()
        .schematic
        .symbols
        .values()
        .next()
        .unwrap()
        .clone();
    assert!(
        run(
            &mut e,
            C::SetRefdes {
                symbol_id: s.id.clone(),
                refdes: "X99".into(),
            }
        )
        .accepted
    );
    let pending = preview(&e);
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
    assert_eq!(by_symbol(e.project())[&s.id].0, "X99");
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
