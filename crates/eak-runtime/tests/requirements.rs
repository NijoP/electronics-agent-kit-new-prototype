//! Schema v9 (ADR-0057) through the engine: the requirement commands are the only write path
//! into the structured requirement record. Each is one undo step, a refusal changes nothing, an
//! agent never confirms or derives a requirement, and the record survives a save and load. Written without
//! running (no local builds): CI decides.

use eak_core::requirements::{evaluate, Level, Operation, SourceKind};
use eak_core::{from_json, to_json, Project, ReqItemKind, ReqSource, ReqStatus, ReqUnit, ReqValue};
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;
use eak_runtime::{CommandError, OperationClass};

fn run_as(e: &mut Engine, actor: Actor, cmd: C) -> CommandResponse {
    let rev = e.project().revision;
    let approval = (actor.kind == ActorKind::Agent).then(|| Approval {
        approved_by: "user".into(),
        max_class: OperationClass::Destructive,
        note: String::new(),
    });
    e.execute(CommandRequest {
        expected_revision: rev,
        actor,
        command: cmd,
        approval,
    })
}

fn ok(e: &mut Engine, cmd: C) -> CommandResponse {
    let name = cmd.name();
    let r = run_as(e, Actor::human(), cmd);
    assert!(r.accepted, "{name}: {:?}", r.error);
    r
}

fn refused_as(e: &mut Engine, actor: Actor, cmd: C) -> CommandResponse {
    let name = cmd.name();
    let before = e.project().clone();
    let r = run_as(e, actor, cmd);
    assert!(!r.accepted, "{name} was accepted");
    assert_eq!(
        e.project(),
        &before,
        "{name}: a refusal changed the project"
    );
    r
}

fn client() -> Option<ReqSource> {
    Some(ReqSource {
        kind: SourceKind::Client,
        reference: "kick-off call 2026-11-20".into(),
    })
}

fn set(path: &str, status: ReqStatus, value: Option<ReqValue>) -> C {
    C::SetRequirementField {
        path: path.into(),
        status,
        value,
        source: client(),
        derived_from: vec![],
        owner: Some("Client PM".into()),
        risk_if_wrong: (status == ReqStatus::NeedsConfirmation)
            .then(|| "the input protection is re-selected".to_string()),
        note: None,
    }
}

fn input_5v() -> Option<ReqValue> {
    Some(ReqValue::Range {
        min: Some(4.5),
        nom: Some(5.0),
        max: Some(5.5),
        unit: ReqUnit::Volt,
    })
}

#[test]
fn requirement_fields_are_set_stamped_undone_and_saved() {
    let mut e = Engine::in_memory(Project::new("v9"));
    let path = "supply.input_voltage";
    ok(&mut e, set(path, ReqStatus::NeedsConfirmation, input_5v()));
    let entry = e.project().requirement_set.fields[path].clone();
    assert_eq!(entry.status, ReqStatus::NeedsConfirmation);
    assert_eq!(entry.updated_revision, e.project().revision);
    assert!(entry.updated_at_ms > 0, "the engine stamps the time");
    assert_eq!(entry.owner.as_deref(), Some("Client PM"));

    // The same content again is no change; an invalid value is refused without a trace.
    refused_as(
        &mut e,
        Actor::human(),
        set(path, ReqStatus::NeedsConfirmation, input_5v()),
    );
    let amps = Some(ReqValue::Number {
        value: 5.0,
        unit: ReqUnit::Ampere,
        tolerance_pct: None,
    });
    refused_as(
        &mut e,
        Actor::human(),
        set(path, ReqStatus::NeedsConfirmation, amps),
    );
    refused_as(
        &mut e,
        Actor::human(),
        set(path, ReqStatus::Unknown, input_5v()),
    );
    refused_as(
        &mut e,
        Actor::human(),
        set("supply.not_a_field", ReqStatus::Unknown, None),
    );

    ok(&mut e, set(path, ReqStatus::Confirmed, input_5v()));
    assert_eq!(
        e.project().requirement_set.fields[path].status,
        ReqStatus::Confirmed
    );
    let rev = e.project().revision;
    assert!(e.undo(rev, &Actor::human()).accepted);
    assert_eq!(
        e.project().requirement_set.fields[path].status,
        ReqStatus::NeedsConfirmation
    );
    let rev = e.project().revision;
    assert!(e.redo(rev, &Actor::human()).accepted);

    let saved = to_json(e.project());
    assert!(saved.contains("\"requirement_set\""));
    let back = from_json(&saved).expect("a v9 project loads");
    assert_eq!(&back, e.project());
    assert_eq!(saved, to_json(&back));

    ok(&mut e, C::ClearRequirementField { path: path.into() });
    assert!(e.project().requirement_set.is_empty());
    refused_as(
        &mut e,
        Actor::human(),
        C::ClearRequirementField { path: path.into() },
    );
}

