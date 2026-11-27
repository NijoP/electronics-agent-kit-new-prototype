//! Reference planes and return paths (M7.6) on the `eak_engineering::planes` model: DRC-029 a
//! declared high-speed track crossing a split in its reference plane, DRC-030 a declared
//! high-speed track with no reference copper beside it, DRC-031 a high-speed layer change with no
//! return via, DRC-032 a plane zone split into islands or narrower than the declared neck width.
//! With no declared high-speed net, DRC-029
//! to DRC-031 are NOT_APPLICABLE (never PASS); a return via without a declared largest distance
//! leaves DRC-031 UNKNOWN naming the missing input.

use crate::report::{finding, rule, Bound, Finding, RuleInfo, Severity::*};
use eak_core::geom::to_mm;
use eak_core::{LayerKind, Nm, Project};
use eak_engineering::board::BoardGeometry;
use eak_engineering::planes::{self, Planes, ReturnNeed};
use std::collections::BTreeSet;

pub(crate) fn rules() -> Vec<RuleInfo> {
    let src = "EAK reference-plane model (eak_engineering::planes, M7.6) on the zone fills of the board geometry (ADR-0052); declared high-speed nets = declared differential pairs and net classes with a target impedance";
    vec![
        rule(
            "DRC-029",
            "High-speed track over a reference-plane split",
            format!("centreline and both edges sampled every 0.1 mm over each reference layer (adjacent plane layer or pour); a change of reference net, or of fill island within one zone, or leaving the plane copper is a crossing; voids within {:.1} mm of fill (anti-pads) are not", to_mm(planes::VOID_REACH)),
            src,
        ),
        rule(
            "DRC-030",
            "High-speed track without a reference plane",
            "the track's layer has an adjacent plane layer or pour, and plane copper lies under some of the track",
            src,
        ),
        rule(
            "DRC-031",
            "Return via at a high-speed layer change",
            "where a high-speed via changes between layers with different nearest reference planes of one net, a via of that net joining both planes lies within the declared distance; a change of reference net needs a stitching capacitor (not checked)",
            src,
        ),
        rule(
            "DRC-032",
            "Plane zone split into islands or narrowed below the neck width",
            "every zone on a plane layer fills as one island (WARNING otherwise); no part of it is narrower than the declared plane_min_neck_width, found by eroding the 0.1 mm fill raster by half that width (ERROR where that splits an island; UNKNOWN while no width is declared)",
            src,
        ),
    ]
}

