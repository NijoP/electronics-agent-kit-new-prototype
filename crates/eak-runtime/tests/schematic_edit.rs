//! Schematic editing commands used by the select tool: `MoveWire`, `MoveNetLabel`, and a
//! drag-move submitted as one `Batch` that rubber-bands the attached wire (connectivity kept,
//! no stale-wire warning, one undo step).

use eak_core::{LabelKind, Point, Project};
use eak_engineering::netlist;
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
    let r = run(e, cmd);
    assert!(r.accepted, "{:?}", r.error);
    r
}

fn place(e: &mut Engine, x: f64, y: f64) -> String {
    ok(
        e,
        C::PlaceSymbol {
            sheet_id: None,
            component_id: "R_0603".into(),
            position: Point::mm(x, y),
            rotation: Default::default(),
            refdes: None,
            value: None,
        },
    )
    .created[0]
        .clone()
}

fn pin(e: &Engine, sym: &str, n: &str) -> Point {
    let p = e.project();
    p.pin_position(&p.schematic.symbols[sym], n).unwrap()
}

fn same_net(e: &Engine, a: (&str, &str), b: (&str, &str)) -> bool {
    let nl = netlist::build(e.project());
    let net = |s: &str, n: &str| nl.pin_net.get(&(s.to_string(), n.to_string())).cloned();
    net(a.0, a.1).is_some() && net(a.0, a.1) == net(b.0, b.1)
}

/// R1 and R2 side by side, R1 pin 2 wired to R2 pin 1.
fn wired() -> (Engine, String, String, String) {
    let mut e = Engine::in_memory(Project::new("x"));
    let r1 = place(&mut e, 0.0, 0.0);
    let r2 = place(&mut e, 20.0, 0.0);
    let (a, b) = (pin(&e, &r1, "2"), pin(&e, &r2, "1"));
    let w = ok(
        &mut e,
        C::AddWire {
            sheet_id: None,
            a,
            b,
        },
    )
    .created[0]
        .clone();
    assert!(same_net(&e, (&r1, "2"), (&r2, "1")));
    (e, r1, r2, w)
}

#[test]
fn move_wire_sets_both_ends_and_refuses_bad_input() {
    let (mut e, _, _, w) = wired();
    let (a, b) = (Point::mm(0.0, 10.0), Point::mm(10.0, 10.0));
    ok(
        &mut e,
        C::MoveWire {
            wire_id: w.clone(),
            a,
            b,
        },
    );
    let wire = &e.project().schematic.wires[&w];
    assert_eq!((wire.a, wire.b), (a, b));
    let r = run(
        &mut e,
        C::MoveWire {
            wire_id: w.clone(),
            a,
            b,
        },
    );
    assert!(matches!(r.error, Some(CommandError::NothingTo(_))));
    let r = run(
        &mut e,
        C::MoveWire {
            wire_id: w.clone(),
            a,
            b: a,
        },
    );
    assert!(matches!(r.error, Some(CommandError::Invalid(_))));
    let r = run(
        &mut e,
        C::MoveWire {
            wire_id: "nope".into(),
            a,
            b,
        },
    );
    assert!(matches!(r.error, Some(CommandError::NotFound(_))));
}

#[test]
fn move_net_label_moves_and_names_the_net_it_touches() {
    let (mut e, r1, r2, _) = wired();
    let l = ok(
        &mut e,
        C::AddNetLabel {
            sheet_id: None,
            name: "SIG".into(),
            position: Point::mm(50.0, 50.0),
            kind: LabelKind::Net,
        },
    )
    .created[0]
        .clone();
    let net_of = |e: &Engine| {
        netlist::build(e.project())
            .pin_net
            .get(&(r1.clone(), "2".to_string()))
            .cloned()
    };
    assert_ne!(net_of(&e).as_deref(), Some("SIG"));
    let on_pin = pin(&e, &r2, "1");
    ok(
        &mut e,
        C::MoveNetLabel {
            label_id: l.clone(),
            position: on_pin,
        },
    );
    assert_eq!(net_of(&e).as_deref(), Some("SIG"));
    let r = run(
        &mut e,
        C::MoveNetLabel {
            label_id: l,
            position: on_pin,
        },
    );
    assert!(matches!(r.error, Some(CommandError::NothingTo(_))));
}

