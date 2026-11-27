//! Split-plane awareness for declared high-speed nets (M7.6, `crate::planes`).
//!
//! Every routed grid plane with a reference layer gets one reference key per cell (the reference
//! copper of its adjacent planes or pours under the cell centre, interned; 0 = none). A declared
//! high-speed connection may then only use, on each plane, the cells over the reference copper
//! one of its ends sits on, kept `guard` clear of every other reference (and of no reference),
//! except close to its ends where it fans out of its pads; so it never crosses a split and goes
//! around one instead. Planes where neither end has reference copper keep no such limit but cost
//! more per step, so a referenced layer is preferred. Nothing changes for a connection when no
//! routed plane has reference copper under its ends.
//!
//! After routing, each high-speed layer change whose nearest reference planes differ but share a
//! net gets a return via of that net (through, the net's via size) in a ring around it
//! (`stitch` ring: 0.6–3.0 mm, 24 angles, no wider than a declared `return_via_max_distance`),
//! checked like any routed copper (`violation`), unless
//! one is already within that ring. A change of reference net only gets a note: a stitching
//! capacitor is a part, not copper the router may place. `eak-validation` judges the result
//! (DRC-029..031), never this module.

use super::*;
use crate::planes::{self, Planes, Reference, ReturnNeed};
use crate::stitch::{RING_ANGLES, RING_MAX_MM, RING_MIN_MM, RING_STEP_MM};

/// Extra step cost on a plane without reference copper under the connection's ends.
const UNREFERENCED_PENALTY: u64 = 10;

/// Reference key per grid plane and cell.
pub(super) struct RefMap {
    /// `None` for a plane that is not routed or has no reference layer; else one key per cell
    /// (0 = no reference copper under the cell).
    keys: Vec<Option<Vec<u32>>>,
}

// shortcut: two zones of one net on a plane get different keys, so a route may not pass from one
// to the other although DRC-029 accepts that (its rule is not transitive, so no key can match it
// exactly); upgrade when a board with abutting same-net plane zones needs such a route.
pub(super) fn ref_map(
    p: &Project,
    geo: &BoardGeometry,
    grid: &Grid,
    layers: &[CopperLayer],
    routable: &[usize],
) -> RefMap {
    let model = Planes::new(p, geo);
    let mut interned: Vec<Vec<Reference>> = vec![];
    let mut keys = vec![];
    for (l, &layer) in layers.iter().enumerate() {
        let refs = planes::reference_layers(p, layer);
        if !routable.contains(&l) || refs.is_empty() {
            keys.push(None);
            continue;
        }
        let mut plane = Vec::with_capacity(grid.cols * grid.rows);
        for i in 0..grid.cols * grid.rows {
            let pt = grid.center(i % grid.cols, i / grid.cols);
            let under: Vec<Reference> = refs.iter().map(|&r| model.reference_at(r, pt)).collect();
            let key = if under.iter().all(|r| *r == Reference::None) {
                0
            } else if let Some(k) = interned.iter().position(|k| *k == under) {
                k + 1
            } else {
                interned.push(under);
                interned.len()
            };
            plane.push(key as u32);
        }
        keys.push(Some(plane));
    }
    RefMap { keys }
}

/// Where one high-speed connection may run.
pub(super) struct Mask {
    /// Per grid plane: allowed cells, `None` = no limit on that plane.
    ok: Vec<Option<Vec<bool>>>,
    /// Per grid plane: no reference copper under the connection's ends there (penalised).
    unreferenced: Vec<bool>,
}

impl Mask {
    pub(super) fn allows(&self, l: usize, i: usize) -> bool {
        self.ok[l].as_ref().is_none_or(|m| m[i])
    }

    pub(super) fn penalty(&self, l: usize) -> u64 {
        if self.unreferenced[l] {
            UNREFERENCED_PENALTY
        } else {
            0
        }
    }
}

