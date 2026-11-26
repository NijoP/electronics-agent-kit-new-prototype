//! MECH-001 bottom-side height limit and MECH-004 enclosure mounting holes (ADR-0055 inputs):
//! PASS, FAIL and UNKNOWN.

use eak_core::geom::mm;
use eak_core::{CopperLayer, Enclosure, MountingHoleSpec, Point, Project, Side, Track, Via};
use eak_engineering::scene3d::body_rect;
use eak_runtime::{fixtures, Engine};
use eak_validation::{gates, mech, CheckState, Finding, GateState, Report};

fn on<'a>(r: &'a Report, rule: &str) -> Vec<&'a Finding> {
    r.findings.iter().filter(|f| f.rule_id == rule).collect()
}

fn messages(r: &Report, rule: &str) -> Vec<String> {
    on(r, rule).iter().map(|f| f.message.clone()).collect()
}

fn gate(p: &Project) -> GateState {
    let r = gates::evaluate(p, None);
    let g = r.gates.iter().find(|g| g.id == "mechanical_fit");
    g.unwrap().state
}

fn hole(x: f64, y: f64, drill: f64) -> MountingHoleSpec {
    MountingHoleSpec {
        position: Point::mm(x, y),
        drill: mm(drill),
        tolerance: mm(0.1),
        keepout_radius: mm(4.0),
    }
}

/// A board with one 3.2 mm via at (50, 50) mm as the mounting hole and the given declared holes.
fn board(specs: Vec<MountingHoleSpec>) -> Project {
    let mut p = Project::new("mech");
    let via = Via {
        id: "mh".into(),
        position: Point::mm(50.0, 50.0),
        drill: mm(3.2),
        diameter: mm(6.0),
        net: "GND".into(),
        from: CopperLayer::Top,
        to: CopperLayer::Bottom,
    };
    p.pcb.vias.insert("mh".into(), via);
    p.constraints.enclosure = Some(Enclosure {
        max_height_bottom: None,
        mounting_holes: specs,
    });
    p
}

#[test]
fn mounting_holes_pass_fail_and_unknown() {
    // No enclosure declared: UNKNOWN, never a pass.
    let mut p = board(vec![]);
    p.constraints.enclosure = None;
    let r = mech::run(&p);
    assert_eq!(r.state_of("MECH-004"), CheckState::Unknown);
    assert_eq!(gate(&p), GateState::Unknown);
    // An enclosure with no holes needs nothing.
    let r = mech::run(&board(vec![]));
    assert_eq!(r.state_of("MECH-004"), CheckState::Pass);
    // The via sits on the declared hole with the declared drill; its own copper is allowed.
    let r = mech::run(&board(vec![hole(50.05, 50.0, 3.2)]));
    assert_eq!(r.state_of("MECH-004"), CheckState::Pass);
    // Too small a drill fails with the measured and required drill.
    let r = mech::run(&board(vec![hole(50.0, 50.0, 3.5)]));
    let small = on(&r, "MECH-004");
    assert_eq!(small.len(), 1, "{small:?}");
    assert!((small[0].measured.as_ref().unwrap().value - 3.2).abs() < 1e-9);
    assert!((small[0].required.as_ref().unwrap().value - 3.5).abs() < 1e-9);
    // No hole at the declared position (0.2 mm off is outside the 0.1 mm tolerance).
    let r = mech::run(&board(vec![hole(50.2, 50.0, 3.2)]));
    let missing = messages(&r, "MECH-004");
    let none = missing.iter().any(|m| m.starts_with("no drilled hole"));
    assert!(none, "{missing:?}");
    assert_eq!(r.state_of("MECH-004"), CheckState::Fail);
    let p = board(vec![hole(50.2, 50.0, 3.2)]);
    assert_eq!(gate(&p), GateState::Fail);
}

#[test]
fn copper_inside_a_mounting_hole_keep_out_fails() {
    let mut p = board(vec![hole(50.0, 50.0, 3.2)]);
    let track = |id: &str, y: f64| Track {
        id: id.into(),
        layer: CopperLayer::Top,
        width: mm(0.25),
        a: Point::mm(40.0, y),
        b: Point::mm(60.0, y),
        net: "SIG".into(),
        arc_deg: None,
    };
    // 3 mm from the centre is inside the 4 mm keep-out; 6 mm is outside.
    p.pcb.tracks.insert("near".into(), track("near", 53.0));
    p.pcb.tracks.insert("far".into(), track("far", 56.0));
    let r = mech::run(&p);
    let hits = messages(&r, "MECH-004");
    let want = "track near is inside the keep-out of mounting hole 1 at (50.00, 50.00) mm";
    assert_eq!(hits, vec![want]);
}

#[test]
fn bottom_side_parts_use_the_enclosure_bottom_limit() {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).unwrap();
    fixtures::build_pcb(&mut e).unwrap();
    let mut p = e.project().clone();
    p.constraints.max_component_height = Some(mm(25.0));
    let r = mech::run(&p);
    assert!(messages(&r, "MECH-001").is_empty());
    // One part flipped to the bottom, where the enclosure allows only 0.1 mm.
    let has_body = |id: &&String| body_rect(&p, id).is_some();
    let id = p.pcb.footprints.keys().find(has_body).cloned().unwrap();
    let flipped = p.pcb.footprints.get_mut(&id).unwrap();
    flipped.side = Side::Bottom;
    let refdes = flipped.refdes.clone();
    p.constraints.enclosure = Some(Enclosure {
        max_height_bottom: Some(mm(0.1)),
        mounting_holes: vec![],
    });
    let r = mech::run(&p);
    let tall = messages(&r, "MECH-001");
    assert_eq!(tall.len(), 1, "{tall:?}");
    assert!(tall[0].starts_with(&format!("{refdes} is")), "{tall:?}");
}
