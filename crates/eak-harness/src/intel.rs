//! Multi-domain engineering model queries (ADR-0027).
//!
//! The requirement compiler's input adapter lives here because the structured spec is the
//! design flow's (ADR-0020): every spec field becomes a typed `Requirement` with its unit,
//! source path and knowledge state, so there is one spec schema and one requirement model.
//! All tools are read-only: they recompute the model at the current revision and never write
//! project or flow files.

use crate::design::{stored_spec, DesignSpec, RailRole, METRICS};
use eak_core::units::{Tolerance, Unit};
use eak_intelligence::systems::model::{
    ApprovalState, Category, Knowledge, Priority, Requirement, SourceKind, SourceRef, Value as V,
};
use eak_intelligence::systems::{
    compile, discipline, domains, interaction, lifecycle, BlockHint, CompileInput,
    EngineeringModel, Level, BOARD, PRODUCT,
};
use eak_runtime::Engine;
use serde_json::{json, Value};

fn s(text: &str) -> Value {
    json!({"type": "string", "description": text})
}

fn obj(p: Value, req: &[&str]) -> Value {
    json!({"type": "object", "properties": p, "required": req, "additionalProperties": false})
}

/// (name, description, input schema) of every `intel_*` tool, all in the Read category.
pub fn tool_specs() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        ("intel_domains", "Engineering domain registry, per-discipline mental models (knowledge, procedure, calculations, verification and AI guidance kept apart, each with its EAK layer) and the cross-domain interaction rules. Static data; DRAFT maturity.", obj(json!({}), &[])),
        ("intel_compile", "Compile the multi-domain engineering model of the current revision from the design and the stored spec: product/board/block profiles with domains, requirements (unknown stays unknown), the constraint graph with provenance, conflicts, calculation records, typed per-domain engineering profiles with checks, active cross-domain interactions, risks and missing information. Read-only.", obj(json!({}), &[])),
        ("intel_summary", "The engineering intelligence summary (classification, domains, architecture, requirement coverage, missing information, assumptions, constraint counts, conflicts, domain check outcomes, risks, human-approval need). Only computed values.", obj(json!({}), &[])),
        ("intel_conflicts", "Constraint conflicts (with engineering trade-offs) and attempts to weaken safety or approved limits.", obj(json!({}), &[])),
        ("intel_impact", "Impact set of changing a requirement, constraint or profile node: every downstream constraint, node, calculation, check and cross-domain review, with the edge that explains each.", obj(json!({"id": s("requirement, constraint or node id, e.g. spec.rail.VIN.voltage, C.spec.rail.3V3.current, block:rail:3V3")}), &["id"])),
        ("intel_applicable", "Constraints that apply to a profile node: its own and every inherited one (with the ancestor it comes from).", obj(json!({"node": s("profile node id, e.g. block:rail:3V3")}), &["node"])),
        ("intel_lifecycle", "The 13-stage engineering lifecycle as a phase state machine: per stage whether it was entered, its exit criteria (from requirements, constraints and the fail-closed gates) and the current stage. Without fab_profile every gate criterion is Unknown.", obj(json!({"fab_profile": {"type": "string", "enum": ["none", "fabricator_standard"], "description": "evaluate the gates with this fabrication profile"}}), &[])),
    ]
}

fn unit_of(metric: &str, unit: &str) -> Option<Unit> {
    match unit.trim() {
        "V" => Some(Unit::Volt),
        "mV" => Some(Unit::Millivolt),
        "A" => Some(Unit::Ampere),
        "mA" => Some(Unit::Milliampere),
        "W" => Some(Unit::Watt),
        "Hz" => Some(Unit::Hertz),
        "kHz" => Some(Unit::Kilohertz),
        "MHz" => Some(Unit::Megahertz),
        "°C" | "C" | "degC" => Some(Unit::DegreeCelsius),
        "" => match metric {
            "net_trace_capacity_a" | "buck_inductor_ripple_a" => Some(Unit::Ampere),
            "buck_duty_cycle" => Some(Unit::Unitless),
            _ => None,
        },
        _ => None,
    }
}

/// One requirement before it becomes a `Requirement` (keeps the adapter readable).
struct R<'a> {
    id: String,
    description: String,
    category: Category,
    domains: Vec<&'a str>,
    quantity: Option<String>,
    value: V,
    unit: Option<Unit>,
    priority: Priority,
    path: String,
    knowledge: Knowledge,
    affects: String,
    validation: Option<&'a str>,
}

