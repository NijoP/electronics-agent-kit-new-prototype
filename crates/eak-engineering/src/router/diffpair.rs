//! Coupled routing of declared differential pairs (`Constraints::diff_pairs`, ADR-0055).
//!
//! A matched pair of connections (one per member, endpoints paired by proximity) is routed as
//! one centreline: A* runs on a grid where the pair's two nets count as one and every other
//! net's blocked cells are grown by the member offset `(gap + width) / 2`, so a free centreline
//! leaves room for both members. The members are the centreline offset to either side (mitred
//! corners), joined to their pads by short fan-out segments. At a layer change the members
//! spread to a via pitch that keeps the clearance between the two vias. Every member segment
//! and via is then checked with exact geometry against all other copper and against the
//! partner (the DRC-002 rule for each pair of nets); nothing that fails is returned.
//!
//! The two ends break out perpendicular to their pad axis, so each member reaches its own pad
//! without running over its partner's. Length matching: when the pair declares `max_skew` and
//! the members differ by more, the shorter member gets a serpentine (rectangular bumps on its
//! outer side, away from the partner) that adds exactly the difference, again checked exactly;
//! larger bumps are tried first. Width and gap come from the rule, or the gap is derived from the target impedance with
//! `impedance` (IPC-2141A); a gap that is neither declared nor derivable means the pair is not
//! routed coupled, and the reason names the missing input.

use super::*;
use crate::clearance;
use crate::impedance::{self, Estimate, Sizing};
use eak_core::DiffPairRule;

/// A pair rule the router can route coupled: the declared rule with its width and gap resolved.
#[derive(Debug, Clone)]
pub(super) struct Coupled {
    pub positive: String,
    pub negative: String,
    pub width: Nm,
    pub gap: Nm,
    pub max_skew: Option<Nm>,
    pub target_zdiff_ohm: Option<f64>,
    pub zdiff_tolerance_pct: Option<f64>,
    /// How width and gap were chosen.
    pub notes: Vec<String>,
}

/// Resolve the routing width and gap of a declared pair, or say why it cannot be routed coupled.
pub(super) fn resolve(p: &Project, rule: &DiffPairRule) -> Result<Coupled, String> {
    let c = &p.constraints;
    let mut notes = vec![];
    let width = match rule.width {
        Some(w) => w,
        None => {
            let w = intel::net_spec(p, &rule.positive).track_width;
            notes.push(format!(
                "width undeclared: the net-class width {:.3} mm is used",
                w as f64 / 1e6
            ));
            w
        }
    }
    .max(c.min_track_width);
    let gap = match (rule.gap, rule.target_zdiff_ohm) {
        (Some(g), _) => g,
        (None, Some(z)) => {
            match impedance::gap_for(&p.pcb.board.stackup, CopperLayer::Top, width, z) {
                Sizing::Known {
                    value,
                    achieved_ohm,
                    formula,
                    ..
                } => {
                    notes.push(format!(
                        "gap {:.3} mm derived for {z} ohm (gives {achieved_ohm:.1} ohm on the top layer; {formula})",
                        value as f64 / 1e6
                    ));
                    value
                }
                Sizing::Unknown { missing } => {
                    return Err(format!(
                    "gap undeclared and not derivable from the {z} ohm target: missing {missing}"
                ))
                }
            }
        }
        (None, None) => {
            return Err(
                "gap undeclared (declare gap or target_zdiff_ohm to route the pair coupled)".into(),
            )
        }
    };
    let need = clearance::required(c, &rule.positive, &rule.negative, CopperLayer::Top);
    if (gap as f64) < need - 1.0 {
        return Err(format!(
            "gap {:.3} mm is below the {:.3} mm clearance the rules require between {} and {}",
            gap as f64 / 1e6,
            need / 1e6,
            rule.positive,
            rule.negative
        ));
    }
    Ok(Coupled {
        positive: rule.positive.clone(),
        negative: rule.negative.clone(),
        width,
        gap,
        max_skew: rule.max_skew,
        target_zdiff_ohm: rule.target_zdiff_ohm,
        zdiff_tolerance_pct: rule.zdiff_tolerance_pct,
        notes,
    })
}

type Seg = (usize, Point, Point);

fn unit(a: Point, b: Point) -> (f64, f64) {
    let l = a.dist(b).max(1.0);
    ((b.x - a.x) as f64 / l, (b.y - a.y) as f64 / l)
}

