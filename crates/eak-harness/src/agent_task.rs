//! Agent task phases (ADR-0036): an execution-grounded record of what a terminal agent has done
//! to the open project, shown identically in the terminal and EDA workspaces.
//!
//! * Phases are **observed, never declared**: the ledger is advanced by the outcome of harness
//!   calls (a committed design command, a finished check, a refused gate), not by what the agent
//!   says it is doing. The agent can only end a task, and "completed" is accepted only when the
//!   recorded evidence supports it (otherwise the task ends `partial`, with the reason).
//! * A task records the revision it started from and the latest revision it observed.
//! * The PCB synchronisation gate lives here too: `SyncSchematicToPcb` is refused until the
//!   schematic holds committed symbols and ERC has run on it (stored evidence, revision-bound).
//!
//! The ledger is `<project>/agent-tasks.json`. It is a progress record, not design state: it
//! never changes the project revision and is never evidence for a verification claim.

use crate::context::CallContext;
use crate::tools::{ErrorCode, Fail};
use eak_runtime::store::{self, EntryState};
use eak_runtime::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

/// File (inside the project folder) holding the ledger.
pub const TASK_FILE: &str = "agent-tasks.json";
/// Tasks kept in the ledger (oldest dropped first).
const MAX_TASKS: usize = 50;
/// Phase entries kept per task.
const MAX_HISTORY: usize = 100;

/// Where a task is, in the order a design normally moves. The declaration order is the progress
/// order of the first ten; the last five are how a task ends or stalls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Planning,
    GeneratingSchematic,
    SchematicCommitted,
    RunningErc,
    SchematicApproved,
    SyncingPcb,
    Placement,
    Routing,
    RunningDrc,
    ManufacturingPreview,
    Completed,
    Partial,
    Blocked,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseEntry {
    pub phase: Phase,
    /// Project revision when the phase was observed.
    pub revision: u64,
    pub at_ms: u64,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub project_id: String,
    pub session_id: String,
    pub agent: String,
    pub title: String,
    /// Revision the project was at when the task began.
    pub starting_revision: u64,
    /// Latest revision this task observed.
    pub revision: u64,
    pub phase: Phase,
    #[serde(default)]
    pub detail: Option<String>,
    pub started_ms: u64,
    pub updated_ms: u64,
    /// Set when the task ended (any of completed, partial, blocked, failed, cancelled).
    #[serde(default)]
    pub ended_ms: Option<u64>,
    /// Errors in the latest ERC this task saw, cleared when the schematic changes afterwards.
    #[serde(default)]
    pub last_erc_errors: Option<usize>,
    /// Errors in the latest DRC this task saw, cleared when the board changes afterwards.
    #[serde(default)]
    pub last_drc_errors: Option<usize>,
    #[serde(default)]
    pub history: Vec<PhaseEntry>,
}

impl AgentTask {
    /// The furthest working phase reached, ignoring a stall (`Blocked`).
    pub fn progress(&self) -> Phase {
        self.history
            .iter()
            .rev()
            .map(|h| h.phase)
            .find(|p| *p != Phase::Blocked)
            .unwrap_or(Phase::Planning)
    }

    fn set(&mut self, phase: Phase, revision: u64, now: u64, note: Option<String>) {
        let changed = self.phase != phase || self.detail != note;
        self.phase = phase;
        self.detail = note.clone();
        self.revision = revision;
        self.updated_ms = now;
        if changed {
            self.history.push(PhaseEntry {
                phase,
                revision,
                at_ms: now,
                note,
            });
            if self.history.len() > MAX_HISTORY {
                self.history.remove(0);
            }
        }
    }
}

/// All tasks of one project.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub tasks: Vec<AgentTask>,
}

/// What a finished harness call told us, already reduced to facts.
pub enum Kind {
    /// A design tool. `commands` are the `Command` type names it carried (several for a batch).
    Design {
        commands: Vec<String>,
        ok: bool,
        /// The message of a `precondition` refusal (a gate), when that is why it failed.
        refusal: Option<String>,
    },
    /// A validation tool that ran: error counts per check actually executed.
    Checks {
        erc: Option<usize>,
        drc: Option<usize>,
    },
    /// The manufacturing package was compiled (a draft; never "manufacturing-ready").
    Manufacturing,
}