impl R<'_> {
    fn build(self) -> Requirement {
        Requirement {
            id: self.id,
            description: self.description,
            category: self.category,
            domains: self.domains.iter().map(|d| d.to_string()).collect(),
            quantity: self.quantity,
            value: self.value,
            unit: self.unit,
            tolerance: Tolerance::None,
            priority: self.priority,
            source: SourceRef {
                kind: if self.knowledge == Knowledge::Assumed {
                    SourceKind::Assumption
                } else {
                    SourceKind::DesignSpec
                },
                reference: format!("design-flow/spec.json {}", self.path),
            },
            evidence: vec![],
            knowledge: self.knowledge,
            affects: vec![self.affects],
            constraints: vec![],
            validation_method: self.validation.map(String::from),
            approval: ApprovalState::Pending,
        }
    }
}

fn range(min: Option<f64>, nominal: Option<f64>, max: Option<f64>) -> V {
    V::Range { min, nominal, max }
}

fn text(t: &str) -> V {
    V::Text { text: t.into() }
}

/// The requirement compiler's input for a stored spec (`None`: IR requirements only).
pub fn spec_input(spec: Option<&DesignSpec>) -> CompileInput {
    let Some(sp) = spec else {
        return CompileInput::default();
    };
    let mut reqs = vec![];
    let mut blocks = vec![];
    let rail_block = |net: &str| format!("block:rail:{net}");
    for r in &sp.rails {
        let (v_key, i_key, role) = match r.role {
            RailRole::Input => ("input_voltage", "input_current", "input"),
            RailRole::Output => ("output_voltage", "output_current", "output"),
            RailRole::Internal => ("rail_voltage", "rail_current", "internal"),
        };
        let b = rail_block(&r.net);
        blocks.push(BlockHint {
            id: b.clone(),
            name: format!("{role} rail {}", r.net),
            level: Level::FunctionalBlock,
            parent: None,
            domain: Some("power".into()),
            reason: format!("spec rail {} ({role})", r.net),
            objects: vec![r.net.clone()],
        });
        let rail = |what: &str,
                    q: &str,
                    value: V,
                    unit: Unit,
                    priority: Priority,
                    validation: Option<&'static str>| {
            R {
                id: format!("spec.rail.{}.{what}", r.net),
                description: format!("{role} rail {} {what}", r.net),
                category: Category::Electrical,
                domains: vec!["power"],
                quantity: Some(format!("{q}:{}", r.net)),
                value,
                unit: Some(unit),
                priority,
                path: format!("rails[net={}]", r.net),
                knowledge: Knowledge::Stated,
                affects: b.clone(),
                validation,
            }
            .build()
        };
        let v = range(Some(r.v_min), Some(r.v_nom), Some(r.v_max));
        reqs.push(rail(
            "voltage",
            v_key,
            v,
            Unit::Volt,
            Priority::Must,
            Some("design_validate"),
        ));
        let i = range(None, None, Some(r.i_max_a));
        reqs.push(rail(
            "current",
            i_key,
            i,
            Unit::Ampere,
            Priority::Must,
            Some("net_trace_capacity_a"),
        ));
        if let Some(rp) = r.ripple_mv_pp {
            let v = range(None, None, Some(rp));
            reqs.push(rail(
                "ripple",
                "ripple",
                v,
                Unit::Millivolt,
                Priority::Should,
                None,
            ));
        }
    }
    if let Some(dc) = &sp.design_class {
        reqs.push(
            R {
                id: "spec.design_class".into(),
                description: format!("design class / topology {dc}"),
                category: Category::Electrical,
                domains: vec![],
                quantity: Some("topology".into()),
                value: text(dc),
                unit: None,
                priority: Priority::Should,
                path: "design_class".into(),
                knowledge: Knowledge::Stated,
                affects: PRODUCT.into(),
                validation: None,
            }
            .build(),
        );
    }
    let c = &sp.conditions;
    reqs.push(
        R {
            id: "spec.conditions.ambient".into(),
            description: "operating ambient temperature (the spec defaults ambient_max_c to 40 °C when omitted)".into(),
            category: Category::Environmental,
            domains: vec![],
            quantity: Some("ambient_temperature".into()),
            value: range(Some(c.ambient_min_c), None, Some(c.ambient_max_c)),
            unit: Some(Unit::DegreeCelsius),
            priority: Priority::Must,
            path: "conditions.ambient_*".into(),
            knowledge: Knowledge::Stated,
            affects: PRODUCT.into(),
            validation: None,
        }
        .build(),
    );
    if let Some(alt) = c.altitude_m {
        reqs.push(
            R {
                id: "spec.conditions.altitude".into(),
                description: "operating altitude".into(),
                category: Category::Environmental,
                domains: vec![],
                quantity: Some("altitude".into()),
                value: range(None, None, Some(alt)),
                unit: Some(Unit::Metre),
                priority: Priority::Should,
                path: "conditions.altitude_m".into(),
                knowledge: Knowledge::Stated,
                affects: PRODUCT.into(),
                validation: None,
            }
            .build(),
        );
    }
    if let Some(sf) = &sp.safety {
        reqs.push(
            R {
                id: "spec.safety.isolation".into(),
                description: format!("isolation class {}", sf.isolation),
                category: Category::Safety,
                domains: vec![],
                quantity: Some("isolation".into()),
                value: text(&sf.isolation),
                unit: None,
                priority: Priority::Must,
                path: "safety.isolation".into(),
                knowledge: Knowledge::Stated,
                affects: BOARD.into(),
                validation: None,
            }
            .build(),
        );
        if let Some(pd) = sf.pollution_degree {
            reqs.push(
                R {
                    id: "spec.safety.pollution_degree".into(),
                    description: "pollution degree".into(),
                    category: Category::Safety,
                    domains: vec![],
                    quantity: Some("pollution_degree".into()),
                    value: V::Number { value: pd.into() },
                    unit: Some(Unit::Unitless),
                    priority: Priority::Must,
                    path: "safety.pollution_degree".into(),
                    knowledge: Knowledge::Stated,
                    affects: BOARD.into(),
                    validation: None,
                }
                .build(),
            );
        }
    }
    for i in &sp.interfaces {
        let dom = domains::from_interface(&i.kind);
        let id = format!("block:if:{}", i.name);
        blocks.push(BlockHint {
            id: id.clone(),
            name: format!("interface {} ({})", i.name, i.kind),
            level: Level::FunctionalBlock,
            parent: None,
            domain: dom.map(String::from),
            reason: format!("spec interface {} kind {}", i.name, i.kind),
            objects: i.nets.clone(),
        });
        reqs.push(
            R {
                id: format!("spec.interface.{}", i.name),
                description: format!("interface {} of kind {}", i.name, i.kind),
                category: Category::Electrical,
                domains: dom.into_iter().collect(),
                quantity: Some(format!("interface:{}", i.kind.to_lowercase())),
                value: text(&i.kind),
                unit: None,
                priority: Priority::Should,
                path: format!("interfaces[name={}]", i.name),
                knowledge: Knowledge::Stated,
                affects: id,
                validation: None,
            }
            .build(),
        );
    }
    for (n, p) in sp.protection.iter().enumerate() {
        reqs.push(
            R {
                id: format!("spec.protection.{n}.{}", p.kind),
                description: format!("protection {} on {}", p.kind, p.nets.join(", ")),
                category: Category::Safety,
                domains: vec![],
                quantity: Some(format!("protection:{}", p.kind)),
                value: text(&p.kind),
                unit: None,
                priority: Priority::Should,
                path: format!("protection[{n}]"),
                knowledge: Knowledge::Stated,
                affects: BOARD.into(),
                validation: None,
            }
            .build(),
        );
    }
    for a in &sp.acceptance {
        let net_is_rail = a
            .net
            .as_ref()
            .is_some_and(|n| sp.rails.iter().any(|r| &r.net == n));
        let target = a
            .net
            .clone()
            .or_else(|| a.refdes.clone())
            .or_else(|| a.component_id.clone());
        let q = match (a.metric.as_str(), &target) {
            ("net_trace_capacity_a", Some(t)) => format!("trace_capacity_a:{t}"),
            (m, Some(t)) => format!("{m}:{t}"),
            (m, None) => m.to_string(),
        };
        let value = if a.min.is_none() && a.max.is_none() {
            text(&a.description)
        } else {
            range(a.min, None, a.max)
        };
        let evaluated = METRICS.iter().any(|(m, _)| *m == a.metric);
        reqs.push(
            R {
                id: format!("spec.acceptance.{}", a.id),
                description: a.description.clone(),
                category: Category::Electrical,
                domains: if net_is_rail { vec!["power"] } else { vec![] },
                quantity: Some(q),
                value,
                unit: unit_of(&a.metric, &a.unit),
                priority: Priority::Must,
                path: format!("acceptance[id={}]", a.id),
                knowledge: Knowledge::Stated,
                affects: match (&a.net, net_is_rail) {
                    (Some(n), true) => rail_block(n),
                    _ => BOARD.into(),
                },
                validation: evaluated.then_some(a.metric.as_str()),
            }
            .build(),
        );
        // Agent-supplied evaluator inputs are assumptions (ADR-0020), never stated values.
        if let Some(&hz) = a.params.get("switching_hz") {
            reqs.push(
                R {
                    id: format!("spec.acceptance.{}.switching_hz", a.id),
                    description: format!("switching frequency used by criterion {}", a.id),
                    category: Category::Electrical,
                    domains: vec!["power"],
                    quantity: Some("switching_frequency".into()),
                    value: V::Number { value: hz },
                    unit: Some(Unit::Hertz),
                    priority: Priority::Should,
                    path: format!("acceptance[id={}].params.switching_hz", a.id),
                    knowledge: Knowledge::Assumed,
                    affects: PRODUCT.into(),
                    validation: None,
                }
                .build(),
            );
        }
    }
    for (n, a) in sp.assumptions.iter().enumerate() {
        reqs.push(
            R {
                id: format!("spec.assumption.{n}"),
                description: a.clone(),
                category: Category::Unclassified,
                domains: vec![],
                quantity: None,
                value: text(a),
                unit: None,
                priority: Priority::Should,
                path: format!("assumptions[{n}]"),
                knowledge: Knowledge::Assumed,
                affects: PRODUCT.into(),
                validation: None,
            }
            .build(),
        );
    }
    CompileInput {
        requirements: reqs,
        blocks,
        spec_source: Some("design-flow/spec.json".into()),
    }
}

