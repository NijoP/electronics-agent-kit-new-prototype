//! Specialist role templates (ADR-0037).
//!
//! `board_design()` is the standard flow from a user's intent to an independently evaluated board:
//! requirements, constraints, component research, datasheet analysis, symbol/footprint mapping,
//! schematic planning and generation, netlist check, ERC, human approval of the schematic, PCB
//! planning, placement, routing, DRC, DFM review and the evaluator. Each node lists only the tools
//! its role needs, and only the build roles hold a write scope. The template passes
//! `Graph::validate` against the real tool list (see the tests), so it cannot drift from the
//! harness. Model choice is not part of the graph: any provider can serve any node.

use crate::task_graph::{DataKind as K, Edge, Graph, InPort, Node, OutPort, Role, WriteScope};

/// Read-only design tools every role may use to look at the committed design.
const READ: &[&str] = &[
    "project_info",
    "list_symbols",
    "get_component",
    "list_footprints",
    "get_board",
    "get_connectivity",
    "get_history",
];

fn tools(extra: &[&str]) -> Vec<String> {
    READ.iter()
        .chain(extra.iter())
        .map(|t| (*t).to_string())
        .collect()
}

fn node(
    id: &str,
    role: Role,
    ins: &[(&str, K, bool)],
    outs: &[(&str, K)],
    scope: WriteScope,
    allowed: Vec<String>,
    timeout_s: u32,
) -> Node {
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
        allowed_tools: allowed,
        write_scope: scope,
        timeout_s,
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

/// Names of the available templates.
pub fn names() -> Vec<&'static str> {
    vec!["board_design"]
}

/// A template by name.
pub fn template(name: &str) -> Option<Graph> {
    match name {
        "board_design" => Some(board_design()),
        _ => None,
    }
}

