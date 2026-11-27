//! Reference planes and return paths (M7.6): DRC-029 split crossing, DRC-030 missing reference,
//! DRC-031 return via at a layer change and DRC-032 plane islands, PASS / FAIL / UNKNOWN /
//! NOT_APPLICABLE on a 4-layer board with a solid GND plane (In1) and a split In2 plane.

use eak_core::{CopperLayer, DiffPairRule, Point, Project, Stackup, Track, Via, Zone, FR4_ER};
use eak_validation::{drc, CheckState, Report, Severity};

const IN1: CopperLayer = CopperLayer::Inner(1);
const IN2: CopperLayer = CopperLayer::Inner(2);
const THROUGH: [CopperLayer; 2] = [CopperLayer::Top, CopperLayer::Bottom];

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Point> {
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        .map(|(x, y)| Point::mm(x, y))
        .to_vec()
}

fn via(p: &mut Project, id: &str, at: (f64, f64), net: &str, span: [CopperLayer; 2]) {
    p.pcb.vias.insert(
        id.into(),
        Via {
            id: id.into(),
            position: Point::mm(at.0, at.1),
            drill: 300_000,
            diameter: 600_000,
            net: net.into(),
            from: span[0],
            to: span[1],
        },
    );
}

fn zone(p: &mut Project, id: &str, layer: CopperLayer, net: &str, outline: Vec<Point>) {
    p.pcb.zones.insert(
        id.into(),
        Zone {
            id: id.into(),
            layer,
            net: net.into(),
            outline,
            clearance: 300_000,
        },
    );
}

fn track(p: &mut Project, id: &str, layer: CopperLayer, a: (f64, f64), b: (f64, f64)) {
    p.pcb.tracks.insert(
        id.into(),
        Track {
            id: id.into(),
            layer,
            width: 250_000,
            a: Point::mm(a.0, a.1),
            b: Point::mm(b.0, b.1),
            net: "D_P".into(),
            arc_deg: None,
        },
    );
}

/// 40 x 20 mm, 4 layers: In1 GND (x 1..39 mm); In2 `left` (x 1..19) and +5V (x 21..39). Each
/// zone keeps its fill through a through via of its net.
fn board(left: &str) -> Project {
    let mut p = Project::new("planes");
    p.pcb.board.outline = rect(0.0, 0.0, 40.0, 20.0);
    p.pcb.board.stackup = Stackup::multilayer(4, 1_600_000, 1.0, 0.5, "FR-4", FR4_ER);
    zone(&mut p, "gnd", IN1, "GND", rect(1.0, 1.0, 39.0, 19.0));
    zone(&mut p, "left", IN2, left, rect(1.0, 1.0, 19.0, 19.0));
    zone(&mut p, "p5", IN2, "+5V", rect(21.0, 1.0, 39.0, 19.0));
    via(&mut p, "vg", (20.0, 10.0), "GND", THROUGH);
    via(&mut p, "vl", (5.0, 5.0), left, THROUGH);
    via(&mut p, "v5", (35.0, 5.0), "+5V", THROUGH);
    p
}

fn declare_pair(p: &mut Project) {
    p.constraints.diff_pairs.push(DiffPairRule {
        positive: "D_P".into(),
        negative: "D_N".into(),
        width: None,
        gap: None,
        target_zdiff_ohm: None,
        zdiff_tolerance_pct: None,
        max_skew: None,
    });
}

fn reason(r: &Report, rule: &str) -> String {
    let rr = r.rule_results.iter().find(|x| x.rule_id == rule).unwrap();
    rr.reason.clone()
}

fn errors(r: &Report, rule: &str) -> Vec<String> {
    r.findings
        .iter()
        .filter(|f| f.rule_id == rule && f.severity == Severity::Error)
        .map(|f| f.message.clone())
        .collect()
}

