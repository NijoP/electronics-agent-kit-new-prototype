//! The lead agent loop (Execution Instructions §5, §10.1, §11; Spec §5.1, §6).
//!
//! * Bounded: at most `max_turns` provider calls and `max_tool_calls_per_turn` tool calls
//!   per turn; tool output is truncated to `max_tool_output_chars`.
//! * Cancellable: the [`CancelToken`] is checked before every provider call and tool call,
//!   and the HTTP transport aborts in-flight requests.
//! * Modes ([`AgentMode`]): `Manual` (the agent inspects and proposes; the user applies),
//!   `Assisted` (the agent applies each command only after the user approves it) and
//!   `Autonomous` (the agent applies commands inside a user pre-authorised scope; anything
//!   beyond it, and every destructive command, still needs explicit approval).
//! * Every mutation is a `CommandRequest` with an `Agent` actor and an [`Approval`] that
//!   covers the command's `OperationClass`, obtained after a dry-run preview on a copy of
//!   the project (scope, affected objects, warnings; Spec AG-02).
//! * Revision safety (AG-03): commands are submitted against the revision the agent last
//!   observed. If the project changed since, the command is refused and the model must
//!   refresh its view first.
//! * The agent never certifies its own work: command acknowledgements carry the engine's
//!   validation note and the checks to re-run; verification is `run_erc`/`run_drc`.

use crate::message::*;
use crate::provider::Provider;
use crate::tools::{self, CommandTool, Domain};
use eak_core::Id;
use eak_runtime::{
    Actor, Approval, Command, CommandError, CommandRequest, CommandResponse, Engine, OperationClass,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// Agent autonomy levels. The source-of-truth documents do not define these names; the
/// mapping follows the Spec §6.1 lifecycle (REQUEST → PLAN → PREVIEW → APPROVE → APPLY →
/// VERIFY) and its per-class approval policy.
pub enum AgentMode {
    /// The user stays in control of every edit: the agent uses read-only tools and records
    /// command proposals (validated and previewed on a sandbox copy) as a plan. Nothing is
    /// applied; the user may later apply the plan with [`Agent::execute_plan`].
    Manual,
    /// The agent applies commands, each after a preview and an explicit user approval.
    Assisted,
    /// The agent applies commands covered by `AgentConfig::pre_authorization` without
    /// asking (Spec §6.1: edits "may be pre-authorised within a scoped task"); commands
    /// above that scope, and every destructive command, go to the user for approval.
    Autonomous,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Actor name recorded in the journal (`agent:<name>`).
    pub name: String,
    pub mode: AgentMode,
    /// Scoped pre-authorisation used only in `Autonomous` mode. It never covers
    /// `Destructive` commands, whatever its `max_class`.
    pub pre_authorization: Option<Approval>,
    pub max_turns: u32,
    pub max_tool_calls_per_turn: usize,
    pub max_tokens: u32,
    pub max_tool_output_chars: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        AgentConfig {
            name: "lead".into(),
            mode: AgentMode::Manual,
            pre_authorization: None,
            max_turns: 12,
            max_tool_calls_per_turn: 16,
            max_tokens: 16_000,
            max_tool_output_chars: 60_000,
        }
    }
}

/// What the user is asked to approve (Spec §6.1: what will change, what validation runs).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApprovalRequest {
    pub tool_call_id: String,
    pub command: Command,
    pub class: OperationClass,
    /// Checks to re-run after applying (ERC/DRC/MECH).
    pub checks_to_run: Vec<String>,
    /// Dry run of the command on a copy of the project: affected/created ids, warnings.
    pub preview: CommandResponse,
    /// The model's accompanying explanation for this turn.
    pub rationale: String,
}

/// Source of approvals (the desktop shows a dialog; tests script decisions).
pub trait Approver {
    /// Return an approval to authorise the command, or `None` to deny it.
    fn approve(&mut self, request: &ApprovalRequest) -> Option<Approval>;
}

