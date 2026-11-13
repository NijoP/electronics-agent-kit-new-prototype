//! Agent loop contract tests with a scripted provider (no network).

use eak_core::{Point, Project};
use eak_intelligence::mock::{call, ScriptedProvider, Step};
use eak_intelligence::*;
use eak_runtime::{Actor, ActorKind, Approval, Command, CommandRequest, Engine, OperationClass};
use serde_json::json;
use std::cell::RefCell;

fn engine() -> Engine {
    Engine::in_memory(Project::new("agent-test"))
}

fn agent(steps: Vec<Step>, mode: AgentMode) -> (Agent, ScriptedProvider) {
    let p = ScriptedProvider::new(steps);
    let a = Agent::new(
        Box::new(p.clone()),
        AgentConfig {
            mode,
            ..AgentConfig::default()
        },
    );
    (a, p)
}

fn approval(max: OperationClass) -> Approval {
    Approval {
        approved_by: "user".into(),
        max_class: max,
        note: "test".into(),
    }
}

fn place_r(id: &str) -> ToolCall {
    call(
        id,
        "PlaceSymbol",
        json!({"component_id": "R_0603", "position": {"x": 10_000_000, "y": 10_000_000}}),
    )
}

fn respond(r: ChatResponse) -> Step {
    Step::Respond(r)
}

fn tool_results(req: &ChatRequest) -> Vec<eak_intelligence::message::ToolResult> {
    match req.messages.last() {
        Some(ChatMessage::Tool { results }) => results.clone(),
        other => panic!("expected tool results, got {other:?}"),
    }
}

#[test]
fn assisted_mode_applies_an_approved_command_as_the_agent_actor() {
    let (mut a, p) = agent(
        vec![
            respond(ChatResponse::tool_calls("Placing R1.", vec![place_r("c1")])),
            respond(ChatResponse::text("Placed R1; run ERC to check it.")),
        ],
        AgentMode::Assisted,
    );
    let mut e = engine();
    let seen = RefCell::new(vec![]);
    let mut approver = |r: &ApprovalRequest| {
        seen.borrow_mut().push(r.clone());
        Some(approval(r.class))
    };
    let run = a.run(&mut e, "add a resistor", &mut approver, &CancelToken::new());

    assert_eq!(run.outcome, AgentOutcome::Completed);
    assert_eq!(run.turns, 2);
    assert_eq!(e.project().revision, 1);
    assert_eq!(e.project().schematic.symbols.len(), 1);
    // The approval request carried a dry-run preview and the model's rationale.
    let seen = seen.into_inner();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].class, OperationClass::ReversibleLocal);
    assert!(seen[0].preview.accepted);
    assert_eq!(seen[0].preview.created.len(), 1);
    assert_eq!(seen[0].checks_to_run, vec!["ERC".to_string()]);
    assert_eq!(seen[0].rationale, "Placing R1.");
    // The submitted request was an agent request with the approval attached.
    let act = &run.actions[0];
    let resp = act.response.as_ref().unwrap();
    assert!(resp.accepted);
    assert_eq!(resp.approval.as_ref().unwrap().approved_by, "user");
    let applied = e
        .events_since(0)
        .into_iter()
        .find(|ev| ev.kind == "command_applied")
        .unwrap();
    assert_eq!(applied.actor, "agent:lead");
    // The tool result returned to the model states that acceptance is not validation.
    let results = tool_results(&p.requests()[1]);
    assert!(!results[0].is_error);
    assert!(results[0].content.contains("validation_note"));
}