fn left(d: (f64, f64)) -> (f64, f64) {
    (-d.1, d.0)
}

fn shift(p: Point, n: (f64, f64), k: f64) -> Point {
    Point::new(
        (p.x as f64 + n.0 * k).round() as Nm,
        (p.y as f64 + n.1 * k).round() as Nm,
    )
}

fn mid(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) / 2, (a.y + b.y) / 2)
}

/// `pts` offset by `o` to the left of the direction of travel (negative = right), mitred.
/// `None` when a corner is sharper than 135 degrees (the offset would fold over).
fn offset(pts: &[Point], o: f64) -> Option<Vec<Point>> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let p = pts[i];
        let q = if i == 0 {
            left(unit(p, pts[1]))
        } else if i + 1 == n {
            left(unit(pts[i - 1], p))
        } else {
            let (a, b) = (left(unit(pts[i - 1], p)), left(unit(p, pts[i + 1])));
            let (mx, my) = (a.0 + b.0, a.1 + b.1);
            let ml = (mx * mx + my * my).sqrt();
            if ml < 1e-9 {
                return None;
            }
            let m = (mx / ml, my / ml);
            let cos = m.0 * a.0 + m.1 * a.1;
            if cos < 0.38 {
                return None;
            }
            (m.0 / cos, m.1 / cos)
        };
        out.push(shift(p, q, o));
    }
    Some(out)
}

/// Grow `m` (a `cols` x `rows` mask) by `k` cells in every direction (square).
pub(super) fn dilate(m: &[bool], cols: usize, rows: usize, k: usize) -> Vec<bool> {
    if k == 0 {
        return m.to_vec();
    }
    let mut h = vec![false; m.len()];
    for r in 0..rows {
        for c in 0..cols {
            let (c0, c1) = (c.saturating_sub(k), (c + k).min(cols - 1));
            h[r * cols + c] = (c0..=c1).any(|x| m[r * cols + x]);
        }
    }
    let mut v = vec![false; m.len()];
    for r in 0..rows {
        let (r0, r1) = (r.saturating_sub(k), (r + k).min(rows - 1));
        for c in 0..cols {
            v[r * cols + c] = (r0..=r1).any(|y| h[y * cols + c]);
        }
    }
    v
}

/// Router grid for the pair's centreline: both members' copper counts as `idp` (cells their
/// pads made shared become the pair's again unless other copper, the board edge or a keep-out
/// also covers them), every other net's blocked cells are grown by `k` cells, except within
/// `keep` of the two ends (where the members fan out to their pads, which the exact check
/// judges).
fn pair_grid(
    env: &Env,
    placed: &[Placed],
    rule: &Coupled,
    (idp, idn): (u32, u32),
    k: usize,
    ends: [Point; 2],
    keep: f64,
) -> Grid {
    let mut g = grid_with(env, placed.iter());
    // Pair members change layers with through vias only (their spread vias are placed as such).
    g.spans.clear();
    let stack = &env.p.pcb.board.stackup;
    let member = |net: &str| net == rule.positive || net == rule.negative;
    let near = |s: &Shape, grow: f64, pt: Point| {
        Shape {
            core: s.core.clone(),
            radius: s.radius + grow,
        }
        .contains(pt)
    };
    for it in env.geo.copper.iter().filter(|i| member(&i.net)) {
        let Some(l) = stack.index_of(it.layer) else {
            continue;
        };
        let inflate = env.inflate[l];
        for i in g.cells(&it.shape, inflate) {
            if g.owner[l][i] != MULTI || env.outside[i] {
                continue;
            }
            let pt = g.center(i % g.cols, i / g.cols);
            let foreign = env
                .geo
                .copper
                .iter()
                .any(|o| o.layer == it.layer && !member(&o.net) && near(&o.shape, inflate, pt))
                || placed.iter().any(|pl| {
                    !member(&pl.net)
                        && pl
                            .shapes
                            .iter()
                            .any(|(sl, s)| *sl == l && near(s, inflate, pt))
                })
                || env
                    .track_keepouts
                    .iter()
                    .any(|ko| ko.layers[l] && near(&ko.shape, inflate, pt));
            if !foreign {
                g.owner[l][i] = idp;
            }
        }
    }
    for plane in g.owner.iter_mut() {
        for o in plane.iter_mut().filter(|o| **o == idn) {
            *o = idp;
        }
    }
    let near_end: Vec<bool> = (0..g.cols * g.rows)
        .map(|i| {
            let p = g.center(i % g.cols, i / g.cols);
            ends.iter().any(|e| e.dist(p) <= keep)
        })
        .collect();
    let (cols, rows) = (g.cols, g.rows);
    for owner in g.owner.iter_mut() {
        let foreign: Vec<bool> = owner.iter().map(|&o| o != FREE && o != idp).collect();
        let grown = dilate(&foreign, cols, rows, k);
        for (i, o) in owner.iter_mut().enumerate() {
            if grown[i] && !foreign[i] && !near_end[i] {
                *o = MULTI;
            }
        }
    }
    g
}

