//! AI panel IPC: propose -> approve -> executed as the agent -> rollback, with the scripted
//! mock provider (no network, no paid API).

use eak_desktop::ai::{self, AiState};
use eak_desktop::models::{MemoryKeyStore, ModelSettings, ProfileInput};
use eak_desktop::*;
use eak_intelligence::mock::{call, ScriptedProvider, Step};
use eak_intelligence::provider::http::default_transport;
use eak_intelligence::{AgentOutcome, ChatResponse};
use eak_runtime::index::Index;
use eak_runtime::{Command, OperationClass};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-desktop-ai-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn open(name: &str) -> Session {
    let s = Session::new(Index::in_memory());
    create_project(&s, &tmpdir(name).display().to_string(), "AI test").unwrap();
    s
}

fn place_r(id: &str) -> Step {
    Step::Respond(ChatResponse::tool_calls(
        "Placing a resistor.",
        vec![call(
            id,
            "PlaceSymbol",
            json!({"component_id": "R_0603", "position": {"x": 10_000_000, "y": 10_000_000}}),
        )],
    ))
}

fn mock(steps: Vec<Step>) -> Box<ScriptedProvider> {
    Box::new(ScriptedProvider::new(steps))
}

#[test]
fn propose_accept_executes_as_agent_and_rollback_undoes_it() {
    let s = open("accept");
    let st = AiState::default();
    let p = ai::propose_with(
        &s,
        &st,
        mock(vec![
            place_r("c1"),
            Step::Respond(ChatResponse::text("Planned R1.")),
        ]),
        "add a resistor",
    )
    .unwrap();
    assert_eq!(p.outcome, AgentOutcome::Completed);
    assert_eq!(p.text, "Planned R1.");
    assert_eq!(p.steps.len(), 1);
    let step = &p.steps[0];
    assert!(matches!(step.command, Command::PlaceSymbol { .. }));
    assert_eq!(step.class, OperationClass::ReversibleLocal);
    assert!(step.preview.accepted);
    // Proposing changes nothing.
    assert_eq!(current_project(&s).unwrap().project.revision, 0);

    let r = ai::ai_accept(&s, &st, step.id).unwrap();
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(r.revision_after, 1);
    let approval = r.approval.clone().expect("approval recorded");
    assert_eq!(approval.approved_by, "human:user");
    let snap = current_project(&s).unwrap();
    assert_eq!(snap.project.schematic.symbols.len(), 1);
    let ev = events_since(&s, 0).unwrap();
    let applied = ev.iter().find(|e| e.op_id == r.op_id).expect("event");
    assert_eq!(applied.actor, "agent:assistant");
    // A step is accepted once.
    assert!(ai::ai_accept(&s, &st, step.id).is_err());

    let u = ai::ai_rollback(&s, &st, &r.op_id).unwrap();
    assert!(u.accepted, "{:?}", u.error);
    assert_eq!(
        current_project(&s).unwrap().project.schematic.symbols.len(),
        0
    );
    assert!(ai::ai_rollback(&s, &st, &r.op_id).is_err());
}

#[test]
fn reject_records_nothing() {
    let s = open("reject");
    let st = AiState::default();
    let p = ai::propose_with(
        &s,
        &st,
        mock(vec![place_r("c1"), Step::Respond(ChatResponse::text("ok"))]),
        "add a resistor",
    )
    .unwrap();
    let before = events_since(&s, 0).unwrap().len();
    ai::ai_reject(&st, p.steps[0].id).unwrap();
    assert!(ai::ai_accept(&s, &st, p.steps[0].id).is_err());
    assert_eq!(current_project(&s).unwrap().project.revision, 0);
    assert_eq!(events_since(&s, 0).unwrap().len(), before);
}

#[test]
fn a_step_is_refused_when_the_project_changed_since_the_proposal() {
    let s = open("stale");
    let st = AiState::default();
    let p = ai::propose_with(
        &s,
        &st,
        mock(vec![place_r("c1"), Step::Respond(ChatResponse::text("ok"))]),
        "add a resistor",
    )
    .unwrap();
    execute_command(&s, 0, Command::SetProjectName { name: "x".into() }).unwrap();
    let r = ai::ai_accept(&s, &st, p.steps[0].id).unwrap();
    assert!(!r.accepted);
    assert_eq!(
        current_project(&s).unwrap().project.schematic.symbols.len(),
        0
    );
}

#[test]
fn provider_failure_is_reported_not_faked() {
    let s = open("fail");
    let st = AiState::default();
    let p = ai::propose_with(&s, &st, mock(vec![]), "hello").unwrap();
    assert!(matches!(p.outcome, AgentOutcome::ProviderFailed(_)));
    assert!(p.steps.is_empty());
}

#[test]
fn no_key_configured_is_a_plain_error() {
    let s = open("nokey");
    let st = AiState::default();
    let m = ModelSettings::new(
        None,
        Arc::new(MemoryKeyStore::default()),
        default_transport(),
    )
    .unwrap();
    let v = m
        .save(
            ProfileInput {
                model: Some("claude-x".into()),
                ..ProfileInput::new("Claude", "anthropic")
            },
            None,
        )
        .unwrap();
    let e = ai::ai_propose(&s, &st, &m, &v.profile.id, "add a resistor").unwrap_err();
    assert!(e.contains("not ready"), "{e}");
    assert!(ai::ai_propose(&s, &st, &m, "nope", "x").is_err());
}
