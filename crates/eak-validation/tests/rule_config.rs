//! ADR-0046 (AD-8, Phase 1 B5/C5): per-rule enable and severity override in the IR
//! constraints. A disabled rule is reported as `DISABLED`, never as passing; an override
//! changes the findings' severity and is stated in the report, never the verdict of the gates.

use eak_core::{known_rule_id, Point, Project, Rotation, RuleConfig, RuleSeverity, RULE_FAMILIES};
use eak_runtime::{fixtures, Actor, Command as C, CommandRequest, CommandResponse, Engine};
use eak_validation::{dfm, drc, erc, gates, mech, sourcing, CheckState, GateState, Severity};
use std::collections::BTreeSet;

fn run(e: &mut Engine, command: C) -> CommandResponse {
    let expected_revision = e.project().revision;
    e.execute(CommandRequest {
        expected_revision,
        actor: Actor::human(),
        command,
        approval: None,
    })
}

/// One resistor with both pins unconnected: ERC-001 fails.
fn floating_resistor() -> Engine {
    let mut e = Engine::in_memory(Project::new("rules"));
    let r = run(
        &mut e,
        C::PlaceSymbol {
            sheet_id: None,
            component_id: "R_0603".into(),
            position: Point::mm(50.0, 50.0),
            rotation: Rotation::R0,
            refdes: None,
            value: None,
        },
    );
    assert!(r.accepted, "{:?}", r.error);
    e
}

fn reference() -> Engine {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).expect("schematic");
    fixtures::build_pcb(&mut e).expect("pcb");
    e
}

const OFF: RuleConfig = RuleConfig {
    enabled: false,
    severity: None,
};

fn lowered(s: RuleSeverity) -> RuleConfig {
    RuleConfig {
        enabled: true,
        severity: Some(s),
    }
}

fn configure(e: &mut Engine, rule: &str, config: Option<RuleConfig>) {
    let r = run(
        e,
        C::SetRuleConfig {
            rule_id: rule.into(),
            config,
        },
    );
    assert!(r.accepted, "{:?}", r.error);
}

#[test]
fn a_disabled_rule_is_reported_disabled_never_pass() {
    let mut e = floating_resistor();
    assert_eq!(erc::run(e.project()).state_of("ERC-001"), CheckState::Fail);
    configure(&mut e, "ERC-001", Some(OFF));
    let r = erc::run(e.project());
    assert_eq!(r.state_of("ERC-001"), CheckState::Disabled);
    assert!(!r.state_of("ERC-001").certifies());
    assert!(r.findings.iter().all(|f| f.rule_id != "ERC-001"));
    let rr = r
        .rule_results
        .iter()
        .find(|x| x.rule_id == "ERC-001")
        .unwrap();
    assert!(rr.reason.contains("disabled"), "{}", rr.reason);
    assert_ne!(
        r.overall,
        CheckState::Pass,
        "a report with a disabled rule never passes"
    );
    assert!(!r.overall.certifies());
    // Not checked, so it is a risk record like an unknown rule.
    assert!(r.risks().iter().any(|k| k.id == "RISK-ERC-001"));
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.contains("\"DISABLED\""), "{json}");
    // Clearing the configuration restores the default.
    configure(&mut e, "ERC-001", None);
    assert!(e.project().constraints.rules.is_empty());
    assert_eq!(erc::run(e.project()).state_of("ERC-001"), CheckState::Fail);
}

#[test]
fn a_severity_override_is_shown_and_never_hides_the_default_verdict() {
    let mut e = floating_resistor();
    configure(&mut e, "ERC-001", Some(lowered(RuleSeverity::Warning)));
    let r = erc::run(e.project());
    let mine: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule_id == "ERC-001")
        .collect();
    assert!(!mine.is_empty());
    assert!(mine.iter().all(|f| f.severity == Severity::Warning));
    let rr = r
        .rule_results
        .iter()
        .find(|x| x.rule_id == "ERC-001")
        .unwrap();
    assert_eq!(rr.state, CheckState::Warning);
    assert_eq!(rr.severity_override, Some(Severity::Warning));
    assert_eq!(rr.default_state, Some(CheckState::Fail));
    assert_eq!(
        r.overall,
        CheckState::Fail,
        "the verdict stays at the default severity"
    );
}

#[test]
fn the_configuration_round_trips_through_save_and_open() {
    let mut e = floating_resistor();
    configure(&mut e, "DRC-002", Some(lowered(RuleSeverity::Info)));
    configure(&mut e, "ERC-006", Some(OFF));
    let saved = eak_core::to_json(e.project());
    let opened = eak_core::from_json(&saved).expect("opens");
    assert_eq!(&opened, e.project());
    assert_eq!(opened.constraints.rules["ERC-006"], OFF);
    assert_eq!(erc::run(&opened).state_of("ERC-006"), CheckState::Disabled);
}

#[test]
fn unknown_rule_ids_are_refused() {
    let mut e = floating_resistor();
    for bad in ["ERC-999", "ERC-1", "XYZ-001", "erc-001", ""] {
        let r = run(
            &mut e,
            C::SetRuleConfig {
                rule_id: bad.into(),
                config: Some(OFF),
            },
        );
        assert!(!r.accepted, "{bad:?} must be refused");
    }
    assert!(e.project().constraints.rules.is_empty());
}

#[test]
fn a_gate_with_a_disabled_rule_is_not_run_and_overrides_do_not_pass_gates() {
    let mut e = reference();
    configure(&mut e, "DRC-003", Some(OFF));
    configure(&mut e, "DRC-009", Some(lowered(RuleSeverity::Info)));
    let g = gates::evaluate(e.project(), None);
    let shorts = g.gates.iter().find(|x| x.id == "no_shorts").unwrap();
    assert_eq!(shorts.state, GateState::NotRun);
    assert!(
        shorts.reasons[0].contains("DRC-003"),
        "{:?}",
        shorts.reasons
    );
    assert!(!g.required_passed);
    let outline = g.gates.iter().find(|x| x.id == "board_outline").unwrap();
    assert!(
        outline
            .reasons
            .iter()
            .any(|r| r.contains("default severity")),
        "{:?}",
        outline.reasons
    );

    // A broken outline whose rule is lowered to info still fails the gate.
    let mut broken = e.project().clone();
    broken.pcb.board.outline.truncate(2);
    broken.pcb.board.outline_arcs.clear();
    let g = gates::evaluate(&broken, None);
    let outline = g.gates.iter().find(|x| x.id == "board_outline").unwrap();
    assert_eq!(outline.state, GateState::Fail);
}

/// `RULE_FAMILIES` (what the engine accepts) lists exactly the rules the checks declare.
#[test]
fn known_rule_ids_are_the_rules_the_checks_declare() {
    let e = reference();
    let p = e.project();
    let declared: BTreeSet<String> = [
        erc::run(p),
        drc::run(p),
        dfm::run(p, &dfm::fabricator_standard()),
        mech::run(p),
        sourcing::run(p, &[]),
    ]
    .iter()
    .flat_map(|r| r.rules.iter().map(|x| x.id.clone()))
    .collect();
    let known: BTreeSet<String> = RULE_FAMILIES
        .iter()
        .flat_map(|(f, n)| (1..=*n).map(move |i| format!("{f}-{i:03}")))
        .collect();
    assert_eq!(declared, known);
    assert!(declared.iter().all(|id| known_rule_id(id)));
}
