//! Mechanical / 3D checks: component height limits, body collisions, missing models.

use crate::report::{finding, rule, Bound, Report, Severity::*};
use crate::state::CheckState;
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
                        f.push(
                            finding(
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
                            )
                            .measured(to_mm(h as i64), "mm")
                            .required(Bound::Max, to_mm(maxh), "mm")
                            .fix(format!(
                                "Choose a lower-profile part or package for {} (at most {:.2} mm tall), or raise the declared height limit if the enclosure allows it.",
                                fp.refdes,
                                to_mm(maxh)
                            )),
                        );
                    }
                }
                bodies.push((fp, r));
            }
            None => f.push(
                finding(
                    "MECH-003",
                    Warning,
                    format!(
                        "{} has no 3D body height; mechanical fit is UNKNOWN",
                        fp.refdes
                    ),
                    vec![fp.id.clone()],
                    Some(fp.position),
                )
                .fix(format!(
                    "Add the body height to the library part used by {} so its mechanical fit can be checked.",
                    fp.refdes
                )),
            ),
        }
    }
    for i in 0..bodies.len() {
        for j in i + 1..bodies.len() {
            let ((a, ra), (b, rb)) = (&bodies[i], &bodies[j]);
            if a.side == b.side && ra.intersects(rb) {
                f.push(
                    finding(
                        "MECH-002",
                        Error,
                        format!("3D bodies of {} and {} collide", a.refdes, b.refdes),
                        vec![a.id.clone(), b.id.clone()],
                        Some(ra.center()),
                    )
                    .fix(format!(
                        "Move or rotate {} or {} until their bodies no longer overlap, or place one of them on the other side of the board.",
                        a.refdes, b.refdes
                    )),
                );
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
    let mut limitations: Vec<String> = vec!["Bodies are parametric boxes derived from library height and courtyard; real package shapes may differ".into()];
    if p.constraints.max_component_height.is_none() {
        limitations.push("NOT EVALUATED: MECH-001 height limit (no limit declared); mechanical fit is unknown and blocks release above DRAFT".into());
    }
    let missing_bodies = fps.len() - bodies.len();
    let mut report = Report::new(
        "MECH",
        p,
        rules,
        f,
        vec![
            "Enclosure fit (no enclosure model)".into(),
            "STEP model import and exact-solid collision".into(),
        ],
        limitations,
    );
    // Unknown mechanical fit is never a pass (EVALUATION C2): no declared height limit, or a
    // part without 3D body data, leaves the affected rules UNKNOWN.
    if p.constraints.max_component_height.is_none() {
        report = report.degrade(
            "MECH-001",
            CheckState::Unknown,
            "no component height limit declared",
        );
    }
    if missing_bodies > 0 {
        let why = format!("{missing_bodies} part(s) have no 3D body data");
        report = report
            .degrade("MECH-001", CheckState::Unknown, &why)
            .degrade("MECH-002", CheckState::Unknown, &why)
            .degrade("MECH-003", CheckState::Unknown, &why);
    }
    report
}

#[cfg(test)]
mod tests {
    // Written during the testing freeze; never compiled or run.
    use super::*;

    #[test]
    fn undeclared_height_limit_is_unknown_not_pass() {
        let p = Project::new("m");
        let r = run(&p);
        assert_eq!(r.state_of("MECH-001"), CheckState::Unknown);
        assert!(!r.overall.certifies());
    }

    #[test]
    fn report_is_stale_after_a_new_revision() {
        let mut p = Project::new("m");
        let r = run(&p);
        assert_eq!(r.state_for(&p), r.overall);
        p.revision += 1;
        assert_eq!(r.state_for(&p), CheckState::Unknown);
    }
}
