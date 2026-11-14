//! Integration tests against a real ngspice. Skipped (with a printed reason) only when
//! ngspice is absent.
mod common;
use common::{volts, Circuit};
use eak_simulation::{probe, run, Analysis, ExtraSource, SimError, SimRequest, SourceKind};

fn ngspice_or_skip(test: &str) -> bool {
    let s = probe();
    if !s.available {
        println!("SKIPPED {test}: ngspice not available ({})", s.message.unwrap_or_default());
    }
    s.available
}

fn v(net: &str, spec: &str) -> ExtraSource {
    ExtraSource { name: "V1".into(), kind: SourceKind::Voltage, positive_net: net.into(), negative_net: String::new(), spec: spec.into() }
}

#[test]
fn rc_divider_operating_point_matches_hand_calculation() {
    if !ngspice_or_skip("rc_divider_operating_point") {
        return;
    }
    let mut c = Circuit::new();
    c.part("R_0603", "R1", "10k", 20.0, "VIN", "OUT");
    c.part("R_0603", "R2", "10k", 40.0, "OUT", "GND");
    let r = run(&c.p, &SimRequest { analysis: Analysis::Op, sources: vec![v("VIN", "5")], timeout_secs: None }).unwrap();
    assert!((volts(&r, "VIN") - 5.0).abs() < 1e-9);
    assert!((volts(&r, "OUT") - 2.5).abs() < 1e-6, "{:?}", r.nodes);
    let i = r.currents.iter().find(|c| c.element == "V1").expect("V1 branch current");
    assert!((i.amps + 250e-6).abs() < 1e-9, "source delivers 250 uA: {}", i.amps);
    assert!(r.tran.is_none() && !r.limitations.is_empty());
}

#[test]
fn rc_transient_charges_with_the_analytical_time_constant() {
    if !ngspice_or_skip("rc_transient") {
        return;
    }
    // R = 1k, C = 1u -> tau = 1 ms.
    let mut c = Circuit::new();
    c.part("R_0603", "R1", "1k", 20.0, "VIN", "OUT");
    c.part("C_0603", "C1", "1u", 40.0, "OUT", "GND");
    let req = SimRequest {
        analysis: Analysis::Tran { stop: 5e-3, step: 10e-6 },
        sources: vec![v("VIN", "PULSE(0 5 0 1n 1n 1 2)")],
        timeout_secs: None,
    };
    let r = run(&c.p, &req).unwrap();
    let t = r.tran.expect("transient data");
    let out = t.traces.iter().find(|x| x.net == "OUT").expect("OUT trace");
    let at = |time: f64| {
        let i = t.time.iter().position(|&x| x >= time).unwrap();
        out.volts[i]
    };
    let expect = |time: f64| 5.0 * (1.0 - (-time / 1e-3f64).exp());
    for time in [1e-3, 2e-3, 4e-3] {
        assert!((at(time) - expect(time)).abs() < 0.05, "t={time}: {} vs {}", at(time), expect(time));
    }
    assert!((volts(&r, "OUT") - expect(5e-3)).abs() < 0.05, "final value");
}

#[test]
fn led_with_series_resistor_conducts_with_a_plausible_forward_drop() {
    if !ngspice_or_skip("led_series_resistor") {
        return;
    }
    let mut c = Circuit::new();
    c.part("R_0603", "R1", "330", 20.0, "VIN", "A");
    c.part("LED_0603", "D1", "Green", 40.0, "GND", "A");
    let r = run(&c.p, &SimRequest { analysis: Analysis::Op, sources: vec![v("VIN", "3.3")], timeout_secs: None }).unwrap();
    let vf = volts(&r, "A");
    assert!(vf > 1.0 && vf < 3.0, "forward drop {vf}");
}

#[test]
fn invalid_requests_fail_before_ngspice_is_needed() {
    let c = Circuit::new();
    let bad = SimRequest { analysis: Analysis::Tran { stop: 1e-3, step: 0.0 }, sources: vec![], timeout_secs: None };
    assert!(matches!(run(&c.p, &bad), Err(SimError::InvalidRequest(_))));
    let huge = SimRequest { analysis: Analysis::Tran { stop: 1.0, step: 1e-9 }, sources: vec![], timeout_secs: None };
    assert!(matches!(run(&c.p, &huge), Err(SimError::InvalidRequest(_))));
}