/// The two members (positive, negative) routed coupled.
#[derive(Clone)]
struct Members {
    segs: [Vec<Seg>; 2],
    vias: [Vec<Point>; 2],
    /// Segments that run coupled (not fan-outs or via spreads), by index into `segs`.
    tunable: [Vec<usize>; 2],
    /// Side of the centreline each member is on (+1 left of travel, -1 right).
    side: [f64; 2],
}

/// Centreline segments and vias to the two members; `ends[m]` are member `m`'s two pads.
fn members(
    centre: &[Seg],
    vias: &[Point],
    ends: [[Point; 2]; 2],
    o: f64,
    via_half_pitch: f64,
    side_p: f64,
) -> Result<Members, String> {
    // Runs: maximal chains of connected centre segments on one layer.
    let mut runs: Vec<(usize, Vec<Point>)> = vec![];
    for &(l, a, b) in centre {
        match runs.last_mut() {
            Some((rl, pts)) if *rl == l && pts.last() == Some(&a) => pts.push(b),
            _ => runs.push((l, vec![a, b])),
        }
    }
    if runs.is_empty() || runs.len() != vias.len() + 1 {
        return Err("the pair's centreline changes layer at an end (no room to fan out)".into());
    }
    let mut out = Members {
        segs: [vec![], vec![]],
        vias: [vec![], vec![]],
        tunable: [vec![], vec![]],
        side: [side_p, -side_p],
    };
    let push = |segs: &mut Vec<Seg>, l: usize, a: Point, b: Point| {
        if a != b {
            segs.push((l, a, b));
        }
    };
    for (m, end) in ends.iter().enumerate() {
        let s = out.side[m];
        let mut from = end[0];
        for (k, (l, pts)) in runs.iter().enumerate() {
            let off = offset(pts, s * o)
                .ok_or("the pair's centreline turns too sharply to offset the members")?;
            // Fan-out from the pad, or from the via on this layer, to the first offset point.
            push(&mut out.segs[m], *l, from, off[0]);
            for w in off.windows(2) {
                if w[0] != w[1] {
                    out.tunable[m].push(out.segs[m].len());
                    out.segs[m].push((*l, w[0], w[1]));
                }
            }
            let last = *off.last().expect("a run has two points");
            if k + 1 < runs.len() {
                let n = left(unit(pts[pts.len() - 2], pts[pts.len() - 1]));
                let v = shift(vias[k], n, s * via_half_pitch);
                push(&mut out.segs[m], *l, last, v);
                out.vias[m].push(v);
                from = v;
            } else {
                push(&mut out.segs[m], *l, last, end[1]);
            }
        }
    }
    Ok(out)
}

fn length(segs: &[Seg]) -> f64 {
    segs.iter().map(|(_, a, b)| a.dist(*b)).sum()
}

/// Exact check of both members against everything the router checks (`violation`: other-net
/// copper, board edge, holes, keep-outs) and against each other.
fn check(
    env: &Env,
    rule: &Coupled,
    shapes: &[Vec<(usize, Shape)>; 2],
    vias: &[Vec<Point>; 2],
    placed: &[Placed],
) -> Result<(), String> {
    let c = &env.p.constraints;
    for (m, net) in [&rule.positive, &rule.negative].into_iter().enumerate() {
        let drill = env.specs[net].via_drill;
        if let Some(v) = violation(env, &shapes[m], &vias[m], drill, net, placed) {
            return Err(format!("CLEARANCE_CONFLICT: {net}: {v}"));
        }
    }
    for (lp, sp) in &shapes[0] {
        for (ln, sn) in &shapes[1] {
            let need = clearance::required(c, &rule.positive, &rule.negative, env.layers[*lp]);
            if lp == ln && distance(sp, sn) < need - 1.0 {
                return Err(format!(
                    "CLEARANCE_CONFLICT: the members would come closer than the {:.3} mm clearance between them",
                    need / 1e6
                ));
            }
        }
    }
    // Hole-to-hole spacing between the members' vias (`violation` only sees placed copper).
    let (rp, rn) = (
        env.specs[&rule.positive].via_drill as f64 / 2.0,
        env.specs[&rule.negative].via_drill as f64 / 2.0,
    );
    via_spacing(vias, rp, rn, env.hole_min)
}

