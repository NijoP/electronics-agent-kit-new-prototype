//! Autonomous design flow driver (ADR-0020).
//!
//! A deterministic, evidence-keeping loop an external agent drives through the harness:
//!
//! 1. `design_spec_set`   – a structured requirement spec (rails, conditions, interfaces,
//!    protection, acceptance criteria), structurally validated and bound to a revision.
//! 2. `design_plan`       – a list of typed `Command`s (each decoded now, nothing executed).
//! 3. `design_apply`      – the plan as ONE `Engine::execute` call (a `Batch`: all or nothing,
//!    one revision, one undo step) under the session's standing approval.
//! 4. `design_validate`   – ERC/DRC/MECH/DFM plus spec checks recomputed by `eak-engineering`;
//!    structured findings, stored as revision-bound evidence.
//! 5. `design_claim`      – the agent states a claim and its basis; EAK labels it, the agent never
//!    labels itself.
//! 6. `design_report`     – the evidence report: every criterion and claim labelled Verified /
//!    Passed deterministic checks / Simulation-supported / Datasheet-supported / Assumed /
//!    Unverified, plus the iteration record and the human-approval state.
//!
//! Approval is deliberately NOT a tool: `eak design approve` only works from an interactive
//! terminal with a typed confirmation, so neither MCP nor `eak call` can approve. Nothing here
//! ever labels a design "manufacturing-ready".
//!
//! Flow records live in `<project>/design-flow/` (spec, plans, iterations, claims, reports,
//! approvals). They are evidence about the design, like `harness-calls.jsonl`, never design
//! data: the design itself changes only through `Engine::execute`.

use crate::context::CallContext;
use crate::tools::{command_error, run_checks, ErrorCode, Fail, CALL_LOG};
use eak_core::Project;
use eak_engineering::{calc, ipc, netlist};
use eak_intelligence::domain::{classify_text, Status};
use eak_intelligence::mental;
use eak_intelligence::tools::{command_tools, decode_command};
use eak_runtime::command::MAX_BATCH;
use eak_runtime::store;
use eak_runtime::{Actor, Approval, Command, CommandRequest, Engine, OperationClass};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Folder (inside the project) holding the flow records.
pub const FLOW_DIR: &str = "design-flow";
const SPEC_FILE: &str = "spec.json";
const PLANS_DIR: &str = "plans";
const REPORTS_DIR: &str = "reports";
const ITERATIONS: &str = "iterations.jsonl";
const CLAIMS: &str = "claims.jsonl";
const APPROVALS: &str = "approvals.jsonl";

// ---------------------------------------------------------------- spec

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RailRole {
    Input,
    Output,
    Internal,
}

/// One supply rail, named by its schematic net.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rail {
    pub net: String,
    pub role: RailRole,
    pub v_min: f64,
    pub v_nom: f64,
    pub v_max: f64,
    /// Worst-case continuous current (A); drives trace sizing.
    pub i_max_a: f64,
    #[serde(default)]
    pub ripple_mv_pp: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    #[serde(default)]
    pub ambient_min_c: f64,
    #[serde(default = "default_ambient_max")]
    pub ambient_max_c: f64,
    #[serde(default)]
    pub altitude_m: Option<f64>,
    #[serde(default)]
    pub coated: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

fn default_ambient_max() -> f64 {
    40.0
}