#[test]
fn without_declared_high_speed_nets_the_reference_rules_do_not_apply() {
    let mut p = board("+3V3");
    track(&mut p, "t", CopperLayer::Bottom, (5.0, 15.0), (35.0, 15.0));
    let r = drc::run(&p);
    for rule in ["DRC-029", "DRC-030", "DRC-031"] {
        assert_eq!(r.state_of(rule), CheckState::NotApplicable, "{rule}");
        assert!(reason(&r, rule).contains("no declared high-speed net"));
    }
    // Plane zones and no declared neck width: the neck check is UNKNOWN, naming the input.
    assert_eq!(r.state_of("DRC-032"), CheckState::Unknown);
    assert!(reason(&r, "DRC-032").contains("plane_min_neck_width"));
    p.constraints.plane_min_neck_width = Some(500_000);
    assert_eq!(drc::run(&p).state_of("DRC-032"), CheckState::Pass);
    let empty = Project::new("empty");
    let r = drc::run(&empty);
    assert_eq!(r.state_of("DRC-032"), CheckState::NotApplicable);
}

#[test]
fn a_high_speed_track_across_the_split_fails_and_one_beside_it_passes() {
    let mut p = board("+3V3");
    declare_pair(&mut p);
    track(
        &mut p,
        "beside",
        CopperLayer::Bottom,
        (3.0, 15.0),
        (17.0, 15.0),
    );
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-029"), CheckState::Pass);
    assert_eq!(r.state_of("DRC-030"), CheckState::Pass);
    track(
        &mut p,
        "across",
        CopperLayer::Bottom,
        (5.0, 12.0),
        (35.0, 12.0),
    );
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-029"), CheckState::Fail);
    let f = r.findings.iter().find(|f| f.rule_id == "DRC-029").unwrap();
    assert_eq!(f.objects, vec!["across".to_string()]);
    assert!(
        f.message.contains("In2.Cu") && f.message.contains("+3V3"),
        "{}",
        f.message
    );
    assert!(f.suggested_fix.is_some());
}

#[test]
fn a_high_speed_track_with_no_reference_copper_fails() {
    let mut p = board("+3V3");
    declare_pair(&mut p);
    // In the gap between the two In2 zones: nothing under it.
    track(
        &mut p,
        "gap",
        CopperLayer::Bottom,
        (19.6, 2.0),
        (19.6, 18.0),
    );
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-030"), CheckState::Fail);
    let e = errors(&r, "DRC-030");
    assert!(e[0].contains("no plane copper under it on In2.Cu"), "{e:?}");
    // A two-layer board without a pour has no reference at all.
    let mut q = Project::new("two");
    q.pcb.board.stackup = Stackup::two_layer(1_600_000, 1.0, "FR-4", FR4_ER);
    declare_pair(&mut q);
    track(&mut q, "t", CopperLayer::Top, (2.0, 2.0), (8.0, 2.0));
    let e = errors(&drc::run(&q), "DRC-030");
    assert!(e[0].contains("no adjacent plane layer or pour"), "{e:?}");
}

/// A D_P change from Top to Bottom at (10, 10) mm.
fn transition(p: &mut Project) {
    declare_pair(p);
    track(p, "top", CopperLayer::Top, (2.0, 10.0), (10.0, 10.0));
    track(p, "bottom", CopperLayer::Bottom, (10.0, 10.0), (15.0, 10.0));
    via(p, "sig", (10.0, 10.0), "D_P", THROUGH);
}

#[test]
fn a_layer_change_between_ground_references_needs_a_return_via() {
    // Both references GND: return vias vl and vg exist, but no largest distance is declared.
    let mut p = board("GND");
    transition(&mut p);
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-031"), CheckState::Unknown);
    let why = reason(&r, "DRC-031");
    assert!(why.contains("nearest GND return via vl"), "{why}");
    assert!(
        why.contains("no largest return-via distance is declared"),
        "{why}"
    );
    // No GND via joins In1 and In2 (each plane is kept by a blind via): FAIL.
    let mut q = board("GND");
    q.pcb.vias.clear();
    via(&mut q, "g1", (30.0, 15.0), "GND", [CopperLayer::Top, IN1]);
    via(&mut q, "g2", (5.0, 5.0), "GND", [IN2, CopperLayer::Bottom]);
    via(&mut q, "v5", (35.0, 5.0), "+5V", THROUGH);
    transition(&mut q);
    let r = drc::run(&q);
    assert_eq!(r.state_of("DRC-031"), CheckState::Fail);
    let e = errors(&r, "DRC-031");
    assert!(
        e[0].contains("no via of GND joins reference planes"),
        "{e:?}"
    );
}

