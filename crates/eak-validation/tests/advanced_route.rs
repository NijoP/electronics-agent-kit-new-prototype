//! Workstream A (eak-route-dp) advanced routing.
//!
//! Differential pairs: a declared 90 ohm differential pair (ADR-0055) between two
//! connectors on a 4-layer board is auto-routed coupled. The gap is derived from the target
//! impedance with the stack-up (IPC-2141A), the members keep that gap everywhere, the skew is
//! reported against `max_skew` (matching is a later increment), the pair is connected, and DRC
//! finds no clearance or short error. A pair whose gap cannot be derived (two-layer board: no
//! reference plane) is not routed coupled and says which input is missing. Written without
//! running (no local builds): CI decides.

use eak_core::geom::segment_segment_distance;
use eak_core::{
    CopperLayer, DiffPairRule, FootprintInstance, NetClass, Point, Project, Rotation, Side,
    Stackup, Track, Via, FR4_ER,
};
use eak_engineering::copper::{self, ItemKind};
use eak_engineering::router::{self, RouteResult, RouterOptions};
use eak_validation::drc;

const P: &str = "USB_DP";
const N: &str = "USB_DN";
const WIDTH: i64 = 500_000;

fn place(p: &mut Project, id: &str, at: Point, rotation: Rotation) {
    let def = eak_core::builtin::get("CONN_1x02_P2.54").unwrap();
    p.library.insert(def.id.clone(), def.clone());
    p.pcb.footprints.insert(
        id.into(),
        FootprintInstance {
            id: id.into(),
            symbol_id: format!("sym_{id}"),
            component_id: def.id,
            refdes: id.into(),
            position: at,
            rotation,
            side: Side::Top,
            pad_nets: Default::default(),
            locked: false,
        },
    );
}

/// Give footprint `id`'s pad with the smallest `rank` (of its centre) net `first`, the other
/// pad `second`.
fn assign(p: &mut Project, id: &str, first: &str, second: &str, rank: impl Fn(Point) -> f64) {
    let mut pads: Vec<(String, Point)> = copper::items(p)
        .into_iter()
        .filter(|i| i.kind == ItemKind::Pad && i.object_id == id && i.layer == CopperLayer::Top)
        .map(|i| (i.sub.clone(), i.shape.bbox().center()))
        .collect();
    pads.sort_by(|a, b| rank(a.1).total_cmp(&rank(b.1)));
    let fp = p.pcb.footprints.get_mut(id).unwrap();
    fp.pad_nets.insert(pads[0].0.clone(), first.into());
    fp.pad_nets.insert(pads[1].0.clone(), second.into());
}

/// J1 (pins one above the other) at (10, 20) and J2 (pins side by side) at (40, 40): the pair
/// leaves J1 heading east and turns 45 degrees towards J2, so its members differ in length.
fn board(stackup: Stackup) -> Project {
    let mut p = Project::new("diff-pair");
    p.pcb.board.outline = vec![
        Point::mm(0.0, 0.0),
        Point::mm(60.0, 0.0),
        Point::mm(60.0, 60.0),
        Point::mm(0.0, 60.0),
    ];
    p.pcb.board.stackup = stackup;
    place(&mut p, "J1", Point::mm(10.0, 20.0), Rotation::R0);
    place(&mut p, "J2", Point::mm(40.0, 40.0), Rotation::R90);
    // The positive member runs on the left of travel: the upper pin at J1, the left one at J2.
    assign(&mut p, "J1", P, N, |c| -(c.y as f64));
    assign(&mut p, "J2", P, N, |c| c.x as f64);
    p.constraints.net_classes.push(NetClass {
        name: "USB".into(),
        track_width: WIDTH,
        clearance: 150_000,
        via_drill: 300_000,
        via_diameter: 600_000,
        nets: vec![P.into(), N.into()],
        target_z0_ohm: None,
    });
    p.constraints.diff_pairs.push(DiffPairRule {
        positive: P.into(),
        negative: N.into(),
        width: Some(WIDTH),
        gap: None,
        target_zdiff_ohm: Some(90.0),
        zdiff_tolerance_pct: Some(10.0),
        max_skew: Some(50_000),
    });
    p
}