impl Default for Conditions {
    fn default() -> Self {
        Conditions {
            ambient_min_c: 0.0,
            ambient_max_c: default_ambient_max(),
            altitude_m: None,
            coated: false,
            notes: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interface {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub nets: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Protection {
    /// e.g. reverse_polarity, overcurrent, tvs, esd, fuse.
    pub kind: String,
    #[serde(default)]
    pub nets: Vec<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// One acceptance criterion. `metric` selects the deterministic evaluator (see `METRICS`);
/// an unknown metric is kept but can only ever be labelled Unverified.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub id: String,
    pub description: String,
    pub metric: String,
    #[serde(default)]
    pub net: Option<String>,
    #[serde(default)]
    pub refdes: Option<String>,
    #[serde(default)]
    pub component_id: Option<String>,
    #[serde(default)]
    pub rating: Option<String>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub unit: String,
    /// Extra numeric inputs (e.g. switching_hz, inductance_h). Agent-supplied inputs make a
    /// result Assumed rather than Verified.
    #[serde(default)]
    pub params: BTreeMap<String, f64>,
}

/// Electrical-safety inputs. Required (a blocking gate) above the 60 V DC SELV limit or for a
/// mains / high-voltage design class; EAK never guesses them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Safety {
    /// none | functional | basic | reinforced
    pub isolation: String,
    #[serde(default)]
    pub pollution_degree: Option<u8>,
    #[serde(default)]
    pub overvoltage_category: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

/// The structured requirement spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignSpec {
    pub title: String,
    /// Design class, e.g. buck_converter (see eak-intelligence domain classes).
    #[serde(default)]
    pub design_class: Option<String>,
    #[serde(default)]
    pub rails: Vec<Rail>,
    #[serde(default)]
    pub conditions: Conditions,
    #[serde(default)]
    pub safety: Option<Safety>,
    #[serde(default)]
    pub interfaces: Vec<Interface>,
    #[serde(default)]
    pub protection: Vec<Protection>,
    #[serde(default)]
    pub acceptance: Vec<Criterion>,
    #[serde(default)]
    pub assumptions: Vec<String>,
}

/// Metrics with a deterministic evaluator.
pub const METRICS: &[(&str, &str)] = &[
    ("erc_errors", "ERC error count at this revision (use max: 0)"),
    ("drc_errors", "DRC error count at this revision (use max: 0)"),
    ("mech_errors", "MECH error count at this revision (use max: 0)"),
    ("dfm_errors", "DFM error count at this revision (use max: 0)"),
    ("net_trace_capacity_a", "IPC-2221 current capacity (A) of the narrowest routed track on `net` (eak-engineering ipc); use min"),
    ("buck_duty_cycle", "ideal duty cycle Vout/Vin at the input rail's v_min (worst case) from the first input and output rails"),
    ("buck_inductor_ripple_a", "ideal inductor ripple (A p-p) at the input rail's v_max; L from the `refdes` symbol value (Verified) or params.inductance_h (Assumed); needs params.switching_hz"),
    ("component_rating", "a rating (`rating`) of `component_id` compared with min/max; Datasheet-supported when the rating is marked verified"),
    ("simulation", "Simulation-supported when a successful run_simulation was logged at this revision; the value itself is not checked by EAK"),
];

/// Structural problems with a spec. `errors` refuse storing; `warnings` are kept.
pub fn check_spec(s: &DesignSpec) -> (Vec<String>, Vec<String>) {
    let (mut errors, mut warnings) = (vec![], vec![]);
    if s.title.trim().is_empty() {
        errors.push("title is empty".into());
    }
    if s.rails.is_empty() {
        errors.push("at least one rail is required".into());
    }
    let mut nets = std::collections::BTreeSet::new();
    for r in &s.rails {
        let finite = [r.v_min, r.v_nom, r.v_max, r.i_max_a]
            .iter()
            .all(|v| v.is_finite());
        if r.net.trim().is_empty() {
            errors.push("a rail has an empty net name".into());
        } else if !nets.insert(r.net.as_str()) {
            errors.push(format!("rail {} is listed twice", r.net));
        }
        if !finite || !(r.v_min <= r.v_nom && r.v_nom <= r.v_max) {
            errors.push(format!("rail {}: need v_min <= v_nom <= v_max", r.net));
        }
        if !(r.i_max_a > 0.0) {
            errors.push(format!(
                "rail {}: i_max_a must be > 0 (unknown current is never sized)",
                r.net
            ));
        }
    }
    if s.conditions.ambient_min_c > s.conditions.ambient_max_c {
        errors.push("conditions: ambient_min_c > ambient_max_c".into());
    }
    if let Some(sf) = &s.safety {
        if !["none", "functional", "basic", "reinforced"].contains(&sf.isolation.as_str()) {
            errors.push("safety.isolation must be none, functional, basic or reinforced".into());
        }
        if sf.pollution_degree.is_some_and(|d| !(1..=4).contains(&d)) {
            errors.push("safety.pollution_degree must be 1..4".into());
        }
    }
    if s.acceptance.is_empty() {
        errors.push("at least one acceptance criterion is required".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for c in &s.acceptance {
        if c.id.trim().is_empty() || !ids.insert(c.id.as_str()) {
            errors.push(format!("criterion id {:?} is empty or repeated", c.id));
        }
        if !METRICS.iter().any(|(m, _)| *m == c.metric) {
            warnings.push(format!(
                "criterion {}: metric {:?} has no deterministic evaluator; it will be Unverified",
                c.id, c.metric
            ));
        }
        if c.min.is_none() && c.max.is_none() && c.metric != "simulation" {
            warnings.push(format!("criterion {}: no min or max bound", c.id));
        }
    }
    (errors, warnings)
}

/// SELV limit used by the high-voltage gate (V DC, IEC 62368-1 ES1 order of magnitude).
pub const SELV_DC_V: f64 = 60.0;

/// The text a spec is classified from (eak-intelligence domain classifier, DESK-derived).
fn spec_text(s: &DesignSpec) -> String {
    let mut t = vec![s.title.clone()];
    t.extend(s.design_class.iter().map(|c| c.replace('_', " ")));
    for i in &s.interfaces {
        t.push(format!("{} {}", i.name, i.kind.replace('_', " ")));
    }
    for p in &s.protection {
        t.push(p.kind.replace('_', " "));
    }
    for r in &s.rails {
        t.push(format!("{} {} V {} A", r.net, r.v_max, r.i_max_a));
    }
    t.extend(s.conditions.notes.iter().cloned());
    t.extend(s.assumptions.iter().cloned());
    t.join(". ")
}

/// Classify the spec and derive the blocking gates: missing critical electrical information
/// that makes automatic design changes unsafe. `design_apply` refuses while any gate is open;
/// the cure is more spec information (from the human), never an assumption.
pub fn classify_and_gate(s: &DesignSpec) -> (Value, Vec<Value>) {
    let c = classify_text(&spec_text(s));
    let domain = c.candidates.first().map(|k| k.domain.clone());
    let mains = domain.as_deref() == Some("ac_mains_hv")
        || c.overlays.iter().any(|o| o == "mains_safety_isolation");
    let mut gates = vec![];
    let v_peak = s.rails.iter().map(|r| r.v_max.abs()).fold(0.0, f64::max);
    if (v_peak > SELV_DC_V || mains) && s.safety.is_none() {
        gates.push(json!({
            "id": "hv-isolation-undefined",
            "message": format!("peak rail {v_peak} V{} needs `safety` (isolation, pollution degree): clearances and creepage are not chosen automatically", if mains { " / mains-class design" } else { "" }),
        }));
    }
    if c.status != Status::Classified && s.design_class.is_none() {
        gates.push(json!({
            "id": "domain-unresolved",
            "message": "the design class is unresolved (status review-required/unclassified): set `design_class` after reviewing the candidates",
            "candidates": c.candidates.iter().take(3).map(|k| json!({"archetype_id": k.archetype_id, "domain": k.domain, "score": k.score})).collect::<Vec<_>>(),
        }));
    }
    let summary = json!({
        "status": c.status,
        "primary": c.primary,
        "domain": domain,
        "confidence": c.confidence,
        "overlays": c.overlays,
        "required_component_families": c.required_component_families,
        "missing_information": c.missing_information,
        "conflicts": c.conflicts,
        "guidance": c.guidance,
        "source": "eak-intelligence domain classifier (DESK-derived registry)",
    });
    (summary, gates)
}

/// JSON schema of the spec (for `design_spec_schema`).
pub fn spec_schema() -> Value {
    let num = json!({"type": "number"});
    let strs = json!({"type": "array", "items": {"type": "string"}});
    json!({
        "schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["title", "rails", "acceptance"],
            "properties": {
                "title": {"type": "string"},
                "design_class": {"type": "string", "description": "e.g. buck_converter"},
                "rails": {"type": "array", "items": {
                    "type": "object", "additionalProperties": false,
                    "required": ["net", "role", "v_min", "v_nom", "v_max", "i_max_a"],
                    "properties": {
                        "net": {"type": "string", "description": "schematic net name"},
                        "role": {"type": "string", "enum": ["input", "output", "internal"]},
                        "v_min": num, "v_nom": num, "v_max": num,
                        "i_max_a": {"type": "number", "exclusiveMinimum": 0},
                        "ripple_mv_pp": num,
                    }}},
                "conditions": {"type": "object", "additionalProperties": false, "properties": {
                    "ambient_min_c": num, "ambient_max_c": num, "altitude_m": num,
                    "coated": {"type": "boolean"}, "notes": {"type": "string"}}},
                "interfaces": {"type": "array", "items": {"type": "object", "additionalProperties": false,
                    "required": ["name", "kind"],
                    "properties": {"name": {"type": "string"}, "kind": {"type": "string"}, "nets": strs, "notes": {"type": "string"}}}},
                "protection": {"type": "array", "items": {"type": "object", "additionalProperties": false,
                    "required": ["kind"],
                    "properties": {"kind": {"type": "string"}, "nets": strs, "notes": {"type": "string"}}}},
                "acceptance": {"type": "array", "items": {"type": "object", "additionalProperties": false,
                    "required": ["id", "description", "metric"],
                    "properties": {
                        "id": {"type": "string"}, "description": {"type": "string"},
                        "metric": {"type": "string", "enum": METRICS.iter().map(|(m, _)| *m).collect::<Vec<_>>()},
                        "net": {"type": "string"}, "refdes": {"type": "string"},
                        "component_id": {"type": "string"}, "rating": {"type": "string"},
                        "min": num, "max": num, "unit": {"type": "string"},
                        "params": {"type": "object", "additionalProperties": {"type": "number"}},
                    }}},
                "safety": {"type": "object", "additionalProperties": false, "required": ["isolation"], "properties": {
                    "isolation": {"type": "string", "enum": ["none", "functional", "basic", "reinforced"]},
                    "pollution_degree": {"type": "integer", "minimum": 1, "maximum": 4},
                    "overvoltage_category": {"type": "string"}, "notes": {"type": "string"}}},
                "assumptions": strs,
            }
        },
        "metrics": METRICS.iter().map(|(m, d)| json!({"metric": m, "evaluates": d})).collect::<Vec<_>>(),
        "labels": LABELS,
        "gates": [
            {"id": "hv-isolation-undefined", "when": format!("any rail above {SELV_DC_V} V or a mains-class design without `safety`")},
            {"id": "domain-unresolved", "when": "classifier not confident and no `design_class`"},
        ],
    })
}

/// Agent-readable procedures and models (DESK-derived, `eak_intelligence::mental`).
pub fn procedure() -> Value {
    json!({
        "schematic_procedure": mental::schematic_procedure(),
        "models": mental::models(),
        "derating": mental::DERATING,
        "note": "facts, constraints, procedures and guidance are kept separate; a constraint with evaluated_by = null is not checked by EAK and is never reported as passing",
    })
}

// ---------------------------------------------------------------- labels

/// Evidence label of a criterion or claim. Assigned by EAK, never by the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Label {
    /// Recomputed by eak-engineering from the design data and the spec, and within bounds.
    Verified,
    /// Implemented deterministic checks (ERC/DRC/MECH/DFM) report no errors; not a proof.
    PassedDeterministicChecks,
    /// Backed by an ngspice run logged at this revision; models and stimuli are the agent's.
    SimulationSupported,
    /// Backed by a datasheet value recorded as verified in the component library.
    DatasheetSupported,
    /// Rests on agent-supplied inputs or stated assumptions.
    Assumed,
    /// Failed, unevaluable, or without evidence.
    Unverified,
}

const LABELS: &[(&str, &str)] = &[
    (
        "verified",
        "recomputed by eak-engineering from the design data and the spec, within bounds",
    ),
    (
        "passed_deterministic_checks",
        "implemented ERC/DRC/MECH/DFM rules report no errors (not a proof of correctness)",
    ),
    (
        "simulation_supported",
        "a successful ngspice run is logged at this revision (models/stimuli chosen by the agent)",
    ),
    (
        "datasheet_supported",
        "a library rating marked verified with a datasheet source",
    ),
    (
        "assumed",
        "rests on agent-supplied inputs or stated assumptions",
    ),
    ("unverified", "failed, not evaluable, or no evidence"),
];

// ---------------------------------------------------------------- results

#[derive(Debug, Clone, Serialize)]
pub struct CriterionResult {
    pub id: String,
    pub metric: String,
    pub description: String,
    /// `None` when EAK cannot evaluate the value itself.
    pub passed: Option<bool>,
    pub measured: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub unit: String,
    pub label: Label,
    pub basis: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub id: String,
    pub source: String,
    pub severity: String,
    pub message: String,
    pub measured: Option<f64>,
    pub required: Option<f64>,
    pub suggested_fix: Option<String>,
}

fn finding(id: String, severity: &str, message: String) -> Finding {
    Finding {
        id,
        source: "spec".into(),
        severity: severity.into(),
        message,
        measured: None,
        required: None,
        suggested_fix: None,
    }
}

fn within(v: f64, min: Option<f64>, max: Option<f64>) -> bool {
    min.is_none_or(|m| v >= m) && max.is_none_or(|m| v <= m)
}

/// Narrowest routed track on `net` and its IPC-2221 capacity (A); `None` when unrouted.
fn net_capacity(p: &Project, net: &str) -> Option<(f64, f64)> {
    p.pcb
        .tracks
        .values()
        .filter(|t| t.net == net)
        .map(|t| {
            let oz = p
                .pcb
                .board
                .stackup
                .layers
                .iter()
                .find(|l| l.layer == t.layer)
                .map(|l| l.copper_oz)
                .unwrap_or(1.0);
            let width_mm = t.width as f64 / 1e6;
            let amps = ipc::trace_current_a(
                width_mm * ipc::MIL_PER_MM,
                !t.layer.is_outer(),
                oz,
                p.constraints.temp_rise_c,
            );
            (width_mm, amps)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

fn symbol_value(p: &Project, refdes: &str) -> Option<f64> {
    p.schematic
        .symbols
        .values()
        .find(|s| s.refdes == refdes)
        .and_then(|s| calc::parse_value(&s.value))
}

fn rating(p: &Project, component_id: &str, name: &str) -> Option<eak_core::Rating> {
    component_def(p, component_id)?
        .ratings
        .into_iter()
        .find(|r| r.name == name)
}

/// Check totals (errors per check, lower-case name) from a `run_checks` result.
fn check_totals(checks: &Value) -> BTreeMap<String, u64> {
    checks["reports"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|r| {
                    (
                        r["check"].as_str().unwrap_or("").to_lowercase(),
                        r["errors"].as_u64().unwrap_or(0),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Evaluate the spec against the design at its current revision.
pub fn evaluate(
    p: &Project,
    spec: &DesignSpec,
    check_errors: &BTreeMap<String, u64>,
    simulated_here: bool,
) -> (Vec<CriterionResult>, Vec<Finding>) {
    let nl = netlist::build(p);
    let mut findings = vec![];
    for r in &spec.rails {
        if !nl.nets.contains_key(&r.net) {
            let mut f = finding(
                format!("rail-net-missing:{}", r.net),
                "error",
                format!("rail {} has no schematic net of that name", r.net),
            );
            f.suggested_fix = Some("name the net with AddNetLabel / LabelPin".into());
            findings.push(f);
            continue;
        }
        let declared_i = p.constraints.net_currents_a.get(&r.net).copied();
        if declared_i.is_none_or(|i| i < r.i_max_a) {
            let mut f = finding(
                format!("rail-current-undeclared:{}", r.net),
                "warning",
                format!(
                    "net {} declares {:?} A but the spec needs {} A; DRC cannot size its tracks",
                    r.net, declared_i, r.i_max_a
                ),
            );
            f.measured = declared_i;
            f.required = Some(r.i_max_a);
            f.suggested_fix = Some(format!(
                "SetNetCurrent {{net: {:?}, amps: {}}}",
                r.net, r.i_max_a
            ));
            findings.push(f);
        }
        let declared_v = p.constraints.net_voltages_v.get(&r.net).copied();
        if declared_v.is_none_or(|v| v < r.v_max) {
            let mut f = finding(
                format!("rail-voltage-undeclared:{}", r.net),
                "warning",
                format!(
                    "net {} declares {:?} V but the spec peak is {} V; clearance checks use the declared value",
                    r.net, declared_v, r.v_max
                ),
            );
            f.measured = declared_v;
            f.required = Some(r.v_max);
            f.suggested_fix = Some(format!(
                "SetNetVoltage {{net: {:?}, volts: {}}}",
                r.net, r.v_max
            ));
            findings.push(f);
        }
        match net_capacity(p, &r.net) {
            None => findings.push(finding(
                format!("rail-unrouted:{}", r.net),
                "info",
                format!("net {} has no routed tracks yet", r.net),
            )),
            Some((w, amps)) if amps < r.i_max_a => {
                let mut f = finding(
                    format!("rail-trace-capacity:{}", r.net),
                    "error",
                    format!(
                        "narrowest track on {} ({w:.3} mm) carries {amps:.2} A (IPC-2221) < {} A",
                        r.net, r.i_max_a
                    ),
                );
                f.source = "eak-engineering".into();
                f.measured = Some(amps);
                f.required = Some(r.i_max_a);
                f.suggested_fix = Some(
                    "widen the tracks (DeleteTrack + AddTrack) or add copper (AddZone)".into(),
                );
                findings.push(f);
            }
            Some(_) => {}
        }
    }

    let input = spec.rails.iter().find(|r| r.role == RailRole::Input);
    let output = spec.rails.iter().find(|r| r.role == RailRole::Output);
    let criteria = spec
        .acceptance
        .iter()
        .map(|c| {
            let res = |passed: Option<bool>, measured: Option<f64>, label: Label, basis: String| {
                CriterionResult {
                    id: c.id.clone(),
                    metric: c.metric.clone(),
                    description: c.description.clone(),
                    passed,
                    measured,
                    min: c.min,
                    max: c.max,
                    unit: c.unit.clone(),
                    label: if passed == Some(false) {
                        Label::Unverified
                    } else {
                        label
                    },
                    basis,
                }
            };
            let bounded = |v: f64, label: Label, basis: String| {
                res(Some(within(v, c.min, c.max)), Some(v), label, basis)
            };
            let unverified = |why: &str| res(None, None, Label::Unverified, why.to_string());
            match c.metric.as_str() {
                m @ ("erc_errors" | "drc_errors" | "mech_errors" | "dfm_errors") => {
                    let check = m.trim_end_matches("_errors");
                    match check_errors.get(check) {
                        Some(n) => bounded(
                            *n as f64,
                            Label::PassedDeterministicChecks,
                            format!("{check} from eak-validation at this revision; implemented rules only"),
                        ),
                        None => unverified("check did not run"),
                    }
                }
                "net_trace_capacity_a" => match c.net.as_deref().map(|n| net_capacity(p, n)) {
                    Some(Some((w, amps))) => bounded(
                        amps,
                        Label::Verified,
                        format!("IPC-2221 formula, narrowest track {w:.3} mm, temp rise {} C", p.constraints.temp_rise_c),
                    ),
                    Some(None) => unverified("net has no routed tracks"),
                    None => unverified("criterion needs `net`"),
                },
                "buck_duty_cycle" => match (input, output) {
                    (Some(i), Some(o)) => match calc::buck_duty_cycle(i.v_min, o.v_nom) {
                        Ok(d) => bounded(d, Label::Verified, format!("eak-engineering calc: {} V / {} V (ideal, losses ignored)", o.v_nom, i.v_min)),
                        Err(e) => unverified(&e.to_string()),
                    },
                    _ => unverified("needs an input and an output rail"),
                },
                "buck_inductor_ripple_a" => {
                    let (l, from_ir) = match c.refdes.as_deref().and_then(|r| symbol_value(p, r)) {
                        Some(l) => (Some(l), true),
                        None => (c.params.get("inductance_h").copied(), false),
                    };
                    match (input, output, l, c.params.get("switching_hz")) {
                        (Some(i), Some(o), Some(l), Some(f)) => {
                            match calc::buck_inductor_ripple_a(i.v_max, o.v_nom, l, *f) {
                                Ok(a) => bounded(
                                    a,
                                    if from_ir { Label::Verified } else { Label::Assumed },
                                    format!("eak-engineering calc at Vin={} V, L={l} H ({}), fsw={f} Hz (agent-supplied)",
                                        i.v_max, if from_ir { "from the schematic value" } else { "agent-supplied" }),
                                ),
                                Err(e) => unverified(&e.to_string()),
                            }
                        }
                        _ => unverified("needs input/output rails, an inductance (refdes or params.inductance_h) and params.switching_hz"),
                    }
                }
                "component_rating" => match (c.component_id.as_deref(), c.rating.as_deref()) {
                    (Some(id), Some(name)) => match rating(p, id, name) {
                        Some(r) => bounded(
                            r.value,
                            if r.verified { Label::DatasheetSupported } else { Label::Assumed },
                            format!("{id} rating {name} = {} {} (source: {})", r.value, r.unit, r.source),
                        ),
                        None => unverified("rating not found in the component definition"),
                    },
                    _ => unverified("criterion needs component_id and rating"),
                },
                "simulation" => {
                    if simulated_here {
                        res(None, None, Label::SimulationSupported, "a successful run_simulation is logged at this revision; EAK did not check the value".into())
                    } else {
                        unverified("no successful run_simulation at this revision")
                    }
                }
                _ => unverified("no deterministic evaluator for this metric"),
            }
        })
        .collect();
    (criteria, findings)
}

// ---------------------------------------------------------------- storage

fn flow_root(engine: &Engine) -> Result<PathBuf, Fail> {
    engine.dir().map(|d| d.join(FLOW_DIR)).ok_or_else(|| {
        Fail(
            ErrorCode::Precondition,
            "the design flow needs a saved project folder".into(),
            Value::Null,
        )
    })
}

pub(crate) fn storage(e: impl std::fmt::Display) -> Fail {
    Fail(ErrorCode::Storage, e.to_string(), Value::Null)
}

pub(crate) fn write_json(path: &Path, v: &Value) -> Result<(), Fail> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(storage)?;
    }
    let text = serde_json::to_string_pretty(v).map_err(storage)?;
    store::atomic_write(path, text.as_bytes()).map_err(storage)
}

pub(crate) fn append_jsonl(path: &Path, v: &Value) -> Result<(), Fail> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(storage)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(storage)?;
    writeln!(f, "{v}").map_err(storage)
}

pub(crate) fn read_jsonl(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// SHA-256 of the compact JSON text (an integrity fingerprint, not a signature).
pub fn fingerprint(v: &Value) -> String {
    format!("{:x}", Sha256::digest(v.to_string().as_bytes()))
}

fn record(
    root: &Path,
    ctx: &CallContext,
    revision: u64,
    event: &str,
    data: Value,
) -> Result<(), Fail> {
    let mut line = json!({
        "ts_ms": store::now_ms() as u64,
        "revision": revision,
        "session": ctx.session_id,
        "agent": ctx.agent,
        "event": event,
    });
    if let (Some(m), Value::Object(d)) = (line.as_object_mut(), data) {
        m.extend(d);
    }
    append_jsonl(&root.join(ITERATIONS), &line)
}

/// The stored spec of a project folder, `None` when none is set. Read-only access for the
/// multi-domain engineering model (ADR-0027, `crate::intel`); a corrupt spec is an error,
/// never silently ignored.
pub fn stored_spec(project_dir: &Path) -> Result<Option<DesignSpec>, String> {
    let root = project_dir.join(FLOW_DIR);
    if !root.join(SPEC_FILE).exists() {
        return Ok(None);
    }
    load_spec(&root)
        .map(|(s, _)| Some(s))
        .map_err(|Fail(_, m, _)| format!("stored design spec unreadable: {m}"))
}

fn load_spec(root: &Path) -> Result<(DesignSpec, Value), Fail> {
    let text = std::fs::read_to_string(root.join(SPEC_FILE)).map_err(|_| {
        Fail(
            ErrorCode::Precondition,
            "no design spec yet: call design_spec_set first".into(),
            Value::Null,
        )
    })?;
    let stored: Value = serde_json::from_str(&text).map_err(storage)?;
    let spec = serde_json::from_value(stored["spec"].clone()).map_err(storage)?;
    Ok((spec, stored))
}

fn simulated_at(project_dir: &Path, revision: u64) -> bool {
    read_jsonl(&project_dir.join(CALL_LOG)).iter().any(|c| {
        c["tool"] == json!("run_simulation")
            && c["ok"] == json!(true)
            && c["revision"].as_u64() == Some(revision)
    })
}

pub(crate) fn invalid(msg: impl Into<String>) -> Fail {
    Fail(ErrorCode::InvalidArguments, msg.into(), Value::Null)
}

pub(crate) fn arg_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, Fail> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| invalid(format!("{key} is required")))
}

// ---------------------------------------------------------------- schematic intent, components

const INTENT_FILE: &str = "intent.json";

fn component_def(p: &Project, id: &str) -> Option<eak_core::ComponentDef> {
    p.library.get(id).cloned().or_else(|| {
        eak_core::builtin::components()
            .into_iter()
            .find(|d| d.id == id)
    })
}

/// `Some(false)` when the symbol exists but its component has no such pin.
fn symbol_pin_ok(p: &Project, symbol_id: &str, pin: &str) -> Option<bool> {
    let s = p.schematic.symbols.get(symbol_id)?;
    Some(component_def(p, &s.component_id).is_some_and(|d| d.pin(pin).is_some()))
}

/// `"U1.2"` -> `("U1", "2")`.
fn split_pin(s: &str) -> Option<(&str, &str)> {
    s.rsplit_once('.')
        .filter(|(r, n)| !r.is_empty() && !n.is_empty())
}

/// Parse a plan's `intent.connections` (`[{net, pins: ["U1.2", ...]}]`) into pin -> net.
/// Pins of parts that already exist are checked against their component definition.
fn parse_intent(
    p: &Project,
    v: Option<&Value>,
    problems: &mut Vec<Value>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return out;
    };
    let Some(conns) = v.get("connections").and_then(Value::as_array) else {
        problems.push(json!({"intent": "intent.connections must be an array of {net, pins}"}));
        return out;
    };
    for c in conns {
        let net = c
            .get("net")
            .and_then(Value::as_str)
            .filter(|n| !n.trim().is_empty());
        let (Some(net), Some(pins)) = (net, c.get("pins").and_then(Value::as_array)) else {
            problems.push(json!({"intent": "each connection needs a net name and a pins array"}));
            continue;
        };
        for pin in pins {
            let Some((r, n)) = pin.as_str().and_then(split_pin) else {
                problems.push(json!({"intent": format!("pin {pin} must be \"REFDES.PIN\"")}));
                continue;
            };
            if let Some(s) = p.schematic.symbols.values().find(|s| s.refdes == r) {
                if !component_def(p, &s.component_id).is_some_and(|d| d.pin(n).is_some()) {
                    problems.push(json!({"intent": format!("pin-not-on-component: {r} ({}) has no pin {n:?}", s.component_id)}));
                }
            }
            let key = format!("{r}.{n}");
            if let Some(prev) = out.insert(key.clone(), net.to_string()) {
                if prev != net {
                    problems.push(
                        json!({"intent": format!("{key} is intended on both {prev} and {net}")}),
                    );
                }
            }
        }
    }
    out
}

fn load_intent(root: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(root.join(INTENT_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Merge a plan's intent into the design intent; returns pins whose intended net changed.
fn merge_intent(root: &Path, new: &BTreeMap<String, String>) -> Result<Vec<Value>, Fail> {
    if new.is_empty() {
        return Ok(vec![]);
    }
    let mut all = load_intent(root);
    let mut replaced = vec![];
    for (pin, net) in new {
        if let Some(was) = all.insert(pin.clone(), net.clone()) {
            if &was != net {
                replaced.push(json!({"pin": pin, "was": was, "now": net}));
            }
        }
    }
    write_json(&root.join(INTENT_FILE), &json!(all))?;
    Ok(replaced)
}

/// Steps S11-S12: rebuild the netlist from EAK-IR and compare every intended pin with it.
fn intent_readback(
    p: &Project,
    nl: &netlist::Netlist,
    intent: &BTreeMap<String, String>,
) -> Vec<Finding> {
    let mut out = vec![];
    for (key, net) in intent {
        let Some((r, n)) = split_pin(key) else {
            continue;
        };
        let Some(s) = p.schematic.symbols.values().find(|s| s.refdes == r) else {
            let mut f = finding(
                format!("intent-part-missing:{r}"),
                "error",
                format!("intent names {key} on {net} but no symbol {r} exists"),
            );
            f.source = "intent".into();
            out.push(f);
            continue;
        };
        if !component_def(p, &s.component_id).is_some_and(|d| d.pin(n).is_some()) {
            let mut f = finding(
                format!("pin-not-on-component:{key}"),
                "error",
                format!("{r} ({}) has no pin {n}", s.component_id),
            );
            f.source = "intent".into();
            out.push(f);
            continue;
        }
        let actual = nl.pin_net.get(&(s.id.clone(), n.to_string()));
        if actual.map(String::as_str) != Some(net.as_str()) {
            let mut f = finding(
                format!("intent-mismatch:{key}"),
                "error",
                format!(
                    "{key} is on {} in the read-back netlist; intended {net}",
                    actual.map(String::as_str).unwrap_or("no net")
                ),
            );
            f.source = "intent".into();
            f.suggested_fix = Some(format!(
                "LabelPin {{symbol_id: {:?}, pin: {n:?}, name: {net:?}}}",
                s.id
            ));
            out.push(f);
        }
    }
    out
}

/// Component intelligence (DESK component-selection model): MPN/datasheet evidence and
/// derated voltage/current ratings against the spec rails on the part's nets.
fn component_evidence(
    p: &Project,
    nl: &netlist::Netlist,
    spec: &DesignSpec,
) -> (Vec<Value>, Vec<Finding>) {
    let rails: BTreeMap<&str, &Rail> = spec.rails.iter().map(|r| (r.net.as_str(), r)).collect();
    let (mut parts, mut findings) = (vec![], vec![]);
    for s in p.schematic.symbols.values() {
        let Some(def) = component_def(p, &s.component_id) else {
            continue;
        };
        let mut on_rails: Vec<&Rail> = nl
            .pin_net
            .iter()
            .filter(|((sym, _), _)| *sym == s.id)
            .filter_map(|(_, net)| rails.get(net.as_str()).copied())
            .collect();
        on_rails.sort_by(|a, b| a.net.cmp(&b.net));
        on_rails.dedup_by(|a, b| a.net == b.net);
        let v_applied = on_rails.iter().map(|r| r.v_max.abs()).fold(0.0, f64::max);
        let i_applied = on_rails.iter().map(|r| r.i_max_a).fold(0.0, f64::max);
        let mut checks = vec![];
        for r in &def.ratings {
            let (quantity, applied) = if mental::VOLTAGE_RATINGS.contains(&r.name.as_str()) {
                ("voltage", v_applied)
            } else if mental::CURRENT_RATINGS.contains(&r.name.as_str()) {
                ("current", i_applied)
            } else {
                continue;
            };
            let Some(d) = mental::derating(&s.refdes, quantity) else {
                continue;
            };
            if applied <= 0.0 || r.value <= 0.0 {
                continue;
            }
            let limit = r.value * d.max_fraction;
            let passed = applied <= limit;
            let label = if !passed {
                Label::Unverified
            } else if r.verified {
                Label::DatasheetSupported
            } else {
                Label::Assumed
            };
            checks.push(json!({
                "rating": r.name, "value": r.value, "unit": r.unit, "rating_source": r.source,
                "rating_verified": r.verified, "applied": applied, "max_fraction": d.max_fraction,
                "derated_limit": limit, "passed": passed, "label": label, "derating_source": d.source,
            }));
            if !passed {
                let mut f = finding(
                    format!("derating:{}:{}", s.refdes, r.name),
                    "error",
                    format!("{} {} {} {} derated to {limit:.3} (x{}) < applied {applied} from the spec rails", s.refdes, r.name, r.value, r.unit, d.max_fraction),
                );
                f.source = "component-intelligence".into();
                f.measured = Some(applied);
                f.required = Some(limit);
                f.suggested_fix = Some("choose a part with a higher rating (UpsertComponentDefinition / AssignComponentFootprint) or lower the rail in the spec".into());
                findings.push(f);
            }
        }
        let blank = |o: &Option<String>| o.as_deref().is_none_or(|x| x.trim().is_empty());
        if blank(&def.mpn) {
            let mut f = finding(
                format!("component-unresolved-mpn:{}", s.refdes),
                "warning",
                format!("{} ({}) has no manufacturer part number", s.refdes, def.id),
            );
            f.source = "component-intelligence".into();
            f.suggested_fix = Some(format!(
                "SetComponentProperties {{component_id: {:?}, mpn: ...}}",
                def.id
            ));
            findings.push(f);
        }
        if blank(&def.datasheet) {
            let mut f = finding(
                format!("component-no-datasheet:{}", s.refdes),
                "warning",
                format!(
                    "{} ({}) has no datasheet reference; its ratings cannot be datasheet-supported",
                    s.refdes, def.id
                ),
            );
            f.source = "component-intelligence".into();
            findings.push(f);
        }
        parts.push(json!({
            "refdes": s.refdes, "component_id": def.id, "value": s.value,
            "mpn": def.mpn, "manufacturer": def.manufacturer, "datasheet": def.datasheet,
            "provenance": def.provenance, "rails": on_rails.iter().map(|r| r.net.clone()).collect::<Vec<_>>(),
            "rating_checks": checks,
        }));
    }
    (parts, findings)
}

// ---------------------------------------------------------------- tools

/// Dispatch one `flow` tool against an open engine.
pub(crate) fn dispatch(
    engine: &mut Engine,
    ctx: &CallContext,
    tool: &str,
    args: &Value,
) -> Result<Value, Fail> {
    let root = flow_root(engine)?;
    match tool {
        "design_spec_set" => spec_set(engine, ctx, &root, args),
        "design_spec_get" => load_spec(&root).map(|(_, stored)| stored),
        "design_plan" => plan(engine, ctx, &root, args),
        "design_apply" => apply(engine, ctx, &root, args),
        "design_validate" => validate(engine, ctx, &root),
        "design_claim" => claim(engine, ctx, &root, args),
        "design_iteration_log" => Ok(iteration_log(&root)),
        "design_report" => report(engine, &root),
        other => Err(Fail(
            ErrorCode::UnknownTool,
            format!("no design flow tool named {other}"),
            Value::Null,
        )),
    }
}

fn spec_set(engine: &Engine, ctx: &CallContext, root: &Path, args: &Value) -> Result<Value, Fail> {
    let raw = args
        .get("spec")
        .cloned()
        .ok_or_else(|| invalid("spec is required"))?;
    let spec: DesignSpec = serde_json::from_value(raw)
        .map_err(|e| invalid(format!("spec: {e} (see design_spec_schema)")))?;
    let (errors, warnings) = check_spec(&spec);
    if !errors.is_empty() {
        return Err(Fail(
            ErrorCode::InvalidArguments,
            "the spec has structural errors; nothing was stored".into(),
            json!({"errors": errors, "warnings": warnings}),
        ));
    }
    let spec_v = serde_json::to_value(&spec).map_err(storage)?;
    let revision = engine.project().revision;
    let (classification, gates) = classify_and_gate(&spec);
    let stored = json!({
        "revision": revision,
        "fingerprint": fingerprint(&spec_v),
        "set_by": {"session": ctx.session_id, "agent": ctx.agent},
        "spec": spec_v,
        "warnings": warnings,
        "classification": classification,
        "gates": gates,
        "gates_note": "design_apply is refused while any gate is open; supply the missing information in the spec",
    });
    write_json(&root.join(SPEC_FILE), &stored)?;
    record(
        root,
        ctx,
        revision,
        "spec",
        json!({"fingerprint": stored["fingerprint"], "warnings": warnings.len()}),
    )?;
    Ok(stored)
}

pub(crate) fn decode_step(step: &Value) -> Result<(Command, Option<OperationClass>), String> {
    let mut obj = step
        .as_object()
        .cloned()
        .ok_or("each step must be a JSON object")?;
    let ty = obj
        .remove("type")
        .and_then(|t| t.as_str().map(String::from))
        .ok_or("each step needs a \"type\" naming the command")?;
    if ty == "Batch" {
        return Err("Batch cannot be a plan step: the whole plan is applied as one batch".into());
    }
    let cmd = decode_command(&ty, &Value::Object(obj))?;
    let class = command_tools()
        .into_iter()
        .find(|t| t.spec.name == ty)
        .map(|t| t.class);
    Ok((cmd, class))
}

fn plan(engine: &Engine, ctx: &CallContext, root: &Path, args: &Value) -> Result<Value, Fail> {
    let title = arg_str(args, "title")?;
    let steps = args
        .get("steps")
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid("steps must be a non-empty array of typed commands"))?;
    if steps.len() > MAX_BATCH {
        return Err(invalid(format!("at most {MAX_BATCH} steps per plan")));
    }
    let mut problems = vec![];
    let mut max_class = OperationClass::ReadOnly;
    let p = engine.project();
    for (i, s) in steps.iter().enumerate() {
        match decode_step(s) {
            Ok((_, class)) => {
                max_class = max_class.max(class.unwrap_or(OperationClass::EngineeringImpacting))
            }
            Err(e) => problems.push(json!({"step": i, "error": e})),
        }
        // Never invent a pin map: a pin on an existing symbol must exist on its component.
        for (sym, pin) in [
            ("symbol_id", "pin"),
            ("a_symbol", "a_pin"),
            ("b_symbol", "b_pin"),
        ] {
            if let (Some(sym), Some(pin)) = (s[sym].as_str(), s[pin].as_str()) {
                if symbol_pin_ok(p, sym, pin) == Some(false) {
                    problems.push(json!({"step": i, "error": format!("pin-not-on-component: symbol {sym} has no pin {pin:?} (read get_component)")}));
                }
            }
        }
    }
    let intent = parse_intent(p, args.get("intent"), &mut problems);
    if !problems.is_empty() {
        return Err(Fail(
            ErrorCode::InvalidArguments,
            "some steps are not valid typed commands; nothing was stored".into(),
            json!({"steps": problems}),
        ));
    }
    let n = std::fs::read_dir(root.join(PLANS_DIR))
        .map(|rd| rd.count())
        .unwrap_or(0);
    let plan_id = format!("plan-{:04}", n + 1);
    let revision = engine.project().revision;
    let needs_opt_in = max_class > ctx.max_class;
    let plan = json!({
        "plan_id": plan_id,
        "title": title,
        "rationale": args.get("rationale").and_then(Value::as_str).unwrap_or(""),
        "addresses": args.get("addresses").cloned().unwrap_or(json!([])),
        "base_revision": revision,
        "max_class": max_class,
        "needs_destructive_opt_in": needs_opt_in,
        "session": ctx.session_id,
        "agent": ctx.agent,
        "steps": steps,
        "intent": intent.iter().map(|(pin, net)| json!({"pin": pin, "net": net})).collect::<Vec<_>>(),
    });
    write_json(&root.join(PLANS_DIR).join(format!("{plan_id}.json")), &plan)?;
    let replaced = merge_intent(root, &intent)?;
    record(
        root,
        ctx,
        revision,
        "plan",
        json!({"plan_id": plan_id, "steps": steps.len(), "title": title}),
    )?;
    Ok(json!({
        "plan_id": plan_id,
        "base_revision": revision,
        "steps": steps.len(),
        "max_class": max_class,
        "intent_pins": intent.len(),
        "intent_replaced": replaced,
        "needs_destructive_opt_in": needs_opt_in,
        "next": format!("design_apply {{plan_id: {plan_id:?}, expected_revision: {revision}}}"),
    }))
}

fn apply(engine: &mut Engine, ctx: &CallContext, root: &Path, args: &Value) -> Result<Value, Fail> {
    let plan_id = arg_str(args, "plan_id")?;
    if !plan_id.starts_with("plan-") || plan_id.contains(['/', '\\', '.']) {
        return Err(invalid("plan_id must be an id returned by design_plan"));
    }
    let expected = args
        .get("expected_revision")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            Fail(
                ErrorCode::MissingExpectedRevision,
                "expected_revision is required".into(),
                Value::Null,
            )
        })?;
    let text = std::fs::read_to_string(root.join(PLANS_DIR).join(format!("{plan_id}.json")))
        .map_err(|_| {
            Fail(
                ErrorCode::NotFound,
                format!("no plan {plan_id}"),
                Value::Null,
            )
        })?;
    let plan: Value = serde_json::from_str(&text).map_err(storage)?;
    let base = plan["base_revision"].as_u64().unwrap_or(u64::MAX);
    if base != expected {
        return Err(Fail(
            ErrorCode::StaleRevision,
            format!(
                "plan {plan_id} was made at revision {base}; re-plan against the current revision"
            ),
            json!({"plan_base_revision": base, "expected_revision": expected}),
        ));
    }
    // Missing critical electrical information blocks automatic changes (domain gates).
    let (_, stored) = load_spec(root)?;
    if stored["gates"].as_array().is_some_and(|g| !g.is_empty()) {
        return Err(Fail(
            ErrorCode::Precondition,
            "the spec has open gates (missing critical information); design_apply is blocked until the spec supplies it".into(),
            json!({"gates": stored["gates"]}),
        ));
    }
    let mut commands = vec![];
    for s in plan["steps"].as_array().cloned().unwrap_or_default() {
        commands.push(decode_step(&s).map_err(invalid)?.0);
    }
    let command = if commands.len() == 1 {
        commands.remove(0)
    } else {
        Command::Batch {
            commands,
            label: Some(format!(
                "{plan_id}: {}",
                plan["title"].as_str().unwrap_or("")
            )),
        }
    };
    let resp = engine.execute(CommandRequest {
        expected_revision: expected,
        actor: Actor::agent_in_session(&ctx.agent, &ctx.session_id),
        command,
        approval: Some(Approval {
            approved_by: format!("harness-session:{}", ctx.session_id),
            max_class: ctx.max_class,
            note: "standing approval of the user who started this agent session through the EAK harness".into(),
        }),
    });
    record(
        root,
        ctx,
        engine.project().revision,
        "apply",
        json!({
            "plan_id": plan_id, "op_id": resp.op_id, "ok": resp.error.is_none(),
            "error": resp.error.as_ref().map(|e| e.to_string()),
            "revision_before": resp.revision_before, "revision_after": resp.revision_after,
        }),
    )?;
    if let Some(e) = &resp.error {
        return Err(command_error(e));
    }
    let mut v = serde_json::to_value(&resp).map_err(storage)?;
    v["plan_id"] = json!(plan_id);
    v["next"] = json!("design_validate");
    Ok(v)
}

type Assessment = (Value, Vec<CriterionResult>, Vec<Finding>, Value, Vec<Value>);

/// Run every check and evaluate the spec; shared by validate and report.
fn assess(engine: &mut Engine, root: &Path) -> Result<Assessment, Fail> {
    let (spec, stored) = load_spec(root)?;
    let checks = run_checks(engine, "all")?;
    let p = engine.project();
    let simulated = engine.dir().is_some_and(|d| simulated_at(d, p.revision));
    let (criteria, mut findings) = evaluate(p, &spec, &check_totals(&checks), simulated);
    // Schematic mental model S11-S12 and component intelligence on the read-back netlist.
    let nl = netlist::build(p);
    findings.extend(intent_readback(p, &nl, &load_intent(root)));
    let (components, component_findings) = component_evidence(p, &nl, &spec);
    findings.extend(component_findings);
    Ok((checks, criteria, findings, stored, components))
}

fn error_count(checks: &Value, criteria: &[CriterionResult], findings: &[Finding]) -> u64 {
    checks["errors"].as_u64().unwrap_or(0)
        + criteria.iter().filter(|c| c.passed == Some(false)).count() as u64
        + findings.iter().filter(|f| f.severity == "error").count() as u64
}

fn validate(engine: &mut Engine, ctx: &CallContext, root: &Path) -> Result<Value, Fail> {
    let (checks, criteria, findings, stored, components) = assess(engine, root)?;
    let revision = engine.project().revision;
    let errors = error_count(&checks, &criteria, &findings);
    let mut out = json!({
        "revision": revision,
        "spec_fingerprint": stored["fingerprint"],
        "errors": errors,
        "criteria": criteria,
        "findings": findings,
        "components": components,
        "reports": checks["reports"],
        "note": "deterministic results only; a clean result does not prove the design correct (see each report's not_implemented and limitations)",
    });
    let evidence = engine.save_evidence(
        "design-validate",
        &serde_json::to_string_pretty(&out).map_err(storage)?,
    );
    record(
        root,
        ctx,
        revision,
        "validate",
        json!({
            "errors": errors,
            "failed_criteria": criteria.iter().filter(|c| c.passed == Some(false)).map(|c| c.id.clone()).collect::<Vec<_>>(),
            "findings": findings.iter().map(|f| f.id.clone()).collect::<Vec<_>>(),
        }),
    )?;
    out["evidence_path"] = json!(evidence);
    Ok(out)
}

const CLAIM_BASES: &[&str] = &[
    "assumption",
    "datasheet",
    "simulation",
    "calculation",
    "check",
];

fn claim(engine: &Engine, ctx: &CallContext, root: &Path, args: &Value) -> Result<Value, Fail> {
    let statement = arg_str(args, "statement")?;
    let basis = arg_str(args, "basis")?;
    if !CLAIM_BASES.contains(&basis) {
        return Err(invalid(format!("basis must be one of {CLAIM_BASES:?}")));
    }
    let revision = engine.project().revision;
    let c = json!({
        "statement": statement,
        "basis": basis,
        "component_id": args.get("component_id"),
        "rating": args.get("rating"),
        "criterion_id": args.get("criterion_id"),
        "made_at_revision": revision,
        "session": ctx.session_id,
        "agent": ctx.agent,
    });
    append_jsonl(&root.join(CLAIMS), &c)?;
    record(root, ctx, revision, "claim", json!({"basis": basis}))?;
    Ok(json!({
        "recorded": c,
        "note": "EAK assigns the label in design_report; the agent's own confidence is not a label",
    }))
}

/// Label one agent claim from evidence EAK can see.
fn label_claim(
    p: &Project,
    c: &Value,
    criteria: &[CriterionResult],
    simulated: bool,
) -> (Label, String) {
    let s = |k: &str| c[k].as_str();
    match s("basis").unwrap_or("") {
        "assumption" => (Label::Assumed, "stated as an assumption".into()),
        "simulation" if simulated => (
            Label::SimulationSupported,
            "successful run_simulation logged at this revision".into(),
        ),
        "simulation" => (
            Label::Unverified,
            "no successful run_simulation at this revision".into(),
        ),
        "datasheet" => match (s("component_id"), s("rating")) {
            (Some(id), Some(name)) => match rating(p, id, name) {
                Some(r) if r.verified => (
                    Label::DatasheetSupported,
                    format!("{id} {name} = {} {} ({})", r.value, r.unit, r.source),
                ),
                Some(r) => (
                    Label::Assumed,
                    format!(
                        "{id} {name} is recorded but not verified (source: {})",
                        r.source
                    ),
                ),
                None => (Label::Unverified, format!("{id} has no rating {name}")),
            },
            _ => (
                Label::Unverified,
                "datasheet claims need component_id and rating".into(),
            ),
        },
        "calculation" | "check" => {
            match s("criterion_id").and_then(|id| criteria.iter().find(|r| r.id == id)) {
                Some(r) if r.passed != Some(false) => (r.label, format!("via criterion {}", r.id)),
                Some(r) => (Label::Unverified, format!("criterion {} failed", r.id)),
                None => (
                    Label::Unverified,
                    "needs a criterion_id from the spec".into(),
                ),
            }
        }
        _ => (Label::Unverified, "unknown basis".into()),
    }
}

fn iteration_log(root: &Path) -> Value {
    let mut plans: Vec<Value> = std::fs::read_dir(root.join(PLANS_DIR))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                .filter_map(|t| serde_json::from_str::<Value>(&t).ok())
                .map(|p| {
                    json!({"plan_id": p["plan_id"], "title": p["title"],
                           "base_revision": p["base_revision"],
                           "steps": p["steps"].as_array().map(Vec::len)})
                })
                .collect()
        })
        .unwrap_or_default();
    plans.sort_by(|a, b| a["plan_id"].as_str().cmp(&b["plan_id"].as_str()));
    json!({
        "iterations": read_jsonl(&root.join(ITERATIONS)),
        "plans": plans,
        "claims": read_jsonl(&root.join(CLAIMS)),
        "approvals": read_jsonl(&root.join(APPROVALS)),
    })
}

fn report_path(root: &Path, revision: u64) -> PathBuf {
    root.join(REPORTS_DIR)
        .join(format!("report-r{revision:06}.json"))
}

fn label_name(l: Label) -> String {
    serde_json::to_value(l)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn report(engine: &mut Engine, root: &Path) -> Result<Value, Fail> {
    let (checks, criteria, findings, stored, components) = assess(engine, root)?;
    let p = engine.project();
    let revision = p.revision;
    let simulated = engine.dir().is_some_and(|d| simulated_at(d, revision));
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for c in &criteria {
        *counts.entry(label_name(c.label)).or_default() += 1;
    }
    let claims: Vec<Value> = read_jsonl(&root.join(CLAIMS))
        .into_iter()
        .map(|c| {
            let (label, basis) = label_claim(p, &c, &criteria, simulated);
            *counts.entry(label_name(label)).or_default() += 1;
            json!({"statement": c["statement"], "basis": c["basis"],
                   "made_at_revision": c["made_at_revision"],
                   "label": label, "label_basis": basis})
        })
        .collect();
    let errors = error_count(&checks, &criteria, &findings);
    // The fingerprinted body is what a human approves: any new claim, changed result or new
    // revision changes it and so invalidates an earlier approval.
    let body = json!({
        "project_id": p.id,
        "revision": revision,
        "spec_fingerprint": stored["fingerprint"],
        "spec": stored["spec"],
        "errors": errors,
        "label_counts": counts,
        "criteria": criteria,
        "findings": findings,
        "components": components,
        "claims": claims,
        "checks": checks["reports"].as_array().map(|a| a.iter().map(|r| json!({
            "check": r["check"], "errors": r["errors"], "warnings": r["warnings"],
            "not_implemented": r["not_implemented"], "limitations": r["limitations"],
        })).collect::<Vec<_>>()),
    });
    let fp = fingerprint(&body);
    let approval = read_jsonl(&root.join(APPROVALS))
        .into_iter()
        .rev()
        .find(|a| {
            a["revision"].as_u64() == Some(revision)
                && a["fingerprint"].as_str() == Some(fp.as_str())
        });
    let iterations = read_jsonl(&root.join(ITERATIONS));
    let release_label = match (&approval, errors) {
        (Some(_), 0) => format!(
            "human-approved release candidate (revision {revision}); not a manufacturing-ready claim"
        ),
        (_, 0) => "draft: awaiting human approval (eak design approve)".to_string(),
        _ => "draft: deterministic errors remain".to_string(),
    };
    let approve_with = if errors == 0 && approval.is_none() {
        json!(format!(
            "eak design approve {revision} {fp}   (a human, in an interactive terminal)"
        ))
    } else {
        Value::Null
    };
    let count = |ev: &str| {
        iterations
            .iter()
            .filter(|i| i["event"] == json!(ev))
            .count()
    };
    let out = json!({
        "fingerprint": fp,
        "release_label": release_label,
        "approval": approval,
        "approve_with": approve_with,
        "labels": LABELS,
        "body": body,
        "iteration_summary": {
            "events": iterations.len(),
            "plans": count("plan"),
            "applies": count("apply"),
            "validations": count("validate"),
            "claims": count("claim"),
        },
        "statement": "EAK never labels a design manufacturing-ready. Labels describe the evidence EAK holds, not engineering sign-off.",
    });
    write_json(&report_path(root, revision), &out)?;
    Ok(out)
}

// ---------------------------------------------------------------- human approval (CLI only)

/// `eak design approve <REV> <FINGERPRINT>`: a human approves one report. Refused unless stdin
/// is an interactive terminal and the typed confirmation matches; agents running commands
/// through pipes cannot pass this gate. Returns the process exit code.
pub fn approve_cli(ctx: &CallContext, revision: u64, fp: &str) -> i32 {
    use std::io::{BufRead, IsTerminal};
    let Some(dir) = ctx.project.dir.as_ref() else {
        eprintln!("eak design approve: no project (pass --project)");
        return 2;
    };
    if !std::io::stdin().is_terminal() {
        eprintln!("eak design approve: needs an interactive terminal; approval is a human step and is not available to agents, MCP or `eak call`");
        return 1;
    }
    let root = dir.join(FLOW_DIR);
    let Some(report) = std::fs::read_to_string(report_path(&root, revision))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        eprintln!(
            "eak design approve: no report for revision {revision}; run `eak design report` first"
        );
        return 1;
    };
    if report["fingerprint"].as_str() != Some(fp) {
        eprintln!("eak design approve: fingerprint does not match the stored report for revision {revision}; re-run the report and review it");
        return 1;
    }
    if report.pointer("/body/errors").and_then(Value::as_u64) != Some(0) {
        eprintln!(
            "eak design approve: the report still has deterministic errors; it cannot be approved"
        );
        return 1;
    }
    println!(
        "Report for revision {revision}: {}\nLabel counts: {}\nThis records YOUR approval of this evidence report. It does not make the design manufacturing-ready.",
        report["release_label"].as_str().unwrap_or(""),
        report.pointer("/body/label_counts").cloned().unwrap_or(Value::Null),
    );
    let phrase = format!("approve r{revision}");
    print!("Type '{phrase}' to approve: ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() || line.trim() != phrase {
        eprintln!("not approved");
        return 1;
    }
    let who = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".into());
    let entry = json!({
        "ts_ms": store::now_ms() as u64,
        "revision": revision,
        "fingerprint": fp,
        "approved_by": format!("human:{who}"),
        "via": "eak design approve (interactive terminal)",
    });
    match append_jsonl(&root.join(APPROVALS), &entry) {
        Ok(()) => {
            println!("approved; re-run `eak design report` to see the release label");
            0
        }
        Err(Fail(_, m, _)) => {
            eprintln!("eak design approve: {m}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    //! Written during the testing freeze: unrun.
    use super::*;

    fn spec() -> DesignSpec {
        serde_json::from_value(json!({
            "title": "12-24 V to 5 V 3 A buck",
            "design_class": "buck_converter",
            "rails": [
                {"net": "VIN", "role": "input", "v_min": 12.0, "v_nom": 24.0, "v_max": 24.0, "i_max_a": 1.5},
                {"net": "+5V", "role": "output", "v_min": 4.9, "v_nom": 5.0, "v_max": 5.1, "i_max_a": 3.0}
            ],
            "acceptance": [
                {"id": "duty", "description": "duty in range", "metric": "buck_duty_cycle", "max": 0.9},
                {"id": "ripple", "description": "ripple", "metric": "buck_inductor_ripple_a", "max": 1.2,
                 "params": {"inductance_h": 10e-6, "switching_hz": 400e3}},
                {"id": "erc", "description": "erc clean", "metric": "erc_errors", "max": 0},
                {"id": "sim", "description": "sim", "metric": "simulation"}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn spec_structure_is_checked() {
        let s = spec();
        assert!(check_spec(&s).0.is_empty());
        let mut bad = s.clone();
        bad.rails[0].v_min = 30.0;
        bad.acceptance.clear();
        assert_eq!(check_spec(&bad).0.len(), 2);
    }

    #[test]
    fn labels_follow_evidence() {
        let p = Project::new("t");
        let mut errs = BTreeMap::new();
        errs.insert("erc".to_string(), 0u64);
        let (c, f) = evaluate(&p, &spec(), &errs, false);
        let by = |id: &str| c.iter().find(|r| r.id == id).unwrap().clone();
        assert_eq!(by("duty").label, Label::Verified);
        assert_eq!(by("duty").passed, Some(true));
        assert_eq!(by("ripple").label, Label::Assumed); // L is agent-supplied
        assert_eq!(by("erc").label, Label::PassedDeterministicChecks);
        assert_eq!(by("sim").label, Label::Unverified);
        assert!(f.iter().any(|f| f.id == "rail-net-missing:+5V"));
    }

    #[test]
    fn high_voltage_without_safety_is_gated() {
        let s = spec();
        assert!(
            classify_and_gate(&s).1.is_empty(),
            "24 V buck with a design_class: no gate"
        );
        let mut hv = s.clone();
        hv.rails[0].v_max = 340.0;
        hv.rails[0].v_nom = 325.0;
        let gates = classify_and_gate(&hv).1;
        assert!(gates.iter().any(|g| g["id"] == "hv-isolation-undefined"));
        hv.safety = Some(Safety {
            isolation: "reinforced".into(),
            pollution_degree: Some(2),
            overvoltage_category: Some("II".into()),
            notes: None,
        });
        assert!(!classify_and_gate(&hv)
            .1
            .iter()
            .any(|g| g["id"] == "hv-isolation-undefined"));
    }

    #[test]
    fn intent_readback_flags_missing_parts() {
        let p = Project::new("t");
        let nl = netlist::build(&p);
        let mut intent = BTreeMap::new();
        intent.insert("U1.2".to_string(), "VIN".to_string());
        let f = intent_readback(&p, &nl, &intent);
        assert_eq!(f[0].id, "intent-part-missing:U1");
        assert_eq!(split_pin("U1.2"), Some(("U1", "2")));
        assert_eq!(split_pin("U1"), None);
    }

    #[test]
    fn fingerprint_is_stable() {
        let v = json!({"a": 1, "b": [1, 2]});
        assert_eq!(fingerprint(&v), fingerprint(&v.clone()));
        assert_eq!(fingerprint(&v).len(), 64);
    }
}