impl<F: FnMut(&ApprovalRequest) -> Option<Approval>> Approver for F {
    fn approve(&mut self, request: &ApprovalRequest) -> Option<Approval> {
        self(request)
    }
}

/// Denies everything (default for unattended runs).
pub struct DenyAll;

impl Approver for DenyAll {
    fn approve(&mut self, _: &ApprovalRequest) -> Option<Approval> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "outcome", content = "detail", rename_all = "snake_case")]
pub enum AgentOutcome {
    /// The model ended its turn. This is not verification of the design.
    Completed,
    TurnLimitReached,
    Cancelled,
    /// The provider's safety system declined the request.
    Refused,
    ProviderFailed(String),
}

/// One command the agent tried to apply, with its full authorisation trail.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionRecord {
    pub tool_call_id: String,
    pub command: Command,
    pub class: OperationClass,
    pub preview: Option<CommandResponse>,
    pub approval: Option<Approval>,
    /// Engine response when the request was submitted.
    pub response: Option<CommandResponse>,
    /// Why the command was not submitted (denied, stale, invalid, cancelled).
    pub not_submitted: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedStep {
    pub command: Command,
    pub class: OperationClass,
    pub checks_to_run: Vec<String>,
    /// Dry run on the cumulative plan sandbox.
    pub preview: CommandResponse,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentEvent {
    TurnStarted {
        turn: u32,
    },
    ModelResponded {
        turn: u32,
        text: String,
        tool_calls: usize,
        stop_reason: StopReason,
        usage: Usage,
    },
    ToolExecuted {
        call_id: String,
        name: String,
        is_error: bool,
    },
    ApprovalRequested {
        call_id: String,
        command: String,
        class: OperationClass,
    },
    ApprovalDenied {
        call_id: String,
        reason: String,
    },
    CommandApplied {
        call_id: String,
        op_id: String,
        revision_after: u64,
    },
    CommandRejected {
        call_id: String,
        error: String,
    },
    PlanStepRecorded {
        step: usize,
        command: String,
    },
    Finished {
        outcome: AgentOutcome,
    },
}

/// Run log (AI-01 evidence: agent run log, commands, revisions).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentRun {
    pub mode: AgentMode,
    pub provider: String,
    pub outcome: AgentOutcome,
    pub final_text: String,
    pub turns: u32,
    pub usage: Usage,
    pub revision_before: u64,
    pub revision_after: u64,
    pub actions: Vec<ActionRecord>,
    pub plan: Vec<PlannedStep>,
    pub events: Vec<AgentEvent>,
}

pub struct Agent {
    provider: Box<dyn Provider>,
    pub config: AgentConfig,
    transcript: Vec<ChatMessage>,
    /// (project id, revision) the agent's context was last refreshed at.
    observed: Option<(Id, u64)>,
    /// Plan recorded by the last `Manual`-mode run, awaiting `execute_plan`.
    pending_plan: Vec<PlannedStep>,
    /// (project id, revision) the pending plan was made against.
    plan_base: Option<(Id, u64)>,
}

fn system_prompt(mode: AgentMode) -> String {
    let mode_rules = match mode {
        AgentMode::Manual => "MODE: MANUAL. The user applies every change. Inspect with read-only tools; propose changes as EAK command tool calls. Each call is validated and previewed on a sandbox copy and recorded as a plan step; nothing is applied to the project. Summarise the plan and the checks to run afterwards.",
        AgentMode::Assisted => "MODE: ASSISTED. Apply changes with EAK command tool calls. Each command is previewed and applied only if the user approves it; a denied command is not applied - do not retry it unchanged. After changes, run the relevant checks (run_erc / run_drc) and report their results.",
        AgentMode::Autonomous => "MODE: AUTONOMOUS. Apply changes with EAK command tool calls. Commands inside the user's pre-authorised scope apply without asking; others (and every destructive command) need the user's approval - a denied command is not applied, do not retry it unchanged. After changes, run the relevant checks (run_erc / run_drc) and report their results.",
    };
    format!(
        "You are the EAK lead engineering agent for an electronics design project.\n{mode_rules}\n\
Rules:\n\
- The design changes only through the typed EAK command tools; never invent object ids - read them with get_design_summary / get_netlist.\n\
- Lengths are integer nanometres (1 mm = 1000000).\n\
- Use the calc_* tools for engineering numbers; do not compute them yourself.\n\
- A command being accepted is not validation. Never claim a design is verified, correct, or manufacturing-ready; report ERC/DRC findings and their not_implemented/limitations lists as given.\n\
- If a command is rejected as stale, refresh with get_design_summary before retrying."
    )
}

