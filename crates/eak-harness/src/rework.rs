//! Existing-project rework flow (ADR-0022).
//!
//! Continue an unfinished or imperfect EAK project with surgical, approved, verified changes
//! instead of rebuilding it. Ported (re-written, no source copied) from the mental model of
//! DESK `desk/rework/` (MIT, Copyright (c) 2026 NijoP; licence in `third_party/licenses/DESK/`):
//!
//! 1. `rework_inspect`  – read-only: identity, inventory, checks. Writes nothing.
//! 2. `rework_baseline` – immutable baseline: project copy + per-object hashes + domain
//!    fingerprints, bound to the confirmed project id. A session id (`rw-NNNN`) is created.
//! 3. `rework_audit`    – classify issues from eak-validation (ERC/DRC/MECH/DFM) and an
//!    eak-engineering current-sizing recompute: category, severity, affected objects.
//! 4. `rework_protect`  – add objects to the PROTECTED set (add-only; locked footprints are
//!    protected automatically). Everything not in an issue's scope is ACCEPTED (must not change).
//! 5. `rework_plan`     – dependency-ordered repairs, each tied to an audit issue and made of
//!    typed commands. Refused when a step mutates a PROTECTED or out-of-scope object.
//! 6. approval          – NOT a tool: `eak rework approve` works only from an interactive
//!    terminal with a typed phrase, bound to the plan fingerprint and base revision.
//! 7. `rework_apply`    – each repair is one `Engine::execute` (a Batch); after each one the
//!    revision is read back and every PROTECTED/ACCEPTED object is re-hashed; a violation
//!    undoes that repair and stops.
//! 8. `rework_report`   – baseline-vs-now diff, protected-object integrity, issue resolution,
//!    unattributed revisions, labelled readiness. Never "manufacturing-ready".
//!
//! Records live in `<project>/rework/<session>/` as evidence about the design, never design
//! data: the design changes only through `Engine::execute`.

use crate::context::CallContext;
use crate::design::{
    append_jsonl, arg_str, decode_step, fingerprint, invalid, read_jsonl, storage, write_json,
};
use crate::tools::{all_reports, command_error, run_checks, ErrorCode, Fail};
use eak_core::Project;
use eak_runtime::command::MAX_BATCH;
use eak_runtime::store::{self, EntryState};
use eak_runtime::{Actor, Approval, Command, CommandRequest, Engine, OperationClass};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub const REWORK_DIR: &str = "rework";
const BASELINE: &str = "baseline.json";
const BASELINE_PROJECT: &str = "baseline-project.json";
const AUDIT: &str = "audit.json";
const AUDITS: &str = "audits";
const PLANS: &str = "plans";
const REPORTS: &str = "reports";
const PROTECTION: &str = "protection.json";
const EVENTS: &str = "events.jsonl";
const APPROVALS: &str = "approvals.jsonl";

/// Engineering dependency order of issue categories (DESK `RepairPlanner`).
const CATEGORIES: [&str; 8] = [
    "requirements",
    "schematic",
    "component",
    "placement",
    "power_thermal",
    "routing",
    "copper",
    "drc",
];

fn category_rank(c: &str) -> usize {
    CATEGORIES.iter().position(|x| *x == c).unwrap_or(99)
}

fn severity_rank(s: &str) -> usize {
    ["critical", "high", "medium", "low"]
        .iter()
        .position(|x| *x == s)
        .unwrap_or(9)
}

// ---------------------------------------------------------------- objects and fingerprints

#[derive(Clone)]
struct Obj {
    kind: &'static str,
    hash: String,
    net: Option<String>,
}

type Index = BTreeMap<String, Obj>;

fn val<T: serde::Serialize>(t: &T) -> Value {
    serde_json::to_value(t).unwrap_or(Value::Null)
}

fn h16(v: &Value) -> String {
    fingerprint(v)[..16].to_string()
}

fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Every design object by id with a content hash. Singletons are `@board`, `@constraints`,
/// `@manufacturing`.
fn index(p: &Project) -> Index {
    let mut m = Index::new();
    let mut put = |id: &str, kind: &'static str, v: Value, net: Option<String>| {
        m.insert(
            id.to_string(),
            Obj {
                kind,
                hash: h16(&v),
                net,
            },
        );
    };
    for (id, x) in &p.pcb.footprints {
        put(id, "footprint", val(x), None);
    }
    for (id, x) in &p.pcb.tracks {
        put(id, "track", val(x), Some(x.net.clone()));
    }
    for (id, x) in &p.pcb.vias {
        put(id, "via", val(x), Some(x.net.clone()));
    }
    for (id, x) in &p.pcb.zones {
        put(id, "zone", val(x), Some(x.net.clone()));
    }
    for (id, x) in &p.schematic.symbols {
        put(id, "symbol", val(x), None);
    }
    for (id, x) in &p.schematic.wires {
        put(id, "wire", val(x), None);
    }
    for (id, x) in &p.schematic.labels {
        put(id, "label", val(x), None);
    }
    for (id, x) in &p.schematic.no_connects {
        put(id, "no_connect", val(x), None);
    }
    for (id, x) in &p.schematic.sheets {
        put(id, "sheet", val(x), None);
    }
    put("@board", "board", val(&p.pcb.board), None);
    put("@constraints", "constraints", val(&p.constraints), None);
    put(
        "@manufacturing",
        "manufacturing",
        val(&p.manufacturing),
        None,
    );
    m
}

fn domain_fp(idx: &Index, kinds: &[&str]) -> String {
    let mut s = String::new();
    for (id, o) in idx {
        if kinds.contains(&o.kind) {
            s.push_str(&format!("{id}:{}\n", o.hash));
        }
    }
    sha_hex(s.as_bytes())
}

fn fingerprints(idx: &Index) -> Value {
    json!({
        "schematic": domain_fp(idx, &["symbol", "wire", "label", "no_connect", "sheet"]),
        "placement": domain_fp(idx, &["footprint"]),
        "copper": domain_fp(idx, &["track", "via", "zone"]),
        "board": domain_fp(idx, &["board"]),
        "rules": domain_fp(idx, &["constraints", "manufacturing"]),
        "composite": domain_fp(idx, &["footprint", "track", "via", "zone", "symbol", "wire", "label", "no_connect", "sheet", "board", "constraints", "manufacturing"]),
    })
}

fn kind_counts(idx: &Index) -> Value {
    let mut c: BTreeMap<&str, usize> = BTreeMap::new();
    for o in idx.values() {
        *c.entry(o.kind).or_insert(0) += 1;
    }
    json!(c)
}

// ---------------------------------------------------------------- storage

