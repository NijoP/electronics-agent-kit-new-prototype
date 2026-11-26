//! WEFT: agent-to-agent orchestration on one project (ADR-0053).
//!
//! Agents (a Claude Code or Codex session, the in-app agent, any MCP client) register, delegate
//! tasks to each other, share context and findings, ask each other or the engineer to review a
//! proposal, and answer with a verdict. Every one of these is a typed event appended to one
//! journal, `<project>/weft/journal.jsonl`; the registry, the task graph and every inbox are
//! derived by replaying it, so the history is the state and nothing is shown that was not sent.
//! The design itself is never touched here: edits stay proposals (ADR-0051) and only the engineer
//! applies one. A review verdict is advice to the engineer, never an approval EAK acts on.
//!
//! Writes happen while the caller holds the project's engine (the project lock, or the app's
//! engine through its endpoint), so two agents' events are appended one after the other.

use crate::context::CallContext;
use crate::proposal;
use crate::tools::{command_error, ErrorCode, Fail};
use eak_runtime::store;
use eak_runtime::{Actor, Engine};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const DIR: &str = "weft";
pub const JOURNAL: &str = "journal.jsonl";
/// Recipient that is the person at EAK, not an agent (review requests only).
pub const ENGINEER: &str = "engineer";
/// Recipient meaning every registered agent (context, findings, status, blockers).
pub const EVERYONE: &str = "*";

/// Launch contract (ADR-0053 addendum): a session started by EAK's AI tools (or any launcher)
/// sets these next to `EAK_AGENT_NAME` and `EAK_SESSION_ID`, and `eak-mcp` then registers the
/// session on the bus itself, as if it had called `weft_register`.
pub const ENV_AGENT_TOOL: &str = "EAK_AGENT_TOOL";
pub const ENV_AGENT_MODEL: &str = "EAK_AGENT_MODEL";
/// Comma-separated.
pub const ENV_AGENT_CAPABILITIES: &str = "EAK_AGENT_CAPABILITIES";

/// What a launcher said about the session it started; `None` when `EAK_AGENT_TOOL` is not set
/// (a session started by hand registers with `weft_register` when it wants to).
pub fn launch_registration(get: impl Fn(&str) -> Option<String>) -> Option<Value> {
    let tool = get(ENV_AGENT_TOOL)
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())?;
    let capabilities: Vec<String> = get(ENV_AGENT_CAPABILITIES)
        .unwrap_or_default()
        .split(',')
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();
    Some(json!({
        "tool": tool,
        "model": get(ENV_AGENT_MODEL).unwrap_or_default().trim(),
        "capabilities": capabilities,
    }))
}
const MAX_TEXT: usize = 8000;

/// A registered agent. `id` is the journal actor label (`agent:<name>@<session>`), so the
/// engine's journal and this one name an agent the same way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentCard {
    pub id: String,
    pub name: String,
    pub session_id: String,
    /// What runs the agent: `claude-code`, `codex`, `eak-app`, or another client name.
    pub tool: String,
    pub model: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Blocked,
    Done,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    /// Agent the task is delegated to; only it changes the task's status.
    pub owner: String,
    pub created_by: String,
    /// Tasks that must be `done` before this one starts or finishes.
    pub depends_on: Vec<String>,
    pub parent: Option<String>,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Delegate,
    ContextShare,
    Status,
    Finding,
    ReviewRequest,
    ReviewVerdict,
    Blocker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Approve,
    RequestChanges,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub kind: MessageKind,
    pub from: String,
    /// An agent id, [`ENGINEER`] or [`EVERYONE`].
    pub to: String,
    pub task_id: Option<String>,
    pub proposal_id: Option<String>,
    pub body: String,
    pub verdict: Option<Verdict>,
    /// For `status` and `blocker`: the task status the sender set.
    pub status: Option<TaskStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    AgentRegistered { agent: AgentCard },
    TaskCreated { task: Task },
    TaskStatus { task_id: String, status: TaskStatus },
    Message { message: Message },
}

/// One journal line: who did what, when (Unix ms), at which project revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub seq: u64,
    pub ts_ms: u64,
    pub by: String,
    pub revision: u64,
    #[serde(flatten)]
    pub event: Event,
}