fn truncate(s: String, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s;
    }
    let mut t: String = s.chars().take(max).collect();
    t.push_str(&format!("\n[truncated: {} characters omitted]", n - max));
    t
}

fn checks(domain: Domain) -> Vec<String> {
    domain.checks().iter().map(|c| c.to_string()).collect()
}

/// Dry-run a command on a disposable in-memory copy (same command implementation as the
/// real engine; nothing is persisted).
fn dry_run(
    sandbox: &mut Engine,
    actor: &Actor,
    command: &Command,
    class: OperationClass,
) -> CommandResponse {
    sandbox.execute(CommandRequest {
        expected_revision: sandbox.project().revision,
        actor: actor.clone(),
        command: command.clone(),
        approval: Some(Approval {
            approved_by: "preview-sandbox".into(),
            max_class: class,
            note: "dry run on a copy; not applied".into(),
        }),
    })
}

struct Ctx<'a> {
    engine: &'a mut Engine,
    approver: &'a mut dyn Approver,
    cancel: &'a CancelToken,
    actions: Vec<ActionRecord>,
    plan: Vec<PlannedStep>,
    plan_sandbox: Option<Engine>,
    events: Vec<AgentEvent>,
}

impl Agent {
    pub fn new(provider: Box<dyn Provider>, config: AgentConfig) -> Agent {
        Agent {
            provider,
            config,
            transcript: vec![],
            observed: None,
            pending_plan: vec![],
            plan_base: None,
        }
    }

    pub fn transcript(&self) -> &[ChatMessage] {
        &self.transcript
    }

    /// Steps recorded by the most recent plan-mode run.
    pub fn pending_plan(&self) -> &[PlannedStep] {
        &self.pending_plan
    }

    fn actor(&self) -> Actor {
        Actor::agent(&self.config.name)
    }

    fn sync_observed(&mut self, engine: &Engine) {
        let p = engine.project();
        if self.observed.as_ref().map(|(id, _)| id) != Some(&p.id) {
            self.observed = Some((p.id.clone(), p.revision));
        }
    }

    fn observed_revision(&self) -> u64 {
        self.observed.as_ref().map(|(_, r)| *r).unwrap_or(0)
    }

    fn mark_observed(&mut self, engine: &Engine) {
        let p = engine.project();
        self.observed = Some((p.id.clone(), p.revision));
    }

    fn offered_tools(&self) -> Vec<ToolSpec> {
        // Every mode offers the command tools: in Manual mode they record proposals.
        tools::read_tools()
            .into_iter()
            .map(|t| t.spec)
            .chain(tools::command_tools().into_iter().map(|t| t.spec))
            .collect()
    }

