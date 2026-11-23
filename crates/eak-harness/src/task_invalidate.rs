//! Dependency-aware invalidation (ADR-0036).
//!
//! When the design changes under a run, only the nodes that actually depend on what changed are
//! reset; everything else keeps its recorded result. A change names the roles it touches (its
//! seeds) and the roles it must not reach (stops); the reset set is the seeds plus everything that
//! depends on them through the graph, minus anything behind a stop. A reset is written to the
//! journal as an event, never implied. Granularity is the graph node (a stage), not one component.
//!
//! Examples on a full board flow: a footprint change resets placement and what follows it
//! (routing, DRC, DFM, evaluation) but not requirements, the schematic, ERC or the netlist check;
//! a net change resets the netlist check, ERC, routing and what follows, but not placement.

use crate::task_graph::{Graph, Role};
use crate::task_journal::{Event, Journal};
use std::collections::{BTreeSet, VecDeque};

/// What changed in the design. Derived from the typed command that changed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// A component's footprint (and possibly its pin map) changed.
    Footprint {
        pin_map_changed: bool,
    },
    /// Schematic connectivity changed (wires, labels, no-connects).
    Net,
    /// Symbols were added, removed or renumbered.
    Schematic,
    Requirements,
    /// Electrical or product constraints changed.
    Constraints,
    /// PCB design rules or net classes changed.
    Rules,
    /// Board outline, stackup or layer count changed.
    BoardOutline,
}

/// Classify an `Engine` command by its type name. `None` means the command does not invalidate
/// any recorded stage (a symbol move, a property edit, a PCB write made by a node itself).
pub fn classify_command(command_type: &str) -> Option<Change> {
    match command_type {
        "AssignComponentFootprint" | "UpsertComponentDefinition" | "ImportLibraryComponent" => {
            Some(Change::Footprint {
                pin_map_changed: true,
            })
        }
        "AddWire" | "DeleteWire" | "MoveWire" | "ConnectPins" | "AddNetLabel" | "LabelPin"
        | "DeleteNetLabel" | "SetNoConnect" | "DeleteNoConnect" | "SetNetLabelScope" | "AddBus"
        | "MoveBus" | "DeleteBus" | "AddBusEntry" | "MoveBusEntry" | "DeleteBusEntry"
        | "RenameSheet" | "RenameNetLabel" => Some(Change::Net),
        "PlaceSymbol" | "DeleteSymbol" | "SetRefdes" | "Annotate" | "AddSheet" | "DeleteSheet"
        | "ReorderSheets" => Some(Change::Schematic),
        "AddRequirement" => Some(Change::Requirements),
        // A pad's own mask or paste expansion changes the fabrication outputs, like SetManufacturing.
        "SetNetCurrent" | "SetNetVoltage" | "SetManufacturing" | "SetPadExpansion" => {
            Some(Change::Constraints)
        }
        "SetDesignRules" | "UpsertNetClass" => Some(Change::Rules),
        "SetBoardOutline" | "SetStackup" | "SetLayerCount" | "SetOutlineArc" | "AddBoardCutout"
        | "DeleteBoardCutout" => Some(Change::BoardOutline),
        // Keep-out and constraint regions are design rules for placement and routing (ADR-0042).
        "AddRegion" | "DeleteRegion" => Some(Change::Rules),
        "DeleteFootprint" => Some(Change::Footprint {
            pin_map_changed: false,
        }),
        _ => None,
    }
}

fn seeds_and_stops(change: Change) -> (Vec<Role>, Vec<Role>) {
    match change {
        Change::Footprint { pin_map_changed } => {
            let mut seeds = vec![Role::Placement];
            if pin_map_changed {
                seeds.push(Role::SymbolFootprintMapping);
            }
            (seeds, vec![])
        }
        Change::Net => (
            vec![Role::NetlistCheck, Role::ErcReview, Role::Routing],
            vec![Role::Placement, Role::PcbPlanning],
        ),
        Change::Schematic => (vec![Role::NetlistCheck, Role::ErcReview], vec![]),
        Change::Requirements => (vec![Role::Requirements], vec![]),
        Change::Constraints => (vec![Role::Constraints], vec![]),
        Change::Rules => (
            vec![Role::Routing, Role::DrcReview, Role::DfmReview],
            vec![],
        ),
        Change::BoardOutline => (vec![Role::PcbPlanning], vec![]),
    }
}

/// The result of planning an invalidation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub change: Change,
    /// Every node whose inputs or assumptions the change touches, finished or not.
    pub affected: BTreeSet<String>,
    /// Affected nodes that had completed: their results are discarded and they run again.
    pub reset: BTreeSet<String>,
    /// Completed nodes that keep their results.
    pub kept: BTreeSet<String>,
}