/// The mask of a connection with ends `ends`: other reference copper (or none) stays `guard`
/// from its centre line, and it is free within `wild` of an end. `None` when no plane has
/// reference copper under an end.
pub(super) fn mask(
    map: &RefMap,
    grid: &Grid,
    ends: [Point; 2],
    guard: f64,
    wild: f64,
) -> Option<Mask> {
    let n = grid.cols * grid.rows;
    let near_end: Vec<bool> = (0..n)
        .map(|i| {
            let pt = grid.center(i % grid.cols, i / grid.cols);
            ends.iter().any(|e| e.dist(pt) <= wild)
        })
        .collect();
    let k = (guard / grid.pitch as f64).ceil() as usize;
    let mut out = Mask {
        ok: vec![],
        unreferenced: vec![],
    };
    for keys in &map.keys {
        let allowed = keys.as_ref().and_then(|keys| {
            ends.iter()
                .map(|e| {
                    let (c, r) = grid.cell_of(*e);
                    keys[grid.idx(c, r)]
                })
                .find(|&key| key != 0)
        });
        match (keys, allowed) {
            (Some(keys), Some(a)) => {
                let other: Vec<bool> = keys.iter().map(|&x| x != a).collect();
                let grown = diffpair::dilate(&other, grid.cols, grid.rows, k);
                let ok = (0..n).map(|i| !grown[i] || near_end[i]).collect();
                out.ok.push(Some(ok));
                out.unreferenced.push(false);
            }
            _ => {
                out.ok.push(None);
                out.unreferenced.push(true);
            }
        }
    }
    out.ok.iter().any(Option::is_some).then_some(out)
}

/// The mask of `net`'s connection between `ends` (`None` for a net that is not declared
/// high-speed or when nothing limits it).
pub(super) fn for_net(
    env: &Env,
    net: &str,
    ends: [Point; 2],
    guard: f64,
    wild: f64,
) -> Option<Mask> {
    let map = env.refs.filter(|_| env.high_speed.contains(net))?;
    mask(map, env.base, ends, guard, wild)
}

/// The mask of single connection `conn`: other reference copper stays half its widest track plus
/// the grid margin from its centre line.
pub(super) fn for_conn(env: &Env, conn: &Conn) -> Option<Mask> {
    let half = env
        .specs
        .get(&conn.net)
        .map_or(0.0, |s| s.track_width.max(s.inner_track_width) as f64 / 2.0);
    let guard = half + env.margin;
    let wild = guard + 2.0 * env.opts.pitch as f64;
    for_net(env, &conn.net, [conn.a, conn.b], guard, wild)
}

/// Return vias for the high-speed routes among `placed`, and notes on the layer changes that did
/// not get one.
pub(super) fn return_vias(env: &Env, placed: &[Placed]) -> (Vec<RoutedVia>, Vec<String>) {
    let model = Planes::new(env.p, env.geo);
    let st = &env.p.pcb.board.stackup;
    // The stitch ring, no wider than the declared largest return-via distance (ADR-0059; 2 nm
    // inside it, so the rounded via position never exceeds it).
    let ring = env
        .p
        .constraints
        .return_via_max_distance
        .map_or(RING_MAX_MM * 1e6, |d| {
            (d as f64 - 2.0).min(RING_MAX_MM * 1e6)
        });
    let mut board: Vec<Placed> = placed.to_vec();
    let mut out: Vec<RoutedVia> = vec![];
    let mut notes = vec![];
    for pl in placed.iter().filter(|pl| env.high_speed.contains(&pl.net)) {
        let changes = pl.vias.iter().copied().chain(pl.blind.iter().map(|b| b.0));
        for v in changes {
            let used: BTreeSet<usize> = pl
                .segs
                .iter()
                .filter(|(_, a, b)| *a == v || *b == v)
                .map(|(l, _, _)| *l)
                .collect();
            let used: Vec<usize> = used.into_iter().collect();
            for (k, &i) in used.iter().enumerate() {
                for &j in &used[k + 1..] {
                    let (la, lb) = (env.layers[i], env.layers[j]);
                    match model.return_need(v, la, lb) {
                        ReturnNeed::Via { net, planes } => {
                            let joins = |from: CopperLayer, to: CopperLayer| {
                                let span = st.layers_between(from, to);
                                span.contains(&planes[0]) && span.contains(&planes[1])
                            };
                            let have = env
                                .p
                                .pcb
                                .vias
                                .values()
                                .filter(|o| o.net == net && joins(o.from, o.to))
                                .map(|o| o.position)
                                .chain(out.iter().filter(|o| o.net == net).map(|o| o.position))
                                .any(|o| o.dist(v) <= ring);
                            if have {
                                continue;
                            }
                            match place(env, &model, &board, &net, planes, (v, ring)) {
                                Some(ret) => {
                                    out.push(RoutedVia {
                                        position: ret.vias[0],
                                        drill: ret.via_drill,
                                        diameter: ret.via_diameter,
                                        net: net.clone(),
                                        from: env.layers[0],
                                        to: env.layers[env.layers.len() - 1],
                                    });
                                    board.push(ret);
                                }
                                None => notes.push(format!(
                                    "{}: no room for a {net} return via within {:.2} mm of the layer change at {}",
                                    pl.net,
                                    ring / 1e6,
                                    at(env, i, v)
                                )),
                            }
                        }
                        ReturnNeed::Capacitor { nets, .. } => notes.push(format!(
                            "{}: the reference changes from {} to {} at the layer change at {}; a stitching capacitor is needed there (not placed)",
                            pl.net,
                            nets[0],
                            nets[1],
                            at(env, i, v)
                        )),
                        ReturnNeed::Nothing | ReturnNeed::NoCopper { .. } => {}
                    }
                }
            }
        }
    }
    (out, notes)
}

