//! Part stress (ERC-011, ERC-022..024) and the `part_ratings` gate: PASS, FAIL and UNKNOWN from
//! the declared net voltages and currents and the library ratings of the reference schematic.

use eak_core::{Project, Rating};
use eak_engineering::netlist;
use eak_runtime::{fixtures, Engine};
use eak_validation::{erc, gates, CheckState, Finding, GateState, Report, Severity};

/// The reference schematic (J1 +5V/GND, U1 AMS1117, a 10k +3V3/RESET, D1 LED, J2 header) with
/// every net declared at `volts` V and `amps` A, GND at 0 V.
fn reference(volts: f64, amps: f64) -> Project {
    let mut e = Engine::in_memory(Project::new("stress"));
    fixtures::build_schematic(&mut e).unwrap();
    let mut p = e.project().clone();
    for net in netlist::build(&p).nets.keys() {
        let v = if net == "GND" { 0.0 } else { volts };
        p.constraints.net_voltages_v.insert(net.clone(), v);
        p.constraints.net_currents_a.insert(net.clone(), amps);
    }
    p
}

fn on<'a>(r: &'a Report, rule: &str, refdes: &str) -> Vec<&'a Finding> {
    let head = format!("{refdes} ");
    r.findings
        .iter()
        .filter(|f| f.rule_id == rule && f.message.starts_with(&head))
        .collect()
}

fn reason(r: &Report, rule: &str) -> String {
    let rr = r.rule_results.iter().find(|x| x.rule_id == rule).unwrap();
    rr.reason.clone()
}

fn gate(p: &Project) -> GateState {
    let g = gates::evaluate(p, None);
    let part = g.gates.iter().find(|x| x.id == "part_ratings").unwrap();
    part.state
}

/// Gives J1 (two nets: +5V and GND, so the voltage on it is exact) a 12 V rating.
fn rate_j1(p: &mut Project) {
    let j1 = p.schematic.symbols.values().find(|s| s.refdes == "J1");
    let id = j1.unwrap().component_id.clone();
    let def = p.library.get_mut(&id).unwrap();
    def.ratings.push(Rating {
        name: "voltage_max".into(),
        value: 12.0,
        unit: "V".into(),
        source: "test".into(),
        verified: false,
    });
}

#[test]
fn voltage_rating_passes_warns_fails_and_is_unknown_without_a_rating() {
    let mut p = reference(5.0, 0.01);
    rate_j1(&mut p);
    let r = erc::run(&p);
    assert!(on(&r, "ERC-022", "J1").is_empty(), "{:?}", r.findings);
    // J2 has no voltage rating: the rule did not check it, so it is UNKNOWN, never PASS.
    assert_eq!(r.state_of("ERC-022"), CheckState::Unknown);
    let why = reason(&r, "ERC-022");
    assert!(why.contains("J2: no voltage rating"), "{why}");
    // U1 spans three nets: 20 V across them only bounds its pins, so it is UNKNOWN, not FAIL.
    let mut q = p.clone();
    q.constraints.net_voltages_v.insert("+5V".into(), 20.0);
    let r = erc::run(&q);
    assert!(on(&r, "ERC-022", "U1").is_empty());
    let why = reason(&r, "ERC-022");
    assert!(why.contains("U1: the upper bound"), "{why}");
    // J1 at 10 V: within 12 V but above 80 % of it (warning); at 20 V above the rating (error).
    p.constraints.net_voltages_v.insert("+5V".into(), 10.0);
    let r = erc::run(&p);
    let w = on(&r, "ERC-022", "J1");
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].severity, Severity::Warning);
    p.constraints.net_voltages_v.insert("+5V".into(), 20.0);
    let r = erc::run(&p);
    let e = on(&r, "ERC-022", "J1");
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].severity, Severity::Error);
    assert_eq!(e[0].measured.as_ref().unwrap().value, 20.0);
    assert_eq!(e[0].required.as_ref().unwrap().value, 12.0);
    assert_eq!(r.state_of("ERC-022"), CheckState::Fail);
}

#[test]
fn resistor_power_from_declared_voltages() {
    // The 10k 0603 (0.1 W) between +3V3 and RESET.
    let mut p = reference(3.3, 0.01);
    let sym = p.schematic.symbols.values().find(|s| s.value == "10k");
    let r10k = sym.unwrap().refdes.clone();
    p.constraints.net_voltages_v.insert("RESET".into(), 0.0);
    let r = erc::run(&p);
    assert!(on(&r, "ERC-024", &r10k).is_empty(), "{:?}", r.findings);
    // 50 V across 10 kΩ is 0.25 W.
    p.constraints.net_voltages_v.insert("+3V3".into(), 50.0);
    let r = erc::run(&p);
    let e = on(&r, "ERC-024", &r10k);
    assert_eq!(e.len(), 1, "{:?}", r.findings);
    assert_eq!(e[0].severity, Severity::Error);
    assert!((e[0].measured.as_ref().unwrap().value - 0.25).abs() < 1e-9);
    assert_eq!(r.state_of("ERC-024"), CheckState::Fail);
    // RESET undeclared: no power, so UNKNOWN naming the net.
    p.constraints.net_voltages_v.remove("RESET");
    let r = erc::run(&p);
    assert!(on(&r, "ERC-024", &r10k).is_empty());
    let why = reason(&r, "ERC-024");
    let want = format!("{r10k}: no declared voltage on RESET");
    assert!(why.contains(&want), "{why}");
}

#[test]
fn part_current_passes_within_the_bound_and_is_unknown_above_it() {
    // D1 is rated 20 mA forward, derated to 15 mA: 10 mA on every net proves it.
    let why = reason(&erc::run(&reference(3.3, 0.01)), "ERC-023");
    assert!(!why.contains("D1"), "{why}");
    // 0.3 A on its nets is only an upper bound on the LED current: UNKNOWN, never FAIL.
    let r = erc::run(&reference(3.3, 0.3));
    assert!(on(&r, "ERC-023", "D1").is_empty());
    let why = reason(&r, "ERC-023");
    assert!(why.contains("D1: the upper bound"), "{why}");
    let mut p = reference(3.3, 0.01);
    p.constraints.net_currents_a.clear();
    let why = reason(&erc::run(&p), "ERC-023");
    assert!(why.contains("D1: no declared current"), "{why}");
}

#[test]
fn capacitor_rating_is_unknown_until_its_nets_are_declared() {
    let mut p = reference(3.3, 0.01);
    assert_ne!(erc::run(&p).state_of("ERC-011"), CheckState::Unknown);
    p.constraints.net_voltages_v.remove("GND");
    let r = erc::run(&p);
    assert_eq!(r.state_of("ERC-011"), CheckState::Unknown);
    let why = reason(&r, "ERC-011");
    assert!(why.contains("no declared voltage on GND"), "{why}");
}

#[test]
fn part_ratings_gate_fails_on_an_exceeded_rating_and_is_never_pass_when_unknown() {
    let mut p = reference(5.0, 0.01);
    assert_eq!(gate(&p), GateState::Unknown, "J2 has no rating");
    rate_j1(&mut p);
    p.constraints.net_voltages_v.insert("+5V".into(), 20.0);
    assert_eq!(gate(&p), GateState::Fail);
    assert_eq!(gate(&Project::new("empty")), GateState::Pass);
}