#[test]
fn drag_move_batch_keeps_connectivity_and_is_one_undo_step() {
    let (mut e, r1, r2, w) = wired();
    let before = e.project().clone();
    let d = Point::mm(0.0, 10.16);
    let new_end = pin(&e, &r2, "1").plus(d);
    let start = pin(&e, &r1, "2");
    let to = e.project().schematic.symbols[&r2].position.plus(d);
    let r = ok(
        &mut e,
        C::Batch {
            commands: vec![
                C::MoveWire {
                    wire_id: w.clone(),
                    a: start,
                    b: new_end,
                },
                C::MoveSymbol {
                    symbol_id: r2.clone(),
                    position: to,
                },
            ],
            label: Some("Move 1 object".into()),
        },
    );
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(pin(&e, &r2, "1"), new_end);
    assert!(same_net(&e, (&r1, "2"), (&r2, "1")));
    let u = e.undo(e.project().revision, &Actor::human());
    assert!(u.accepted);
    let mut expect = before;
    expect.revision = e.project().revision;
    assert_eq!(e.project(), &expect);
}

#[test]
fn move_symbol_alone_warns_only_when_it_leaves_wire_ends_behind() {
    let (mut e, r1, r2, _) = wired();
    let r = ok(
        &mut e,
        C::MoveSymbol {
            symbol_id: r2.clone(),
            position: Point::mm(20.0, 30.0),
        },
    );
    assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
    assert!(r.warnings[0].contains("1 wire end(s)"), "{:?}", r.warnings);
    assert!(!same_net(&e, (&r1, "2"), (&r2, "1")));
    // A symbol with nothing attached moves silently.
    let lone = place(&mut e, 100.0, 100.0);
    let r = ok(
        &mut e,
        C::MoveSymbol {
            symbol_id: lone,
            position: Point::mm(110.0, 100.0),
        },
    );
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

/// Flip (ADR-0016): every pin mirrors about the sheet axis through the symbol origin, two
/// flips restore the original, a flip is one undo step, and unmirrored projects keep their
/// on-disk form (no `mirror` key).
#[test]
fn flip_symbol_mirrors_pins_about_the_origin_and_round_trips() {
    use eak_core::FlipAxis;
    let mut e = Engine::in_memory(Project::new("x"));
    let r = place(&mut e, 10.0, 20.0);
    let o = e.project().schematic.symbols[&r].position;
    let mirror_x = |p: Point| Point::new(2 * o.x - p.x, p.y);
    let mirror_y = |p: Point| Point::new(p.x, 2 * o.y - p.y);
    let json = serde_json::to_string(e.project()).unwrap();
    assert!(
        !json.contains("\"mirror\""),
        "unmirrored symbols must not gain a key"
    );
    for rot in ["R0", "R90", "R180", "R270"] {
        let rotation = serde_json::from_value(serde_json::json!(rot)).unwrap();
        if e.project().schematic.symbols[&r].rotation != rotation {
            ok(
                &mut e,
                C::RotateSymbol {
                    symbol_id: r.clone(),
                    rotation,
                },
            );
        }
        for (axis, f) in [
            (FlipAxis::Horizontal, &mirror_x as &dyn Fn(Point) -> Point),
            (FlipAxis::Vertical, &mirror_y),
        ] {
            let (p1, p2) = (pin(&e, &r, "1"), pin(&e, &r, "2"));
            ok(
                &mut e,
                C::FlipSymbol {
                    symbol_id: r.clone(),
                    axis,
                },
            );
            assert_eq!(
                (pin(&e, &r, "1"), pin(&e, &r, "2")),
                (f(p1), f(p2)),
                "{rot} {axis:?}"
            );
            assert!(e.project().schematic.symbols[&r].mirror);
            let back = serde_json::to_string(e.project()).unwrap();
            let loaded: Project = serde_json::from_str(&back).unwrap();
            assert_eq!(&loaded, e.project(), "mirror survives a save/load");
            ok(
                &mut e,
                C::FlipSymbol {
                    symbol_id: r.clone(),
                    axis,
                },
            );
            assert_eq!(
                (pin(&e, &r, "1"), pin(&e, &r, "2")),
                (p1, p2),
                "{rot} {axis:?} twice"
            );
            assert!(!e.project().schematic.symbols[&r].mirror);
        }
    }
    let before = e.project().clone();
    ok(
        &mut e,
        C::FlipSymbol {
            symbol_id: r.clone(),
            axis: FlipAxis::Horizontal,
        },
    );
    assert!(e.undo(e.project().revision, &Actor::human()).accepted);
    let mut expect = before;
    expect.revision = e.project().revision;
    assert_eq!(e.project(), &expect);
    let missing = run(
        &mut e,
        C::FlipSymbol {
            symbol_id: "nope".into(),
            axis: FlipAxis::Vertical,
        },
    );
    assert!(!missing.accepted);
}
