//! Slice C3 acceptance: push mode never creates a DRC-002 (clearance) or DRC-003 (short)
//! violation. Over generated fixture boards (rows of tracks of several nets, a route drawn
//! close beside one of them), the route step's result is committed as the route tool commits
//! it (moved tracks replaced by their polylines, route segments added) and the engine's DRC
//! is run on the result. Written without running (no local builds in this phase): unrun.

use eak_core::{CopperLayer, Nm, Point, Project, Track};
use eak_engineering::interactive::{route_step, BendMode, ConflictMode, RouteStepRequest};
use eak_validation::drc;

fn board() -> Project {
    let mut p = Project::new("push-shove");
    p.pcb.board.outline = vec![
        Point::mm(0.0, 0.0),
        Point::mm(50.0, 0.0),
        Point::mm(50.0, 40.0),
        Point::mm(0.0, 40.0),
    ];
    p
}

fn add(p: &mut Project, id: &str, net: &str, a: Point, b: Point, width: Nm) {
    let t = Track {
        id: id.into(),
        layer: CopperLayer::Top,
        width,
        a,
        b,
        net: net.into(),
    };
    p.pcb.tracks.insert(id.into(), t);
}

/// The DRC-002 and DRC-003 findings of a board.
fn copper_errors(p: &Project) -> Vec<String> {
    let report = drc::run(p);
    let mut out = Vec::new();
    for f in report.findings {
        if f.rule_id == "DRC-002" || f.rule_id == "DRC-003" {
            out.push(f.message);
        }
    }
    out
}

#[test]
fn push_mode_never_creates_a_clearance_or_short_violation() {
    let mut seed: u64 = 0xc3_5eed_0042;
    let mut rnd = |lo: f64, hi: f64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        lo + (hi - lo) * ((seed >> 11) as f64 / (1u64 << 53) as f64)
    };
    let mut pushed = 0;
    for case in 0..40 {
        let mut p = board();
        let mut rows = Vec::new();
        for k in 0..6 {
            let y = 6.0 + 4.0 * k as f64 + rnd(0.0, 0.5);
            let (x0, x1) = (rnd(2.0, 8.0), rnd(42.0, 48.0));
            let w = rnd(150_000.0, 300_000.0) as Nm;
            let (a, b) = (Point::mm(x0, y), Point::mm(x1, y));
            add(&mut p, &format!("t{k}"), &format!("N{k}"), a, b, w);
            rows.push(y);
        }
        let clean = copper_errors(&p).is_empty();
        assert!(clean, "case {case}: the fixture is clean");

        // A route close beside one row (closer than width + clearance), on either side.
        let row = rows[case % rows.len()];
        let off = rnd(0.25, 0.4);
        let y = if case % 2 == 0 { row + off } else { row - off };
        let from = Point::mm(rnd(12.0, 16.0), y);
        let to = Point::mm(rnd(30.0, 36.0), y + rnd(-1.0, 1.0));
        let req = RouteStepRequest {
            net: "SIG".into(),
            layer: CopperLayer::Top,
            from,
            to,
            mode: BendMode::Diagonal45,
            width: None,
            via_at_start: false,
            conflict: ConflictMode::Push,
            via: None,
            pending: Default::default(),
        };
        let r = route_step(&p, &req);
        if !r.shoved.is_empty() {
            pushed += 1;
        }

        // Commit as the route tool does: moved tracks replaced, then the route added.
        for s in &r.shoved {
            let gone = p.pcb.tracks.remove(&s.id).is_some();
            assert!(gone, "case {case}: {}", s.id);
            for (k, w) in s.points.windows(2).enumerate() {
                let id = format!("{}_{k}", s.id);
                add(&mut p, &id, &s.net, w[0], w[1], s.width);
            }
        }
        for (k, g) in r.segments.iter().enumerate() {
            add(&mut p, &format!("route{k}"), "SIG", g.a, g.b, r.width);
        }
        let errors = copper_errors(&p);
        assert!(errors.is_empty(), "case {case}: {errors:?}\n{r:?}");
    }
    assert!(pushed >= 5, "only {pushed} of 40 cases pushed");
}
