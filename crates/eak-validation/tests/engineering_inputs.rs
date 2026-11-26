//! Project rules on the schema v8 engineering inputs (ADR-0055): DRC-027 via current, ERC-025
//! junction temperature and the insulation basis of DRC-002, PASS, FAIL and UNKNOWN.

use eak_core::geom::mm;
use eak_core::{CopperLayer, Insulation, InsulationStandard, Point, Project, Rating, Via};
use eak_runtime::{fixtures, Engine};
use eak_validation::{drc, erc, CheckState, Report};

fn reason(r: &Report, rule: &str) -> String {
    let rr = r.rule_results.iter().find(|x| x.rule_id == rule).unwrap();
    rr.reason.clone()
}

fn with_via(amps: f64, plating_um: Option<f64>) -> Project {
    let mut p = Project::new("via");
    let via = Via {
        id: "v1".into(),
        position: Point::mm(10.0, 10.0),
        drill: mm(0.3),
        diameter: mm(0.6),
        net: "PWR".into(),
        from: CopperLayer::Top,
        to: CopperLayer::Bottom,
    };
    p.pcb.vias.insert("v1".into(), via);
    p.constraints.net_currents_a.insert("PWR".into(), amps);
    p.constraints.via_plating_um = plating_um;
    p
}

#[test]
fn via_current_passes_fails_and_needs_the_plating() {
    let r = drc::run(&with_via(0.1, None));
    assert_eq!(r.state_of("DRC-027"), CheckState::Unknown);
    let why = reason(&r, "DRC-027");
    assert!(why.contains("via plating thickness"), "{why}");
    let ok = drc::run(&with_via(0.1, Some(25.0)));
    assert_eq!(ok.state_of("DRC-027"), CheckState::Pass);
    let r = drc::run(&with_via(50.0, Some(25.0)));
    assert_eq!(r.state_of("DRC-027"), CheckState::Fail);
    let f = r.findings.iter().find(|f| f.rule_id == "DRC-027").unwrap();
    assert_eq!(f.measured.as_ref().unwrap().value, 50.0);
}

#[test]
fn junction_temperature_from_declared_dissipation() {
    let mut e = Engine::in_memory(Project::new("tj"));
    fixtures::build_schematic(&mut e).unwrap();
    let mut p = e.project().clone();
    let u1 = p.schematic.symbols.values().find(|s| s.refdes == "U1");
    let id = u1.unwrap().component_id.clone();
    // Nothing declared: nothing to check.
    assert_eq!(erc::run(&p).state_of("ERC-025"), CheckState::Pass);
    p.constraints.part_dissipation_w.insert("U1".into(), 1.0);
    let r = erc::run(&p);
    assert_eq!(r.state_of("ERC-025"), CheckState::Unknown);
    let why = reason(&r, "ERC-025");
    assert!(
        why.contains("U1: missing input: θJA, ambient temperature"),
        "{why}"
    );
    let rate = |name: &str, value: f64, unit: &str| Rating {
        name: name.into(),
        value,
        unit: unit.into(),
        source: "test".into(),
        verified: false,
    };
    let def = p.library.get_mut(&id).unwrap();
    def.ratings.push(rate("theta_ja", 90.0, "°C/W"));
    def.ratings.push(rate("tj_max", 125.0, "°C"));
    p.constraints.ambient_temp_c = Some(25.0);
    // 25 + 1 W × 90 = 115 °C: within 125 °C.
    assert_eq!(erc::run(&p).state_of("ERC-025"), CheckState::Pass);
    p.constraints.part_dissipation_w.insert("U1".into(), 2.0);
    let r = erc::run(&p);
    assert_eq!(r.state_of("ERC-025"), CheckState::Fail);
    let f = r.findings.iter().find(|f| f.rule_id == "ERC-025").unwrap();
    assert!((f.measured.as_ref().unwrap().value - 205.0).abs() < 1e-9);
    // A key that names no part is not silently dropped.
    p.constraints.part_dissipation_w = [("U99".to_string(), 0.5)].into();
    let why = reason(&erc::run(&p), "ERC-025");
    assert!(why.contains("U99: no such part"), "{why}");
}

#[test]
fn high_voltage_spacing_is_unknown_until_an_insulation_basis_is_declared() {
    let mut p = Project::new("hv");
    p.constraints.net_voltages_v.insert("HV".into(), 300.0);
    p.constraints.net_voltages_v.insert("GND".into(), 0.0);
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-002"), CheckState::Unknown);
    let why = reason(&r, "DRC-002");
    assert!(why.contains("no insulation environment declared"), "{why}");
    p.constraints.insulation = Some(Insulation {
        standard: InsulationStandard::Ipc2221b,
        pollution_degree: 2,
        material_group: "IIIb".into(),
        altitude_m: 0.0,
        coated: false,
    });
    assert_eq!(drc::run(&p).state_of("DRC-002"), CheckState::Pass);
}
