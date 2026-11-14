mod common;
use common::Circuit;
use eak_core::{new_id, ComponentDef, Point, Rotation, SymbolInstance};
use eak_simulation::{export_netlist, ElementKind, ExtraSource, SimError, SourceKind};

fn vsrc(name: &str, net: &str, spec: &str) -> ExtraSource {
    ExtraSource { name: name.into(), kind: SourceKind::Voltage, positive_net: net.into(), negative_net: String::new(), spec: spec.into() }
}

fn divider() -> Circuit {
    let mut c = Circuit::new();
    c.part("R_0603", "R1", "10k", 20.0, "VIN", "OUT");
    c.part("R_0603", "R2", "4k7", 40.0, "OUT", "GND");
    c
}

#[test]
fn divider_exports_passives_with_ground_as_node_zero() {
    let e = export_netlist(&divider().p, &[vsrc("V1", "VIN", "5")]).unwrap();
    assert!(e.text.contains("R1 n_vin n_out 1e4\n"), "{}", e.text);
    assert!(e.text.contains("R2 n_out 0 4.7e3\n"), "{}", e.text);
    assert!(e.text.contains("V1 n_vin 0 DC 5e0\n"), "{}", e.text);
    assert_eq!(e.nodes.get("n_out").map(String::as_str), Some("OUT"));
    assert!(!e.nodes.values().any(|n| n == "GND"));
    assert!(e.skipped.is_empty());
}

#[test]
fn led_is_oriented_anode_to_cathode_and_gets_a_generic_model() {
    let mut c = Circuit::new();
    // LED_0603: pin 1 = K, pin 2 = A.
    c.part("LED_0603", "D1", "Green", 20.0, "K", "A");
    let e = export_netlist(&c.p, &[vsrc("V1", "A", "3.3")]).unwrap();
    assert!(e.text.contains("D1 n_a n_k EAK_LED"), "{}", e.text);
    assert!(e.text.contains(".model EAK_LED D("));
    assert_eq!(e.elements.iter().filter(|x| x.kind == ElementKind::Led).count(), 1);
}

#[test]
fn unsimulatable_parts_and_bad_values_are_reported_not_dropped() {
    let mut c = divider();
    c.part("R_0603", "R3", "banana", 60.0, "OUT", "GND");
    c.part("CONN_1x02_P2.54", "J1", "CONN", 80.0, "VIN", "GND");
    let e = export_netlist(&c.p, &[vsrc("V1", "VIN", "5")]).unwrap();
    let names: Vec<_> = e.skipped.iter().map(|s| s.refdes.as_str()).collect();
    assert_eq!(names, ["J1", "R3"]);
    assert!(e.warnings.iter().any(|w| w.starts_with("R3 not simulated")));
}

#[test]
fn library_parts_with_source_prefix_become_sources() {
    let mut c = Circuit::new();
    let mut def: ComponentDef = c.p.library["R_0603"].clone();
    def.id = "V_SRC".into();
    def.refdes_prefix = "V".into();
    c.p.library.insert(def.id.clone(), def);
    c.part("V_SRC", "V1", "PULSE(0 5 0 1n 1n 1 2)", 20.0, "VIN", "GND");
    c.part("R_0603", "R1", "1k", 40.0, "VIN", "GND");
    let e = export_netlist(&c.p, &[]).unwrap();
    assert!(e.text.contains("V1 n_vin 0 PULSE(0 5 0 1n 1n 1 2)"), "{}", e.text);
}

#[test]
fn net_names_are_sanitised_and_unique() {
    let mut c = Circuit::new();
    c.part("R_0603", "R1", "1k", 20.0, "A/B", "GND");
    c.part("R_0603", "R2", "1k", 40.0, "a-b", "GND");
    let e = export_netlist(&c.p, &[vsrc("V1", "A/B", "1")]).unwrap();
    assert_eq!(e.nodes.len(), 2, "{:?}", e.nodes);
    assert!(e.nodes.contains_key("n_a_b") && e.nodes.contains_key("n_a_b_2"));
}

#[test]
fn unconnected_pin_and_empty_circuit_are_clear_errors() {
    let mut c = Circuit::new();
    assert!(matches!(export_netlist(&c.p, &[]), Err(SimError::EmptyCircuit(_))));
    let id = new_id("sym");
    c.p.schematic.symbols.insert(
        id.clone(),
        SymbolInstance { id, sheet_id: c.p.first_sheet_id().unwrap().clone(), component_id: "R_0603".into(), refdes: "R1".into(), value: "1k".into(), position: Point::mm(20.0, 20.0), rotation: Rotation::R0 },
    );
    match export_netlist(&c.p, &[]) {
        Err(SimError::EmptyCircuit(_)) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn source_specs_are_whitelisted_so_no_netlist_injection_is_possible() {
    let c = divider();
    for bad in ["5\n.control\nshell rm -rf /\n.endc", "DC 5; shell", "$(x)", "banana", ""] {
        assert!(export_netlist(&c.p, &[vsrc("V1", "VIN", bad)]).is_err(), "{bad:?}");
    }
    assert!(export_netlist(&c.p, &[vsrc("V1", "NOPE", "5")]).is_err(), "unknown net");
    assert!(export_netlist(&c.p, &[vsrc("V1", "VIN", "5"), vsrc("V1", "OUT", "1")]).is_err(), "duplicate name");
    assert!(export_netlist(&c.p, &[vsrc("V1", "VIN", "SIN(0 1 1k)")]).is_ok());
}
