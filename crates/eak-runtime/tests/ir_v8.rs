//! Schema v8 (ADR-0055) through the engine: `SetDiffPair`, `DeleteDiffPair`, `SetViaSpans` and
//! `SetEngineeringInputs`. Each is one undo step, an invalid value is refused without changing
//! the project, and everything survives a save and load. Written without running (no local
//! builds): CI decides.

use eak_core::{
    from_json, to_json, CopperLayer, DiffPairRule, Enclosure, Insulation, InsulationStandard,
    MountingHoleSpec, Point, Project, ViaSpanRule,
};
use eak_runtime::command::Command as C;
use eak_runtime::engine::*;

fn run(e: &mut Engine, cmd: C) -> CommandResponse {
    let rev = e.project().revision;
    e.execute(CommandRequest {
        expected_revision: rev,
        actor: Actor::human(),
        command: cmd,
        approval: None,
    })
}

fn ok(e: &mut Engine, cmd: C) {
    let name = cmd.name();
    let r = run(e, cmd);
    assert!(r.accepted, "{name}: {:?}", r.error);
}

fn refused(e: &mut Engine, cmd: C) {
    let name = cmd.name();
    let before = e.project().clone();
    let r = run(e, cmd);
    assert!(!r.accepted, "{name} was accepted");
    assert_eq!(
        e.project(),
        &before,
        "{name}: a refusal changed the project"
    );
}

fn pair(p: &str, n: &str) -> DiffPairRule {
    DiffPairRule {
        positive: p.into(),
        negative: n.into(),
        width: Some(200_000),
        gap: Some(150_000),
        target_zdiff_ohm: Some(90.0),
        zdiff_tolerance_pct: Some(10.0),
        max_skew: Some(100_000),
    }
}

fn insulation() -> Insulation {
    Insulation {
        standard: InsulationStandard::Iec60664_1,
        pollution_degree: 2,
        material_group: "IIIa".into(),
        altitude_m: 2000.0,
        coated: false,
    }
}

/// `SetEngineeringInputs` with only the insulation basis declared.
fn only_insulation(ins: Option<Insulation>) -> C {
    C::SetEngineeringInputs {
        ambient_temp_c: None,
        via_plating_um: None,
        net_max_drop_v: Default::default(),
        part_dissipation_w: Default::default(),
        insulation: ins,
        enclosure: None,
    }
}

#[test]
fn diff_pairs_replace_by_net_delete_and_undo() {
    let mut e = Engine::in_memory(Project::new("v8"));
    ok(
        &mut e,
        C::SetDiffPair {
            rule: pair("USB_DP", "USB_DN"),
        },
    );
    // A rule sharing a net replaces the old pair instead of joining a net to two pairs.
    ok(
        &mut e,
        C::SetDiffPair {
            rule: pair("USB_DP", "USB_DM"),
        },
    );
    let pairs = &e.project().constraints.diff_pairs;
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].negative, "USB_DM");
    let rev = e.project().revision;
    assert!(e.undo(rev, &Actor::human()).accepted);
    assert_eq!(e.project().constraints.diff_pairs[0].negative, "USB_DN");

    refused(
        &mut e,
        C::SetDiffPair {
            rule: pair("A", "A"),
        },
    );
    let mut bad = pair("CLK_P", "CLK_N");
    bad.gap = Some(0);
    refused(&mut e, C::SetDiffPair { rule: bad });
    let mut bad = pair("CLK_P", "CLK_N");
    bad.target_zdiff_ohm = Some(f64::NAN);
    refused(&mut e, C::SetDiffPair { rule: bad });

    ok(
        &mut e,
        C::DeleteDiffPair {
            net: "USB_DN".into(),
        },
    );
    assert!(e.project().constraints.diff_pairs.is_empty());
    refused(
        &mut e,
        C::DeleteDiffPair {
            net: "USB_DN".into(),
        },
    );
}

#[test]
fn via_spans_and_engineering_inputs_are_validated_and_survive_save() {
    let mut e = Engine::in_memory(Project::new("v8"));
    let span = ViaSpanRule {
        from: CopperLayer::Top,
        to: CopperLayer::Inner(1),
        drill: 150_000,
        diameter: 350_000,
    };
    ok(
        &mut e,
        C::SetViaSpans {
            spans: vec![span.clone()],
        },
    );
    refused(
        &mut e,
        C::SetViaSpans {
            spans: vec![span.clone(), span.clone()],
        },
    );
    refused(
        &mut e,
        C::SetViaSpans {
            spans: vec![ViaSpanRule {
                diameter: 100_000,
                ..span.clone()
            }],
        },
    );
    let mut bad = insulation();
    bad.pollution_degree = 5;
    refused(&mut e, only_insulation(Some(bad)));
    let mut bad = insulation();
    bad.material_group = "IV".into();
    refused(&mut e, only_insulation(Some(bad)));

    ok(
        &mut e,
        C::SetEngineeringInputs {
            ambient_temp_c: Some(40.0),
            via_plating_um: Some(25.0),
            net_max_drop_v: [("VIN".to_string(), 0.1)].into_iter().collect(),
            part_dissipation_w: [("U1".to_string(), 0.5)].into_iter().collect(),
            insulation: Some(insulation()),
            enclosure: Some(Enclosure {
                max_height_bottom: Some(2_000_000),
                mounting_holes: vec![MountingHoleSpec {
                    position: Point::mm(3.0, 3.0),
                    drill: 3_200_000,
                    tolerance: 100_000,
                    keepout_radius: 3_000_000,
                }],
            }),
        },
    );
    ok(
        &mut e,
        C::SetDiffPair {
            rule: pair("USB_DP", "USB_DN"),
        },
    );

    let saved = to_json(e.project());
    let back = from_json(&saved).expect("a v8 project loads");
    assert_eq!(&back, e.project());
    assert_eq!(saved, to_json(&back));
    let c = &back.constraints;
    assert_eq!(c.via_spans, vec![span]);
    assert_eq!(c.ambient_temp_c, Some(40.0));
    assert_eq!(c.net_max_drop_v["VIN"], 0.1);
    assert_eq!(
        c.enclosure.as_ref().unwrap().mounting_holes[0].drill,
        3_200_000
    );

    // An omitted field is cleared: the command carries the whole set.
    ok(&mut e, only_insulation(None));
    let json = to_json(e.project());
    for k in [
        "ambient_temp_c",
        "insulation",
        "enclosure",
        "net_max_drop_v",
    ] {
        assert!(!json.contains(k), "an undeclared {k} must not be written");
    }
}
