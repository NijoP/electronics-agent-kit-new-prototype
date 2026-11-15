//! The harness tool registry and dispatcher.
//!
//! Tools are grouped by what they may do (`Category`):
//! * `read`          – inspect project state; never changes anything.
//! * `design`        – one typed `eak_runtime::Command` (or undo/redo) through
//!   `Engine::execute`; needs `expected_revision`.
//! * `simulation`    – ngspice on a snapshot of the current revision; never edits the design.
//! * `validation`    – ERC / DRC / MECH / DFM from `eak-validation`; each report is stored as
//!   revision-bound evidence. Never edits the design.
//! * `manufacturing` – compile the manufacturing package of the current revision into
//!   `output/rev-N/` (re-runs every check first). Never edits the design.
//! * `project` / `discovery` – create a project; list tools; read the agent guide.
//!
//! The command tools and most read tools are the ones `eak-intelligence` already generates from
//! the runtime (one schema per `Command` variant, checked against serde in its tests), so the
//! harness cannot drift from the command set.

use crate::context::CallContext;
use eak_core::Project;
use eak_engineering::netlist;
use eak_intelligence::tools::{command_tools, decode_command, read_tools, readonly};
use eak_runtime::store::{self, EntryState};
use eak_runtime::{Actor, Approval, CommandError, CommandRequest, Engine, OperationClass};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::Path;

/// File (inside the project folder) recording every harness call: who, what, outcome, revision.
pub const CALL_LOG: &str = "harness-calls.jsonl";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Read,
    Design,
    Simulation,
    Validation,
    Manufacturing,
    Project,
    Discovery,
    /// Autonomous design flow (ADR-0020): spec, plan, apply, validate, claim, report.
    Flow,
    /// Existing-project rework (ADR-0022): inspect, baseline, audit, protect, plan, apply, report.
    Rework,
    /// Advisory knowledge retrieval (ADR-0024): never a validation result, never a design edit.
    Knowledge,
    /// Agent task ledger (ADR-0036): begin, finish and read the execution-grounded task record.
    Task,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: String,
    pub category: Category,
    pub description: String,
    pub input_schema: Value,
    /// Risk class of a design tool (approval policy); `None` for other tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<OperationClass>,
    /// Checks to re-run after a design tool (from the command's domain).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rerun_checks: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnknownTool,
    InvalidArguments,
    NoProject,
    ProjectNotFound,
    ProjectExists,
    ProjectMismatch,
    ProjectLocked,
    MissingExpectedRevision,
    StaleRevision,
    ApprovalRequired,
    NotFound,
    Precondition,
    Integrity,
    Storage,
    NothingToDo,
    SimulatorUnavailable,
    SimulationFailed,
    ExportFailed,
    AppUnreachable,
    AuthFailed,
    Internal,
}

fn str_schema(desc: &str) -> Value {
    json!({"type": "string", "description": desc})
}

/// Add the common project-targeting properties (and `expected_revision`).
fn with_common(mut schema: Value, needs_revision: bool, revision_optional: bool) -> Value {
    let props = schema["properties"]
        .as_object_mut()
        .expect("tool schemas are objects");
    props.insert(
        "project_dir".into(),
        str_schema("absolute project folder; defaults to $EAK_PROJECT_DIR"),
    );
    props.insert(
        "project_id".into(),
        str_schema("expected project id; the call is refused if the folder holds another project (defaults to $EAK_PROJECT_ID)"),
    );
    if needs_revision || revision_optional {
        props.insert(
            "expected_revision".into(),
            json!({"type": "integer", "minimum": 0, "description": "project revision this call was planned against; a different current revision is refused with stale_revision"}),
        );
    }
    if needs_revision {
        schema["required"]
            .as_array_mut()
            .expect("required list")
            .push(json!("expected_revision"));
    }
    schema
}

fn spec(name: &str, category: Category, description: &str, schema: Value) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        category,
        description: description.into(),
        input_schema: with_common(
            schema,
            category == Category::Design,
            category == Category::Manufacturing || category == Category::Rework,
        ),
        class: None,
        rerun_checks: vec![],
    }
}

fn empty() -> Value {
    json!({"type": "object", "properties": {}, "required": [], "additionalProperties": false})
}

fn props(p: Value, required: &[&str]) -> Value {
    json!({"type": "object", "properties": p, "required": required, "additionalProperties": false})
}

fn plain(name: &str, category: Category, description: &str, input_schema: Value) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        category,
        description: description.into(),
        input_schema,
        class: None,
        rerun_checks: vec![],
    }
}