fn root(engine: &Engine) -> Result<PathBuf, Fail> {
    engine.dir().map(|d| d.join(REWORK_DIR)).ok_or_else(|| {
        Fail(
            ErrorCode::Precondition,
            "rework needs a saved project folder".into(),
            Value::Null,
        )
    })
}

fn sessions(root: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(root)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("rw-"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn safe_id(s: &str) -> bool {
    s.starts_with("rw-")
        && s.len() <= 40
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn session_of(root: &Path, args: &Value) -> Result<String, Fail> {
    if let Some(s) = args.get("session_id").and_then(Value::as_str) {
        if !safe_id(s) || !root.join(s).is_dir() {
            return Err(Fail(
                ErrorCode::NotFound,
                format!("no rework session {s}; call rework_inspect"),
                Value::Null,
            ));
        }
        return Ok(s.to_string());
    }
    sessions(root).pop().ok_or_else(|| {
        Fail(
            ErrorCode::Precondition,
            "no rework session yet: call rework_baseline first".into(),
            Value::Null,
        )
    })
}

/// The baseline body, verified: its fingerprint and the stored project copy's hash.
fn load_baseline(root: &Path, sid: &str) -> Result<Value, Fail> {
    let dir = root.join(sid);
    let text = std::fs::read_to_string(dir.join(BASELINE)).map_err(|_| {
        Fail(
            ErrorCode::Precondition,
            "this session has no baseline".into(),
            Value::Null,
        )
    })?;
    let doc: Value = serde_json::from_str(&text).map_err(storage)?;
    let body = doc["body"].clone();
    let copy = std::fs::read(dir.join(BASELINE_PROJECT)).unwrap_or_default();
    if doc["fingerprint"].as_str() != Some(fingerprint(&body).as_str())
        || body["project_sha256"].as_str() != Some(sha_hex(&copy).as_str())
    {
        return Err(Fail(
            ErrorCode::Integrity,
            "the baseline was modified after capture; it is no longer trustworthy. Start a new rework session".into(),
            json!({"session_id": sid}),
        ));
    }
    Ok(body)
}

fn log(root: &Path, sid: &str, ctx: &CallContext, event: &str, data: Value) -> Result<(), Fail> {
    let mut line = json!({
        "ts_ms": store::now_ms() as u64,
        "session": ctx.session_id,
        "agent": ctx.agent,
        "event": event,
    });
    if let (Some(m), Value::Object(d)) = (line.as_object_mut(), data) {
        m.extend(d);
    }
    append_jsonl(&root.join(sid).join(EVENTS), &line)
}

fn protected_set(root: &Path, sid: &str, p: &Project) -> BTreeSet<String> {
    let mut s: BTreeSet<String> = p
        .pcb
        .footprints
        .values()
        .filter(|f| f.locked)
        .map(|f| f.id.clone())
        .collect();
    if let Ok(text) = std::fs::read_to_string(root.join(sid).join(PROTECTION)) {
        if let Ok(v) = serde_json::from_str::<Value>(&text) {
            for e in v["protected"].as_array().cloned().unwrap_or_default() {
                if let Some(id) = e["id"].as_str() {
                    s.insert(id.to_string());
                }
            }
        }
    }
    s
}

// ---------------------------------------------------------------- dispatch

/// Dispatch one `rework` tool against an open engine.
pub(crate) fn dispatch(
    engine: &mut Engine,
    ctx: &CallContext,
    tool: &str,
    args: &Value,
) -> Result<Value, Fail> {
    let root = root(engine)?;
    match tool {
        "rework_inspect" => inspect(engine, &root, args),
        "rework_baseline" => baseline(engine, ctx, &root, args),
        "rework_audit" => audit(engine, ctx, &root, args),
        "rework_protect" => protect(engine, ctx, &root, args),
        "rework_plan" => plan(engine, ctx, &root, args),
        "rework_apply" => apply(engine, ctx, &root, args),
        "rework_report" => report(engine, ctx, &root, args),
        other => Err(Fail(
            ErrorCode::UnknownTool,
            format!("no rework tool named {other}"),
            Value::Null,
        )),
    }
}

// ---------------------------------------------------------------- 1. inspect (read-only)

fn identity_check(p: &Project, args: &Value) -> Result<bool, Fail> {
    let mut provided = false;
    for (key, actual) in [("expect_project_id", &p.id), ("expect_name", &p.name)] {
        if let Some(want) = args.get(key).and_then(Value::as_str) {
            provided = true;
            if want != actual {
                return Err(Fail(
                    ErrorCode::ProjectMismatch,
                    format!(
                        "{key} does not match: the folder holds project {} ({})",
                        p.id, p.name
                    ),
                    json!({"expected": want, "actual": actual}),
                ));
            }
        }
    }
    Ok(provided)
}

fn inspect(engine: &Engine, root: &Path, args: &Value) -> Result<Value, Fail> {
    let p = engine.project();
    let confirmed = identity_check(p, args)?;
    let idx = index(p);
    let reports = all_reports(p);
    let checks: Vec<Value> = reports
        .iter()
        .map(|r| {
            json!({"check": r.check, "errors": r.errors, "warnings": r.warnings,
                   "outcome": r.outcome, "not_implemented": r.not_implemented.len()})
        })
        .collect();
    let conn = eak_engineering::copper::analyse(p);
    let committed_revisions = engine
        .dir()
        .map(|d| {
            store::read_journal(d)
                .iter()
                .filter(|e| e.state == EntryState::Committed)
                .count()
        })
        .unwrap_or(0);
    let known: Vec<Value> = sessions(root)
        .iter()
        .map(|s| {
            let d = root.join(s);
            json!({
                "session_id": s,
                "baseline": d.join(BASELINE).exists(),
                "audit": d.join(AUDIT).exists(),
                "plans": std::fs::read_dir(d.join(PLANS)).map(|r| r.count()).unwrap_or(0),
                "approvals": read_jsonl(&d.join(APPROVALS)).len(),
            })
        })
        .collect();
    Ok(json!({
        "read_only": true,
        "revision": p.revision,
        "identity": {"project_id": p.id, "name": p.name, "confirmed_by_caller": confirmed},
        "object_counts": kind_counts(&idx),
        "fingerprints": fingerprints(&idx),
        "checks": checks,
        "unrouted_connections": conn.ratsnest.len(),
        "shorts": conn.shorts.len(),
        "committed_revisions": committed_revisions,
        "locked_footprints": p.pcb.footprints.values().filter(|f| f.locked).count(),
        "sessions": known,
        "next": "rework_baseline {confirm_project_id: <project id above>}",
        "note": "nothing was written. The design is not rebuilt: capture a baseline, audit, then plan surgical repairs",
    }))
}

// ---------------------------------------------------------------- 2. baseline

fn baseline(engine: &Engine, ctx: &CallContext, root: &Path, args: &Value) -> Result<Value, Fail> {
    let p = engine.project();
    let confirm = arg_str(args, "confirm_project_id")?;
    if confirm != p.id {
        return Err(Fail(
            ErrorCode::ProjectMismatch,
            format!(
                "confirm_project_id does not match the project in this folder ({})",
                p.id
            ),
            json!({"expected": confirm, "actual": p.id}),
        ));
    }
    let idx = index(p);
    let sid = format!("rw-{:04}", sessions(root).len() + 1);
    let dir = root.join(&sid);
    if dir.exists() {
        return Err(Fail(
            ErrorCode::Precondition,
            format!("{sid} already exists"),
            Value::Null,
        ));
    }
    std::fs::create_dir_all(&dir).map_err(storage)?;
    let copy = eak_core::to_json(p);
    store::atomic_write(&dir.join(BASELINE_PROJECT), copy.as_bytes()).map_err(storage)?;
    let objects: BTreeMap<&String, Value> = idx
        .iter()
        .map(|(id, o)| (id, json!({"kind": o.kind, "hash": o.hash})))
        .collect();
    let body = json!({
        "session_id": sid,
        "project_id": p.id,
        "name": p.name,
        "revision": p.revision,
        "captured_at_ms": store::now_ms() as u64,
        "captured_by": {"agent": ctx.agent, "session": ctx.session_id},
        "fingerprints": fingerprints(&idx),
        "counts": kind_counts(&idx),
        "project_sha256": sha_hex(copy.as_bytes()),
        "objects": objects,
    });
    let doc = json!({"body": body, "fingerprint": fingerprint(&body)});
    let path = dir.join(BASELINE);
    write_json(&path, &doc)?;
    // Immutability is best effort at the file-system level and verified on every load.
    for f in [path.clone(), dir.join(BASELINE_PROJECT)] {
        if let Ok(meta) = std::fs::metadata(&f) {
            let mut perm = meta.permissions();
            perm.set_readonly(true);
            let _ = std::fs::set_permissions(&f, perm);
        }
    }
    log(root, &sid, ctx, "baseline", json!({"revision": p.revision}))?;
    Ok(json!({
        "session_id": sid,
        "baseline_revision": p.revision,
        "baseline_fingerprint": doc["fingerprint"],
        "fingerprints": body["fingerprints"],
        "counts": body["counts"],
        "immutable": "the baseline is never rewritten; every later tool verifies its fingerprint",
        "next": "rework_audit",
    }))
}

// ---------------------------------------------------------------- 3. audit

fn category_of(check: &str, rule: &str) -> &'static str {
    match check.to_ascii_lowercase().as_str() {
        "erc" => "schematic",
        "mech" => "placement",
        "dfm" => "drc",
        _ => match rule {
            "DRC-003" | "DRC-004" => "routing",
            "DRC-013" => "copper",
            _ => "drc",
        },
    }
}

/// Issue inventory of the current design: eak-validation findings plus an eak-engineering
/// recompute of current-based track sizing. Deterministic; sorted by dependency order.
fn collect_issues(p: &Project, idx: &Index) -> Vec<Value> {
    let mut out: Vec<Value> = vec![];
    for r in all_reports(p) {
        for f in &r.findings {
            let sev = match f.severity {
                eak_validation::Severity::Error if f.rule_id == "DRC-003" => "critical",
                eak_validation::Severity::Error => "high",
                eak_validation::Severity::Warning => "medium",
                eak_validation::Severity::Info => continue,
            };
            let nets: BTreeSet<String> = f
                .objects
                .iter()
                .filter_map(|id| idx.get(id).and_then(|o| o.net.clone()))
                .collect();
            let id = if f.id.is_empty() {
                format!(
                    "RW-{}",
                    &fingerprint(&json!([r.check, f.rule_id, f.message]))[..10]
                )
            } else {
                f.id.clone()
            };
            out.push(json!({
                "issue_id": id,
                "category": category_of(&r.check, &f.rule_id),
                "severity": sev,
                "title": format!("{} {}", f.rule_id, r.check),
                "description": f.message,
                "source": {"check": r.check, "rule_id": f.rule_id},
                "affected": {"ids": f.objects, "nets": nets},
                "suggested_fix": f.suggested_fix,
                "evidence": "deterministic (eak-validation)",
            }));
        }
    }
    // Current sizing recomputed by eak-engineering, independent of DRC.
    let oz = p
        .pcb
        .board
        .stackup
        .copper_oz(eak_core::CopperLayer::Top)
        .unwrap_or(1.0);
    for (net, amps) in &p.constraints.net_currents_a {
        if *amps <= 0.0 {
            continue;
        }
        let Ok(mm) =
            eak_engineering::ipc::trace_width_mm(*amps, false, oz, p.constraints.temp_rise_c)
        else {
            continue;
        };
        let need = (mm * 1e6).ceil() as i64;
        let narrow: Vec<&String> = p
            .pcb
            .tracks
            .values()
            .filter(|t| &t.net == net && t.width < need)
            .map(|t| &t.id)
            .collect();
        if !narrow.is_empty() {
            out.push(json!({
                "issue_id": format!("PW-{}", &fingerprint(&json!(["power", net, need]))[..10]),
                "category": "power_thermal",
                "severity": "high",
                "title": format!("{} track(s) on {net} narrower than {mm:.3} mm for {amps} A", narrow.len()),
                "description": format!("IPC-2221 external, {oz} oz, {} C rise needs {mm:.3} mm; narrower tracks exceed the declared current", p.constraints.temp_rise_c),
                "source": {"check": "eak-engineering", "rule_id": "IPC-2221-WIDTH"},
                "affected": {"ids": narrow, "nets": [net]},
                "suggested_fix": "widen or re-route the named tracks",
                "evidence": "deterministic (eak-engineering recompute)",
            }));
        }
    }
    out.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                category_rank(v["category"].as_str().unwrap_or("")),
                severity_rank(v["severity"].as_str().unwrap_or("")),
                v["issue_id"].as_str().unwrap_or("").to_string(),
            )
        };
        key(a).cmp(&key(b))
    });
    out.dedup_by(|a, b| a["issue_id"] == b["issue_id"]);
    out
}

