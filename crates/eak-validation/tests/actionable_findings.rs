//! Actionable findings: stable ids, measured vs required values for quantitative rules,
//! suggested fixes, and backward-compatible JSON.

use eak_core::{geom::mm, CopperLayer, Point, Project, Track, Via, Wire, Zone};
use eak_runtime::{fixtures, Engine};
use eak_validation::report::finding_id;
use eak_validation::{drc, erc, mech, Bound, Finding, Report, Severity};
use std::collections::BTreeSet;

fn reference() -> Project {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).unwrap();
    fixtures::build_pcb(&mut e).unwrap();
    e.project().clone()
}

fn track(p: &mut Project, id: &str, net: &str, width: f64, a: (f64, f64), b: (f64, f64)) {
    p.pcb.tracks.insert(
        id.into(),
        Track {
            id: id.into(),
            layer: CopperLayer::Top,
            width: mm(width),
            a: Point::mm(a.0, a.1),
            b: Point::mm(b.0, b.1),
            net: net.into(),
        },
    );
}

/// Findings of `rule` whose objects are exactly `objects`.
fn find<'a>(r: &'a Report, rule: &str, objects: &[&str]) -> Vec<&'a Finding> {
    let want: BTreeSet<&str> = objects.iter().copied().collect();
    r.findings
        .iter()
        .filter(|f| {
            f.rule_id == rule
                && f.objects
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>()
                    == want
        })
        .collect()
}

/// Every finding has a unique id prefixed by its rule and a suggested fix; measured and
/// required are set together with the same unit.
fn assert_actionable(r: &Report) {
    let mut ids = BTreeSet::new();
    for f in &r.findings {
        assert!(f.id.starts_with(&format!("{}-", f.rule_id)), "{f:?}");
        assert!(ids.insert(f.id.clone()), "duplicate id {}", f.id);
        assert!(
            f.suggested_fix
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty()),
            "no fix: {f:?}"
        );
        match (&f.measured, &f.required) {
            (Some(m), Some(q)) => assert_eq!(m.unit, q.unit, "{f:?}"),
            (None, None) => {}
            _ => panic!("measured/required must be set together: {f:?}"),
        }
    }
}

fn assert_mm(f: &Finding, measured: f64, bound: Bound, required: f64) {
    let m = f.measured.as_ref().expect("measured");
    let q = f.required.as_ref().expect("required");
    assert_eq!((m.unit.as_str(), q.unit.as_str()), ("mm", "mm"), "{f:?}");
    assert!(
        (m.value - measured).abs() < 1e-6,
        "measured {} vs {measured}",
        m.value
    );
    assert_eq!(q.bound, bound);
    assert!(
        (q.value - required).abs() < 1e-6,
        "required {} vs {required}",
        q.value
    );
}

#[test]
fn finding_id_is_deterministic_and_order_independent() {
    let ab = finding_id("DRC-002", &["a".into(), "b".into()], "k");
    // Pinned: SHA-256("DRC-002" 0x1f "a" 0x1f "b" 0x1e "k"), first 12 hex digits.
    assert_eq!(ab, "DRC-002-6b4c35b291b2");
    assert_eq!(
        finding_id("DRC-002", &["b".into(), "a".into(), "a".into()], "k"),
        ab
    );
    assert_ne!(finding_id("DRC-002", &["a".into(), "b".into()], "j"), ab);
    assert_ne!(finding_id("DRC-003", &["a".into(), "b".into()], "k"), ab);
    assert_eq!(finding_id("DRC-009", &[], ""), "DRC-009-7f529af239f0");
}

#[test]
fn colliding_ids_get_a_deterministic_suffix() {
    // Two THT pads sharing a number on one footprint would give the same id.
    let f = |msg: &str| Finding {
        id: "DRC-006-aaaaaaaaaaaa".into(),
        rule_id: "DRC-006".into(),
        severity: Severity::Error,
        message: msg.into(),
        objects: vec!["fp".into()],
        location: None,
        measured: None,
        required: None,
        suggested_fix: None,
    };
    let r = Report::new(
        "DRC",
        &Project::new("x"),
        vec![],
        vec![f("b"), f("a")],
        vec![],
        vec![],
    );
    let ids: Vec<_> = r.findings.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["DRC-006-aaaaaaaaaaaa", "DRC-006-aaaaaaaaaaaa-2"]);
    assert_eq!(r.findings[0].message, "a");
}

#[test]
fn reference_reports_are_actionable_and_ids_repeat_across_runs() {
    let p = reference();
    let ids = |r: &Report| r.findings.iter().map(|f| f.id.clone()).collect::<Vec<_>>();
    for (a, b) in [
        (erc::run(&p), erc::run(&p)),
        (drc::run(&p), drc::run(&p)),
        (mech::run(&p), mech::run(&p)),
    ] {
        assert_actionable(&a);
        assert_eq!(ids(&a), ids(&b));
    }
}