/// Every tool the harness offers, in a stable order.
pub fn tool_specs() -> Vec<ToolSpec> {
    use Category::{Design, Discovery, Manufacturing, Read, Simulation, Validation};
    let mut v = vec![
        plain("list_tools", Discovery, "List every EAK harness tool with its category and JSON input schema.", empty()),
        plain("guide", Discovery, "The agent guide: workflow, revision rules, categories and error codes (markdown).", empty()),
        plain(
            "create_project",
            Category::Project,
            "Create a new, empty EAK project folder (refuses an existing project). Returns its id at revision 0.",
            props(
                json!({"dir": str_schema("absolute folder to create"), "name": str_schema("project name")}),
                &["dir", "name"],
            ),
        ),
        spec("project_info", Read, "Project id, name, folder, current revision, undo/redo availability, recovery block and object counts. Call this first; use `revision` as expected_revision for design tools.", empty()),
        spec("list_symbols", Read, "Schematic symbols with refdes, value, component, position and every pin (number, name, electrical type, absolute position, net).", empty()),
        spec("get_component", Read, "Full definition of one component (project library or built-in): properties, pins, footprint pads, pin map, ratings, provenance.", props(json!({"component_id": str_schema("component id, e.g. R_0805")}), &["component_id"])),
        spec("list_footprints", Read, "PCB footprints with position, rotation, side, lock state and every pad (absolute position, size, shape, drill, net).", empty()),
        spec("get_board", Read, "Board geometry: outline, stackup, tracks, vias, zones and design-rule constraints (lengths in integer nm).", empty()),
        spec("get_connectivity", Read, "Physical copper connectivity of the PCB: per-net copper groups (1 = connected), shorts, and the ratsnest of unrouted pad pairs.", empty()),
        spec("get_history", Read, "Committed revisions from the operation journal (actor, command, revision before/after), harness tool calls, and stored validation evidence.", props(json!({"since_revision": {"type": "integer", "minimum": 0, "description": "only entries after this revision"}}), &[])),
    ];
    for t in read_tools() {
        if t.spec.name == "run_erc" || t.spec.name == "run_drc" {
            continue; // superseded by run_check, which also stores evidence
        }
        v.push(spec(
            &t.spec.name,
            Read,
            &t.spec.description,
            t.spec.input_schema.clone(),
        ));
    }
    v.push(spec(
        "run_check",
        Validation,
        "Run deterministic verification on the current revision: erc, drc, mech, dfm or all. Returns structured findings (rule_id, severity, message, location, objects, measured/required, suggested_fix) plus not_implemented and limitations, and stores each report as evidence. No errors does not prove the design correct.",
        props(json!({"check": {"type": "string", "enum": ["erc", "drc", "mech", "dfm", "all"], "description": "which check"}}), &["check"]),
    ));
    v.push(spec(
        "run_gates",
        Validation,
        "Fail-closed engineering gates on the current revision: shorts, required pads connected, net connectivity, clearances, annular ring, outline, stackup, copper continuity, fabricator capability, electrical constraints, return path. Each gate is PASS, FAIL, UNKNOWN or NOT_RUN with evidence; only PASS passes and unknown is never a pass. Stored as evidence. Not a manufacturing-ready claim.",
        props(json!({"fab_profile": {"type": "string", "enum": ["none", "jlcpcb_standard"], "description": "fabricator capability profile; none leaves the capability gate UNKNOWN"}}), &["fab_profile"]),
    ));
    let num = |d: &str| json!({"type": "number", "description": d});
    v.push(spec(
        "calc_copper_trace_width",
        Read,
        "IPC-2221 minimum trace width with every input explicit (no defaults); flags inputs outside the formula's source range. Source table unverified against the standard text.",
        props(
            json!({
                "current_a": num("continuous current, A"),
                "copper_thickness_um": num("copper thickness, µm (1 oz/ft² ≈ 34.8 µm)"),
                "temp_rise_k": num("allowed conductor temperature rise, K (= °C difference)"),
                "layer": {"type": "string", "enum": ["external", "internal"]},
            }),
            &["current_a", "copper_thickness_um", "temp_rise_k", "layer"],
        ),
    ));
    v.push(spec(
        "calc_copper_voltage_drop",
        Read,
        "DC resistance, voltage drop and dissipation of one copper conductor (IEC 60028 annealed copper, temperature-corrected). All inputs explicit; supply voltage only adds the percentage.",
        props(
            json!({
                "current_a": num("current, A"),
                "length_mm": num("conductor length, mm"),
                "width_mm": num("conductor width, mm"),
                "copper_thickness_um": num("copper thickness, µm"),
                "conductor_temperature_c": num("absolute conductor temperature, °C"),
                "supply_voltage_v": num("optional supply voltage, V"),
            }),
            &["current_a", "length_mm", "width_mm", "copper_thickness_um", "conductor_temperature_c"],
        ),
    ));
    v.push(spec(
        "power_analysis",
        Read,
        "Track-only DC drop upper bound per net with a declared current, declared currents with no copper, and return-path continuity (signal tracks over gaps in the adjacent ground plane, split ground zones). Unknown entries are reported with the reason.",
        empty(),
    ));
    v.extend(
        crate::intel::tool_specs()
            .into_iter()
            .map(|(name, description, schema)| spec(name, Read, description, schema)),
    );
    v.push(spec(
        "simulation_status",
        Simulation,
        "Whether ngspice is installed, and the nets/elements a simulation of the current revision would use (and parts it would skip).",
        empty(),
    ));
    v.push(spec(
        "run_simulation",
        Simulation,
        "Run ngspice on a snapshot of the current revision. Needs ngspice installed; never edits the design. Arguments are an eak-simulation SimRequest.",
        props(
            json!({
                "analysis": {"type": "object", "description": "the analysis, e.g. {\"kind\":\"op\"}; see eak-simulation Analysis"},
                "sources": {"type": "array", "description": "extra stimulus sources (eak-simulation ExtraSource)"},
                "timeout_secs": {"type": "integer", "minimum": 1},
            }),
            &["analysis"],
        ),
    ));
    v.push(spec(
        "export_manufacturing",
        Manufacturing,
        "Compile the manufacturing package (Gerber, drill, BOM, CPL, manifest) of the current revision into <project>/output/rev-N after re-running ERC/DRC/MECH/DFM. Failing gates mark the package draft. Never 'manufacturing-ready'.",
        empty(),
    ));
    // Advisory knowledge retrieval (ADR-0024, `knowledge` module).
    v.push(spec(
        "knowledge_search",
        Category::Knowledge,
        "Search local engineering knowledge (DESK-derived rules, EAK docs, text the user imported) with BM25. Hits carry source, page/section, revision and hashes. ADVISORY ONLY: never a validation result and never evidence for a gate.",
        props(
            json!({
                "query": str_schema("keywords"),
                "limit": {"type": "integer", "minimum": 1, "maximum": 20},
                "kind": {"type": "string", "enum": ["desk_rule", "eak_doc", "user_document"]},
                "part": str_schema("restrict to chunks about this MPN or LCSC id"),
            }),
            &["query"],
        ),
    ));
    v.push(spec(
        "knowledge_lookup",
        Category::Knowledge,
        "Exact lookup by MPN (given at import) or LCSC id (found in imported text): every chunk about that part with provenance and trust label. Advisory only.",
        props(json!({"part": str_schema("MPN or LCSC id")}), &["part"]),
    ));
    v.push(spec(
        "knowledge_cite",
        Category::Knowledge,
        "Citation (quote plus source, locator, revision, source and chunk SHA-256) for a chunk id returned by knowledge_search. Advisory only.",
        props(json!({"chunk_id": str_schema("id from knowledge_search")}), &["chunk_id"]),
    ));
    v.push(spec(
        "knowledge_import",
        Category::Knowledge,
        "Index a UTF-8 text file inside the project folder (extracted datasheet text, notes) into <project>/knowledge/index.json. Form feed = page break, markdown headings = sections. Does not change the design or its revision.",
        props(
            json!({
                "path": str_schema("file inside the project folder (absolute or relative to it)"),
                "source": str_schema("optional source label; defaults to the project-relative path"),
                "mpn": str_schema("optional manufacturer part number every chunk of this document is about (enables knowledge_lookup)"),
            }),
            &["path"],
        ),
    ));
    v.push(spec(
        "agent_task_begin",
        Category::Task,
        "Name the task this session is working on (starts the session task record at the current revision; an active task is only renamed). Phases are recorded from what the harness observes, never from this call. Does not change the design.",
        props(json!({"title": str_schema("short task title")}), &["title"]),
    ));
    v.push(spec(
        "agent_task_finish",
        Category::Task,
        "End this session's task: completed, partial, blocked, failed or cancelled. `completed` is granted only when the recorded evidence supports it (ERC-clean schematic, and a clean DRC after any board change); otherwise the task ends partial with the reason. Does not change the design.",
        props(
            json!({
                "outcome": {"type": "string", "enum": ["completed", "partial", "blocked", "failed", "cancelled"]},
                "note": str_schema("what was achieved or why it stopped"),
            }),
            &["outcome"],
        ),
    ));
    v.push(spec(
        "agent_task_status",
        Category::Task,
        "This session's active task and the project's recent tasks: phase, starting revision, latest revision, last ERC/DRC error counts and phase history. Read-only.",
        empty(),
    ));
    // Autonomous design flow (ADR-0020, `design` module).
    let flow = Category::Flow;
    v.push(plain(
        "design_spec_schema",
        Discovery,
        "JSON schema of the design requirement spec, the acceptance metrics EAK can evaluate deterministically, and the evidence labels.",
        empty(),
    ));
    v.push(plain(
        "design_procedure",
        Discovery,
        "Engineering mental models recovered from DESK as typed data: the 12-step schematic procedure (each step with the EAK gate that checks it), M1-M6 hard constraints (with their evaluator or none), derating facts and trade-off guidance.",
        empty(),
    ));
    v.push(spec(
        "design_spec_set",
        flow,
        "Store the structured requirement spec (rails with voltage/current, conditions, interfaces, protection, acceptance criteria, assumptions). Structural errors refuse it. Never edits the design.",
        props(json!({"spec": {"type": "object", "description": "see design_spec_schema"}}), &["spec"]),
    ));
    v.push(spec(
        "design_spec_get",
        flow,
        "The stored spec with its fingerprint and the revision it was set at.",
        empty(),
    ));
    v.push(spec(
        "design_plan",
        flow,
        "Record a plan: an ordered list of typed commands ({\"type\": \"PlaceSymbol\", ...}), each decoded now against the command schemas. Nothing is executed; returns plan_id bound to the current revision.",
        props(
            json!({
                "title": str_schema("short plan title"),
                "rationale": str_schema("why: which spec items or findings this addresses"),
                "addresses": {"type": "array", "items": {"type": "string"}, "description": "finding or criterion ids this plan fixes"},
                "steps": {"type": "array", "items": {"type": "object"}, "description": "typed commands; same shapes as the design tools plus \"type\""},
            }),
            &["title", "steps"],
        ),
    ));
    v.push(spec(
        "design_apply",
        flow,
        "Execute a recorded plan as one Engine::execute call (a Batch: all or nothing, one revision, one undo step) under the session's standing approval. expected_revision must equal the plan's base revision.",
        props(
            json!({
                "plan_id": str_schema("id from design_plan"),
                "expected_revision": {"type": "integer", "minimum": 0},
            }),
            &["plan_id", "expected_revision"],
        ),
    ));
    v.push(spec(
        "design_validate",
        flow,
        "Run ERC/DRC/MECH/DFM and evaluate the spec with eak-engineering (rail nets, declared current/voltage, IPC-2221 trace capacity, buck duty/ripple, ratings). Structured findings and labelled criteria, stored as evidence.",
        empty(),
    ));
    v.push(spec(
        "design_claim",
        flow,
        "Record a claim and its basis (assumption, datasheet, simulation, calculation, check). EAK labels it from the evidence it holds in design_report.",
        props(
            json!({
                "statement": str_schema("the claim"),
                "basis": {"type": "string", "enum": ["assumption", "datasheet", "simulation", "calculation", "check"]},
                "component_id": str_schema("datasheet basis: component"),
                "rating": str_schema("datasheet basis: rating name"),
                "criterion_id": str_schema("calculation/check basis: spec criterion id"),
            }),
            &["statement", "basis"],
        ),
    ));
    v.push(spec("design_iteration_log", flow, "The iteration record: spec, plan, apply, validate and claim events (revision, session, findings, fixes), plans, claims and human approvals.", empty()));
    v.push(spec(
        "design_report",
        flow,
        "The final evidence report at the current revision: every criterion and claim labelled verified / passed_deterministic_checks / simulation_supported / datasheet_supported / assumed / unverified, a fingerprint, and the human-approval state. Only a human can approve (eak design approve, interactive terminal). Never 'manufacturing-ready'.",
        empty(),
    ));
    // Existing-project rework (ADR-0022, `rework` module). Approval is a human CLI step, not a tool.
    let rework = Category::Rework;
    v.push(spec(
        "rework_inspect",
        rework,
        "Read-only inspection of an existing project: identity (optionally confirm expect_project_id / expect_name), object counts, domain fingerprints, check totals, unrouted connections, rework sessions. Writes nothing.",
        props(json!({"expect_project_id": str_schema("fail unless the folder holds this project id"), "expect_name": str_schema("fail unless the project has this name")}), &[]),
    ));
    v.push(spec(
        "rework_baseline",
        rework,
        "Capture the immutable baseline of the project (project copy, per-object hashes, domain fingerprints) and open a rework session rw-NNNN. Requires confirm_project_id equal to the project's id. Never rewritten; every later rework tool verifies it.",
        props(json!({"confirm_project_id": str_schema("the project id read from rework_inspect")}), &["confirm_project_id"]),
    ));
    v.push(spec(
        "rework_audit",
        rework,
        "Classify the design's issues (schematic, placement, power_thermal, routing, copper, drc) from eak-validation findings and an eak-engineering current-sizing recompute: severity, affected objects, suggested fix. Never edits the design.",
        props(json!({"session_id": str_schema("rework session (default: latest)")}), &[]),
    ));
    v.push(spec(
        "rework_protect",
        rework,
        "Add objects (by id) to the PROTECTED set so no repair may change them. Add-only: no tool releases protection; locked footprints are always protected. Everything outside an issue's scope is ACCEPTED and verified unchanged after each repair.",
        props(json!({"session_id": str_schema("rework session (default: latest)"), "ids": {"type": "array", "items": {"type": "string"}, "description": "object ids"}, "reason": str_schema("why these objects are protected")}), &["ids", "reason"]),
    ));
    v.push(spec(
        "rework_plan",
        rework,
        "Record a dependency-ordered repair plan: one repair per audit issue, each a list of typed commands ({\"type\":\"MoveFootprint\",...}). Refused (nothing stored) if a step changes a PROTECTED or out-of-scope object, or is not surgical (AutoPlace, unscoped ClearRouting/AutoRoute). Nothing runs: a human must approve with `eak rework approve` (interactive terminal).",
        props(json!({
            "session_id": str_schema("rework session (default: latest)"),
            "title": str_schema("plan title"),
            "repairs": {"type": "array", "description": "repairs: {issue_id, description, steps:[typed commands]}", "items": {"type": "object"}},
        }), &["title", "repairs"]),
    ));
    v.push(spec(
        "rework_apply",
        rework,
        "Execute a human-approved plan: each repair is one Engine::execute (a Batch); after each, the revision is read back and every PROTECTED/ACCEPTED object is re-hashed; a violation undoes that repair and stops. Then runs ERC/DRC/MECH/DFM. Needs expected_revision equal to the plan's base revision.",
        props(json!({"plan_id": str_schema("id from rework_plan"), "expected_revision": {"type": "integer", "minimum": 0}}), &["plan_id", "expected_revision"]),
    ));
    v.push(spec(
        "rework_report",
        rework,
        "Readiness report at the current revision: diff against the baseline, PROTECTED integrity, issues resolved/remaining/new, revisions made outside rework, checks with not_implemented and limitations, human approvals. Labelled (integrity_violation, changes_outside_rework, repairs_incomplete, passed_deterministic_checks); never 'manufacturing-ready'.",
        props(json!({"session_id": str_schema("rework session (default: latest)")}), &[]),
    ));
    for t in command_tools() {
        let mut s = spec(
            &t.spec.name,
            Design,
            &t.spec.description,
            t.spec.input_schema.clone(),
        );
        s.class = Some(t.class);
        s.rerun_checks = t.domain.checks().iter().map(|c| c.to_string()).collect();
        v.push(s);
    }
    for (name, what) in [
        ("Undo", "Undo the latest change (any actor)."),
        ("Redo", "Redo the latest undone change."),
    ] {
        let mut s = spec(name, Design, what, empty());
        s.class = Some(OperationClass::ReversibleLocal);
        s.rerun_checks = vec!["ERC".into(), "DRC".into(), "MECH".into()];
        v.push(s);
    }
    v
}

