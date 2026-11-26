//! Proposals (ADR-0051): what a harness session in propose mode builds instead of editing.
//!
//! A session whose context says `apply_mode = propose` never executes a design command. Each
//! design tool call is checked on a copy of the project (the same `command::apply` and integrity
//! check as `Engine::execute`) and appended as a step to the session's open proposal, stored as
//! `<project>/proposals/<id>.json`. The project and its revision do not change.
//!
//! A proposal is reviewed as its steps (one `Batch` when applied), the delta each step and the
//! whole selection make on a copy, the ERC and DRC result on that copy against the project as it
//! is (findings that appear, findings resolved, rules newly UNKNOWN) and evidence labels. A
//! review is never stored: it is recomputed from the project every time, so it is EAK's, not
//! the agent's.
//!
//! Only a person applies ([`apply`] refuses agent actors). Apply is refused unless the project is
//! at the proposal's base revision; it saves a checkpoint, then executes the chosen steps as one
//! `Batch` through `Engine::execute`: one revision, one undo step.
//!
//! Object ids: a step that creates objects gets random ids, and a later step may name them (the
//! agent was shown them). Each step records the ids it allocated on the copy, and every re-run
//! (review and apply) replays them through [`eak_core::with_id_script`], so the objects keep the
//! ids the agent saw. A re-run that allocates other ids is refused, never applied.

use crate::context::CallContext;
use crate::design::Label;
use crate::tools::{command_error, ErrorCode, Fail};
use eak_core::{Id, Project};
use eak_runtime::command::{self, Command, CommandError, OperationClass};
use eak_runtime::delta::{self, Change};
use eak_runtime::store;
use eak_runtime::{Actor, ActorKind, Approval, CommandRequest, CommandResponse, Engine};
use eak_validation::Report;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Project sub-folder holding one JSON file per proposal and the proposals journal.
pub const PROPOSALS_DIR: &str = "proposals";
const JOURNAL: &str = "journal.jsonl";
/// Most steps one proposal may hold: it is applied as one `Batch`.
pub const MAX_STEPS: usize = command::MAX_BATCH;
/// Longest title or rationale kept.
const MAX_TEXT: usize = 2000;

/// How a harness session's design tools act (ADR-0051).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyMode {
    /// Design tools execute at once under the session's standing approval (ADR-0018).
    #[default]
    Apply,
    /// Design tools add steps to a proposal; nothing changes until a person applies it.
    Propose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Takes steps from its session.
    Open,
    /// The agent marked it ready for review; it takes no more steps.
    Ready,
    Applied,
    Discarded,
}

impl Status {
    fn reviewable(self) -> bool {
        matches!(self, Status::Open | Status::Ready)
    }
    fn name(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Ready => "ready",
            Status::Applied => "applied",
            Status::Discarded => "discarded",
        }
    }
}

/// One proposed command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Harness tool the step came from (`PlaceSymbol`, `design_apply`, ...).
    pub tool: String,
    pub command: Command,
    pub class: OperationClass,
    /// Ids the step allocated on the copy; replayed by every re-run.
    #[serde(default)]
    pub ids: Vec<Id>,
}

/// What Apply did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Applied {
    pub revision_before: u64,
    pub revision_after: u64,
    pub op_id: String,
    /// Checkpoint saved just before the apply (restore it to go back).
    pub checkpoint: String,
    /// Step indices applied (all of them, or the person's selection).
    pub steps: Vec<usize>,
    pub by: String,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub project_id: Id,
    pub session_id: String,
    pub agent: String,
    pub title: String,
    /// The agent's reason for the change. Shown as assumed, never as evidence.
    #[serde(default)]
    pub rationale: String,
    /// Revision the steps were planned and checked against; apply only at this revision.
    pub base_revision: u64,
    pub status: Status,
    pub steps: Vec<Step>,
    pub created_ms: u64,
    pub updated_ms: u64,
    #[serde(default)]
    pub applied: Option<Applied>,
    /// Who discarded it.
    #[serde(default)]
    pub discarded_by: Option<String>,
}

