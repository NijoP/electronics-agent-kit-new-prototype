//! Typed agent task graph (ADR-0036).
//!
//! A run is a graph of scoped nodes connected by typed ports. The graph, not a model, decides what
//! may run: a node is ready only when every input it needs has been produced, independent nodes may
//! run in parallel, and the whole graph is validated before anything executes. This module is pure
//! (no I/O, no model calls); the journal, scoping and execution live beside it.
//!
//! Design rules enforced by [`Graph::validate`]:
//! * every input is fed by exactly one edge of the same data kind, or is declared `external`;
//! * no cycles;
//! * PCB work is downstream of schematic generation and a netlist check;
//! * two nodes that write the same scope are ordered, never parallel;
//! * review roles (netlist, ERC, DRC, DFM, evaluator) cannot write;
//! * the independent evaluator judges from evidence kinds only, never agent narrative.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// What a node is for. Roles are distinct responsibilities, not distinct processes or models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Requirements,
    Constraints,
    ComponentResearch,
    DatasheetAnalysis,
    SymbolFootprintMapping,
    SchematicPlanning,
    SchematicGeneration,
    NetlistCheck,
    ErcReview,
    /// A suspended node that waits for a human decision.
    Approval,
    PcbPlanning,
    Placement,
    Routing,
    DrcReview,
    DfmReview,
    /// Independent final judge; reads design state and validation evidence only.
    Evaluator,
}

impl Role {
    /// Roles that do PCB work.
    pub fn is_pcb(self) -> bool {
        matches!(
            self,
            Role::PcbPlanning | Role::Placement | Role::Routing | Role::DrcReview
        )
    }

    /// Roles that only read and judge; they may never hold a write scope.
    pub fn is_reviewer(self) -> bool {
        matches!(
            self,
            Role::NetlistCheck
                | Role::ErcReview
                | Role::DrcReview
                | Role::DfmReview
                | Role::Evaluator
        )
    }
}

/// The type carried on a port. Edges connect only equal kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataKind {
    UserIntent,
    Requirements,
    Constraints,
    ComponentSet,
    MappingReport,
    SchematicPlan,
    /// A committed schematic at a project revision (a reference, not the data).
    SchematicRevision,
    NetlistReport,
    ErcReport,
    ApprovalDecision,
    BoardPlan,
    PlacementResult,
    RoutingResult,
    DrcReport,
    DfmReport,
    EvaluationReport,
    /// Free-text account written by an agent. Never evidence.
    Narrative,
}

impl DataKind {
    /// Kinds an independent evaluator may read: validation evidence and design references.
    pub fn is_evidence(self) -> bool {
        matches!(
            self,
            DataKind::SchematicRevision
                | DataKind::NetlistReport
                | DataKind::ErcReport
                | DataKind::DrcReport
                | DataKind::DfmReport
                | DataKind::MappingReport
                | DataKind::PlacementResult
                | DataKind::RoutingResult
                | DataKind::Constraints
                | DataKind::Requirements
        )
    }
}