fn audit(engine: &Engine, ctx: &CallContext, root: &Path, args: &Value) -> Result<Value, Fail> {
    let sid = session_of(root, args)?;
    let base = load_baseline(root, &sid)?;
    let p = engine.project();
    let idx = index(p);
    let issues = collect_issues(p, &idx);
    let mut by_cat: BTreeMap<&str, usize> = BTreeMap::new();
    for i in &issues {
        *by_cat
            .entry(i["category"].as_str().unwrap_or(""))
            .or_insert(0) += 1;
    }
    let doc = json!({
        "session_id": sid,
        "revision": p.revision,
        "baseline_revision": base["revision"],
        "design_changed_since_baseline": base["revision"].as_u64() != Some(p.revision),
        "issue_count": issues.len(),
        "by_category": by_cat,
        "issues": issues,
        "classification": "deterministic findings only; issues eak-validation does not implement are not listed (see rework_report not_implemented)",
    });
    let dir = root.join(&sid);
    write_json(
        &dir.join(AUDITS).join(format!("rev-{}.json", p.revision)),
        &doc,
    )?;
    if !dir.join(AUDIT).exists() {
        write_json(&dir.join(AUDIT), &doc)?;
    }
    log(
        root,
        &sid,
        ctx,
        "audit",
        json!({"revision": p.revision, "issues": doc["issue_count"]}),
    )?;
    let mut out = doc;
    out["next"] = json!("rework_protect (optional), then rework_plan with one repair per issue");
    Ok(out)
}