/// The state the journal describes.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct State {
    pub agents: BTreeMap<String, AgentCard>,
    pub tasks: BTreeMap<String, Task>,
    /// Messages with their journal sequence number.
    pub messages: Vec<(u64, Message)>,
    pub last_seq: u64,
}

fn journal_path(dir: &Path) -> PathBuf {
    dir.join(DIR).join(JOURNAL)
}

/// Every journal entry, oldest first, and how many lines did not parse (skipped, never guessed).
pub fn entries(dir: &Path) -> (Vec<Entry>, usize) {
    let Ok(text) = std::fs::read_to_string(journal_path(dir)) else {
        return (vec![], 0);
    };
    let mut bad = 0;
    let mut out = vec![];
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Entry>(line) {
            Ok(e) => out.push(e),
            Err(_) => bad += 1,
        }
    }
    (out, bad)
}

/// Replay journal entries into the registry, the tasks and the messages.
pub fn replay(entries: &[Entry]) -> State {
    let mut s = State::default();
    for e in entries {
        s.last_seq = s.last_seq.max(e.seq);
        match &e.event {
            Event::AgentRegistered { agent } => {
                s.agents.insert(agent.id.clone(), agent.clone());
            }
            Event::TaskCreated { task } => {
                s.tasks.insert(task.id.clone(), task.clone());
            }
            Event::TaskStatus { task_id, status } => {
                if let Some(t) = s.tasks.get_mut(task_id) {
                    t.status = *status;
                }
            }
            Event::Message { message } => s.messages.push((e.seq, message.clone())),
        }
    }
    s
}

pub fn read(dir: &Path) -> State {
    replay(&entries(dir).0)
}

fn append(dir: &Path, by: &str, revision: u64, events: Vec<Event>) -> Result<Vec<Entry>, Fail> {
    std::fs::create_dir_all(dir.join(DIR)).map_err(storage)?;
    let start = read(dir).last_seq;
    let ts_ms = store::now_ms() as u64;
    let mut lines = String::new();
    let mut out = vec![];
    for (i, event) in events.into_iter().enumerate() {
        let e = Entry {
            seq: start + i as u64 + 1,
            ts_ms,
            by: by.to_string(),
            revision,
            event,
        };
        lines.push_str(&serde_json::to_string(&e).map_err(storage)?);
        lines.push('\n');
        out.push(e);
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal_path(dir))
        .and_then(|mut f| f.write_all(lines.as_bytes()))
        .map_err(storage)?;
    Ok(out)
}

fn storage(e: impl std::fmt::Display) -> Fail {
    Fail(
        ErrorCode::Storage,
        format!("weft journal: {e}"),
        Value::Null,
    )
}
fn invalid(m: impl Into<String>) -> Fail {
    Fail(ErrorCode::InvalidArguments, m.into(), Value::Null)
}
fn precondition(m: impl Into<String>) -> Fail {
    Fail(ErrorCode::Precondition, m.into(), Value::Null)
}
fn not_found(m: impl Into<String>) -> Fail {
    Fail(ErrorCode::NotFound, m.into(), Value::Null)
}

fn text<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(Value::as_str).unwrap_or("").trim()
}
fn opt(args: &Value, key: &str) -> Option<String> {
    Some(text(args, key).to_string()).filter(|s| !s.is_empty())
}
fn strings(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}
fn clip(s: &str) -> String {
    s.chars().take(MAX_TEXT).collect()
}
fn enum_arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<Option<T>, Fail> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|_| invalid(format!("{key}: unknown value {v}"))),
    }
}

/// This caller's agent id.
pub fn agent_id(ctx: &CallContext) -> String {
    Actor::agent_in_session(&ctx.agent, &ctx.session_id).label()
}

/// The agent id of a proposal's author.
pub fn author(p: &proposal::Proposal) -> String {
    Actor::agent_in_session(&p.agent, &p.session_id).label()
}

/// Tasks a task waits for that are not done yet.
pub fn waiting_on(state: &State, task: &Task) -> Vec<String> {
    task.depends_on
        .iter()
        .filter(|d| state.tasks.get(*d).map(|t| t.status) != Some(TaskStatus::Done))
        .cloned()
        .collect()
}

fn task_view(state: &State, t: &Task) -> Value {
    let waiting = waiting_on(state, t);
    json!({
        "task": t,
        "waiting_on": waiting,
        "ready": t.status == TaskStatus::Pending && waiting.is_empty(),
    })
}