#[test]
fn a_layer_change_onto_a_power_reference_warns_and_stays_unknown() {
    let mut p = board("+3V3");
    transition(&mut p);
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-031"), CheckState::Unknown);
    let w = r.findings.iter().find(|f| f.rule_id == "DRC-031").unwrap();
    assert_eq!(w.severity, Severity::Warning);
    let want = "from GND (In1.Cu) to +3V3 (In2.Cu)";
    assert!(w.message.contains(want), "{}", w.message);
    assert!(reason(&r, "DRC-031").contains("stitching capacitor"));
}

#[test]
fn a_plane_zone_cut_in_two_is_reported() {
    let mut p = board("+3V3");
    p.constraints.plane_min_neck_width = Some(500_000);
    // A +5V track on In2 cuts the left zone top to bottom; a second +3V3 via keeps both halves.
    via(&mut p, "vl2", (15.0, 5.0), "+3V3", THROUGH);
    p.pcb.tracks.insert(
        "cut".into(),
        Track {
            id: "cut".into(),
            layer: IN2,
            width: 500_000,
            a: Point::mm(10.0, 0.5),
            b: Point::mm(10.0, 19.5),
            net: "+5V".into(),
            arc_deg: None,
        },
    );
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-032"), CheckState::Warning);
    let f = r.findings.iter().find(|f| f.rule_id == "DRC-032").unwrap();
    assert_eq!(f.objects, vec!["left".to_string()]);
    assert!(f.message.contains("split into 2 islands"), "{}", f.message);
}

#[test]
fn a_return_via_beyond_the_declared_distance_fails() {
    // The nearest GND via joining In1 and In2 is vl at (5, 5) mm, 7.07 mm from the change.
    let mut p = board("GND");
    transition(&mut p);
    p.constraints.return_via_max_distance = Some(8_000_000);
    assert_eq!(drc::run(&p).state_of("DRC-031"), CheckState::Pass);
    p.constraints.return_via_max_distance = Some(5_000_000);
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-031"), CheckState::Fail);
    let f = r.findings.iter().find(|f| f.rule_id == "DRC-031").unwrap();
    assert_eq!(f.objects, vec!["sig".to_string(), "vl".to_string()]);
    assert!(f.message.contains("7.07 mm away"), "{}", f.message);
}

#[test]
fn a_plane_narrower_than_the_declared_neck_width_fails() {
    // In1 GND: two 10 x 10 mm areas joined by a bridge 2 mm wide; a GND via keeps the fill.
    let mut p = Project::new("neck");
    p.pcb.board.outline = rect(0.0, 0.0, 32.0, 12.0);
    p.pcb.board.stackup = Stackup::multilayer(4, 1_600_000, 1.0, 0.5, "FR-4", FR4_ER);
    let dumbbell = [
        (1.0, 1.0),
        (11.0, 1.0),
        (11.0, 5.0),
        (21.0, 5.0),
        (21.0, 1.0),
        (31.0, 1.0),
        (31.0, 11.0),
        (21.0, 11.0),
        (21.0, 7.0),
        (11.0, 7.0),
        (11.0, 11.0),
        (1.0, 11.0),
    ];
    let outline = dumbbell.map(|(x, y)| Point::mm(x, y)).to_vec();
    zone(&mut p, "gnd", IN1, "GND", outline);
    via(&mut p, "vg", (5.0, 6.0), "GND", THROUGH);
    p.constraints.plane_min_neck_width = Some(4_000_000);
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-032"), CheckState::Fail);
    let f = r
        .findings
        .iter()
        .find(|f| f.rule_id == "DRC-032" && f.severity == Severity::Error)
        .expect("a neck finding");
    assert_eq!(f.objects, vec!["gnd".to_string()]);
    assert!(
        f.message.contains("narrows below the declared 4.00 mm"),
        "{}",
        f.message
    );
    let at = f.location.expect("the neck is located");
    assert!((11_000_000..21_000_000).contains(&at.x), "{at:?}");
    // The bridge is 2 mm wide: a 0.5 mm minimum passes.
    p.constraints.plane_min_neck_width = Some(500_000);
    assert_eq!(drc::run(&p).state_of("DRC-032"), CheckState::Pass);
}