pub fn find_spec(name: &str) -> Option<ToolSpec> {
    tool_specs().into_iter().find(|t| t.name == name)
}

/// A failed call that never reached an open project.
pub fn envelope_error(
    tool: &str,
    ctx: &CallContext,
    code: ErrorCode,
    message: &str,
    details: Value,
) -> Value {
    json!({
        "ok": false,
        "tool": tool,
        "category": find_spec(tool).map(|s| s.category),
        "session": {"id": ctx.session_id, "agent": ctx.agent},
        "project": ctx.project.dir.as_ref().map(|d| json!({"dir": d, "id": ctx.project.id})),
        "error": {"code": code, "message": message, "details": details},
    })
}

fn project_echo(e: &Engine) -> Value {
    let p = e.project();
    json!({"id": p.id, "name": p.name, "dir": e.dir(), "revision": p.revision})
}

pub(crate) struct Fail(pub(crate) ErrorCode, pub(crate) String, pub(crate) Value);

fn fail(code: ErrorCode, msg: impl Into<String>) -> Fail {
    Fail(code, msg.into(), Value::Null)
}

pub(crate) fn command_error(e: &CommandError) -> Fail {
    let (code, details) = match e {
        CommandError::NotFound(_) => (ErrorCode::NotFound, Value::Null),
        CommandError::Invalid(_) => (ErrorCode::InvalidArguments, Value::Null),
        CommandError::Precondition(_) => (ErrorCode::Precondition, Value::Null),
        CommandError::StaleRevision { expected, actual } => (
            ErrorCode::StaleRevision,
            json!({"expected_revision": expected, "current_revision": actual,
                   "hint": "re-read the project (project_info), re-plan against the current revision and retry"}),
        ),
        CommandError::ApprovalRequired(_) => (
            ErrorCode::ApprovalRequired,
            json!({"hint": "destructive commands need the user to start the agent with EAK_HARNESS_ALLOW_DESTRUCTIVE=1 (or eak --allow-destructive)"}),
        ),
        CommandError::Integrity(_) => (ErrorCode::Integrity, Value::Null),
        CommandError::Storage(_) => (ErrorCode::Storage, Value::Null),
        CommandError::NothingTo(_) => (ErrorCode::NothingToDo, Value::Null),
    };
    Fail(code, e.to_string(), details)
}