#[test]
fn denied_or_insufficient_approval_is_never_submitted() {
    for granted in [None, Some(approval(OperationClass::ReversibleLocal))] {
        // SetBoardOutline is engineering-impacting: a reversible-local approval cannot cover it.
        let outline = call(
            "c1",
            "SetBoardOutline",
            json!({"points": [{"x":0,"y":0},{"x":30_000_000,"y":0},{"x":30_000_000,"y":20_000_000}]}),
        );
        let (mut a, p) = agent(
            vec![
                respond(ChatResponse::tool_calls("", vec![outline])),
                respond(ChatResponse::text("Not applied.")),
            ],
            AgentMode::Assisted,
        );
        let mut e = engine();
        let mut approver = |_: &ApprovalRequest| granted.clone();
        let run = a.run(&mut e, "set outline", &mut approver, &CancelToken::new());
        assert_eq!(run.outcome, AgentOutcome::Completed);
        assert_eq!(e.project().revision, 0, "nothing applied");
        assert!(e.project().pcb.board.outline.is_empty());
        assert!(run.actions[0].response.is_none());
        assert!(run.actions[0]
            .not_submitted
            .as_ref()
            .unwrap()
            .contains("did not approve"));
        let results = tool_results(&p.requests()[1]);
        assert!(results[0].is_error);
    }
}

#[test]
fn manual_mode_records_a_previewed_plan_without_touching_the_project() {
    let (mut a, _p) = agent(
        vec![
            respond(ChatResponse::tool_calls(
                "Plan: two resistors.",
                vec![place_r("c1"), place_r("c2")],
            )),
            respond(ChatResponse::text("Plan ready.")),
        ],
        AgentMode::Manual,
    );
    let mut e = engine();
    let run = a.run(
        &mut e,
        "plan two resistors",
        &mut DenyAll,
        &CancelToken::new(),
    );
    assert_eq!(run.outcome, AgentOutcome::Completed);
    assert_eq!(e.project().revision, 0);
    assert!(e.project().schematic.symbols.is_empty());
    assert!(run.actions.is_empty());
    assert_eq!(run.plan.len(), 2);
    // Steps are previewed cumulatively: the second resistor gets the next designator.
    assert!(run.plan.iter().all(|s| s.preview.accepted));

    // The user applies the plan; each step is approved individually.
    let calls = RefCell::new(0);
    let mut approver = |r: &ApprovalRequest| {
        *calls.borrow_mut() += 1;
        Some(approval(r.class))
    };
    let actions = a
        .execute_plan(&mut e, &mut approver, &CancelToken::new())
        .unwrap();
    assert_eq!(actions.len(), 2);
    assert_eq!(*calls.borrow(), 2);
    assert_eq!(e.project().revision, 2);
    let mut refs: Vec<_> = e
        .project()
        .schematic
        .symbols
        .values()
        .map(|s| s.refdes.clone())
        .collect();
    refs.sort();
    assert_eq!(refs, vec!["R1", "R2"]);
    // The plan is consumed.
    assert!(a
        .execute_plan(&mut e, &mut approver, &CancelToken::new())
        .is_err());
}

#[test]
fn a_plan_made_against_an_older_revision_is_refused() {
    let (mut a, _p) = agent(
        vec![
            respond(ChatResponse::tool_calls("", vec![place_r("c1")])),
            respond(ChatResponse::text("ok")),
        ],
        AgentMode::Manual,
    );
    let mut e = engine();
    a.run(&mut e, "plan", &mut DenyAll, &CancelToken::new());
    // A human edit lands after the plan was made.
    let r = e.execute(CommandRequest {
        expected_revision: 0,
        actor: Actor::human(),
        command: Command::SetProjectName { name: "x".into() },
        approval: None,
    });
    assert!(r.accepted);
    let mut approver = |r: &ApprovalRequest| Some(approval(r.class));
    let err = a
        .execute_plan(&mut e, &mut approver, &CancelToken::new())
        .unwrap_err();
    assert!(err.contains("re-plan"), "{err}");
    assert!(e.project().schematic.symbols.is_empty());
}

