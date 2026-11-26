//! DC path resistance of a net from its routed copper (workstream B voltage drop).
//!
//! The net's copper becomes a resistor network: every track chord is cut at each point another
//! item of the net touches it, and each piece is `ρ(T)·L/(w·t)` with `t` from its layer's copper
//! weight; each via is a chain of barrel resistors `ρ(T)·h/(π·(d+tp)·tp)` between the adjacent
//! layers of its span (`h` = dielectric plus half of both foils, `tp` = declared plating); pads
//! are equipotential nodes. The conductor temperature is the declared ambient plus the allowed
//! rise. The network is solved (Jacobi-preconditioned conjugate gradients on the grounded
//! conductance matrix) for the effective resistance from the source pad to each sink pad.
//!
//! The source is the pad set of the net's power-output pins when they belong to one part (the
//! part ties them internally). Without one the worst pad pair is reported instead: it bounds
//! the drop for any choice of source, so it can prove a pass but never a failure.

use crate::copper::{self, Core, ItemKind};
use crate::power::{copper_resistance_ohm, copper_thickness_from_oz, COPPER_UM_PER_OZ};
use crate::verify::Verdict;
use eak_core::{CopperLayer, Id, PinType, Point, Project};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const SOURCE_PATH: &str = "R = ρ(T)·L/(w·t) per track piece and ρ(T)·h/(π·(d+tp)·tp) per via barrel (ρ20 = 1.7241e-8 Ω·m, α20 = 0.00393/K, IEC 60028 annealed copper; T = ambient + allowed rise), effective resistance of the copper network source pad → sink pad; drop V = I·R (Ohm's law) with the whole net current on each sink path";

pub const LIMITATIONS: &[&str] = &[
    "Voltage drop (DRC-028): pads are equipotential (pad copper and THT barrels carry no resistance); track length runs along the centre line to the point nearest each contact, so ends inside pads are counted; DC only",
    "Voltage drop (DRC-028): where two different tracks overlap (a jog shorter than the track width) they join at their closest centre-line points, so up to about one width of copper per such jog is not counted",
    "Voltage drop (DRC-028): the whole net current is taken through each sink path, an upper bound when the load is shared between sinks; plated copper is a few percent more resistive than the annealed-copper model",
];

/// Resistance from one pad (set) to another along the net's copper.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PadPath {
    pub from: String,
    pub to: String,
    /// Footprint of the `to` pad and its position.
    pub to_footprint: Id,
    pub at: Point,
    pub resistance_ohm: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NetPath {
    pub net: String,
    /// The source part's pads, or `None` when the net has no single power-output part.
    pub source: Option<Vec<String>>,
    /// One path per sink pad from the source; without a source only the worst pad pair.
    pub paths: Vec<PadPath>,
}

impl NetPath {
    /// The path of highest resistance.
    pub fn worst(&self) -> Option<&PadPath> {
        self.paths
            .iter()
            .max_by(|a, b| a.resistance_ohm.total_cmp(&b.resistance_ohm))
    }
}

/// Why the network could not be evaluated: `Unknown` (a named input is missing) or `Blocked`.
pub type PathError = (Verdict, String);

fn unknown(why: impl Into<String>) -> PathError {
    (Verdict::Unknown, why.into())
}

struct Dsu(Vec<usize>);
impl Dsu {
    fn add(&mut self) -> usize {
        self.0.push(self.0.len());
        self.0.len() - 1
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.0[r] != r {
            r = self.0[r];
        }
        self.0[x] = r;
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra] = rb;
        }
    }
}

type F2 = (f64, f64);

fn f2(p: Point) -> F2 {
    (p.x as f64, p.y as f64)
}

fn core_center(c: &Core) -> F2 {
    match c {
        Core::Segment(a, b) => ((a.x + b.x) as f64 / 2.0, (a.y + b.y) as f64 / 2.0),
        Core::Polygon(v) => {
            let n = v.len().max(1) as f64;
            let (x, y) = v
                .iter()
                .fold((0.0, 0.0), |s, p| (s.0 + p.x as f64, s.1 + p.y as f64));
            (x / n, y / n)
        }
    }
}

/// Parameter in [0, 1] of the point of segment `a`–`b` nearest `q`.
fn nearest_t(a: F2, b: F2, q: F2) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    if l2 == 0.0 {
        return 0.0;
    }
    (((q.0 - a.0) * dx + (q.1 - a.1) * dy) / l2).clamp(0.0, 1.0)
}