/// Find an issue by id in the newest audit that lists it.
fn find_issue(root: &Path, sid: &str, id: &str) -> Option<Value> {
    let dir = root.join(sid).join(AUDITS);
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    files.sort();
    for f in files.iter().rev() {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()?;
        if let Some(i) = v["issues"]
            .as_array()
            .and_then(|a| a.iter().find(|i| i["issue_id"] == json!(id)))
        {
            return Some(i.clone());
        }
    }
    None
}

// ---------------------------------------------------------------- 4. protect

fn protect(engine: &Engine, ctx: &CallContext, root: &Path, args: &Value) -> Result<Value, Fail> {
    let sid = session_of(root, args)?;
    load_baseline(root, &sid)?;
    let reason = arg_str(args, "reason")?;
    let ids: Vec<String> = args
        .get("ids")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Err(invalid("ids must be a non-empty array of object ids"));
    }
    let p = engine.project();
    let idx = index(p);
    let unknown: Vec<&String> = ids.iter().filter(|i| !idx.contains_key(*i)).collect();
    if !unknown.is_empty() {
        return Err(Fail(
            ErrorCode::NotFound,
            "some ids are not objects of this project; nothing was stored".into(),
            json!({"unknown": unknown}),
        ));
    }
    let path = root.join(&sid).join(PROTECTION);
    let mut doc: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({"protected": []}));
    let have: BTreeSet<String> = doc["protected"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| e["id"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let list = doc["protected"].as_array_mut().expect("protected list");
    for id in ids.iter().filter(|i| !have.contains(*i)) {
        list.push(json!({
            "id": id, "reason": reason, "revision": p.revision,
            "by": {"agent": ctx.agent, "session": ctx.session_id},
            "ts_ms": store::now_ms() as u64,
        }));
    }
    write_json(&path, &doc)?;
    log(root, &sid, ctx, "protect", json!({"ids": ids}))?;
    Ok(json!({
        "session_id": sid,
        "protected_total": protected_set(root, &sid, p).len(),
        "note": "protection is add-only: no tool releases it (locked footprints are always protected). A human who must release an object edits the record or starts a new session",
    }))
}

// ---------------------------------------------------------------- 5. plan

fn collect_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| collect_strings(x, out)),
        Value::Object(m) => m.values().for_each(|x| collect_strings(x, out)),
        _ => {}
    }
}

/// Commands that change an existing object (as opposed to merely referencing one).
fn mutates_existing(ty: &str) -> bool {
    [
        "Delete", "Move", "Rotate", "Flip", "Set", "Upsert", "Assign", "Clear",
        // EditTrack and EditZone change an existing track or zone (ADR-0042).
        "Edit",
    ]
    .iter()
    .any(|p| ty.starts_with(p))
}

type Touches = (BTreeSet<String>, BTreeSet<String>);

/// Objects a step touches: `(strict, referenced)`. `Err` for commands that are not surgical.
fn step_touches(step: &Value, idx: &Index) -> Result<Touches, String> {
    let ty = step["type"].as_str().unwrap_or("");
    let missing = |k: &str| step.get(k).is_none_or(Value::is_null);
    match ty {
        "AutoPlace" => return Err("AutoPlace repositions every unlocked footprint; rework repairs must be surgical (use MoveFootprint on named footprints)".into()),
        "AutoRoute" if missing("nets") => return Err("AutoRoute needs `nets`: rework routes only the nets an issue names".into()),
        "ClearRouting" if missing("net") => return Err("ClearRouting needs a `net`".into()),
        "StitchZones" if missing("net") => return Err("StitchZones needs a `net`: rework stitches only the net an issue names".into()),
        _ => {}
    }
    let mut hit = BTreeSet::new();
    let mut strings = vec![];
    collect_strings(step, &mut strings);
    for s in strings {
        if idx.contains_key(&s) {
            hit.insert(s);
        }
    }
    match ty {
        "SetBoardOutline" | "SetStackup" | "SetLayerCount" => {
            hit.insert("@board".into());
        }
        "SetDesignRules"
        | "UpsertNetClass"
        | "SetNetCurrent"
        | "SetNetVoltage"
        | "SetRuleConfig"
        | "SetClearanceCell"
        | "SetNetPairClearance" => {
            hit.insert("@constraints".into());
        }
        "SetManufacturing" => {
            hit.insert("@manufacturing".into());
        }
        "ClearRouting" => {
            let net = step["net"].as_str().unwrap_or("");
            for (id, o) in idx {
                if o.net.as_deref() == Some(net) {
                    hit.insert(id.clone());
                }
            }
        }
        _ => {}
    }
    if mutates_existing(ty) {
        Ok((hit, BTreeSet::new()))
    } else {
        Ok((BTreeSet::new(), hit))
    }
}