/// The effect of one step on the copy.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepEffect {
    /// Index into the proposal's steps.
    pub index: usize,
    pub tool: String,
    pub command: String,
    pub class: OperationClass,
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
    pub created: Vec<Id>,
    /// Earlier steps creating an object this step names: it cannot be applied without them.
    pub requires: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Counts {
    pub errors: usize,
    pub warnings: usize,
    /// Rules that could not be evaluated (`UNKNOWN`, never a pass).
    pub unknown_rules: usize,
}

/// One check run on the project as it is and on the copy with the selected steps.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckDelta {
    pub check: String,
    pub before: Counts,
    pub after: Counts,
    /// Findings on the copy that the project does not have.
    pub appeared: Vec<eak_validation::Finding>,
    /// Findings of the project that the copy no longer has.
    pub resolved: Vec<eak_validation::Finding>,
    pub unchanged: usize,
    /// `RULE: reason` for rules UNKNOWN on the copy but not on the project.
    pub newly_unknown: Vec<String>,
    pub not_implemented: Vec<String>,
    pub limitations: Vec<String>,
    pub label: Label,
}

/// An evidence line of a review, labelled by EAK.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evidence {
    pub subject: String,
    pub label: Label,
    pub basis: String,
}

/// A proposal as EAK sees it now. Recomputed on every call, never stored.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Review {
    pub proposal: Proposal,
    pub current_revision: u64,
    /// The project moved on since the proposal's base revision: it cannot be applied.
    pub stale: bool,
    /// Step indices this review covers (all, or the requested selection).
    pub selected: Vec<usize>,
    pub steps: Vec<StepEffect>,
    /// Total delta of the selection on the copy (what Apply would journal).
    pub changes: Vec<Change>,
    pub checks: Vec<CheckDelta>,
    pub evidence: Vec<Evidence>,
    /// Why the selection cannot be applied as it stands, or `None`.
    pub problem: Option<String>,
    pub note: &'static str,
}

const REVIEW_NOTE: &str = "computed by EAK on a copy of the project; nothing was applied. ERC/DRC report implemented rules only: no errors does not prove the design correct, and EAK never calls a design manufacturing-ready";

/// What [`apply`] returns.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApplyOutcome {
    pub response: CommandResponse,
    pub checkpoint: String,
    pub proposal: Proposal,
}

fn now() -> u64 {
    store::now_ms() as u64
}

fn precondition(m: impl Into<String>) -> CommandError {
    CommandError::Precondition(m.into())
}

fn project_dir(engine: &Engine) -> Result<PathBuf, CommandError> {
    engine
        .dir()
        .map(Path::to_path_buf)
        .ok_or_else(|| precondition("proposals need a saved project folder"))
}

fn valid_id(id: &str) -> bool {
    id.len() <= 64
        && id.starts_with("prop-")
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn clip(s: &str) -> String {
    s.trim().chars().take(MAX_TEXT).collect()
}

fn not_reviewable(p: &Proposal) -> CommandError {
    precondition(format!("proposal {} is {}", p.id, p.status.name()))
}

/// One stored proposal of a project folder.
pub fn load(project_dir: &Path, id: &str) -> Result<Proposal, CommandError> {
    if !valid_id(id) {
        return Err(CommandError::Invalid(format!(
            "{id:?} is not a proposal id"
        )));
    }
    let path = project_dir.join(PROPOSALS_DIR).join(format!("{id}.json"));
    let text = std::fs::read_to_string(path)
        .map_err(|_| CommandError::NotFound(format!("proposal {id}")))?;
    serde_json::from_str(&text)
        .map_err(|e| CommandError::Storage(format!("proposal {id} is unreadable: {e}")))
}

/// Every stored proposal of a project folder, oldest first. Unreadable files are skipped.
pub fn list(project_dir: &Path) -> Vec<Proposal> {
    let mut v: Vec<Proposal> = std::fs::read_dir(project_dir.join(PROPOSALS_DIR))
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let id = name.strip_suffix(".json")?.to_string();
                    load(project_dir, &id).ok()
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| (a.created_ms, &a.id).cmp(&(b.created_ms, &b.id)));
    v
}

