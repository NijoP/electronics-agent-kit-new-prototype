//! DRC-008 sizes each track with the copper weight of the stackup layer it is on.

use eak_core::{CopperLayer, Point, Project, Track};
use eak_engineering::ipc;
use eak_validation::{drc, Severity};

fn current_findings(p: &Project) -> usize {
    drc::run(p)
        .findings
        .iter()
        .filter(|f| f.rule_id == "DRC-008" && f.severity == Severity::Error)
        .count()
}

#[test]
fn track_width_uses_its_own_layer_copper_weight() {
    let mut p = Project::new("buck");
    // Top 1 oz, bottom 2 oz (the per-layer stackup a power board needs).
    p.pcb.board.stackup.layers[1].copper_oz = 2.0;
    p.pcb.board.stackup.layers[0].dielectric_after_um -= 35;
    assert!(p.pcb.board.stackup.validate().is_empty());

    let amps = 3.0;
    let dt = p.constraints.temp_rise_c;
    let need_1oz = ipc::trace_width_mm(amps, false, 1.0, dt).unwrap();
    let need_2oz = ipc::trace_width_mm(amps, false, 2.0, dt).unwrap();
    assert!(need_2oz < need_1oz);
    // Wide enough for 2 oz copper, too narrow for 1 oz.
    let width_mm = (need_1oz + need_2oz) / 2.0;
    p.constraints.net_currents_a.insert("VOUT".into(), amps);
    p.pcb.tracks.insert(
        "trk_1".into(),
        Track {
            id: "trk_1".into(),
            layer: CopperLayer::Bottom,
            width: (width_mm * 1e6) as i64,
            a: Point::mm(1.0, 1.0),
            b: Point::mm(10.0, 1.0),
            net: "VOUT".into(),
            arc_deg: None,
        },
    );
    assert_eq!(current_findings(&p), 0, "2 oz bottom copper carries 3 A");

    p.pcb.tracks.get_mut("trk_1").unwrap().layer = CopperLayer::Top;
    assert_eq!(current_findings(&p), 1, "1 oz top copper does not");
}