fn find_task<'a>(state: &'a State, id: Option<&str>) -> Result<&'a Task, Fail> {
    let id = id.ok_or_else(|| invalid("task_id is required"))?;
    state
        .tasks
        .get(id)
        .ok_or_else(|| not_found(format!("no task {id}")))
}

fn known_agent(state: &State, id: &str) -> Result<(), Fail> {
    if state.agents.contains_key(id) {
        Ok(())
    } else {
        Err(not_found(format!(
            "no registered agent {id} (see weft_agents)"
        )))
    }
}

fn registered(state: &State, ctx: &CallContext) -> Result<String, Fail> {
    let id = agent_id(ctx);
    if state.agents.contains_key(&id) {
        Ok(id)
    } else {
        Err(precondition(
            "this session is not registered with WEFT: call weft_register first",
        ))
    }
}

/// Proposals that cannot all be applied as they stand: one made at another revision, or two that
/// touch the same objects. Applying one makes the others stale, and `proposal::apply` refuses a
/// stale one, so nothing is overwritten silently. The engineer rejects one, or its agent proposes
/// again on the new revision. EAK does not rebase a proposal.
pub(crate) fn conflicts(engine: &Engine) -> Result<Vec<Value>, Fail> {
    let dir = engine
        .dir()
        .ok_or_else(|| precondition("WEFT needs a saved project folder"))?;
    let current = engine.project().revision;
    let mut out = vec![];
    let mut touched: Vec<(String, BTreeSet<String>)> = vec![];
    for p in proposal::list(dir)
        .into_iter()
        .filter(|p| p.project_id == engine.project().id && p.status.reviewable())
    {
        if p.base_revision != current {
            out.push(json!({
                "kind": "stale",
                "proposals": [p.id],
                "base_revision": p.base_revision,
                "current_revision": current,
                "resolve": "reject it, or have its agent propose again on the current revision (EAK does not rebase)",
            }));
            continue;
        }
        let review = proposal::review(engine, &p.id, None).map_err(|e| command_error(&e))?;
        let objects: BTreeSet<String> = review
            .changes
            .iter()
            .map(|c| format!("{}:{}", c.path.join("."), c.key.clone().unwrap_or_default()))
            .collect();
        touched.push((p.id, objects));
    }
    for (i, (a, oa)) in touched.iter().enumerate() {
        for (b, ob) in touched.iter().skip(i + 1) {
            let both: Vec<&String> = oa.intersection(ob).collect();
            if !both.is_empty() {
                out.push(json!({
                    "kind": "overlap",
                    "proposals": [a, b],
                    "objects": both,
                    "resolve": "apply at most one: applying it makes the other stale; reject one, or have its agent propose again after the first is applied",
                }));
            }
        }
    }
    Ok(out)
}

// ------------------------------------------------- the engineer's side (desktop, ADR-0053)

/// The conflicts among open proposals, for the activity view.
pub fn conflict_list(engine: &Engine) -> Result<Vec<Value>, String> {
    conflicts(engine).map_err(|f| f.1)
}

/// Review requests routed to the engineer that the engineer has not answered yet, for proposals
/// that can still be applied (oldest first, one per proposal).
pub fn engineer_queue(engine: &Engine) -> Vec<Value> {
    let Some(dir) = engine.dir() else {
        return vec![];
    };
    let state = read(dir);
    let answered: BTreeSet<String> = state
        .messages
        .iter()
        .filter(|(_, m)| m.from == ENGINEER && m.kind == MessageKind::ReviewVerdict)
        .filter_map(|(_, m)| m.proposal_id.clone())
        .collect();
    let mut seen = BTreeSet::new();
    let mut out = vec![];
    for (seq, m) in &state.messages {
        if m.kind != MessageKind::ReviewRequest || m.to != ENGINEER {
            continue;
        }
        let Some(pid) = m.proposal_id.clone() else {
            continue;
        };
        if answered.contains(&pid) || !seen.insert(pid.clone()) {
            continue;
        }
        let Ok(p) = proposal::load(dir, &pid) else {
            continue;
        };
        if !p.status.reviewable() {
            continue;
        }
        out.push(json!({
            "seq": seq, "proposal_id": pid, "from": m.from, "body": m.body,
            "title": p.title, "base_revision": p.base_revision,
        }));
    }
    out
}

