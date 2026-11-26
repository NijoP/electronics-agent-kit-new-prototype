//! Mechanical / 3D checks: component height limits, body collisions, missing models.

use crate::report::{finding, rule, Bound, Finding, Report, Severity::*};
use crate::state::CheckState;
use eak_core::{geom::to_mm, FootprintInstance, MountingHoleSpec, Point, Project, Rect, Side};
use eak_engineering::board::holes;
use eak_engineering::copper::{self, Core, Shape};
use eak_engineering::scene3d::body_rect;
use std::collections::BTreeSet;

pub fn run(p: &Project) -> Report {
    let c = &p.constraints;
    let enclosure = c.enclosure.as_ref();
    // Bottom-side parts: the enclosure's bottom limit, else the board-wide height limit.
    let bottom = enclosure.and_then(|e| e.max_height_bottom);
    let bottom = bottom.or(c.max_component_height);
    let mut f = Vec::new();
    let fps: Vec<_> = p.pcb.footprints.values().collect();
    let mut bodies = Vec::new();
    for &fp in &fps {
        match body_rect(p, &fp.id) {
            Some((r, h)) => {
                let limit = match fp.side {
                    Side::Top => c.max_component_height,
                    Side::Bottom => bottom,
                };
                if let Some(maxh) = limit {
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
    if let Some(e) = enclosure {
        mounting_holes(p, &e.mounting_holes, &bodies, &mut f);
    }
    let rules = vec![
        rule(
            "MECH-001",
            "Component height limit",
            format!(
                "top {}, bottom {}",
                limit_mm(c.max_component_height),
                limit_mm(bottom)
            ),
            "Project constraints (bottom: enclosure max_height_bottom, else the board-wide limit)",
        ),
        rule(
            "MECH-002",
            "3D body collision",
            "parametric bodies (85 % of courtyard)",
            "EAK",
        ),
        rule("MECH-003", "Missing 3D model data", "", "EAK"),
        rule(
            "MECH-004",
            "Enclosure mounting holes",
            "a drilled hole within each declared position tolerance, at least the declared drill, and no other copper (tracks, pads, vias) or part body inside the keep-out radius",
            "Project enclosure declaration (ADR-0055)",
        ),
    ];
    let mut limitations: Vec<String> = vec!["Bodies are parametric boxes derived from library height and courtyard; real package shapes may differ".into()];
    limitations.push("MECH-004 keep-outs check tracks, pads, vias and part bodies; zone fills are not checked against them".into());
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
            "Enclosure solid-model fit (only declared height limits and mounting holes are checked)".into(),
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
    if enclosure.is_none() {
        let why = "no enclosure declared (Constraints.enclosure): mounting holes not checked";
        report = report.degrade("MECH-004", CheckState::Unknown, why);
    }
    if missing_bodies > 0 {
        let why = format!("{missing_bodies} part(s) have no 3D body data");
        report = report
            .degrade("MECH-001", CheckState::Unknown, &why)
            .degrade("MECH-002", CheckState::Unknown, &why)
            .degrade("MECH-003", CheckState::Unknown, &why);
        if enclosure.is_some_and(|e| !e.mounting_holes.is_empty()) {
            report = report.degrade("MECH-004", CheckState::Unknown, &why);
        }
    }
    report
}

fn limit_mm(h: Option<i64>) -> String {
    h.map_or("none declared".into(), |h| format!("{:.2} mm", to_mm(h)))
}

/// Edge distance (nm) from `p` to `r`; 0 inside.
fn rect_distance(r: &Rect, p: Point) -> f64 {
    let dx = (r.min.x - p.x).max(p.x - r.max.x).max(0) as f64;
    let dy = (r.min.y - p.y).max(p.y - r.max.y).max(0) as f64;
    dx.hypot(dy)
}

/// MECH-004: each declared mounting hole needs a drilled hole within its position tolerance and
/// at least its drill, and nothing but that hole's own part inside its keep-out radius.
fn mounting_holes(
    p: &Project,
    specs: &[MountingHoleSpec],
    bodies: &[(&FootprintInstance, Rect)],
    f: &mut Vec<Finding>,
) {
    if specs.is_empty() {
        return;
    }
    let (drilled, items) = (holes(p), copper::items(p));
    for (i, s) in specs.iter().enumerate() {
        let (x, y) = (to_mm(s.position.x), to_mm(s.position.y));
        let at = format!("mounting hole {} at ({x:.2}, {y:.2}) mm", i + 1);
        let key = format!("hole{i}");
        let tol = s.tolerance as f64;
        let near = drilled.iter().filter(|h| h.center.dist(s.position) <= tol);
        let hole = near.max_by_key(|h| h.drill);
        let owner = hole.map(|h| h.object_id.as_str());
        match hole {
            None => f.push(
                finding(
                    "MECH-004",
                    Error,
                    format!("no drilled hole at {at}"),
                    vec![],
                    Some(s.position),
                )
                .key(&key)
                .fix(format!(
                    "Place a mounting-hole part or via with a {:.2} mm drill within {:.2} mm of ({x:.2}, {y:.2}) mm.",
                    to_mm(s.drill),
                    to_mm(s.tolerance)
                )),
            ),
            Some(h) if h.drill < s.drill => f.push(
                finding(
                    "MECH-004",
                    Error,
                    format!("{} drill is smaller than {at} needs", h.label),
                    vec![h.object_id.clone()],
                    Some(h.center),
                )
                .key(&key)
                .measured(to_mm(h.drill), "mm")
                .required(Bound::Min, to_mm(s.drill), "mm")
                .fix(format!(
                    "Use a drill of at least {:.2} mm for {}.",
                    to_mm(s.drill),
                    h.label
                )),
            ),
            Some(_) => {}
        }
        let keepout = Shape {
            core: Core::Segment(s.position, s.position),
            radius: s.keepout_radius as f64,
        };
        let mut seen = BTreeSet::new();
        for it in &items {
            let mine = Some(it.object_id.as_str()) == owner;
            let clear = copper::distance(&it.shape, &keepout) >= 0.0;
            if mine || clear || !seen.insert(it.label()) {
                continue;
            }
            let msg = format!("{} is inside the keep-out of {at}", it.label());
            let objs = vec![it.object_id.clone()];
            f.push(
                finding("MECH-004", Error, msg, objs, Some(s.position))
                    .key(&format!("{key}-{}", it.label()))
                    .fix(format!(
                        "Move {} at least {:.2} mm from the hole centre.",
                        it.label(),
                        to_mm(s.keepout_radius)
                    )),
            );
        }
        let radius = s.keepout_radius as f64;
        for (fp, r) in bodies {
            if Some(fp.id.as_str()) == owner || rect_distance(r, s.position) >= radius {
                continue;
            }
            let msg = format!("{} body is inside the keep-out of {at}", fp.refdes);
            let objs = vec![fp.id.clone()];
            f.push(
                finding("MECH-004", Error, msg, objs, Some(s.position))
                    .key(&format!("{key}-{}", fp.id))
                    .fix(format!("Move {} clear of the hole keep-out.", fp.refdes)),
            );
        }
    }
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
