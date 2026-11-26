//! Schematic connectivity: resolves wires, pins, labels and buses into canonical nets.
//!
//! Rules (ADR-0040):
//! * wire endpoints join; a pin, label, bus-entry wire point or wire endpoint lying on a wire
//!   joins that wire;
//! * labels of one name on one sheet join whatever their scope; across sheets only labels that
//!   connect globally (global, off-page, power ports) join;
//! * a bus is not a conductor: nothing joins it electrically. A label on a bus is a bus port.
//!   A bus with a non-local port exports its members: a member name that names a net on that
//!   sheet joins the global net of that name;
//! * net names come from labels (a net named only by local labels is `<sheet>/<name>`);
//!   unnamed nets get a deterministic name from their lowest pin. Every name is unique.

use eak_core::geom::point_segment_distance;
use eak_core::{bus_members, Id, LabelKind, PinType, Point, Project, ON_SEGMENT_TOL_NM};
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

/// Connectivity facts about one bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BusNet {
    pub name: String,
    /// Member net names parsed from the bus name; `None` when the name is not a legal bus name.
    pub members: Option<Vec<String>>,
    /// A non-local port on the bus exports its members to every sheet.
    pub exported: bool,
    /// Labels lying on the bus (its ports).
    pub port_label_ids: Vec<Id>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Netlist {
    pub nets: BTreeMap<String, Net>,
    /// (symbol id, pin) -> net name
    pub pin_net: BTreeMap<(Id, String), String>,
    /// Wires that connect to nothing at one or both ends.
    pub dangling_wire_ends: Vec<(Id, Point)>,
    /// Bus id -> bus facts.
    #[serde(default)]
    pub buses: BTreeMap<Id, BusNet>,
    /// Bus entry id -> the net its wire side reaches (`None`: no named or pinned net).
    #[serde(default)]
    pub bus_entries: BTreeMap<Id, Option<String>>,
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
    BusEntry(Id),
}

/// Join `i` to the first node recorded under `key`, or record `i` as that first node.
fn join_by_key<K: Ord>(dsu: &mut Dsu, firsts: &mut BTreeMap<K, usize>, key: K, i: usize) {
    match firsts.get(&key) {
        Some(&first) => dsu.union(first, i),
        None => {
            firsts.insert(key, i);
        }
    }
}