/// The engineer answers a proposal on the bus: a review verdict sent to its author. With
/// `discard` (Reject, Re-propose) the proposal is discarded first, by the person, so it can no
/// longer be applied. A verdict never applies anything: Apply stays in the review dialog.
pub fn engineer_verdict(
    engine: &Engine,
    proposal_id: &str,
    verdict: Verdict,
    body: &str,
    discard: bool,
) -> Result<Value, String> {
    let dir = engine
        .dir()
        .ok_or("WEFT needs a saved project folder")?
        .to_path_buf();
    let p = proposal::load(&dir, proposal_id).map_err(|e| e.to_string())?;
    if !p.status.reviewable() {
        return Err(format!(
            "proposal {} is no longer open: nothing to answer",
            p.id
        ));
    }
    if discard {
        proposal::discard(engine, proposal_id, &Actor::human(), None).map_err(|e| e.to_string())?;
    }
    let message = Message {
        id: eak_core::new_id("msg"),
        kind: MessageKind::ReviewVerdict,
        from: ENGINEER.to_string(),
        to: author(&p),
        task_id: None,
        proposal_id: Some(p.id.clone()),
        body: clip(body),
        verdict: Some(verdict),
        status: None,
    };
    append(
        &dir,
        ENGINEER,
        engine.project().revision,
        vec![Event::Message {
            message: message.clone(),
        }],
    )
    .map_err(|f| f.1)?;
    Ok(json!({"sent": message, "discarded": discard}))
}