/// Objects a repair for `issue` may modify: the issue's objects and the copper of its nets.
fn scope_of(issue: &Value, idx: &Index) -> BTreeSet<String> {
    let mut s: BTreeSet<String> = issue["affected"]["ids"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let nets: BTreeSet<&str> = issue["affected"]["nets"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    for (id, o) in idx {
        if o.net.as_deref().is_some_and(|n| nets.contains(n)) {
            s.insert(id.clone());
        }
    }
    s
}

struct Rep {
    issue: Value,
    description: String,
    steps: Vec<Value>,
    strict: BTreeSet<String>,
    refs: BTreeSet<String>,
    scope: BTreeSet<String>,
    class: OperationClass,
}

fn plan(engine: &Engine, ctx: &CallContext, root: &Path, args: &Value) -> Result<Value, Fail> {
    let sid = session_of(root, args)?;
    let base = load_baseline(root, &sid)?;
    let title = arg_str(args, "title")?;
    let repairs_in = args
        .get("repairs")
        .and_then(Value::as_array)
        .filter(|r| !r.is_empty() && r.len() <= 20)
        .ok_or_else(|| invalid("repairs must be an array of 1..20 repairs"))?;
    let p = engine.project();
    let idx = index(p);
    let protected = protected_set(root, &sid, p);

    let mut reps: Vec<Rep> = vec![];
    let mut problems: Vec<Value> = vec![];
    for (n, r) in repairs_in.iter().enumerate() {
        let issue_id = r["issue_id"].as_str().unwrap_or("");
        let Some(issue) = find_issue(root, &sid, issue_id) else {
            problems.push(json!({"repair": n, "error": format!("issue_id {issue_id:?} is in no audit of this session: run rework_audit")}));
            continue;
        };
        let steps: Vec<Value> = r["steps"].as_array().cloned().unwrap_or_default();
        if steps.is_empty() || steps.len() > MAX_BATCH {
            problems.push(json!({"repair": n, "error": format!("steps must hold 1..{MAX_BATCH} typed commands")}));
            continue;
        }
        let scope = scope_of(&issue, &idx);
        let (mut strict, mut refs) = (BTreeSet::new(), BTreeSet::new());
        let mut class = OperationClass::ReadOnly;
        let mut ok = true;
        for (si, s) in steps.iter().enumerate() {
            match decode_step(s) {
                Ok((_, c)) => class = class.max(c.unwrap_or(OperationClass::EngineeringImpacting)),
                Err(e) => {
                    problems.push(json!({"repair": n, "step": si, "error": e}));
                    ok = false;
                    continue;
                }
            }
            match step_touches(s, &idx) {
                Ok((st, rf)) => {
                    strict.extend(st);
                    refs.extend(rf);
                }
                Err(e) => {
                    problems.push(json!({"repair": n, "step": si, "error": e}));
                    ok = false;
                }
            }
        }
        let touched_protected: Vec<&String> =
            strict.iter().filter(|i| protected.contains(*i)).collect();
        if !touched_protected.is_empty() {
            problems.push(json!({"repair": n, "error": "a step changes PROTECTED objects", "protected": touched_protected}));
            ok = false;
        }
        let out_of_scope: Vec<&String> = strict.iter().filter(|i| !scope.contains(*i)).collect();
        if !out_of_scope.is_empty() {
            problems.push(json!({"repair": n, "error": "a step changes ACCEPTED objects outside the issue's scope; choose a narrower step", "out_of_scope": out_of_scope}));
            ok = false;
        }
        if ok {
            reps.push(Rep {
                issue,
                description: r["description"].as_str().unwrap_or("").to_string(),
                steps,
                strict,
                refs,
                scope,
                class,
            });
        }
    }
    if !problems.is_empty() {
        return Err(Fail(
            ErrorCode::InvalidArguments,
            "the plan was refused and nothing was stored".into(),
            json!({"problems": problems}),
        ));
    }
    reps.sort_by_key(|r| {
        (
            category_rank(r.issue["category"].as_str().unwrap_or("")),
            severity_rank(r.issue["severity"].as_str().unwrap_or("")),
        )
    });
    let mut last_by_cat: BTreeMap<String, String> = BTreeMap::new();
    let mut repairs_out = vec![];
    let mut allowed: BTreeSet<String> = BTreeSet::new();
    let mut max_class = OperationClass::ReadOnly;
    for (i, r) in reps.iter().enumerate() {
        let rid = format!("RP-{:02}", i + 1);
        let cat = r.issue["category"].as_str().unwrap_or("").to_string();
        let rank = category_rank(&cat);
        let mut deps: Vec<String> = vec![];
        if rank > 1 {
            deps.extend(last_by_cat.get("schematic").cloned());
        }
        if rank > 3 {
            deps.extend(last_by_cat.get("placement").cloned());
        }
        allowed.extend(r.scope.iter().cloned());
        max_class = max_class.max(r.class);
        repairs_out.push(json!({
            "repair_id": rid,
            "issue_id": r.issue["issue_id"],
            "category": cat,
            "severity": r.issue["severity"],
            "description": r.description,
            "depends_on": deps,
            "changes": r.strict,
            "references": r.refs,
            "steps": r.steps,
            "max_class": r.class,
            "verification": "readback of the revision, re-hash of every PROTECTED/ACCEPTED object, then eak-validation checks",
        }));
        last_by_cat.insert(cat, rid);
    }
    let n = std::fs::read_dir(root.join(&sid).join(PLANS))
        .map(|rd| rd.count())
        .unwrap_or(0);
    let plan_id = format!("{sid}-p{:02}", n + 1);
    let body = json!({
        "plan_id": plan_id,
        "session_id": sid,
        "title": title,
        "base_revision": p.revision,
        "baseline_fingerprint": fingerprint(&base),
        "created_by": {"agent": ctx.agent, "session": ctx.session_id},
        "repairs": repairs_out,
        "allowed": allowed,
        "max_class": max_class,
        "needs_destructive_opt_in": max_class > ctx.max_class,
        "protected_count": protected.len(),
    });
    let doc = json!({"body": body, "fingerprint": fingerprint(&body)});
    write_json(
        &root.join(&sid).join(PLANS).join(format!("{plan_id}.json")),
        &doc,
    )?;
    log(
        root,
        &sid,
        ctx,
        "plan",
        json!({"plan_id": plan_id, "repairs": reps.len()}),
    )?;
    Ok(json!({
        "plan_id": plan_id,
        "fingerprint": doc["fingerprint"],
        "base_revision": p.revision,
        "repairs": body["repairs"],
        "needs_destructive_opt_in": body["needs_destructive_opt_in"],
        "status": "pending_human_approval",
        "next": format!("a human runs `eak rework approve {plan_id} <fingerprint>` in an interactive terminal; then rework_apply {{plan_id, expected_revision: {}}}", p.revision),
    }))
}

// ---------------------------------------------------------------- 6. approval (CLI only)

fn session_from_plan(plan_id: &str) -> Option<&str> {
    plan_id
        .rsplit_once("-p")
        .map(|(s, _)| s)
        .filter(|s| safe_id(s))
}

/// `eak rework approve <PLAN_ID> <FINGERPRINT>`: a human approves one plan. Refused unless stdin is
/// an interactive terminal and the typed phrase matches; agents running commands through pipes,
/// MCP or `eak call` cannot pass this gate. Returns the process exit code.
pub fn approve_cli(ctx: &CallContext, plan_id: &str, fp: &str) -> i32 {
    use std::io::{BufRead, IsTerminal, Write};
    let Some(dir) = ctx.project.dir.as_ref() else {
        eprintln!("eak rework approve: no project (pass --project)");
        return 2;
    };
    if !std::io::stdin().is_terminal() {
        eprintln!("eak rework approve: needs an interactive terminal; approval is a human step and is not available to agents, MCP or `eak call`");
        return 1;
    }
    let Some(sid) = session_from_plan(plan_id) else {
        eprintln!("eak rework approve: {plan_id} is not a plan id");
        return 1;
    };
    let sdir = dir.join(REWORK_DIR).join(sid);
    let Some(doc) = std::fs::read_to_string(sdir.join(PLANS).join(format!("{plan_id}.json")))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        eprintln!("eak rework approve: no plan {plan_id}");
        return 1;
    };
    if doc["fingerprint"].as_str() != Some(fp) || fingerprint(&doc["body"]) != fp {
        eprintln!(
            "eak rework approve: fingerprint does not match the stored plan; re-read the plan"
        );
        return 1;
    }
    let body = &doc["body"];
    println!(
        "Plan {plan_id}: {}\nBase revision {}. {} repair(s):",
        body["title"].as_str().unwrap_or(""),
        body["base_revision"],
        body["repairs"].as_array().map_or(0, Vec::len)
    );
    for r in body["repairs"].as_array().cloned().unwrap_or_default() {
        println!(
            "  {} [{}] {}: {}\n    changes: {}  ({} step(s))",
            r["repair_id"].as_str().unwrap_or(""),
            r["severity"].as_str().unwrap_or(""),
            r["issue_id"].as_str().unwrap_or(""),
            r["description"].as_str().unwrap_or(""),
            r["changes"],
            r["steps"].as_array().map_or(0, Vec::len)
        );
    }
    println!("Everything else in the project is preserved and verified after each repair. This approves the changes above only; it does not make the design manufacturing-ready.");
    let phrase = format!("approve {plan_id}");
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
        "plan_id": plan_id,
        "fingerprint": fp,
        "base_revision": body["base_revision"],
        "approved_by": format!("human:{who}"),
        "via": "eak rework approve (interactive terminal)",
    });
    match append_jsonl(&sdir.join(APPROVALS), &entry) {
        Ok(()) => {
            println!("approved; the agent can now call rework_apply");
            0
        }
        Err(Fail(_, m, _)) => {
            eprintln!("eak rework approve: {m}");
            1
        }
    }
}

