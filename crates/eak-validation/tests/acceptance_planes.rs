//! M7.6 acceptance: a 4-layer board with a split power plane, built only through typed engine
//! commands. In1 is solid GND; In2 is the split plane: GND with a +5V island notched into it from
//! the board edge, straight across the line between the two headers. A declared USB pair runs
//! from header J1 to header J2; track keep-outs (Top in the middle, Bottom at both ends) make it
//! change Top -> Bottom -> Top, so its Bottom run is referenced to In2 and must go around the
//! island.
//!
//! Accepted when the router routes everything (empty ratsnest, no shorts), the pair's Bottom run
//! goes around the island, every pair layer change has a GND return via joining In1 and In2
//! within the stitch ring, and DRC reports no error, with DRC-028 (split crossing), DRC-029
//! (missing reference) and DRC-031 (plane islands) PASS. DRC-030 stays UNKNOWN naming the missing
//! input until a largest return-via distance can be declared. Negative: a pair track straight
//! across the island FAILS DRC-028, and the board without its return vias FAILS DRC-030. Written
//! without running (no local builds): CI decides.

use eak_core::{CopperLayer, DiffPairRule, Id, LabelKind, NetClass, Point, RegionObject, Rotation};
use eak_core::{Project, Track};
use eak_engineering::copper;
use eak_engineering::router::{self, RouterOptions};
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;
use eak_validation::report::Severity;
use eak_validation::{drc, CheckState};

const DP: &str = "USB_DP";
const DN: &str = "USB_DN";
const IN1: CopperLayer = CopperLayer::Inner(1);
const IN2: CopperLayer = CopperLayer::Inner(2);
/// The +5V island on In2 (mm): x 25..35, from the board edge up to y 27.
const ISLAND: (f64, f64, f64) = (25.0, 35.0, 27.0);

fn run(e: &mut Engine, cmd: C) -> CommandResponse {
    let name = cmd.name();
    let r = e.execute(CommandRequest {
        expected_revision: e.project().revision,
        actor: Actor::human(),
        command: cmd,
        approval: None,
    });
    assert!(r.accepted, "{name}: {:?}", r.error);
    r
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        .map(|(x, y)| Point::mm(x, y))
        .to_vec()
}

/// A header symbol with its pins labelled `nets` in order.
fn header(e: &mut Engine, comp: &str, nets: &[&str]) -> Id {
    let n = e.project().schematic.symbols.len() as f64;
    let sym: Id = run(
        e,
        C::PlaceSymbol {
            sheet_id: None,
            component_id: comp.into(),
            position: Point::mm(20.0 * (n + 1.0), 50.0),
            rotation: Rotation::R0,
            refdes: None,
            value: None,
        },
    )
    .created[0]
        .clone();
    for (i, net) in nets.iter().enumerate() {
        run(
            e,
            C::LabelPin {
                symbol_id: sym.clone(),
                pin: (i + 1).to_string(),
                name: (*net).into(),
                kind: LabelKind::Net,
                scope: Default::default(),
                power_style: None,
            },
        );
    }
    sym
}

fn move_to(e: &mut Engine, sym: &Id, at: (f64, f64)) {
    let fp = e
        .project()
        .pcb
        .footprints
        .values()
        .find(|f| &f.symbol_id == sym)
        .map(|f| f.id.clone())
        .expect("synced footprint");
    run(
        e,
        C::MoveFootprint {
            footprint_id: fp,
            position: Point::mm(at.0, at.1),
            rotation: Some(Rotation::R0),
            side: None,
        },
    );
}

