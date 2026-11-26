//! Placement intelligence: the mental models behind component placement, as deterministic
//! rules over the one EAK-IR (no second design model).
//!
//! Ported (as ideas, in Rust) from the MIT-licensed DESK project's placement engine
//! (`third_party/licenses/DESK/`). The hierarchy, strictly in this order:
//! mechanical integrity (mounting holes, connectors at the board edge) → footprint integrity
//! (courtyards never overlap; enforced by `placer`) → electrical integrity (decoupling
//! capacitors, crystal load capacitors and ESD parts snapped to the pin they serve) →
//! functional clustering (passives grouped around the IC they share nets with) → routability.
//!
//! `plan` feeds `placer::place` (the `AutoPlace` command); `analyze` is the read-only
//! `analyze_placement` harness tool. Roles come from reference-designator prefixes and net
//! names: a heuristic, so every result carries its limitations and is never a certification.

use eak_core::geom::point_segment_distance;
use eak_core::{Id, Point, Project, Rect};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A bypass capacitor must sit this close to the IC pad it serves (DESK `decap_max_mm`).
pub const DECAP_MAX_MM: f64 = 2.0;
/// Connectors and screw terminals belong at the board edge: centre within this distance.
pub const CONNECTOR_EDGE_MAX_MM: f64 = 8.0;
/// Mounting holes sit this far in from the board corners (DESK uses 4 mm).
pub const MOUNT_HOLE_INSET_MM: f64 = 4.0;
/// Screw head / standoff keep-out radius around a mounting hole (6 mm standoff + 0.5 mm tool).
pub const MOUNT_HOLE_KEEPOUT_MM: f64 = 3.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    MountingHole,
    Connector,
    Ic,
    Crystal,
    DecouplingCapacitor,
    CrystalLoadCapacitor,
    Protection,
    Passive,
    Other,
}

impl Role {
    /// Placement phase: lower phases are placed first (mechanical before electrical).
    pub fn phase(self) -> u8 {
        match self {
            Role::MountingHole => 0,
            Role::Connector => 1,
            Role::Ic | Role::Crystal => 2,
            _ => 3,
        }
    }
}

/// The pad of another footprint a part must stay close to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnchorTarget {
    pub footprint_id: Id,
    pub pad: String,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub roles: BTreeMap<Id, Role>,
    /// Candidate anchor pads per snapped part; the nearest placed one wins.
    pub anchors: BTreeMap<Id, Vec<AnchorTarget>>,
    pub reasons: BTreeMap<Id, String>,
}

impl Plan {
    pub fn role(&self, id: &str) -> Role {
        self.roles.get(id).copied().unwrap_or(Role::Other)
    }
}

fn prefix(refdes: &str) -> String {
    refdes
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_uppercase()
}

pub fn is_ground(net: &str) -> bool {
    let n = net.to_ascii_uppercase();
    n.contains("GND") || n.starts_with("VSS") || n == "0"
}

pub fn is_power(net: &str) -> bool {
    if net.is_empty() || is_ground(net) {
        return false;
    }
    let n = net.to_ascii_uppercase();
    [
        "VCC", "VDD", "VBAT", "VBUS", "VIN", "VOUT", "3V3", "5V", "1V8", "1V2", "12V",
    ]
    .iter()
    .any(|k| n.contains(k))
        || n.starts_with('+')
}

fn base_role(refdes: &str) -> Role {
    match prefix(refdes).as_str() {
        "MH" | "H" => Role::MountingHole,
        "J" | "P" | "CN" | "USB" | "FPC" | "CON" | "BAT" | "ANT" | "TB" => Role::Connector,
        "U" | "IC" | "Q" => Role::Ic,
        "Y" | "XTAL" => Role::Crystal,
        "D" | "TVS" | "ESD" | "ZD" => Role::Protection,
        "R" | "C" | "L" | "FB" => Role::Passive,
        _ => Role::Other,
    }
}

