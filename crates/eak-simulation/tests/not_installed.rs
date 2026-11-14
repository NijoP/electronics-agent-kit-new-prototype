//! Own test binary: it overrides EAK_NGSPICE, which is process-global.
mod common;
use common::Circuit;
use eak_simulation::{probe, run, Analysis, ExtraSource, SimError, SimRequest, SourceKind};

#[test]
fn missing_ngspice_is_a_clear_not_installed_error() {
    std::env::set_var("EAK_NGSPICE", "/nonexistent/eak-ngspice");
    let mut c = Circuit::new();
    c.part("R_0603", "R1", "1k", 20.0, "VIN", "GND");
    let src = ExtraSource {
        name: "V1".into(),
        kind: SourceKind::Voltage,
        positive_net: "VIN".into(),
        negative_net: String::new(),
        spec: "1".into(),
    };
    let r = run(
        &c.p,
        &SimRequest {
            analysis: Analysis::Op,
            sources: vec![src],
            timeout_secs: None,
        },
    );
    let s = probe();
    assert!(matches!(r, Err(SimError::NotInstalled)), "{r:?}");
    assert!(!s.available && s.message.unwrap().contains("not installed"));
}
