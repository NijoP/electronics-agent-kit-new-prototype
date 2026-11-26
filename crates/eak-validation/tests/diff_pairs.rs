//! Declared differential pairs (ADR-0055): DRC-024 skew, DRC-025 gap and DRC-026 impedance,
//! PASS, FAIL and UNKNOWN.

use eak_core::geom::mm;
use eak_core::{CopperLayer, DiffPairRule, Point, Project, Stackup, Track, FR4_ER};
use eak_engineering::impedance;
use eak_validation::{drc, CheckState, Report};

fn track(id: &str, net: &str, y: f64, len: f64) -> Track {
    Track {
        id: id.into(),
        layer: CopperLayer::Top,
        width: mm(0.15),
        a: Point::mm(0.0, y),
        b: Point::mm(len, y),
        net: net.into(),
        arc_deg: None,
    }
}

/// A 4-layer board with P and N side by side on Top, 0.15 mm wide and 0.15 mm apart.
fn board(n_len: f64, rule: DiffPairRule) -> Project {
    let mut p = Project::new("pairs");
    p.pcb.board.stackup = Stackup::multilayer(4, 1_600_000, 1.0, 0.5, "FR-4", FR4_ER);
    p.pcb.tracks.insert("tp".into(), track("tp", "P", 0.0, 20.0));
    p.pcb.tracks.insert("tn".into(), track("tn", "N", 0.3, n_len));
    p.constraints.diff_pairs = vec![rule];
    p
}

fn pair() -> DiffPairRule {
    DiffPairRule {
        positive: "P".into(),
        negative: "N".into(),
        width: None,
        gap: None,
        target_zdiff_ohm: None,
        zdiff_tolerance_pct: None,
        max_skew: None,
    }
}

fn states(r: &Report) -> [CheckState; 3] {
    ["DRC-024", "DRC-025", "DRC-026"].map(|id| r.state_of(id))
}

fn reason(r: &Report, rule: &str) -> String {
    let rr = r.rule_results.iter().find(|x| x.rule_id == rule).unwrap();
    rr.reason.clone()
}

/// The estimate DRC-026 should find for the board's 0.15 / 0.15 mm pair on Top.
fn estimate(p: &Project) -> f64 {
    let st = &p.pcb.board.stackup;
    let g = impedance::geometry(st, CopperLayer::Top).unwrap();
    impedance::zdiff(g, mm(0.15), mm(0.15)).ohm().unwrap()
}

#[test]
fn undeclared_figures_are_unknown_and_no_pair_is_nothing_to_check() {
    let mut p = board(20.0, pair());
    let r = drc::run(&p);
    assert_eq!(states(&r), [CheckState::Unknown; 3]);
    let skew = reason(&r, "DRC-024");
    assert!(skew.contains("P/N: no max skew declared"), "{skew}");
    let z = reason(&r, "DRC-026");
    assert!(z.contains("no target impedance declared"), "{z}");
    p.constraints.diff_pairs.clear();
    assert_eq!(states(&drc::run(&p)), [CheckState::Pass; 3]);
    // A declared member without copper cannot be checked.
    let mut p = board(20.0, pair());
    p.pcb.tracks.remove("tn");
    let r = drc::run(&p);
    assert!(reason(&r, "DRC-025").contains("a member has no tracks"));
}

#[test]
fn skew_gap_and_impedance_pass_and_fail() {
    let mut rule = pair();
    rule.max_skew = Some(mm(0.1));
    rule.gap = Some(mm(0.15));
    rule.zdiff_tolerance_pct = Some(10.0);
    let mut p = board(20.0, rule.clone());
    let z = estimate(&p);
    p.constraints.diff_pairs[0].target_zdiff_ohm = Some(z);
    assert_eq!(states(&drc::run(&p)), [CheckState::Pass; 3]);
    // N 0.5 mm longer, a 0.2 mm gap declared and a target 50 % above the estimate: all fail.
    rule.gap = Some(mm(0.2));
    rule.target_zdiff_ohm = Some(z * 1.5);
    let r = drc::run(&board(20.5, rule));
    assert_eq!(states(&r), [CheckState::Fail; 3]);
    let skew = r.findings.iter().find(|f| f.rule_id == "DRC-024").unwrap();
    assert!((skew.measured.as_ref().unwrap().value - 0.5).abs() < 1e-6);
    let zf = r.findings.iter().find(|f| f.rule_id == "DRC-026").unwrap();
    assert!((zf.measured.as_ref().unwrap().value - z).abs() < 1e-6);
}

#[test]
fn impedance_without_a_reference_plane_is_unknown() {
    let mut rule = pair();
    rule.target_zdiff_ohm = Some(90.0);
    rule.zdiff_tolerance_pct = Some(10.0);
    let mut p = board(20.0, rule);
    p.pcb.board.stackup = Stackup::two_layer(1_600_000, 1.0, "FR-4", FR4_ER);
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-026"), CheckState::Unknown);
    let why = reason(&r, "DRC-026");
    assert!(why.contains("track tp needs"), "{why}");
}
