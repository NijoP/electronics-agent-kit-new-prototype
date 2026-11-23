//! Schema v4 PCB objects and edits (ADR-0041): every new command works through the engine
//! (accepted, refused with a reason that leaves the project untouched, undoable).

use eak_core::*;
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;
use eak_runtime::{fixtures, CommandError, OperationClass};

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
    let before = e.project().clone();
    let r = run(e, cmd);
    assert!(!r.accepted, "{name} was accepted");
    assert_eq!(
        e.project(),
        &before,
        "{name}: a refusal changed the project"
    );
    r.error.expect("refusal carries an error")
}

fn undo(e: &mut Engine) {
    let rev = e.project().revision;
    let r = e.undo(rev, &Actor::human());
    assert!(r.accepted, "undo: {:?}", r.error);
}

fn mm(x: f64, y: f64) -> Point {
    Point::mm(x, y)
}

fn board() -> Engine {
    let mut e = Engine::in_memory(Project::new("c4"));
    ok(
        &mut e,
        C::SetBoardOutline {
            points: vec![mm(0.0, 0.0), mm(40.0, 0.0), mm(40.0, 30.0), mm(0.0, 30.0)],
        },
    );
    e
}

fn track(e: &mut Engine, layer: CopperLayer, a: Point, b: Point, net: &str) -> Id {
    ok(
        e,
        C::AddTrack {
            layer,
            a,
            b,
            width: None,
            net: net.into(),
            arc_deg: None,
        },
    )
    .created[0]
        .clone()
}

#[test]
fn move_track_drags_the_ends_that_met_it_and_undo_restores_them() {
    let mut e = board();
    let left = track(&mut e, CopperLayer::Top, mm(5.0, 10.0), mm(10.0, 10.0), "N");
    let mid = track(
        &mut e,
        CopperLayer::Top,
        mm(10.0, 10.0),
        mm(20.0, 10.0),
        "N",
    );
    let right = track(
        &mut e,
        CopperLayer::Top,
        mm(20.0, 10.0),
        mm(25.0, 10.0),
        "N",
    );
    // Same point, other net or other layer: never dragged.
    let other_net = track(
        &mut e,
        CopperLayer::Top,
        mm(10.0, 10.0),
        mm(10.0, 20.0),
        "M",
    );
    let other_layer = track(
        &mut e,
        CopperLayer::Bottom,
        mm(20.0, 10.0),
        mm(20.0, 20.0),
        "N",
    );
    let before = e.project().clone();
    let r = ok(
        &mut e,
        C::MoveTrack {
            track_id: mid.clone(),
            by: mm(0.0, 2.0),
        },
    );
    assert_eq!(r.class, OperationClass::EngineeringImpacting);
    assert!(
        r.warnings.iter().any(|w| w.contains("2 connected")),
        "{:?}",
        r.warnings
    );
    let t = &e.project().pcb.tracks;
    assert_eq!((t[&mid].a, t[&mid].b), (mm(10.0, 12.0), mm(20.0, 12.0)));
    assert_eq!(t[&left].b, mm(10.0, 12.0));
    assert_eq!(t[&right].a, mm(20.0, 12.0));
    assert_eq!(t[&other_net].a, mm(10.0, 10.0));
    assert_eq!(t[&other_layer].a, mm(20.0, 10.0));
    undo(&mut e);
    assert_eq!(e.project().pcb, before.pcb);
    // Moving by exactly the segment's length moves each neighbour once, not twice.
    ok(
        &mut e,
        C::MoveTrack {
            track_id: mid.clone(),
            by: mm(10.0, 0.0),
        },
    );
    let t = &e.project().pcb.tracks;
    assert_eq!(t[&left].b, mm(20.0, 10.0));
    assert_eq!(t[&right].a, mm(30.0, 10.0));
    assert!(matches!(
        refused(
            &mut e,
            C::MoveTrack {
                track_id: mid.clone(),
                by: Point::default(),
            }
        ),
        CommandError::NothingTo(_)
    ));
    assert!(matches!(
        refused(
            &mut e,
            C::MoveTrack {
                track_id: "nope".into(),
                by: mm(1.0, 0.0),
            }
        ),
        CommandError::NotFound(_)
    ));
    // A drag that would collapse a neighbour to nothing is refused as a whole.
    let short = track(&mut e, CopperLayer::Top, mm(1.0, 1.0), mm(2.0, 1.0), "S");
    let long = track(&mut e, CopperLayer::Top, mm(2.0, 1.0), mm(8.0, 1.0), "S");
    assert!(matches!(
        refused(
            &mut e,
            C::MoveTrack {
                track_id: long,
                by: mm(-1.0, 0.0),
            }
        ),
        CommandError::Invalid(_)
    ));
    assert!(e.project().pcb.tracks.contains_key(&short));
}

