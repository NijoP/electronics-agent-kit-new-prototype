//! `Command::Batch` contract (ADR-0013): atomic, one revision, one undo step, class of the
//! riskiest step, no nesting, survives reopen as a single history entry.

use eak_core::{Point, Project};
use eak_runtime::command::{Command as C, MAX_BATCH};
use eak_runtime::engine::*;
use eak_runtime::{CommandError, OperationClass};

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

fn mv(id: &str, x: f64, y: f64) -> C {
    C::MoveSymbol {
        symbol_id: id.into(),
        position: Point::mm(x, y),
    }
}

fn batch(commands: Vec<C>) -> C {
    C::Batch {
        commands,
        label: Some("test".into()),
    }
}

#[test]
fn batch_is_one_revision_and_one_undo_step() {
    let mut e = Engine::in_memory(Project::new("x"));
    let r1 = place(&mut e, "R_0603", 0.0, 0.0);
    let r2 = place(&mut e, "R_0603", 10.0, 0.0);
    let before = e.project().clone();
    let r = run(
        &mut e,
        Actor::human(),
        batch(vec![
            mv(&r1, 5.0, 5.0),
            mv(&r2, 15.0, 5.0),
            C::AddWire {
                sheet_id: None,
                a: Point::mm(0.0, 20.0),
                b: Point::mm(10.0, 20.0),
            },
        ]),
    );
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(r.command, "Batch");
    assert_eq!(r.revision_after, before.revision + 1);
    assert_eq!(r.created.len(), 1, "the wire id is reported");
    assert_eq!(
        e.project().schematic.symbols[&r1].position,
        Point::mm(5.0, 5.0)
    );
    assert_eq!(e.project().schematic.wires.len(), 1);
    let after = e.project().clone();

    // One undo reverts every step.
    let u = e.undo(e.project().revision, &Actor::human());
    assert!(u.accepted, "{:?}", u.error);
    let mut expect = before.clone();
    expect.revision = e.project().revision;
    assert_eq!(e.project(), &expect);
    // And one redo re-applies every step.
    let rd = e.redo(e.project().revision, &Actor::human());
    assert!(rd.accepted, "{:?}", rd.error);
    let mut expect = after;
    expect.revision = e.project().revision;
    assert_eq!(e.project(), &expect);
}