/// Rule states the checks could not settle: (rule, why) to degrade to UNKNOWN, and (rule, why)
/// with nothing to evaluate.
#[derive(Default)]
pub(crate) struct Gaps {
    pub unknown: Vec<(&'static str, String)>,
    pub not_applicable: Vec<(&'static str, String)>,
}

/// DRC-029..032 findings against the declared limits (ADR-0059): `return_via_max_distance`
/// (undeclared: DRC-031 is UNKNOWN wherever a return via is present) and `plane_min_neck_width`
/// (undeclared: DRC-032 is UNKNOWN on a board with plane zones).
pub(crate) fn check(p: &Project, geo: &BoardGeometry, f: &mut Vec<Finding>) -> Gaps {
    let mut gaps = Gaps::default();
    let return_max = p.constraints.return_via_max_distance;
    let neck = p.constraints.plane_min_neck_width;
    let st = &p.pcb.board.stackup;
    let model = Planes::new(p, geo);

    // DRC-032: plane zones fill as one island.
    let mut plane_fills = 0;
    for fill in geo
        .fills
        .iter()
        .filter(|z| st.kind_of(z.layer) == Some(LayerKind::Plane))
    {
        plane_fills += 1;
        if fill.islands > 1 {
            let (z, n) = (&fill.zone_id, fill.islands);
            let msg = format!(
                "plane zone {z} ({} on {}) is split into {n} islands",
                fill.net,
                fill.layer.name()
            );
            f.push(
                finding("DRC-032", Warning, msg, vec![z.clone()], None)
                    .measured(f64::from(n), "islands")
                    .required(Bound::Max, 1.0, "islands")
                    .fix(format!("Move the copper that cuts zone {z} off its layer, or join the islands, so the plane is continuous.")),
            );
        }
        for (island, at) in neck.map_or(vec![], |w| planes::narrow_necks(fill, w)) {
            let (z, w_mm) = (&fill.zone_id, to_mm(neck.unwrap_or_default()));
            let msg = format!(
                "plane zone {z} ({} on {}) narrows below the declared {w_mm:.2} mm neck width (island {island})",
                fill.net,
                fill.layer.name()
            );
            f.push(
                finding("DRC-032", Error, msg, vec![z.clone()], Some(at))
                    .key(&format!("neck-{island}"))
                    .required(Bound::Min, w_mm, "mm")
                    .fix(format!("Widen zone {z} there, or move the copper or anti-pads that pinch it, so the plane is at least {w_mm:.2} mm wide.")),
            );
        }
    }
    if plane_fills == 0 {
        gaps.not_applicable
            .push(("DRC-032", "no zone on a plane layer".into()));
    }
    if plane_fills > 0 && neck.is_none() {
        gaps.unknown.push((
            "DRC-032",
            "plane neck width not checked: no plane_min_neck_width is declared (SetEngineeringInputs)".into(),
        ));
    }

    let hs = planes::declared_high_speed(p);
    if hs.is_empty() {
        let why =
            "no declared high-speed net (differential pair or net class with a target impedance)";
        for r in ["DRC-029", "DRC-030", "DRC-031"] {
            gaps.not_applicable.push((r, why.into()));
        }
        return gaps;
    }

    // DRC-029 / DRC-030: every declared high-speed track.
    for t in p.pcb.tracks.values().filter(|t| hs.contains(&t.net)) {
        let refs = planes::reference_layers(p, t.layer);
        let path = t.path();
        let seen: Vec<_> = refs
            .iter()
            .map(|&pl| model.along(pl, &path, t.width))
            .collect();
        if seen.iter().all(|s| s.absent) {
            let why = if refs.is_empty() {
                format!("{} has no adjacent plane layer or pour", t.layer.name())
            } else {
                let names: Vec<String> = refs.iter().map(|l| l.name()).collect();
                format!("no plane copper under it on {}", names.join(", "))
            };
            let msg = format!(
                "high-speed track {} ({}) on {}: {why}",
                t.id,
                t.net,
                t.layer.name()
            );
            f.push(
                finding("DRC-030", Error, msg, vec![t.id.clone()], Some(t.a)).fix(format!(
                    "Route {} on a layer next to a plane, or pour reference copper under it.",
                    t.net
                )),
            );
            continue;
        }
        for s in seen.iter().filter(|s| !s.absent) {
            let Some(c) = s.crossings.first() else {
                continue;
            };
            let msg = format!(
                "high-speed track {} ({}) on {} crosses a split in reference plane {}: {} to {} ({} reference change(s))",
                t.id,
                t.net,
                t.layer.name(),
                s.plane.name(),
                c.from.label(),
                c.to.label(),
                s.crossings.len()
            );
            f.push(
                finding("DRC-029", Error, msg, vec![t.id.clone()], Some(c.at))
                    .key(&s.plane.name())
                    .fix(format!(
                        "Re-route {} around the split in {}, or on a layer whose reference is continuous.",
                        t.net,
                        s.plane.name()
                    )),
            );
        }
    }

    // DRC-031: every layer change of a declared high-speed net.
    let mut unknown: Vec<String> = vec![];
    for v in p.pcb.vias.values().filter(|v| hs.contains(&v.net)) {
        let r = v.diameter as f64 / 2.0;
        let used: BTreeSet<usize> = p
            .pcb
            .tracks
            .values()
            .filter(|t| t.net == v.net)
            .filter(|t| t.a.dist(v.position) <= r || t.b.dist(v.position) <= r)
            .filter_map(|t| st.index_of(t.layer))
            .collect();
        let used: Vec<usize> = used.into_iter().collect();
        for (k, &i) in used.iter().enumerate() {
            for &j in &used[k + 1..] {
                let (la, lb) = (st.layers[i].layer, st.layers[j].layer);
                let change = format!(
                    "via {} ({}) from {} to {}",
                    v.id,
                    v.net,
                    la.name(),
                    lb.name()
                );
                let key = format!("{}-{}", la.name(), lb.name());
                match model.return_need(v.position, la, lb) {
                    ReturnNeed::Nothing => {}
                    ReturnNeed::NoCopper { plane } => {
                        let msg = format!(
                            "{change}: no copper of reference plane {} at the via",
                            plane.name()
                        );
                        f.push(
                            finding("DRC-031", Error, msg, vec![v.id.clone()], Some(v.position))
                                .key(&key)
                                .fix(format!(
                                    "Move via {} over continuous reference copper.",
                                    v.id
                                )),
                        );
                    }
                    ReturnNeed::Capacitor { nets, planes } => {
                        let msg = format!(
                            "{change}: the reference changes from {} ({}) to {} ({}); no via can carry the return",
                            nets[0],
                            planes[0].name(),
                            nets[1],
                            planes[1].name()
                        );
                        f.push(
                            finding("DRC-031", Warning, msg, vec![v.id.clone()], Some(v.position))
                                .key(&key)
                                .fix(format!(
                                    "Place a stitching capacitor between {} and {} next to via {}, or keep {} on layers that share a reference.",
                                    nets[0], nets[1], v.id, v.net
                                )),
                        );
                        unknown.push(format!(
                            "{change}: a stitching capacitor between {} and {} is not checked",
                            nets[0], nets[1]
                        ));
                    }
                    ReturnNeed::Via { net, planes } => {
                        let nearest = p
                            .pcb
                            .vias
                            .values()
                            .filter(|o| o.net == net)
                            .filter(|o| {
                                let ls = o.layers(st);
                                ls.contains(&planes[0]) && ls.contains(&planes[1])
                            })
                            .map(|o| (o.position.dist(v.position), o))
                            .min_by(|a, b| a.0.total_cmp(&b.0));
                        let (p0, p1) = (planes[0].name(), planes[1].name());
                        match (nearest, return_max) {
                            (None, _) => {
                                let msg = format!("{change}: no via of {net} joins reference planes {p0} and {p1}; the return current has no path");
                                f.push(
                                    finding("DRC-031", Error, msg, vec![v.id.clone()], Some(v.position))
                                        .key(&key)
                                        .fix(format!("Place a {net} via joining {p0} and {p1} next to via {}.", v.id)),
                                );
                            }
                            (Some((d, o)), Some(max)) if d > max as f64 => {
                                let (d_mm, max_mm) = (to_mm(d as Nm), to_mm(max));
                                let msg = format!("{change}: the nearest {net} return via {} is {d_mm:.2} mm away", o.id);
                                f.push(
                                    finding("DRC-031", Error, msg, vec![v.id.clone(), o.id.clone()], Some(v.position))
                                        .key(&key)
                                        .measured(d_mm, "mm")
                                        .required(Bound::Max, max_mm, "mm")
                                        .fix(format!("Place a {net} via within {max_mm:.2} mm of via {}.", v.id)),
                                );
                            }
                            (Some(_), Some(_)) => {}
                            (Some((d, o)), None) => unknown.push(format!(
                                "{change}: nearest {net} return via {} at {:.2} mm, and no largest return-via distance is declared",
                                o.id,
                                to_mm(d as Nm)
                            )),
                        }
                    }
                }
            }
        }
    }
    if !unknown.is_empty() {
        gaps.unknown.push(("DRC-031", unknown.join("; ")));
    }
    gaps
}