/// The part of the project a node may mutate (through typed commands only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteScope {
    None,
    Library,
    Schematic,
    Pcb,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InPort {
    pub name: String,
    pub kind: DataKind,
    /// Fed by the run's own input rather than by an edge.
    #[serde(default)]
    pub external: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutPort {
    pub name: String,
    pub kind: DataKind,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub role: Role,
    pub inputs: Vec<InPort>,
    pub outputs: Vec<OutPort>,
    /// Tool names this node may call; anything else is refused at run time.
    pub allowed_tools: Vec<String>,
    pub write_scope: WriteScope,
    /// Wall-clock limit in seconds; must be positive.
    pub timeout_s: u32,
    /// Whether a run can cancel this node while it executes.
    #[serde(default = "yes")]
    pub cancellable: bool,
    /// Optional model-token budget for this node.
    #[serde(default)]
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub from_port: String,
    pub to: String,
    pub to_port: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl Graph {
    fn index(&self) -> BTreeMap<&str, &Node> {
        self.nodes.iter().map(|n| (n.id.as_str(), n)).collect()
    }

    /// Direct predecessors of every node, from edges whose ends exist.
    fn preds(&self) -> BTreeMap<&str, BTreeSet<&str>> {
        let idx = self.index();
        let mut m: BTreeMap<&str, BTreeSet<&str>> = self
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), BTreeSet::new()))
            .collect();
        for e in &self.edges {
            if idx.contains_key(e.from.as_str()) && idx.contains_key(e.to.as_str()) {
                m.entry(e.to.as_str()).or_default().insert(e.from.as_str());
            }
        }
        m
    }

    /// Node ids in an order where every node follows its predecessors (Kahn's algorithm,
    /// ties broken by id so the order is deterministic). `Err` lists the ids on or behind a cycle.
    pub fn topo_order(&self) -> Result<Vec<String>, Vec<String>> {
        let preds = self.preds();
        let mut indeg: BTreeMap<&str, usize> = preds.iter().map(|(k, v)| (*k, v.len())).collect();
        let mut succ: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (to, ps) in &preds {
            for p in ps {
                succ.entry(*p).or_default().push(*to);
            }
        }
        let mut q: VecDeque<&str> = indeg
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(k, _)| *k)
            .collect();
        let mut out: Vec<String> = Vec::new();
        while let Some(n) = q.pop_front() {
            out.push(n.to_string());
            let mut next: Vec<&str> = Vec::new();
            for s in succ.get(n).map(|v| v.as_slice()).unwrap_or(&[]) {
                if let Some(d) = indeg.get_mut(s) {
                    *d -= 1;
                    if *d == 0 {
                        next.push(*s);
                    }
                }
            }
            next.sort_unstable();
            q.extend(next);
        }
        if out.len() == self.nodes.len() {
            Ok(out)
        } else {
            let done: BTreeSet<&str> = out.iter().map(String::as_str).collect();
            Err(self
                .nodes
                .iter()
                .map(|n| n.id.as_str())
                .filter(|id| !done.contains(id))
                .map(str::to_string)
                .collect())
        }
    }

    /// Every node that depends, directly or transitively, on `id`. Empty for an unknown id.
    pub fn descendants(&self, id: &str) -> BTreeSet<String> {
        let preds = self.preds();
        let mut succ: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (to, ps) in &preds {
            for p in ps {
                succ.entry(*p).or_default().push(*to);
            }
        }
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut stack: Vec<&str> = vec![id];
        while let Some(n) = stack.pop() {
            for s in succ.get(n).map(|v| v.as_slice()).unwrap_or(&[]) {
                if seen.insert((*s).to_string()) {
                    stack.push(*s);
                }
            }
        }
        seen
    }

    /// Nodes that can start now: not done, and every predecessor is done. Independent nodes are
    /// returned together so they can run in parallel.
    pub fn ready(&self, done: &BTreeSet<String>) -> Vec<String> {
        let preds = self.preds();
        self.nodes
            .iter()
            .filter(|n| !done.contains(&n.id))
            .filter(|n| {
                preds
                    .get(n.id.as_str())
                    .is_some_and(|ps| ps.iter().all(|p| done.contains(*p)))
            })
            .map(|n| n.id.clone())
            .collect()
    }

    /// Check the whole graph before it runs. Returns every problem found, empty when valid.
    /// `known_tools`, when given, is the set of tool names a node may list.
    pub fn validate(&self, known_tools: Option<&[&str]>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut ids: BTreeSet<&str> = BTreeSet::new();
        for n in &self.nodes {
            if n.id.trim().is_empty() {
                out.push("a node has an empty id".to_string());
            } else if !ids.insert(n.id.as_str()) {
                out.push(format!("duplicate node id {}", n.id));
            }
            if n.timeout_s == 0 {
                out.push(format!("{}: timeout must be positive", n.id));
            }
            if n.role.is_reviewer() && n.write_scope != WriteScope::None {
                out.push(format!(
                    "{}: review role {:?} cannot hold a write scope",
                    n.id, n.role
                ));
            }
            let mut names: BTreeSet<&str> = BTreeSet::new();
            for p in &n.inputs {
                if !names.insert(p.name.as_str()) {
                    out.push(format!("{}: duplicate input port {}", n.id, p.name));
                }
            }
            let mut onames: BTreeSet<&str> = BTreeSet::new();
            for p in &n.outputs {
                if !onames.insert(p.name.as_str()) {
                    out.push(format!("{}: duplicate output port {}", n.id, p.name));
                }
            }
            if let Some(known) = known_tools {
                for t in &n.allowed_tools {
                    if !known.contains(&t.as_str()) {
                        out.push(format!("{}: unknown tool {t}", n.id));
                    }
                }
            }
            if n.role == Role::Evaluator {
                for p in &n.inputs {
                    if !p.kind.is_evidence() {
                        out.push(format!(
                            "{}: evaluator input {} has kind {:?}, which is not evidence",
                            n.id, p.name, p.kind
                        ));
                    }
                }
            }
        }
        let idx = self.index();
        let mut fed: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        for e in &self.edges {
            let (Some(a), Some(b)) = (idx.get(e.from.as_str()), idx.get(e.to.as_str())) else {
                out.push(format!(
                    "edge {}.{} -> {}.{} names an unknown node",
                    e.from, e.from_port, e.to, e.to_port
                ));
                continue;
            };
            let Some(o) = a.outputs.iter().find(|p| p.name == e.from_port) else {
                out.push(format!(
                    "edge from {}: no output port {}",
                    e.from, e.from_port
                ));
                continue;
            };
            let Some(i) = b.inputs.iter().find(|p| p.name == e.to_port) else {
                out.push(format!("edge to {}: no input port {}", e.to, e.to_port));
                continue;
            };
            if o.kind != i.kind {
                out.push(format!(
                    "edge {}.{} -> {}.{}: kind {:?} cannot feed {:?}",
                    e.from, e.from_port, e.to, e.to_port, o.kind, i.kind
                ));
            }
            if i.external {
                out.push(format!(
                    "{}.{} is external and must not also be wired",
                    e.to, e.to_port
                ));
            }
            *fed.entry((e.to.as_str(), e.to_port.as_str())).or_insert(0) += 1;
        }
        for n in &self.nodes {
            for p in &n.inputs {
                let c = fed
                    .get(&(n.id.as_str(), p.name.as_str()))
                    .copied()
                    .unwrap_or(0);
                if c > 1 {
                    out.push(format!(
                        "{}.{} is fed by {c} edges (exactly one allowed)",
                        n.id, p.name
                    ));
                }
                if c == 0 && !p.external {
                    out.push(format!("{}.{} has no source", n.id, p.name));
                }
            }
        }
        let order = match self.topo_order() {
            Ok(o) => o,
            Err(stuck) => {
                out.push(format!("cycle involving: {}", stuck.join(", ")));
                return out;
            }
        };
        // Ancestor sets, built in dependency order.
        let preds = self.preds();
        let mut anc: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for id in &order {
            let mut set: BTreeSet<&str> = BTreeSet::new();
            for p in preds.get(id.as_str()).into_iter().flatten() {
                set.insert(*p);
                if let Some(a) = anc.get(*p) {
                    set.extend(a.iter().copied());
                }
            }
            anc.insert(id.as_str(), set);
        }
        let has_ancestor = |id: &str, want: &dyn Fn(Role) -> bool| -> bool {
            anc.get(id)
                .is_some_and(|s| s.iter().any(|a| idx.get(a).is_some_and(|n| want(n.role))))
        };
        for n in &self.nodes {
            if n.role.is_pcb() || n.write_scope == WriteScope::Pcb {
                if !has_ancestor(&n.id, &|r| r == Role::SchematicGeneration) {
                    out.push(format!(
                        "{}: PCB work must depend on schematic generation",
                        n.id
                    ));
                }
                if !has_ancestor(&n.id, &|r| r == Role::NetlistCheck) {
                    out.push(format!("{}: PCB work must depend on a netlist check", n.id));
                }
            }
            if n.role == Role::Evaluator && !has_ancestor(&n.id, &|r| r.is_reviewer()) {
                out.push(format!(
                    "{}: evaluator must depend on at least one review node",
                    n.id
                ));
            }
        }
        for (i, a) in self.nodes.iter().enumerate() {
            for b in self.nodes.iter().skip(i + 1) {
                if a.write_scope != WriteScope::None && a.write_scope == b.write_scope {
                    let ordered = anc
                        .get(a.id.as_str())
                        .is_some_and(|s| s.contains(b.id.as_str()))
                        || anc
                            .get(b.id.as_str())
                            .is_some_and(|s| s.contains(a.id.as_str()));
                    if !ordered {
                        out.push(format!(
                            "{} and {} both write {:?} but are not ordered",
                            a.id, b.id, a.write_scope
                        ));
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Ins<'a> = &'a [(&'a str, DataKind, bool)];
    type Outs<'a> = &'a [(&'a str, DataKind)];

    fn node(id: &str, role: Role, ins: Ins, outs: Outs, scope: WriteScope) -> Node {
        Node {
            id: id.into(),
            role,
            inputs: ins
                .iter()
                .map(|(n, k, e)| InPort {
                    name: (*n).into(),
                    kind: *k,
                    external: *e,
                })
                .collect(),
            outputs: outs
                .iter()
                .map(|(n, k)| OutPort {
                    name: (*n).into(),
                    kind: *k,
                })
                .collect(),
            allowed_tools: vec!["get_design".into()],
            write_scope: scope,
            timeout_s: 60,
            cancellable: true,
            max_tokens: None,
        }
    }

    fn edge(a: &str, ap: &str, b: &str, bp: &str) -> Edge {
        Edge {
            from: a.into(),
            from_port: ap.into(),
            to: b.into(),
            to_port: bp.into(),
        }
    }

    fn valid() -> Graph {
        use DataKind as K;
        use WriteScope as W;
        Graph {
            nodes: vec![
                node(
                    "req",
                    Role::Requirements,
                    &[("intent", K::UserIntent, true)],
                    &[("requirements", K::Requirements)],
                    W::None,
                ),
                node(
                    "sch",
                    Role::SchematicGeneration,
                    &[("requirements", K::Requirements, false)],
                    &[("schematic", K::SchematicRevision)],
                    W::Schematic,
                ),
                node(
                    "net",
                    Role::NetlistCheck,
                    &[("schematic", K::SchematicRevision, false)],
                    &[("netlist", K::NetlistReport)],
                    W::None,
                ),
                node(
                    "place",
                    Role::Placement,
                    &[("netlist", K::NetlistReport, false)],
                    &[("placement", K::PlacementResult)],
                    W::Pcb,
                ),
                node(
                    "eval",
                    Role::Evaluator,
                    &[
                        ("netlist", K::NetlistReport, false),
                        ("placement", K::PlacementResult, false),
                    ],
                    &[("report", K::EvaluationReport)],
                    W::None,
                ),
            ],
            edges: vec![
                edge("req", "requirements", "sch", "requirements"),
                edge("sch", "schematic", "net", "schematic"),
                edge("net", "netlist", "place", "netlist"),
                edge("net", "netlist", "eval", "netlist"),
                edge("place", "placement", "eval", "placement"),
            ],
        }
    }

    fn has(problems: &[String], text: &str) -> bool {
        problems.iter().any(|m| m.contains(text))
    }

    #[test]
    fn a_well_formed_graph_validates_and_orders_dependencies_first() {
        let g = valid();
        assert!(g.validate(None).is_empty(), "{:?}", g.validate(None));
        assert_eq!(
            g.topo_order().unwrap(),
            vec!["req", "sch", "net", "place", "eval"]
        );
    }

    #[test]
    fn ready_nodes_follow_completed_dependencies() {
        let g = valid();
        let mut done = BTreeSet::new();
        assert_eq!(g.ready(&done), vec!["req"]);
        done.insert("req".to_string());
        assert_eq!(g.ready(&done), vec!["sch"]);
        done.extend(["sch".to_string(), "net".to_string()]);
        assert_eq!(g.ready(&done), vec!["place"]);
    }

    #[test]
    fn independent_nodes_become_ready_together() {
        use DataKind as K;
        let mut g = valid();
        g.nodes.push(node(
            "research",
            Role::ComponentResearch,
            &[("requirements", K::Requirements, false)],
            &[("components", K::ComponentSet)],
            WriteScope::None,
        ));
        g.edges
            .push(edge("req", "requirements", "research", "requirements"));
        let done: BTreeSet<String> = ["req".to_string()].into_iter().collect();
        assert_eq!(g.ready(&done), vec!["sch", "research"]);
    }

    #[test]
    fn descendants_are_the_transitive_dependents() {
        let g = valid();
        let d: Vec<String> = g.descendants("sch").into_iter().collect();
        assert_eq!(d, vec!["eval", "net", "place"]);
        assert!(g.descendants("eval").is_empty());
        assert!(g.descendants("missing").is_empty());
    }

    #[test]
    fn external_wired_missing_and_mismatched_inputs_are_reported() {
        let mut g = valid();
        g.edges.push(edge("place", "placement", "req", "intent"));
        assert!(has(
            &g.validate(None),
            "external and must not also be wired"
        ));

        let mut g = valid();
        g.edges.retain(|e| e.to != "sch");
        assert!(has(&g.validate(None), "sch.requirements has no source"));

        let mut g = valid();
        g.nodes[0].outputs[0].kind = DataKind::Narrative;
        assert!(has(&g.validate(None), "cannot feed"));
    }

    #[test]
    fn cycles_are_detected() {
        use DataKind as K;
        let cyc = Graph {
            nodes: vec![
                node(
                    "a",
                    Role::Requirements,
                    &[("x", K::Requirements, false)],
                    &[("y", K::Requirements)],
                    WriteScope::None,
                ),
                node(
                    "b",
                    Role::Constraints,
                    &[("x", K::Requirements, false)],
                    &[("y", K::Requirements)],
                    WriteScope::None,
                ),
            ],
            edges: vec![edge("a", "y", "b", "x"), edge("b", "y", "a", "x")],
        };
        assert!(has(&cyc.validate(None), "cycle involving"));
        assert!(cyc.topo_order().is_err());
    }

    #[test]
    fn pcb_work_must_follow_schematic_generation_and_a_netlist_check() {
        use DataKind as K;
        let g = Graph {
            nodes: vec![node(
                "place",
                Role::Placement,
                &[("n", K::NetlistReport, true)],
                &[("placement", K::PlacementResult)],
                WriteScope::Pcb,
            )],
            edges: vec![],
        };
        let p = g.validate(None);
        assert!(has(&p, "must depend on schematic generation"), "{p:?}");
        assert!(has(&p, "must depend on a netlist check"), "{p:?}");
    }

    #[test]
    fn parallel_writers_of_one_scope_and_writing_reviewers_are_rejected() {
        use DataKind as K;
        let mut g = valid();
        g.nodes.push(node(
            "sch2",
            Role::SchematicGeneration,
            &[("requirements", K::Requirements, false)],
            &[("schematic", K::SchematicRevision)],
            WriteScope::Schematic,
        ));
        g.edges
            .push(edge("req", "requirements", "sch2", "requirements"));
        assert!(has(
            &g.validate(None),
            "both write Schematic but are not ordered"
        ));

        let mut g = valid();
        g.nodes[2].write_scope = WriteScope::Schematic;
        assert!(has(&g.validate(None), "cannot hold a write scope"));
    }

    #[test]
    fn the_evaluator_reads_evidence_only_and_needs_a_reviewer_upstream() {
        use DataKind as K;
        let mut g = valid();
        g.nodes[4].inputs.push(InPort {
            name: "story".into(),
            kind: K::Narrative,
            external: true,
        });
        assert!(has(&g.validate(None), "not evidence"));

        let lone = Graph {
            nodes: vec![node(
                "eval",
                Role::Evaluator,
                &[("r", K::Requirements, true)],
                &[("report", K::EvaluationReport)],
                WriteScope::None,
            )],
            edges: vec![],
        };
        assert!(has(&lone.validate(None), "at least one review node"));
    }

    #[test]
    fn unknown_tools_zero_timeouts_and_duplicates_are_reported() {
        let mut g = valid();
        g.nodes[0].allowed_tools.push("delete_everything".into());
        g.nodes[1].timeout_s = 0;
        let dup = g.nodes[0].clone();
        g.nodes.push(dup);
        let p = g.validate(Some(&["get_design"]));
        assert!(has(&p, "unknown tool delete_everything"));
        assert!(has(&p, "timeout must be positive"));
        assert!(has(&p, "duplicate node id req"));
    }
}