#[test]
fn an_agent_proposes_values_but_never_confirms_them() {
    let mut e = Engine::in_memory(Project::new("v9-agent"));
    let agent = Actor::agent_in_session("claude", "s1");
    let path = "environment.operating_temp";
    let temp = Some(ReqValue::Range {
        min: Some(-20.0),
        nom: None,
        max: Some(60.0),
        unit: ReqUnit::DegreeCelsius,
    });
    let r = run_as(
        &mut e,
        agent.clone(),
        set(path, ReqStatus::NeedsConfirmation, temp.clone()),
    );
    assert!(r.accepted, "{:?}", r.error);
    for status in [
        ReqStatus::Confirmed,
        ReqStatus::Derived,
        ReqStatus::NotApplicable,
    ] {
        let r = refused_as(&mut e, agent.clone(), set(path, status, temp.clone()));
        assert!(
            matches!(r.error, Some(CommandError::Precondition(_))),
            "{:?}",
            r.error
        );
    }
    // Not through a batch either.
    refused_as(
        &mut e,
        agent,
        C::Batch {
            commands: vec![set(path, ReqStatus::Confirmed, temp.clone())],
            label: None,
        },
    );
    // A person confirms it.
    ok(&mut e, set(path, ReqStatus::Confirmed, temp));
    assert_eq!(
        e.project().requirement_set.fields[path].status,
        ReqStatus::Confirmed
    );
}

#[test]
fn rails_and_interfaces_are_items_with_their_own_fields() {
    let mut e = Engine::in_memory(Project::new("v9-items"));
    let r = ok(
        &mut e,
        C::UpsertRequirementItem {
            item_id: None,
            kind: ReqItemKind::Rail,
            name: "VBUS".into(),
            parent: None,
        },
    );
    let vbus = r.created[0].clone();
    let r = ok(
        &mut e,
        C::UpsertRequirementItem {
            item_id: None,
            kind: ReqItemKind::Rail,
            name: "3V3".into(),
            parent: Some(vbus.clone()),
        },
    );
    let v3 = r.created[0].clone();
    assert_eq!(
        e.project().requirement_set.items[&v3].parent.as_deref(),
        Some(vbus.as_str())
    );
    let peak = Some(ReqValue::Number {
        value: 300.0,
        unit: ReqUnit::Milliampere,
        tolerance_pct: None,
    });
    ok(
        &mut e,
        set(&format!("{v3}/current_peak"), ReqStatus::Confirmed, peak),
    );
    // A field the rail kind does not have, and a parent that would make a loop, are refused.
    refused_as(
        &mut e,
        Actor::human(),
        set(&format!("{v3}/protocol"), ReqStatus::Unknown, None),
    );
    refused_as(
        &mut e,
        Actor::human(),
        C::UpsertRequirementItem {
            item_id: Some(vbus.clone()),
            kind: ReqItemKind::Rail,
            name: "VBUS".into(),
            parent: Some(v3.clone()),
        },
    );
    // A rail another rail is generated from cannot be deleted.
    refused_as(
        &mut e,
        Actor::human(),
        C::DeleteRequirementItem {
            item_id: vbus.clone(),
        },
    );

    // The evaluation sees the rail tree and what still blocks power sizing.
    let report = evaluate(&e.project().requirement_set);
    let sizing = report
        .operations
        .iter()
        .find(|o| o.operation == Operation::PowerSizing)
        .unwrap();
    assert!(!sizing.ready);
    assert!(!sizing
        .blocked_by
        .iter()
        .any(|b| b.path == format!("{v3}/current_peak")));
    assert!(sizing
        .blocked_by
        .iter()
        .any(|b| b.path == format!("{vbus}/voltage") && b.level == Level::Missing));

    // Deleting the leaf is one undo step that brings back its fields.
    ok(
        &mut e,
        C::DeleteRequirementItem {
            item_id: v3.clone(),
        },
    );
    assert!(!e.project().requirement_set.items.contains_key(&v3));
    let rev = e.project().revision;
    assert!(e.undo(rev, &Actor::human()).accepted);
    assert!(e.project().requirement_set.items[&v3]
        .fields
        .contains_key("current_peak"));
}