#[test]
fn edit_track_changes_width_layer_ends_and_arc() {
    let mut e = board();
    let t = track(&mut e, CopperLayer::Top, mm(5.0, 5.0), mm(15.0, 5.0), "N");
    ok(
        &mut e,
        C::EditTrack {
            track_id: t.clone(),
            width: Some(500_000),
            layer: Some(CopperLayer::Bottom),
            a: None,
            b: Some(mm(15.0, 8.0)),
            arc_deg: Some(90.0),
        },
    );
    let got = e.project().pcb.tracks[&t].clone();
    assert_eq!(
        (got.width, got.layer, got.b, got.arc_deg),
        (500_000, CopperLayer::Bottom, mm(15.0, 8.0), Some(90.0))
    );
    assert!(
        got.length() > got.a.dist(got.b),
        "an arc is longer than its chord"
    );
    // arc_deg 0 makes it straight again.
    ok(
        &mut e,
        C::EditTrack {
            track_id: t.clone(),
            width: None,
            layer: None,
            a: None,
            b: None,
            arc_deg: Some(0.0),
        },
    );
    assert_eq!(e.project().pcb.tracks[&t].arc_deg, None);
    let edit = |width: Option<Nm>, layer: Option<CopperLayer>, arc_deg: Option<f64>| C::EditTrack {
        track_id: t.clone(),
        width,
        layer,
        a: None,
        b: None,
        arc_deg,
    };
    assert!(matches!(
        refused(&mut e, edit(Some(500_000), None, None)),
        CommandError::NothingTo(_)
    ));
    assert!(matches!(
        refused(&mut e, edit(Some(0), None, None)),
        CommandError::Invalid(_)
    ));
    assert!(matches!(
        refused(&mut e, edit(None, None, Some(360.0))),
        CommandError::Invalid(_)
    ));
    assert!(matches!(
        refused(&mut e, edit(None, Some(CopperLayer::Inner(1)), None)),
        CommandError::Invalid(_)
    ));
    assert!(matches!(
        refused(
            &mut e,
            C::EditTrack {
                track_id: t.clone(),
                width: None,
                layer: None,
                a: Some(mm(15.0, 8.0)),
                b: None,
                arc_deg: None,
            }
        ),
        CommandError::Invalid(_)
    ));
    // AddTrack can draw the arc directly; an invalid sweep is refused.
    let arc = ok(
        &mut e,
        C::AddTrack {
            layer: CopperLayer::Top,
            a: mm(20.0, 20.0),
            b: mm(30.0, 20.0),
            width: None,
            net: "N".into(),
            arc_deg: Some(-120.0),
        },
    )
    .created[0]
        .clone();
    assert_eq!(e.project().pcb.tracks[&arc].arc_deg, Some(-120.0));
    assert!(matches!(
        refused(
            &mut e,
            C::AddTrack {
                layer: CopperLayer::Top,
                a: mm(20.0, 20.0),
                b: mm(30.0, 20.0),
                width: None,
                net: "N".into(),
                arc_deg: Some(400.0),
            }
        ),
        CommandError::Invalid(_)
    ));
}