/// Compile the model of the engine's current revision (spec read from the project folder).
pub fn model(engine: &Engine) -> Result<EngineeringModel, String> {
    let spec = match engine.dir() {
        Some(d) => stored_spec(d)?,
        None => None,
    };
    Ok(compile(engine.project(), spec_input(spec.as_ref())))
}

fn to_json(v: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// Dispatch an `intel_*` read tool.
pub fn read(engine: &Engine, name: &str, args: &Value) -> Result<Value, String> {
    let arg = |k: &str| {
        args.get(k)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{k} is required"))
    };
    match name {
        "intel_domains" => to_json(json!({
            "domains": domains::DOMAINS,
            "discipline_models": discipline::models(),
            "process_models": "DESK process models (schematic, placement, routing, copper, manufacturing, review) are served by the design flow",
            "interaction_rules": interaction::RULES,
        })),
        "intel_compile" => to_json(model(engine)?),
        "intel_summary" => to_json(model(engine)?.summary()),
        "intel_conflicts" => {
            let m = model(engine)?;
            to_json(json!({
                "revision": m.revision,
                "conflicts": m.conflicts,
                "precedence_violations": m.precedence_violations,
            }))
        }
        "intel_impact" => to_json(model(engine)?.impact(arg("id")?)?),
        "intel_applicable" => {
            let m = model(engine)?;
            let node = arg("node")?;
            if m.product.node(node).is_none() {
                return Err(format!("{node} is not a profile node of this model"));
            }
            let list: Vec<Value> = m
                .applicable(node)
                .into_iter()
                .map(|(c, from)| json!({"constraint": c, "inherited_from": from}))
                .collect();
            to_json(json!({"revision": m.revision, "node": node, "constraints": list}))
        }
        "intel_lifecycle" => {
            let m = model(engine)?;
            let gates = match args.get("fab_profile") {
                None | Some(Value::Null) => None,
                Some(_) => Some(crate::engineering::run_gates(engine.project(), args)?),
            };
            to_json(lifecycle::evaluate(&m, engine.project(), gates.as_ref()))
        }
        other => Err(format!("no intel tool named {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Written, not run (code-reviewed only).
    #[test]
    fn spec_maps_to_typed_requirements() {
        assert!(tool_specs().iter().all(|(n, _, _)| n.starts_with("intel_")));
        let spec: DesignSpec = serde_json::from_value(json!({
            "title": "t",
            "rails": [
                {"net": "VIN", "role": "input", "v_min": 9.0, "v_nom": 12.0, "v_max": 15.0, "i_max_a": 1.0},
                {"net": "3V3", "role": "output", "v_min": 3.2, "v_nom": 3.3, "v_max": 3.4, "i_max_a": 2.0}
            ],
            "interfaces": [{"name": "radio", "kind": "LoRa"}],
            "acceptance": [{"id": "cap", "description": "trace", "metric": "net_trace_capacity_a", "net": "3V3", "min": 2.0}],
            "assumptions": ["switching at 500 kHz"]
        }))
        .unwrap();
        let input = spec_input(Some(&spec));
        assert!(input
            .requirements
            .iter()
            .any(|r| r.quantity.as_deref() == Some("output_current:3V3")));
        let cap = input
            .requirements
            .iter()
            .find(|r| r.id == "spec.acceptance.cap")
            .unwrap();
        assert_eq!(cap.quantity.as_deref(), Some("trace_capacity_a:3V3"));
        assert_eq!(cap.unit, Some(Unit::Ampere));
        assert!(input
            .blocks
            .iter()
            .any(|b| b.domain.as_deref() == Some("rf")));
        let assumed = input
            .requirements
            .iter()
            .find(|r| r.id == "spec.assumption.0")
            .unwrap();
        assert_eq!(assumed.knowledge, Knowledge::Assumed);
        let m = compile(&eak_core::Project::new("t"), input);
        assert!(
            m.conflicts.is_empty(),
            "min 2 A agrees with the derived 2 A"
        );
    }
}