fn build() -> Engine {
    let mut e = Engine::in_memory(Project::new("M7.6 split-plane acceptance"));
    // Same rotation at both ends: D+ above D- at both, so the pair is not crossed.
    let pins = ["GND", DN, DP, "GND"];
    let j1 = header(&mut e, "CONN_1x04_P2.54", &pins);
    let j2 = header(&mut e, "CONN_1x04_P2.54", &pins);
    // A +5V load on the island keeps it filled (its GND pin joins In1).
    let j3 = header(&mut e, "CONN_1x02_P2.54", &["+5V", "GND"]);
    run(
        &mut e,
        C::SetLayerCount {
            count: 4,
            board_thickness: Some(1_600_000),
        },
    );
    run(
        &mut e,
        C::SetBoardOutline {
            points: rect(0.0, 0.0, 60.0, 40.0),
        },
    );
    run(
        &mut e,
        C::UpsertNetClass {
            class: NetClass {
                name: "USB".into(),
                track_width: 500_000,
                clearance: 150_000,
                via_drill: 300_000,
                via_diameter: 600_000,
                nets: vec![DP.into(), DN.into()],
                target_z0_ohm: None,
            },
        },
    );
    run(
        &mut e,
        C::SetDiffPair {
            rule: DiffPairRule {
                positive: DP.into(),
                negative: DN.into(),
                width: Some(500_000),
                gap: None,
                target_zdiff_ohm: Some(90.0),
                zdiff_tolerance_pct: Some(10.0),
                max_skew: Some(50_000),
            },
        },
    );
    run(&mut e, C::SyncSchematicToPcb);
    move_to(&mut e, &j1, (8.0, 15.0));
    move_to(&mut e, &j2, (52.0, 15.0));
    move_to(&mut e, &j3, (30.0, 10.0));

    // In1: solid GND. In2: GND with the +5V island notched in from the edge (1 mm split).
    let (x0, x1, top) = ISLAND;
    let zones = [
        (IN1, "GND", rect(0.5, 0.5, 59.5, 39.5)),
        (
            IN2,
            "GND",
            [
                (0.5, 0.5),
                (x0 - 1.0, 0.5),
                (x0 - 1.0, top + 1.0),
                (x1 + 1.0, top + 1.0),
                (x1 + 1.0, 0.5),
                (59.5, 0.5),
                (59.5, 39.5),
                (0.5, 39.5),
            ]
            .map(|(x, y)| Point::mm(x, y))
            .to_vec(),
        ),
        (IN2, "+5V", rect(x0, 0.5, x1, top)),
    ];
    for (layer, net, outline) in zones {
        run(
            &mut e,
            C::AddZone {
                layer,
                net: net.into(),
                outline,
                clearance: Some(300_000),
            },
        );
    }
    // No tracks on Top over the middle, none on Bottom at the ends: Top -> Bottom -> Top.
    for (name, layer, outline) in [
        ("no top", CopperLayer::Top, rect(22.0, 0.0, 38.0, 40.0)),
        (
            "no bottom left",
            CopperLayer::Bottom,
            rect(0.0, 0.0, 18.0, 40.0),
        ),
        (
            "no bottom right",
            CopperLayer::Bottom,
            rect(42.0, 0.0, 60.0, 40.0),
        ),
    ] {
        run(
            &mut e,
            C::AddRegion {
                name: Some(name.into()),
                outline,
                layers: vec![layer],
                forbid: vec![RegionObject::Tracks],
                min_track_width: None,
                max_track_width: None,
            },
        );
    }
    e
}

fn errors(p: &Project) -> Vec<String> {
    drc::run(p)
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("{}: {}", f.rule_id, f.message))
        .collect()
}

fn pair_tracks(p: &Project, layer: CopperLayer) -> Vec<&Track> {
    p.pcb
        .tracks
        .values()
        .filter(|t| (t.net == DP || t.net == DN) && t.layer == layer)
        .collect()
}