/// Decide what a change resets. `completed` is the set of nodes with a recorded result.
pub fn plan(graph: &Graph, change: Change, completed: &BTreeSet<String>) -> Plan {
    let (seed_roles, stop_roles) = seeds_and_stops(change);
    let role_of = |id: &str| graph.nodes.iter().find(|n| n.id == id).map(|n| n.role);
    let mut affected: BTreeSet<String> = graph
        .nodes
        .iter()
        .filter(|n| seed_roles.contains(&n.role))
        .map(|n| n.id.clone())
        .collect();
    let mut queue: VecDeque<String> = affected.iter().cloned().collect();
    while let Some(n) = queue.pop_front() {
        for e in graph.edges.iter().filter(|e| e.from == n) {
            let Some(role) = role_of(&e.to) else { continue };
            if stop_roles.contains(&role) {
                continue;
            }
            if affected.insert(e.to.clone()) {
                queue.push_back(e.to.clone());
            }
        }
    }
    // A final independent judgement is stale as soon as anything it judged has changed.
    if !affected.is_empty() {
        for n in graph.nodes.iter().filter(|n| n.role == Role::Evaluator) {
            affected.insert(n.id.clone());
        }
    }
    let reset: BTreeSet<String> = affected.intersection(completed).cloned().collect();
    let kept: BTreeSet<String> = completed.difference(&affected).cloned().collect();
    Plan {
        change,
        affected,
        reset,
        kept,
    }
}