#[test]
fn move_via_drags_track_ends_on_the_layers_it_spans() {
    let mut e = board();
    ok(
        &mut e,
        C::SetLayerCount {
            count: 4,
            board_thickness: None,
        },
    );
    let via = ok(
        &mut e,
        C::AddVia {
            position: mm(10.0, 10.0),
            net: "N".into(),
            drill: None,
            diameter: None,
            from: Some(CopperLayer::Top),
            to: Some(CopperLayer::Inner(1)),
        },
    )
    .created[0]
        .clone();
    let top = track(&mut e, CopperLayer::Top, mm(5.0, 10.0), mm(10.0, 10.0), "N");
    let inner = track(
        &mut e,
        CopperLayer::Inner(1),
        mm(10.0, 10.0),
        mm(10.0, 20.0),
        "N",
    );
    // The blind via does not reach the bottom layer: that track end stays.
    let bottom = track(
        &mut e,
        CopperLayer::Bottom,
        mm(10.0, 10.0),
        mm(20.0, 10.0),
        "N",
    );
    let r = ok(
        &mut e,
        C::MoveVia {
            via_id: via.clone(),
            position: mm(12.0, 12.0),
        },
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("2 track end")),
        "{:?}",
        r.warnings
    );
    let p = e.project();
    assert_eq!(p.pcb.vias[&via].position, mm(12.0, 12.0));
    assert_eq!(p.pcb.tracks[&top].b, mm(12.0, 12.0));
    assert_eq!(p.pcb.tracks[&inner].a, mm(12.0, 12.0));
    assert_eq!(p.pcb.tracks[&bottom].a, mm(10.0, 10.0));
    assert!(matches!(
        refused(
            &mut e,
            C::MoveVia {
                via_id: via,
                position: mm(12.0, 12.0),
            }
        ),
        CommandError::NothingTo(_)
    ));
}

#[test]
fn edit_zone_and_delete_footprint() {
    let mut e = Engine::in_memory(Project::new("c4-fp"));
    fixtures::build_schematic(&mut e).unwrap();
    ok(
        &mut e,
        C::SetBoardOutline {
            points: vec![mm(0.0, 0.0), mm(40.0, 0.0), mm(40.0, 30.0), mm(0.0, 30.0)],
        },
    );
    ok(&mut e, C::SyncSchematicToPcb);
    let zone = ok(
        &mut e,
        C::AddZone {
            layer: CopperLayer::Bottom,
            net: "GND".into(),
            outline: vec![mm(1.0, 1.0), mm(39.0, 1.0), mm(39.0, 29.0)],
            clearance: None,
        },
    )
    .created[0]
        .clone();
    ok(
        &mut e,
        C::EditZone {
            zone_id: zone.clone(),
            outline: Some(vec![
                mm(1.0, 1.0),
                mm(39.0, 1.0),
                mm(39.0, 29.0),
                mm(1.0, 29.0),
            ]),
            net: None,
            layer: Some(CopperLayer::Top),
            clearance: Some(400_000),
        },
    );
    let z = &e.project().pcb.zones[&zone];
    assert_eq!(
        (z.outline.len(), z.layer, z.clearance),
        (4, CopperLayer::Top, 400_000)
    );
    let edit = |outline: Option<Vec<Point>>, clearance: Option<Nm>| C::EditZone {
        zone_id: zone.clone(),
        outline,
        net: None,
        layer: None,
        clearance,
    };
    assert!(matches!(
        refused(&mut e, edit(None, Some(400_000))),
        CommandError::NothingTo(_)
    ));
    assert!(matches!(
        refused(&mut e, edit(Some(vec![mm(0.0, 0.0), mm(1.0, 0.0)]), None)),
        CommandError::Invalid(_)
    ));
    assert!(matches!(
        refused(&mut e, edit(None, Some(-1))),
        CommandError::Invalid(_)
    ));

    let (fp_id, refdes) = {
        let f = e.project().pcb.footprints.values().next().unwrap();
        (f.id.clone(), f.refdes.clone())
    };
    let count = e.project().pcb.footprints.len();
    ok(
        &mut e,
        C::SetFootprintLocked {
            footprint_id: fp_id.clone(),
            locked: true,
        },
    );
    assert!(matches!(
        refused(
            &mut e,
            C::DeleteFootprint {
                footprint_id: fp_id.clone(),
            }
        ),
        CommandError::Precondition(_)
    ));
    undo(&mut e);
    let r = ok(
        &mut e,
        C::DeleteFootprint {
            footprint_id: fp_id.clone(),
        },
    );
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains(&refdes) && w.contains("sync")),
        "{:?}",
        r.warnings
    );
    assert_eq!(e.project().pcb.footprints.len(), count - 1);
    undo(&mut e);
    assert!(e.project().pcb.footprints.contains_key(&fp_id));
}