    /// Run one user request to completion, the turn limit, cancellation or failure.
    pub fn run(
        &mut self,
        engine: &mut Engine,
        prompt: &str,
        approver: &mut dyn Approver,
        cancel: &CancelToken,
    ) -> AgentRun {
        self.sync_observed(engine);
        let revision_before = engine.project().revision;
        let mode = self.config.mode;
        let plan_sandbox =
            (mode == AgentMode::Manual).then(|| Engine::in_memory(engine.project().clone()));
        let mut ctx = Ctx {
            engine,
            approver,
            cancel,
            actions: vec![],
            plan: vec![],
            plan_sandbox,
            events: vec![],
        };
        self.transcript.push(ChatMessage::User {
            text: prompt.into(),
        });
        let tools_offered = self.offered_tools();
        let command_tools = tools::command_tools();
        let mut usage = Usage::default();
        let mut final_text = String::new();
        let mut turns = 0;
        let outcome = loop {
            if cancel.is_cancelled() {
                break AgentOutcome::Cancelled;
            }
            if turns >= self.config.max_turns {
                break AgentOutcome::TurnLimitReached;
            }
            turns += 1;
            ctx.events.push(AgentEvent::TurnStarted { turn: turns });
            let request = ChatRequest {
                system: system_prompt(mode),
                messages: self.transcript.clone(),
                tools: tools_offered.clone(),
                max_tokens: self.config.max_tokens,
            };
            let resp = match self.provider.complete(&request, cancel) {
                Ok(r) => r,
                Err(crate::provider::ProviderError::Cancelled) => break AgentOutcome::Cancelled,
                Err(e) => break AgentOutcome::ProviderFailed(e.to_string()),
            };
            usage.input_tokens += resp.usage.input_tokens;
            usage.output_tokens += resp.usage.output_tokens;
            ctx.events.push(AgentEvent::ModelResponded {
                turn: turns,
                text: resp.text.clone(),
                tool_calls: resp.tool_calls.len(),
                stop_reason: resp.stop_reason,
                usage: resp.usage,
            });
            final_text = resp.text.clone();
            self.transcript.push(ChatMessage::Assistant {
                text: resp.text.clone(),
                tool_calls: resp.tool_calls.clone(),
                raw: resp.raw.clone(),
            });
            if resp.stop_reason == StopReason::Refusal {
                break AgentOutcome::Refused;
            }
            if resp.tool_calls.is_empty() {
                break AgentOutcome::Completed;
            }
            let mut results = vec![];
            for (i, call) in resp.tool_calls.iter().enumerate() {
                let (content, is_error) = if cancel.is_cancelled() {
                    (
                        "cancelled by the user before this tool ran".to_string(),
                        true,
                    )
                } else if i >= self.config.max_tool_calls_per_turn {
                    (
                        format!(
                            "not executed: more than {} tool calls in one turn",
                            self.config.max_tool_calls_per_turn
                        ),
                        true,
                    )
                } else {
                    self.dispatch(&mut ctx, call, &resp.text, &command_tools)
                };
                ctx.events.push(AgentEvent::ToolExecuted {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    is_error,
                });
                results.push(ToolResult {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: truncate(content, self.config.max_tool_output_chars),
                    is_error,
                });
            }
            // Every tool call gets a result, even when cancelled, so the transcript stays
            // valid for the next request.
            self.transcript.push(ChatMessage::Tool { results });
        };
        ctx.events.push(AgentEvent::Finished {
            outcome: outcome.clone(),
        });
        if mode == AgentMode::Manual {
            self.pending_plan = ctx.plan.clone();
            let p = ctx.engine.project();
            self.plan_base = Some((p.id.clone(), p.revision));
        }
        AgentRun {
            mode,
            provider: self.provider.label(),
            outcome,
            final_text,
            turns,
            usage,
            revision_before,
            revision_after: ctx.engine.project().revision,
            actions: ctx.actions,
            plan: ctx.plan,
            events: ctx.events,
        }
    }