pub struct Event<'a> {
    pub session: &'a str,
    pub agent: &'a str,
    pub project_id: &'a str,
    pub kind: Kind,
    pub rev_before: u64,
    pub rev_after: u64,
    /// Committed schematic symbols after the call.
    pub symbols: usize,
    pub now: u64,
}

const SCHEMATIC_COMMANDS: &[&str] = &[
    "AddSheet",
    "RenameSheet",
    "DeleteSheet",
    "ReorderSheets",
    "PlaceSymbol",
    "MoveSymbol",
    "RotateSymbol",
    "FlipSymbol",
    "SetSymbolValue",
    "SetRefdes",
    "DeleteSymbol",
    "AddWire",
    "DeleteWire",
    "MoveWire",
    "ConnectPins",
    "AddNetLabel",
    "LabelPin",
    "DeleteNetLabel",
    "MoveNetLabel",
    "SetNoConnect",
    "DeleteNoConnect",
    "SetNetLabelScope",
    "SetPowerPortStyle",
    "SetFieldVisibility",
    "AddBus",
    "MoveBus",
    "DeleteBus",
    "AddBusEntry",
    "MoveBusEntry",
    "DeleteBusEntry",
    "AddSheetGraphic",
    "MoveSheetGraphic",
    "DeleteSheetGraphic",
    "SetSheetFormat",
    "Annotate",
    "ImportLibraryComponent",
    "UpsertComponentDefinition",
    "SetComponentProperties",
    "AssignComponentFootprint",
];

const PLACEMENT_COMMANDS: &[&str] = &[
    "AutoPlace",
    "MoveFootprint",
    "SetFootprintLocked",
    "DeleteFootprint",
];

const ROUTING_COMMANDS: &[&str] = &[
    "AutoRoute",
    "AddTrack",
    "DeleteTrack",
    "AddVia",
    "DeleteVia",
    "AddZone",
    "DeleteZone",
    "MoveTrack",
    "EditTrack",
    "MoveVia",
    "EditZone",
    "AddRegion",
    "DeleteRegion",
    "StitchZones",
    "ClearRouting",
];

/// The phase a committed command moves a task to, if it is a phase-defining command.
fn command_phase(command: &str, symbols: usize) -> Option<Phase> {
    if command == "SyncSchematicToPcb" || command == "SyncSchematicToPcbSelected" {
        Some(Phase::SyncingPcb)
    } else if PLACEMENT_COMMANDS.contains(&command) {
        Some(Phase::Placement)
    } else if ROUTING_COMMANDS.contains(&command) {
        Some(Phase::Routing)
    } else if SCHEMATIC_COMMANDS.contains(&command) {
        // A schematic is "committed" once committed content exists; until then it is being generated.
        Some(if symbols > 0 {
            Phase::SchematicCommitted
        } else {
            Phase::GeneratingSchematic
        })
    } else {
        None
    }
}

impl Ledger {
    fn active(&self, session: &str, project_id: &str) -> Option<usize> {
        self.tasks.iter().rposition(|t| {
            t.session_id == session && t.project_id == project_id && t.ended_ms.is_none()
        })
    }

    fn begin(&mut self, ev: &Event, title: &str) -> usize {
        self.seq += 1;
        self.tasks.push(AgentTask {
            id: format!("task-{}", self.seq),
            project_id: ev.project_id.into(),
            session_id: ev.session.into(),
            agent: ev.agent.into(),
            title: title.into(),
            starting_revision: ev.rev_before,
            revision: ev.rev_before,
            phase: Phase::Planning,
            detail: None,
            started_ms: ev.now,
            updated_ms: ev.now,
            ended_ms: None,
            last_erc_errors: None,
            last_drc_errors: None,
            history: vec![PhaseEntry {
                phase: Phase::Planning,
                revision: ev.rev_before,
                at_ms: ev.now,
                note: None,
            }],
        });
        if self.tasks.len() > MAX_TASKS {
            self.tasks.remove(0);
        }
        self.tasks.len() - 1
    }

