//! Phase 1 C5 (ADR-0045): every clearance-matrix cell DRC enforces is enforced by a test here.
//! Two objects of the cell's classes (or one object and the board outline) sit well apart for
//! every default rule; the rule enforcing the cell is silent without the cell and fires once the
//! cell asks for more than that distance. A configured cell no rule enforces is reported (DRC-002
//! UNKNOWN), never assumed met.

use eak_core::{CopperLayer, ObjectClass, Point, Project, Rotation};
use eak_runtime::{Actor, Command as C, CommandRequest, CommandResponse, Engine};
use eak_validation::drc::{self, matrix_cell_rule, unenforced_cells};
use eak_validation::CheckState;

fn try_run(e: &mut Engine, command: C) -> CommandResponse {
    let expected_revision = e.project().revision;
    e.execute(CommandRequest {
        expected_revision,
        actor: Actor::human(),
        command,
        approval: None,
    })
}

fn run(e: &mut Engine, command: C) -> CommandResponse {
    let name = command.name();
    let r = try_run(e, command);
    assert!(r.accepted, "{name}: {:?}", r.error);
    r
}

/// A 100 mm x 100 mm board with nothing on it.
fn board() -> Engine {
    let mut e = Engine::in_memory(Project::new("matrix"));
    run(
        &mut e,
        C::SetBoardOutline {
            points: vec![
                Point::mm(0.0, 0.0),
                Point::mm(100.0, 0.0),
                Point::mm(100.0, 100.0),
                Point::mm(0.0, 100.0),
            ],
        },
    );
    e
}

/// Place a part and move its footprint to `at`.
fn footprint(e: &mut Engine, component: &str, at: Point) {
    let x = 20.0 + e.project().schematic.symbols.len() as f64 * 20.0;
    run(
        e,
        C::PlaceSymbol {
            sheet_id: None,
            component_id: component.into(),
            position: Point::mm(x, 50.0),
            rotation: Rotation::R0,
            refdes: None,
            value: None,
        },
    );
    let before: Vec<String> = e.project().pcb.footprints.keys().cloned().collect();
    run(e, C::SyncSchematicToPcb);
    let id = e
        .project()
        .pcb
        .footprints
        .keys()
        .find(|k| !before.contains(k))
        .expect("sync placed the new footprint")
        .clone();
    run(
        e,
        C::MoveFootprint {
            footprint_id: id,
            position: at,
            rotation: None,
            side: None,
        },
    );
}

/// One object of `class` at `at` on `net` (copper). A hole is a via's drill; silkscreen is the
/// legend of an SMD resistor.
fn add(e: &mut Engine, class: ObjectClass, at: Point, net: &str) {
    match class {
        ObjectClass::Track => {
            run(
                e,
                C::AddTrack {
                    layer: CopperLayer::Top,
                    a: Point {
                        x: at.x,
                        y: at.y - 1_000_000,
                    },
                    b: Point {
                        x: at.x,
                        y: at.y + 1_000_000,
                    },
                    width: Some(250_000),
                    net: net.into(),
                    arc_deg: None,
                },
            );
        }
        ObjectClass::Via | ObjectClass::Hole => {
            run(
                e,
                C::AddVia {
                    position: at,
                    net: net.into(),
                    drill: None,
                    diameter: None,
                    from: None,
                    to: None,
                },
            );
        }
        ObjectClass::SmdPad | ObjectClass::Silk => footprint(e, "R_0603", at),
        ObjectClass::ThPad => footprint(e, "CONN_1x02_P2.54", at),
        ObjectClass::Zone | ObjectClass::Outline => unreachable!("not built by this test"),
    }
}

fn fires(e: &Engine, rule: &str) -> Vec<String> {
    drc::run(e.project())
        .findings
        .into_iter()
        .filter(|f| f.rule_id == rule)
        .map(|f| f.message)
        .collect()
}

fn set_cell(e: &mut Engine, a: ObjectClass, b: ObjectClass, mm: f64) {
    run(
        e,
        C::SetClearanceCell {
            a,
            b,
            clearance: Some((mm * 1e6) as i64),
        },
    );
}