    fn dispatch(
        &mut self,
        ctx: &mut Ctx,
        call: &ToolCall,
        rationale: &str,
        command_tools: &[CommandTool],
    ) -> (String, bool) {
        if let Some(t) = tools::read_tools()
            .into_iter()
            .find(|t| t.spec.name == call.name)
        {
            let r = tools::readonly::run(&call.name, &call.arguments, ctx.engine.project())
                .expect("listed read tool dispatches");
            if t.reads_project {
                self.mark_observed(ctx.engine);
            }
            return match r {
                Ok(v) => (v.to_string(), false),
                Err(e) => (e, true),
            };
        }
        let Some(tool) = command_tools.iter().find(|t| t.spec.name == call.name) else {
            return (format!("unknown tool {}", call.name), true);
        };
        let command = match tools::decode_command(&call.name, &call.arguments) {
            Ok(c) => c,
            Err(e) => return (e, true),
        };
        let class = command.class();
        let checks_to_run = checks(tool.domain);
        if self.config.mode == AgentMode::Manual {
            let actor = self.actor();
            let sandbox = ctx
                .plan_sandbox
                .as_mut()
                .expect("manual mode has a plan sandbox");
            let preview = dry_run(sandbox, &actor, &command, class);
            if !preview.accepted {
                return (
                    json!({"plan_step": null, "error": preview.error, "note": "invalid step; not recorded"}).to_string(),
                    true,
                );
            }
            ctx.plan.push(PlannedStep {
                command: command.clone(),
                class,
                checks_to_run: checks_to_run.clone(),
                preview: preview.clone(),
            });
            ctx.events.push(AgentEvent::PlanStepRecorded {
                step: ctx.plan.len(),
                command: call.name.clone(),
            });
            return (
                json!({
                    "plan_step": ctx.plan.len(),
                    "class": class,
                    "preview": {"created": preview.created, "affected": preview.affected, "warnings": preview.warnings},
                    "checks_to_run": checks_to_run,
                    "note": "recorded in the plan; NOT applied to the project. Ids created in the preview are placeholders.",
                })
                .to_string(),
                false,
            );
        }
        let record = self.submit(
            ctx,
            &call.id,
            command,
            class,
            checks_to_run.clone(),
            rationale,
        );
        let result = match (&record.response, &record.not_submitted) {
            (Some(r), _) => (
                json!({
                    "accepted": r.accepted,
                    "revision_after": r.revision_after,
                    "created": r.created,
                    "affected": r.affected,
                    "warnings": r.warnings,
                    "error": r.error,
                    "validation_note": r.validation_note,
                    "checks_to_run": checks_to_run,
                })
                .to_string(),
                !r.accepted,
            ),
            (None, Some(why)) => (why.clone(), true),
            (None, None) => unreachable!("a record is either submitted or explains why not"),
        };
        ctx.actions.push(record);
        result
    }