fn expected_revision(args: &Value) -> Result<u64, Fail> {
    match args.get("expected_revision") {
        None | Some(Value::Null) => Err(fail(
            ErrorCode::MissingExpectedRevision,
            "design tools need expected_revision (read it from project_info)",
        )),
        Some(v) => v.as_u64().ok_or_else(|| {
            fail(
                ErrorCode::InvalidArguments,
                "expected_revision must be a non-negative integer",
            )
        }),
    }
}

/// Strip the harness-level keys before decoding command / simulation arguments.
fn command_args(args: &Value) -> Value {
    let mut m = args.as_object().cloned().unwrap_or_default();
    for k in ["expected_revision", "project_dir", "project_id"] {
        m.remove(k);
    }
    Value::Object(m)
}

/// Dispatch one tool call against an open engine and wrap the result in the response envelope.
/// `route` is echoed ("direct" or "app"). The call is appended to the project's call log.
pub fn handle(
    engine: &mut Engine,
    ctx: &CallContext,
    route: &str,
    tool: &str,
    args: &Value,
) -> Value {
    let spec = find_spec(tool);
    let result = match &spec {
        None => Err(fail(
            ErrorCode::UnknownTool,
            format!("no tool named {tool}; call list_tools"),
        )),
        Some(_) if !args.is_object() && !args.is_null() => Err(fail(
            ErrorCode::InvalidArguments,
            "arguments must be a JSON object",
        )),
        Some(s) => match &ctx.project.id {
            Some(id) if *id != engine.project().id => Err(Fail(
                ErrorCode::ProjectMismatch,
                format!(
                    "the folder holds project {} but the call targets {id}",
                    engine.project().id
                ),
                json!({"expected_project_id": id, "actual_project_id": engine.project().id}),
            )),
            _ => dispatch(engine, ctx, s, args),
        },
    };
    let mut out = json!({
        "tool": tool,
        "category": spec.as_ref().map(|s| s.category),
        "route": route,
        "session": {"id": ctx.session_id, "agent": ctx.agent},
    });
    let (ok, code) = match result {
        Ok(v) => {
            out["ok"] = json!(true);
            out["result"] = v;
            (true, None)
        }
        Err(Fail(code, message, details)) => {
            out["ok"] = json!(false);
            out["error"] = json!({"code": code, "message": message, "details": details});
            (false, Some(code))
        }
    };
    // Echo the project state *after* the call (a design tool moved the revision).
    out["project"] = project_echo(engine);
    crate::agent_task::record(engine, ctx, tool, args, &out);
    log_call(engine, ctx, tool, ok, code, &out);
    out
}