/// `CLEARANCE_CONFLICT [DRC-019]` when a positive-member via and a negative-member via (drill
/// radii `rp`, `rn`) would be closer than `hole_min` edge to edge.
fn via_spacing(vias: &[Vec<Point>; 2], rp: f64, rn: f64, hole_min: f64) -> Result<(), String> {
    let close = vias[0].iter().any(|vp| {
        vias[1]
            .iter()
            .any(|vn| vp.dist(*vn) - rp - rn < hole_min - 1.0)
    });
    if close {
        return Err(format!(
            "CLEARANCE_CONFLICT: the members' vias would be closer than the {:.3} mm hole-to-hole spacing [DRC-019]",
            hole_min / 1e6
        ));
    }
    Ok(())
}

/// Rectangular bumps on the `tunable` segments (longest first) on the `outer` side, adding
/// `extra` nm of length, at most `amp` nm high and `pitch` nm wide, with `pitch` between
/// bumps and at both ends. `None` when they do not fit.
fn serpentine(
    segs: &[Seg],
    tunable: &[usize],
    outer: f64,
    extra: f64,
    amp: f64,
    pitch: f64,
) -> Option<Vec<Seg>> {
    let len = |i: usize| segs[i].1.dist(segs[i].2);
    let mut order: Vec<usize> = tunable.to_vec();
    order.sort_by(|&a, &b| len(b).total_cmp(&len(a)));
    let mut rest = extra;
    // Per segment: (start along the segment, height) of each bump.
    let mut bumps: BTreeMap<usize, Vec<(f64, f64)>> = BTreeMap::new();
    for i in order {
        let fit = ((len(i) - pitch) / (2.0 * pitch)).floor().max(0.0) as usize;
        for j in 0..fit {
            if rest < 1.0 {
                break;
            }
            let h = amp.min(rest / 2.0);
            bumps
                .entry(i)
                .or_default()
                .push((pitch + 2.0 * pitch * j as f64, h));
            rest -= 2.0 * h;
        }
    }
    if rest >= 1.0 {
        return None;
    }
    let mut out = Vec::with_capacity(segs.len() + 4 * bumps.values().map(Vec::len).sum::<usize>());
    for (i, &(l, a, b)) in segs.iter().enumerate() {
        let Some(list) = bumps.get(&i) else {
            out.push((l, a, b));
            continue;
        };
        let d = unit(a, b);
        let n = left(d);
        let n = (n.0 * outer, n.1 * outer);
        let mut cur = a;
        for &(s0, h) in list {
            let (p0, p1) = (shift(a, d, s0), shift(a, d, s0 + pitch));
            let (u0, u1) = (shift(p0, n, h), shift(p1, n, h));
            for (x, y) in [(cur, p0), (p0, u0), (u0, u1), (u1, p1)] {
                if x != y {
                    out.push((l, x, y));
                }
            }
            cur = p1;
        }
        if cur != b {
            out.push((l, cur, b));
        }
    }
    Some(out)
}

/// The routed pair: one placed route per member (positive first) and what was done.
pub(super) struct PairRoute {
    pub placed: [Placed; 2],
    pub note: String,
}