/// `weft_send`: check one typed message against the journal state and append it, with the task
/// it creates or the status it sets.
fn send(engine: &Engine, dir: &Path, ctx: &CallContext, args: &Value) -> Result<Value, Fail> {
    let state = read(dir);
    let me = registered(&state, ctx)?;
    let kind: MessageKind = enum_arg(args, "kind")?.ok_or_else(|| {
        invalid("kind is required: delegate, context_share, status, finding, review_request, review_verdict or blocker")
    })?;
    let to = opt(args, "to").unwrap_or_else(|| EVERYONE.to_string());
    let task_id = opt(args, "task_id");
    let proposal_id = opt(args, "proposal_id");
    let mut message = Message {
        id: eak_core::new_id("msg"),
        kind,
        from: me.clone(),
        to: to.clone(),
        task_id: task_id.clone(),
        proposal_id: proposal_id.clone(),
        body: clip(text(args, "body")),
        verdict: None,
        status: None,
    };
    let load = |id: &Option<String>| -> Result<proposal::Proposal, Fail> {
        let id = id
            .as_deref()
            .ok_or_else(|| invalid("proposal_id is required"))?;
        proposal::load(dir, id).map_err(|e| command_error(&e))
    };
    let mut events = vec![];
    match kind {
        MessageKind::Delegate => {
            known_agent(&state, &to)?;
            let title = clip(text(args, "title"));
            if title.is_empty() {
                return Err(invalid("a delegated task needs a title"));
            }
            let depends_on = strings(args, "depends_on");
            for d in &depends_on {
                if !state.tasks.contains_key(d) {
                    return Err(not_found(format!("depends_on names no task {d}")));
                }
            }
            if task_id.is_some() {
                find_task(&state, task_id.as_deref())?;
            }
            let t = Task {
                id: eak_core::new_id("task"),
                title,
                owner: to.clone(),
                created_by: me.clone(),
                depends_on,
                parent: task_id.clone(),
                status: TaskStatus::Pending,
            };
            message.task_id = Some(t.id.clone());
            events.push(Event::TaskCreated { task: t });
        }
        MessageKind::Status | MessageKind::Blocker => {
            let status = if kind == MessageKind::Blocker {
                TaskStatus::Blocked
            } else {
                enum_arg::<TaskStatus>(args, "status")?.ok_or_else(|| {
                    invalid("status is required: pending, in_progress, blocked, done or rejected")
                })?
            };
            if kind == MessageKind::Status || task_id.is_some() {
                let t = find_task(&state, task_id.as_deref())?;
                if t.owner != me {
                    return Err(Fail(
                        ErrorCode::ApprovalRequired,
                        format!("only the task's owner {} changes its status", t.owner),
                        Value::Null,
                    ));
                }
                if matches!(status, TaskStatus::InProgress | TaskStatus::Done) {
                    let waiting = waiting_on(&state, t);
                    if !waiting.is_empty() {
                        return Err(Fail(
                            ErrorCode::Precondition,
                            format!(
                                "task {} waits for {} to be done first",
                                t.id,
                                waiting.join(", ")
                            ),
                            json!({"waiting_on": waiting}),
                        ));
                    }
                }
                message.status = Some(status);
                events.push(Event::TaskStatus {
                    task_id: t.id.clone(),
                    status,
                });
            }
            if to != EVERYONE {
                known_agent(&state, &to)?;
            }
        }
        MessageKind::ContextShare | MessageKind::Finding => {
            if message.body.is_empty() {
                return Err(invalid("body is required"));
            }
            if to != EVERYONE {
                known_agent(&state, &to)?;
            }
            if task_id.is_some() {
                find_task(&state, task_id.as_deref())?;
            }
        }
        MessageKind::ReviewRequest => {
            let p = load(&proposal_id)?;
            if p.project_id != engine.project().id {
                return Err(precondition("the proposal belongs to another project"));
            }
            if to == EVERYONE {
                return Err(invalid(
                    "a review request names its reviewer: an agent id or engineer",
                ));
            }
            if to != ENGINEER {
                known_agent(&state, &to)?;
                if author(&p) == to {
                    return Err(precondition(
                        "a proposal is reviewed by someone other than its author",
                    ));
                }
            }
        }
        MessageKind::ReviewVerdict => {
            let p = load(&proposal_id)?;
            let verdict: Verdict = enum_arg(args, "verdict")?.ok_or_else(|| {
                invalid("verdict is required: approve, request_changes or reject")
            })?;
            if author(&p) == me {
                return Err(precondition("an agent does not review its own proposal"));
            }
            let asked = state.messages.iter().any(|(_, m)| {
                m.kind == MessageKind::ReviewRequest
                    && m.to == me
                    && m.proposal_id.as_deref() == Some(p.id.as_str())
            });
            if !asked {
                return Err(precondition(format!(
                    "nobody asked {me} to review {}",
                    p.id
                )));
            }
            message.verdict = Some(verdict);
            if message.to == EVERYONE {
                message.to = author(&p);
            }
        }
    }
    events.push(Event::Message {
        message: message.clone(),
    });
    let written = append(dir, &me, engine.project().revision, events)?;
    let mut out = json!({"sent": message, "seq": written.last().map(|e| e.seq).unwrap_or(0)});
    if kind == MessageKind::Delegate {
        let after = read(dir);
        if let Some(t) = message
            .task_id
            .as_deref()
            .and_then(|id| after.tasks.get(id))
        {
            out["task"] = task_view(&after, t);
        }
    }
    if kind == MessageKind::ReviewVerdict {
        out["note"] = json!(
            "a verdict is advice to the engineer; only the engineer applies a proposal, in EAK"
        );
    }
    Ok(out)
}