fn save(engine: &Engine, p: &Proposal, event: &str, actor: &str) -> Result<(), CommandError> {
    let dir = project_dir(engine)?.join(PROPOSALS_DIR);
    let storage = |e: std::io::Error| CommandError::Storage(e.to_string());
    std::fs::create_dir_all(&dir).map_err(storage)?;
    let text = serde_json::to_string_pretty(p).map_err(|e| CommandError::Storage(e.to_string()))?;
    store::atomic_write(&dir.join(format!("{}.json", p.id)), text.as_bytes()).map_err(storage)?;
    let line = json!({
        "ts_ms": now(), "proposal_id": p.id, "event": event, "actor": actor,
        "status": p.status, "steps": p.steps.len(), "base_revision": p.base_revision,
        "revision": engine.project().revision,
    });
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(JOURNAL))
        .and_then(|mut f| writeln!(f, "{line}"))
        .map_err(storage)
}

/// The proposal a session adds steps to: its latest `Open` one on this project.
fn open_for_session(engine: &Engine, ctx: &CallContext) -> Option<Proposal> {
    let dir = engine.dir()?;
    list(dir).into_iter().rev().find(|p| {
        p.status == Status::Open
            && p.session_id == ctx.session_id
            && p.project_id == engine.project().id
    })
}

fn new_proposal(engine: &Engine, ctx: &CallContext, title: &str) -> Proposal {
    let u = uuid::Uuid::new_v4().simple().to_string();
    let t = now();
    Proposal {
        id: format!("prop-{}", &u[..12]),
        project_id: engine.project().id.clone(),
        session_id: ctx.session_id.clone(),
        agent: ctx.agent.clone(),
        title: if title.trim().is_empty() {
            format!("Changes proposed by {}", ctx.agent)
        } else {
            clip(title)
        },
        rationale: String::new(),
        base_revision: engine.project().revision,
        status: Status::Open,
        steps: vec![],
        created_ms: t,
        updated_ms: t,
        applied: None,
        discarded_by: None,
    }
}

// ---------------------------------------------------------------- simulation

struct Sim {
    project: Project,
    effects: Vec<StepEffect>,
}

/// Run the `chosen` steps in order on a copy of `base`, as a `Batch` would (the same
/// `command::apply` on one working copy, then the integrity check), each replaying its recorded
/// ids. A step that allocates other ids than it recorded is refused: the agent's later steps and
/// the review would no longer name the same objects.
fn simulate(base: &Project, steps: &[Step], chosen: &[usize]) -> Result<Sim, CommandError> {
    let mut p = base.clone();
    let mut effects = vec![];
    for &i in chosen {
        let s = &steps[i];
        let before = p.clone();
        let (res, issued) = eak_core::with_id_script(&s.ids, || command::apply(&mut p, &s.command));
        let applied = res.map_err(|e| step_error(i, s, e))?;
        if issued != s.ids {
            return Err(precondition(format!(
                "step {} ({}) no longer creates the objects it was proposed with; review it again with the steps it depends on",
                i + 1,
                s.command.name()
            )));
        }
        effects.push(StepEffect {
            index: i,
            tool: s.tool.clone(),
            command: s.command.name(),
            class: s.class,
            changes: delta::diff(&before, &p),
            warnings: applied.warnings,
            created: applied.created,
            requires: requires(steps, i),
        });
    }
    let problems = eak_core::integrity::check(&p);
    if !problems.is_empty() {
        return Err(CommandError::Integrity(problems.join("; ")));
    }
    Ok(Sim {
        project: p,
        effects,
    })
}

/// Earlier steps that create an object step `i` names (by the id the agent was shown).
fn requires(steps: &[Step], i: usize) -> Vec<usize> {
    let text = serde_json::to_string(&steps[i].command).unwrap_or_default();
    (0..i)
        .filter(|&j| steps[j].ids.iter().any(|id| text.contains(id.as_str())))
        .collect()
}

fn step_error(i: usize, s: &Step, e: CommandError) -> CommandError {
    let msg = format!("step {} ({}): {e}", i + 1, s.command.name());
    match e {
        CommandError::NotFound(_) => CommandError::NotFound(msg),
        CommandError::Invalid(_) => CommandError::Invalid(msg),
        CommandError::Integrity(_) => CommandError::Integrity(msg),
        CommandError::ApprovalRequired(_) => CommandError::ApprovalRequired(msg),
        _ => CommandError::Precondition(msg),
    }
}

fn all_steps(p: &Proposal) -> Vec<usize> {
    (0..p.steps.len()).collect()
}

