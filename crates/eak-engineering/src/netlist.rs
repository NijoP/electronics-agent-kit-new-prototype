//! Schematic connectivity: resolves wires, pins and labels into canonical nets.
//!
//! Rules: wire endpoints join; a pin, label or wire endpoint lying on a wire joins that
//! wire; labels with the same name join globally (across sheets). Net names come from
//! labels; unnamed nets get a deterministic name from their lowest pin.

use eak_core::geom::point_segment_distance;
use eak_core::{Id, LabelKind, PinType, Point, Project};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PinRef {
    pub symbol_id: Id,
    pub refdes: String,
    pub pin: String,
    pub pin_name: String,
    pub electrical_type: PinType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Net {
    pub name: String,
    pub pins: Vec<PinRef>,
    pub label_ids: Vec<Id>,
    pub wire_ids: Vec<Id>,
    /// Driven by a power port label.
    pub has_power_port: bool,
    /// Distinct label names found on this net (more than one = naming conflict).
    pub label_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Netlist {
    pub nets: BTreeMap<String, Net>,
    /// (symbol id, pin) -> net name
    pub pin_net: BTreeMap<(Id, String), String>,
    /// Wires that connect to nothing at one or both ends.
    pub dangling_wire_ends: Vec<(Id, Point)>,
}

struct Dsu(Vec<usize>);
impl Dsu {
    fn new(n: usize) -> Self {
        Dsu((0..n).collect())
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut c = x;
        while self.0[c] != r {
            let n = self.0[c];
            self.0[c] = r;
            c = n;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra.max(rb)] = ra.min(rb);
        }
    }
}

enum Node {
    Pin(PinRef),
    Label(Id),
    WireEnd(Id),
}

/// Tolerance for "point lies on wire" (schematic coordinates are grid-aligned).
const ON_WIRE_TOL_NM: f64 = 1_000.0;

pub fn build(p: &Project) -> Netlist {
    let mut nodes: Vec<(Id, Point, Node)> = Vec::new(); // (sheet, position, node)
    for s in p.schematic.symbols.values() {
        let Some(def) = p.library.get(&s.component_id) else {
            continue;
        };
        for pin in &def.symbol.pins {
            let pos = s.place(pin.position);
            nodes.push((
                s.sheet_id.clone(),
                pos,
                Node::Pin(PinRef {
                    symbol_id: s.id.clone(),
                    refdes: s.refdes.clone(),
                    pin: pin.number.clone(),
                    pin_name: pin.name.clone(),
                    electrical_type: pin.electrical_type,
                }),
            ));
        }
    }
    for l in p.schematic.labels.values() {
        nodes.push((l.sheet_id.clone(), l.position, Node::Label(l.id.clone())));
    }
    let wire_base = nodes.len();
    let wires: Vec<_> = p.schematic.wires.values().collect();
    for w in &wires {
        nodes.push((w.sheet_id.clone(), w.a, Node::WireEnd(w.id.clone())));
        nodes.push((w.sheet_id.clone(), w.b, Node::WireEnd(w.id.clone())));
    }
    let mut dsu = Dsu::new(nodes.len());
    let mut touches = vec![0u32; nodes.len()];

    // Each wire's two ends are one conductor.
    for i in 0..wires.len() {
        dsu.union(wire_base + 2 * i, wire_base + 2 * i + 1);
    }
    // Coincident points on the same sheet join.
    let mut by_pos: BTreeMap<(&Id, Point), Vec<usize>> = BTreeMap::new();
    for (i, (sheet, pos, _)) in nodes.iter().enumerate() {
        by_pos.entry((sheet, *pos)).or_default().push(i);
    }
    for group in by_pos.values() {
        for w in group.windows(2) {
            dsu.union(w[0], w[1]);
        }
        if group.len() > 1 {
            for &i in group {
                touches[i] += 1;
            }
        }
    }
    // Any node lying in the interior of a wire joins it (T-junctions, pins on wires).
    for (wi, w) in wires.iter().enumerate() {
        for (i, (sheet, pos, _)) in nodes.iter().enumerate() {
            if *sheet != w.sheet_id || i == wire_base + 2 * wi || i == wire_base + 2 * wi + 1 {
                continue;
            }
            if point_segment_distance(*pos, w.a, w.b) <= ON_WIRE_TOL_NM {
                dsu.union(i, wire_base + 2 * wi);
                touches[i] += 1;
                touches[wire_base + 2 * wi] += 1;
            }
        }
    }
    // Same-name labels join globally.
    let mut by_name: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, (_, _, n)) in nodes.iter().enumerate() {
        if let Node::Label(id) = n {
            let name = p.schematic.labels[id].name.as_str();
            if let Some(&first) = by_name.get(name) {
                dsu.union(first, i);
            } else {
                by_name.insert(name, i);
            }
        }
    }

    // Group into nets.
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..nodes.len() {
        let r = dsu.find(i);
        groups.entry(r).or_default().push(i);
    }
    let mut nl = Netlist::default();
    let mut used_names = BTreeSet::new();
    for members in groups.values() {
        let mut pins = Vec::new();
        let mut label_ids = Vec::new();
        let mut wire_ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut power = false;
        for &i in members {
            match &nodes[i].2 {
                Node::Pin(pr) => pins.push(pr.clone()),
                Node::Label(id) => {
                    let l = &p.schematic.labels[id];
                    names.insert(l.name.clone());
                    power |= l.kind == LabelKind::Power;
                    label_ids.push(id.clone());
                }
                Node::WireEnd(id) => {
                    wire_ids.insert(id.clone());
                }
            }
        }
        if pins.is_empty() && label_ids.is_empty() {
            // A floating wire group: still record dangling ends below, but not a net.
            for &i in members {
                if let Node::WireEnd(id) = &nodes[i].2 {
                    nl.dangling_wire_ends.push((id.clone(), nodes[i].1));
                }
            }
            continue;
        }
        pins.sort();
        let name = match names.iter().next() {
            Some(n) => n.clone(),
            None => {
                let first = &pins[0];
                let mut candidate = format!("N_{}_{}", first.refdes, first.pin);
                let mut k = 2;
                while used_names.contains(&candidate) {
                    candidate = format!("N_{}_{}_{k}", first.refdes, first.pin);
                    k += 1;
                }
                candidate
            }
        };
        used_names.insert(name.clone());
        for pr in &pins {
            nl.pin_net
                .insert((pr.symbol_id.clone(), pr.pin.clone()), name.clone());
        }
        for &i in members {
            if let Node::WireEnd(id) = &nodes[i].2 {
                if touches[i] == 0 {
                    nl.dangling_wire_ends.push((id.clone(), nodes[i].1));
                }
            }
        }
        label_ids.sort();
        nl.nets.insert(
            name.clone(),
            Net {
                name,
                pins,
                label_ids,
                wire_ids: wire_ids.into_iter().collect(),
                has_power_port: power,
                label_names: names.into_iter().collect(),
            },
        );
    }
    nl
}