/// Write one `NodeInvalidated` event per reset node. Returns how many were recorded.
pub fn record(
    journal: &Journal,
    run: &str,
    revision: u64,
    at_ms: u64,
    plan: &Plan,
    reason: &str,
) -> Result<usize, String> {
    for node in &plan.reset {
        journal.append(
            run,
            revision,
            at_ms,
            Event::NodeInvalidated {
                node: node.clone(),
                reason: reason.to_string(),
            },
        )?;
    }
    Ok(plan.reset.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_graph::{Edge, Node, WriteScope};
    use crate::task_journal::{fold, NodeStatus};

    fn node(id: &str, role: Role) -> Node {
        Node {
            id: id.into(),
            role,
            inputs: vec![],
            outputs: vec![],
            allowed_tools: vec![],
            write_scope: WriteScope::None,
            timeout_s: 10,
            cancellable: true,
            max_tokens: None,
        }
    }

    fn edge(a: &str, b: &str) -> Edge {
        Edge {
            from: a.into(),
            from_port: "o".into(),
            to: b.into(),
            to_port: "i".into(),
        }
    }

    /// req -> sch -> net -> erc -> appr -> plan -> place -> route -> drc -> eval, with a mapping
    /// node feeding placement, DFM after routing, and ERC/DRC/DFM all feeding the evaluator.
    fn flow() -> Graph {
        Graph {
            nodes: vec![
                node("req", Role::Requirements),
                node("sch", Role::SchematicGeneration),
                node("net", Role::NetlistCheck),
                node("erc", Role::ErcReview),
                node("appr", Role::Approval),
                node("map", Role::SymbolFootprintMapping),
                node("plan", Role::PcbPlanning),
                node("place", Role::Placement),
                node("route", Role::Routing),
                node("drc", Role::DrcReview),
                node("dfm", Role::DfmReview),
                node("eval", Role::Evaluator),
            ],
            edges: vec![
                edge("req", "sch"),
                edge("sch", "net"),
                edge("net", "erc"),
                edge("erc", "appr"),
                edge("appr", "plan"),
                edge("net", "plan"),
                edge("plan", "place"),
                edge("map", "place"),
                edge("place", "route"),
                edge("route", "drc"),
                edge("route", "dfm"),
                edge("erc", "eval"),
                edge("drc", "eval"),
                edge("dfm", "eval"),
            ],
        }
    }

    fn all_done(g: &Graph) -> BTreeSet<String> {
        g.nodes.iter().map(|n| n.id.clone()).collect()
    }

    fn set(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    const FP: Change = Change::Footprint {
        pin_map_changed: false,
    };

    #[test]
    fn a_footprint_change_resets_placement_and_what_follows_only() {
        let g = flow();
        let p = plan(&g, FP, &all_done(&g));
        assert_eq!(p.affected, set(&["place", "route", "drc", "dfm", "eval"]));
        assert_eq!(p.reset, p.affected);
        for kept in ["req", "sch", "net", "erc", "appr", "map", "plan"] {
            assert!(p.kept.contains(kept), "{kept} must keep its result");
        }
    }

    #[test]
    fn a_pin_map_change_also_resets_the_mapping_node() {
        let g = flow();
        let change = Change::Footprint {
            pin_map_changed: true,
        };
        let p = plan(&g, change, &all_done(&g));
        assert!(p.affected.contains("map"));
        assert!(!p.affected.contains("sch") && !p.affected.contains("erc"));
    }

    #[test]
    fn a_net_change_resets_connectivity_and_routing_but_not_placement() {
        let g = flow();
        let p = plan(&g, Change::Net, &all_done(&g));
        assert_eq!(
            p.affected,
            set(&["net", "erc", "appr", "route", "drc", "dfm", "eval"])
        );
        assert!(p.kept.contains("place") && p.kept.contains("plan"));
        assert!(p.kept.contains("sch") && p.kept.contains("req"));
    }

    #[test]
    fn a_schematic_change_reaches_the_pcb_side_but_not_requirements_or_the_schematic_node() {
        let g = flow();
        let p = plan(&g, Change::Schematic, &all_done(&g));
        assert_eq!(
            p.affected,
            set(&["net", "erc", "appr", "plan", "place", "route", "drc", "dfm", "eval"])
        );
        assert!(p.kept.contains("req") && p.kept.contains("sch") && p.kept.contains("map"));
    }

    #[test]
    fn requirements_rules_and_outline_changes_have_their_own_reach() {
        let g = flow();
        let req = plan(&g, Change::Requirements, &all_done(&g));
        assert!(req.affected.contains("sch") && req.affected.contains("eval"));
        assert!(
            !req.affected.contains("map"),
            "mapping does not depend on requirements here"
        );

        let rules = plan(&g, Change::Rules, &all_done(&g));
        assert_eq!(rules.affected, set(&["route", "drc", "dfm", "eval"]));

        let outline = plan(&g, Change::BoardOutline, &all_done(&g));
        assert_eq!(
            outline.affected,
            set(&["plan", "place", "route", "drc", "dfm", "eval"])
        );
    }

    #[test]
    fn only_completed_nodes_are_reset_and_an_unmatched_change_does_nothing() {
        let g = flow();
        let done = set(&["req", "sch", "net", "place"]);
        let p = plan(&g, FP, &done);
        assert!(
            p.affected.contains("route"),
            "affected includes unfinished dependents"
        );
        assert_eq!(p.reset, set(&["place"]));
        assert_eq!(p.kept, set(&["req", "sch", "net"]));

        let none = plan(&g, Change::Constraints, &all_done(&g));
        assert!(none.affected.is_empty() && none.reset.is_empty());
    }

    #[test]
    fn commands_are_classified_and_non_invalidating_ones_are_ignored() {
        assert_eq!(
            classify_command("AssignComponentFootprint"),
            Some(Change::Footprint {
                pin_map_changed: true
            })
        );
        assert_eq!(classify_command("AddNetLabel"), Some(Change::Net));
        assert_eq!(classify_command("PlaceSymbol"), Some(Change::Schematic));
        assert_eq!(classify_command("SetDesignRules"), Some(Change::Rules));
        assert_eq!(
            classify_command("SetBoardOutline"),
            Some(Change::BoardOutline)
        );
        assert_eq!(classify_command("MoveSymbol"), None);
        assert_eq!(classify_command("SetComponentProperties"), None);
        assert_eq!(classify_command("AutoRoute"), None);
        assert_eq!(
            classify_command("SetPadExpansion"),
            Some(Change::Constraints)
        );
        // Saved route tool options change no design data.
        assert_eq!(classify_command("SetRoutingOptions"), None);
        assert_eq!(classify_command("AddRegion"), Some(Change::Rules));
        assert_eq!(
            classify_command("AddBoardCutout"),
            Some(Change::BoardOutline)
        );
        assert_eq!(classify_command("AddPcbText"), None);
    }

    #[test]
    fn a_reset_is_recorded_in_the_journal_and_visible_after_a_restart() {
        let dir = std::env::temp_dir().join(format!("eak-invalidate-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let j = Journal::open(&dir);
        let g = flow();
        for n in ["place", "route"] {
            let started = Event::NodeStarted { node: n.into() };
            j.append("r1", 1, 1, started).unwrap();
            let done = Event::NodeCompleted {
                node: n.into(),
                output_ref: format!("o-{n}"),
            };
            j.append("r1", 1, 2, done).unwrap();
        }
        let before = fold(&j.read().unwrap().entries, "r1");
        let p = plan(&g, FP, &before.completed());
        assert_eq!(record(&j, "r1", 2, 3, &p, "footprint changed").unwrap(), 2);
        let after = fold(&Journal::open(&dir).read().unwrap().entries, "r1");
        assert!(after.completed().is_empty());
        assert_eq!(
            after.nodes.get("route"),
            Some(&NodeStatus::Invalidated {
                reason: "footprint changed".into()
            })
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
