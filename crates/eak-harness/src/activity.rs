//! Agent activity timeline (ADR-0053): one time-ordered list built only from what the project
//! records, so the activity view cannot show anything that did not happen:
//! * `revision` – committed operations of the engine journal (actor, command, revision before and
//!   after, the objects the change touched);
//! * `proposal` – proposal events (`proposals/journal.jsonl`: created, renamed, submitted, applied,
//!   discarded);
//! * `weft` – WEFT registrations, tasks, status changes and messages;
//! * `check` – validation reports saved as evidence (`evidence/<check>-rNNNNNN.json`) with their
//!   error and warning counts.
//!
//! Reads files only; never writes.

use crate::proposal::PROPOSALS_DIR;
use crate::weft::{self, Event};
use eak_runtime::store::{self, EntryState};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub ts_ms: u64,
    /// `revision`, `proposal`, `weft` or `check`.
    pub source: &'static str,
    /// Journal actor label (`agent:<name>@<session>`, `human:…`), or `system`.
    pub actor: String,
    /// Command name, proposal event, WEFT event or message kind, or the check name.
    pub kind: String,
    pub summary: String,
    pub revision_before: Option<u64>,
    pub revision_after: Option<u64>,
    pub op_id: Option<String>,
    pub proposal_id: Option<String>,
    pub task_id: Option<String>,
    /// For a revision: the changed objects as `path:key` (what the canvas highlights).
    pub objects: Vec<String>,
    /// For a check: its counts on that revision.
    pub errors: Option<u64>,
    pub warnings: Option<u64>,
}

fn item(ts_ms: u64, source: &'static str, actor: &str, kind: &str, summary: String) -> Item {
    Item {
        ts_ms,
        source,
        actor: actor.to_string(),
        kind: kind.to_string(),
        summary,
        revision_before: None,
        revision_after: None,
        op_id: None,
        proposal_id: None,
        task_id: None,
        objects: vec![],
        errors: None,
        warnings: None,
    }
}

fn revisions(dir: &Path) -> Vec<Item> {
    // The journal may hold a pending and a committed line per operation: keep the last one.
    let mut last: BTreeMap<String, store::JournalEntry> = BTreeMap::new();
    for e in store::read_journal(dir) {
        last.insert(e.op_id.clone(), e);
    }
    last.into_values()
        .filter(|e| e.state == EntryState::Committed)
        .map(|e| {
            let kind = format!("{:?}", e.kind).to_lowercase();
            let mut it = item(
                e.timestamp_ms as u64,
                "revision",
                &e.actor,
                &e.command,
                format!(
                    "{} {} → revision {} ({} change(s))",
                    if kind == "do" { "ran" } else { kind.as_str() },
                    e.command,
                    e.revision_after,
                    e.changes.len()
                ),
            );
            it.revision_before = Some(e.revision_before);
            it.revision_after = Some(e.revision_after);
            it.objects = e
                .changes
                .iter()
                .map(|c| format!("{}:{}", c.path.join("."), c.key.clone().unwrap_or_default()))
                .collect();
            it.op_id = Some(e.op_id);
            it
        })
        .collect()
}

fn proposals(dir: &Path) -> Vec<Item> {
    let Ok(text) = std::fs::read_to_string(dir.join(PROPOSALS_DIR).join("journal.jsonl")) else {
        return vec![];
    };
    text.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .map(|v| {
            let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            let n = |k: &str| v.get(k).and_then(Value::as_u64);
            let event = s("event");
            let mut it = item(
                n("ts_ms").unwrap_or(0),
                "proposal",
                &s("actor"),
                &event,
                format!(
                    "proposal {} {event} ({} step(s), made at revision {})",
                    s("proposal_id"),
                    n("steps").unwrap_or(0),
                    n("base_revision").unwrap_or(0)
                ),
            );
            it.proposal_id = Some(s("proposal_id")).filter(|p| !p.is_empty());
            it.revision_after = n("revision");
            it
        })
        .collect()
}

fn weft_items(dir: &Path) -> Vec<Item> {
    weft::entries(dir)
        .0
        .into_iter()
        .map(|e| {
            let (kind, summary, task, proposal) = match &e.event {
                Event::AgentRegistered { agent } => (
                    "agent_registered".to_string(),
                    format!(
                        "registered ({}{})",
                        agent.tool,
                        if agent.model.is_empty() {
                            String::new()
                        } else {
                            format!(", {}", agent.model)
                        }
                    ),
                    None,
                    None,
                ),
                Event::TaskCreated { task } => (
                    "task_created".to_string(),
                    format!("task \"{}\" for {}", task.title, task.owner),
                    Some(task.id.clone()),
                    None,
                ),
                Event::TaskStatus { task_id, status } => (
                    "task_status".to_string(),
                    format!(
                        "task {task_id} → {}",
                        serde_json::to_value(status)
                            .ok()
                            .and_then(|v| v.as_str().map(String::from))
                            .unwrap_or_default()
                    ),
                    Some(task_id.clone()),
                    None,
                ),
                Event::Message { message: m } => {
                    let kind = serde_json::to_value(m.kind)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default();
                    let verdict = m
                        .verdict
                        .and_then(|v| serde_json::to_value(v).ok())
                        .and_then(|v| v.as_str().map(|s| format!(" ({s})")))
                        .unwrap_or_default();
                    let body = if m.body.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", m.body.chars().take(160).collect::<String>())
                    };
                    (
                        kind.clone(),
                        format!("{kind}{verdict} to {}{body}", m.to),
                        m.task_id.clone(),
                        m.proposal_id.clone(),
                    )
                }
            };
            let mut it = item(e.ts_ms, "weft", &e.by, &kind, summary);
            it.revision_after = Some(e.revision);
            it.task_id = task;
            it.proposal_id = proposal;
            it
        })
        .collect()
}

fn checks(dir: &Path) -> Vec<Item> {
    let Ok(read) = std::fs::read_dir(dir.join("evidence")) else {
        return vec![];
    };
    let mut out = vec![];
    for f in read.flatten() {
        let path = f.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(v) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            continue;
        };
        // Only validation reports: they carry a check name, a revision and both counts.
        let (Some(check), Some(rev), Some(errors), Some(warnings)) = (
            v.get("check").and_then(Value::as_str),
            v.get("revision").and_then(Value::as_u64),
            v.get("errors").and_then(Value::as_u64),
            v.get("warnings").and_then(Value::as_u64),
        ) else {
            continue;
        };
        let ts = f
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut it = item(
            ts,
            "check",
            "system",
            check,
            format!("{check} on revision {rev}: {errors} error(s), {warnings} warning(s) in implemented checks"),
        );
        it.revision_after = Some(rev);
        it.errors = Some(errors);
        it.warnings = Some(warnings);
        out.push(it);
    }
    out
}

/// Everything the project recorded, oldest first (ties keep source order: revision, proposal,
/// WEFT, check).
pub fn timeline(dir: &Path) -> Vec<Item> {
    let mut all = revisions(dir);
    all.extend(proposals(dir));
    all.extend(weft_items(dir));
    all.extend(checks(dir));
    all.sort_by_key(|i| i.ts_ms);
    all
}