#[test]
fn id_survives_a_changed_measurement() {
    let mut p = reference();
    track(&mut p, "t_thin", "N_THIN", 0.10, (5.0, 5.0), (5.0, 6.0));
    let first = find(&drc::run(&p), "DRC-001", &["t_thin"])[0].id.clone();
    // Same violation, different measured width, later revision: same id.
    p.pcb.tracks.get_mut("t_thin").unwrap().width = mm(0.12);
    p.revision += 7;
    let r = drc::run(&p);
    let again = find(&r, "DRC-001", &["t_thin"]);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].id, first);
    assert_mm(again[0], 0.12, Bound::Min, 0.15);
}

#[test]
fn track_width_and_current_capacity_report_measured_and_required() {
    let mut p = reference();
    track(&mut p, "t_thin", "N_THIN", 0.10, (5.0, 5.0), (5.0, 6.0));
    track(&mut p, "t_class", "N_CLASS", 0.20, (7.0, 5.0), (7.0, 6.0));
    let r = drc::run(&p);
    assert_actionable(&r);
    let thin = find(&r, "DRC-001", &["t_thin"]);
    assert_eq!(thin[0].severity, Severity::Error);
    assert_mm(thin[0], 0.10, Bound::Min, 0.15);
    let class = find(&r, "DRC-001", &["t_class"]);
    assert_eq!(class[0].severity, Severity::Warning);
    assert_mm(class[0], 0.20, Bound::Min, 0.25);

    // DRC-008: a large declared current makes the routed +5V tracks too narrow.
    let mut p = reference();
    p.constraints.net_currents_a.insert("+5V".into(), 5.0);
    let r = drc::run(&p);
    assert_actionable(&r);
    let hot: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule_id == "DRC-008")
        .collect();
    assert!(!hot.is_empty());
    for f in hot {
        let (m, q) = (f.measured.as_ref().unwrap(), f.required.as_ref().unwrap());
        let t = &p.pcb.tracks[&f.objects[0]];
        assert!((m.value - t.width as f64 / 1e6).abs() < 1e-6);
        assert_eq!(q.bound, Bound::Min);
        assert!(q.value > m.value, "{f:?}");
    }
}

#[test]
fn via_drill_and_ring_are_separate_findings_with_values() {
    let mut p = reference();
    p.pcb.vias.insert(
        "v_bad".into(),
        Via {
            id: "v_bad".into(),
            position: Point::mm(20.0, 15.0),
            drill: mm(0.2),
            diameter: mm(0.4),
            net: "N_VIA".into(),
            from: eak_core::CopperLayer::Top,
            to: eak_core::CopperLayer::Bottom,
        },
    );
    let r = drc::run(&p);
    assert_actionable(&r);
    let v = find(&r, "DRC-006", &["v_bad"]);
    assert_eq!(v.len(), 2, "{v:?}");
    assert_ne!(v[0].id, v[1].id);
    let drill = v.iter().find(|f| f.message.contains("drill")).unwrap();
    assert_mm(drill, 0.2, Bound::Min, 0.3);
    let ring = v.iter().find(|f| f.message.contains("annular")).unwrap();
    assert_mm(ring, 0.1, Bound::Min, 0.13);
}

#[test]
fn clearance_and_edge_findings_report_gap_and_limit() {
    let mut p = reference();
    // Two 0.2 mm tracks on different nets, 0.3 mm apart centre to centre: 0.1 mm gap.
    track(&mut p, "t_a", "N_A", 0.2, (12.0, 2.0), (12.0, 3.0));
    track(&mut p, "t_b", "N_B", 0.2, (12.3, 2.0), (12.3, 3.0));
    // 0.2 mm track whose centreline is 0.3 mm from the left edge: 0.2 mm to the edge.
    track(&mut p, "t_e", "N_E", 0.2, (0.3, 12.0), (0.3, 14.0));
    let r = drc::run(&p);
    assert_actionable(&r);
    let gap = find(&r, "DRC-002", &["t_a", "t_b"]);
    assert_eq!(gap.len(), 1, "{:?}", r.findings);
    assert_mm(gap[0], 0.1, Bound::Min, 0.2);
    let at = gap[0].location.expect("gap location");
    assert!(at.x > mm(12.0) && at.x < mm(12.3), "{at:?}");
    let edge = find(&r, "DRC-005", &["t_e"]);
    assert_eq!(edge.len(), 1, "{:?}", r.findings);
    assert_mm(edge[0], 0.2, Bound::Min, 0.3);
}