/// A selection: sorted and unique, in range, not empty, and holding every step a selected step
/// depends on.
fn selection(p: &Proposal, steps: Option<&[usize]>) -> Result<Vec<usize>, CommandError> {
    let Some(s) = steps else {
        return Ok(all_steps(p));
    };
    let set: BTreeSet<usize> = s.iter().copied().collect();
    if set.is_empty() {
        return Err(CommandError::Invalid("no step selected".into()));
    }
    if let Some(bad) = set.iter().find(|&&i| i >= p.steps.len()) {
        return Err(CommandError::Invalid(format!(
            "step {} does not exist (the proposal has {})",
            bad + 1,
            p.steps.len()
        )));
    }
    for &i in &set {
        if let Some(m) = requires(&p.steps, i).into_iter().find(|m| !set.contains(m)) {
            return Err(precondition(format!(
                "step {} ({}) uses an object step {} creates: select step {} too",
                i + 1,
                p.steps[i].command.name(),
                m + 1,
                m + 1
            )));
        }
    }
    Ok(set.into_iter().collect())
}

// ---------------------------------------------------------------- check delta

fn unknown_rules(r: &Report) -> Vec<(String, String)> {
    r.rule_results
        .iter()
        .filter(|x| {
            x.state == eak_validation::CheckState::Unknown
                || x.default_state == Some(eak_validation::CheckState::Unknown)
        })
        .map(|x| {
            let why = if x.reason.is_empty() {
                "not evaluated".to_string()
            } else {
                x.reason.clone()
            };
            (x.rule_id.clone(), why)
        })
        .collect()
}

fn counts(r: &Report) -> Counts {
    Counts {
        errors: r.errors,
        warnings: r.warnings,
        unknown_rules: unknown_rules(r).len(),
    }
}

fn finding_key(f: &eak_validation::Finding) -> String {
    if f.id.is_empty() {
        format!("{}|{}|{:?}", f.rule_id, f.message, f.objects)
    } else {
        f.id.clone()
    }
}

/// What changes between a check on the project (`before`) and on the copy (`after`).
pub fn check_delta(before: &Report, after: &Report) -> CheckDelta {
    let bk: BTreeSet<String> = before.findings.iter().map(finding_key).collect();
    let ak: BTreeSet<String> = after.findings.iter().map(finding_key).collect();
    let only = |r: &Report, other: &BTreeSet<String>| {
        r.findings
            .iter()
            .filter(|f| !other.contains(&finding_key(f)))
            .cloned()
            .collect::<Vec<_>>()
    };
    let was_unknown: BTreeSet<String> = unknown_rules(before).into_iter().map(|(r, _)| r).collect();
    let after_counts = counts(after);
    CheckDelta {
        check: after.check.clone(),
        before: counts(before),
        after: after_counts,
        appeared: only(after, &bk),
        resolved: only(before, &ak),
        unchanged: bk.intersection(&ak).count(),
        newly_unknown: unknown_rules(after)
            .into_iter()
            .filter(|(r, _)| !was_unknown.contains(r))
            .map(|(r, why)| format!("{r}: {why}"))
            .collect(),
        not_implemented: after.not_implemented.clone(),
        limitations: after.limitations.clone(),
        // UNKNOWN is never a pass (ADR-0048): only a copy with no error and no unknown rule
        // passes the implemented deterministic checks.
        label: if after_counts.errors == 0 && after_counts.unknown_rules == 0 {
            Label::PassedDeterministicChecks
        } else {
            Label::Unverified
        },
    }
}

fn checks(base: &Project, copy: &Project) -> Vec<CheckDelta> {
    vec![
        check_delta(
            &eak_validation::erc::run(base),
            &eak_validation::erc::run(copy),
        ),
        check_delta(
            &eak_validation::drc::run(base),
            &eak_validation::drc::run(copy),
        ),
    ]
}