fn lerp(a: F2, b: F2, t: f64) -> F2 {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn d2(p: F2, q: F2) -> f64 {
    (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)
}

/// Parameters of the closest pair of points of segments `a`–`b` and `c`–`d`.
fn closest_params(a: F2, b: F2, c: F2, d: F2) -> (f64, f64) {
    let (r, s) = ((b.0 - a.0, b.1 - a.1), (d.0 - c.0, d.1 - c.1));
    let den = r.0 * s.1 - r.1 * s.0;
    if den != 0.0 {
        let (qx, qy) = (c.0 - a.0, c.1 - a.1);
        let t = (qx * s.1 - qy * s.0) / den;
        let u = (qx * r.1 - qy * r.0) / den;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
            return (t, u);
        }
    }
    let gap = |x: &(f64, f64)| d2(lerp(a, b, x.0), lerp(c, d, x.1));
    [
        (0.0, nearest_t(c, d, a)),
        (1.0, nearest_t(c, d, b)),
        (nearest_t(a, b, c), 0.0),
        (nearest_t(a, b, d), 1.0),
    ]
    .into_iter()
    .min_by(|x, y| gap(x).total_cmp(&gap(y)))
    .unwrap_or((0.0, 0.0))
}

/// Pads of `net` on the power-output pins of each part, by footprint id.
fn source_pads(p: &Project, net: &str) -> BTreeMap<Id, Vec<String>> {
    let mut out: BTreeMap<Id, Vec<String>> = BTreeMap::new();
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        for pin in &def.symbol.pins {
            if pin.electrical_type != PinType::PowerOut {
                continue;
            }
            for pad in def.pin_map.get(&pin.number).into_iter().flatten() {
                if fp.pad_nets.get(pad).is_some_and(|n| n == net) {
                    out.entry(fp.id.clone()).or_default().push(pad.clone());
                }
            }
        }
    }
    out
}

/// Conductance network: adjacency lists of (node, siemens).
type Network = Vec<Vec<(usize, f64)>>;

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Node voltages for 1 A injected at `inject` with `ground` held at 0 V; `None` if the
/// solver did not converge.
fn solve(g: &Network, ground: usize, inject: usize) -> Option<Vec<f64>> {
    let n = g.len();
    let diag: Vec<f64> = g
        .iter()
        .enumerate()
        .map(|(i, e)| {
            if i == ground {
                1.0
            } else {
                e.iter().map(|x| x.1).sum()
            }
        })
        .collect();
    let mul = |v: &[f64], out: &mut [f64]| {
        for (i, o) in out.iter_mut().enumerate() {
            *o = if i == ground {
                0.0
            } else {
                g[i].iter().map(|&(j, s)| s * (v[i] - v[j])).sum()
            };
        }
    };
    let precondition =
        |r: &[f64]| -> Vec<f64> { r.iter().zip(&diag).map(|(a, d)| a / d).collect() };
    let mut x = vec![0.0; n];
    let mut r = vec![0.0; n];
    r[inject] = 1.0;
    let mut dir = precondition(&r);
    let mut rz = dot(&r, &dir);
    let mut ad = vec![0.0; n];
    for _ in 0..(10 * n + 100) {
        mul(&dir, &mut ad);
        let curv = dot(&dir, &ad);
        if curv.is_nan() || curv <= 0.0 {
            return None;
        }
        let alpha = rz / curv;
        x.iter_mut()
            .zip(&dir)
            .for_each(|(xi, di)| *xi += alpha * di);
        r.iter_mut().zip(&ad).for_each(|(ri, ai)| *ri -= alpha * ai);
        if dot(&r, &r).sqrt() <= 1e-12 {
            return Some(x);
        }
        let z = precondition(&r);
        let rz2 = dot(&r, &z);
        let beta = rz2 / rz;
        rz = rz2;
        dir.iter_mut()
            .zip(&z)
            .for_each(|(di, zi)| *di = zi + beta * *di);
    }
    None
}

/// Index of the DSU root of `x` in `idx`, added when new.
fn root_index(dsu: &mut Dsu, x: usize, idx: &mut BTreeMap<usize, usize>) -> usize {
    let r = dsu.find(x);
    let n = idx.len();
    *idx.entry(r).or_insert(n)
}