#[test]
fn capacitor_rating_reports_volts_against_rail_and_margin() {
    let mut p = reference();
    p.constraints.net_voltages_v.insert("+5V".into(), 12.0);
    let r = erc::run(&p);
    assert_actionable(&r);
    let w: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule_id == "ERC-011")
        .collect();
    assert!(!w.is_empty());
    for f in &w {
        assert_eq!(f.severity, Severity::Warning);
        assert_eq!(f.measured.as_ref().unwrap().value, 16.0);
        let q = f.required.as_ref().unwrap();
        assert_eq!((q.bound, q.value, q.unit.as_str()), (Bound::Min, 18.0, "V"));
    }
    p.constraints.net_voltages_v.insert("+5V".into(), 20.0);
    let r = erc::run(&p);
    let e: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule_id == "ERC-011")
        .collect();
    assert!(!e.is_empty());
    assert!(e
        .iter()
        .all(|f| f.severity == Severity::Error && f.required.as_ref().unwrap().value == 20.0));
}

#[test]
fn component_height_reports_maximum() {
    let mut p = reference();
    p.constraints.max_component_height = Some(mm(0.1));
    let r = mech::run(&p);
    assert_actionable(&r);
    let tall: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule_id == "MECH-001")
        .collect();
    assert!(!tall.is_empty());
    for f in tall {
        let q = f.required.as_ref().unwrap();
        assert_eq!((q.bound, q.value), (Bound::Max, 0.1));
        assert!(f.measured.as_ref().unwrap().value > 0.1);
    }
}

#[test]
fn unconnected_pins_get_distinct_ids_and_pin_locations() {
    let mut p = reference();
    p.schematic
        .labels
        .retain(|_, l| l.name != "IO1" && l.name != "IO2");
    let r = erc::run(&p);
    assert_actionable(&r);
    let u2 = p
        .schematic
        .symbols
        .values()
        .find(|s| s.refdes == "U2")
        .unwrap();
    let open = find(&r, "ERC-001", &[u2.id.as_str()]);
    assert_eq!(open.len(), 2, "{open:?}");
    assert_ne!(open[0].id, open[1].id);
    for f in open {
        assert!(f.measured.is_none() && f.required.is_none());
        let pin = if f.message.contains("pin 6") {
            "6"
        } else {
            "7"
        };
        assert!(f.location.is_some());
        assert_eq!(f.location, p.pin_position(u2, pin));
    }
}

#[test]
fn old_finding_json_still_deserialises() {
    let old = r#"{"rule_id":"ERC-001","severity":"error","message":"D1 pin 1 (K) is unconnected","objects":["fp_d1"],"location":null}"#;
    let f: Finding = serde_json::from_str(old).unwrap();
    assert_eq!(f.rule_id, "ERC-001");
    assert_eq!(f.id, "");
    assert!(f.measured.is_none() && f.required.is_none() && f.suggested_fix.is_none());
    // Older still: no location key at all.
    let older = r#"{"rule_id":"DRC-009","severity":"warning","message":"m","objects":[]}"#;
    let f: Finding = serde_json::from_str(older).unwrap();
    assert_eq!(f.location, None);
}

#[test]
fn new_report_json_round_trips() {
    let mut p = reference();
    track(&mut p, "t_thin", "N_THIN", 0.10, (5.0, 5.0), (5.0, 6.0));
    let r = drc::run(&p);
    let json = serde_json::to_string(&r).unwrap();
    let back: Report = serde_json::from_str(&json).unwrap();
    assert_eq!(back, r);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let thin = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["objects"][0] == "t_thin" && f["rule_id"] == "DRC-001")
        .unwrap();
    assert_eq!(thin["measured"]["unit"], "mm");
    assert_eq!(thin["required"]["bound"], "min");
    assert!(thin["suggested_fix"].is_string());
    assert!(thin["id"].as_str().unwrap().starts_with("DRC-001-"));
}

#[test]
fn capacitor_margin_is_checked_before_rounding() {
    let mut p = reference();
    // 1.5 x 10.6669333 V = 16.0004 V, which rounds to 16.000 V; a 16 V part is still short.
    p.constraints
        .net_voltages_v
        .insert("+5V".into(), 16.0004 / 1.5);
    let r = erc::run(&p);
    let w: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule_id == "ERC-011")
        .collect();
    assert!(!w.is_empty(), "{:?}", r.findings);
    for f in w {
        assert_eq!(f.severity, Severity::Warning);
        assert_eq!(f.required.as_ref().unwrap().value, 16.0);
    }
}