fn evidence(p: &Proposal, base_revision: u64, checks: &[CheckDelta]) -> Vec<Evidence> {
    let mut v: Vec<Evidence> = checks
        .iter()
        .map(|c| Evidence {
            subject: format!("{} on the proposed copy", c.check),
            label: c.label,
            basis: format!(
                "eak-validation {} on a copy of revision {base_revision} with the selected steps: {} error(s) ({} before), {} rule(s) UNKNOWN; implemented rules only",
                c.check, c.after.errors, c.before.errors, c.after.unknown_rules
            ),
        })
        .collect();
    if !p.rationale.is_empty() {
        v.push(Evidence {
            subject: "the agent's rationale".into(),
            label: Label::Assumed,
            basis: format!("stated by {}; EAK has not checked it", p.agent),
        });
    }
    v
}

// ---------------------------------------------------------------- review

/// Review a stored proposal against the engine's project: the selection's delta, check delta
/// and evidence, all recomputed on a copy. Read-only. A proposal that cannot be applied as it
/// stands (stale, discarded, failing on the copy, bad selection) is still returned, with the
/// reason in `problem`.
pub fn review(engine: &Engine, id: &str, steps: Option<&[usize]>) -> Result<Review, CommandError> {
    let p = load(&project_dir(engine)?, id)?;
    let base = engine.project();
    let mut out = Review {
        current_revision: base.revision,
        stale: p.base_revision != base.revision,
        selected: steps.map_or_else(|| all_steps(&p), <[usize]>::to_vec),
        steps: vec![],
        changes: vec![],
        checks: vec![],
        evidence: vec![],
        problem: None,
        note: REVIEW_NOTE,
        proposal: p,
    };
    let p = &out.proposal;
    let problem = if p.project_id != base.id {
        Some("the proposal belongs to another project".to_string())
    } else if !p.status.reviewable() {
        Some(not_reviewable(p).to_string())
    } else if out.stale {
        Some(format!(
            "the project is at revision {} but the proposal was made at revision {}: it can no longer be applied; discard it and ask for a new one",
            base.revision, p.base_revision
        ))
    } else if p.steps.is_empty() {
        Some("the proposal has no steps yet".to_string())
    } else {
        None
    };
    if problem.is_some() {
        out.problem = problem;
        return Ok(out);
    }
    // Every step's own effect, then the selection's.
    match simulate(base, &p.steps, &all_steps(p)) {
        Ok(full) => out.steps = full.effects,
        Err(e) => {
            out.problem = Some(format!(
                "the proposal no longer applies on this revision: {e}"
            ));
            return Ok(out);
        }
    }
    let chosen = match selection(p, steps) {
        Ok(c) => c,
        Err(e) => {
            out.problem = Some(e.to_string());
            return Ok(out);
        }
    };
    let sim = match simulate(base, &p.steps, &chosen) {
        Ok(s) => s,
        Err(e) => {
            out.problem = Some(format!("the selected steps do not apply on their own: {e}"));
            return Ok(out);
        }
    };
    out.selected = chosen;
    out.changes = delta::diff(base, &sim.project);
    if out.changes.is_empty() {
        out.problem = Some("the selected steps change nothing".into());
    }
    out.checks = checks(base, &sim.project);
    out.evidence = evidence(p, base.revision, &out.checks);
    Ok(out)
}

// ---------------------------------------------------------------- apply / discard