/// Copper network of `net` and the resistance from its source to each sink pad.
pub fn net_path(p: &Project, net: &str) -> Result<NetPath, PathError> {
    let c = &p.constraints;
    let st = &p.pcb.board.stackup;
    let ambient = c
        .ambient_temp_c
        .ok_or_else(|| unknown("missing input: ambient temperature"))?;
    let temp_k = ambient + c.temp_rise_c + 273.15;
    if p.pcb.zones.values().any(|z| z.net == net) {
        // shortcut: pours are not in the network yet, so a poured net is UNKNOWN; next increment.
        return Err(unknown(
            "net has a copper zone: conduction through zones is not modelled yet",
        ));
    }
    let items: Vec<_> = copper::items(p)
        .into_iter()
        .filter(|i| i.net == net)
        .collect();

    let mut dsu = Dsu(vec![]);
    let mut pad_node: BTreeMap<(Id, String), usize> = BTreeMap::new();
    let mut via_node: BTreeMap<(Id, CopperLayer), usize> = BTreeMap::new();
    // Node of each pad / via item; contact nodes (t, node) along each track item.
    let mut base = vec![usize::MAX; items.len()];
    let mut contacts: Vec<Vec<(f64, usize)>> = vec![vec![]; items.len()];
    for (i, it) in items.iter().enumerate() {
        base[i] = match it.kind {
            ItemKind::Pad => *pad_node
                .entry((it.object_id.clone(), it.sub.clone()))
                .or_insert_with(|| dsu.add()),
            ItemKind::Via => *via_node
                .entry((it.object_id.clone(), it.layer))
                .or_insert_with(|| dsu.add()),
            _ => continue,
        };
    }
    if pad_node.len() < 2 {
        return Err(unknown("net has fewer than two pads: no path to evaluate"));
    }
    let seg = |i: usize| match &items[i].shape.core {
        Core::Segment(a, b) => (f2(*a), f2(*b)),
        Core::Polygon(v) => (f2(v[0]), f2(v[0])),
    };
    let boxes: Vec<_> = items.iter().map(|i| i.shape.bbox()).collect();
    for i in 0..items.len() {
        for j in i + 1..items.len() {
            let (a, b) = (&items[i], &items[j]);
            if a.layer != b.layer
                || !boxes[i].inflate(1).intersects(&boxes[j])
                || copper::distance(&a.shape, &b.shape) > 0.0
            {
                continue;
            }
            match (a.kind == ItemKind::Track, b.kind == ItemKind::Track) {
                (false, false) => dsu.union(base[i], base[j]),
                // Chords of one (arc) track run in series: only consecutive chords meet, at
                // their shared vertex. A short chord narrower than the track would otherwise
                // let non-adjacent chords touch and short the arc out.
                (true, true) if a.object_id == b.object_id => {
                    if j == i + 1 {
                        let n = dsu.add();
                        contacts[i].push((1.0, n));
                        contacts[j].push((0.0, n));
                    }
                }
                (true, true) => {
                    let ((p0, p1), (q0, q1)) = (seg(i), seg(j));
                    let (t, u) = closest_params(p0, p1, q0, q1);
                    let n = dsu.add();
                    contacts[i].push((t, n));
                    contacts[j].push((u, n));
                }
                _ => {
                    let (tr, other) = if a.kind == ItemKind::Track {
                        (i, j)
                    } else {
                        (j, i)
                    };
                    let (p0, p1) = seg(tr);
                    let t = nearest_t(p0, p1, core_center(&items[other].shape.core));
                    contacts[tr].push((t, base[other]));
                }
            }
        }
    }
    // Resistors (node, node, Ω), resolved to DSU roots once every zero-ohm join is known.
    let mut res: Vec<(usize, usize, f64)> = vec![];
    for (i, it) in items.iter().enumerate() {
        if it.kind != ItemKind::Track || contacts[i].len() < 2 {
            continue;
        }
        let oz = st
            .copper_oz(it.layer)
            .filter(|oz| *oz > 0.0)
            .ok_or_else(|| {
                unknown(format!(
                    "missing input: copper weight of layer {}",
                    it.layer.name()
                ))
            })?;
        let thick = copper_thickness_from_oz(oz)
            .map_err(|e| unknown(e.to_string()))?
            .si_magnitude();
        let (p0, p1) = seg(i);
        let len_m = d2(p0, p1).sqrt() * 1e-9;
        let w_m = it.shape.radius * 2.0 * 1e-9;
        let mut cs = contacts[i].clone();
        cs.sort_by(|x, y| x.0.total_cmp(&y.0));
        for w in cs.windows(2) {
            let l = (w[1].0 - w[0].0) * len_m;
            if l <= 1e-9 {
                dsu.union(w[0].1, w[1].1);
            } else {
                let r = copper_resistance_ohm(l, w_m, thick, temp_k)
                    .map_err(|e| (Verdict::Blocked, format!("track {}: {e}", it.object_id)))?;
                res.push((w[0].1, w[1].1, r));
            }
        }
    }
    for v in p.pcb.vias.values().filter(|v| v.net == net) {
        let layers = v.layers(st);
        if layers.len() < 2 {
            continue;
        }
        let tp = c
            .via_plating_um
            .ok_or_else(|| unknown("missing input: via plating thickness"))?
            * 1e-6;
        let d = v.drill as f64 * 1e-9;
        let foil = |k: usize| st.layers[k].copper_oz * COPPER_UM_PER_OZ;
        for w in layers.windows(2) {
            let (Some(ia), Some(ib)) = (st.index_of(w[0]), st.index_of(w[1])) else {
                continue;
            };
            let (Some(&na), Some(&nb)) = (
                via_node.get(&(v.id.clone(), w[0])),
                via_node.get(&(v.id.clone(), w[1])),
            ) else {
                continue;
            };
            let h_um =
                st.layers[ia.min(ib)].dielectric_after_um as f64 + (foil(ia) + foil(ib)) / 2.0;
            let annulus = std::f64::consts::PI * (d + tp);
            let r = copper_resistance_ohm(h_um * 1e-6, annulus, tp, temp_k)
                .map_err(|e| (Verdict::Blocked, format!("via {}: {e}", v.id)))?;
            res.push((na, nb, r));
        }
    }
    let label = |(fp, pad): &(Id, String)| {
        let refdes = p
            .pcb
            .footprints
            .get(fp)
            .map_or(fp.as_str(), |f| f.refdes.as_str());
        format!("{refdes}.{pad}")
    };
    let sources = source_pads(p, net);
    let mut source: Option<Vec<(Id, String)>> = None;
    if let (1, Some((fp, pads))) = (sources.len(), sources.iter().next()) {
        let keys: Vec<(Id, String)> = pads
            .iter()
            .map(|pad| (fp.clone(), pad.clone()))
            .filter(|k| pad_node.contains_key(k))
            .collect();
        // The part ties its power-output pads together internally.
        for w in keys.windows(2) {
            dsu.union(pad_node[&w[0]], pad_node[&w[1]]);
        }
        source = (!keys.is_empty()).then_some(keys);
    }

    // Compact roots and build the conductance network.
    let mut idx: BTreeMap<usize, usize> = BTreeMap::new();
    let pads: Vec<((Id, String), usize)> = pad_node
        .iter()
        .map(|(k, &n)| (k.clone(), root_index(&mut dsu, n, &mut idx)))
        .collect();
    let edges: Vec<(usize, usize, f64)> = res
        .iter()
        .map(|&(a, b, r)| {
            let na = root_index(&mut dsu, a, &mut idx);
            (na, root_index(&mut dsu, b, &mut idx), r)
        })
        .filter(|e| e.0 != e.1)
        .collect();
    let mut g: Network = vec![vec![]; idx.len()];
    for &(a, b, r) in &edges {
        g[a].push((b, 1.0 / r));
        g[b].push((a, 1.0 / r));
    }
    let ground = match &source {
        Some(keys) => pads.iter().find(|(k, _)| k == &keys[0]).map_or(0, |x| x.1),
        None => pads[0].1,
    };
    // Unrouted: pads the ground pad's copper does not reach.
    let mut seen = vec![false; g.len()];
    let mut queue = VecDeque::from([ground]);
    seen[ground] = true;
    while let Some(x) = queue.pop_front() {
        for &(y, _) in &g[x] {
            if !seen[y] {
                seen[y] = true;
                queue.push_back(y);
            }
        }
    }
    let unreached: Vec<String> = pads
        .iter()
        .filter(|(_, n)| !seen[*n])
        .map(|(k, _)| label(k))
        .collect();
    if !unreached.is_empty() {
        return Err(unknown(format!(
            "net unrouted: {} not connected by copper",
            unreached.join(", ")
        )));
    }
    let blocked = || {
        (
            Verdict::Blocked,
            "network solver did not converge".to_string(),
        )
    };
    let path = |from: String, k: &(Id, String), r: f64| {
        let at = p
            .pcb
            .footprints
            .get(&k.0)
            .and_then(|f| {
                let def = p.library.get(&f.component_id)?;
                let pad = def.footprint.pads.iter().find(|x| x.number == k.1)?;
                Some(p.pad_position(f, pad))
            })
            .unwrap_or_default();
        PadPath {
            from,
            to: label(k),
            to_footprint: k.0.clone(),
            at,
            resistance_ohm: r,
        }
    };
    let mut paths = vec![];
    if let Some(keys) = &source {
        let from = keys.iter().map(label).collect::<Vec<_>>().join("+");
        for (k, n) in pads.iter().filter(|(k, _)| !keys.contains(k)) {
            let r = if *n == ground {
                0.0
            } else {
                solve(&g, ground, *n).ok_or_else(blocked)?[*n]
            };
            paths.push(path(from.clone(), k, r));
        }
    } else {
        // Columns of the grounded network's inverse: R(i, j) = Zii + Zjj − 2·Zij.
        let roots: BTreeSet<usize> = pads.iter().map(|x| x.1).collect();
        let mut z: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
        for &n in roots.iter().filter(|&&n| n != ground) {
            z.insert(n, solve(&g, ground, n).ok_or_else(blocked)?);
        }
        let zz = |a: usize, b: usize| z.get(&a).map_or(0.0, |col| col[b]);
        let mut worst = f64::NEG_INFINITY;
        for (i, a) in pads.iter().enumerate() {
            for b in &pads[i + 1..] {
                let r = (zz(a.1, a.1) + zz(b.1, b.1) - 2.0 * zz(a.1, b.1)).max(0.0);
                if r > worst {
                    worst = r;
                    paths = vec![path(label(&a.0), &b.0, r)];
                }
            }
        }
    }
    Ok(NetPath {
        net: net.into(),
        source: source.map(|keys| keys.iter().map(label).collect()),
        paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::{ALPHA_CU_20C_PER_K, RHO_CU_20C_OHM_M};
    use eak_core::geom::mm;
    use eak_core::{FootprintInstance, Rotation, Side, Stackup, Track, Via};

    /// ρ at 25 °C ambient + 10 °C rise = 35 °C (hand formula, IEC 60028 constants).
    fn rho35() -> f64 {
        RHO_CU_20C_OHM_M * (1.0 + ALPHA_CU_20C_PER_K * 15.0)
    }
    const T1OZ: f64 = COPPER_UM_PER_OZ * 1e-6;

    fn base() -> Project {
        let mut p = Project::new("vdrop");
        p.pcb.board.stackup = Stackup::two_layer(mm(1.6), 1.0, "FR-4", 4.5);
        p.constraints.ambient_temp_c = Some(25.0);
        p.constraints.temp_rise_c = 10.0;
        for id in ["CONN_1x02_P2.54", "R_0603"] {
            let def = eak_core::builtin::get(id).unwrap();
            p.library.insert(def.id.clone(), def);
        }
        p
    }

    /// Footprint `id` of `component` with pad 1 on PWR; returns pad 1's position.
    fn place(p: &mut Project, id: &str, component: &str, at: Point, side: Side) -> Point {
        let fp = FootprintInstance {
            id: id.into(),
            symbol_id: format!("s{id}"),
            component_id: component.into(),
            refdes: id.into(),
            position: at,
            rotation: Rotation::R0,
            side,
            pad_nets: [("1".to_string(), "PWR".to_string())].into(),
            locked: false,
        };
        let def = &p.library[component];
        let pad = def.footprint.pads.iter().find(|x| x.number == "1").unwrap();
        let pos = p.pad_position(&fp, pad);
        p.pcb.footprints.insert(id.into(), fp);
        pos
    }

    fn track(p: &mut Project, id: &str, layer: CopperLayer, w: f64, a: Point, b: Point) {
        let t = Track {
            id: id.into(),
            layer,
            width: mm(w),
            a,
            b,
            net: "PWR".into(),
            arc_deg: None,
        };
        p.pcb.tracks.insert(id.into(), t);
    }

    fn worst(p: &Project) -> f64 {
        net_path(p, "PWR").unwrap().worst().unwrap().resistance_ohm
    }

    #[test]
    fn straight_track_matches_the_hand_calculation() {
        let mut p = base();
        let a = place(
            &mut p,
            "J1",
            "CONN_1x02_P2.54",
            Point::mm(0.0, 0.0),
            Side::Top,
        );
        let b = place(
            &mut p,
            "J2",
            "CONN_1x02_P2.54",
            Point::mm(20.0, 0.0),
            Side::Top,
        );
        track(&mut p, "t", CopperLayer::Top, 0.5, a, b);
        // 20 mm × 0.5 mm × 1 oz (34.8 µm) at 35 °C: ρ·L/(w·t) ≈ 0.02099 Ω.
        let hand = rho35() * 0.020 / (0.5e-3 * T1OZ);
        assert!((hand - 0.02099).abs() < 1e-4, "{hand}");
        let r = worst(&p);
        assert!((r - hand).abs() / hand < 1e-9, "{r} vs {hand}");
    }

    #[test]
    fn track_via_track_adds_the_barrel() {
        let mut p = base();
        p.constraints.via_plating_um = Some(25.0);
        let a = place(&mut p, "R1", "R_0603", Point::mm(0.0, 0.0), Side::Top);
        let b = place(&mut p, "R2", "R_0603", Point::mm(20.0, 0.0), Side::Bottom);
        let v = Point::new(mm(10.0), a.y);
        track(&mut p, "t1", CopperLayer::Top, 0.3, a, v);
        track(&mut p, "t2", CopperLayer::Bottom, 0.3, v, b);
        let via = Via {
            id: "v".into(),
            position: v,
            drill: mm(0.3),
            diameter: mm(0.6),
            net: "PWR".into(),
            from: CopperLayer::Top,
            to: CopperLayer::Bottom,
        };
        p.pcb.vias.insert("v".into(), via);
        let tr = |q: Point, r: Point| rho35() * q.dist(r) * 1e-9 / (0.3e-3 * T1OZ);
        // Barrel: h = dielectric + two half foils, annulus π·(d + tp)·tp.
        let diel = p.pcb.board.stackup.layers[0].dielectric_after_um as f64;
        let h = (diel + COPPER_UM_PER_OZ) * 1e-6;
        let barrel = rho35() * h / (std::f64::consts::PI * (0.3e-3 + 25e-6) * 25e-6);
        let hand = tr(a, v) + barrel + tr(v, b);
        let r = worst(&p);
        assert!((r - hand).abs() / hand < 1e-9, "{r} vs {hand}");
        p.constraints.via_plating_um = None;
        let e = net_path(&p, "PWR").unwrap_err();
        assert_eq!(e, unknown("missing input: via plating thickness"));
    }

    #[test]
    fn two_parallel_paths_halve_the_resistance() {
        let mut p = base();
        let a = place(
            &mut p,
            "J1",
            "CONN_1x02_P2.54",
            Point::mm(0.0, 0.0),
            Side::Top,
        );
        let b = place(
            &mut p,
            "J2",
            "CONN_1x02_P2.54",
            Point::mm(20.0, 0.0),
            Side::Top,
        );
        let off = |q: Point, dy: f64| Point::new(q.x, q.y + mm(dy));
        track(&mut p, "u", CopperLayer::Top, 0.3, off(a, 0.5), off(b, 0.5));
        track(
            &mut p,
            "d",
            CopperLayer::Top,
            0.3,
            off(a, -0.5),
            off(b, -0.5),
        );
        let one = rho35() * 0.020 / (0.3e-3 * T1OZ);
        let r = worst(&p);
        assert!((r - one / 2.0).abs() / one < 1e-9, "{r} vs {}", one / 2.0);
    }

    #[test]
    fn source_pin_gives_a_path_to_each_sink_and_unrouted_is_unknown() {
        let mut p = base();
        let mut src = p.library["CONN_1x02_P2.54"].clone();
        src.id = "SRC".into();
        src.symbol.pins[0].electrical_type = PinType::PowerOut;
        p.library.insert("SRC".into(), src);
        let a = place(&mut p, "J1", "SRC", Point::mm(0.0, 0.0), Side::Top);
        place(
            &mut p,
            "J2",
            "CONN_1x02_P2.54",
            Point::mm(10.0, 0.0),
            Side::Top,
        );
        let c = place(
            &mut p,
            "J3",
            "CONN_1x02_P2.54",
            Point::mm(20.0, 0.0),
            Side::Top,
        );
        // One track through J2's pad: the network splits it there.
        track(&mut p, "t", CopperLayer::Top, 0.5, a, c);
        let np = net_path(&p, "PWR").unwrap();
        assert_eq!(np.source, Some(vec!["J1.1".to_string()]));
        let r: BTreeMap<_, _> = np
            .paths
            .iter()
            .map(|x| (x.to.as_str(), x.resistance_ohm))
            .collect();
        let full = rho35() * 0.020 / (0.5e-3 * T1OZ);
        assert!((r["J3.1"] - full).abs() / full < 1e-9, "{r:?}");
        assert!((r["J2.1"] - full / 2.0).abs() / full < 1e-9, "{r:?}");
        p.pcb.tracks.clear();
        let e = net_path(&p, "PWR").unwrap_err();
        assert!(e.1.contains("net unrouted: J2.1, J3.1"), "{}", e.1);
        p.constraints.ambient_temp_c = None;
        let e = net_path(&p, "PWR").unwrap_err();
        assert_eq!(e, unknown("missing input: ambient temperature"));
    }

    #[test]
    fn arc_track_counts_its_full_length() {
        // Regression (review of PR 143, B1): a 1 mm wide arc flattens into 0.89 mm chords, so
        // non-adjacent chords touched and shorted the arc out.
        let mut p = base();
        let a = place(
            &mut p,
            "J1",
            "CONN_1x02_P2.54",
            Point::mm(-5.0, -1.27),
            Side::Top,
        );
        let b = place(
            &mut p,
            "J2",
            "CONN_1x02_P2.54",
            Point::mm(10.0, 13.73),
            Side::Top,
        );
        let (s, e) = (Point::mm(0.0, 0.0), Point::mm(10.0, 10.0));
        track(&mut p, "l1", CopperLayer::Top, 0.2, a, s);
        track(&mut p, "l2", CopperLayer::Top, 0.2, e, b);
        // 90° counter-clockwise about (0, 10): r = 10 mm.
        track(&mut p, "arc", CopperLayer::Top, 1.0, s, e);
        p.pcb.tracks.get_mut("arc").unwrap().arc_deg = Some(90.0);
        // Hand: two 5 mm × 0.2 mm leads + ρ·r·θ/(w·t) for the arc.
        let leads = 2.0 * rho35() * 0.005 / (0.2e-3 * T1OZ);
        let arc = rho35() * 0.010 * std::f64::consts::FRAC_PI_2 / (1e-3 * T1OZ);
        let hand = leads + arc;
        let r = worst(&p);
        // The 10 µm chord flattening shortens the arc by about 0.03 %.
        assert!((r - hand).abs() / hand < 3e-3, "{r} vs {hand}");
    }

    #[test]
    fn a_short_chamfer_on_a_wide_track_loses_at_most_one_width() {
        let mut p = base();
        let a = place(
            &mut p,
            "J1",
            "CONN_1x02_P2.54",
            Point::mm(0.0, -1.27),
            Side::Top,
        );
        let b = place(
            &mut p,
            "J2",
            "CONN_1x02_P2.54",
            Point::mm(10.3, 9.03),
            Side::Top,
        );
        let (k1, k2) = (Point::mm(10.0, 0.0), Point::mm(10.3, 0.3));
        track(&mut p, "t1", CopperLayer::Top, 1.0, a, k1);
        track(&mut p, "t2", CopperLayer::Top, 1.0, k1, k2);
        track(&mut p, "t3", CopperLayer::Top, 1.0, k2, b);
        let len = a.dist(k1) + k1.dist(k2) + k2.dist(b);
        let per_m = rho35() / (1e-3 * T1OZ);
        let (hi, lo) = (per_m * len * 1e-9, per_m * (len * 1e-9 - 1e-3));
        let r = worst(&p);
        assert!(r <= hi * (1.0 + 1e-9) && r >= lo, "{r} not in [{lo}, {hi}]");
    }

    #[test]
    fn closest_params_of_crossing_and_apart_segments() {
        let (t, u) = closest_params((0.0, 0.0), (2.0, 0.0), (1.0, -1.0), (1.0, 1.0));
        assert_eq!((t, u), (0.5, 0.5));
        let (t, u) = closest_params((0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0));
        assert_eq!((t, u), (1.0, 0.0));
    }
}