/// Classify every footprint and compute topological snaps.
pub fn plan(p: &Project) -> Plan {
    let mut out = Plan::default();
    for fp in p.pcb.footprints.values() {
        out.roles.insert(fp.id.clone(), base_role(&fp.refdes));
    }
    let nets_of = |id: &str| -> BTreeMap<String, String> {
        p.pcb
            .footprints
            .get(id)
            .map(|f| f.pad_nets.clone())
            .unwrap_or_default()
    };
    // In refdes order, not id order: ids are random per run and the anchor candidates built from
    // these lists break placement ties, so the same board would place differently each run.
    let ids_of = |r: Role, roles: &BTreeMap<Id, Role>| -> Vec<Id> {
        let mut v: Vec<Id> = roles
            .iter()
            .filter(|(_, x)| **x == r)
            .map(|(id, _)| id.clone())
            .collect();
        v.sort_by_key(|id| p.pcb.footprints[id].refdes.clone());
        v
    };
    let ics = ids_of(Role::Ic, &out.roles);
    let crystals = ids_of(Role::Crystal, &out.roles);
    let connectors = ids_of(Role::Connector, &out.roles);
    // Pads of `targets` that carry any net in `wanted`.
    let pads_on = |targets: &[Id], wanted: &BTreeSet<String>| -> Vec<AnchorTarget> {
        let mut v = vec![];
        for t in targets {
            for (pad, n) in nets_of(t) {
                if wanted.contains(&n) {
                    v.push(AnchorTarget {
                        footprint_id: t.clone(),
                        pad,
                    });
                }
            }
        }
        v
    };

    let ids: Vec<Id> = p.pcb.footprints.keys().cloned().collect();
    for id in ids {
        let fp = &p.pcb.footprints[&id];
        let nets = fp.pad_nets.clone();
        let two_pad = nets.len() == 2;
        let gnd_side = nets.values().any(|n| is_ground(n));
        let other: Vec<String> = nets.values().filter(|n| !is_ground(n)).cloned().collect();

        if prefix(&fp.refdes) == "C" && two_pad && gnd_side && other.len() == 1 {
            let net = other[0].clone();
            let wanted: BTreeSet<String> = [net.clone()].into();
            if is_power(&net) {
                let cands = pads_on(&ics, &wanted);
                if !cands.is_empty() {
                    out.roles.insert(id.clone(), Role::DecouplingCapacitor);
                    out.reasons.insert(
                        id.clone(),
                        format!(
                            "decoupling on {net}: keep within {DECAP_MAX_MM} mm of the IC supply pad"
                        ),
                    );
                    out.anchors.insert(id.clone(), cands);
                    continue;
                }
            }
            let cands = pads_on(&crystals, &wanted);
            if !cands.is_empty() {
                out.roles.insert(id.clone(), Role::CrystalLoadCapacitor);
                out.reasons.insert(
                    id.clone(),
                    format!("crystal load capacitor on {net}: next to the crystal pad"),
                );
                out.anchors.insert(id.clone(), cands);
                continue;
            }
        }
        if out.role(&id) == Role::Protection {
            let wanted: BTreeSet<String> = other.into_iter().collect();
            let cands = pads_on(&connectors, &wanted);
            if !cands.is_empty() {
                out.reasons.insert(
                    id.clone(),
                    "ESD/protection part: immediately behind the connector pin it protects".into(),
                );
                out.anchors.insert(id.clone(), cands);
            }
        }
    }
    out
}

/// Absolute position of `pad` on footprint `id` if the footprint stood at `pos`/`rot`.
pub fn pad_at(
    p: &Project,
    id: &str,
    pos: Point,
    rot: eak_core::Rotation,
    pad: &str,
) -> Option<Point> {
    let fp = p.pcb.footprints.get(id)?;
    let def = p.library.get(&fp.component_id)?;
    let pd = def.footprint.pads.iter().find(|x| x.number == pad)?;
    let mut f = fp.clone();
    f.position = pos;
    f.rotation = rot;
    Some(p.pad_position(&f, pd))
}

/// Distance from `at` to the nearest board edge segment (nm).
pub fn edge_distance(at: Point, outline: &[Point]) -> f64 {
    (0..outline.len())
        .map(|i| point_segment_distance(at, outline[i], outline[(i + 1) % outline.len()]))
        .fold(f64::INFINITY, f64::min)
}

/// The four inset corner targets for mounting holes (bounding box of the outline).
pub fn corner_targets(bb: &Rect, inset: i64) -> [Point; 4] {
    [
        Point::new(bb.min.x + inset, bb.min.y + inset),
        Point::new(bb.max.x - inset, bb.min.y + inset),
        Point::new(bb.max.x - inset, bb.max.y - inset),
        Point::new(bb.min.x + inset, bb.max.y - inset),
    ]
}

// ---------------------------------------------------------------- analysis (read-only tool)

#[derive(Debug, Clone, Serialize)]
pub struct RoleEntry {
    pub refdes: String,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize)]