/// The `weft_*` tools.
pub(crate) fn dispatch(
    engine: &mut Engine,
    ctx: &CallContext,
    tool: &str,
    args: &Value,
) -> Result<Value, Fail> {
    let dir = engine
        .dir()
        .map(Path::to_path_buf)
        .ok_or_else(|| precondition("WEFT needs a saved project folder"))?;
    match tool {
        "weft_register" => {
            let client = text(args, "tool");
            if client.is_empty() {
                return Err(invalid(
                    "tool is required (claude-code, codex, eak-app or the client's name)",
                ));
            }
            let card = AgentCard {
                id: agent_id(ctx),
                name: ctx.agent.clone(),
                session_id: ctx.session_id.clone(),
                tool: clip(client),
                model: clip(text(args, "model")),
                capabilities: strings(args, "capabilities"),
            };
            if read(&dir).agents.get(&card.id) != Some(&card) {
                append(
                    &dir,
                    &card.id,
                    engine.project().revision,
                    vec![Event::AgentRegistered {
                        agent: card.clone(),
                    }],
                )?;
            }
            Ok(json!({"agent": card}))
        }
        "weft_agents" => Ok(json!({"agents": read(&dir).agents.into_values().collect::<Vec<_>>()})),
        "weft_send" => send(engine, &dir, ctx, args),
        "weft_inbox" => {
            let state = read(&dir);
            let me = registered(&state, ctx)?;
            let since = args.get("since").and_then(Value::as_u64).unwrap_or(0);
            let msgs: Vec<Value> = state
                .messages
                .iter()
                .filter(|(seq, m)| *seq > since && m.from != me && (m.to == me || m.to == EVERYONE))
                .map(|(seq, m)| json!({"seq": seq, "message": m}))
                .collect();
            Ok(json!({"agent": me, "messages": msgs, "next_since": state.last_seq}))
        }
        "weft_tasks" => {
            let state = read(&dir);
            let me = agent_id(ctx);
            let mine = args.get("mine").and_then(Value::as_bool).unwrap_or(false);
            let tasks: Vec<Value> = state
                .tasks
                .values()
                .filter(|t| !mine || t.owner == me)
                .map(|t| task_view(&state, t))
                .collect();
            Ok(json!({"tasks": tasks}))
        }
        "weft_conflicts" => Ok(json!({
            "revision": engine.project().revision,
            "conflicts": conflicts(engine)?,
            "note": "no proposal is applied over another silently: once one is applied the others are stale and refused",
        })),
        "weft_history" => {
            let (all, unreadable) = entries(&dir);
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(200)
                .min(2000) as usize;
            let skip = all.len().saturating_sub(limit);
            Ok(json!({"entries": &all[skip..], "total": all.len(), "unreadable": unreadable}))
        }
        _ => Err(Fail(
            ErrorCode::UnknownTool,
            format!("unknown WEFT tool {tool}"),
            Value::Null,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_registration_comes_only_from_a_launcher_that_named_the_tool() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| {
                pairs
                    .iter()
                    .find(|(n, _)| *n == k)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(launch_registration(env(&[])), None);
        assert_eq!(launch_registration(env(&[(ENV_AGENT_TOOL, " ")])), None);
        assert_eq!(
            launch_registration(env(&[
                (ENV_AGENT_TOOL, "codex"),
                (ENV_AGENT_MODEL, "m1"),
                (ENV_AGENT_CAPABILITIES, "schematic, review,,"),
            ])),
            Some(json!({"tool": "codex", "model": "m1", "capabilities": ["schematic", "review"]}))
        );
    }

    fn card(id: &str) -> AgentCard {
        AgentCard {
            id: id.into(),
            name: id.into(),
            session_id: "s".into(),
            tool: "codex".into(),
            model: String::new(),
            capabilities: vec![],
        }
    }

    fn entry(seq: u64, event: Event) -> Entry {
        Entry {
            seq,
            ts_ms: 0,
            by: "a".into(),
            revision: 0,
            event,
        }
    }

    #[test]
    fn replay_builds_the_registry_tasks_and_messages_from_the_journal_alone() {
        let task = Task {
            id: "task_1".into(),
            title: "route power".into(),
            owner: "b".into(),
            created_by: "a".into(),
            depends_on: vec![],
            parent: None,
            status: TaskStatus::Pending,
        };
        let msg = Message {
            id: "msg_1".into(),
            kind: MessageKind::Delegate,
            from: "a".into(),
            to: "b".into(),
            task_id: Some("task_1".into()),
            proposal_id: None,
            body: String::new(),
            verdict: None,
            status: None,
        };
        let log = vec![
            entry(1, Event::AgentRegistered { agent: card("a") }),
            entry(2, Event::AgentRegistered { agent: card("b") }),
            entry(3, Event::TaskCreated { task: task.clone() }),
            entry(4, Event::Message { message: msg }),
            entry(
                5,
                Event::TaskStatus {
                    task_id: "task_1".into(),
                    status: TaskStatus::Done,
                },
            ),
        ];
        // Each entry round-trips through its journal line.
        for x in &log {
            let line = serde_json::to_string(x).unwrap();
            assert_eq!(&serde_json::from_str::<Entry>(&line).unwrap(), x, "{line}");
        }
        let s = replay(&log);
        assert_eq!(s.agents.len(), 2);
        assert_eq!(s.tasks["task_1"].status, TaskStatus::Done);
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.last_seq, 5);
        let later = Task {
            id: "task_2".into(),
            depends_on: vec!["task_1".into(), "task_x".into()],
            ..task
        };
        assert_eq!(waiting_on(&s, &later), vec!["task_x".to_string()]);
    }
}