pub fn build(p: &Project) -> Netlist {
    let sch = &p.schematic;
    // Labels lying on a bus are bus ports, not electrical nodes.
    let mut bus_ports: BTreeMap<Id, Vec<Id>> = BTreeMap::new();
    let mut port_labels: BTreeSet<Id> = BTreeSet::new();
    for l in sch.labels.values() {
        for b in sch.buses.values() {
            if b.sheet_id == l.sheet_id && b.touches(l.position, ON_SEGMENT_TOL_NM) {
                bus_ports
                    .entry(b.id.clone())
                    .or_default()
                    .push(l.id.clone());
                port_labels.insert(l.id.clone());
            }
        }
    }

    let mut nodes: Vec<(Id, Point, Node)> = Vec::new(); // (sheet, position, node)
    for s in sch.symbols.values() {
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
    for l in sch.labels.values() {
        if !port_labels.contains(&l.id) {
            nodes.push((l.sheet_id.clone(), l.position, Node::Label(l.id.clone())));
        }
    }
    for e in sch.bus_entries.values() {
        nodes.push((
            e.sheet_id.clone(),
            e.wire_point,
            Node::BusEntry(e.id.clone()),
        ));
    }
    let wire_base = nodes.len();
    let wires: Vec<_> = sch.wires.values().collect();
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
            if point_segment_distance(*pos, w.a, w.b) <= ON_SEGMENT_TOL_NM {
                dsu.union(i, wire_base + 2 * wi);
                touches[i] += 1;
                touches[wire_base + 2 * wi] += 1;
            }
        }
    }
    // Same-name labels join on their sheet; labels that connect globally also join across
    // sheets.
    let mut on_sheet: BTreeMap<(Id, String), usize> = BTreeMap::new();
    let mut global: BTreeMap<String, usize> = BTreeMap::new();
    for (i, (_, _, n)) in nodes.iter().enumerate() {
        if let Node::Label(id) = n {
            let l = &sch.labels[id];
            join_by_key(
                &mut dsu,
                &mut on_sheet,
                (l.sheet_id.clone(), l.name.clone()),
                i,
            );
            if l.connects_globally() {
                join_by_key(&mut dsu, &mut global, l.name.clone(), i);
            }
        }
    }
    // Exported buses: each member that names a net on the bus's sheet joins the global member.
    let mut buses = BTreeMap::new();
    for b in sch.buses.values() {
        let ports = bus_ports.remove(&b.id).unwrap_or_default();
        let exported = ports
            .iter()
            .any(|id| sch.labels.get(id).is_some_and(|l| l.connects_globally()));
        let members = bus_members(&b.name);
        if exported {
            for m in members.iter().flatten() {
                if let Some(&i) = on_sheet.get(&(b.sheet_id.clone(), m.clone())) {
                    join_by_key(&mut dsu, &mut global, m.clone(), i);
                }
            }
        }
        buses.insert(
            b.id.clone(),
            BusNet {
                name: b.name.clone(),
                members,
                exported,
                port_label_ids: ports,
            },
        );
    }

    // Group into nets.
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..nodes.len() {
        let r = dsu.find(i);
        groups.entry(r).or_default().push(i);
    }
    let mut nl = Netlist {
        buses,
        ..Netlist::default()
    };
    let mut used_names = BTreeSet::new();
    for members in groups.values() {
        let mut pins = Vec::new();
        let mut label_ids = Vec::new();
        let mut wire_ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut power = false;
        let mut global_name = false;
        let mut local_sheet: Option<&Id> = None;
        let mut entries = Vec::new();
        for &i in members {
            match &nodes[i].2 {
                Node::Pin(pr) => pins.push(pr.clone()),
                Node::Label(id) => {
                    let l = &sch.labels[id];
                    names.insert(l.name.clone());
                    power |= l.kind == LabelKind::Power;
                    if l.connects_globally() {
                        global_name = true;
                    } else if local_sheet.is_none() {
                        local_sheet = Some(&l.sheet_id);
                    }
                    label_ids.push(id.clone());
                }
                Node::WireEnd(id) => {
                    wire_ids.insert(id.clone());
                }
                Node::BusEntry(id) => entries.push(id.clone()),
            }
        }
        if pins.is_empty() && label_ids.is_empty() {
            // A floating wire group: still record dangling ends below, but not a net.
            for &i in members {
                if let Node::WireEnd(id) = &nodes[i].2 {
                    nl.dangling_wire_ends.push((id.clone(), nodes[i].1));
                }
            }
            for id in entries {
                nl.bus_entries.insert(id, None);
            }
            continue;
        }
        // By refdes and pin, not by the derived order (symbol id first): ids are random per
        // run, so an unlabelled net would be named after a different pin each time.
        pins.sort_by(|a, b| {
            (&a.refdes, &a.pin, &a.symbol_id).cmp(&(&b.refdes, &b.pin, &b.symbol_id))
        });
        let base = match names.iter().next() {
            Some(n) if global_name => n.clone(),
            Some(n) => {
                let sheet = local_sheet
                    .and_then(|s| sch.sheets.get(s))
                    .map(|s| s.name.as_str())
                    .unwrap_or("");
                format!("{sheet}/{n}")
            }
            None => format!("N_{}_{}", pins[0].refdes, pins[0].pin),
        };
        let mut name = base.clone();
        let mut k = 2;
        while used_names.contains(&name) {
            name = format!("{base}_{k}");
            k += 1;
        }
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
        for id in entries {
            nl.bus_entries.insert(id, Some(name.clone()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::{Bus, BusEntry, NetLabel, NetScope, Rotation, Sheet, SymbolInstance, Wire};

    fn sheet(p: &mut Project, id: &str, name: &str) {
        p.schematic.sheets.insert(
            id.into(),
            Sheet {
                id: id.into(),
                name: name.into(),
                width: 297_000_000,
                height: 210_000_000,
                format: None,
                order: 0,
            },
        );
    }

    fn label(p: &mut Project, sheet: &str, id: &str, name: &str, at: Point, scope: NetScope) {
        p.schematic.labels.insert(
            id.into(),
            NetLabel {
                id: id.into(),
                sheet_id: sheet.into(),
                name: name.into(),
                position: at,
                kind: LabelKind::Net,
                scope,
                power_style: None,
            },
        );
    }

    /// A resistor on `sheet` at `x` mm whose pin 1 carries a label `name` of `scope`.
    fn labelled_r(p: &mut Project, sheet: &str, refdes: &str, x: f64, name: &str, scope: NetScope) {
        let def = eak_core::builtin::get("R_0603").unwrap();
        p.library.insert(def.id.clone(), def);
        let s = SymbolInstance {
            id: format!("sym_{refdes}"),
            sheet_id: sheet.into(),
            component_id: "R_0603".into(),
            refdes: refdes.into(),
            value: "1k".into(),
            position: Point::mm(x, 0.0),
            rotation: Rotation::R0,
            mirror: false,
            field_visibility: Default::default(),
        };
        let at = p.pin_position(&s, "1").unwrap();
        p.schematic.symbols.insert(s.id.clone(), s);
        label(p, sheet, &format!("lbl_{refdes}"), name, at, scope);
    }

    fn net_of(nl: &Netlist, refdes: &str) -> String {
        nl.pin_net[&(format!("sym_{refdes}"), "1".to_string())].clone()
    }

    fn two_sheets() -> Project {
        let mut p = Project::new("scope");
        p.schematic.sheets.clear();
        sheet(&mut p, "sht_a", "Main");
        sheet(&mut p, "sht_b", "IO");
        p
    }

    #[test]
    fn global_labels_join_across_sheets_and_local_labels_do_not() {
        let mut p = two_sheets();
        labelled_r(&mut p, "sht_a", "R1", 0.0, "SDA", NetScope::Global);
        labelled_r(&mut p, "sht_b", "R2", 0.0, "SDA", NetScope::OffPage);
        labelled_r(&mut p, "sht_a", "R3", 20.0, "EN", NetScope::Local);
        labelled_r(&mut p, "sht_b", "R4", 20.0, "EN", NetScope::Local);
        labelled_r(&mut p, "sht_a", "R5", 40.0, "EN", NetScope::Local);
        let nl = build(&p);
        assert_eq!(net_of(&nl, "R1"), "SDA");
        assert_eq!(net_of(&nl, "R2"), "SDA");
        // Local labels: one net per sheet, named after the sheet, same-sheet labels joined.
        assert_eq!(net_of(&nl, "R3"), "Main/EN");
        assert_eq!(net_of(&nl, "R5"), "Main/EN");
        assert_eq!(net_of(&nl, "R4"), "IO/EN");
        assert_eq!(nl.nets["Main/EN"].pins.len(), 2);
    }

    #[test]
    fn a_global_label_on_a_sheet_makes_its_local_namesakes_global() {
        let mut p = two_sheets();
        labelled_r(&mut p, "sht_a", "R1", 0.0, "EN", NetScope::Local);
        labelled_r(&mut p, "sht_a", "R2", 20.0, "EN", NetScope::Global);
        labelled_r(&mut p, "sht_b", "R3", 0.0, "EN", NetScope::Global);
        labelled_r(&mut p, "sht_b", "R4", 20.0, "EN", NetScope::Local);
        let nl = build(&p);
        for r in ["R1", "R2", "R3", "R4"] {
            assert_eq!(net_of(&nl, r), "EN", "{r}");
        }
    }

    #[test]
    fn net_names_never_collide() {
        let mut p = two_sheets();
        // A label spelled like the automatic name of R2's other pin.
        labelled_r(&mut p, "sht_a", "R1", 0.0, "N_R2_2", NetScope::Global);
        labelled_r(&mut p, "sht_a", "R2", 20.0, "X", NetScope::Global);
        let nl = build(&p);
        let r2_pin2 = nl.pin_net[&("sym_R2".to_string(), "2".to_string())].clone();
        assert_ne!(r2_pin2, net_of(&nl, "R1"));
        let distinct: BTreeSet<&String> = nl.pin_net.values().collect();
        assert_eq!(nl.nets.len(), distinct.len());
    }

    /// Bus D[0..1] on sheet A with two entries: entry 0's wire is labelled D0 (joined by name to
    /// R1 pin 1 on the same sheet), entry 1's wire is unnamed. R2 on sheet B is a global D0.
    fn bus_sheet(p: &mut Project, port: Option<NetScope>) {
        p.schematic.buses.insert(
            "bus_1".into(),
            Bus {
                id: "bus_1".into(),
                sheet_id: "sht_a".into(),
                name: "D[0..1]".into(),
                points: vec![Point::mm(0.0, 50.0), Point::mm(100.0, 50.0)],
            },
        );
        if let Some(scope) = port {
            label(
                p,
                "sht_a",
                "lbl_port",
                "D[0..1]",
                Point::mm(90.0, 50.0),
                scope,
            );
        }
        for (i, x) in [(0, 10.0), (1, 30.0)] {
            let wire_point = Point::mm(x + 2.54, 52.54);
            p.schematic.bus_entries.insert(
                format!("be_{i}"),
                BusEntry {
                    id: format!("be_{i}"),
                    sheet_id: "sht_a".into(),
                    bus_id: "bus_1".into(),
                    bus_point: Point::mm(x, 50.0),
                    wire_point,
                },
            );
            p.schematic.wires.insert(
                format!("wir_{i}"),
                Wire {
                    id: format!("wir_{i}"),
                    sheet_id: "sht_a".into(),
                    a: wire_point,
                    b: Point::mm(x + 2.54, 60.0),
                },
            );
        }
        label(
            p,
            "sht_a",
            "lbl_d0",
            "D0",
            Point::mm(12.54, 60.0),
            NetScope::Local,
        );
        labelled_r(p, "sht_a", "R1", 0.0, "D0", NetScope::Local);
        labelled_r(p, "sht_b", "R2", 0.0, "D0", NetScope::Global);
    }

    #[test]
    fn buses_are_not_conductors_and_report_their_entries() {
        let mut p = two_sheets();
        bus_sheet(&mut p, None);
        let nl = build(&p);
        let bus = &nl.buses["bus_1"];
        assert_eq!(bus.members, Some(vec!["D0".to_string(), "D1".to_string()]));
        assert!(!bus.exported && bus.port_label_ids.is_empty());
        assert_eq!(nl.bus_entries["be_0"], Some("Main/D0".to_string()));
        assert_eq!(nl.bus_entries["be_1"], None);
        assert_eq!(net_of(&nl, "R1"), "Main/D0");
        // Not exported: the local member stays on its sheet.
        assert_eq!(net_of(&nl, "R2"), "D0");
        // The wire end at a bus entry is attached, not dangling.
        let at = p.schematic.bus_entries["be_0"].wire_point;
        assert!(!nl
            .dangling_wire_ends
            .iter()
            .any(|(w, pt)| w == "wir_0" && *pt == at));
        // No net was made for the bus or its name.
        assert!(!nl.nets.keys().any(|n| n.contains('[')));
    }

    #[test]
    fn a_global_port_on_a_bus_exports_its_members() {
        let mut p = two_sheets();
        bus_sheet(&mut p, Some(NetScope::Global));
        let nl = build(&p);
        let bus = &nl.buses["bus_1"];
        assert!(bus.exported);
        assert_eq!(bus.port_label_ids, vec!["lbl_port".to_string()]);
        assert_eq!(net_of(&nl, "R1"), "D0");
        assert_eq!(net_of(&nl, "R2"), "D0");
        assert_eq!(nl.bus_entries["be_0"], Some("D0".to_string()));
        // The bus port label is not an electrical node of its own.
        assert!(!nl.nets.keys().any(|n| n.contains('[')));
        // A local port does not export.
        let mut q = two_sheets();
        bus_sheet(&mut q, Some(NetScope::Local));
        let nl = build(&q);
        assert!(!nl.buses["bus_1"].exported);
        assert_eq!(net_of(&nl, "R2"), "D0");
        assert_eq!(net_of(&nl, "R1"), "Main/D0");
    }
}