/// Route a declared pair's matched connections (`cp` of the positive net, `cn` of the negative)
/// coupled, or say why not.
pub(super) fn attempt(
    env: &Env,
    rule: &Coupled,
    (cp, ip): (&Conn, usize),
    (cn, inn): (&Conn, usize),
    placed: &[Placed],
    costs: &Costs,
) -> Result<PairRoute, String> {
    let (Some(&idp), Some(&idn)) = (env.net_ids.get(&cp.net), env.net_ids.get(&cn.net)) else {
        return Err("NO_NET: a member has no net".into());
    };
    // Pair the negative ends with the positive ones they are nearest to.
    let (na, nb) = if cp.a.dist(cn.a) + cp.b.dist(cn.b) <= cp.a.dist(cn.b) + cp.b.dist(cn.a) {
        (cn.a, cn.b)
    } else {
        (cn.b, cn.a)
    };
    let (ca, cb) = (mid(cp.a, na), mid(cp.b, nb));
    let w = rule.width as f64;
    let o = (rule.gap as f64 + w) / 2.0;
    let pitch = env.opts.pitch as f64;
    let specs = [&env.specs[&cp.net], &env.specs[&cn.net]];
    let c_pair = clearance::required(&env.p.constraints, &cp.net, &cn.net, CopperLayer::Top);
    // Vias side by side need their pad clearance and the hole-to-hole spacing (DRC-019), so the
    // members spread at a layer change.
    let via_d = specs[0].via_diameter.max(specs[1].via_diameter) as f64;
    let drill = specs[0].via_drill.max(specs[1].via_drill) as f64;
    let via_half = o
        .max((via_d + c_pair) / 2.0 + 1_000.0)
        .max((drill + env.hole_min) / 2.0 + 1_000.0);
    // Break out of each end perpendicular to its pad axis, towards the other end: the members
    // then leave their pads side by side instead of one running over its partner's pad.
    let reach = 2.0 * o + 2.0 * pitch;
    let keep = cp.a.dist(ca).max(cp.b.dist(cb)) + o + reach + 2.0 * pitch;
    let k = (o / pitch).ceil() as usize;
    let grid = pair_grid(env, placed, rule, (idp, idn), k, [ca, cb], keep);
    let breakout = |m: Point, pad: Point, other: Point| -> Point {
        let n = left(unit(m, pad));
        let towards = (other.x - m.x) as f64 * n.0 + (other.y - m.y) as f64 * n.1;
        let raw = shift(m, n, if towards >= 0.0 { reach } else { -reach });
        let (c, r) = grid.cell_of(raw);
        grid.center(c, r)
    };
    let (ba, bb) = (breakout(ca, cp.a, cb), breakout(cb, cp.b, ca));
    let common = |x: Point, y: Point| -> Vec<usize> {
        let ly = pad_layers(env, y, &cn.net);
        pad_layers(env, x, &cp.net)
            .into_iter()
            .filter(|l| ly.contains(l))
            .collect()
    };
    let (la, lb) = (common(cp.a, na), common(cp.b, nb));
    if la.is_empty() || lb.is_empty() {
        return Err("the members' pads share no routable layer at one end".into());
    }
    // The members keep to the reference copper under the pair's ends (`split`).
    let guard = o + w / 2.0 + env.margin;
    let mask = split::for_net(env, &cp.net, [ca, cb], guard, reach + 2.0 * pitch);
    let costs = &Costs {
        split: mask.as_ref().or(costs.split),
        ..*costs
    };
    let path = astar(
        &grid,
        idp,
        (ba, &la),
        (bb, &lb),
        env.routable,
        env.opts,
        costs,
    )
    .ok_or("NO_LEGAL_PATH: no corridor wide enough for both members")?;
    let (mut centre, cvias) = path_to_segments(&grid, &path, ba, bb);
    let (l0, l1) = (path[0].0, path[path.len() - 1].0);
    centre.insert(0, (l0, ca, ba));
    centre.push((l1, bb, cb));
    centre.retain(|(_, a, b)| a != b);
    if centre.is_empty() {
        return Err("the pair's ends coincide".into());
    }
    // The positive member runs on the side of the centreline its first pad is on.
    let n0 = left(unit(centre[0].1, centre[0].2));
    let toward = (cp.a.x - ca.x) as f64 * n0.0 + (cp.a.y - ca.y) as f64 * n0.1;
    let side_p = if toward >= 0.0 { 1.0 } else { -1.0 };
    let mut m = members(
        &centre,
        &cvias,
        [[cp.a, cp.b], [na, nb]],
        o,
        via_half,
        side_p,
    )?;
    let nl = env.layers.len();
    let widths = vec![rule.width; nl];
    let shapes = |m: &Members, k: usize| {
        shapes_for(&m.segs[k], &m.vias[k], &widths, specs[k].via_diameter, nl)
    };
    let both = |m: &Members| [shapes(m, 0), shapes(m, 1)];
    check(env, rule, &both(&m), &m.vias, placed)?;

    // Length matching on the shorter member: a serpentine on its outer side, larger bumps first.
    let (lp, ln) = (length(&m.segs[0]), length(&m.segs[1]));
    let mut note = format!(
        "coupled: width {:.3} mm, gap {:.3} mm",
        rule.width as f64 / 1e6,
        rule.gap as f64 / 1e6
    );
    match rule.max_skew {
        None => note.push_str(&format!(
            "; skew {:.3} mm (no max_skew declared: not length matched)",
            (lp - ln).abs() / 1e6
        )),
        Some(max) if (lp - ln).abs() > max as f64 => {
            let skew = (lp - ln).abs();
            let short = usize::from(ln < lp);
            let q = (3.0 * w).max(w + 2.0 * rule.gap as f64);
            let tuned = [4.0 * q, 3.0 * q, 2.0 * q, q].into_iter().find_map(|amp| {
                let segs = serpentine(
                    &m.segs[short],
                    &m.tunable[short],
                    m.side[short],
                    skew,
                    amp,
                    q,
                )?;
                let mut cand = m.clone();
                cand.segs[short] = segs;
                check(env, rule, &both(&cand), &cand.vias, placed)
                    .ok()
                    .map(|()| (cand, amp))
            });
            let member = if short == 0 {
                &rule.positive
            } else {
                &rule.negative
            };
            match tuned {
                Some((cand, amp)) => {
                    note.push_str(&format!(
                        "; length matched with a serpentine on {member} ({:.3} mm added, bumps up to {:.3} mm)",
                        skew / 1e6,
                        amp / 1e6
                    ));
                    m = cand;
                }
                None => note.push_str(&format!(
                    "; skew {:.3} mm exceeds {:.3} mm and no serpentine on {member} fits clear of other copper",
                    skew / 1e6,
                    max as f64 / 1e6
                )),
            }
        }
        Some(_) => note.push_str(&format!(
            "; skew {:.3} mm within max_skew without tuning",
            (lp - ln).abs() / 1e6
        )),
    }
    let [sp, sn] = both(&m);
    let make = |k: usize, ci: usize, conn: &Conn, id: u32, shapes: Vec<(usize, Shape)>| Placed {
        conn: ci,
        net: conn.net.clone(),
        id,
        widths: widths.clone(),
        via_drill: specs[k].via_drill,
        via_diameter: specs[k].via_diameter,
        segs: m.segs[k].clone(),
        vias: m.vias[k].clone(),
        blind: vec![],
        shapes,
        pair: true,
        moved: None,
    };
    Ok(PairRoute {
        placed: [make(0, ip, cp, idp, sp), make(1, inn, cn, idn, sn)],
        note,
    })
}