/// The 20 enforced cells, each with its rule.
#[test]
fn every_enforced_cell_is_enforced_by_its_rule() {
    use ObjectClass::*;
    let all = ObjectClass::ALL;
    let mut checked = 0;
    for (i, a) in all.iter().enumerate() {
        for b in &all[i..] {
            let Some(rule) = matrix_cell_rule(*a, *b) else {
                continue;
            };
            let mut e = board();
            let cell_mm = if *a == Outline || *b == Outline {
                // One object 8 mm inside the left edge: far beyond the default edge clearance.
                let k = if *a == Outline { *b } else { *a };
                add(&mut e, k, Point::mm(8.0, 50.0), "A");
                if k == Silk {
                    60.0
                } else {
                    15.0
                }
            } else {
                // Two objects 8 mm apart in the middle of the board, on different nets.
                add(&mut e, *a, Point::mm(46.0, 50.0), "A");
                add(&mut e, *b, Point::mm(54.0, 50.0), "B");
                12.0
            };
            assert!(
                fires(&e, rule).is_empty(),
                "{a:?}/{b:?}: {rule} fired without the cell: {:?}",
                fires(&e, rule)
            );
            set_cell(&mut e, *a, *b, cell_mm);
            let hits = fires(&e, rule);
            assert!(
                !hits.is_empty(),
                "{a:?}/{b:?}: {rule} did not enforce the cell"
            );
            if rule == "DRC-002" {
                assert!(
                    hits.iter().any(|m| m.contains("clearance matrix")),
                    "{a:?}/{b:?}: {hits:?}"
                );
            }
            // The pair is unordered, and clearing the cell restores the default rule.
            run(
                &mut e,
                C::SetClearanceCell {
                    a: *b,
                    b: *a,
                    clearance: None,
                },
            );
            assert!(e.project().constraints.clearance_matrix.is_empty());
            assert!(fires(&e, rule).is_empty());
            checked += 1;
        }
    }
    assert_eq!(checked, 20);
    assert_eq!(unenforced_cells().len(), 36 - 20);
}

#[test]
fn a_net_pair_rule_raises_drc_002_between_those_nets_only() {
    let mut e = board();
    add(&mut e, ObjectClass::Track, Point::mm(46.0, 50.0), "A");
    add(&mut e, ObjectClass::Track, Point::mm(54.0, 50.0), "B");
    add(&mut e, ObjectClass::Track, Point::mm(46.0, 80.0), "C");
    add(&mut e, ObjectClass::Track, Point::mm(54.0, 80.0), "D");
    assert!(fires(&e, "DRC-002").is_empty());
    run(
        &mut e,
        C::SetNetPairClearance {
            net_a: "B".into(),
            net_b: "A".into(),
            clearance: Some(12_000_000),
        },
    );
    let hits = fires(&e, "DRC-002");
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].contains("net pair rule A / B"), "{hits:?}");
    assert_eq!(e.project().constraints.net_pair_rules[0].net_a, "A");
}

#[test]
fn a_configured_cell_no_rule_enforces_is_reported_never_assumed() {
    // No copper, so DRC-002 is not UNKNOWN for another reason (undeclared voltages).
    let mut e = board();
    let r = drc::run(e.project());
    assert!(r
        .not_implemented
        .iter()
        .any(|n| n.contains("Clearance-matrix cells not enforced") && n.contains("track / zone")));
    assert_ne!(r.state_of("DRC-002"), CheckState::Unknown);
    set_cell(&mut e, ObjectClass::Zone, ObjectClass::Track, 1.0);
    let r = drc::run(e.project());
    assert_eq!(r.state_of("DRC-002"), CheckState::Unknown);
    let rr = r
        .rule_results
        .iter()
        .find(|x| x.rule_id == "DRC-002")
        .unwrap();
    assert!(rr.reason.contains("track / zone"), "{}", rr.reason);
}

#[test]
fn invalid_cells_and_pairs_are_refused() {
    let mut e = board();
    let bad = [
        C::SetClearanceCell {
            a: ObjectClass::Track,
            b: ObjectClass::Via,
            clearance: Some(0),
        },
        C::SetNetPairClearance {
            net_a: "A".into(),
            net_b: "A".into(),
            clearance: Some(1),
        },
        C::SetNetPairClearance {
            net_a: "".into(),
            net_b: "A".into(),
            clearance: Some(1),
        },
    ];
    for c in bad {
        assert!(!try_run(&mut e, c).accepted);
    }
    assert!(e.project().constraints.clearance_matrix.is_empty());
    assert!(e.project().constraints.net_pair_rules.is_empty());
}