#[test]
fn regions_text_dimensions_and_their_classes() {
    let mut e = board();
    let rg = ok(
        &mut e,
        C::AddRegion {
            name: Some(" antenna ".into()),
            outline: vec![mm(20.0, 20.0), mm(30.0, 20.0), mm(30.0, 28.0)],
            layers: vec![CopperLayer::Top],
            forbid: vec![RegionObject::Tracks, RegionObject::Vias],
            min_track_width: None,
            max_track_width: None,
        },
    );
    assert_eq!(rg.class, OperationClass::EngineeringImpacting);
    let r = &e.project().pcb.regions[&rg.created[0]];
    assert_eq!(r.name, "antenna");
    assert!(r.forbids(RegionObject::Vias) && !r.forbids(RegionObject::Zones));
    let region =
        |layers: Vec<CopperLayer>, forbid: Vec<RegionObject>, min: Option<Nm>| C::AddRegion {
            name: None,
            outline: vec![mm(1.0, 1.0), mm(5.0, 1.0), mm(5.0, 5.0)],
            layers,
            forbid,
            min_track_width: min,
            max_track_width: None,
        };
    assert!(matches!(
        refused(&mut e, region(vec![], vec![], None)),
        CommandError::Invalid(_)
    ));
    assert!(matches!(
        refused(
            &mut e,
            region(
                vec![CopperLayer::Inner(1)],
                vec![RegionObject::Tracks],
                None
            )
        ),
        CommandError::Invalid(_)
    ));
    ok(&mut e, region(vec![], vec![], Some(300_000)));
    ok(
        &mut e,
        C::DeleteRegion {
            region_id: rg.created[0].clone(),
        },
    );
    assert!(matches!(
        refused(
            &mut e,
            C::DeleteRegion {
                region_id: rg.created[0].clone(),
            }
        ),
        CommandError::NotFound(_)
    ));

    let tx = ok(
        &mut e,
        C::AddPcbText {
            layer: DrawLayer::BottomSilk,
            position: mm(10.0, 2.0),
            text: "REV A".into(),
            size: None,
            stroke_width: None,
            rotation: None,
        },
    );
    assert_eq!(tx.class, OperationClass::ReversibleLocal);
    let id = tx.created[0].clone();
    assert_eq!(e.project().pcb.texts[&id].size, 1_000_000);
    ok(
        &mut e,
        C::MovePcbText {
            text_id: id.clone(),
            position: mm(12.0, 2.0),
            rotation: Some(Rotation::R90),
        },
    );
    assert_eq!(e.project().pcb.texts[&id].rotation, Rotation::R90);
    assert!(matches!(
        refused(
            &mut e,
            C::MovePcbText {
                text_id: id.clone(),
                position: mm(12.0, 2.0),
                rotation: None,
            }
        ),
        CommandError::NothingTo(_)
    ));
    assert!(matches!(
        refused(
            &mut e,
            C::AddPcbText {
                layer: DrawLayer::TopSilk,
                position: mm(1.0, 1.0),
                text: "  ".into(),
                size: None,
                stroke_width: None,
                rotation: None,
            }
        ),
        CommandError::Invalid(_)
    ));
    ok(&mut e, C::DeletePcbText { text_id: id });

    let dim = ok(
        &mut e,
        C::AddDimension {
            layer: None,
            a: mm(0.0, 0.0),
            b: mm(40.0, 0.0),
            offset: Some(-3_000_000),
            unit: None,
            precision: None,
            text_size: None,
        },
    );
    assert_eq!(dim.class, OperationClass::ReversibleLocal);
    let d = &e.project().pcb.dimensions[&dim.created[0]];
    assert_eq!(
        (d.layer, d.label()),
        (DrawLayer::Documentation, "40.00 mm".to_string())
    );
    assert!(matches!(
        refused(
            &mut e,
            C::AddDimension {
                layer: None,
                a: mm(1.0, 1.0),
                b: mm(1.0, 1.0),
                offset: None,
                unit: None,
                precision: Some(9),
                text_size: None,
            }
        ),
        CommandError::Invalid(_)
    ));
    ok(
        &mut e,
        C::DeleteDimension {
            dimension_id: dim.created[0].clone(),
        },
    );
    assert!(e.project().pcb.dimensions.is_empty());
}