fn log_call(
    engine: &Engine,
    ctx: &CallContext,
    tool: &str,
    ok: bool,
    code: Option<ErrorCode>,
    out: &Value,
) {
    let Some(dir) = engine.dir() else { return };
    let checks = out
        .pointer("/result/reports")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
            .map(|x| json!({"check": x["check"], "errors": x["errors"], "warnings": x["warnings"]}))
            .collect::<Vec<_>>()
        });
    let line = json!({
        "ts_ms": store::now_ms(),
        "session": ctx.session_id,
        "agent": ctx.agent,
        "tool": tool,
        "ok": ok,
        "error": code,
        "revision": engine.project().revision,
        "op_id": out.pointer("/result/op_id"),
        "revision_before": out.pointer("/result/revision_before"),
        "revision_after": out.pointer("/result/revision_after"),
        "checks": checks,
    });
    let res = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(CALL_LOG))
        .and_then(|mut f| writeln!(f, "{line}"));
    if let Err(e) = res {
        eprintln!("eak-harness: call log not written: {e}");
    }
}

fn dispatch(
    engine: &mut Engine,
    ctx: &CallContext,
    spec: &ToolSpec,
    args: &Value,
) -> Result<Value, Fail> {
    match spec.category {
        Category::Design => design(engine, ctx, spec, args),
        Category::Validation if spec.name == "run_gates" => run_gates(engine, args),
        Category::Validation => {
            let check = args.get("check").and_then(Value::as_str).unwrap_or("");
            run_checks(engine, check)
        }
        Category::Manufacturing => export(engine, args),
        Category::Simulation => simulation(engine.project(), &spec.name, args),
        Category::Read => read(engine, &spec.name, args),
        Category::Flow => crate::design::dispatch(engine, ctx, &spec.name, args),
        Category::Rework => crate::rework::dispatch(engine, ctx, &spec.name, args),
        Category::Knowledge => crate::knowledge::dispatch(engine, &spec.name, args),
        Category::Task => crate::agent_task::dispatch(engine, ctx, &spec.name, args),
        // Discovery needs no project, but a client that sends its project context (the MCP
        // server, any app-endpoint client) must still get an answer rather than a refusal.
        Category::Discovery => Ok(discovery(&spec.name)),
        Category::Project => Err(fail(
            ErrorCode::InvalidArguments,
            format!("{} does not run against an open project", spec.name),
        )),
    }
}

