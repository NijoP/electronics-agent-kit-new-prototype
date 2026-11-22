//! Scoped context for one task-graph node (ADR-0037).
//!
//! A node never receives the whole project, the chat history or every earlier result. It gets its
//! task contract, exactly the inputs its ports declare (small values inline, large ones as a
//! reference with a digest), and stable references to the design parts its role may read, all
//! bound to the run's base revision. The struct has no field for anything else, so a node cannot be
//! handed more by accident. This module is pure: it assembles data and does no I/O.

use crate::task_graph::{DataKind, Graph, OutPort, Role, WriteScope};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// A part of the project a node may be pointed at. References are by part and revision, never the
/// data itself; the node reads the part through typed, revision-checked tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignPart {
    Library,
    Schematic,
    Netlist,
    Pcb,
    Manufacturing,
    Validation,
}

/// The design parts a role may read. No role gets all of them.
pub fn parts_for(role: Role) -> &'static [DesignPart] {
    use DesignPart as P;
    match role {
        Role::Requirements | Role::Constraints | Role::Approval => &[],
        Role::ComponentResearch | Role::DatasheetAnalysis => &[P::Library],
        Role::SymbolFootprintMapping => &[P::Library],
        Role::SchematicPlanning => &[P::Library, P::Schematic],
        Role::SchematicGeneration => &[P::Library, P::Schematic],
        Role::NetlistCheck | Role::ErcReview => &[P::Schematic, P::Netlist],
        Role::PcbPlanning => &[P::Netlist, P::Pcb],
        Role::Placement => &[P::Library, P::Netlist, P::Pcb],
        Role::Routing | Role::DrcReview => &[P::Netlist, P::Pcb],
        Role::DfmReview => &[P::Pcb, P::Manufacturing],
        Role::Evaluator => &[P::Schematic, P::Pcb, P::Validation],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesignRef {
    pub part: DesignPart,
    /// The run's base revision: the node reads the part as of this revision.
    pub revision: u64,
}

/// A finished node output as kept by the runner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputRecord {
    pub output_ref: String,
    /// SHA-256 of the canonical JSON of the value.
    pub digest: String,
    pub bytes: usize,
    /// Project revision when the producing node finished.
    pub revision: u64,
    pub value: Value,
}

