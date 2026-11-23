//! ERC for buses, bus entries and port scope (ADR-0040): ERC-017..021, and the bus rules that are
//! not checked yet are listed as not implemented instead of passing.

use eak_core::*;
use eak_runtime::command::Command as C;
use eak_runtime::{Actor, CommandRequest, Engine};

fn run(e: &mut Engine, cmd: C) -> Id {
    let rev = e.project().revision;
    let r = e.execute(CommandRequest {
        expected_revision: rev,
        actor: Actor::human(),
        command: cmd,
        approval: None,
    });
    assert!(r.accepted, "{}: {:?}", r.command, r.error);
    r.created.first().cloned().unwrap_or_default()
}

fn count(p: &Project, rule: &str) -> usize {
    eak_validation::erc::run(p)
        .findings
        .iter()
        .filter(|f| f.rule_id == rule)
        .count()
}

#[test]
fn bus_rules_are_listed_and_unchecked_bus_rules_are_not_implemented() {
    let r = eak_validation::erc::run(&Project::new("empty"));
    for id in ["ERC-017", "ERC-018", "ERC-019", "ERC-020", "ERC-021"] {
        assert!(r.rules.iter().any(|x| x.id == id), "{id} not listed");
    }
    let ni = r.not_implemented.join("\n");
    for topic in [
        "bus entry",
        "over each other",
        "grid",
        "Net classes",
        "hierarch",
    ] {
        assert!(ni.contains(topic), "not_implemented lacks '{topic}': {ni}");
    }
}

fn label(name: &str, at: Point, scope: NetScope) -> C {
    C::AddNetLabel {
        sheet_id: None,
        name: name.into(),
        position: at,
        kind: LabelKind::Net,
        scope,
        power_style: None,
    }
}

#[test]
fn bus_entry_and_bus_name_findings() {
    let mut e = Engine::in_memory(Project::new("bus"));
    let bus = run(
        &mut e,
        C::AddBus {
            sheet_id: None,
            name: "D[0..1]".into(),
            points: vec![Point::mm(0.0, 50.0), Point::mm(100.0, 50.0)],
        },
    );
    run(
        &mut e,
        C::AddBus {
            sheet_id: None,
            name: "DATA".into(),
            points: vec![Point::mm(0.0, 150.0), Point::mm(50.0, 150.0)],
        },
    );
    assert_eq!(count(e.project(), "ERC-017"), 1, "illegal bus name");

    // An entry whose wire reaches nothing is not a member (ERC-018).
    let entry = run(
        &mut e,
        C::AddBusEntry {
            bus_id: bus.clone(),
            bus_point: Point::mm(10.0, 50.0),
            wire_point: Point::mm(12.54, 52.54),
        },
    );
    assert_eq!(count(e.project(), "ERC-018"), 1);
    // Label the entry's wire with a member: the finding goes away.
    run(
        &mut e,
        C::AddWire {
            sheet_id: None,
            a: Point::mm(12.54, 52.54),
            b: Point::mm(12.54, 70.0),
        },
    );
    let d7 = run(
        &mut e,
        label("D7", Point::mm(12.54, 70.0), NetScope::Global),
    );
    assert_eq!(count(e.project(), "ERC-018"), 1, "D7 is not in D[0..1]");
    run(&mut e, C::DeleteNetLabel { label_id: d7 });
    run(
        &mut e,
        label("D1", Point::mm(12.54, 70.0), NetScope::Global),
    );
    assert_eq!(count(e.project(), "ERC-018"), 0);
    assert_eq!(count(e.project(), "ERC-019"), 0);

    // Reshaping the bus leaves the entry off it (ERC-019).
    run(
        &mut e,
        C::MoveBus {
            bus_id: bus.clone(),
            points: vec![Point::mm(0.0, 40.0), Point::mm(100.0, 40.0)],
        },
    );
    assert_eq!(count(e.project(), "ERC-019"), 1);
    run(&mut e, C::DeleteBusEntry { entry_id: entry });
    assert_eq!(count(e.project(), "ERC-019"), 0);

    // A port on the bus must carry the bus name (ERC-020).
    run(
        &mut e,
        label("DATA_BUS", Point::mm(60.0, 40.0), NetScope::Global),
    );
    assert_eq!(count(e.project(), "ERC-020"), 1);
}

#[test]
fn off_page_connector_needs_a_partner_on_another_sheet() {
    let mut e = Engine::in_memory(Project::new("offpage"));
    let off = run(
        &mut e,
        label("CLK", Point::mm(20.0, 20.0), NetScope::OffPage),
    );
    assert_eq!(count(e.project(), "ERC-021"), 1);
    let s2 = run(
        &mut e,
        C::AddSheet {
            name: "Sheet 2".into(),
        },
    );
    let local = run(
        &mut e,
        C::AddNetLabel {
            sheet_id: Some(s2),
            name: "CLK".into(),
            position: Point::mm(20.0, 20.0),
            kind: LabelKind::Net,
            scope: NetScope::Local,
            power_style: None,
        },
    );
    assert_eq!(
        count(e.project(), "ERC-021"),
        1,
        "a local label is no partner"
    );
    run(
        &mut e,
        C::SetNetLabelScope {
            label_id: local,
            scope: NetScope::OffPage,
        },
    );
    assert_eq!(count(e.project(), "ERC-021"), 0);
    // Global labels (every v2 label) never raise it.
    run(
        &mut e,
        C::SetNetLabelScope {
            label_id: off,
            scope: NetScope::Global,
        },
    );
    assert_eq!(count(e.project(), "ERC-021"), 0);
}