/// Result of a discovery tool; none of them reads a project.
pub fn discovery(tool: &str) -> Value {
    match tool {
        "guide" => json!({"markdown": crate::GUIDE}),
        "design_spec_schema" => crate::design::spec_schema(),
        "design_procedure" => crate::design::procedure(),
        _ => list_tools_json(),
    }
}

fn design(
    engine: &mut Engine,
    ctx: &CallContext,
    spec: &ToolSpec,
    args: &Value,
) -> Result<Value, Fail> {
    let expected = expected_revision(args)?;
    let actor = Actor::agent_in_session(&ctx.agent, &ctx.session_id);
    let resp = match spec.name.as_str() {
        "Undo" => engine.undo(expected, &actor),
        "Redo" => engine.redo(expected, &actor),
        name => {
            let command = decode_command(name, &command_args(args))
                .map_err(|e| fail(ErrorCode::InvalidArguments, e))?;
            // ADR-0036: PCB synchronisation needs a committed schematic and an ERC run on it.
            if carries_pcb_sync(&command) {
                crate::agent_task::pcb_sync_gate(engine).map_err(|why| {
                    Fail(
                        ErrorCode::Precondition,
                        why,
                        json!({"gate": "pcb_sync", "needs": ["committed schematic symbols", "ERC run since the last schematic change"]}),
                    )
                })?;
            }
            engine.execute(CommandRequest {
                expected_revision: expected,
                actor,
                command,
                approval: Some(Approval {
                    approved_by: format!("harness-session:{}", ctx.session_id),
                    max_class: ctx.max_class,
                    note: "standing approval of the user who started this agent session through the EAK harness".into(),
                }),
            })
        }
    };
    if let Some(e) = &resp.error {
        let mut f = command_error(e);
        if f.2.is_null() {
            f.2 = json!({"op_id": resp.op_id, "revision": resp.revision_before});
        }
        return Err(f);
    }
    let mut v =
        serde_json::to_value(&resp).map_err(|e| fail(ErrorCode::Internal, e.to_string()))?;
    v["rerun_checks"] = json!(spec.rerun_checks);
    Ok(v)
}

/// Whether a command is, or (in a batch) contains, the PCB synchronisation the ERC gate guards.
fn carries_pcb_sync(c: &eak_runtime::Command) -> bool {
    match c {
        eak_runtime::Command::SyncSchematicToPcb => true,
        eak_runtime::Command::Batch { commands, .. } => commands.iter().any(carries_pcb_sync),
        _ => false,
    }
}

fn report_json(r: &eak_validation::Report, evidence: Option<&Path>) -> Value {
    let mut v = serde_json::to_value(r).unwrap_or(Value::Null);
    if let Some(m) = v.as_object_mut() {
        let rules = m
            .remove("rules")
            .and_then(|r| r.as_array().map(Vec::len))
            .unwrap_or(0);
        m.insert("rules_evaluated".into(), json!(rules));
        m.insert("evidence_path".into(), json!(evidence));
    }
    v
}

pub(crate) fn all_reports(p: &Project) -> Vec<eak_validation::Report> {
    vec![
        eak_validation::erc::run(p),
        eak_validation::drc::run(p),
        eak_validation::mech::run(p),
        eak_validation::dfm::run(p, &eak_validation::dfm::jlcpcb_standard()),
    ]
}

pub(crate) fn run_checks(engine: &mut Engine, check: &str) -> Result<Value, Fail> {
    let p = engine.project();
    let reports = match check {
        "erc" => vec![eak_validation::erc::run(p)],
        "drc" => vec![eak_validation::drc::run(p)],
        "mech" => vec![eak_validation::mech::run(p)],
        "dfm" => vec![eak_validation::dfm::run(
            p,
            &eak_validation::dfm::jlcpcb_standard(),
        )],
        "all" => all_reports(p),
        other => {
            return Err(fail(
                ErrorCode::InvalidArguments,
                format!("check must be erc, drc, mech, dfm or all, got {other:?}"),
            ))
        }
    };
    let mut out = vec![];
    for r in &reports {
        let path = match engine.dir() {
            Some(_) => {
                let json = serde_json::to_string_pretty(r)
                    .map_err(|e| fail(ErrorCode::Internal, e.to_string()))?;
                Some(engine.save_evidence(&r.check, &json).ok_or_else(|| {
                    fail(
                        ErrorCode::Storage,
                        format!("{} ran but its evidence could not be saved", r.check),
                    )
                })?)
            }
            None => None,
        };
        out.push(report_json(r, path.as_deref()));
    }
    let errors: usize = reports.iter().map(|r| r.errors).sum();
    let warnings: usize = reports.iter().map(|r| r.warnings).sum();
    Ok(json!({
        "revision": engine.project().revision,
        "errors": errors,
        "warnings": warnings,
        "reports": out,
        "note": "deterministic checks from eak-validation; no errors in implemented checks does not prove the design correct (see not_implemented and limitations)",
    }))
}