/// Apply a proposal (all steps, or the selection `steps`) as one `Batch` through
/// `Engine::execute`, after saving a checkpoint. Only a person applies: an agent actor is
/// refused. Refused unless `expected_revision` is the project's revision and the proposal's base
/// revision. The journal names the proposing agent as actor and `by` as the approver.
pub fn apply(
    engine: &mut Engine,
    id: &str,
    expected_revision: u64,
    steps: Option<&[usize]>,
    by: &Actor,
) -> Result<ApplyOutcome, CommandError> {
    if by.kind == ActorKind::Agent {
        return Err(CommandError::ApprovalRequired(
            "a proposal is applied by a person from the review, never by an agent".into(),
        ));
    }
    let mut p = load(&project_dir(engine)?, id)?;
    if p.project_id != engine.project().id {
        return Err(precondition("the proposal belongs to another project"));
    }
    if !p.status.reviewable() {
        return Err(not_reviewable(&p));
    }
    if let Some(b) = engine.blocked_reason() {
        return Err(precondition(b.to_string()));
    }
    let actual = engine.project().revision;
    if expected_revision != actual {
        return Err(CommandError::StaleRevision {
            expected: expected_revision,
            actual,
        });
    }
    if p.base_revision != actual {
        return Err(CommandError::StaleRevision {
            expected: p.base_revision,
            actual,
        });
    }
    if p.steps.is_empty() {
        return Err(CommandError::NothingTo(
            "apply: the proposal has no steps".into(),
        ));
    }
    let chosen = selection(&p, steps)?;
    // The computation the review showed, refused before anything is written if it fails.
    simulate(engine.project(), &p.steps, &chosen)?;
    let replay: Vec<Id> = chosen
        .iter()
        .flat_map(|&i| p.steps[i].ids.clone())
        .collect();
    let commands: Vec<Command> = chosen.iter().map(|&i| p.steps[i].command.clone()).collect();
    let max_class = chosen
        .iter()
        .map(|&i| p.steps[i].class)
        .max()
        .unwrap_or(OperationClass::ReversibleLocal);
    let label: String = format!("{} ({id})", p.title).chars().take(200).collect();
    let checkpoint = engine
        .create_checkpoint(&format!("before-{id}"))
        .map_err(CommandError::Storage)?;
    let request = CommandRequest {
        expected_revision,
        actor: Actor::agent_in_session(&p.agent, &p.session_id),
        command: Command::Batch {
            commands,
            label: Some(label),
        },
        approval: Some(Approval {
            approved_by: by.label(),
            max_class,
            note: format!("applied by {} from the review of proposal {id}", by.label()),
        }),
    };
    let (resp, issued) = eak_core::with_id_script(&replay, || engine.execute(request));
    if let Some(e) = resp.error.clone() {
        return Err(e);
    }
    // The objects must be the ones the review showed; otherwise take the change back at once.
    let issued: Vec<Id> = issued
        .into_iter()
        .filter(|i| !i.starts_with("op_"))
        .collect();
    if issued != replay {
        let back = engine.undo(resp.revision_after, by);
        return Err(CommandError::Integrity(format!(
            "the applied objects did not match the review, so the change was undone (undo accepted: {})",
            back.accepted
        )));
    }
    p.status = Status::Applied;
    p.updated_ms = now();
    p.applied = Some(Applied {
        revision_before: resp.revision_before,
        revision_after: resp.revision_after,
        op_id: resp.op_id.clone(),
        checkpoint: checkpoint.clone(),
        steps: chosen,
        by: by.label(),
        ts_ms: p.updated_ms,
    });
    let mut response = resp;
    if let Err(e) = save(engine, &p, "applied", &by.label()) {
        response.warnings.push(format!(
            "the change is applied but the proposal record was not updated: {e}"
        ));
    }
    Ok(ApplyOutcome {
        response,
        checkpoint,
        proposal: p,
    })
}

/// Discard a proposal. A person may discard any; an agent only its own session's.
pub fn discard(
    engine: &Engine,
    id: &str,
    by: &Actor,
    session: Option<&str>,
) -> Result<Proposal, CommandError> {
    let mut p = load(&project_dir(engine)?, id)?;
    if by.kind == ActorKind::Agent && Some(p.session_id.as_str()) != session {
        return Err(CommandError::ApprovalRequired(
            "an agent discards only its own session's proposals".into(),
        ));
    }
    if !p.status.reviewable() {
        return Err(not_reviewable(&p));
    }
    p.status = Status::Discarded;
    p.updated_ms = now();
    p.discarded_by = Some(by.label());
    save(engine, &p, "discarded", &by.label())?;
    Ok(p)
}

// ---------------------------------------------------------------- harness (propose mode)

fn fail(e: &CommandError) -> Fail {
    command_error(e)
}

