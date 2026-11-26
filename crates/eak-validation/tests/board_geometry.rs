//! DRC agreement (ADR-0051): the zone fill DRC checks is the fill the 2D/3D views draw. A zone
//! clearance edit moves a thermal relief's spoke from landing to blocked; before and after, the
//! thermal-relief state DRC reports matches the copper in the serialized `board_geometry`
//! payload, spoke by spoke.

use eak_core::{CopperLayer, FootprintInstance, Nm, Point, Project, Rotation, Side, Track, Zone};
use eak_engineering::board::BoardGeometry;
use eak_engineering::copper::ItemKind;
use eak_engineering::fill_view::{self, FillView};
use eak_validation::drc;

/// 20 × 20 mm board: header J1 at (10, 10) with pin 1 on GND inside a bottom GND zone, and a
/// bottom SIG track 3 mm to the right of pin 1 whose keep-out grows with the zone clearance.
fn board(zone_clearance: Nm) -> Project {
    let mut p = Project::new("drc-agreement");
    let def = eak_core::builtin::get("CONN_1x02_P2.54").unwrap();
    p.library.insert(def.id.clone(), def.clone());
    p.pcb.board.outline = vec![
        Point::mm(0.0, 0.0),
        Point::mm(20.0, 0.0),
        Point::mm(20.0, 20.0),
        Point::mm(0.0, 20.0),
    ];
    p.pcb.footprints.insert(
        "fp".into(),
        FootprintInstance {
            id: "fp".into(),
            symbol_id: "sym".into(),
            component_id: def.id.clone(),
            refdes: "J1".into(),
            position: Point::mm(10.0, 10.0),
            rotation: Rotation::R0,
            side: Side::Top,
            pad_nets: [("1".to_string(), "GND".to_string())].into(),
            locked: false,
        },
    );
    p.pcb.tracks.insert(
        "sig".into(),
        Track {
            id: "sig".into(),
            layer: CopperLayer::Bottom,
            width: 250_000,
            a: Point::mm(13.0, 5.0),
            b: Point::mm(13.0, 15.0),
            net: "SIG".into(),
            arc_deg: None,
        },
    );
    p.pcb.zones.insert(
        "z".into(),
        Zone {
            id: "z".into(),
            layer: CopperLayer::Bottom,
            net: "GND".into(),
            outline: vec![
                Point::mm(1.0, 1.0),
                Point::mm(19.0, 1.0),
                Point::mm(19.0, 19.0),
                Point::mm(1.0, 19.0),
            ],
            clearance: zone_clearance,
        },
    );
    p
}

/// The fills exactly as the desktop receives them: the JSON payload of the geometry.
fn drawn_fills(g: &BoardGeometry) -> Vec<FillView> {
    let wire = serde_json::to_value(g).unwrap();
    serde_json::from_value(wire["fill_views"].clone()).unwrap()
}

fn drawn_at(f: &FillView, q: Point) -> bool {
    f.rects
        .iter()
        .any(|[x0, y0, x1, y1]| (*x0..*x1).contains(&q.x) && (*y0..*y1).contains(&q.y))
}

/// Per spoke (+X, +Y, -X, -Y) of pad J1.1: whether the drawn fill has copper half-way across the
/// relief gap on the spoke's centre line.
fn drawn_spokes(p: &Project, g: &BoardGeometry) -> Vec<bool> {
    let fill = &drawn_fills(g)[0];
    let pad = g
        .copper
        .iter()
        .find(|i| i.kind == ItemKind::Pad && i.sub == "1" && i.layer == CopperLayer::Bottom)
        .unwrap();
    let bb = pad.shape.bbox();
    let c = Point::new((bb.min.x + bb.max.x) / 2, (bb.min.y + bb.max.y) / 2);
    let half = (bb.max.x - bb.min.x) / 2;
    let gap = p.pcb.zones["z"].clearance;
    eak_engineering::copper::SPOKE_DIRS
        .iter()
        .map(|(dx, dy)| {
            let r = half + gap / 2;
            drawn_at(fill, Point::new(c.x + dx * r, c.y + dy * r))
        })
        .collect()
}

#[test]
fn drc_and_the_drawn_fill_agree_across_a_clearance_edit() {
    let mut seen = Vec::new();
    for clearance in [300_000, 1_000_000] {
        let p = board(clearance);
        let g = BoardGeometry::build(&p);
        // DRC is the check of this very geometry.
        let report = drc::run_with(&p, &g);
        assert_eq!(report, drc::run(&p));
        // The payload's fill is DRC's fill (and an independent recomputation agrees).
        let fill = g.fill("z").expect("zone fills");
        assert_eq!(drawn_fills(&g)[0].rects, fill_view::rects_of(fill));
        assert_eq!(drawn_fills(&g), fill_view::fill_views(&p));
        // Spoke by spoke: DRC's relief state is what the drawn copper shows.
        let relief = fill
            .reliefs
            .iter()
            .find(|r| r.pad == "1")
            .expect("J1.1 relief");
        let checked: Vec<bool> = relief.spokes.iter().map(|s| s.connected).collect();
        let drawn = drawn_spokes(&p, &g);
        assert_eq!(drawn, checked, "clearance {clearance}");
        let n = drawn.iter().filter(|c| **c).count();
        let few = report
            .findings
            .iter()
            .any(|f| f.rule_id == "DRC-014" && f.message.contains(&format!("lands {n} of 4")));
        assert_eq!(few, n < drc::MIN_SPOKES, "clearance {clearance}");
        seen.push((drawn, drawn_fills(&g)));
    }
    // The edit really moved copper: the +X spoke lands at 0.3 mm and is blocked by the SIG
    // track's keep-out at 1.0 mm, and the drawn fill changed with it.
    assert!(
        seen[0].0[0] && !seen[1].0[0],
        "{:?}",
        (&seen[0].0, &seen[1].0)
    );
    assert_ne!(seen[0].1, seen[1].1);
}
