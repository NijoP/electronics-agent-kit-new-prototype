//! Declared differential pairs (`Constraints::diff_pairs`, ADR-0055): DRC-024 length skew,
//! DRC-025 gap and DRC-026 differential impedance from the stack-up. A figure the pair does not
//! declare, or a member without tracks, leaves the rule UNKNOWN naming it, never PASS.

use crate::report::{finding, rule, Bound, Finding, RuleInfo, Severity::*};
use eak_core::geom::{segment_segment_distance, to_mm};
use eak_core::{DiffPairRule, Nm, Project, Track};
use eak_engineering::impedance::{self, Estimate};
use std::collections::BTreeMap;

pub(crate) fn rules() -> Vec<RuleInfo> {
    let src = "Project differential-pair declarations (ADR-0055)";
    vec![
        rule(
            "DRC-024",
            "Differential pair length skew",
            "difference of the members' summed track lengths (vias not counted) ≤ the declared max skew",
            src,
        ),
        rule(
            "DRC-025",
            "Differential pair gap",
            "closest edge-to-edge distance between the members' tracks on a shared layer ≥ the declared gap",
            src,
        ),
        rule(
            "DRC-026",
            "Differential pair impedance",
            "each coupled positive-member track (a negative track on its layer within twice the declared gap, else twice its width): Zdiff from its width, that gap and the stack-up, within the declared target ± tolerance",
            "IPC-2141A eq. 4-9 / 4-10 estimates via eak_engineering::impedance (typically within ±10 % of a field solver); the fabricator's controlled-impedance stack-up is authoritative",
        ),
    ]
}

/// Edge-to-edge distance between two tracks (nm); ≤ 0 when they touch.
fn gap(a: &Track, b: &Track) -> f64 {
    let (pa, pb) = (a.path(), b.path());
    let mut best = f64::INFINITY;
    for s in pa.windows(2) {
        for t in pb.windows(2) {
            best = best.min(segment_segment_distance(s[0], s[1], t[0], t[1]));
        }
    }
    best - (a.width + b.width) as f64 / 2.0
}

fn tracks<'a>(p: &'a Project, net: &str) -> Vec<&'a Track> {
    p.pcb.tracks.values().filter(|t| t.net == net).collect()
}

/// DRC-024..026 findings for every declared pair, and per rule why a pair was not evaluated.
pub(crate) fn check(p: &Project, f: &mut Vec<Finding>) -> Vec<(&'static str, String)> {
    let mut unknown: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for pair in &p.constraints.diff_pairs {
        let mut note = |r: &'static str, w: String| unknown.entry(r).or_default().push(w);
        check_pair(p, pair, f, &mut note);
    }
    unknown
        .into_iter()
        .map(|(rule, why)| (rule, why.join("; ")))
        .collect()
}