/// A design tool call in propose mode: check `command` on a copy and add it to the session's
/// open proposal (starting one at the current revision). Never changes the project.
pub(crate) fn propose(
    engine: &Engine,
    ctx: &CallContext,
    tool: &str,
    command: Command,
    expected: u64,
) -> Result<Value, Fail> {
    if let Some(b) = engine.blocked_reason() {
        return Err(fail(&precondition(b.to_string())));
    }
    let current = engine.project().revision;
    if expected != current {
        return Err(fail(&CommandError::StaleRevision {
            expected,
            actual: current,
        }));
    }
    let mut p = match open_for_session(engine, ctx) {
        Some(p) if p.base_revision != current => {
            return Err(Fail(
                ErrorCode::StaleRevision,
                format!(
                    "the project moved to revision {current} after proposal {} was started at revision {}; discard it (proposal_discard) and propose again",
                    p.id, p.base_revision
                ),
                json!({"proposal_id": p.id, "proposal_base_revision": p.base_revision, "current_revision": current}),
            ))
        }
        Some(p) => p,
        None => new_proposal(engine, ctx, ""),
    };
    // A batch becomes its steps: the proposal is applied as one batch, and batches do not nest.
    let commands = match command {
        Command::Batch { commands, .. } => commands,
        c => vec![c],
    };
    if p.steps.len() + commands.len() > MAX_STEPS {
        return Err(fail(&CommandError::Invalid(format!(
            "a proposal holds at most {MAX_STEPS} steps"
        ))));
    }
    for c in &commands {
        if matches!(c, Command::Batch { .. }) {
            return Err(fail(&CommandError::Invalid("batches do not nest".into())));
        }
        // The session's standing policy bounds what it may propose, as it bounds what it may do.
        let class = c.class();
        if class > ctx.max_class {
            return Err(fail(&CommandError::ApprovalRequired(format!(
                "{} is {class:?}; this session may propose up to {:?}",
                c.name(),
                ctx.max_class
            ))));
        }
    }
    // Earlier steps first (they must still apply), then the new ones, recording their ids.
    let first_new = p.steps.len();
    let mut sim = simulate(engine.project(), &p.steps, &all_steps(&p)).map_err(|e| fail(&e))?;
    let mut created = vec![];
    let mut changes = vec![];
    let mut warnings = vec![];
    for (k, c) in commands.into_iter().enumerate() {
        let before = sim.project.clone();
        let (res, ids) = eak_core::with_id_script(&[], || command::apply(&mut sim.project, &c));
        let step = Step {
            tool: tool.into(),
            class: c.class(),
            command: c,
            ids,
        };
        let applied = res.map_err(|e| fail(&step_error(first_new + k, &step, e)))?;
        let d = delta::diff(&before, &sim.project);
        if d.is_empty() {
            return Err(fail(&CommandError::NothingTo(format!(
                "{}: no change",
                step.command.name()
            ))));
        }
        created.extend(applied.created);
        warnings.extend(applied.warnings);
        changes.extend(d);
        p.steps.push(step);
    }
    let problems = eak_core::integrity::check(&sim.project);
    if !problems.is_empty() {
        return Err(fail(&CommandError::Integrity(problems.join("; "))));
    }
    p.updated_ms = now();
    let event = if first_new == 0 {
        "created"
    } else {
        "step_added"
    };
    let actor = Actor::agent_in_session(&ctx.agent, &ctx.session_id).label();
    save(engine, &p, event, &actor).map_err(|e| fail(&e))?;
    Ok(json!({
        "proposed": true,
        "proposal_id": p.id,
        "steps_added": (first_new..p.steps.len()).collect::<Vec<_>>(),
        "steps": p.steps.len(),
        "revision_before": current,
        "revision_after": current,
        "created": created,
        "changes": changes,
        "warnings": warnings,
        "note": "propose mode: nothing was applied. The step was checked on a copy and added to the proposal; the user reviews and applies it in EAK. Reads and checks in this session show the proposal applied on a copy.",
        "next": "add more steps, run run_check to see the copy's ERC/DRC, then proposal_submit",
    }))
}

/// The project as this session's open proposal would make it, as an in-memory engine, and the
/// proposal id (reads and checks in propose mode see the session's own pending edits). `None`
/// without an open proposal that applies at the current revision.
pub(crate) fn session_view(engine: &Engine, ctx: &CallContext) -> Option<(Engine, String)> {
    let p = open_for_session(engine, ctx)?;
    if p.steps.is_empty() || p.base_revision != engine.project().revision {
        return None;
    }
    let sim = simulate(engine.project(), &p.steps, &all_steps(&p)).ok()?;
    Some((Engine::in_memory(sim.project), p.id))
}

/// One line of `proposal_list`.
pub fn summary(p: &Proposal, current_revision: u64) -> Value {
    json!({
        "id": p.id, "title": p.title, "status": p.status, "agent": p.agent,
        "session_id": p.session_id, "base_revision": p.base_revision,
        "stale": p.status.reviewable() && p.base_revision != current_revision,
        "steps": p.steps.len(), "created_ms": p.created_ms, "updated_ms": p.updated_ms,
        "applied": p.applied,
    })
}