fn square(x: f64, y: f64, s: f64) -> Vec<Point> {
    vec![mm(x, y), mm(x + s, y), mm(x + s, y + s), mm(x, y + s)]
}

#[test]
fn cutouts_stay_inside_the_board_and_apart() {
    let mut empty = Engine::in_memory(Project::new("no-outline"));
    assert!(matches!(
        refused(
            &mut empty,
            C::AddBoardCutout {
                outline: square(5.0, 5.0, 4.0),
            }
        ),
        CommandError::Precondition(_)
    ));
    let mut e = board();
    let c = ok(
        &mut e,
        C::AddBoardCutout {
            outline: square(5.0, 5.0, 4.0),
        },
    )
    .created[0]
        .clone();
    // Outside the board, overlapping the first cut-out, touching the board edge.
    for bad in [
        square(38.0, 5.0, 4.0),
        square(7.0, 7.0, 4.0),
        square(0.0, 10.0, 3.0),
    ] {
        assert!(matches!(
            refused(&mut e, C::AddBoardCutout { outline: bad }),
            CommandError::Invalid(_)
        ));
    }
    ok(
        &mut e,
        C::AddBoardCutout {
            outline: square(20.0, 5.0, 4.0),
        },
    );
    ok(
        &mut e,
        C::DeleteBoardCutout {
            cutout_id: c.clone(),
        },
    );
    assert!(matches!(
        refused(&mut e, C::DeleteBoardCutout { cutout_id: c }),
        CommandError::NotFound(_)
    ));
}

#[test]
fn outline_arcs_are_set_cleared_and_survive_a_vertex_drag() {
    let mut e = board();
    let r = ok(
        &mut e,
        C::SetOutlineArc {
            edge: 1,
            sweep_deg: Some(60.0),
        },
    );
    assert_eq!(r.class, OperationClass::EngineeringImpacting);
    assert_eq!(e.project().pcb.board.edge_sweep(1), Some(60.0));
    assert!(e.project().pcb.board.outline_path().len() > 4);
    assert!(matches!(
        refused(
            &mut e,
            C::SetOutlineArc {
                edge: 1,
                sweep_deg: Some(60.0),
            }
        ),
        CommandError::NothingTo(_)
    ));
    assert!(matches!(
        refused(
            &mut e,
            C::SetOutlineArc {
                edge: 4,
                sweep_deg: Some(30.0),
            }
        ),
        CommandError::Invalid(_)
    ));
    // An inward 270° bulge on the left edge (centre 15,15 mm, radius 21.2 mm) crosses the
    // bottom and top edges and is refused. (A -300° one would loop round the whole board
    // without crossing anything, which is a valid simple outline.)
    assert!(matches!(
        refused(
            &mut e,
            C::SetOutlineArc {
                edge: 3,
                sweep_deg: Some(-270.0),
            }
        ),
        CommandError::Invalid(_)
    ));
    // Dragging one vertex keeps the arc; changing the vertex count drops it with a warning.
    let r = ok(
        &mut e,
        C::SetBoardOutline {
            points: vec![mm(0.0, 0.0), mm(42.0, 0.0), mm(40.0, 30.0), mm(0.0, 30.0)],
        },
    );
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(e.project().pcb.board.edge_sweep(1), Some(60.0));
    let r = ok(
        &mut e,
        C::SetBoardOutline {
            points: vec![mm(0.0, 0.0), mm(40.0, 0.0), mm(20.0, 30.0)],
        },
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("arc")),
        "{:?}",
        r.warnings
    );
    assert!(e.project().pcb.board.outline_arcs.is_empty());
    undo(&mut e);
    assert_eq!(e.project().pcb.board.edge_sweep(1), Some(60.0));
    ok(
        &mut e,
        C::SetOutlineArc {
            edge: 1,
            sweep_deg: None,
        },
    );
    assert!(e.project().pcb.board.outline_arcs.is_empty());
}