fn run_gates(engine: &mut Engine, args: &Value) -> Result<Value, Fail> {
    let r = crate::engineering::run_gates(engine.project(), args)
        .map_err(|e| fail(ErrorCode::InvalidArguments, e))?;
    let json =
        serde_json::to_string_pretty(&r).map_err(|e| fail(ErrorCode::Internal, e.to_string()))?;
    let path = match engine.dir() {
        Some(_) => Some(engine.save_evidence("GATES", &json).ok_or_else(|| {
            fail(
                ErrorCode::Storage,
                "gates ran but their evidence could not be saved",
            )
        })?),
        None => None,
    };
    let mut v = serde_json::to_value(&r).map_err(|e| fail(ErrorCode::Internal, e.to_string()))?;
    v["evidence_path"] = json!(path);
    Ok(v)
}

fn export(engine: &mut Engine, args: &Value) -> Result<Value, Fail> {
    if let Some(v) = args.get("expected_revision").filter(|v| !v.is_null()) {
        let expected = v.as_u64().ok_or_else(|| {
            fail(
                ErrorCode::InvalidArguments,
                "expected_revision must be a non-negative integer",
            )
        })?;
        let actual = engine.project().revision;
        if expected != actual {
            return Err(command_error(&CommandError::StaleRevision {
                expected,
                actual,
            }));
        }
    }
    let dir = engine
        .dir()
        .ok_or_else(|| {
            fail(
                ErrorCode::Precondition,
                "export needs a saved project folder",
            )
        })?
        .to_path_buf();
    let reports = all_reports(engine.project());
    for r in &reports {
        let json = serde_json::to_string_pretty(r)
            .map_err(|e| fail(ErrorCode::Internal, e.to_string()))?;
        engine.save_evidence(&r.check, &json).ok_or_else(|| {
            fail(
                ErrorCode::Storage,
                format!("{} evidence could not be saved", r.check),
            )
        })?;
    }
    let out = dir
        .join("output")
        .join(format!("rev-{}", engine.project().revision));
    let refs: Vec<&eak_validation::Report> = reports.iter().collect();
    let manifest = eak_manufacturing::compile(engine.project(), &refs, &out)
        .map_err(|e| fail(ErrorCode::ExportFailed, e))?;
    Ok(json!({
        "revision": engine.project().revision,
        "out_dir": out,
        "manifest": manifest,
        "checks": reports.iter().map(|r| json!({"check": r.check, "errors": r.errors, "warnings": r.warnings})).collect::<Vec<_>>(),
    }))
}

fn simulation(p: &Project, name: &str, args: &Value) -> Result<Value, Fail> {
    if name == "simulation_status" {
        return Ok(json!({
            "revision": p.revision,
            "ngspice": eak_simulation::probe(),
            "circuit": eak_simulation::describe(p),
        }));
    }
    let req: eak_simulation::SimRequest =
        serde_json::from_value(command_args(args)).map_err(|e| {
            fail(
                ErrorCode::InvalidArguments,
                format!("simulation request: {e}"),
            )
        })?;
    let status = eak_simulation::probe();
    if !status.available {
        return Err(Fail(
            ErrorCode::SimulatorUnavailable,
            status
                .message
                .clone()
                .unwrap_or_else(|| "ngspice is not installed".into()),
            json!({"ngspice": status}),
        ));
    }
    let r = eak_simulation::run(p, &req)
        .map_err(|e| fail(ErrorCode::SimulationFailed, e.to_string()))?;
    Ok(json!({"revision": p.revision, "simulation": r}))
}

const UNITS: &str = "lengths are integer nanometres (1 mm = 1000000)";

fn read(engine: &Engine, name: &str, args: &Value) -> Result<Value, Fail> {
    let p = engine.project();
    match name {
        "project_info" => Ok(json!({
            "id": p.id,
            "name": p.name,
            "dir": engine.dir(),
            "revision": p.revision,
            "can_undo": engine.can_undo(),
            "can_redo": engine.can_redo(),
            "blocked": engine.blocked_reason(),
            "counts": {
                "sheets": p.schematic.sheets.len(),
                "symbols": p.schematic.symbols.len(),
                "wires": p.schematic.wires.len(),
                "labels": p.schematic.labels.len(),
                "footprints": p.pcb.footprints.len(),
                "tracks": p.pcb.tracks.len(),
                "vias": p.pcb.vias.len(),
                "zones": p.pcb.zones.len(),
                "board_outline_points": p.pcb.board.outline.len(),
            },
            "units": UNITS,
        })),
        "list_symbols" => Ok(list_symbols(p)),
        "get_component" => {
            let id = args
                .get("component_id")
                .and_then(Value::as_str)
                .ok_or_else(|| fail(ErrorCode::InvalidArguments, "component_id is required"))?;
            let (def, in_project) = match p.library.get(id) {
                Some(d) => (d.clone(), true),
                None => (
                    eak_core::builtin::components()
                        .into_iter()
                        .find(|d| d.id == id)
                        .ok_or_else(|| {
                            fail(
                                ErrorCode::NotFound,
                                format!("component {id} (call list_library_components)"),
                            )
                        })?,
                    false,
                ),
            };
            Ok(
                json!({"revision": p.revision, "in_project": in_project, "component": def, "units": UNITS}),
            )
        }
        "list_footprints" => Ok(list_footprints(p)),
        "get_board" => Ok(json!({
            "revision": p.revision,
            "outline": p.pcb.board.outline,
            "stackup": p.pcb.board.stackup,
            "tracks": p.pcb.tracks.values().collect::<Vec<_>>(),
            "vias": p.pcb.vias.values().collect::<Vec<_>>(),
            "zones": p.pcb.zones.values().collect::<Vec<_>>(),
            "constraints": p.constraints,
            "units": UNITS,
        })),
        "get_connectivity" => {
            let c = eak_engineering::copper::analyse(p);
            let mut v =
                serde_json::to_value(&c).map_err(|e| fail(ErrorCode::Internal, e.to_string()))?;
            v["revision"] = json!(p.revision);
            v["unrouted_connections"] = json!(c.ratsnest.len());
            Ok(v)
        }
        "get_history" => Ok(history(
            engine,
            args.get("since_revision")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        )),
        "calc_copper_trace_width" => {
            crate::engineering::trace_width(args).map_err(|e| fail(ErrorCode::InvalidArguments, e))
        }
        "calc_copper_voltage_drop" => {
            crate::engineering::voltage_drop(args).map_err(|e| fail(ErrorCode::InvalidArguments, e))
        }
        "power_analysis" => {
            crate::engineering::power_analysis(p).map_err(|e| fail(ErrorCode::Internal, e))
        }
        n if n.starts_with("intel_") => {
            crate::intel::read(engine, n, args).map_err(|e| fail(ErrorCode::InvalidArguments, e))
        }
        other => match readonly::run(other, args, p) {
            Some(r) => r.map_err(|e| fail(ErrorCode::InvalidArguments, e)),
            None => Err(fail(
                ErrorCode::UnknownTool,
                format!("no read tool named {other}"),
            )),
        },
    }
}