fn arg_id(dir: &Path, ctx: &CallContext, args: &Value) -> Result<String, Fail> {
    if let Some(id) = args.get("proposal_id").and_then(Value::as_str) {
        return Ok(id.to_string());
    }
    list(dir)
        .into_iter()
        .rev()
        .find(|p| p.session_id == ctx.session_id && p.status.reviewable())
        .map(|p| p.id)
        .ok_or_else(|| {
            Fail(
                ErrorCode::NotFound,
                "this session has no open proposal; pass proposal_id".into(),
                Value::Null,
            )
        })
}

fn arg_text<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(Value::as_str).unwrap_or("").trim()
}

/// The `proposal_*` harness tools. None of them applies a proposal: a person does, in EAK.
pub(crate) fn dispatch(
    engine: &mut Engine,
    ctx: &CallContext,
    tool: &str,
    args: &Value,
) -> Result<Value, Fail> {
    let dir = project_dir(engine).map_err(|e| fail(&e))?;
    let current = engine.project().revision;
    let agent = Actor::agent_in_session(&ctx.agent, &ctx.session_id).label();
    match tool {
        "proposal_begin" => {
            let (mut p, event) = match open_for_session(engine, ctx) {
                Some(p) => (p, "renamed"),
                None => (
                    new_proposal(engine, ctx, arg_text(args, "title")),
                    "created",
                ),
            };
            if !arg_text(args, "title").is_empty() {
                p.title = clip(arg_text(args, "title"));
            }
            if !arg_text(args, "rationale").is_empty() {
                p.rationale = clip(arg_text(args, "rationale"));
            }
            p.updated_ms = now();
            save(engine, &p, event, &agent).map_err(|e| fail(&e))?;
            Ok(json!({"proposal": summary(&p, current), "apply_mode": ctx.apply_mode}))
        }
        "proposal_list" => Ok(json!({
            "revision": current,
            "proposals": list(&dir).iter().map(|p| summary(p, current)).collect::<Vec<_>>(),
        })),
        "proposal_status" => {
            let id = arg_id(&dir, ctx, args)?;
            let steps: Option<Vec<usize>> = match args.get("steps") {
                None | Some(Value::Null) => None,
                Some(v) => Some(serde_json::from_value(v.clone()).map_err(|_| {
                    fail(&CommandError::Invalid(
                        "steps must be a list of step indices".into(),
                    ))
                })?),
            };
            let r = review(engine, &id, steps.as_deref()).map_err(|e| fail(&e))?;
            serde_json::to_value(&r)
                .map_err(|e| Fail(ErrorCode::Internal, e.to_string(), Value::Null))
        }
        "proposal_submit" => {
            let id = arg_id(&dir, ctx, args)?;
            let mut p = load(&dir, &id).map_err(|e| fail(&e))?;
            if p.session_id != ctx.session_id {
                return Err(fail(&CommandError::ApprovalRequired(
                    "a session submits only its own proposals".into(),
                )));
            }
            if p.status != Status::Open {
                return Err(fail(&not_reviewable(&p)));
            }
            if p.steps.is_empty() {
                return Err(fail(&CommandError::NothingTo(
                    "submit: the proposal has no steps".into(),
                )));
            }
            p.status = Status::Ready;
            p.updated_ms = now();
            save(engine, &p, "submitted", &agent).map_err(|e| fail(&e))?;
            Ok(json!({
                "proposal": summary(&p, current),
                "note": "ready for review: the user applies or discards it in EAK; this session's next design call starts a new proposal",
            }))
        }
        "proposal_discard" => {
            let id = arg_id(&dir, ctx, args)?;
            let by = Actor::agent_in_session(&ctx.agent, &ctx.session_id);
            let p = discard(engine, &id, &by, Some(&ctx.session_id)).map_err(|e| fail(&e))?;
            Ok(json!({"proposal": summary(&p, current)}))
        }
        other => Err(Fail(
            ErrorCode::UnknownTool,
            format!("no proposal tool named {other}"),
            Value::Null,
        )),
    }
}