// ---------------------------------------------------------------- 7. apply

/// Baseline objects that must not change (PROTECTED, or ACCEPTED = outside `allowed`) but did.
fn violations(
    pre: &Index,
    post: &Index,
    allowed: &BTreeSet<String>,
    protected: &BTreeSet<String>,
) -> Vec<Value> {
    let mut out = vec![];
    for (id, o) in pre {
        if allowed.contains(id) && !protected.contains(id) {
            continue;
        }
        match post.get(id) {
            None => out.push(json!({"id": id, "kind": o.kind, "what": "removed"})),
            Some(n) if n.hash != o.hash => {
                out.push(json!({"id": id, "kind": o.kind, "what": "changed"}))
            }
            _ => {}
        }
    }
    out
}

fn load_plan(root: &Path, plan_id: &str) -> Result<(String, Value, Value), Fail> {
    let sid = session_from_plan(plan_id)
        .ok_or_else(|| invalid("plan_id must be an id returned by rework_plan"))?
        .to_string();
    let text = std::fs::read_to_string(root.join(&sid).join(PLANS).join(format!("{plan_id}.json")))
        .map_err(|_| {
            Fail(
                ErrorCode::NotFound,
                format!("no plan {plan_id}"),
                Value::Null,
            )
        })?;
    let doc: Value = serde_json::from_str(&text).map_err(storage)?;
    if doc["fingerprint"].as_str() != Some(fingerprint(&doc["body"]).as_str()) {
        return Err(Fail(
            ErrorCode::Integrity,
            format!("plan {plan_id} was modified after it was made; re-plan"),
            Value::Null,
        ));
    }
    Ok((sid, doc["body"].clone(), doc))
}