/// A through via of `net` on the ring around `around`, over `net` copper on both reference
/// `planes` and clear of everything (`violation`), as a placed route.
fn place(
    env: &Env,
    model: &Planes,
    board: &[Placed],
    net: &str,
    planes: [CopperLayer; 2],
    (around, ring): (Point, f64),
) -> Option<Placed> {
    let id = *env.net_ids.get(net)?;
    let spec = env.specs.get(net)?;
    let nl = env.layers.len();
    let widths = vec![0; nl];
    let steps = ((RING_MAX_MM - RING_MIN_MM) / RING_STEP_MM).round() as usize;
    for s in 0..=steps {
        let r = (RING_MIN_MM + s as f64 * RING_STEP_MM) * 1e6;
        if r > ring + 1.0 {
            break;
        }
        for k in 0..RING_ANGLES {
            let a = k as f64 * std::f64::consts::TAU / RING_ANGLES as f64;
            let c = Point::new(
                around.x + (r * a.cos()).round() as Nm,
                around.y + (r * a.sin()).round() as Nm,
            );
            let on_net = planes
                .iter()
                .all(|&pl| model.reference_at(pl, c).net() == Some(net));
            if !on_net {
                continue;
            }
            let shapes = shapes_for(&[], &[c], &widths, spec.via_diameter, nl);
            if violation(env, &shapes, &[c], spec.via_drill, net, board).is_none() {
                return Some(Placed {
                    conn: usize::MAX,
                    net: net.into(),
                    id,
                    widths,
                    via_drill: spec.via_drill,
                    via_diameter: spec.via_diameter,
                    segs: vec![],
                    vias: vec![c],
                    blind: vec![],
                    shapes,
                    pair: false,
                    moved: None,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(cols: usize, rows: usize) -> Grid {
        let n = cols * rows;
        Grid {
            origin: Point::new(0, 0),
            pitch: 100_000,
            cols,
            rows,
            owner: vec![vec![FREE; n]],
            no_via: vec![false; n],
            no_via_on: vec![vec![false; n]],
            spans: vec![],
        }
    }

    #[test]
    fn a_high_speed_path_goes_around_another_reference_and_never_over_it() {
        let g = grid(30, 20);
        // Reference 1 everywhere except a block of reference 2 across the straight line.
        let keys: Vec<u32> = (0..600)
            .map(|i| {
                let (c, r) = (i % 30, i / 30);
                if (12..18).contains(&c) && r >= 5 {
                    2
                } else {
                    1
                }
            })
            .collect();
        let map = RefMap {
            keys: vec![Some(keys.clone())],
        };
        let (a, b) = (g.center(2, 10), g.center(27, 10));
        let m = mask(&map, &g, [a, b], 150_000.0, 200_000.0).expect("referenced ends");
        assert!(!m.allows(0, g.idx(15, 10)), "over the other reference");
        assert!(!m.allows(0, g.idx(11, 10)), "within the guard of it");
        assert!(m.allows(0, g.idx(2, 10)) && m.allows(0, g.idx(15, 1)));
        assert_eq!(m.penalty(0), 0);
        let pen = vec![vec![0u8; 600]];
        let costs = Costs {
            congestion: &pen,
            affinity: None,
            split: Some(&m),
        };
        let opts = RouterOptions::default();
        let path = astar(&g, 1, (a, &[0]), (b, &[0]), &[0], &opts, &costs).expect("a way round");
        assert!(path.iter().all(|&(_, c, r)| keys[g.idx(c, r)] == 1));
        assert!(
            path.iter().any(|&(_, _, r)| r < 3),
            "it went round the block"
        );
        // Ends over no reference copper: nothing to keep to.
        let void = RefMap {
            keys: vec![Some(vec![0; 600])],
        };
        assert!(mask(&void, &g, [a, b], 150_000.0, 200_000.0).is_none());
    }
}