    /// Preview → approve → submit one command as the agent actor.
    fn submit(
        &mut self,
        ctx: &mut Ctx,
        call_id: &str,
        command: Command,
        class: OperationClass,
        checks_to_run: Vec<String>,
        rationale: &str,
    ) -> ActionRecord {
        let mut record = ActionRecord {
            tool_call_id: call_id.into(),
            command: command.clone(),
            class,
            preview: None,
            approval: None,
            response: None,
            not_submitted: None,
        };
        let current = ctx.engine.project().revision;
        let observed = self.observed_revision();
        if ctx.engine.blocked_reason().is_some() || observed != current {
            let why = match ctx.engine.blocked_reason() {
                Some(b) => format!("not applied: project is blocked: {b}"),
                None => format!(
                    "not applied: stale revision (your view is revision {observed}, the project is at {current}); refresh with get_design_summary and re-plan"
                ),
            };
            ctx.events.push(AgentEvent::CommandRejected {
                call_id: call_id.into(),
                error: why.clone(),
            });
            record.not_submitted = Some(why);
            return record;
        }
        let actor = self.actor();
        let mut sandbox = Engine::in_memory(ctx.engine.project().clone());
        let preview = dry_run(&mut sandbox, &actor, &command, class);
        record.preview = Some(preview.clone());
        if !preview.accepted {
            let why = format!(
                "not applied: the command would be rejected: {}",
                preview
                    .error
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_default()
            );
            ctx.events.push(AgentEvent::CommandRejected {
                call_id: call_id.into(),
                error: why.clone(),
            });
            record.not_submitted = Some(why);
            return record;
        }
        // Autonomous mode: the user's scoped pre-authorisation covers the command when its
        // class is within scope. Destructive commands always need explicit approval.
        let pre_authorized = match (&self.config.mode, &self.config.pre_authorization) {
            (AgentMode::Autonomous, Some(a))
                if class < OperationClass::Destructive && a.max_class >= class =>
            {
                Some(a.clone())
            }
            _ => None,
        };
        let approval = match pre_authorized {
            Some(a) => Some(a),
            None => {
                ctx.events.push(AgentEvent::ApprovalRequested {
                    call_id: call_id.into(),
                    command: command.name(),
                    class,
                });
                let request = ApprovalRequest {
                    tool_call_id: call_id.into(),
                    command: command.clone(),
                    class,
                    checks_to_run,
                    preview,
                    rationale: rationale.into(),
                };
                ctx.approver.approve(&request)
            }
        }
        .filter(|a| a.max_class >= class && !a.approved_by.trim().is_empty());
        let Some(approval) = approval else {
            let why = format!(
                "not applied: the user did not approve this {class:?} operation; do not retry it unchanged"
            );
            ctx.events.push(AgentEvent::ApprovalDenied {
                call_id: call_id.into(),
                reason: why.clone(),
            });
            record.not_submitted = Some(why);
            return record;
        };
        if ctx.cancel.is_cancelled() {
            record.approval = Some(approval);
            record.not_submitted = Some("not applied: cancelled before submission".into());
            return record;
        }
        record.approval = Some(approval.clone());
        let response = ctx.engine.execute(CommandRequest {
            expected_revision: observed,
            actor,
            command,
            approval: Some(approval),
        });
        if response.accepted {
            self.mark_observed(ctx.engine);
            ctx.events.push(AgentEvent::CommandApplied {
                call_id: call_id.into(),
                op_id: response.op_id.clone(),
                revision_after: response.revision_after,
            });
        } else {
            if matches!(response.error, Some(CommandError::StaleRevision { .. })) {
                // Our view is outdated; force a refresh before the next mutation.
                self.observed = None;
            }
            ctx.events.push(AgentEvent::CommandRejected {
                call_id: call_id.into(),
                error: response
                    .error
                    .as_ref()
                    .map(|e| e.to_string())
                    .unwrap_or_default(),
            });
        }
        record.response = Some(response);
        record
    }

    /// Apply the plan recorded by the last plan-mode run, step by step, each with its own
    /// preview and approval. Stops at the first step that is not applied. The plan is
    /// consumed either way (re-plan after a failure). A plan made against another revision
    /// or project is refused, never rebased silently (AG-03).
    pub fn execute_plan(
        &mut self,
        engine: &mut Engine,
        approver: &mut dyn Approver,
        cancel: &CancelToken,
    ) -> Result<Vec<ActionRecord>, String> {
        let steps = std::mem::take(&mut self.pending_plan);
        let base = self.plan_base.take();
        let p = engine.project();
        match base {
            None => return Err("no pending plan".into()),
            Some((id, rev)) if id != p.id || rev != p.revision => {
                return Err(format!(
                "the plan was made at revision {rev}; the project is now at revision {}; re-plan",
                p.revision
            ))
            }
            Some(_) => {}
        }
        if steps.is_empty() {
            return Err("the pending plan has no steps".into());
        }
        self.mark_observed(engine);
        let mut ctx = Ctx {
            engine,
            approver,
            cancel,
            actions: vec![],
            plan: vec![],
            plan_sandbox: None,
            events: vec![],
        };
        for (i, step) in steps.into_iter().enumerate() {
            if cancel.is_cancelled() {
                break;
            }
            let record = self.submit(
                &mut ctx,
                &format!("plan_step_{}", i + 1),
                step.command,
                step.class,
                step.checks_to_run,
                "executing approved plan",
            );
            let ok = record.response.as_ref().is_some_and(|r| r.accepted);
            ctx.actions.push(record);
            if !ok {
                break;
            }
        }
        Ok(ctx.actions)
    }

    /// Serialise a run for the evidence/run log.
    pub fn run_log(run: &AgentRun) -> Value {
        serde_json::to_value(run).unwrap_or(Value::Null)
    }
}