    /// Fold one observed call into the session's active task (starting one if there is none).
    /// Returns the task's index when anything changed.
    pub fn observe(&mut self, ev: &Event) -> Option<usize> {
        let moved = ev.rev_after > ev.rev_before;
        let relevant = match &ev.kind {
            Kind::Design { ok, refusal, .. } => (*ok && moved) || refusal.is_some(),
            Kind::Checks { erc, drc } => erc.is_some() || drc.is_some(),
            Kind::Manufacturing => true,
        };
        if !relevant {
            return None;
        }
        let i = match self.active(ev.session, ev.project_id) {
            Some(i) => i,
            None => self.begin(ev, "Agent session work"),
        };
        let t = &mut self.tasks[i];
        let progress = t.progress();
        match &ev.kind {
            Kind::Design {
                commands,
                ok,
                refusal,
            } => {
                if *ok {
                    let phase = commands
                        .iter()
                        .filter_map(|c| command_phase(c, ev.symbols))
                        .max();
                    match phase {
                        Some(p) => {
                            if p <= Phase::SchematicCommitted {
                                t.last_erc_errors = None;
                            }
                            if p != Phase::SyncingPcb {
                                t.last_drc_errors = None;
                            }
                            t.set(p, ev.rev_after, ev.now, None);
                        }
                        None => t.revision = ev.rev_after,
                    }
                } else if let Some(why) = refusal {
                    t.set(Phase::Blocked, ev.rev_after, ev.now, Some(why.clone()));
                }
            }
            Kind::Checks { erc, drc } => {
                if let Some(n) = erc {
                    t.last_erc_errors = Some(*n);
                    // ERC moves the phase only while the schematic is still being finished.
                    if progress < Phase::SyncingPcb {
                        let note = format!("ERC: {n} error(s) at revision {}", ev.rev_after);
                        t.set(Phase::RunningErc, ev.rev_after, ev.now, Some(note));
                        if *n == 0 && ev.symbols > 0 {
                            let ok = "ERC clean; schematic approved for PCB synchronisation (not a correctness claim)".to_string();
                            t.set(Phase::SchematicApproved, ev.rev_after, ev.now, Some(ok));
                        }
                    }
                }
                if let Some(n) = drc {
                    t.last_drc_errors = Some(*n);
                    if progress >= Phase::SyncingPcb && progress < Phase::ManufacturingPreview {
                        let note = format!("DRC: {n} error(s) at revision {}", ev.rev_after);
                        t.set(Phase::RunningDrc, ev.rev_after, ev.now, Some(note));
                    }
                }
                t.revision = ev.rev_after;
            }
            Kind::Manufacturing => {
                let note = "draft manufacturing package compiled; not manufacturing-ready";
                t.set(
                    Phase::ManufacturingPreview,
                    ev.rev_after,
                    ev.now,
                    Some(note.into()),
                );
            }
        }
        t.updated_ms = ev.now;
        Some(i)
    }

    /// End the session's active task. `completed` is granted only when the recorded evidence
    /// supports it; otherwise the task ends `partial` and says why. Returns the ended task.
    pub fn finish(
        &mut self,
        session: &str,
        project_id: &str,
        outcome: &str,
        note: Option<String>,
        revision: u64,
        now: u64,
    ) -> Result<&AgentTask, String> {
        let i = self
            .active(session, project_id)
            .ok_or("this session has no active task")?;
        let t = &mut self.tasks[i];
        let (phase, detail) = match outcome {
            "cancelled" => (Phase::Cancelled, note),
            "failed" => (Phase::Failed, note),
            "blocked" => (Phase::Blocked, note),
            "partial" => (Phase::Partial, note),
            "completed" => match completion_gap(t) {
                None => (Phase::Completed, note),
                Some(gap) => (
                    Phase::Partial,
                    Some(format!("completion not supported by evidence: {gap}")),
                ),
            },
            other => {
                return Err(format!(
                "outcome must be completed, partial, blocked, failed or cancelled, got {other:?}"
            ))
            }
        };
        t.set(phase, revision, now, detail);
        t.ended_ms = Some(now);
        Ok(&self.tasks[i])
    }
}

/// Why `completed` cannot be granted, or `None` when the recorded evidence supports it.
fn completion_gap(t: &AgentTask) -> Option<String> {
    let p = t.progress();
    if p < Phase::SchematicApproved {
        return Some(format!(
            "the task never reached an ERC-clean schematic (furthest phase: {p:?})"
        ));
    }
    if t.last_erc_errors.is_some_and(|n| n > 0) {
        return Some("the latest ERC reported errors".into());
    }
    if p >= Phase::SyncingPcb {
        return match t.last_drc_errors {
            None => Some("no DRC ran after the last board change".into()),
            Some(n) if n > 0 => Some(format!("the latest DRC reported {n} error(s)")),
            Some(_) => None,
        };
    }
    None
}

