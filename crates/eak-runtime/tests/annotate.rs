//! Annotate options (Phase 1 B5, ADR-0046): order (across / down within the sheet order), scope
//! (unannotated / all / selection) and reset. Designators outside the scope never change and are
//! never reused; `{"type":"Annotate"}` keeps its old meaning.

use eak_core::{Point, Project, Rotation};
use eak_runtime::{
    Actor, AnnotateOrder, AnnotateScope, Command as C, CommandRequest, CommandResponse, Engine,
};

fn run(e: &mut Engine, command: C) -> CommandResponse {
    let expected_revision = e.project().revision;
    e.execute(CommandRequest {
        expected_revision,
        actor: Actor::human(),
        command,
        approval: None,
    })
}

fn ok(e: &mut Engine, command: C) {
    let r = run(e, command);
    assert!(r.accepted, "{:?}", r.error);
}

fn place(e: &mut Engine, x: f64, y: f64) -> String {
    let r = run(
        e,
        C::PlaceSymbol {
            sheet_id: None,
            component_id: "R_0603".into(),
            position: Point::mm(x, y),
            rotation: Rotation::R0,
            refdes: None,
            value: None,
        },
    );
    assert!(r.accepted, "{:?}", r.error);
    r.created[0].clone()
}

fn refdes(e: &Engine, id: &str) -> String {
    e.project().schematic.symbols[id].refdes.clone()
}

fn annotate(order: AnnotateOrder, scope: AnnotateScope, symbols: &[&str], reset: bool) -> C {
    C::Annotate {
        order,
        scope,
        symbols: symbols.iter().map(|s| s.to_string()).collect(),
        reset,
    }
}

/// A at (60, 10), B at (10, 30), C at (30, 10): placed as R1, R2, R3.
fn three() -> (Engine, String, String, String) {
    let mut e = Engine::in_memory(Project::new("annotate"));
    let a = place(&mut e, 60.0, 10.0);
    let b = place(&mut e, 10.0, 30.0);
    let c = place(&mut e, 30.0, 10.0);
    assert_eq!(
        (refdes(&e, &a), refdes(&e, &b), refdes(&e, &c)),
        ("R1".into(), "R2".into(), "R3".into())
    );
    (e, a, b, c)
}

#[test]
fn all_across_numbers_by_y_then_x_and_down_by_x_then_y() {
    let (mut e, a, b, c) = three();
    ok(
        &mut e,
        annotate(AnnotateOrder::Across, AnnotateScope::All, &[], false),
    );
    assert_eq!(
        (refdes(&e, &c), refdes(&e, &a), refdes(&e, &b)),
        ("R1".into(), "R2".into(), "R3".into())
    );
    ok(
        &mut e,
        annotate(AnnotateOrder::Down, AnnotateScope::All, &[], false),
    );
    assert_eq!(
        (refdes(&e, &b), refdes(&e, &c), refdes(&e, &a)),
        ("R1".into(), "R2".into(), "R3".into())
    );
}

#[test]
fn a_selection_renumbers_only_itself_and_never_reuses_an_outside_designator() {
    let (mut e, a, b, c) = three();
    ok(
        &mut e,
        annotate(
            AnnotateOrder::Across,
            AnnotateScope::Selection,
            &[&b, &c],
            false,
        ),
    );
    assert_eq!(refdes(&e, &a), "R1", "outside the scope: unchanged");
    // B and C free R2 and R3 and take them in reading order: C (y 10) before B (y 30).
    assert_eq!((refdes(&e, &c), refdes(&e, &b)), ("R2".into(), "R3".into()));
}

#[test]
fn reset_then_the_default_annotate_fills_the_lowest_free_number() {
    let (mut e, a, b, c) = three();
    ok(
        &mut e,
        annotate(AnnotateOrder::Across, AnnotateScope::Selection, &[&b], true),
    );
    assert_eq!(refdes(&e, &b), "R?");
    assert_eq!((refdes(&e, &a), refdes(&e, &c)), ("R1".into(), "R3".into()));
    // `{"type":"Annotate"}` is the default: the unannotated symbols, across.
    let plain: C = serde_json::from_str(r#"{"type":"Annotate"}"#).unwrap();
    assert_eq!(plain, C::annotate());
    ok(&mut e, plain);
    assert_eq!(refdes(&e, &b), "R2");
    // Nothing left to annotate is refused, not a silent no-op.
    assert!(!run(&mut e, C::annotate()).accepted);
}

#[test]
fn reset_all_and_bad_selections_are_handled() {
    let (mut e, a, b, c) = three();
    ok(
        &mut e,
        annotate(AnnotateOrder::Across, AnnotateScope::All, &[], true),
    );
    for id in [&a, &b, &c] {
        assert_eq!(refdes(&e, id), "R?");
    }
    assert!(
        !run(
            &mut e,
            annotate(AnnotateOrder::Across, AnnotateScope::All, &[], true)
        )
        .accepted,
        "already reset"
    );
    assert!(
        !run(
            &mut e,
            annotate(AnnotateOrder::Across, AnnotateScope::Selection, &[], false)
        )
        .accepted,
        "a selection scope needs symbols"
    );
    assert!(
        !run(
            &mut e,
            annotate(
                AnnotateOrder::Across,
                AnnotateScope::Selection,
                &["sym_nope"],
                false
            )
        )
        .accepted
    );
}
