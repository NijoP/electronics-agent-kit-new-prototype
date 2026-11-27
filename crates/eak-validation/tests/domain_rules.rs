//! ADR-0061 (M4.4): the versioned rule registry and domain-overlay composition, driven through
//! `Engine::execute`. A recorded domain class adds the rules it requires; composition never
//! drops a rule; a project configuration that weakens a required rule fails the `domain_rules`
//! gate instead of being applied silently; concerns no evaluator covers stay UNKNOWN.
//! Written without running (no local builds): CI decides.

use eak_core::requirements::SourceKind;
use eak_core::{Project, ReqSource, ReqStatus, ReqValue, RuleConfig, RuleSeverity};
use eak_runtime::{Actor, Command as C, CommandRequest, CommandResponse, Engine};
use eak_validation::rules::{self, CONCERNS_GATE_ID, GATE_ID};
use eak_validation::{gates, GateState};

fn run(e: &mut Engine, command: C) -> CommandResponse {
    let expected_revision = e.project().revision;
    let r = e.execute(CommandRequest {
        expected_revision,
        actor: Actor::human(),
        command,
        approval: None,
    });
    assert!(r.accepted, "{:?}", r.error);
    r
}

fn class(e: &mut Engine, id: &str, applies: bool, status: ReqStatus) {
    run(
        e,
        C::SetRequirementField {
            path: format!("domain.{id}"),
            status,
            value: Some(ReqValue::Flag { value: applies }),
            source: Some(ReqSource {
                kind: SourceKind::Engineer,
                reference: "design review 2026-11-27".into(),
            }),
            derived_from: vec![],
            owner: Some("Lead engineer".into()),
            risk_if_wrong: (status == ReqStatus::NeedsConfirmation)
                .then(|| "the board's domain rules are wrong".to_string()),
            note: None,
            supersede: false,
        },
    );
}

fn configure(e: &mut Engine, rule: &str, config: Option<RuleConfig>) {
    run(
        e,
        C::SetRuleConfig {
            rule_id: rule.into(),
            config,
        },
    );
}

fn gate(p: &Project, id: &str) -> gates::Gate {
    gates::evaluate(p, None)
        .gates
        .into_iter()
        .find(|g| g.id == id)
        .unwrap()
}

#[test]
fn an_unclassified_board_never_passes_the_domain_gate() {
    let p = Project::new("blank");
    let c = rules::compose(&p);
    assert!(!c.classified);
    assert!(c.active.is_empty() && c.rules.is_empty());
    let g = gate(&p, GATE_ID);
    assert!(g.required);
    assert_eq!(g.state, GateState::Unknown);
    assert!(g.reasons[0].contains("ClassifyDomain"), "{:?}", g.reasons);
    let r = gates::evaluate(&p, None);
    assert!(r.blocking.iter().any(|b| b.starts_with(GATE_ID)));
    let concerns = gate(&p, CONCERNS_GATE_ID);
    assert!(!concerns.required);
    assert_eq!(concerns.state, GateState::Unknown);
}

#[test]
fn recorded_classes_compose_their_rules_and_a_rejection_adds_none() {
    let mut e = Engine::in_memory(Project::new("usb"));
    class(&mut e, "embedded_mcu", true, ReqStatus::Confirmed);
    class(&mut e, "usb_interface", true, ReqStatus::Confirmed);
    class(&mut e, "high_voltage", false, ReqStatus::Confirmed);
    let c = rules::compose(e.project());
    let active: Vec<&str> = c.active.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(active, ["embedded_mcu", "usb_interface"]);
    assert_eq!(c.not_applying, ["high_voltage"]);
    let ids: Vec<&str> = c.rules.iter().map(|r| r.rule_id.as_str()).collect();
    for r in [
        "DRC-024", "DRC-025", "DRC-026", "ERC-001", "ERC-003", "ERC-010",
    ] {
        assert!(ids.contains(&r), "{r} missing from {ids:?}");
    }
    assert!(!ids.contains(&"DRC-002"), "a rejected class adds no rule");
    assert_eq!(c.conflicts().count(), 0);
    assert_eq!(gate(e.project(), GATE_ID).state, GateState::Pass);
    // Concerns without an evaluator are listed, never passed.
    let concerns = gate(e.project(), CONCERNS_GATE_ID);
    assert_eq!(concerns.state, GateState::Unknown);
    assert!(
        concerns
            .reasons
            .iter()
            .any(|r| r.contains("ESD protection")),
        "{:?}",
        concerns.reasons
    );
}

#[test]
fn a_project_configuration_never_silently_weakens_a_required_rule() {
    let mut e = Engine::in_memory(Project::new("usb"));
    class(&mut e, "embedded_mcu", true, ReqStatus::Confirmed);
    class(&mut e, "usb_interface", true, ReqStatus::Confirmed);
    configure(
        &mut e,
        "DRC-025",
        Some(RuleConfig {
            enabled: false,
            severity: None,
        }),
    );
    let g = gate(e.project(), GATE_ID);
    assert_eq!(g.state, GateState::Fail, "{:?}", g.reasons);
    assert!(g.reasons.iter().any(|r| r.contains("DRC-025 is disabled")));
    assert!(gates::evaluate(e.project(), None)
        .blocking
        .iter()
        .any(|b| b.starts_with(GATE_ID)));

    configure(
        &mut e,
        "DRC-025",
        Some(RuleConfig {
            enabled: true,
            severity: Some(RuleSeverity::Warning),
        }),
    );
    let g = gate(e.project(), GATE_ID);
    assert_eq!(g.state, GateState::Fail);
    assert!(
        g.reasons.iter().any(|r| r.contains("at error")),
        "{:?}",
        g.reasons
    );

    // Raising to error is stricter and accepted; clearing the configuration restores the default.
    configure(
        &mut e,
        "DRC-025",
        Some(RuleConfig {
            enabled: true,
            severity: Some(RuleSeverity::Error),
        }),
    );
    assert_eq!(gate(e.project(), GATE_ID).state, GateState::Pass);
    configure(&mut e, "DRC-025", None);
    assert_eq!(gate(e.project(), GATE_ID).state, GateState::Pass);
}

#[test]
fn a_conflicting_true_keeps_the_stricter_reading_until_a_person_resolves_it() {
    let mut e = Engine::in_memory(Project::new("hv"));
    class(&mut e, "dcdc_power", true, ReqStatus::Confirmed);
    class(&mut e, "high_voltage", false, ReqStatus::Confirmed);
    // A second source says it applies: recorded beside the first as a conflict (ADR-0060).
    class(&mut e, "high_voltage", true, ReqStatus::NeedsConfirmation);
    let c = rules::compose(e.project());
    assert!(c.active.iter().any(|a| a.id == "high_voltage"), "{c:?}");
    assert!(c.not_firm.contains(&"high_voltage".to_string()));
    assert!(c.rules.iter().any(|r| r.rule_id == "DRC-002"));
    let g = gate(e.project(), GATE_ID);
    assert_eq!(g.state, GateState::Unknown);
    assert!(g
        .reasons
        .iter()
        .any(|r| r.contains("domain.high_voltage is not firm")));
}

#[test]
fn a_provisional_archetype_leaves_the_gate_unknown() {
    let mut e = Engine::in_memory(Project::new("rf"));
    class(&mut e, "rf_wireless", true, ReqStatus::NeedsConfirmation);
    let c = rules::compose(e.project());
    assert_eq!(c.active.len(), 1);
    assert!(c.rules.iter().any(|r| r.rule_id == "DRC-021"));
    assert_eq!(gate(e.project(), GATE_ID).state, GateState::Unknown);
}