pub fn path(dir: &Path) -> std::path::PathBuf {
    dir.join(TASK_FILE)
}

/// The ledger on disk; missing or unreadable means empty (it is a progress record, not state).
pub fn read(dir: &Path) -> Ledger {
    std::fs::read_to_string(path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write(dir: &Path, ledger: &Ledger) -> Result<(), String> {
    let json = serde_json::to_string_pretty(ledger).map_err(|e| e.to_string())?;
    store::atomic_write(&path(dir), json.as_bytes()).map_err(|e| e.to_string())
}

/// Cheap change marker for a project's ledger: (task count, newest update, phase entries).
pub fn stamp(dir: &Path) -> (usize, u64, usize) {
    let l = read(dir);
    (
        l.tasks.len(),
        l.tasks.iter().map(|t| t.updated_ms).max().unwrap_or(0),
        l.tasks.iter().map(|t| t.history.len()).sum(),
    )
}

fn now() -> u64 {
    store::now_ms() as u64
}

fn check_errors(out: &Value, name: &str) -> Option<usize> {
    out.pointer("/result/reports")?
        .as_array()?
        .iter()
        .find(|r| {
            r["check"]
                .as_str()
                .is_some_and(|c| c.eq_ignore_ascii_case(name))
        })
        .and_then(|r| r["errors"].as_u64())
        .map(|n| n as usize)
}

/// Command type names carried by a design tool call (`Batch` carries several).
fn command_names(tool: &str, args: &Value) -> Vec<String> {
    if tool == "Batch" {
        args.get("commands")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| c["type"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![tool.to_string()]
    }
}

/// Command names for the revisions in `(before, after]`, from the journal: the entry's own
/// command, or for a `Batch` the IR sections it changed (footprints = PCB sync, tracks/vias/zones
/// = routing, schematic = schematic edits).
fn journal_commands(dir: &Path, before: u64, after: u64) -> Vec<String> {
    let mut out = vec![];
    for e in store::read_journal(dir) {
        if e.state != EntryState::Pending || e.revision_after <= before || e.revision_after > after
        {
            continue;
        }
        if e.command != "Batch" {
            out.push(e.command.clone());
            continue;
        }
        for c in &e.changes {
            let p = |i: usize| c.path.get(i).map(String::as_str);
            let name = match (p(0), p(1)) {
                (Some("schematic"), _) => "PlaceSymbol",
                (Some("pcb"), Some("footprints")) => "SyncSchematicToPcb",
                (Some("pcb"), Some("tracks" | "vias" | "zones")) => "AddTrack",
                _ => continue,
            };
            out.push(name.to_string());
        }
    }
    out
}

/// Record one finished harness call (best effort: a ledger that cannot be written is reported
/// on stderr and never fails the call).
pub fn record(
    engine: &Engine,
    ctx: &CallContext,
    tool: &str,
    args: &Value,
    out: &Value,
    call_rev: u64,
) {
    let Some(dir) = engine.dir() else { return };
    let ok = out["ok"].as_bool().unwrap_or(false);
    let p = engine.project();
    let kind = match out["category"].as_str() {
        Some("design") => Kind::Design {
            commands: command_names(tool, args),
            ok,
            refusal: (out.pointer("/error/code").and_then(Value::as_str) == Some("precondition"))
                .then(|| out["error"]["message"].as_str().unwrap_or("").to_string()),
        },
        Some("validation") if ok && tool == "run_check" => Kind::Checks {
            erc: check_errors(out, "erc"),
            drc: check_errors(out, "drc"),
        },
        Some("manufacturing") if ok => Kind::Manufacturing,
        // Autonomous flow and rework apply paths: the commands come from the engine's own
        // journal for the revisions they committed, not from the tool name (ADR-0036).
        Some("flow" | "rework") if ok => Kind::Design {
            commands: journal_commands(dir, call_rev, p.revision),
            ok,
            refusal: None,
        },
        Some("flow" | "rework") => Kind::Design {
            commands: vec![],
            ok: false,
            refusal: (out.pointer("/error/code").and_then(Value::as_str) == Some("precondition"))
                .then(|| out["error"]["message"].as_str().unwrap_or("").to_string()),
        },
        _ => return,
    };
    let rev_after = p.revision;
    let rev_before = out
        .pointer("/result/revision_before")
        .and_then(Value::as_u64)
        .unwrap_or(call_rev);
    let ev = Event {
        session: &ctx.session_id,
        agent: &ctx.agent,
        project_id: &p.id,
        kind,
        rev_before,
        rev_after,
        symbols: p.schematic.symbols.len(),
        now: now(),
    };
    let mut ledger = read(dir);
    if ledger.observe(&ev).is_some() {
        if let Err(e) = write(dir, &ledger) {
            eprintln!("eak-harness: agent task ledger not written: {e}");
        }
    }
}

/// The evidence behind the PCB synchronisation gate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Gate {
    pub erc_revision: u64,
    pub erc_errors: usize,
}

/// Newest stored ERC evidence at or before `current`: (revision it ran at, errors it reported).
fn latest_erc(dir: &Path, current: u64) -> Option<(u64, usize)> {
    let mut best: Option<(u64, usize)> = None;
    for e in std::fs::read_dir(dir.join("evidence")).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let rev = name
            .strip_prefix("erc-r")
            .and_then(|r| r.strip_suffix(".json"))
            .and_then(|r| r.parse::<u64>().ok());
        let Some(rev) = rev.filter(|r| *r <= current) else {
            continue;
        };
        if best.is_some_and(|(b, _)| b >= rev) {
            continue;
        }
        let errors = std::fs::read_to_string(e.path())
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| v["errors"].as_u64());
        if let Some(n) = errors {
            best = Some((rev, n as usize));
        }
    }
    best
}