/// Differential impedance of the routed pair on each router layer it uses, and whether every
/// one is within the declared target and tolerance (`None` when either is undeclared or a
/// layer's impedance is UNKNOWN).
pub(super) fn impedance_on(
    env: &Env,
    rule: &Coupled,
    layers: &BTreeSet<usize>,
) -> (Vec<(String, Estimate)>, Option<bool>) {
    let est: Vec<(String, Estimate)> = layers
        .iter()
        .map(|&l| {
            let layer = env.layers[l];
            let e = match impedance::geometry(&env.p.pcb.board.stackup, layer) {
                Ok(g) => impedance::zdiff(g, rule.width, rule.gap),
                Err(missing) => Estimate::Unknown { missing },
            };
            (layer.name(), e)
        })
        .collect();
    let verdict = match (rule.target_zdiff_ohm, rule.zdiff_tolerance_pct) {
        (Some(t), Some(tol)) if !est.is_empty() => est
            .iter()
            .map(|(_, e)| e.ohm().map(|z| (z - t).abs() <= t * tol / 100.0))
            .collect::<Option<Vec<bool>>>()
            .map(|v| v.into_iter().all(|x| x)),
        _ => None,
    };
    (est, verdict)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_of_a_45_degree_bend_keeps_the_distance() {
        let pts = [
            Point::new(0, 0),
            Point::new(1_000_000, 0),
            Point::new(2_000_000, 1_000_000),
        ];
        let off = offset(&pts, 100_000.0).unwrap();
        assert_eq!(off[0], Point::new(0, 100_000));
        // Each offset vertex is 100 µm from its centre segment.
        for k in 0..2 {
            let d = point_segment_distance(off[k], pts[k], pts[k + 1]);
            assert!((d - 100_000.0).abs() < 2.0, "{d}");
        }
        // A reversal cannot be offset.
        assert!(offset(&[pts[0], pts[1], pts[0]], 100_000.0).is_none());
    }

    #[test]
    fn serpentine_adds_exactly_the_missing_length() {
        let segs = vec![(0, Point::new(0, 0), Point::new(10_000_000, 0))];
        let out = serpentine(&segs, &[0], 1.0, 1_234_000.0, 400_000.0, 300_000.0).unwrap();
        let added = length(&out) - length(&segs);
        assert!((added - 1_234_000.0).abs() < 10.0, "{added}");
        // Bumps go to the outer side only and keep both ends.
        assert!(out.iter().all(|(_, a, b)| a.y >= 0 && b.y >= 0));
        assert_eq!(out.first().unwrap().1, Point::new(0, 0));
        assert_eq!(out.last().unwrap().2, Point::new(10_000_000, 0));
        // More than the segment can take.
        assert!(serpentine(&segs, &[0], 1.0, 1e9, 400_000.0, 300_000.0).is_none());
    }

    #[test]
    fn member_vias_closer_than_the_hole_spacing_are_a_drc_019_conflict() {
        // 0.3 mm drills, 0.5 mm hole-to-hole minimum: centres 0.8 mm apart are exactly legal.
        let (r, min) = (150_000.0, 500_000.0);
        let at = |x: i64| vec![Point::new(x, 0)];
        let conflict = |vias: [Vec<Point>; 2]| {
            let e = via_spacing(&vias, r, r, min).expect_err("too close");
            assert!(e.starts_with("CLEARANCE_CONFLICT"), "{e}");
            assert!(e.contains("[DRC-019]") && e.contains("0.500 mm"), "{e}");
        };
        assert_eq!(via_spacing(&[at(0), at(800_000)], r, r, min), Ok(()));
        // The acceptance fixture's failure: 0.452 mm edge to edge.
        conflict([at(0), at(752_000)]);
        // Any pair counts, not only the first; a member with no via never conflicts.
        let p = vec![Point::new(0, 5_000_000), Point::new(0, 0)];
        conflict([p.clone(), at(700_000)]);
        assert_eq!(via_spacing(&[p, vec![]], r, r, min), Ok(()));
    }

    #[test]
    fn dilation_grows_by_k_cells() {
        let mut m = vec![false; 25];
        m[12] = true; // centre of 5 x 5
        let g = dilate(&m, 5, 5, 1);
        assert_eq!(g.iter().filter(|&&b| b).count(), 9);
        assert!(!g[0] && g[6] && g[18]);
    }

    #[test]
    fn members_fan_out_and_spread_at_a_via() {
        // Centre: east on layer 0, via, east on layer 1.
        let c = [
            (0, Point::new(0, 0), Point::new(4_000_000, 0)),
            (1, Point::new(4_000_000, 0), Point::new(8_000_000, 0)),
        ];
        let ends = [
            [Point::new(0, 300_000), Point::new(8_000_000, 300_000)],
            [Point::new(0, -300_000), Point::new(8_000_000, -300_000)],
        ];
        let m = members(
            &c,
            &[Point::new(4_000_000, 0)],
            ends,
            200_000.0,
            500_000.0,
            1.0,
        )
        .unwrap();
        assert_eq!(m.vias[0], vec![Point::new(4_000_000, 500_000)]);
        assert_eq!(m.vias[1], vec![Point::new(4_000_000, -500_000)]);
        // Symmetric pair: equal lengths, each member starts and ends at its own pads.
        assert!((length(&m.segs[0]) - length(&m.segs[1])).abs() < 1.0);
        assert_eq!(m.segs[0][0].1, ends[0][0]);
        assert_eq!(m.segs[1].last().unwrap().2, ends[1][1]);
        // A layer change without a run on each side is refused.
        assert!(members(
            &c[..1],
            &[Point::new(4_000_000, 0)],
            ends,
            200_000.0,
            500_000.0,
            1.0
        )
        .is_err());
    }
}