#[test]
fn failing_step_rejects_the_whole_batch() {
    let mut e = Engine::in_memory(Project::new("x"));
    let r1 = place(&mut e, "R_0603", 0.0, 0.0);
    let before = e.project().clone();
    let r = run(
        &mut e,
        Actor::human(),
        batch(vec![mv(&r1, 5.0, 5.0), mv("no-such-symbol", 1.0, 1.0)]),
    );
    assert!(!r.accepted);
    match &r.error {
        Some(CommandError::NotFound(m)) => {
            assert!(m.contains("batch step 2 (MoveSymbol)"), "{m}")
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(e.project(), &before, "first step must not leak");
    let u = e.undo(e.project().revision, &Actor::human());
    assert!(u.accepted);
    assert!(
        e.project().schematic.symbols.is_empty(),
        "undo hit the place, not the rejected batch"
    );
}

#[test]
fn empty_nested_and_oversized_batches_are_invalid() {
    let mut e = Engine::in_memory(Project::new("x"));
    let r1 = place(&mut e, "R_0603", 0.0, 0.0);
    let r = run(&mut e, Actor::human(), batch(vec![]));
    assert!(
        matches!(r.error, Some(CommandError::Invalid(_))),
        "{:?}",
        r.error
    );
    let r = run(
        &mut e,
        Actor::human(),
        batch(vec![mv(&r1, 1.0, 1.0), batch(vec![mv(&r1, 2.0, 2.0)])]),
    );
    match &r.error {
        Some(CommandError::Invalid(m)) => assert!(m.contains("do not nest"), "{m}"),
        other => panic!("unexpected {other:?}"),
    }
    let many = (0..=MAX_BATCH).map(|i| mv(&r1, i as f64, 0.0)).collect();
    let r = run(&mut e, Actor::human(), batch(many));
    assert!(
        matches!(r.error, Some(CommandError::Invalid(_))),
        "{:?}",
        r.error
    );
    assert_eq!(e.project().revision, 1);
}

#[test]
fn batch_class_is_its_riskiest_step_and_gates_agent_approval() {
    let low = batch(vec![mv("a", 0.0, 0.0)]);
    let high = batch(vec![
        mv("a", 0.0, 0.0),
        C::DeleteSymbol {
            symbol_id: "a".into(),
        },
    ]);
    assert_eq!(low.class(), OperationClass::ReversibleLocal);
    assert_eq!(high.class(), OperationClass::Destructive);

    let mut e = Engine::in_memory(Project::new("x"));
    let r1 = place(&mut e, "R_0603", 0.0, 0.0);
    let cmd = batch(vec![
        mv(&r1, 3.0, 3.0),
        C::DeleteSymbol {
            symbol_id: r1.clone(),
        },
    ]);
    let approve = |max_class| Approval {
        approved_by: "user".into(),
        max_class,
        note: String::new(),
    };
    let r = e.execute(CommandRequest {
        expected_revision: 1,
        actor: Actor::agent("lead"),
        command: cmd.clone(),
        approval: Some(approve(OperationClass::EngineeringImpacting)),
    });
    assert!(matches!(r.error, Some(CommandError::ApprovalRequired(_))));
    let r = e.execute(CommandRequest {
        expected_revision: 1,
        actor: Actor::agent("lead"),
        command: cmd,
        approval: Some(approve(OperationClass::Destructive)),
    });
    assert!(r.accepted, "{:?}", r.error);
    assert!(e.project().schematic.symbols.is_empty());
}

#[test]
fn batch_round_trips_through_json_with_the_type_tag() {
    let c = batch(vec![mv("s", 1.0, 2.0), C::annotate()]);
    let v = serde_json::to_value(&c).unwrap();
    assert_eq!(v["type"], "Batch");
    assert_eq!(v["commands"][1]["type"], "Annotate");
    let back: C = serde_json::from_value(v).unwrap();
    assert_eq!(back, c);
    // `label` may be omitted.
    let bare: C =
        serde_json::from_str(r#"{"type":"Batch","commands":[{"type":"Annotate"}]}"#).unwrap();
    assert_eq!(
        bare,
        C::Batch {
            commands: vec![C::annotate()],
            label: None
        }
    );
}

#[test]
fn persisted_batch_reopens_as_one_undo_step() {
    let dir = std::env::temp_dir().join(format!("eak-test-batch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (r1, r2);
    {
        let mut e = Engine::create(&dir, "Board").unwrap();
        r1 = place(&mut e, "R_0603", 0.0, 0.0);
        r2 = place(&mut e, "C_0603", 10.0, 0.0);
        let r = run(
            &mut e,
            Actor::human(),
            batch(vec![mv(&r1, 1.0, 1.0), mv(&r2, 11.0, 1.0)]),
        );
        assert!(r.accepted, "{:?}", r.error);
    }
    let mut e = Engine::open(&dir, false).unwrap();
    assert_eq!(e.project().revision, 3);
    let u = e.undo(3, &Actor::human());
    assert!(u.accepted, "{:?}", u.error);
    let s = &e.project().schematic.symbols;
    assert_eq!(s[&r1].position, Point::mm(0.0, 0.0));
    assert_eq!(s[&r2].position, Point::mm(10.0, 0.0));
    drop(e);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Property: for any sequence of batches of moves, undoing once per accepted batch restores
/// the original design exactly (deterministic xorshift-generated cases).
#[test]
fn undoing_every_batch_restores_the_original() {
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut rnd = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    for case in 0..20 {
        let mut e = Engine::in_memory(Project::new("x"));
        let ids: Vec<String> = (0..4)
            .map(|i| place(&mut e, "R_0603", i as f64 * 10.0, 0.0))
            .collect();
        let original = e.project().clone();
        let mut accepted = 0;
        for _ in 0..(1 + rnd(6)) {
            let steps = (0..(1 + rnd(4)))
                .map(|_| {
                    let id = &ids[rnd(ids.len() as u64) as usize];
                    mv(id, rnd(100) as f64, rnd(100) as f64)
                })
                .collect();
            if run(&mut e, Actor::human(), batch(steps)).accepted {
                accepted += 1;
            }
        }
        for _ in 0..accepted {
            let u = e.undo(e.project().revision, &Actor::human());
            assert!(u.accepted, "case {case}: {:?}", u.error);
        }
        let mut expect = original.clone();
        expect.revision = e.project().revision;
        assert_eq!(e.project(), &expect, "case {case}");
    }
}