fn apply(engine: &mut Engine, ctx: &CallContext, root: &Path, args: &Value) -> Result<Value, Fail> {
    let plan_id = arg_str(args, "plan_id")?.to_string();
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
    let (sid, body, doc) = load_plan(root, &plan_id)?;
    load_baseline(root, &sid)?;
    let base_rev = body["base_revision"].as_u64().unwrap_or(u64::MAX);
    if base_rev != expected || engine.project().revision != expected {
        return Err(Fail(
            ErrorCode::StaleRevision,
            format!("plan {plan_id} was made at revision {base_rev} and the project is at {}; re-audit and re-plan", engine.project().revision),
            json!({"plan_base_revision": base_rev, "expected_revision": expected, "current_revision": engine.project().revision}),
        ));
    }
    let fp = doc["fingerprint"].as_str().unwrap_or("");
    let approved = read_jsonl(&root.join(&sid).join(APPROVALS))
        .iter()
        .any(|a| {
            a["plan_id"] == json!(plan_id)
                && a["fingerprint"] == json!(fp)
                && a["base_revision"].as_u64() == Some(base_rev)
        });
    if !approved {
        return Err(Fail(
            ErrorCode::ApprovalRequired,
            "this plan has no human approval: a human must run `eak rework approve` in an interactive terminal".into(),
            json!({"plan_id": plan_id, "fingerprint": fp, "command": format!("eak rework approve {plan_id} {fp}")}),
        ));
    }
    if body["needs_destructive_opt_in"] == json!(true) {
        return Err(Fail(
            ErrorCode::ApprovalRequired,
            "the plan contains commands above this session's operation class (destructive opt-in needed when the session was started)".into(),
            Value::Null,
        ));
    }
    let allowed: BTreeSet<String> = body["allowed"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let protected = protected_set(root, &sid, engine.project());
    let mut applied: Vec<Value> = vec![];
    for repair in body["repairs"].as_array().cloned().unwrap_or_default() {
        let rid = repair["repair_id"].as_str().unwrap_or("").to_string();
        let pre = index(engine.project());
        let mut commands: Vec<Command> = vec![];
        for s in repair["steps"].as_array().cloned().unwrap_or_default() {
            commands.push(decode_step(&s).map_err(invalid)?.0);
        }
        let command = if commands.len() == 1 {
            commands.remove(0)
        } else {
            Command::Batch {
                commands,
                label: Some(format!("{plan_id} {rid}")),
            }
        };
        let before = engine.project().revision;
        // ADR-0036: rework repairs obey the same ERC gate before PCB synchronisation.
        if crate::tools::carries_pcb_sync(&command) {
            crate::agent_task::pcb_sync_gate(engine)
                .map_err(|why| Fail(ErrorCode::Precondition, why, json!({"gate": "pcb_sync"})))?;
        }
        let resp = engine.execute(CommandRequest {
            expected_revision: before,
            actor: Actor::agent_in_session(&ctx.agent, &ctx.session_id),
            command,
            approval: Some(Approval {
                approved_by: format!("harness-session:{}", ctx.session_id),
                max_class: ctx.max_class,
                note: format!("rework plan {plan_id} approved by a human via eak rework approve"),
            }),
        });
        log(
            root,
            &sid,
            ctx,
            "apply_repair",
            json!({"plan_id": plan_id, "repair_id": rid, "op_id": resp.op_id,
                   "ok": resp.error.is_none(), "revision_before": resp.revision_before,
                   "revision_after": resp.revision_after}),
        )?;
        if let Some(e) = &resp.error {
            let mut f = command_error(e);
            f.2 = json!({"repair_id": rid, "applied_before": applied, "stopped": true});
            return Err(f);
        }
        // Readback: the revision moved by exactly one and nothing outside scope changed.
        let post = index(engine.project());
        let bad = violations(&pre, &post, &allowed, &protected);
        let rev_ok =
            engine.project().revision == resp.revision_after && resp.revision_after == before + 1;
        if !bad.is_empty() || !rev_ok {
            let undo = engine.undo(
                resp.revision_after,
                &Actor::agent_in_session(&ctx.agent, &ctx.session_id),
            );
            log(
                root,
                &sid,
                ctx,
                "undo",
                json!({"plan_id": plan_id, "repair_id": rid, "op_id": undo.op_id,
                       "ok": undo.error.is_none(), "reason": "readback failed"}),
            )?;
            return Err(Fail(
                ErrorCode::Integrity,
                format!("readback of {rid} failed; the repair was undone and the plan stopped"),
                json!({
                    "repair_id": rid, "violations": bad, "revision_as_expected": rev_ok,
                    "undone": undo.error.is_none(),
                    "revision": engine.project().revision,
                    "applied_before": applied,
                }),
            ));
        }
        applied.push(json!({
            "repair_id": rid, "issue_id": repair["issue_id"], "op_id": resp.op_id,
            "revision_before": resp.revision_before, "revision_after": resp.revision_after,
            "created": resp.created,
            "readback": {"revision_ok": true, "protected_and_accepted_objects_unchanged": true},
        }));
    }
    log(
        root,
        &sid,
        ctx,
        "apply_complete",
        json!({"plan_id": plan_id}),
    )?;
    let checks = run_checks(engine, "all")?;
    Ok(json!({
        "plan_id": plan_id,
        "applied": applied,
        "revision": engine.project().revision,
        "checks": {"errors": checks["errors"], "warnings": checks["warnings"]},
        "next": "rework_report (baseline diff, issue resolution, readiness)",
        "note": "post-mutation validation ran; fix or re-plan anything still open. This is not a manufacturing-readiness claim",
    }))
}

// ---------------------------------------------------------------- 8. report

fn report(engine: &Engine, ctx: &CallContext, root: &Path, args: &Value) -> Result<Value, Fail> {
    let sid = session_of(root, args)?;
    let base = load_baseline(root, &sid)?;
    let sdir = root.join(&sid);
    let p = engine.project();
    let idx = index(p);
    let base_objects = base["objects"].as_object().cloned().unwrap_or_default();

    let events = read_jsonl(&sdir.join(EVENTS));
    let mut own_ops: BTreeSet<String> = BTreeSet::new();
    let mut applied_plans: BTreeSet<String> = BTreeSet::new();
    // Scope authorised by plans with at least one executed repair; anything changed outside
    // every such scope is reported as outside approved scope.
    let mut authorised: BTreeSet<String> = BTreeSet::new();
    for e in &events {
        if matches!(e["event"].as_str(), Some("apply_repair" | "undo")) {
            if let Some(op) = e["op_id"].as_str() {
                own_ops.insert(op.to_string());
            }
        }
        if e["event"] == json!("apply_complete") {
            if let Some(pl) = e["plan_id"].as_str() {
                applied_plans.insert(pl.to_string());
            }
        }
        if e["event"] == json!("apply_repair") {
            if let Some(pl) = e["plan_id"].as_str() {
                if let Ok((_, b, _)) = load_plan(root, pl) {
                    for a in b["allowed"].as_array().cloned().unwrap_or_default() {
                        if let Some(s) = a.as_str() {
                            authorised.insert(s.to_string());
                        }
                    }
                }
            }
        }
    }
    let protected = protected_set(root, &sid, p);

    let (mut added, mut removed, mut changed) = (vec![], vec![], vec![]);
    for (id, o) in &base_objects {
        match idx.get(id) {
            None => removed.push(id.clone()),
            Some(n) if Some(n.hash.as_str()) != o["hash"].as_str() => changed.push(id.clone()),
            _ => {}
        }
    }
    for id in idx.keys() {
        if !base_objects.contains_key(id) {
            added.push(id.clone());
        }
    }
    let touched: Vec<&String> = removed.iter().chain(changed.iter()).collect();
    let protected_changed: Vec<&String> = touched
        .iter()
        .copied()
        .filter(|i| protected.contains(*i))
        .collect();
    let outside_scope: Vec<&String> = touched
        .iter()
        .copied()
        .filter(|i| !authorised.contains(*i))
        .collect();
    let base_revision = base["revision"].as_u64().unwrap_or(0);
    let unattributed: Vec<Value> = engine
        .dir()
        .map(|d| {
            let journal = store::read_journal(d);
            let committed: BTreeSet<&str> = journal
                .iter()
                .filter(|e| e.state == EntryState::Committed)
                .map(|e| e.op_id.as_str())
                .collect();
            journal
                .iter()
                .filter(|e| {
                    e.state == EntryState::Pending
                        && committed.contains(e.op_id.as_str())
                        && e.revision_after > base_revision
                        && !own_ops.contains(&e.op_id)
                })
                .map(|e| json!({"op_id": e.op_id, "revision_after": e.revision_after, "actor": e.actor, "command": e.command}))
                .collect()
        })
        .unwrap_or_default();

    // Issue resolution against the first audit.
    let now = collect_issues(p, &idx);
    let first: Option<Value> = std::fs::read_to_string(sdir.join(AUDIT))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok());
    let first_ids: BTreeSet<String> = first
        .as_ref()
        .and_then(|a| a["issues"].as_array())
        .map(|a| {
            a.iter()
                .filter_map(|i| i["issue_id"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let now_ids: BTreeSet<String> = now
        .iter()
        .filter_map(|i| i["issue_id"].as_str().map(String::from))
        .collect();
    let resolved: Vec<&String> = first_ids.difference(&now_ids).collect();
    let remaining: Vec<&String> = first_ids.intersection(&now_ids).collect();
    let new_issues: Vec<&Value> = now
        .iter()
        .filter(|i| !first_ids.contains(i["issue_id"].as_str().unwrap_or("")))
        .collect();
    let open_errors = now
        .iter()
        .filter(|i| matches!(i["severity"].as_str(), Some("critical" | "high")))
        .count();

    let reports = all_reports(p);
    let mut not_impl: BTreeSet<String> = BTreeSet::new();
    let mut limits: BTreeSet<String> = BTreeSet::new();
    for r in &reports {
        not_impl.extend(r.not_implemented.iter().cloned());
        limits.extend(r.limitations.iter().cloned());
    }
    let checks: Vec<Value> = reports
        .iter()
        .map(|r| json!({"check": r.check, "errors": r.errors, "warnings": r.warnings, "outcome": r.outcome}))
        .collect();
    let approvals = read_jsonl(&sdir.join(APPROVALS));

    let (label, why) = if !protected_changed.is_empty() {
        (
            "integrity_violation",
            "PROTECTED objects differ from the baseline",
        )
    } else if !outside_scope.is_empty() || !unattributed.is_empty() {
        (
            "changes_outside_rework",
            "the design changed in ways no approved rework plan covers; review them",
        )
    } else if first.is_none() {
        (
            "not_audited",
            "no audit was recorded, so issue resolution cannot be judged",
        )
    } else if open_errors > 0 {
        ("repairs_incomplete", "high or critical issues remain")
    } else {
        (
            "passed_deterministic_checks",
            "no high/critical issue remains in the implemented checks; preserved objects are unchanged",
        )
    };
    let body = json!({
        "session_id": sid,
        "revision": p.revision,
        "baseline_revision": base["revision"],
        "baseline_fingerprint": base["fingerprints"]["composite"],
        "current_fingerprint": fingerprints(&idx)["composite"],
        "readiness": {"label": label, "reason": why,
            "never_claimed": "manufacturing-ready (a human signs off after review; see not_implemented)"},
        "diff_vs_baseline": {
            "added": added.len(), "removed": removed.len(), "changed": changed.len(),
            "removed_ids": removed.iter().take(50).collect::<Vec<_>>(),
            "changed_ids": changed.iter().take(50).collect::<Vec<_>>(),
        },
        "protected_objects": {"count": protected.len(), "changed": protected_changed},
        "changes_outside_approved_scope": outside_scope,
        "unattributed_revisions": unattributed,
        "issues": {"at_baseline": first_ids.len(), "resolved": resolved, "remaining": remaining,
                   "new": new_issues, "open_high_or_critical": open_errors},
        "plans_applied": applied_plans,
        "human_approvals": approvals,
        "checks": checks,
        "not_implemented": not_impl,
        "limitations": limits,
    });
    let doc = json!({"body": body, "fingerprint": fingerprint(&body)});
    write_json(
        &sdir.join(REPORTS).join(format!("rev-{}.json", p.revision)),
        &doc,
    )?;
    log(
        root,
        &sid,
        ctx,
        "report",
        json!({"revision": p.revision, "label": label}),
    )?;
    Ok(doc)
}

// Written without running (no local builds/tests in this phase): unrun.
#[cfg(test)]
mod tests {
    use super::*;

    fn obj(kind: &'static str, hash: &str) -> Obj {
        Obj {
            kind,
            hash: hash.into(),
            net: None,
        }
    }

    #[test]
    fn plan_ids_map_to_sessions() {
        assert_eq!(session_from_plan("rw-0001-p03"), Some("rw-0001"));
        assert_eq!(session_from_plan("../x-p01"), None);
    }

    #[test]
    fn unrelated_changes_are_violations_and_scoped_ones_are_not() {
        let pre: Index = [
            ("a", obj("track", "1")),
            ("b", obj("track", "1")),
            ("c", obj("via", "1")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let mut post = pre.clone();
        post.insert("a".into(), obj("track", "2")); // in scope
        post.insert("b".into(), obj("track", "2")); // out of scope
        post.remove("c"); // out of scope, removed
        let allowed: BTreeSet<String> = ["a".to_string()].into();
        let v = violations(&pre, &post, &allowed, &BTreeSet::new());
        let ids: Vec<&str> = v.iter().filter_map(|x| x["id"].as_str()).collect();
        assert_eq!(ids, ["b", "c"]);
        // Protection beats scope.
        let protected: BTreeSet<String> = ["a".to_string()].into();
        assert_eq!(violations(&pre, &post, &allowed, &protected).len(), 3);
    }

    #[test]
    fn surgical_steps_only() {
        let idx = Index::new();
        assert!(step_touches(&json!({"type": "AutoPlace"}), &idx).is_err());
        assert!(step_touches(&json!({"type": "AutoRoute", "nets": null}), &idx).is_err());
        assert!(step_touches(&json!({"type": "ClearRouting"}), &idx).is_err());
        assert!(step_touches(&json!({"type": "AutoRoute", "nets": ["VCC"]}), &idx).is_ok());
    }

    #[test]
    fn mutation_vs_reference_is_split() {
        let mut idx = Index::new();
        idx.insert("trk-1".into(), obj("track", "1"));
        idx.insert("sym-1".into(), obj("symbol", "1"));
        let (strict, refs) =
            step_touches(&json!({"type": "DeleteTrack", "id": "trk-1"}), &idx).unwrap();
        assert!(strict.contains("trk-1") && refs.is_empty());
        let (strict, refs) =
            step_touches(&json!({"type": "ConnectPins", "from": "sym-1"}), &idx).unwrap();
        assert!(strict.is_empty() && refs.contains("sym-1"));
        let (strict, _) =
            step_touches(&json!({"type": "SetNetCurrent", "net": "X"}), &idx).unwrap();
        assert!(strict.contains("@constraints"));
    }

    #[test]
    fn categories_follow_dependency_order() {
        assert!(category_rank("schematic") < category_rank("placement"));
        assert!(category_rank("placement") < category_rank("routing"));
        assert_eq!(category_of("ERC", "ERC-001"), "schematic");
        assert_eq!(category_of("DRC", "DRC-004"), "routing");
    }
}
