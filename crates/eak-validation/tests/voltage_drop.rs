//! DRC-028 DC voltage drop along the routed copper: PASS, FAIL and UNKNOWN naming the missing
//! input, and the `electrical_constraints` gate it feeds.

use eak_core::geom::mm;
use eak_core::{
    CopperLayer, FootprintInstance, PinType, Point, Project, Rotation, Side, Stackup, Track,
};
use eak_validation::{drc, gates, CheckState, GateState, Report};

fn reason(r: &Report) -> String {
    let rr = r.rule_results.iter().find(|x| x.rule_id == "DRC-028");
    rr.unwrap().reason.clone()
}

/// J1 (pin 1 a power output when `source`) feeds J2 over 20 mm of 0.5 mm, 1 oz top copper:
/// 0.0210 Ω at 25 °C + 10 °C rise.
fn rail(source: bool) -> Project {
    let mut p = Project::new("vdrop");
    p.pcb.board.stackup = Stackup::two_layer(mm(1.6), 1.0, "FR-4", 4.5);
    let def = eak_core::builtin::get("CONN_1x02_P2.54").unwrap();
    let mut src = def.clone();
    src.id = "SRC".into();
    if source {
        src.symbol.pins[0].electrical_type = PinType::PowerOut;
    }
    p.library.insert(def.id.clone(), def.clone());
    p.library.insert(src.id.clone(), src);
    let mut ends = vec![];
    for (id, x, component) in [("J1", 0.0, "SRC"), ("J2", 20.0, def.id.as_str())] {
        let fp = FootprintInstance {
            id: id.into(),
            symbol_id: format!("s{id}"),
            component_id: component.into(),
            refdes: id.into(),
            position: Point::mm(x, 0.0),
            rotation: Rotation::R0,
            side: Side::Top,
            pad_nets: [("1".to_string(), "PWR".to_string())].into(),
            locked: false,
        };
        ends.push(p.pad_position(&fp, &def.footprint.pads[0]));
        p.pcb.footprints.insert(id.into(), fp);
    }
    let t = Track {
        id: "t".into(),
        layer: CopperLayer::Top,
        width: mm(0.5),
        a: ends[0],
        b: ends[1],
        net: "PWR".into(),
        arc_deg: None,
    };
    p.pcb.tracks.insert("t".into(), t);
    let c = &mut p.constraints;
    c.temp_rise_c = 10.0;
    c.ambient_temp_c = Some(25.0);
    c.net_currents_a.insert("PWR".into(), 1.0);
    p
}

fn gate(p: &Project) -> GateState {
    let r = gates::evaluate(p, None);
    let g = r.gates.iter().find(|g| g.id == "electrical_constraints");
    g.unwrap().state
}

#[test]
fn a_power_net_without_an_allowed_drop_is_unknown_never_pass() {
    // J1 drives PWR through a power-output pin, but no net_max_drop_v is declared.
    let r = drc::run(&rail(true));
    assert_eq!(r.state_of("DRC-028"), CheckState::Unknown);
    let why = reason(&r);
    assert!(
        why.contains("missing input: allowed drop (net_max_drop_v) for power net PWR"),
        "{why}"
    );
    assert_eq!(gate(&rail(true)), GateState::Unknown);
}

#[test]
fn drop_passes_and_fails_against_the_allowed_drop() {
    let mut p = rail(true);
    p.constraints.net_max_drop_v.insert("PWR".into(), 0.1);
    // 1 A × 0.021 Ω = 0.021 V.
    assert_eq!(drc::run(&p).state_of("DRC-028"), CheckState::Pass);
    p.constraints.net_currents_a.insert("PWR".into(), 10.0);
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-028"), CheckState::Fail);
    let f = r.findings.iter().find(|f| f.rule_id == "DRC-028").unwrap();
    let v = f.measured.as_ref().unwrap().value;
    assert!((v - 0.2099).abs() < 1e-3, "{v}");
    assert!(f.message.contains("J1.1 to J2.1"), "{}", f.message);
    assert_eq!(gate(&p), GateState::Fail);
}

#[test]
fn missing_inputs_are_unknown_and_named() {
    let unknown = |p: &Project, needle: &str| {
        let r = drc::run(p);
        assert_eq!(r.state_of("DRC-028"), CheckState::Unknown);
        let why = reason(&r);
        assert!(why.contains(needle), "{why}");
    };
    let mut p = rail(true);
    p.constraints.net_max_drop_v.insert("PWR".into(), 0.1);
    let mut q = p.clone();
    q.constraints.net_currents_a.clear();
    unknown(&q, "PWR: missing input: net current");
    assert_eq!(gate(&q), GateState::Unknown);
    let mut q = p.clone();
    q.constraints.ambient_temp_c = None;
    unknown(&q, "missing input: ambient temperature");
    let mut q = p.clone();
    q.pcb.board.stackup.layers[0].copper_oz = 0.0;
    unknown(&q, "missing input: copper weight of layer");
    let mut q = p.clone();
    q.pcb.tracks.clear();
    unknown(&q, "net unrouted: J2.1");
}

#[test]
fn without_a_source_pad_only_a_pass_is_proven() {
    let mut p = rail(false);
    p.constraints.net_max_drop_v.insert("PWR".into(), 0.1);
    // The worst pad pair holds for any source: a pass.
    assert_eq!(drc::run(&p).state_of("DRC-028"), CheckState::Pass);
    p.constraints.net_currents_a.insert("PWR".into(), 10.0);
    let r = drc::run(&p);
    assert_eq!(r.state_of("DRC-028"), CheckState::Unknown);
    let why = reason(&r);
    assert!(why.contains("missing input: source pad"), "{why}");
    assert!(!r.findings.iter().any(|f| f.rule_id == "DRC-028"));
}

#[test]
fn schematic_power_nets_without_an_allowed_drop_are_unknown() {
    let mut e = eak_runtime::Engine::in_memory(Project::new("nets"));
    eak_runtime::fixtures::build_schematic(&mut e).unwrap();
    let r = drc::run(e.project());
    assert_eq!(r.state_of("DRC-028"), CheckState::Unknown);
    let why = reason(&r);
    assert!(
        why.contains("+3V3") && why.contains("missing input: allowed drop"),
        "{why}"
    );
    assert!(!why.contains("GND"), "{why}");
}
