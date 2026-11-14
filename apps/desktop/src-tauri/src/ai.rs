//! AI panel IPC: the `eak-intelligence` agent loop over the open project.
//!
//! * `ai_propose` runs the agent in **Manual** mode with the user's configured provider on
//!   a copy of the open project. Every typed command the model proposes is validated and
//!   dry-run on a sandbox and comes back as a pending step with its preview. Nothing is
//!   applied.
//! * `ai_accept` applies one pending step through `Engine::execute` with the **Agent** actor
//!   and an approval recorded as given by the user (the engine refuses agent requests
//!   without one). The step is submitted against the revision the plan was made at (plus
//!   the steps already accepted from it), so an edit made in between makes it stale and it
//!   is refused, never rebased.
//! * `ai_reject` drops a pending step; nothing is written anywhere.
//! * `ai_rollback` undoes an accepted agent step, only while it is still the latest change.
//!
//! No provider, no key, or a provider failure is returned as an error or an outcome: the
//! panel never fabricates a reply.

use crate::models::ModelSettings;
use crate::Session;
use eak_intelligence::{
    Agent, AgentConfig, AgentMode, AgentOutcome, CancelToken, DenyAll, Provider,
};
use eak_runtime::{
    Actor, Approval, Command, CommandRequest, CommandResponse, Engine, OperationClass,
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;

/// Actor name recorded in the journal for applied AI steps (`agent:assistant`).
pub const AGENT_NAME: &str = "assistant";

#[derive(Debug, Clone, Serialize)]
pub struct AiStep {
    pub id: u64,
    pub proposal_id: u64,
    pub command: Command,
    pub class: OperationClass,
    pub checks_to_run: Vec<String>,
    /// Dry run on a sandbox copy (cumulative over the earlier steps of the same proposal).
    pub preview: CommandResponse,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiProposal {
    pub id: u64,
    pub provider: String,
    pub outcome: AgentOutcome,
    pub text: String,
    pub revision: u64,
    pub steps: Vec<AiStep>,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    steps: HashMap<u64, AiStep>,
    /// Revision the next step of each proposal must be submitted against.
    proposal_revision: HashMap<u64, u64>,
    /// Accepted agent steps: revision_after -> op_id (for rollback).
    accepted: HashMap<u64, String>,
}

/// Pending AI steps for this desktop process.
#[derive(Default)]
pub struct AiState(Mutex<Inner>);

impl AiState {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Inner>, String> {
        self.0
            .lock()
            .map_err(|_| "AI state is poisoned; restart EAK".to_string())
    }
}

/// Run the agent with the configured provider profile `provider_id`.
pub fn ai_propose(
    s: &Session,
    ai: &AiState,
    models: &ModelSettings,
    provider_id: &str,
    prompt: &str,
) -> Result<AiProposal, String> {
    let provider = models.provider(provider_id).map_err(|e| {
        format!("AI provider is not ready: {e}. Open Model settings to configure it.")
    })?;
    propose_with(s, ai, provider, prompt)
}

/// Run one Manual-mode agent request on a copy of the open project and keep its proposed
/// steps pending. The live engine is locked only to copy the project.
pub fn propose_with(
    s: &Session,
    ai: &AiState,
    provider: Box<dyn Provider>,
    prompt: &str,
) -> Result<AiProposal, String> {
    if prompt.trim().is_empty() {
        return Err("empty request".into());
    }
    let mut copy = s.with_engine(|e| Engine::in_memory(e.project().clone()))?;
    let revision = copy.project().revision;
    let label = provider.label();
    let mut agent = Agent::new(
        provider,
        AgentConfig {
            name: AGENT_NAME.into(),
            mode: AgentMode::Manual,
            ..AgentConfig::default()
        },
    );
    let run = agent.run(&mut copy, prompt, &mut DenyAll, &CancelToken::new());
    let mut inner = ai.lock()?;
    inner.next_id += 1;
    let proposal_id = inner.next_id;
    let mut steps = vec![];
    for p in run.plan {
        inner.next_id += 1;
        let step = AiStep {
            id: inner.next_id,
            proposal_id,
            command: p.command,
            class: p.class,
            checks_to_run: p.checks_to_run,
            preview: p.preview,
        };
        inner.steps.insert(step.id, step.clone());
        steps.push(step);
    }
    inner.proposal_revision.insert(proposal_id, revision);
    Ok(AiProposal {
        id: proposal_id,
        provider: label,
        outcome: run.outcome,
        text: run.final_text,
        revision,
        steps,
    })
}

/// Apply a pending step as the agent, with the user's approval of exactly its class.
pub fn ai_accept(s: &Session, ai: &AiState, step_id: u64) -> Result<CommandResponse, String> {
    let mut inner = ai.lock()?;
    let step = inner
        .steps
        .remove(&step_id)
        .ok_or_else(|| format!("no pending AI step {step_id}"))?;
    let expected_revision = inner
        .proposal_revision
        .get(&step.proposal_id)
        .copied()
        .unwrap_or(step.preview.revision_before);
    let resp = s.with_engine(|e| {
        e.execute(CommandRequest {
            expected_revision,
            actor: Actor::agent(AGENT_NAME),
            command: step.command.clone(),
            approval: Some(Approval {
                approved_by: Actor::human().label(),
                max_class: step.class,
                note: format!("accepted in the AI panel (step {step_id})"),
            }),
        })
    })?;
    if resp.accepted {
        inner
            .proposal_revision
            .insert(step.proposal_id, resp.revision_after);
        inner
            .accepted
            .insert(resp.revision_after, resp.op_id.clone());
    }
    Ok(resp)
}

/// Drop a pending step. Nothing is recorded in the project or journal.
pub fn ai_reject(ai: &AiState, step_id: u64) -> Result<(), String> {
    ai.lock()?
        .steps
        .remove(&step_id)
        .map(|_| ())
        .ok_or_else(|| format!("no pending AI step {step_id}"))
}

/// Undo the accepted agent step `op_id`, only while it is still the latest change.
pub fn ai_rollback(s: &Session, ai: &AiState, op_id: &str) -> Result<CommandResponse, String> {
    let mut inner = ai.lock()?;
    let resp = s.with_engine(|e| {
        let rev = e.project().revision;
        match inner.accepted.get(&rev) {
            Some(op) if op == op_id => Ok(e.undo(rev, &Actor::human())),
            _ => Err(format!(
                "AI step {op_id} is not the latest change (project is at revision {rev}); use Undo instead"
            )),
        }
    })??;
    if resp.accepted {
        inner.accepted.retain(|_, op| op != op_id);
    }
    Ok(resp)
}

pub mod cmd {
    use super::*;
    use tauri::State;

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn ai_propose(
        state: State<'_, Session>,
        ai: State<'_, AiState>,
        models: State<'_, ModelSettings>,
        provider_id: String,
        prompt: String,
    ) -> Result<AiProposal, String> {
        super::ai_propose(&state, &ai, &models, &provider_id, &prompt)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn ai_accept(
        state: State<'_, Session>,
        ai: State<'_, AiState>,
        step_id: u64,
    ) -> Result<CommandResponse, String> {
        super::ai_accept(&state, &ai, step_id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn ai_reject(ai: State<'_, AiState>, step_id: u64) -> Result<(), String> {
        super::ai_reject(&ai, step_id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn ai_rollback(
        state: State<'_, Session>,
        ai: State<'_, AiState>,
        op_id: String,
    ) -> Result<CommandResponse, String> {
        super::ai_rollback(&state, &ai, &op_id)
    }
}