#[test]
fn autonomous_mode_uses_the_scope_but_destructive_commands_still_ask() {
    let mut e = engine();
    // First run: place a resistor under the pre-authorisation (approver never consulted).
    let (mut a, _p) = agent(
        vec![
            respond(ChatResponse::tool_calls("", vec![place_r("c1")])),
            respond(ChatResponse::text("done")),
        ],
        AgentMode::Autonomous,
    );
    a.config.pre_authorization = Some(Approval {
        approved_by: "user (scoped task)".into(),
        max_class: OperationClass::Destructive, // capped below Destructive by the agent
        note: "pre-authorised".into(),
    });
    let asked = RefCell::new(vec![]);
    let mut approver = |r: &ApprovalRequest| {
        asked.borrow_mut().push(r.command.name());
        None
    };
    let run = a.run(&mut e, "place", &mut approver, &CancelToken::new());
    assert!(run.actions[0].response.as_ref().unwrap().accepted);
    assert!(asked.borrow().is_empty());
    let sym = e.project().schematic.symbols.keys().next().unwrap().clone();

    // Second run: a destructive delete goes to the approver, which denies it.
    let (mut a2, _p2) = agent(
        vec![
            respond(ChatResponse::tool_calls(
                "",
                vec![call("c2", "DeleteSymbol", json!({"symbol_id": sym}))],
            )),
            respond(ChatResponse::text("denied")),
        ],
        AgentMode::Autonomous,
    );
    a2.config.pre_authorization = a.config.pre_authorization.clone();
    let run2 = a2.run(&mut e, "delete it", &mut approver, &CancelToken::new());
    assert_eq!(*asked.borrow(), vec!["DeleteSymbol".to_string()]);
    assert!(run2.actions[0].response.is_none());
    assert_eq!(e.project().schematic.symbols.len(), 1);
}

#[test]
fn stale_view_is_refused_until_the_agent_refreshes() {
    let mut e = engine();
    let (mut a, p) = agent(
        vec![
            respond(ChatResponse::text("Looked at it.")),
            // After a human edit the model tries to mutate without refreshing.
            respond(ChatResponse::tool_calls("", vec![place_r("c1")])),
            respond(ChatResponse::tool_calls(
                "",
                vec![call("c2", "get_design_summary", json!({}))],
            )),
            respond(ChatResponse::tool_calls("", vec![place_r("c3")])),
            respond(ChatResponse::text("Placed after refresh.")),
        ],
        AgentMode::Assisted,
    );
    let mut approver = |r: &ApprovalRequest| Some(approval(r.class));
    a.run(&mut e, "look", &mut approver, &CancelToken::new());
    let human = e.execute(CommandRequest {
        expected_revision: 0,
        actor: Actor::human(),
        command: Command::AddRequirement {
            text: "5 V input".into(),
        },
        approval: None,
    });
    assert!(human.accepted);
    let run = a.run(&mut e, "now add R", &mut approver, &CancelToken::new());
    assert_eq!(run.outcome, AgentOutcome::Completed);
    assert!(run.actions[0]
        .not_submitted
        .as_ref()
        .unwrap()
        .contains("stale revision"));
    assert!(run.actions[1].response.as_ref().unwrap().accepted);
    assert_eq!(e.project().revision, 2);
    let reqs = p.requests();
    assert!(tool_results(&reqs[2])[0].is_error);
}

#[test]
fn the_loop_is_bounded_by_max_turns() {
    let steps = (0..10)
        .map(|i| {
            respond(ChatResponse::tool_calls(
                "",
                vec![call(&format!("c{i}"), "get_netlist", json!({}))],
            ))
        })
        .collect();
    let (mut a, p) = agent(steps, AgentMode::Manual);
    a.config.max_turns = 3;
    let mut e = engine();
    let run = a.run(&mut e, "loop forever", &mut DenyAll, &CancelToken::new());
    assert_eq!(run.outcome, AgentOutcome::TurnLimitReached);
    assert_eq!(run.turns, 3);
    assert_eq!(p.requests().len(), 3);
}

