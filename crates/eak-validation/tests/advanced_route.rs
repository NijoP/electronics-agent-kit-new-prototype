//! Workstream A (eak-route-dp) advanced routing.
//!
//! Differential pairs: a declared 90 ohm differential pair (ADR-0055) between two
//! connectors on a 4-layer board is auto-routed coupled. The gap is derived from the target
//! impedance with the stack-up (IPC-2141A), the members keep that gap everywhere, the skew is
//! length-matched within `max_skew`, the pair is connected, and DRC
//! finds no clearance or short error. A pair whose gap cannot be derived (two-layer board: no
//! reference plane) is not routed coupled and says which input is missing. Written without
//! running (no local builds): CI decides.

use eak_core::geom::segment_segment_distance;
use eak_core::pcb_objects::{Region, RegionObject};
use eak_core::{
    CopperLayer, DiffPairRule, FootprintInstance, Insulation, InsulationStandard, LayerKind,
    NetClass, Point, Project, Rotation, Side, Stackup, Track, Via, FR4_ER,
};
use eak_engineering::clearance;
use eak_engineering::copper::{self, ItemKind};
use eak_engineering::impedance;
use eak_engineering::router::{self, RouteResult, RouterOptions};
use eak_validation::{drc, CheckState, Severity};

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

/// Commit a route result the way `AutoRoute` does (pushed-aside tracks are replaced).
fn commit(p: &mut Project, res: &RouteResult) {
    for id in &res.replaced_tracks {
        assert!(
            p.pcb.tracks.remove(id).is_some(),
            "replaced track {id} exists"
        );
    }
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
fn declared_pair_is_routed_coupled_at_the_impedance_gap_and_length_matched() {
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
    assert_eq!(pair.skew_within_tolerance, Some(true), "{pair:?}");
    assert!(pair.skew_mm <= 0.05 + 1e-6, "{pair:?}");
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

fn track(p: &mut Project, id: &str, layer: CopperLayer, a: Point, b: Point) {
    p.pcb.tracks.insert(
        id.into(),
        Track {
            id: id.into(),
            layer,
            width: 250_000,
            a,
            b,
            net: "SIG".into(),
            arc_deg: None,
        },
    );
}

/// Push-and-shove: two wide areas joined by a 2.7 mm neck (y 8.2..10.9 mm). A low-priority SIG
/// track crosses the neck at y 9.6 mm on both layers. The 0.8 mm VCC route (0.2 mm clearance,
/// 0.3 mm edge clearance) fits neither below it (it would need y <= 8.875 and >= 8.9) nor
/// above it (y >= 10.325 but <= 10.2), so the plain router fails; pushing SIG up makes room.
#[test]
fn a_priority_net_shoves_a_lower_priority_track_aside_and_stays_drc_clean() {
    let mut p = Project::new("shove");
    p.pcb.board.outline = [
        (0.0, 0.0),
        (15.0, 0.0),
        (15.0, 8.2),
        (35.0, 8.2),
        (35.0, 0.0),
        (50.0, 0.0),
        (50.0, 14.0),
        (35.0, 14.0),
        (35.0, 10.9),
        (15.0, 10.9),
        (15.0, 14.0),
        (0.0, 14.0),
    ]
    .into_iter()
    .map(|(x, y)| Point::mm(x, y))
    .collect();
    place(&mut p, "J1", Point::mm(5.0, 4.0), Rotation::R90);
    place(&mut p, "J2", Point::mm(45.0, 4.0), Rotation::R90);
    assign(&mut p, "J1", "VCC", "VCC", |c| c.x as f64);
    assign(&mut p, "J2", "VCC", "VCC", |c| c.x as f64);
    p.constraints.net_classes.push(NetClass {
        name: "PWR".into(),
        track_width: 800_000,
        clearance: 200_000,
        via_drill: 400_000,
        via_diameter: 800_000,
        nets: vec!["VCC".into()],
        target_z0_ohm: None,
    });
    let (a, b) = (Point::mm(2.0, 9.6), Point::mm(48.0, 9.6));
    track(&mut p, "sig_top", CopperLayer::Top, a, b);
    track(&mut p, "sig_bot", CopperLayer::Bottom, a, b);

    let res = router::route(&p, &RouterOptions::default());
    assert!(
        res.failed_connections.is_empty(),
        "{:?}",
        res.failed_connections
    );
    assert_eq!(res.replaced_tracks.len(), 1, "{:?}", res.replaced_tracks);
    let moved = &res.replaced_tracks[0];
    assert!(moved == "sig_top" || moved == "sig_bot");
    // The moved SIG copper keeps both of its ends.
    let sig: Vec<_> = res.tracks.iter().filter(|t| t.net == "SIG").collect();
    assert!(sig.iter().any(|t| t.a == a || t.b == a), "{sig:?}");
    assert!(sig.iter().any(|t| t.a == b || t.b == b), "{sig:?}");
    assert!(
        res.notes.iter().any(|n| n.contains("push-and-shove")),
        "{:?}",
        res.notes
    );

    commit(&mut p, &res);
    let conn = copper::analyse(&p);
    assert!(
        !conn.ratsnest.iter().any(|(n, _, _)| n == "VCC"),
        "{:?}",
        conn.ratsnest
    );
    assert!(conn.shorts.is_empty(), "{:?}", conn.shorts);
    let bad: Vec<String> = drc::run(&p)
        .findings
        .into_iter()
        .filter(|f| ["DRC-002", "DRC-003", "DRC-005"].contains(&f.rule_id.as_str()))
        .map(|f| format!("{}: {}", f.rule_id, f.message))
        .collect();
    assert!(bad.is_empty(), "{bad:?}");
}

/// Two wide areas joined by the 2.7 mm neck of the shove test (usable y 8.5..10.6 mm inside the
/// 0.3 mm edge clearance), walled at y 9.625 mm on both layers by a 0.25 mm GND track (the
/// highest priority, so never shoved). Below the wall the channel is 1.0 mm: a 0.25 mm FINE
/// track at the default 0.2 mm clearance fits (0.45 mm, a 0.55 mm band of centres on the
/// 0.25 mm grid); a 0.25 mm WIDE track whose own class asks 0.8 mm does not (1.05 mm). FINE runs
/// J1-J2 and WIDE J3-J4, far enough apart that WIDE's rule never closes FINE's pads.
fn neck_with_two_clearances() -> Project {
    let mut p = Project::new("per-net-clearance");
    p.pcb.board.outline = [
        (0.0, 0.0),
        (15.0, 0.0),
        (15.0, 8.2),
        (35.0, 8.2),
        (35.0, 0.0),
        (50.0, 0.0),
        (50.0, 14.0),
        (35.0, 14.0),
        (35.0, 10.9),
        (15.0, 10.9),
        (15.0, 14.0),
        (0.0, 14.0),
    ]
    .into_iter()
    .map(|(x, y)| Point::mm(x, y))
    .collect();
    place(&mut p, "J1", Point::mm(5.0, 5.0), Rotation::R90);
    place(&mut p, "J2", Point::mm(45.0, 5.0), Rotation::R90);
    place(&mut p, "J3", Point::mm(10.0, 2.0), Rotation::R90);
    place(&mut p, "J4", Point::mm(40.0, 2.0), Rotation::R90);
    for (j, net) in [
        ("J1", "FINE"),
        ("J2", "FINE"),
        ("J3", "WIDE"),
        ("J4", "WIDE"),
    ] {
        assign(&mut p, j, net, net, |c| c.x as f64);
    }
    p.constraints.net_classes.push(NetClass {
        name: "WIDE_GAP".into(),
        track_width: 250_000,
        clearance: 800_000,
        via_drill: 300_000,
        via_diameter: 600_000,
        nets: vec!["WIDE".into()],
        target_z0_ohm: None,
    });
    let (a, b) = (Point::mm(2.0, 9.625), Point::mm(48.0, 9.625));
    for (id, layer) in [
        ("gnd_top", CopperLayer::Top),
        ("gnd_bot", CopperLayer::Bottom),
    ] {
        track(&mut p, id, layer, a, b);
        p.pcb.tracks.get_mut(id).unwrap().net = "GND".into();
    }
    p
}

/// Edge-to-edge distance (nm) from `net`'s routed tracks to the GND wall.
fn gap_to_gnd(res: &RouteResult, p: &Project, net: &str) -> f64 {
    let gnd: Vec<&Track> = p.pcb.tracks.values().filter(|t| t.net == "GND").collect();
    res.tracks
        .iter()
        .filter(|t| t.net == net)
        .flat_map(|t| {
            gnd.iter()
                .filter(move |g| g.layer == t.layer)
                .map(move |g| {
                    segment_segment_distance(t.a, t.b, g.a, g.b) - (t.width + g.width) as f64 / 2.0
                })
        })
        .fold(f64::MAX, f64::min)
}

/// M7.5 (a): a fine-pitch net routes closer to its neighbour than another net's own larger
/// clearance allows, while that larger clearance is still enforced: the WIDE net is not routed
/// through a gap that only meets FINE's rule.
#[test]
fn a_nets_own_larger_clearance_is_enforced_while_a_fine_net_routes_closer() {
    let p = neck_with_two_clearances();
    assert_eq!(
        clearance::required(&p.constraints, "FINE", "GND", CopperLayer::Top),
        200_000.0
    );
    assert_eq!(
        clearance::required(&p.constraints, "WIDE", "GND", CopperLayer::Top),
        800_000.0
    );
    let res = router::route(&p, &RouterOptions::default());
    assert!(
        !res.failed_connections.iter().any(|f| f.0 == "FINE"),
        "{:?}",
        res.failed_connections
    );
    let fine = gap_to_gnd(&res, &p, "FINE");
    assert!(
        (199_999.0..800_000.0).contains(&fine),
        "FINE passes the wall at {fine} nm: within its 0.2 mm rule, inside WIDE's 0.8 mm"
    );
    assert!(
        res.tracks.iter().all(|t| t.net != "WIDE") || gap_to_gnd(&res, &p, "WIDE") >= 799_999.0,
        "WIDE copper closer than its own 0.8 mm clearance"
    );
}

/// M7.5 (b): when a net's own clearance cannot be met anywhere, the router reports the
/// connection as failed with a reason and places no copper for it; nothing is routed too close.
#[test]
fn the_router_reports_a_connection_whose_own_clearance_cannot_be_met() {
    let mut p = neck_with_two_clearances();
    let res = router::route(&p, &RouterOptions::default());
    let wide: Vec<_> = res
        .failed_connections
        .iter()
        .filter(|f| f.0 == "WIDE")
        .collect();
    assert_eq!(wide.len(), 1, "{:?}", res.failed_connections);
    assert!(
        !wide[0].3.is_empty(),
        "a failed connection names its reason"
    );
    // J3's and J4's own pad-to-pad links are legal; no WIDE copper enters the neck (x 15..35).
    let in_neck = |q: Point| (15_000_000..=35_000_000).contains(&q.x);
    assert!(
        res.tracks
            .iter()
            .all(|t| t.net != "WIDE" || !(in_neck(t.a) || in_neck(t.b))),
        "WIDE copper was placed in the neck"
    );
    commit(&mut p, &res);
    let conn = copper::analyse(&p);
    assert!(
        conn.ratsnest.iter().any(|(n, _, _)| n == "WIDE"),
        "WIDE still unrouted"
    );
    assert!(conn.shorts.is_empty(), "{:?}", conn.shorts);
    let too_close: Vec<String> = drc::run(&p)
        .findings
        .into_iter()
        .filter(|f| f.rule_id == "DRC-002")
        .map(|f| f.message)
        .collect();
    assert!(too_close.is_empty(), "{too_close:?}");
}

/// M7.5 (c): voltage spacing fails closed. A routed 100 V board is DRC-002 UNKNOWN, never
/// PASS, while the HV net's voltage is undeclared or no insulation environment is declared
/// (`clearance::spacing_unknown`: altitude or coating would change the rule); with both
/// declared the check is decided.
#[test]
fn voltage_spacing_is_unknown_not_pass_while_an_input_is_undeclared() {
    let mut p = Project::new("hv");
    p.pcb.board.outline = vec![
        Point::mm(0.0, 0.0),
        Point::mm(50.0, 0.0),
        Point::mm(50.0, 20.0),
        Point::mm(0.0, 20.0),
    ];
    place(&mut p, "J1", Point::mm(10.0, 10.0), Rotation::R90);
    place(&mut p, "J2", Point::mm(40.0, 10.0), Rotation::R90);
    assign(&mut p, "J1", "HV", "GND", |c| c.x as f64);
    assign(&mut p, "J2", "HV", "GND", |c| c.x as f64);
    let c = &mut p.constraints;
    c.net_voltages_v.insert("HV".into(), 100.0);
    c.net_voltages_v.insert("GND".into(), 0.0);
    c.insulation = Some(Insulation {
        standard: InsulationStandard::Ipc2221b,
        pollution_degree: 2,
        material_group: "IIIb".into(),
        altitude_m: 0.0,
        coated: false,
    });
    let res = router::route(&p, &RouterOptions::default());
    assert!(
        res.failed_connections.is_empty(),
        "{:?}",
        res.failed_connections
    );
    commit(&mut p, &res);
    let state = |p: &Project| {
        let r = drc::run(p);
        (r.state_of("DRC-002"), format!("{r:?}"))
    };
    let (decided, why) = state(&p);
    assert_ne!(decided, CheckState::Unknown, "{why}");
    let errors: Vec<_> = drc::run(&p)
        .findings
        .into_iter()
        .filter(|f| f.rule_id == "DRC-002" && f.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");

    let mut no_ins = p.clone();
    no_ins.constraints.insulation = None;
    assert!(clearance::spacing_unknown(&no_ins.constraints).is_some());
    let (s, why) = state(&no_ins);
    assert_eq!(s, CheckState::Unknown, "{why}");
    assert!(why.contains("no insulation environment declared"), "{why}");

    let mut no_v = p.clone();
    no_v.constraints.net_voltages_v.remove("HV");
    let (s, why) = state(&no_v);
    assert_eq!(s, CheckState::Unknown, "{why}");
    assert!(why.contains("no declared voltage for: HV"), "{why}");
}

/// Top, In1 plane, In2 stripline, In3 plane, Bottom on a 3.2 mm board, a 0.3 mm declared gap and a
/// target equal to the Top estimate; track keep-outs on Top and Bottom across x 20..30 mm leave
/// In2 the only way from J1 to J2.
fn stripline_board(tolerance_pct: f64) -> (Project, f64, f64) {
    let mut st = Stackup::multilayer(5, 3_200_000, 1.0, 0.5, "FR-4", FR4_ER);
    st.layers[3].kind = LayerKind::Plane;
    let z = |layer| {
        let g = impedance::geometry(&st, layer).expect("modelled layer");
        impedance::zdiff(g, WIDTH, 300_000)
            .ohm()
            .expect("in the model")
    };
    let (top, in2) = (z(CopperLayer::Top), z(CopperLayer::Inner(2)));
    let mut p = board(st);
    let pair = &mut p.constraints.diff_pairs[0];
    pair.gap = Some(300_000);
    pair.target_zdiff_ohm = Some(top);
    pair.zdiff_tolerance_pct = Some(tolerance_pct);
    for (id, layer) in [
        ("ko_top", CopperLayer::Top),
        ("ko_bot", CopperLayer::Bottom),
    ] {
        let outline = [(20.0, 0.0), (30.0, 0.0), (30.0, 60.0), (20.0, 60.0)];
        p.pcb.regions.insert(
            id.into(),
            Region {
                id: id.into(),
                name: id.into(),
                outline: outline.map(|(x, y)| Point::mm(x, y)).to_vec(),
                layers: vec![layer],
                forbid: [RegionObject::Tracks].into(),
                min_track_width: None,
                max_track_width: None,
            },
        );
    }
    (p, top, in2)
}

#[test]
fn a_pair_never_routes_coupled_on_a_layer_outside_its_impedance_band() {
    // M7.7: the same width and gap are about 113 ohm on Top but about 88 ohm as In2 stripline.
    let (p, top, in2) = stripline_board(10.0);
    assert!(
        (in2 - top).abs() > top * 0.1,
        "precondition: {top} vs {in2}"
    );
    let res = router::route(&p, &RouterOptions::default());
    let pair = res.pairs.iter().find(|r| r.positive == P).unwrap();
    assert!(pair.declared && !pair.coupled, "{pair:?}");
    assert!(
        pair.note.contains("impedance band") && pair.note.contains("In2"),
        "{}",
        pair.note
    );
    assert_ne!(pair.impedance_within_tolerance, Some(true), "{pair:?}");

    // With a band that takes In2 in, the pair routes coupled through In2 and is within it.
    let (p, top, in2) = stripline_board(30.0);
    assert!(
        (in2 - top).abs() <= top * 0.3,
        "precondition: {top} vs {in2}"
    );
    let res = router::route(&p, &RouterOptions::default());
    let pair = res.pairs.iter().find(|r| r.positive == P).unwrap();
    assert!(pair.coupled, "{pair:?}");
    assert!(
        pair.impedance.iter().any(|l| l.layer.starts_with("In2")),
        "{pair:?}"
    );
    assert_eq!(pair.impedance_within_tolerance, Some(true), "{pair:?}");
}
