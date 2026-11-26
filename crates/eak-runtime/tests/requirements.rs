//! Schema v9 and v11 (ADR-0057, ADR-0060) through the engine: the requirement commands are the
//! only write path into the structured requirement record. Each is one undo step, a refusal changes
//! nothing, an agent never confirms, derives, resolves or replaces a person's requirement, a
//! different value is a conflict until a person resolves it, the domain classifier records its
//! result through ClassifyDomain, and the record survives a save and load. Written without running
//! (no local builds): CI decides.

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
    set_from(path, status, value, client(), false)
}

fn set_from(
    path: &str,
    status: ReqStatus,
    value: Option<ReqValue>,
    source: Option<ReqSource>,
    supersede: bool,
) -> C {
    C::SetRequirementField {
        path: path.into(),
        status,
        value,
        source,
        derived_from: vec![],
        owner: Some("Client PM".into()),
        risk_if_wrong: (status == ReqStatus::NeedsConfirmation)
            .then(|| "the input protection is re-selected".to_string()),
        note: None,
        supersede,
    }
}

fn source(kind: SourceKind, reference: &str) -> Option<ReqSource> {
    Some(ReqSource {
        kind,
        reference: reference.into(),
    })
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
    // DERIVED comes only from a deterministic evaluator: a person cannot record it either.
    let r = refused_as(
        &mut e,
        Actor::human(),
        set(path, ReqStatus::Derived, input_5v()),
    );
    assert!(
        matches!(r.error, Some(CommandError::Precondition(_))),
        "{:?}",
        r.error
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

fn input(nom: f64) -> Option<ReqValue> {
    Some(ReqValue::Range {
        min: Some(nom * 0.9),
        nom: Some(nom),
        max: Some(nom * 1.1),
        unit: ReqUnit::Volt,
    })
}

fn precondition(r: &CommandResponse) {
    assert!(
        matches!(r.error, Some(CommandError::Precondition(_))),
        "{:?}",
        r.error
    );
}

#[test]
fn a_different_value_is_a_conflict_until_a_person_resolves_it() {
    let mut e = Engine::in_memory(Project::new("v11-conflicts"));
    let agent = Actor::agent_in_session("claude", "s1");
    let path = "supply.input_voltage";
    ok(&mut e, set(path, ReqStatus::Confirmed, input(12.0)));
    // A different value never overwrites the recorded one: it is kept beside it.
    let other = set(path, ReqStatus::NeedsConfirmation, input(24.0));
    let r = ok(&mut e, other);
    assert!(r.warnings.iter().any(|w| w.contains("conflict")), "{r:?}");
    let entry = e.project().requirement_set.fields[path].clone();
    assert_eq!(entry.value, input(12.0));
    assert_eq!(entry.status, ReqStatus::Confirmed);
    assert_eq!(entry.conflicts.len(), 1);
    assert_eq!(entry.conflicts[0].value, input(24.0));
    // While in conflict the value counts as missing.
    let report = evaluate(&e.project().requirement_set);
    let f = report.fields.iter().find(|f| f.path == path).unwrap();
    assert_eq!(f.level, Level::Missing);
    assert_eq!(f.conflicts.len(), 1);

    // An agent may add its own differing value as another conflict...
    let proposal = set_from(
        path,
        ReqStatus::NeedsConfirmation,
        input(48.0),
        source(SourceKind::AgentProposal, "adapter datasheet"),
        false,
    );
    let r = run_as(&mut e, agent.clone(), proposal);
    assert!(r.accepted, "{:?}", r.error);
    assert_eq!(e.project().requirement_set.fields[path].conflicts.len(), 2);
    // ...but never replace, clear or resolve a person's decision or a conflict.
    let nc = ReqStatus::NeedsConfirmation;
    let replace = set_from(path, nc, input(48.0), client(), true);
    precondition(&refused_as(&mut e, agent.clone(), replace.clone()));
    precondition(&refused_as(
        &mut e,
        agent.clone(),
        C::Batch {
            commands: vec![replace],
            label: None,
        },
    ));
    precondition(&refused_as(
        &mut e,
        agent.clone(),
        C::ClearRequirementField { path: path.into() },
    ));
    let resolve = |keep: usize| C::ResolveRequirementConflict {
        path: path.into(),
        keep,
        owner: "A. Engineer".into(),
        note: Some("the adapter label is authoritative".into()),
    };
    precondition(&refused_as(&mut e, agent, resolve(1)));
    // Out of range is refused for a person too.
    refused_as(&mut e, Actor::human(), resolve(3));

    // A person resolves: the kept value is CONFIRMED by them and the others are dropped.
    ok(&mut e, resolve(1));
    let entry = e.project().requirement_set.fields[path].clone();
    assert_eq!(entry.value, input(24.0));
    assert_eq!(entry.status, ReqStatus::Confirmed);
    assert_eq!(entry.owner.as_deref(), Some("A. Engineer"));
    assert!(entry.conflicts.is_empty() && entry.risk_if_wrong.is_none());
    refused_as(&mut e, Actor::human(), resolve(1));
    // One undo step brings the conflict back; it survives a save and load.
    let rev = e.project().revision;
    assert!(e.undo(rev, &Actor::human()).accepted);
    assert_eq!(e.project().requirement_set.fields[path].conflicts.len(), 2);
    let saved = to_json(e.project());
    assert!(saved.contains("\"conflicts\""));
    let back = from_json(&saved).expect("a v11 project loads");
    assert_eq!(&back, e.project());
    // A person may supersede: the new value replaces the old and its conflicts.
    let five = set_from(path, ReqStatus::Confirmed, input(5.0), client(), true);
    ok(&mut e, five);
    let entry = &e.project().requirement_set.fields[path];
    assert_eq!(entry.value, input(5.0));
    assert!(entry.conflicts.is_empty());
}

#[test]
fn classify_domain_records_the_classifier_result_as_derived() {
    use eak_engineering::domain;
    let mut e = Engine::in_memory(Project::new("v11-classify"));
    // Nothing recorded: nothing to classify.
    let r = refused_as(&mut e, Actor::human(), C::ClassifyDomain);
    assert!(
        matches!(r.error, Some(CommandError::NothingTo(_))),
        "{:?}",
        r.error
    );
    let r = ok(
        &mut e,
        C::UpsertRequirementItem {
            item_id: None,
            kind: ReqItemKind::Interface,
            name: "Radio".into(),
            parent: None,
        },
    );
    let radio = r.created[0].clone();
    let ble = Some(ReqValue::Text {
        text: "BLE 5".into(),
    });
    let protocol = format!("{radio}/protocol");
    ok(&mut e, set(&protocol, ReqStatus::Confirmed, ble));
    // No command records DERIVED for a domain class directly.
    let flag = Some(ReqValue::Flag { value: true });
    precondition(&refused_as(
        &mut e,
        Actor::human(),
        set("domain.rf_wireless", ReqStatus::Derived, flag),
    ));

    // An agent may run the classifier: its result is the evaluator's, not the agent's.
    let agent = Actor::agent_in_session("claude", "s1");
    let r = run_as(&mut e, agent.clone(), C::ClassifyDomain);
    assert!(r.accepted, "{:?}", r.error);
    let rf = e.project().requirement_set.fields["domain.rf_wireless"].clone();
    assert_eq!(rf.status, ReqStatus::Derived);
    assert_eq!(rf.value, Some(ReqValue::Flag { value: true }));
    assert_eq!(rf.derived_from, vec![protocol.clone()]);
    let source = rf.source.as_ref().unwrap();
    assert_eq!(source.kind, SourceKind::Calculation);
    assert!(source.reference.starts_with(domain::EVALUATOR));
    assert!(rf.note.as_deref().unwrap().contains(&protocol));
    // Up to date: classifying again changes nothing.
    refused_as(&mut e, agent.clone(), C::ClassifyDomain);

    // A person rejects the class; the classifier keeps its view beside it, never over it.
    let reject = set_from(
        "domain.rf_wireless",
        ReqStatus::Confirmed,
        Some(ReqValue::Flag { value: false }),
        source(SourceKind::Engineer, "design review"),
        true,
    );
    ok(&mut e, reject);
    let r = run_as(&mut e, agent, C::ClassifyDomain);
    assert!(r.accepted, "{:?}", r.error);
    let rf = e.project().requirement_set.fields["domain.rf_wireless"].clone();
    assert_eq!(rf.status, ReqStatus::Confirmed);
    assert_eq!(rf.value, Some(ReqValue::Flag { value: false }));
    assert_eq!(rf.conflicts.len(), 1);
    assert!(domain::by_classifier(&rf.conflicts[0]));
    // Each run is one undo step.
    let rev = e.project().revision;
    assert!(e.undo(rev, &Actor::human()).accepted);
    assert!(e.project().requirement_set.fields["domain.rf_wireless"]
        .conflicts
        .is_empty());
}