#[test]
fn per_turn_tool_calls_and_output_are_bounded() {
    let calls = (0..5)
        .map(|i| call(&format!("c{i}"), "get_design_summary", json!({})))
        .collect();
    let (mut a, p) = agent(
        vec![
            respond(ChatResponse::tool_calls("", calls)),
            respond(ChatResponse::text("ok")),
        ],
        AgentMode::Manual,
    );
    a.config.max_tool_calls_per_turn = 2;
    a.config.max_tool_output_chars = 50;
    let mut e = engine();
    a.run(&mut e, "x", &mut DenyAll, &CancelToken::new());
    let results = tool_results(&p.requests()[1]);
    assert_eq!(results.len(), 5, "every call gets a result");
    assert!(!results[0].is_error);
    assert!(results[0].content.contains("[truncated:"));
    assert!(results[2..].iter().all(|r| r.is_error));
}

#[test]
fn cancellation_mid_run_stops_before_any_mutation() {
    let (mut a, _p) = agent(
        vec![Step::Hook(Box::new(|cancel: &CancelToken| {
            // The user presses Cancel while the model is answering.
            cancel.cancel();
            Ok(ChatResponse::tool_calls("", vec![place_r("c1")]))
        }))],
        AgentMode::Assisted,
    );
    let mut e = engine();
    let mut approver = |r: &ApprovalRequest| Some(approval(r.class));
    let cancel = CancelToken::new();
    let run = a.run(&mut e, "place", &mut approver, &cancel);
    assert_eq!(run.outcome, AgentOutcome::Cancelled);
    assert_eq!(e.project().revision, 0);
    assert!(run.actions.is_empty());
    // The transcript stays well-formed: the dangling tool call got a "cancelled" result.
    match a.transcript().last().unwrap() {
        ChatMessage::Tool { results } => {
            assert!(results[0].is_error);
            assert!(results[0].content.contains("cancelled"));
        }
        other => panic!("{other:?}"),
    }
    // A run started with a cancelled token never calls the provider.
    let (mut b, pb) = agent(vec![respond(ChatResponse::text("x"))], AgentMode::Manual);
    let run = b.run(&mut e, "q", &mut DenyAll, &cancel);
    assert_eq!(run.outcome, AgentOutcome::Cancelled);
    assert!(pb.requests().is_empty());
}

#[test]
fn provider_failure_refusal_and_bad_arguments_are_reported() {
    let mut e = engine();
    let (mut a, _p) = agent(
        vec![Step::Fail(ProviderError::RateLimited("slow down".into()))],
        AgentMode::Manual,
    );
    let run = a.run(&mut e, "q", &mut DenyAll, &CancelToken::new());
    assert!(matches!(run.outcome, AgentOutcome::ProviderFailed(m) if m.contains("slow down")));

    let refusal = ChatResponse {
        stop_reason: message::StopReason::Refusal,
        ..ChatResponse::text("")
    };
    let (mut a, _p) = agent(vec![respond(refusal)], AgentMode::Manual);
    assert_eq!(
        a.run(&mut e, "q", &mut DenyAll, &CancelToken::new())
            .outcome,
        AgentOutcome::Refused
    );

    // Invalid arguments and unknown tools are returned to the model, which can correct them.
    let (mut a, p) = agent(
        vec![
            respond(ChatResponse::tool_calls(
                "",
                vec![
                    call("c1", "MoveSymbol", json!({"symbol_id": "nope"})),
                    call("c2", "write_project_file", json!({"json": "{}"})),
                ],
            )),
            respond(ChatResponse::text("fixed")),
        ],
        AgentMode::Assisted,
    );
    let mut approver = |r: &ApprovalRequest| Some(approval(r.class));
    let run = a.run(&mut e, "q", &mut approver, &CancelToken::new());
    assert_eq!(run.outcome, AgentOutcome::Completed);
    let results = tool_results(&p.requests()[1]);
    assert!(results[0].is_error && results[0].content.contains("position"));
    assert!(results[1].is_error && results[1].content.contains("unknown tool"));
    assert_eq!(e.project().revision, 0);
}

