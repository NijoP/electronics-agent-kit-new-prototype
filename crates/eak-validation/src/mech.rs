//! Mechanical / 3D checks: component height limits, body collisions, missing models.

use crate::report::{finding, rule, Report, Severity::*};
use eak_core::{geom::to_mm, Project};
use eak_engineering::scene3d::body_rect;

pub fn run(p: &Project) -> Report {
    let mut f = Vec::new();
    let fps: Vec<_> = p.pcb.footprints.values().collect();
    let mut bodies = Vec::new();
    for fp in &fps {
        match body_rect(p, &fp.id) {
            Some((r, h)) => {
                if let Some(maxh) = p.constraints.max_component_height {
                    if h as i64 > maxh {
                        f.push(finding(
                            "MECH-001",
                            Error,
                            format!(
                                "{} is {:.2} mm tall; limit {:.2} mm",
                                fp.refdes,
                                to_mm(h as i64),
                                to_mm(maxh)
                            ),
                            vec![fp.id.clone()],
                            Some(fp.position),
                        ));
                    }
                }
                bodies.push((fp, r));
            }
            None => f.push(finding(
                "MECH-003",
                Warning,
                format!(
                    "{} has no 3D body height; mechanical fit is UNKNOWN",
                    fp.refdes
                ),
                vec![fp.id.clone()],
                Some(fp.position),
            )),
        }
    }
    for i in 0..bodies.len() {
        for j in i + 1..bodies.len() {
            let ((a, ra), (b, rb)) = (&bodies[i], &bodies[j]);
            if a.side == b.side && ra.intersects(rb) {
                f.push(finding(
                    "MECH-002",
                    Error,
                    format!("3D bodies of {} and {} collide", a.refdes, b.refdes),
                    vec![a.id.clone(), b.id.clone()],
                    Some(ra.center()),
                ));
            }
        }
    }
    let rules = vec![
        rule(
            "MECH-001",
            "Component height limit",
            p.constraints
                .max_component_height
                .map_or("none declared".into(), |h| format!("{:.2} mm", to_mm(h))),
            "Project constraints",
        ),
        rule(
            "MECH-002",
            "3D body collision",
            "parametric bodies (85 % of courtyard)",
            "EAK",
        ),
        rule("MECH-003", "Missing 3D model data", "", "EAK"),
    ];
    Report::new(
        "MECH",
        p,
        rules,
        f,
        vec!["Enclosure fit (no enclosure model)".into(), "STEP model import and exact-solid collision".into()],
        vec!["Bodies are parametric boxes derived from library height and courtyard; real package shapes may differ".into()],
    )
}