/// PCB synchronisation may start only after the schematic holds committed symbols and ERC has
/// run on it with no schematic or library change since. ERC *errors* do not block (the engineer
/// may sync a design with known ERC errors) but are returned and keep the task from `completed`.
pub fn pcb_sync_gate(engine: &Engine) -> Result<Gate, String> {
    let p = engine.project();
    if p.schematic.symbols.is_empty() {
        return Err("PCB synchronisation refused: the schematic has no committed symbols; place and commit the schematic first".into());
    }
    let dir = engine
        .dir()
        .ok_or("PCB synchronisation refused: the project has no folder to hold ERC evidence")?;
    let (rev, errors) = latest_erc(dir, p.revision).ok_or(
        "PCB synchronisation refused: ERC has not run on this project; run `run_check` with check=erc first",
    )?;
    let touched = store::read_journal(dir).iter().any(|e| {
        e.state == EntryState::Pending
            && e.revision_after > rev
            && e.revision_after <= p.revision
            && e.changes.iter().any(|c| {
                matches!(
                    c.path.first().map(String::as_str),
                    Some("schematic" | "library")
                )
            })
    });
    if touched {
        return Err(format!(
            "PCB synchronisation refused: the schematic changed after the last ERC (revision {rev}); run `run_check` with check=erc again"
        ));
    }
    Ok(Gate {
        erc_revision: rev,
        erc_errors: errors,
    })
}

fn fail(code: ErrorCode, msg: impl Into<String>) -> Fail {
    Fail(code, msg.into(), Value::Null)
}