fn four_layer() -> Stackup {
    Stackup::multilayer(4, 1_600_000, 1.0, 0.5, "FR-4", FR4_ER)
}

/// Commit a route result the way `AutoRoute` does.
fn commit(p: &mut Project, res: &RouteResult) {
    for (k, t) in res.tracks.iter().enumerate() {
        let id = format!("trk{k}");
        p.pcb.tracks.insert(
            id.clone(),
            Track {
                id,
                layer: t.layer,
                width: t.width,
                a: t.a,
                b: t.b,
                net: t.net.clone(),
                arc_deg: None,
            },
        );
    }
    for (k, v) in res.vias.iter().enumerate() {
        let id = format!("via{k}");
        p.pcb.vias.insert(
            id.clone(),
            Via {
                id,
                position: v.position,
                drill: v.drill,
                diameter: v.diameter,
                net: v.net.clone(),
                from: CopperLayer::Top,
                to: CopperLayer::Bottom,
            },
        );
    }
}

#[test]
fn declared_pair_is_routed_coupled_at_the_impedance_gap() {
    let mut p = board(four_layer());
    let res = router::route(&p, &RouterOptions::default());
    assert!(
        res.failed_connections.is_empty(),
        "{:?}",
        res.failed_connections
    );
    let pair = res
        .pairs
        .iter()
        .find(|r| r.positive == P)
        .expect("the declared pair is reported");
    assert!(pair.declared && pair.coupled, "{pair:?}");
    let gap_mm = pair.gap_mm.unwrap();
    assert!(
        gap_mm >= 0.15,
        "the derived gap keeps the class clearance: {gap_mm}"
    );
    assert!(pair.note.contains("derived for 90 ohm"), "{}", pair.note);
    // Length matching is a later increment: the skew verdict is known, not yet guaranteed.
    assert!(pair.skew_within_tolerance.is_some(), "{pair:?}");
    assert!(pair.note.contains("not length matched"), "{}", pair.note);
    assert_eq!(pair.impedance_within_tolerance, Some(true), "{pair:?}");
    assert!(pair
        .impedance
        .iter()
        .all(|l| matches!(l.zdiff, eak_engineering::impedance::Estimate::Known { .. })));

    // The members keep the gap: no P segment comes closer than the gap to an N segment, and
    // they do run coupled (the closest approach is the gap itself).
    let gap = gap_mm * 1e6;
    let members = |net: &str| -> Vec<_> { res.tracks.iter().filter(|t| t.net == net).collect() };
    let mut closest = f64::INFINITY;
    for a in members(P) {
        for b in members(N).into_iter().filter(|b| b.layer == a.layer) {
            closest = closest.min(segment_segment_distance(a.a, a.b, b.a, b.b) - WIDTH as f64);
        }
    }
    assert!(
        closest >= gap - 2_000.0,
        "closest {closest} nm, gap {gap} nm"
    );
    assert!(
        closest <= gap + 2_000.0,
        "the members never run coupled: {closest} nm"
    );

    commit(&mut p, &res);
    let conn = copper::analyse(&p);
    assert!(
        !conn.ratsnest.iter().any(|(n, _, _)| n == P || n == N),
        "pair left unrouted: {:?}",
        conn.ratsnest
    );
    assert!(conn.shorts.is_empty(), "{:?}", conn.shorts);
    let bad: Vec<String> = drc::run(&p)
        .findings
        .into_iter()
        .filter(|f| f.rule_id == "DRC-002" || f.rule_id == "DRC-003")
        .map(|f| f.message)
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
}

#[test]
fn a_pair_whose_gap_cannot_be_derived_is_not_routed_coupled_and_says_why() {
    // Two-layer board: no reference plane, so the 90 ohm gap is UNKNOWN.
    let p = board(Stackup::two_layer(1_600_000, 1.0, "FR-4", FR4_ER));
    let res = router::route(&p, &RouterOptions::default());
    let pair = res.pairs.iter().find(|r| r.positive == P).unwrap();
    assert!(pair.declared && !pair.coupled, "{pair:?}");
    assert!(pair.note.contains("reference plane"), "{}", pair.note);
    assert_eq!(pair.gap_mm, None);
    assert_eq!(pair.impedance_within_tolerance, None);
}