fn check_pair(
    p: &Project,
    pair: &DiffPairRule,
    f: &mut Vec<Finding>,
    note: &mut impl FnMut(&'static str, String),
) {
    let name = format!("{}/{}", pair.positive, pair.negative);
    let (pos, neg) = (tracks(p, &pair.positive), tracks(p, &pair.negative));
    if pos.is_empty() || neg.is_empty() {
        for rule in ["DRC-024", "DRC-025", "DRC-026"] {
            note(rule, format!("{name}: a member has no tracks"));
        }
        return;
    }
    // DRC-024: length skew.
    let length = |ts: &[&Track]| ts.iter().map(|t| t.length()).sum::<f64>();
    let skew = (length(&pos) - length(&neg)).abs();
    let skew_mm = to_mm(skew as Nm);
    let ids: Vec<String> = pos.iter().chain(&neg).map(|t| t.id.clone()).collect();
    match pair.max_skew {
        None => note("DRC-024", format!("{name}: no max skew declared")),
        Some(max) if skew > max as f64 => {
            let msg = format!("pair {name}: members differ in length by {skew_mm:.3} mm");
            let max_mm = to_mm(max);
            f.push(
                finding("DRC-024", Error, msg, ids, None)
                    .key(&name)
                    .measured(skew_mm, "mm")
                    .required(Bound::Max, max_mm, "mm")
                    .fix(format!(
                        "Tune the shorter member of {name} (add length) until the two are within {max_mm:.3} mm."
                    )),
            )
        }
        Some(_) => {}
    }
    // DRC-025: the closest approach of the members on a shared layer.
    let mut closest: Option<(f64, &Track, &Track)> = None;
    for a in &pos {
        for b in neg.iter().filter(|b| b.layer == a.layer) {
            let g = gap(a, b);
            if closest.is_none_or(|c| g < c.0) {
                closest = Some((g, *a, *b));
            }
        }
    }
    match (pair.gap, closest) {
        (None, _) => note("DRC-025", format!("{name}: no gap declared")),
        (Some(_), None) => note("DRC-025", format!("{name}: the members share no layer")),
        // 1 nm of slack for the rounding of track coordinates.
        (Some(want), Some((g, a, b))) if g + 1.0 < want as f64 => {
            let (ta, tb, want_mm) = (&a.id, &b.id, to_mm(want));
            let msg = format!("pair {name}: tracks {ta} and {tb} are closer than the pair gap");
            let objs = vec![ta.clone(), tb.clone()];
            f.push(
                finding("DRC-025", Error, msg, objs, Some(a.a))
                    .key(&name)
                    .mm_min(to_mm(g as Nm), want_mm)
                    .fix(format!(
                        "Space {ta} and {tb} {want_mm:.3} mm apart (edge to edge)."
                    )),
            )
        }
        (Some(_), Some(_)) => {}
    }
    // DRC-026: differential impedance of each coupled positive track.
    let (Some(target), Some(tol)) = (pair.target_zdiff_ohm, pair.zdiff_tolerance_pct) else {
        let missing = match pair.target_zdiff_ohm {
            None => "no target impedance declared",
            Some(_) => "no impedance tolerance declared",
        };
        note("DRC-026", format!("{name}: {missing}"));
        return;
    };
    let st = &p.pcb.board.stackup;
    let mut coupled = 0;
    for a in &pos {
        let reach = 2.0 * pair.gap.unwrap_or(a.width) as f64;
        let on_layer = neg.iter().filter(|b| b.layer == a.layer);
        let near = on_layer.map(|b| (gap(a, b), *b));
        let coupled_to = near.filter(|(g, _)| *g <= reach);
        let Some((g, b)) = coupled_to.min_by(|x, y| x.0.total_cmp(&y.0)) else {
            continue;
        };
        coupled += 1;
        let geo = impedance::geometry(st, a.layer);
        let est = geo.map(|geo| impedance::zdiff(geo, a.width, g as Nm));
        let ohm = match est {
            Err(missing) | Ok(Estimate::Unknown { missing }) => {
                let why = format!("{name}: track {} needs {missing}", a.id);
                note("DRC-026", why);
                continue;
            }
            Ok(Estimate::Known { ohm, .. }) => ohm,
        };
        let band = target * tol / 100.0;
        if (ohm - target).abs() > band {
            let (bound, limit) = match ohm > target {
                true => (Bound::Max, target + band),
                false => (Bound::Min, target - band),
            };
            let (ta, tb, layer) = (&a.id, &b.id, a.layer.name());
            let msg = format!(
                "pair {name}: track {ta} is about {ohm:.1} Ω differential, target {target:.1} Ω ± {tol} %"
            );
            let objs = vec![ta.clone(), tb.clone()];
            f.push(
                finding("DRC-026", Error, msg, objs, Some(a.a))
                    .key(&format!("{name}-{ta}"))
                    .measured(ohm, "Ω")
                    .required(bound, limit, "Ω")
                    .fix(format!(
                        "Change the width of {ta} or its gap to {tb} so the pair estimates {target:.1} Ω on {layer}."
                    )),
            );
        }
    }
    if coupled == 0 {
        let why = format!("{name}: no coupled segment (the members never run side by side)");
        note("DRC-026", why);
    }
}