pub struct SnapEntry {
    pub refdes: String,
    pub role: Role,
    pub anchor: String,
    pub anchor_pad: String,
    pub reason: String,
    pub distance_mm: f64,
    pub within_limit: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cluster {
    pub ic: String,
    pub parts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub rule: String,
    pub refdes: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetLength {
    pub net: String,
    pub airwire_mm: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    /// Measured, recomputed from the IR: sum of per-net minimum-spanning-tree pad distances.
    pub total_airwire_mm: f64,
    pub longest_nets: Vec<NetLength>,
    pub hierarchy: Vec<&'static str>,
    pub roles: Vec<RoleEntry>,
    pub snaps: Vec<SnapEntry>,
    pub clusters: Vec<Cluster>,
    pub findings: Vec<Finding>,
    pub limitations: Vec<&'static str>,
}

pub fn analyze(p: &Project) -> Analysis {
    let pl = plan(p);
    let outline = &p.pcb.board.outline;
    let (total, longest) = airwire(p);
    let mut a = Analysis {
        total_airwire_mm: total,
        longest_nets: longest,
        hierarchy: vec![
            "1 mechanical integrity: mounting holes in corners, connectors at the board edge",
            "2 footprint integrity: courtyards never overlap, inside the board edge clearance",
            "3 electrical integrity: decoupling / crystal load / ESD parts snapped to their pin",
            "4 functional clustering: passives grouped around the IC they share nets with",
            "5 routability: connected parts close together, short ratsnest",
        ],
        roles: vec![],
        snaps: vec![],
        clusters: vec![],
        findings: vec![],
        limitations: vec![
            "roles are inferred from reference-designator prefixes and net names (heuristic)",
            "distances are centre-to-pad on the current placement; no thermal, EMC, RF, impedance or assembly-access analysis",
            "not evaluated: switching-node separation, analog/digital partitioning, RF and antenna keep-outs, differential pairs, escape feasibility, copper area",
            "a clean result is not a certification; run DRC / MECH / DFM for verification",
        ],
    };
    for fp in p.pcb.footprints.values() {
        a.roles.push(RoleEntry {
            refdes: fp.refdes.clone(),
            role: pl.role(&fp.id),
        });
    }
    a.roles.sort_by(|x, y| x.refdes.cmp(&y.refdes));

    for (id, cands) in &pl.anchors {
        let fp = &p.pcb.footprints[id];
        let role = pl.role(id);
        let best = cands
            .iter()
            .filter_map(|c| {
                let anchor = p.pcb.footprints.get(&c.footprint_id)?;
                let pos = pad_at(p, &c.footprint_id, anchor.position, anchor.rotation, &c.pad)?;
                Some((fp.position.dist(pos), c, anchor.refdes.clone()))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0));
        let Some((d, c, anchor_ref)) = best else {
            continue;
        };
        let dmm = d / 1_000_000.0;
        let ok = (role == Role::DecouplingCapacitor).then_some(dmm <= DECAP_MAX_MM);
        if ok == Some(false) {
            a.findings.push(Finding {
                rule: "PLACE-DECAP-DISTANCE".into(),
                refdes: fp.refdes.clone(),
                message: format!(
                    "{} is {dmm:.2} mm from {anchor_ref}.{} (limit {DECAP_MAX_MM} mm); run AutoPlace or move it",
                    fp.refdes, c.pad
                ),
            });
        }
        a.snaps.push(SnapEntry {
            refdes: fp.refdes.clone(),
            role,
            anchor: anchor_ref,
            anchor_pad: c.pad.clone(),
            reason: pl.reasons.get(id).cloned().unwrap_or_default(),
            distance_mm: dmm,
            within_limit: ok,
        });
    }
    a.snaps.sort_by(|x, y| x.refdes.cmp(&y.refdes));

    if outline.len() >= 3 {
        for fp in p.pcb.footprints.values() {
            if pl.role(&fp.id) == Role::Connector {
                let d = edge_distance(fp.position, outline) / 1_000_000.0;
                if d > CONNECTOR_EDGE_MAX_MM {
                    a.findings.push(Finding {
                        rule: "PLACE-CONNECTOR-EDGE".into(),
                        refdes: fp.refdes.clone(),
                        message: format!(
                            "connector {} centre is {d:.1} mm from the board edge (expected <= {CONNECTOR_EDGE_MAX_MM} mm so cables can mate)",
                            fp.refdes
                        ),
                    });
                }
            }
        }
    }

    // Functional clusters: each passive joins the IC it shares the most signal nets with.
    let ics: Vec<Id> = pl
        .roles
        .iter()
        .filter(|(_, r)| **r == Role::Ic)
        .map(|(i, _)| i.clone())
        .collect();
    let signal = |id: &str| -> BTreeSet<String> {
        p.pcb.footprints[id]
            .pad_nets
            .values()
            .filter(|n| !is_ground(n))
            .cloned()
            .collect()
    };
    let mut members: BTreeMap<Id, Vec<String>> = ics.iter().map(|i| (i.clone(), vec![])).collect();
    for (id, role) in &pl.roles {
        if !matches!(
            role,
            Role::Passive
                | Role::DecouplingCapacitor
                | Role::CrystalLoadCapacitor
                | Role::Protection
        ) {
            continue;
        }
        let mine = signal(id);
        let best = ics
            .iter()
            .map(|ic| (signal(ic).intersection(&mine).count(), ic))
            .filter(|(n, _)| *n > 0)
            .max_by_key(|(n, _)| *n);
        if let Some((_, ic)) = best {
            if let Some(list) = members.get_mut(ic) {
                list.push(p.pcb.footprints[id].refdes.clone());
            }
        }
    }
    for (ic, mut parts) in members {
        parts.sort();
        a.clusters.push(Cluster {
            ic: p.pcb.footprints[&ic].refdes.clone(),
            parts,
        });
    }
    a.clusters.sort_by(|x, y| x.ic.cmp(&y.ic));
    thermal(p, &mut a);
    a
}

/// Per-net airwire length: minimum spanning tree (Prim) over the pad positions of each net.
fn airwire(p: &Project) -> (f64, Vec<NetLength>) {
    let mut by_net: BTreeMap<&str, Vec<Point>> = BTreeMap::new();
    for fp in p.pcb.footprints.values() {
        let Some(def) = p.library.get(&fp.component_id) else {
            continue;
        };
        for pad in &def.footprint.pads {
            if let Some(net) = fp.pad_nets.get(&pad.number) {
                if !net.is_empty() {
                    by_net
                        .entry(net.as_str())
                        .or_default()
                        .push(p.pad_position(fp, pad));
                }
            }
        }
    }
    let mut nets = vec![];
    for (net, pts) in by_net {
        if pts.len() < 2 {
            continue;
        }
        let mut in_tree = vec![false; pts.len()];
        let mut best = vec![f64::INFINITY; pts.len()];
        best[0] = 0.0;
        let mut len = 0.0;
        for _ in 0..pts.len() {
            let Some(i) = (0..pts.len())
                .filter(|i| !in_tree[*i])
                .min_by(|a, b| best[*a].total_cmp(&best[*b]))
            else {
                break;
            };
            in_tree[i] = true;
            len += best[i];
            for j in 0..pts.len() {
                if !in_tree[j] {
                    best[j] = best[j].min(pts[i].dist(pts[j]));
                }
            }
        }
        nets.push(NetLength {
            net: net.to_string(),
            airwire_mm: len / 1_000_000.0,
        });
    }
    let total = nets.iter().map(|n| n.airwire_mm).sum();
    nets.sort_by(|a, b| b.airwire_mm.total_cmp(&a.airwire_mm));
    nets.truncate(5);
    (total, nets)
}

/// Two heat-generating parts (regulators, power switches, drivers; classified by library
/// component name) closer than this (centre to centre) share heat: flagged, not failed.
const HOT_SPACING_MM: f64 = 6.0;

fn thermal(p: &Project, a: &mut Analysis) {
    let hot: Vec<_> = p
        .pcb
        .footprints
        .values()
        .filter(|f| {
            let id = f.component_id.to_ascii_uppercase();
            ["LDO", "REG", "BUCK", "BOOST", "MOSFET", "DRIVER", "AMP"]
                .iter()
                .any(|k| id.contains(k))
        })
        .collect();
    for (i, x) in hot.iter().enumerate() {
        for y in &hot[i + 1..] {
            let d = x.position.dist(y.position) / 1_000_000.0;
            if d < HOT_SPACING_MM {
                a.findings.push(Finding {
                    rule: "PLACE-THERMAL-SPACING".into(),
                    refdes: x.refdes.clone(),
                    message: format!(
                        "heat-generating {} and {} are {d:.1} mm apart (< {HOT_SPACING_MM} mm); spread them or add copper area (heuristic: classified by component name)",
                        x.refdes, y.refdes
                    ),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn net_classes() {
        assert!(is_ground("GND") && is_ground("AGND") && !is_ground("3V3"));
        assert!(is_power("3V3") && is_power("+5V") && is_power("VCC") && !is_power("GND"));
        assert!(!is_power("SCL"));
    }

    #[test]
    fn prefix_roles() {
        assert_eq!(base_role("MH1"), Role::MountingHole);
        assert_eq!(base_role("J12"), Role::Connector);
        assert_eq!(base_role("U3"), Role::Ic);
        assert_eq!(base_role("Y1"), Role::Crystal);
        assert_eq!(base_role("R7"), Role::Passive);
        assert!(Role::MountingHole.phase() < Role::Connector.phase());
        assert!(Role::Connector.phase() < Role::Ic.phase());
        assert!(Role::Ic.phase() < Role::DecouplingCapacitor.phase());
    }

    #[test]
    fn empty_project_analyses_cleanly() {
        let a = analyze(&Project::new("t"));
        assert!(a.roles.is_empty() && a.findings.is_empty());
        assert!(!a.limitations.is_empty());
    }
}