impl OutputRecord {
    pub fn from_value(output_ref: &str, revision: u64, value: Value) -> OutputRecord {
        let raw = serde_json::to_vec(&value).unwrap_or_default();
        let digest = Sha256::digest(&raw)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        OutputRecord {
            output_ref: output_ref.to_string(),
            digest,
            bytes: raw.len(),
            revision,
            value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "via", rename_all = "snake_case")]
pub enum Source {
    /// Supplied with the run itself (for example the user's intent).
    External { value: Value },
    /// A small upstream result, passed by value.
    Inline {
        producer: String,
        output_ref: String,
        digest: String,
        revision: u64,
        value: Value,
    },
    /// A large upstream result, passed by reference; fetch it by `output_ref` and check `digest`.
    Ref {
        producer: String,
        output_ref: String,
        digest: String,
        bytes: usize,
        revision: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextInput {
    pub port: String,
    pub kind: DataKind,
    pub source: Source,
}

/// Everything one node is given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopedContext {
    pub run: String,
    pub node: String,
    pub role: Role,
    pub project_id: String,
    pub base_revision: u64,
    pub allowed_tools: Vec<String>,
    pub write_scope: WriteScope,
    pub timeout_s: u32,
    pub max_tokens: Option<u64>,
    pub inputs: Vec<ContextInput>,
    pub design: Vec<DesignRef>,
    pub expected_outputs: Vec<OutPort>,
}

impl ScopedContext {
    /// Size of the serialised context, for budget checks.
    pub fn approx_bytes(&self) -> usize {
        serde_json::to_vec(self).map(|v| v.len()).unwrap_or(0)
    }
}

/// Build the context for `node_id`. `outputs` holds finished results keyed by (node, output port);
/// `external` holds run inputs keyed `node.port`. Values up to `inline_limit` bytes are inlined.
#[allow(clippy::too_many_arguments)]
pub fn build(
    graph: &Graph,
    node_id: &str,
    run: &str,
    project_id: &str,
    base_revision: u64,
    outputs: &BTreeMap<(String, String), OutputRecord>,
    external: &BTreeMap<String, Value>,
    inline_limit: usize,
) -> Result<ScopedContext, String> {
    let node = graph
        .nodes
        .iter()
        .find(|n| n.id == node_id)
        .ok_or_else(|| format!("unknown node {node_id}"))?;
    let mut inputs = Vec::new();
    for p in &node.inputs {
        let source = if p.external {
            let key = format!("{}.{}", node.id, p.name);
            let v = external
                .get(&key)
                .ok_or_else(|| format!("external input {key} was not supplied"))?;
            Source::External { value: v.clone() }
        } else {
            let e = graph
                .edges
                .iter()
                .find(|e| e.to == node.id && e.to_port == p.name)
                .ok_or_else(|| format!("{}.{} has no source edge", node.id, p.name))?;
            let rec = outputs
                .get(&(e.from.clone(), e.from_port.clone()))
                .ok_or_else(|| {
                    format!(
                        "input {} of {} is not available: {} has not produced {}",
                        p.name, node.id, e.from, e.from_port
                    )
                })?;
            if rec.bytes <= inline_limit {
                Source::Inline {
                    producer: e.from.clone(),
                    output_ref: rec.output_ref.clone(),
                    digest: rec.digest.clone(),
                    revision: rec.revision,
                    value: rec.value.clone(),
                }
            } else {
                Source::Ref {
                    producer: e.from.clone(),
                    output_ref: rec.output_ref.clone(),
                    digest: rec.digest.clone(),
                    bytes: rec.bytes,
                    revision: rec.revision,
                }
            }
        };
        inputs.push(ContextInput {
            port: p.name.clone(),
            kind: p.kind,
            source,
        });
    }
    Ok(ScopedContext {
        run: run.to_string(),
        node: node.id.clone(),
        role: node.role,
        project_id: project_id.to_string(),
        base_revision,
        allowed_tools: node.allowed_tools.clone(),
        write_scope: node.write_scope,
        timeout_s: node.timeout_s,
        max_tokens: node.max_tokens,
        inputs,
        design: parts_for(node.role)
            .iter()
            .map(|p| DesignRef {
                part: *p,
                revision: base_revision,
            })
            .collect(),
        expected_outputs: node.outputs.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_graph::{Edge, InPort, Node};
    use serde_json::json;

    type Ins<'a> = &'a [(&'a str, DataKind, bool)];
    type Outs<'a> = &'a [(&'a str, DataKind)];

    fn node(id: &str, role: Role, ins: Ins, outs: Outs) -> Node {
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
            write_scope: WriteScope::None,
            timeout_s: 30,
            cancellable: true,
            max_tokens: Some(1000),
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

    /// req -> sch, and an unrelated research node whose result sch does not consume.
    fn graph() -> Graph {
        use DataKind as K;
        Graph {
            nodes: vec![
                node(
                    "req",
                    Role::Requirements,
                    &[("intent", K::UserIntent, true)],
                    &[("requirements", K::Requirements)],
                ),
                node(
                    "research",
                    Role::ComponentResearch,
                    &[("intent", K::UserIntent, true)],
                    &[("components", K::ComponentSet)],
                ),
                node(
                    "sch",
                    Role::SchematicGeneration,
                    &[("requirements", K::Requirements, false)],
                    &[("schematic", K::SchematicRevision)],
                ),
            ],
            edges: vec![edge("req", "requirements", "sch", "requirements")],
        }
    }

    type Store = BTreeMap<(String, String), OutputRecord>;

    fn store(req: Value) -> Store {
        let mut m = BTreeMap::new();
        m.insert(
            ("req".to_string(), "requirements".to_string()),
            OutputRecord::from_value("out-req-1", 4, req),
        );
        let other = json!({"parts": ["SECRET-UNRELATED"]});
        m.insert(
            ("research".to_string(), "components".to_string()),
            OutputRecord::from_value("out-research-1", 4, other),
        );
        m
    }

    fn none() -> BTreeMap<String, Value> {
        BTreeMap::new()
    }

    #[test]
    fn a_node_gets_only_the_inputs_its_ports_declare() {
        let g = graph();
        let s = store(json!({"v": 3.3}));
        let ctx = build(&g, "sch", "r1", "prj_1", 4, &s, &none(), 2048).unwrap();
        assert_eq!(ctx.inputs.len(), 1);
        assert_eq!(ctx.inputs[0].port, "requirements");
        let text = serde_json::to_string(&ctx).unwrap();
        assert!(
            !text.contains("SECRET-UNRELATED"),
            "another node's result leaked"
        );
        assert!(!text.contains("out-research-1"));
        assert_eq!(ctx.project_id, "prj_1");
        assert_eq!(ctx.allowed_tools, vec!["get_design"]);
        assert_eq!(ctx.expected_outputs.len(), 1);
    }

    #[test]
    fn small_values_inline_and_large_values_by_reference_with_a_digest() {
        let g = graph();
        let s = store(json!({"v": 1}));
        let small = build(&g, "sch", "r1", "p", 4, &s, &none(), 2048).unwrap();
        assert!(matches!(small.inputs[0].source, Source::Inline { .. }));

        let big = json!({"blob": "x".repeat(5000)});
        let rec = OutputRecord::from_value("out-req-1", 4, big.clone());
        let ctx = build(&g, "sch", "r1", "p", 4, &store(big), &none(), 2048).unwrap();
        match &ctx.inputs[0].source {
            Source::Ref {
                output_ref,
                digest,
                bytes,
                ..
            } => {
                assert_eq!(output_ref, "out-req-1");
                assert_eq!(digest, &rec.digest);
                assert_eq!(*bytes, rec.bytes);
            }
            other => panic!("expected a reference, got {other:?}"),
        }
        assert!(
            ctx.approx_bytes() < 1500,
            "the big value must not be serialised into the context"
        );
        assert_eq!(rec.digest.len(), 64);
    }

    #[test]
    fn an_input_whose_producer_has_not_finished_is_refused() {
        let g = graph();
        let empty: Store = BTreeMap::new();
        let e = build(&g, "sch", "r1", "p", 4, &empty, &none(), 2048).unwrap_err();
        assert!(e.contains("req has not produced requirements"), "{e}");
        assert!(build(&g, "nope", "r1", "p", 4, &empty, &none(), 10).is_err());
    }

    #[test]
    fn external_inputs_come_from_the_run_and_must_be_supplied() {
        let g = graph();
        let empty: Store = BTreeMap::new();
        let e = build(&g, "req", "r1", "p", 0, &empty, &none(), 10).unwrap_err();
        assert!(e.contains("req.intent"), "{e}");
        let mut ext = none();
        ext.insert("req.intent".to_string(), json!("ESP32 dev board"));
        let ctx = build(&g, "req", "r1", "p", 0, &empty, &ext, 10).unwrap();
        match &ctx.inputs[0].source {
            Source::External { value } => assert_eq!(value, &json!("ESP32 dev board")),
            other => panic!("expected an external input, got {other:?}"),
        }
    }

    #[test]
    fn design_references_are_role_specific_and_bound_to_the_base_revision() {
        let g = graph();
        let empty: Store = BTreeMap::new();
        let mut ext = none();
        ext.insert("req.intent".to_string(), json!("x"));
        let req = build(&g, "req", "r1", "p", 9, &empty, &ext, 10).unwrap();
        assert!(req.design.is_empty());
        let sch = build(&g, "sch", "r1", "p", 9, &store(json!(1)), &none(), 100).unwrap();
        assert_eq!(
            sch.design,
            vec![
                DesignRef {
                    part: DesignPart::Library,
                    revision: 9
                },
                DesignRef {
                    part: DesignPart::Schematic,
                    revision: 9
                },
            ]
        );
        assert!(parts_for(Role::Evaluator).contains(&DesignPart::Validation));
        assert!(!parts_for(Role::Routing).contains(&DesignPart::Schematic));
        for role in [
            Role::Placement,
            Role::Routing,
            Role::Evaluator,
            Role::DfmReview,
        ] {
            assert!(parts_for(role).len() < 6, "no role may read every part");
        }
    }
}
