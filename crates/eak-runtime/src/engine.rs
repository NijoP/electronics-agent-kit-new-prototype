//! The shared command engine: one per open project. Every mutation — from the UI, the
//! CLI or the AI agent — goes through `Engine::execute`.
//!
//! Pipeline: revision check → approval policy → apply on a clone (transaction) →
//! integrity check → diff → journal(pending) → atomic persist → journal(committed) →
//! swap in-memory state → event.

use crate::command::{apply, Command, CommandError, OperationClass};
use crate::delta::{self, Change};
use crate::store::{self, EntryKind, EntryState, JournalEntry, ProjectLock, RecoveryReport};
use eak_core::{Id, Project};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    Human,
    Agent,
    Cli,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Actor {
    pub kind: ActorKind,
    pub name: String,
    /// Session the actor works in (e.g. the EAK terminal session an external coding agent
    /// runs in, ADR-0018). Recorded in the journal label so agent edits stay attributable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

impl Actor {
    pub fn human() -> Actor {
        Actor {
            kind: ActorKind::Human,
            name: "user".into(),
            session: None,
        }
    }
    pub fn agent(name: &str) -> Actor {
        Actor {
            kind: ActorKind::Agent,
            name: name.into(),
            session: None,
        }
    }
    /// An agent acting inside an identified session (harness calls).
    pub fn agent_in_session(name: &str, session: &str) -> Actor {
        Actor {
            session: Some(session.into()),
            ..Actor::agent(name)
        }
    }
    /// Journal label: `kind:name`, plus `@session` when the actor has one.
    pub fn label(&self) -> String {
        let base = format!("{:?}:{}", self.kind, self.name).to_lowercase();
        match &self.session {
            Some(s) => format!("{base}@{s}"),
            None => base,
        }
    }
}

/// Authorisation attached to a request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Approval {
    pub approved_by: String,
    /// Highest operation class this approval covers.
    pub max_class: OperationClass,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandRequest {
    pub expected_revision: u64,
    pub actor: Actor,
    pub command: Command,
    #[serde(default)]
    pub approval: Option<Approval>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandResponse {
    pub op_id: String,
    pub project_id: Id,
    pub command: String,
    pub class: OperationClass,
    pub accepted: bool,
    pub revision_before: u64,
    pub revision_after: u64,
    pub created: Vec<Id>,
    pub affected: Vec<String>,
    pub warnings: Vec<String>,
    pub error: Option<CommandError>,
    pub approval: Option<Approval>,
    /// Journal reference for undo.
    pub undo_ref: Option<String>,
    /// Command acknowledgement is not validation: verification runs separately.
    pub validation_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub project_id: Id,
    pub op_id: String,
    pub kind: String,
    pub actor: String,
    pub summary: String,
    pub revision: u64,
    pub timestamp_ms: u128,
}

const MAX_EVENTS: usize = 1000;
const MAX_UNDO: usize = 500;

pub struct Engine {
    project: Project,
    dir: Option<PathBuf>,
    _lock: Option<ProjectLock>,
    undo: Vec<(String, Vec<Change>)>,
    redo: Vec<(String, Vec<Change>)>,
    events: VecDeque<Event>,
    seq: u64,
    blocked: Option<String>,
    pub recovery: RecoveryReport,
}

fn op_id() -> String {
    eak_core::new_id("op")
}

impl Engine {
    /// In-memory engine (tests, previews). Nothing is persisted.
    pub fn in_memory(project: Project) -> Engine {
        Engine {
            project,
            dir: None,
            _lock: None,
            undo: vec![],
            redo: vec![],
            events: VecDeque::new(),
            seq: 0,
            blocked: None,
            recovery: RecoveryReport::default(),
        }
    }

    /// Create a new project folder. Refuses to overwrite an existing project.
    pub fn create(dir: &Path, name: &str) -> Result<Engine, String> {
        if dir.join(store::PROJECT_FILE).exists() {
            return Err(format!("{} already contains an EAK project", dir.display()));
        }
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let lock = store::acquire_lock(dir, false)?;
        let p = Project::new(name);
        store::write_project(dir, &p).map_err(|e| e.to_string())?;
        let mut e = Engine::in_memory(p);
        e.dir = Some(dir.to_path_buf());
        e._lock = Some(lock);
        e.emit(
            "project_created",
            "-",
            &Actor::human().label(),
            format!("created project {name}"),
        );
        Ok(e)
    }

    /// Open a project folder, reconcile interrupted operations and rebuild undo history.
    pub fn open(dir: &Path, take_over_lock: bool) -> Result<Engine, String> {
        let lock = store::acquire_lock(dir, take_over_lock)?;
        let project = store::read_project(dir)?;
        let recovery = store::recover(dir, &project).map_err(|e| e.to_string())?;
        let mut e = Engine::in_memory(project);
        e.dir = Some(dir.to_path_buf());
        e._lock = Some(lock);
        // Reconciliation records the UNKNOWN_OUTCOME in the journal, so a later open no
        // longer sees it as pending. The write block is therefore persisted separately and
        // only cleared by an explicit acknowledgement: reopening never lifts it silently.
        let block_file = dir.join(store::RECOVERY_BLOCK_FILE);
        let unknown: Vec<&str> = recovery
            .reconciled
            .iter()
            .filter(|(_, s)| *s == EntryState::UnknownOutcome)
            .map(|(op, _)| op.as_str())
            .collect();
        if !unknown.is_empty() {
            let reason = format!(
                "interrupted operation(s) {} have an unknown outcome; inspect the project and acknowledge recovery before editing",
                unknown.join(", ")
            );
            store::atomic_write(&block_file, reason.as_bytes())
                .map_err(|err| format!("cannot persist the recovery block: {err}"))?;
        }
        if block_file.exists() {
            let reason = std::fs::read_to_string(&block_file)
                .map_err(|err| format!("cannot read {}: {err}", block_file.display()))?;
            e.blocked = Some(reason);
        }
|(_, s)| *s == EntryState::UnknownOutcome)
        {
            e.blocked = Some("an interrupted operation has an unknown outcome; inspect the project and acknowledge recovery before editing".into());
        }
        e.recovery = recovery;
        e.rebuild_history();
        e.emit(
            "project_opened",
            "-",
            "system",
            format!("opened at revision {}", e.project.revision),
        );
        Ok(e)
    }

    fn rebuild_history(&mut self) {
        let Some(dir) = &self.dir else { return };
        let journal = store::read_journal(dir);
        let committed: std::collections::BTreeSet<&str> = journal
            .iter()
            .filter(|e| e.state == EntryState::Committed)
            .map(|e| e.op_id.as_str())
            .collect();
        for e in journal
            .iter()
            .filter(|e| e.state == EntryState::Pending && committed.contains(e.op_id.as_str()))
        {
            match e.kind {
                EntryKind::Do => {
                    self.undo.push((e.op_id.clone(), e.changes.clone()));
                    self.redo.clear();
                }
                EntryKind::Undo => {
                    if let Some(x) = self.undo.pop() {
                        self.redo.push(x);
                    }
                }
                EntryKind::Redo => {
                    if let Some(x) = self.redo.pop() {
                        self.undo.push(x);
                    }
                }
                EntryKind::Restore => {
                    self.undo.push((e.op_id.clone(), e.changes.clone()));
                    self.redo.clear();
                }
            }
        }
        let overflow = self.undo.len().saturating_sub(MAX_UNDO);
        self.undo.drain(..overflow);
    }

    pub fn project(&self) -> &Project {
        &self.project
    }
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn blocked_reason(&self) -> Option<&str> {
        self.blocked.as_deref()
    }
    /// The user accepts the on-disk state after an UNKNOWN_OUTCOME recovery; lifts the
    /// persisted write block.
    pub fn acknowledge_recovery(&mut self) -> Result<(), String> {
        if let Some(dir) = &self.dir {
            match std::fs::remove_file(dir.join(store::RECOVERY_BLOCK_FILE)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("cannot clear the recovery block: {e}")),
            }
        }
        self.blocked = None;
        self.emit(
            "recovery_acknowledged",
            "-",
            &Actor::human().label(),
            "user accepted the on-disk state after recovery".into(),
        );
        Ok(())
    }
    pub fn events_since(&self, seq: u64) -> Vec<Event> {
        self.events
            .iter()
            .filter(|e| e.seq > seq)
            .cloned()
            .collect()
    }

    fn emit(&mut self, kind: &str, op: &str, actor: &str, summary: String) {
        self.seq += 1;
        self.events.push_back(Event {
            seq: self.seq,
            project_id: self.project.id.clone(),
            op_id: op.into(),
            kind: kind.into(),
            actor: actor.into(),
            summary,
            revision: self.project.revision,
            timestamp_ms: store::now_ms(),
        });
        while self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }

    fn check_policy(req: &CommandRequest, class: OperationClass) -> Result<(), CommandError> {
        match req.actor.kind {
            // A human acting directly in the editor authorises their own edit; the UI
            // confirms destructive actions before submitting them.
            ActorKind::Human | ActorKind::Cli => Ok(()),
            ActorKind::Agent => match &req.approval {
                Some(a) if a.max_class >= class => Ok(()),
                _ if class == OperationClass::ReadOnly => Ok(()),
                _ => Err(CommandError::ApprovalRequired(format!(
                    "agent operation of class {class:?} needs an explicit approval"
                ))),
            },
        }
    }

    /// Persist a transaction: pending record → project file → committed record.
    fn commit(
        &mut self,
        next: Project,
        kind: EntryKind,
        target: Option<String>,
        op: &str,
        actor: &str,
        command: &str,
        changes: &[Change],
    ) -> Result<(), CommandError> {
        if let Some(dir) = self.dir.clone() {
            let mut entry = JournalEntry {
                op_id: op.into(),
                state: EntryState::Pending,
                kind,
                target_op: target,
                revision_before: self.project.revision,
                revision_after: next.revision,
                actor: actor.into(),
                command: command.into(),
                changes: changes.to_vec(),
                timestamp_ms: store::now_ms(),
            };
            store::append_journal(&dir, &entry)
                .map_err(|e| CommandError::Storage(e.to_string()))?;
            if let Err(e) = store::write_project(&dir, &next) {
                entry.state = EntryState::Aborted;
                entry.changes.clear();
                let _ = store::append_journal(&dir, &entry);
                return Err(CommandError::Storage(e.to_string()));
            }
            entry.state = EntryState::Committed;
            entry.changes.clear();
            store::append_journal(&dir, &entry).map_err(|e| {
                CommandError::Storage(format!("project saved but journal commit failed: {e}"))
            })?;
        }
        self.project = next;
        Ok(())
    }

    pub fn execute(&mut self, req: CommandRequest) -> CommandResponse {
        let op = op_id();
        let class = req.command.class();
        let name = req.command.name();
        let before_rev = self.project.revision;
        let mut resp = CommandResponse {
            op_id: op.clone(),
            project_id: self.project.id.clone(),
            command: name.clone(),
            class,
            accepted: false,
            revision_before: before_rev,
            revision_after: before_rev,
            created: vec![],
            affected: vec![],
            warnings: vec![],
            error: None,
            approval: req.approval.clone(),
            undo_ref: None,
            validation_note:
                "command accepted is not engineering validation; run ERC/DRC for evidence".into(),
        };
        let result =
            (|| -> Result<(Project, crate::command::Applied, Vec<Change>), CommandError> {
                if let Some(b) = &self.blocked {
                    return Err(CommandError::Precondition(b.clone()));
                }
                if req.expected_revision != self.project.revision {
                    return Err(CommandError::StaleRevision {
                        expected: req.expected_revision,
                        actual: self.project.revision,
                    });
                }
                Self::check_policy(&req, class)?;
                let mut next = self.project.clone();
                let applied = apply(&mut next, &req.command)?;
                let problems = eak_core::integrity::check(&next);
                if !problems.is_empty() {
                    return Err(CommandError::Integrity(problems.join("; ")));
                }
                let changes = delta::diff(&self.project, &next);
                if changes.is_empty() {
                    return Err(CommandError::NothingTo(format!("{name}: no change")));
                }
                next.revision += 1;
                Ok((next, applied, changes))
            })();
        match result {
            Ok((next, applied, changes)) => {
                let actor = req.actor.label();
                if let Err(e) = self.commit(next, EntryKind::Do, None, &op, &actor, &name, &changes)
                {
                    resp.error = Some(e);
                    self.emit(
                        "command_failed",
                        &op,
                        &actor,
                        format!("{name} failed to persist"),
                    );
                    return resp;
                }
                resp.accepted = true;
                resp.revision_after = self.project.revision;
                resp.created = applied.created;
                resp.warnings = applied.warnings;
                resp.affected = delta::affected_ids(&changes);
                resp.undo_ref = Some(op.clone());
                self.undo.push((op.clone(), changes));
                if self.undo.len() > MAX_UNDO {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.emit(
                    "command_applied",
                    &op,
                    &actor,
                    format!("{name} → revision {}", self.project.revision),
                );
            }
            Err(e) => {
                self.emit(
                    "command_rejected",
                    &op,
                    &req.actor.label(),
                    format!("{name}: {e}"),
                );
                resp.error = Some(e);
            }
        }
        resp
    }

    fn reverse(&mut self, expected_revision: u64, actor: &Actor, is_undo: bool) -> CommandResponse {
        let op = op_id();
        let what = if is_undo { "Undo" } else { "Redo" };
        let mut resp = CommandResponse {
            op_id: op.clone(),
            project_id: self.project.id.clone(),
            command: what.into(),
            class: OperationClass::ReversibleLocal,
            accepted: false,
            revision_before: self.project.revision,
            revision_after: self.project.revision,
            created: vec![],
            affected: vec![],
            warnings: vec![],
            error: None,
            approval: None,
            undo_ref: None,
            validation_note: "re-run ERC/DRC after undo/redo".into(),
        };
        if let Some(b) = &self.blocked {
            resp.error = Some(CommandError::Precondition(b.clone()));
            return resp;
        }
        if expected_revision != self.project.revision {
            resp.error = Some(CommandError::StaleRevision {
                expected: expected_revision,
                actual: self.project.revision,
            });
            return resp;
        }
        let stack = if is_undo {
            &mut self.undo
        } else {
            &mut self.redo
        };
        let Some((target, changes)) = stack.pop() else {
            resp.error = Some(CommandError::NothingTo(what.to_lowercase()));
            return resp;
        };
        let mut next = match delta::apply(&self.project, &changes, is_undo) {
            Ok(n) => n,
            Err(e) => {
                let stack = if is_undo {
                    &mut self.undo
                } else {
                    &mut self.redo
                };
                stack.push((target, changes));
                resp.error = Some(CommandError::Integrity(e));
                return resp;
            }
        };
        next.revision = self.project.revision + 1;
        let kind = if is_undo {
            EntryKind::Undo
        } else {
            EntryKind::Redo
        };
        if let Err(e) = self.commit(
            next,
            kind,
            Some(target.clone()),
            &op,
            &actor.label(),
            what,
            &[],
        ) {
            let stack = if is_undo {
                &mut self.undo
            } else {
                &mut self.redo
            };
            stack.push((target, changes));
            resp.error = Some(e);
            return resp;
        }
        resp.affected = delta::affected_ids(&changes);
        if is_undo {
            self.redo.push((target.clone(), changes));
        } else {
            self.undo.push((target.clone(), changes));
        }
        resp.accepted = true;
        resp.revision_after = self.project.revision;
        resp.undo_ref = Some(target.clone());
        self.emit(
            &what.to_lowercase(),
            &op,
            &actor.label(),
            format!("{what} of {target} → revision {}", self.project.revision),
        );
        resp
    }

    pub fn undo(&mut self, expected_revision: u64, actor: &Actor) -> CommandResponse {
        self.reverse(expected_revision, actor, true)
    }
    pub fn redo(&mut self, expected_revision: u64, actor: &Actor) -> CommandResponse {
        self.reverse(expected_revision, actor, false)
    }

    /// Save a named full snapshot.
    pub fn create_checkpoint(&mut self, name: &str) -> Result<String, String> {
        let dir = self
            .dir
            .clone()
            .ok_or("in-memory project has no checkpoints")?;
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let cp_dir = dir.join("checkpoints");
        std::fs::create_dir_all(&cp_dir).map_err(|e| e.to_string())?;
        let id = format!("r{:06}-{safe}", self.project.revision);
        store::atomic_write(
            &cp_dir.join(format!("{id}.json")),
            eak_core::to_json(&self.project).as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        self.emit(
            "checkpoint_created",
            "-",
            &Actor::human().label(),
            format!("checkpoint {id}"),
        );
        Ok(id)
    }

    pub fn list_checkpoints(&self) -> Vec<String> {
        let Some(dir) = &self.dir else { return vec![] };
        let mut v: Vec<String> = std::fs::read_dir(dir.join("checkpoints"))
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter_map(|e| {
                        e.file_name()
                            .to_str()
                            .and_then(|s| s.strip_suffix(".json"))
                            .map(String::from)
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Restore a checkpoint as a new, undoable revision (history is never rewritten).
    pub fn restore_checkpoint(
        &mut self,
        id: &str,
        expected_revision: u64,
        actor: &Actor,
    ) -> CommandResponse {
        let op = op_id();
        let mut resp = CommandResponse {
            op_id: op.clone(),
            project_id: self.project.id.clone(),
            command: "RestoreCheckpoint".into(),
            class: OperationClass::Destructive,
            accepted: false,
            revision_before: self.project.revision,
            revision_after: self.project.revision,
            created: vec![],
            affected: vec![],
            warnings: vec![],
            error: None,
            approval: None,
            undo_ref: None,
            validation_note: "re-run ERC/DRC after restore".into(),
        };
        if expected_revision != self.project.revision {
            resp.error = Some(CommandError::StaleRevision {
                expected: expected_revision,
                actual: self.project.revision,
            });
            return resp;
        }
        let Some(dir) = self.dir.clone() else {
            resp.error = Some(CommandError::Precondition("in-memory project".into()));
            return resp;
        };
        let snap = match std::fs::read_to_string(dir.join("checkpoints").join(format!("{id}.json")))
            .map_err(|e| e.to_string())
            .and_then(|s| eak_core::from_json(&s).map_err(|e| e.to_string()))
        {
            Ok(s) => s,
            Err(e) => {
                resp.error = Some(CommandError::NotFound(format!("checkpoint {id}: {e}")));
                return resp;
            }
        };
        if snap.id != self.project.id {
            resp.error = Some(CommandError::Precondition(
                "checkpoint belongs to another project".into(),
            ));
            return resp;
        }
        let changes = delta::diff(&self.project, &snap);
        let mut next = match delta::apply(&self.project, &changes, false) {
            Ok(n) => n,
            Err(e) => {
                resp.error = Some(CommandError::Integrity(e));
                return resp;
            }
        };
        next.revision = self.project.revision + 1;
        if let Err(e) = self.commit(
            next,
            EntryKind::Restore,
            None,
            &op,
            &actor.label(),
            "RestoreCheckpoint",
            &changes,
        ) {
            resp.error = Some(e);
            return resp;
        }
        resp.affected = delta::affected_ids(&changes);
        self.undo.push((op.clone(), changes));
        self.redo.clear();
        resp.accepted = true;
        resp.revision_after = self.project.revision;
        resp.undo_ref = Some(op.clone());
        self.emit(
            "checkpoint_restored",
            &op,
            &actor.label(),
            format!("restored {id} → revision {}", self.project.revision),
        );
        resp
    }

    /// Store a verification report under evidence/ (bound to its revision).
    pub fn save_evidence(&mut self, check: &str, json: &str) -> Option<PathBuf> {
        let dir = self.dir.clone()?;
        let ev = dir.join("evidence");
        std::fs::create_dir_all(&ev).ok()?;
        let path = ev.join(format!(
            "{}-r{:06}.json",
            check.to_lowercase(),
            self.project.revision
        ));
        store::atomic_write(&path, json.as_bytes()).ok()?;
        self.emit(
            "evidence_recorded",
            "-",
            "system",
            format!("{check} evidence for revision {}", self.project.revision),
        );
        Some(path)
    }

    /// Write one project record that is not design data (ADR-0039): supplier sourcing records
    /// and fabricator assembly classes under `<project>/sourcing/`. They are evidence about
    /// parts, not part of EAK-IR, so they carry no revision or undo step; instead every write is
    /// appended to the area's own journal (`<area>/journal.jsonl`) and emitted as an event.
    ///
    /// `file` is a `/`-separated path relative to the area, each component limited to
    /// letters, digits, `_`, `-` and `.` with no leading dot, ending in `.json`. Refused while
    /// the project is blocked by recovery, for projects without a folder, and for agent actors
    /// (an agent never records supplier or assembly evidence on its own).
    pub fn write_project_record(
        &mut self,
        actor: &Actor,
        area: &str,
        file: &str,
        data: &[u8],
        summary: &str,
    ) -> Result<PathBuf, CommandError> {
        if let Some(b) = &self.blocked {
            return Err(CommandError::Precondition(b.clone()));
        }
        if actor.kind == ActorKind::Agent {
            return Err(CommandError::ApprovalRequired(
                "agents do not record sourcing or assembly evidence; a person does".into(),
            ));
        }
        if !RECORD_AREAS.contains(&area) {
            return Err(CommandError::Invalid(format!(
                "{area:?} is not a project record area"
            )));
        }
        let parts = record_path(file)?;
        let dir = self.dir.clone().ok_or_else(|| {
            CommandError::Precondition("recording this needs a saved project folder".into())
        })?;
        let area_dir = dir.join(area);
        let path = parts.iter().fold(area_dir.clone(), |p, c| p.join(c));
        let storage = |e: std::io::Error| CommandError::Storage(e.to_string());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(storage)?;
        }
        store::atomic_write(&path, data).map_err(storage)?;
        let op = op_id();
        let entry = RecordWrite {
            op_id: op.clone(),
            actor: actor.label(),
            area: area.into(),
            file: parts.join("/"),
            bytes: data.len() as u64,
            revision: self.project.revision,
            summary: summary.into(),
            timestamp_ms: store::now_ms(),
        };
        let mut line =
            serde_json::to_string(&entry).map_err(|e| CommandError::Storage(e.to_string()))?;
        line.push('\n');
        {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(area_dir.join(RECORD_JOURNAL))
                .and_then(|mut f| f.write_all(line.as_bytes()))
                .map_err(storage)?;
        }
        self.emit(
            "project_record_written",
            &op,
            &actor.label(),
            format!("{area}/{}: {summary}", entry.file),
        );
        Ok(path)
    }
}

/// Project sub-folders written only through [`Engine::write_project_record`] (ADR-0039).
pub const RECORD_AREAS: &[&str] = &["sourcing"];
/// Journal file inside each record area.
pub const RECORD_JOURNAL: &str = "journal.jsonl";

/// One line of a record area's journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordWrite {
    pub op_id: String,
    pub actor: String,
    pub area: String,
    pub file: String,
    pub bytes: u64,
    /// Design revision the project was at when the record was written.
    pub revision: u64,
    pub summary: String,
    pub timestamp_ms: u128,
}

/// Every entry of `<dir>/<area>/journal.jsonl`, oldest first; unreadable lines are skipped.
pub fn read_record_journal(dir: &Path, area: &str) -> Vec<RecordWrite> {
    std::fs::read_to_string(dir.join(area).join(RECORD_JOURNAL))
        .map(|t| {
            t.lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn record_path(file: &str) -> Result<Vec<String>, CommandError> {
    let bad = || CommandError::Invalid(format!("{file:?} is not a usable record file name"));
    let parts: Vec<String> = file.split('/').map(str::to_string).collect();
    if parts.len() > 4 || !file.ends_with(".json") {
        return Err(bad());
    }
    for c in &parts {
        let ok = !c.is_empty()
            && c.len() <= 80
            && !c.starts_with('.')
            && c.chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'));
        if !ok {
            return Err(bad());
        }
    }
    Ok(parts)
}