fn list_symbols(p: &Project) -> Value {
    let nl = netlist::build(p);
    let symbols: Vec<Value> = p
        .schematic
        .symbols
        .values()
        .map(|s| {
            let pins: Vec<Value> = p
                .library
                .get(&s.component_id)
                .map(|d| {
                    d.symbol
                        .pins
                        .iter()
                        .map(|pin| {
                            json!({
                                "number": pin.number,
                                "name": pin.name,
                                "type": pin.electrical_type,
                                "position": p.pin_position(s, &pin.number),
                                "net": nl.pin_net.get(&(s.id.clone(), pin.number.clone())),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            json!({
                "id": s.id, "refdes": s.refdes, "value": s.value,
                "component_id": s.component_id, "sheet_id": s.sheet_id,
                "position": s.position, "rotation": s.rotation, "pins": pins,
            })
        })
        .collect();
    json!({"revision": p.revision, "symbols": symbols, "units": UNITS})
}

fn list_footprints(p: &Project) -> Value {
    let fps: Vec<Value> = p
        .pcb
        .footprints
        .values()
        .map(|f| {
            let pads: Vec<Value> = p
                .library
                .get(&f.component_id)
                .map(|d| {
                    d.footprint
                        .pads
                        .iter()
                        .map(|pad| {
                            json!({
                                "number": pad.number, "kind": pad.kind, "shape": pad.shape,
                                "position": p.pad_position(f, pad),
                                "size_x": pad.size_x, "size_y": pad.size_y, "drill": pad.drill,
                                "net": f.pad_nets.get(&pad.number),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            json!({
                "id": f.id, "refdes": f.refdes, "symbol_id": f.symbol_id,
                "component_id": f.component_id, "position": f.position,
                "rotation": f.rotation, "side": f.side, "locked": f.locked, "pads": pads,
            })
        })
        .collect();
    json!({"revision": p.revision, "footprints": fps, "units": UNITS})
}

fn history(engine: &Engine, since: u64) -> Value {
    let Some(dir) = engine.dir() else {
        return json!({"revision": engine.project().revision, "journal": [], "tool_calls": [], "evidence": []});
    };
    let journal = store::read_journal(dir);
    let committed: std::collections::BTreeSet<&str> = journal
        .iter()
        .filter(|e| e.state == EntryState::Committed)
        .map(|e| e.op_id.as_str())
        .collect();
    let revisions: Vec<Value> = journal
        .iter()
        .filter(|e| e.state == EntryState::Pending && committed.contains(e.op_id.as_str()))
        .filter(|e| e.revision_after > since)
        .map(|e| {
            json!({
                "op_id": e.op_id, "kind": e.kind, "target_op": e.target_op,
                "revision_before": e.revision_before, "revision_after": e.revision_after,
                "actor": e.actor, "command": e.command, "timestamp_ms": e.timestamp_ms,
                "changes": e.changes.len(),
            })
        })
        .collect();
    let calls: Vec<Value> = std::fs::read_to_string(dir.join(CALL_LOG))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|c| since == 0 || c["revision"].as_u64().unwrap_or(0) > since)
        .collect();
    let calls = calls[calls.len().saturating_sub(200)..].to_vec();
    let mut evidence: Vec<String> = std::fs::read_dir(dir.join("evidence"))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    evidence.sort();
    json!({
        "revision": engine.project().revision,
        "journal": revisions,
        "tool_calls": calls,
        "evidence": evidence,
    })
}

/// The `list_tools` payload.
pub fn list_tools_json() -> Value {
    let tools = tool_specs();
    let mut by_cat: Map<String, Value> = Map::new();
    for t in &tools {
        let k = serde_json::to_value(t.category)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        if let Some(a) = by_cat.entry(k).or_insert_with(|| json!([])).as_array_mut() {
            a.push(json!(t.name));
        }
    }
    json!({"count": tools.len(), "by_category": by_cat, "tools": tools})
}