/// The standard board-design flow.
pub fn board_design() -> Graph {
    use WriteScope as W;
    let nodes = vec![
        node(
            "requirements",
            Role::Requirements,
            &[("intent", K::UserIntent, true)],
            &[("requirements", K::Requirements)],
            W::None,
            vec![],
            300,
        ),
        node(
            "constraints",
            Role::Constraints,
            &[("requirements", K::Requirements, false)],
            &[("constraints", K::Constraints)],
            W::None,
            vec![],
            300,
        ),
        node(
            "research",
            Role::ComponentResearch,
            &[
                ("requirements", K::Requirements, false),
                ("constraints", K::Constraints, false),
            ],
            &[("components", K::ComponentSet)],
            W::None,
            tools(&[]),
            900,
        ),
        node(
            "datasheets",
            Role::DatasheetAnalysis,
            &[("components", K::ComponentSet, false)],
            &[("verified_components", K::ComponentSet)],
            W::None,
            tools(&[]),
            900,
        ),
        node(
            "mapping",
            Role::SymbolFootprintMapping,
            &[("components", K::ComponentSet, false)],
            &[("mapping", K::MappingReport)],
            W::Library,
            tools(&[
                "ImportLibraryComponent",
                "UpsertComponentDefinition",
                "AssignComponentFootprint",
                "SetComponentProperties",
            ]),
            900,
        ),
        node(
            "schematic_plan",
            Role::SchematicPlanning,
            &[
                ("components", K::ComponentSet, false),
                ("mapping", K::MappingReport, false),
                ("constraints", K::Constraints, false),
            ],
            &[("plan", K::SchematicPlan)],
            W::None,
            tools(&[]),
            600,
        ),
        node(
            "schematic",
            Role::SchematicGeneration,
            &[("plan", K::SchematicPlan, false)],
            &[("schematic", K::SchematicRevision)],
            W::Schematic,
            tools(&[
                "PlaceSymbol",
                "SetSymbolValue",
                "SetRefdes",
                "AddWire",
                "ConnectPins",
                "AddNetLabel",
                "LabelPin",
                "SetNoConnect",
                "Annotate",
                "AddSheet",
            ]),
            1800,
        ),
        node(
            "netlist",
            Role::NetlistCheck,
            &[("schematic", K::SchematicRevision, false)],
            &[("netlist", K::NetlistReport)],
            W::None,
            tools(&["run_check"]),
            300,
        ),
        node(
            "erc",
            Role::ErcReview,
            &[("schematic", K::SchematicRevision, false)],
            &[("erc", K::ErcReport)],
            W::None,
            tools(&["run_check"]),
            300,
        ),
        node(
            "schematic_approval",
            Role::Approval,
            &[("erc", K::ErcReport, false)],
            &[("approval", K::ApprovalDecision)],
            W::None,
            vec![],
            86_400,
        ),
        node(
            "pcb_plan",
            Role::PcbPlanning,
            &[
                ("netlist", K::NetlistReport, false),
                ("approval", K::ApprovalDecision, false),
                ("constraints", K::Constraints, false),
            ],
            &[("board_plan", K::BoardPlan)],
            W::Pcb,
            tools(&["SetBoardOutline", "SetStackup", "SetLayerCount"]),
            600,
        ),
        node(
            "placement",
            Role::Placement,
            &[
                ("board_plan", K::BoardPlan, false),
                ("netlist", K::NetlistReport, false),
            ],
            &[("placement", K::PlacementResult)],
            W::Pcb,
            tools(&[
                "SyncSchematicToPcb",
                "AutoPlace",
                "MoveFootprint",
                "SetFootprintLocked",
            ]),
            1800,
        ),
        node(
            "routing",
            Role::Routing,
            &[("placement", K::PlacementResult, false)],
            &[("routing", K::RoutingResult)],
            W::Pcb,
            tools(&[
                "AutoRoute",
                "AddTrack",
                "AddVia",
                "AddZone",
                "StitchZones",
                "DeleteTrack",
                "DeleteVia",
                "ClearRouting",
            ]),
            3600,
        ),
        node(
            "drc",
            Role::DrcReview,
            &[("routing", K::RoutingResult, false)],
            &[("drc", K::DrcReport)],
            W::None,
            tools(&["run_check"]),
            300,
        ),
        node(
            "dfm",
            Role::DfmReview,
            &[("routing", K::RoutingResult, false)],
            &[("dfm", K::DfmReport)],
            W::None,
            tools(&["run_check"]),
            300,
        ),
        node(
            "evaluator",
            Role::Evaluator,
            &[
                ("netlist", K::NetlistReport, false),
                ("erc", K::ErcReport, false),
                ("drc", K::DrcReport, false),
                ("dfm", K::DfmReport, false),
            ],
            &[("report", K::EvaluationReport)],
            W::None,
            tools(&["run_check"]),
            600,
        ),
    ];
    let edges = vec![
        edge("requirements", "requirements", "constraints", "requirements"),
        edge("requirements", "requirements", "research", "requirements"),
        edge("constraints", "constraints", "research", "constraints"),
        edge("research", "components", "datasheets", "components"),
        edge("datasheets", "verified_components", "mapping", "components"),
        edge(
            "datasheets",
            "verified_components",
            "schematic_plan",
            "components",
        ),
        edge("mapping", "mapping", "schematic_plan", "mapping"),
        edge("constraints", "constraints", "schematic_plan", "constraints"),
        edge("schematic_plan", "plan", "schematic", "plan"),
        edge("schematic", "schematic", "netlist", "schematic"),
        edge("schematic", "schematic", "erc", "schematic"),
        edge("erc", "erc", "schematic_approval", "erc"),
        edge("netlist", "netlist", "pcb_plan", "netlist"),
        edge("schematic_approval", "approval", "pcb_plan", "approval"),
        edge("constraints", "constraints", "pcb_plan", "constraints"),
        edge("pcb_plan", "board_plan", "placement", "board_plan"),
        edge("netlist", "netlist", "placement", "netlist"),
        edge("placement", "placement", "routing", "placement"),
        edge("routing", "routing", "drc", "routing"),
        edge("routing", "routing", "dfm", "routing"),
        edge("netlist", "netlist", "evaluator", "netlist"),
        edge("erc", "erc", "evaluator", "erc"),
        edge("drc", "drc", "evaluator", "drc"),
        edge("dfm", "dfm", "evaluator", "dfm"),
    ];
    Graph { nodes, edges }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn the_board_design_template_validates_against_the_real_tool_list() {
        let specs = crate::tools::tool_specs();
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        let g = board_design();
        let problems = g.validate(Some(&names));
        assert!(problems.is_empty(), "{problems:#?}");
        assert!(g.topo_order().is_ok());
    }

    #[test]
    fn only_build_roles_write_and_reviewers_and_the_evaluator_never_do() {
        let g = board_design();
        for n in &g.nodes {
            if n.role.is_reviewer() {
                assert_eq!(n.write_scope, WriteScope::None, "{}", n.id);
            }
            if n.write_scope == WriteScope::None {
                for t in &n.allowed_tools {
                    let is_command = t.chars().next().is_some_and(|c| c.is_ascii_uppercase());
                    assert!(
                        !is_command,
                        "{} lists the command {t} without a write scope",
                        n.id
                    );
                }
            }
        }
        let writers: BTreeSet<&str> = g
            .nodes
            .iter()
            .filter(|n| n.write_scope != WriteScope::None)
            .map(|n| n.id.as_str())
            .collect();
        let want: BTreeSet<&str> = ["mapping", "schematic", "pcb_plan", "placement", "routing"]
            .into_iter()
            .collect();
        assert_eq!(writers, want);
    }

    #[test]
    fn the_flow_waits_for_approval_and_ends_in_an_evaluator_over_evidence() {
        let g = board_design();
        let order = g.topo_order().unwrap();
        let pos = |id: &str| order.iter().position(|n| n == id).unwrap();
        assert!(pos("schematic_approval") < pos("pcb_plan"));
        assert!(pos("schematic") < pos("netlist"));
        assert!(pos("routing") < pos("drc"));
        assert_eq!(order.last().map(String::as_str), Some("evaluator"));
        assert!(g.descendants("placement").contains("evaluator"));
    }

    #[test]
    fn template_lookup_is_by_name() {
        assert_eq!(names(), vec!["board_design"]);
        assert!(template("board_design").is_some());
        assert!(template("nope").is_none());
    }
}