#[test]
fn dangling_wire_end_id_survives_moving_the_end() {
    let mut p = reference();
    let sheet = p.schematic.wires.values().next().unwrap().sheet_id.clone();
    let put = |p: &mut Project, a: (f64, f64), b: (f64, f64)| {
        p.schematic.wires.insert(
            "w_float".into(),
            Wire {
                id: "w_float".into(),
                sheet_id: sheet.clone(),
                a: Point::mm(a.0, a.1),
                b: Point::mm(b.0, b.1),
            },
        );
    };
    let ids = |p: &Project| {
        let r = erc::run(p);
        let mut v: Vec<_> = find(&r, "ERC-008", &["w_float"])
            .iter()
            .map(|f| (f.location, f.id.clone()))
            .collect();
        v.sort_by_key(|(l, _)| l.map(|l| (l.x, l.y)));
        v
    };
    put(&mut p, (200.0, 200.0), (210.0, 200.0));
    let before = ids(&p);
    assert_eq!(before.len(), 2, "{before:?}");
    assert_ne!(before[0].1, before[1].1);
    put(&mut p, (200.0, 250.0), (230.0, 250.0));
    let after = ids(&p);
    assert_eq!(after.len(), 2, "{after:?}");
    assert_eq!(after[0].1, before[0].1);
    assert_eq!(after[1].1, before[1].1);
    assert_ne!(after[0].0, before[0].0);
}

#[test]
fn zone_and_failed_current_findings_carry_locations() {
    let mut p = reference();
    let outline = vec![
        Point::mm(30.0, 10.0),
        Point::mm(45.0, 10.0),
        Point::mm(45.0, 20.0),
        Point::mm(30.0, 20.0),
    ];
    p.pcb.zones.insert(
        "z_out".into(),
        Zone {
            id: "z_out".into(),
            layer: CopperLayer::Top,
            net: "N_ZONE".into(),
            outline: outline.clone(),
            clearance: mm(0.2),
        },
    );
    let r = drc::run(&p);
    let edge = find(&r, "DRC-005", &["z_out"]);
    assert_eq!(edge.len(), 1, "{:?}", r.findings);
    assert_eq!(edge[0].location, Some(outline[1]));
    let island = find(&r, "DRC-013", &["z_out"]);
    assert_eq!(island.len(), 1, "{:?}", r.findings);
    assert_eq!(island[0].location, Some(outline[0]));

    let mut p = reference();
    p.constraints.net_currents_a.insert("+5V".into(), 1.0);
    p.constraints.temp_rise_c = 0.0;
    let r = drc::run(&p);
    let failed: Vec<_> = r
        .findings
        .iter()
        .filter(|f| f.rule_id == "DRC-008")
        .collect();
    assert!(!failed.is_empty(), "{:?}", r.findings);
    for f in failed {
        assert_eq!(f.location, Some(p.pcb.tracks[&f.objects[0]].a), "{f:?}");
    }
}

/// Regression (dev-board run B, E3): a rail that enters through a connector pin, with no
/// power port, fails ERC-003; the fix names the exact typed LabelPin power-port command on
/// that connector pin, and applying it clears the finding. (unrun: builds are frozen)
#[test]
fn undriven_rail_fix_names_the_connector_power_port() {
    let mut p = reference();
    let j1 = p
        .schematic
        .symbols
        .values()
        .find(|s| s.refdes == "J1")
        .unwrap()
        .id
        .clone();
    for l in p.schematic.labels.values_mut() {
        if l.name == "+5V" {
            l.kind = eak_core::LabelKind::Net;
        }
    }
    let r = erc::run(&p);
    let f = r
        .findings
        .iter()
        .find(|f| f.rule_id == "ERC-003" && f.message.contains("+5V"))
        .expect("ERC-003 on +5V");
    let fix = f.suggested_fix.as_deref().unwrap();
    assert!(fix.contains("\"type\":\"LabelPin\""), "{fix}");
    assert!(fix.contains(&format!("\"symbol_id\":\"{j1}\"")), "{fix}");
    assert!(fix.contains("\"kind\":\"power\""), "{fix}");

    let mut e = Engine::in_memory(p);
    let rev = e.project().revision;
    let resp = e.execute(eak_runtime::CommandRequest {
        expected_revision: rev,
        command: eak_runtime::Command::LabelPin {
            symbol_id: j1,
            pin: "1".into(),
            name: "+5V".into(),
            kind: eak_core::LabelKind::Power,
            scope: Default::default(),
            power_style: None,
        },
        actor: eak_runtime::Actor {
            kind: eak_runtime::ActorKind::Cli,
            name: "test".into(),
            session: None,
        },
        approval: None,
    });
    assert!(resp.error.is_none(), "{:?}", resp.error);
    let r = erc::run(e.project());
    assert!(
        !r.findings
            .iter()
            .any(|f| f.rule_id == "ERC-003" && f.message.contains("+5V")),
        "{:?}",
        r.findings
    );
}