/// `agent_task_begin`, `agent_task_finish` and `agent_task_status`.
pub(crate) fn dispatch(
    engine: &Engine,
    ctx: &CallContext,
    name: &str,
    args: &Value,
) -> Result<Value, Fail> {
    let dir = engine.dir().ok_or_else(|| {
        fail(
            ErrorCode::Precondition,
            "agent tasks need a saved project folder",
        )
    })?;
    let p = engine.project();
    let mut ledger = read(dir);
    let save = |l: &Ledger| write(dir, l).map_err(|e| fail(ErrorCode::Storage, e));
    match name {
        "agent_task_begin" => {
            let title = args
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| fail(ErrorCode::InvalidArguments, "title is required"))?;
            let ev = Event {
                session: &ctx.session_id,
                agent: &ctx.agent,
                project_id: &p.id,
                kind: Kind::Manufacturing, // unused: begin() reads only identity and revision
                rev_before: p.revision,
                rev_after: p.revision,
                symbols: p.schematic.symbols.len(),
                now: now(),
            };
            let i = match ledger.active(&ctx.session_id, &p.id) {
                Some(i) => {
                    ledger.tasks[i].title = title.to_string();
                    ledger.tasks[i].updated_ms = ev.now;
                    i
                }
                None => ledger.begin(&ev, title),
            };
            save(&ledger)?;
            Ok(json!({"task": ledger.tasks[i], "project_revision": p.revision}))
        }
        "agent_task_finish" => {
            let outcome = args.get("outcome").and_then(Value::as_str).unwrap_or("");
            let note = args
                .get("note")
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            let t = ledger
                .finish(&ctx.session_id, &p.id, outcome, note, p.revision, now())
                .map_err(|e| fail(ErrorCode::InvalidArguments, e))?
                .clone();
            save(&ledger)?;
            Ok(json!({"task": t, "project_revision": p.revision}))
        }
        "agent_task_status" => {
            let mine = ledger
                .active(&ctx.session_id, &p.id)
                .map(|i| ledger.tasks[i].clone());
            let recent: Vec<&AgentTask> = ledger
                .tasks
                .iter()
                .rev()
                .filter(|t| t.project_id == p.id)
                .take(10)
                .collect();
            Ok(json!({"session_task": mine, "recent": recent, "project_revision": p.revision}))
        }
        other => Err(fail(
            ErrorCode::UnknownTool,
            format!("no agent task tool named {other}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: Kind, before: u64, after: u64, symbols: usize) -> Event<'static> {
        Event {
            session: "s1",
            agent: "claude",
            project_id: "prj_1",
            kind,
            rev_before: before,
            rev_after: after,
            symbols,
            now: 1000 + after,
        }
    }

    fn design(cmds: &[&str]) -> Kind {
        Kind::Design {
            commands: cmds.iter().map(|c| c.to_string()).collect(),
            ok: true,
            refusal: None,
        }
    }

    fn erc(n: usize) -> Kind {
        Kind::Checks {
            erc: Some(n),
            drc: None,
        }
    }

    fn drc(n: usize) -> Kind {
        Kind::Checks {
            erc: None,
            drc: Some(n),
        }
    }

    #[test]
    fn phases_follow_observed_calls_and_record_the_starting_revision() {
        let mut l = Ledger::default();
        l.observe(&ev(design(&["AddSheet"]), 4, 5, 0)).unwrap();
        assert_eq!(l.tasks[0].starting_revision, 4);
        assert_eq!(l.tasks[0].phase, Phase::GeneratingSchematic);
        l.observe(&ev(design(&["PlaceSymbol"]), 5, 6, 1)).unwrap();
        assert_eq!(l.tasks[0].phase, Phase::SchematicCommitted);
        l.observe(&ev(erc(0), 6, 6, 1)).unwrap();
        assert_eq!(l.tasks[0].phase, Phase::SchematicApproved);
        l.observe(&ev(design(&["SyncSchematicToPcb"]), 6, 7, 1))
            .unwrap();
        l.observe(&ev(design(&["AutoPlace"]), 7, 8, 1)).unwrap();
        l.observe(&ev(design(&["AutoRoute"]), 8, 9, 1)).unwrap();
        assert_eq!(l.tasks[0].phase, Phase::Routing);
        l.observe(&ev(drc(0), 9, 9, 1)).unwrap();
        assert_eq!(l.tasks[0].phase, Phase::RunningDrc);
        assert_eq!(l.tasks.len(), 1);
        assert_eq!(l.tasks[0].revision, 9);
        assert_eq!(l.tasks[0].starting_revision, 4);
    }

    #[test]
    fn erc_errors_keep_the_schematic_unapproved() {
        let mut l = Ledger::default();
        l.observe(&ev(design(&["PlaceSymbol"]), 0, 1, 1)).unwrap();
        l.observe(&ev(erc(3), 1, 1, 1)).unwrap();
        assert_eq!(l.tasks[0].phase, Phase::RunningErc);
        assert_eq!(l.tasks[0].last_erc_errors, Some(3));
    }

    #[test]
    fn a_later_schematic_edit_withdraws_the_approval() {
        let mut l = Ledger::default();
        l.observe(&ev(design(&["PlaceSymbol"]), 0, 1, 1)).unwrap();
        l.observe(&ev(erc(0), 1, 1, 1)).unwrap();
        l.observe(&ev(design(&["AddWire"]), 1, 2, 1)).unwrap();
        assert_eq!(l.tasks[0].phase, Phase::SchematicCommitted);
        assert_eq!(l.tasks[0].last_erc_errors, None);
    }

    #[test]
    fn a_refused_gate_blocks_and_the_next_grounded_call_unblocks() {
        let mut l = Ledger::default();
        l.observe(&ev(design(&["PlaceSymbol"]), 0, 1, 1)).unwrap();
        let refused = Kind::Design {
            commands: vec!["SyncSchematicToPcb".into()],
            ok: false,
            refusal: Some("ERC has not run".into()),
        };
        l.observe(&ev(refused, 1, 1, 1)).unwrap();
        assert_eq!(l.tasks[0].phase, Phase::Blocked);
        assert_eq!(l.tasks[0].progress(), Phase::SchematicCommitted);
        l.observe(&ev(erc(0), 1, 1, 1)).unwrap();
        assert_eq!(l.tasks[0].phase, Phase::SchematicApproved);
    }

    #[test]
    fn calls_that_changed_nothing_create_no_task() {
        let mut l = Ledger::default();
        assert!(l.observe(&ev(design(&["PlaceSymbol"]), 3, 3, 1)).is_none());
        assert!(l.tasks.is_empty());
    }

    #[test]
    fn completed_needs_evidence_otherwise_partial() {
        let mut l = Ledger::default();
        l.observe(&ev(design(&["PlaceSymbol"]), 0, 1, 1)).unwrap();
        let t = l.finish("s1", "prj_1", "completed", None, 1, 2000).unwrap();
        assert_eq!(t.phase, Phase::Partial);
        assert!(t.detail.as_deref().unwrap().contains("ERC-clean"));

        let mut l = Ledger::default();
        l.observe(&ev(design(&["PlaceSymbol"]), 0, 1, 1)).unwrap();
        l.observe(&ev(erc(0), 1, 1, 1)).unwrap();
        let t = l.finish("s1", "prj_1", "completed", None, 1, 2000).unwrap();
        assert_eq!(t.phase, Phase::Completed);
        assert!(t.ended_ms.is_some());
        assert!(l.finish("s1", "prj_1", "completed", None, 1, 2001).is_err());
    }

    #[test]
    fn a_pcb_stage_task_needs_a_clean_drc_to_complete() {
        let mut l = Ledger::default();
        l.observe(&ev(design(&["PlaceSymbol"]), 0, 1, 1)).unwrap();
        l.observe(&ev(erc(0), 1, 1, 1)).unwrap();
        l.observe(&ev(design(&["SyncSchematicToPcb"]), 1, 2, 1))
            .unwrap();
        let t = l.finish("s1", "prj_1", "completed", None, 2, 2000).unwrap();
        assert_eq!(t.phase, Phase::Partial);
        assert!(t.detail.as_deref().unwrap().contains("DRC"));
    }

    #[test]
    fn sessions_do_not_share_tasks() {
        let mut l = Ledger::default();
        l.observe(&ev(design(&["PlaceSymbol"]), 0, 1, 1)).unwrap();
        let mut other = ev(design(&["PlaceSymbol"]), 1, 2, 1);
        other.session = "s2";
        l.observe(&other).unwrap();
        assert_eq!(l.tasks.len(), 2);
        assert_eq!(l.tasks[1].starting_revision, 1);
    }

    #[test]
    fn batch_names_and_check_counts_are_read_from_the_envelope() {
        let args = json!({"commands": [{"type": "PlaceSymbol"}, {"type": "SyncSchematicToPcb"}]});
        assert_eq!(
            command_names("Batch", &args),
            vec!["PlaceSymbol", "SyncSchematicToPcb"]
        );
        assert_eq!(command_names("AddWire", &json!({})), vec!["AddWire"]);
        let out = json!({"result": {"reports": [
            {"check": "ERC", "errors": 2}, {"check": "DRC", "errors": 0}]}});
        assert_eq!(check_errors(&out, "erc"), Some(2));
        assert_eq!(check_errors(&out, "drc"), Some(0));
        assert_eq!(check_errors(&out, "dfm"), None);
    }
}