#[test]
fn a_high_speed_pair_routes_around_a_power_plane_split_with_return_vias_and_drc_clean() {
    let mut e = build();

    let plan = router::route(e.project(), &RouterOptions::default());
    assert!(
        plan.failed_connections.is_empty(),
        "{:?}",
        plan.failed_connections
    );
    let pair = plan.pairs.iter().find(|r| r.positive == DP).expect("pair");
    assert!(pair.declared && pair.coupled, "{pair:?}");
    assert!(
        plan.notes
            .iter()
            .any(|n| n.contains("return via(s) placed")),
        "{:?}",
        plan.notes
    );

    run(&mut e, C::AutoRoute { nets: None });
    let p = e.project();

    // Every net is proven by copper connectivity (GND through the planes).
    let conn = copper::analyse(p);
    assert!(conn.ratsnest.is_empty(), "unrouted: {:?}", conn.ratsnest);
    assert!(conn.shorts.is_empty(), "{:?}", conn.shorts);

    // The pair changes to Bottom and goes around the island there, never over it.
    let bottom = pair_tracks(p, CopperLayer::Bottom);
    assert!(!bottom.is_empty(), "the pair has no Bottom run");
    let top = ISLAND.2;
    let highest = bottom
        .iter()
        .flat_map(|t| [t.a, t.b])
        .map(|pt| pt.y as f64 / 1e6)
        .fold(f64::MIN, f64::max);
    assert!(highest > top + 1.0, "Bottom run tops out at {highest} mm");

    // Every pair layer change has a GND return via joining In1 and In2 within 3 mm.
    let st = &p.pcb.board.stackup;
    let pair_vias: Vec<_> = p
        .pcb
        .vias
        .values()
        .filter(|v| v.net == DP || v.net == DN)
        .collect();
    assert!(pair_vias.len() >= 4, "{} pair vias", pair_vias.len());
    let returns: Vec<_> = p
        .pcb
        .vias
        .values()
        .filter(|v| v.net == "GND")
        .filter(|v| {
            let ls = v.layers(st);
            ls.contains(&IN1) && ls.contains(&IN2)
        })
        .collect();
    for v in &pair_vias {
        let d = returns
            .iter()
            .map(|r| r.position.dist(v.position))
            .fold(f64::MAX, f64::min);
        assert!(d <= 3_000_000.0, "via {} nearest return via {d} nm", v.id);
    }

    // DRC: clean, the reference rules judged independently of the router.
    let errs = errors(p);
    assert!(errs.is_empty(), "{errs:#?}");
    let r = drc::run(p);
    for rule in ["DRC-028", "DRC-029", "DRC-031"] {
        assert_eq!(r.state_of(rule), CheckState::Pass, "{rule}");
    }
    // shortcut: DRC-030 is UNKNOWN until `return_via_max_distance` can be declared (approved IR
    // field, after schema v9 lands); then declare it here and assert PASS.
    assert_eq!(r.state_of("DRC-030"), CheckState::Unknown);
    let why = &r
        .rule_results
        .iter()
        .find(|x| x.rule_id == "DRC-030")
        .unwrap()
        .reason;
    assert!(why.contains("nearest GND return via"), "{why}");
    assert!(why.contains("no largest return-via distance"), "{why}");

    // Negative: a pair track straight across the island fails DRC-028.
    let mut bad = e.project().clone();
    bad.pcb.tracks.insert(
        "across".into(),
        Track {
            id: "across".into(),
            layer: CopperLayer::Bottom,
            width: 500_000,
            a: Point::mm(23.0, 15.0),
            b: Point::mm(37.0, 15.0),
            net: DP.into(),
            arc_deg: None,
        },
    );
    let r = drc::run(&bad);
    assert_eq!(r.state_of("DRC-028"), CheckState::Fail);
    let f = r.findings.iter().find(|f| f.rule_id == "DRC-028").unwrap();
    assert_eq!(f.objects, vec!["across".to_string()]);
    assert!(f.message.contains("In2.Cu"), "{}", f.message);

    // Negative: without the return vias, the layer changes fail DRC-030.
    let ids: Vec<Id> = returns.iter().map(|v| v.id.clone()).collect();
    for via_id in ids {
        run(&mut e, C::DeleteVia { via_id });
    }
    let r = drc::run(e.project());
    assert_eq!(r.state_of("DRC-030"), CheckState::Fail);
    assert!(
        r.findings
            .iter()
            .any(|f| f.rule_id == "DRC-030" && f.message.contains("no via of GND joins")),
        "{:#?}",
        r.findings
    );
}

#[test]
fn a_pair_whose_only_way_crosses_the_split_fails_naming_the_split() {
    let mut e = build();
    // Close the way round above the island on Bottom: the only Bottom path left crosses it.
    run(
        &mut e,
        C::AddRegion {
            name: Some("no way round".into()),
            outline: rect(18.0, 26.0, 42.0, 40.0),
            layers: vec![CopperLayer::Bottom],
            forbid: vec![RegionObject::Tracks],
            min_track_width: None,
            max_track_width: None,
        },
    );
    let plan = router::route(e.project(), &RouterOptions::default());
    let split: Vec<_> = plan
        .failed_connections
        .iter()
        .filter(|(net, ..)| net == DP || net == DN)
        .map(|(_, _, _, why)| why)
        .collect();
    assert!(!split.is_empty(), "the pair routed over the split");
    for why in split {
        assert!(
            why.contains("crosses a split in a reference plane"),
            "{why}"
        );
    }
}