#[test]
fn a_command_the_engine_would_reject_is_not_put_to_the_user() {
    let (mut a, _p) = agent(
        vec![
            respond(ChatResponse::tool_calls(
                "",
                vec![call(
                    "c1",
                    "MoveSymbol",
                    json!({"symbol_id": "sym_missing", "position": {"x": 0, "y": 0}}),
                )],
            )),
            respond(ChatResponse::text("ok")),
        ],
        AgentMode::Assisted,
    );
    let mut e = engine();
    let asked = RefCell::new(0);
    let mut approver = |r: &ApprovalRequest| {
        *asked.borrow_mut() += 1;
        Some(approval(r.class))
    };
    let run = a.run(&mut e, "move", &mut approver, &CancelToken::new());
    assert_eq!(*asked.borrow(), 0);
    assert!(run.actions[0]
        .not_submitted
        .as_ref()
        .unwrap()
        .contains("would be rejected"));
}

#[test]
fn every_mode_offers_read_only_and_command_tools_and_reports_are_honest() {
    let (mut a, p) = agent(
        vec![
            respond(ChatResponse::tool_calls(
                "",
                vec![
                    call("c1", "run_erc", json!({})),
                    call("c2", "calc_trace_width", json!({"current_a": 3.0})),
                ],
            )),
            respond(ChatResponse::text("ERC ran; see not_implemented.")),
        ],
        AgentMode::Manual,
    );
    let mut e = engine();
    let run = a.run(&mut e, "check", &mut DenyAll, &CancelToken::new());
    assert_eq!(run.outcome, AgentOutcome::Completed);
    let reqs = p.requests();
    let names: Vec<_> = reqs[0].tools.iter().map(|t| t.name.as_str()).collect();
    for n in [
        "get_netlist",
        "run_erc",
        "run_drc",
        "calc_trace_width",
        "PlaceSymbol",
        "AutoRoute",
    ] {
        assert!(names.contains(&n), "{n} not offered");
    }
    assert!(reqs[0].system.contains("MODE: MANUAL"));
    let results = tool_results(&reqs[1]);
    let erc: serde_json::Value = serde_json::from_str(&results[0].content).unwrap();
    assert!(erc["not_implemented"].is_array());
    let calc: serde_json::Value = serde_json::from_str(&results[1].content).unwrap();
    let expect = eak_engineering::ipc::trace_width_mm(3.0, false, 1.0, 10.0).unwrap();
    assert!((calc["width_mm"].as_f64().unwrap() - expect).abs() < 1e-12);
    // The run log serialises for evidence.
    let log = Agent::run_log(&run);
    assert_eq!(log["outcome"]["outcome"], "completed");
}

#[test]
fn persisted_project_journals_agent_commands() {
    let dir = std::env::temp_dir().join(format!(
        "eak-intel-{}-{}",
        std::process::id(),
        eak_core::new_id("t")
    ));
    let mut e = Engine::create(&dir, "persisted").unwrap();
    let (mut a, _p) = agent(
        vec![
            respond(ChatResponse::tool_calls("", vec![place_r("c1")])),
            respond(ChatResponse::text("ok")),
        ],
        AgentMode::Assisted,
    );
    let mut approver = |r: &ApprovalRequest| Some(approval(r.class));
    let run = a.run(&mut e, "place", &mut approver, &CancelToken::new());
    assert!(run.actions[0].response.as_ref().unwrap().accepted);
    let journal = eak_runtime::store::read_journal(&dir);
    assert!(journal
        .iter()
        .any(|j| j.actor == "agent:lead" && j.command == "PlaceSymbol"));
    drop(e);
    let reopened = Engine::open(&dir, false).unwrap();
    assert_eq!(reopened.project().schematic.symbols.len(), 1);
    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
    // Sanity: the agent actor kind is Agent.
    assert_eq!(Actor::agent("lead").kind, ActorKind::Agent);
    let _ = Point::new(0, 0);
}
