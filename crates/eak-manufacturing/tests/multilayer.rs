//! Proof for multilayer boards: a real 4-layer board is built only through typed commands,
//! passes DRC, and exports a Gerber file per copper layer, drill files per via span and a
//! stackup file. The board is built by `eak_runtime::fixtures::build_pcb_layers` (the fixture
//! is code; no build outputs are committed).

use eak_core::{CopperLayer, Project, ViaKind};
use eak_manufacturing::{compile, stackup, verify_package, PackageStatus};
use eak_runtime::{fixtures, Engine};
use eak_validation::{dfm, drc, erc, mech};

fn board(layers: u8) -> Engine {
    let mut e = Engine::in_memory(Project::new("ml"));
    fixtures::build_schematic(&mut e).expect("schematic");
    fixtures::build_pcb_layers(&mut e, layers).expect("pcb");
    // Declare a height limit so MECH-001 is evaluated (unknown mechanical fit blocks release).
    let rev = e.project().revision;
    let r = e.execute(eak_runtime::CommandRequest {
        expected_revision: rev,
        actor: eak_runtime::Actor::human(),
        command: eak_runtime::Command::SetDesignRules {
            min_track_width: None,
            min_clearance: None,
            min_drill: None,
            min_annular_ring: None,
            board_edge_clearance: None,
            default_track_width: None,
            default_clearance: None,
            temp_rise_c: None,
            max_component_height: Some(25_000_000),
            clear_max_component_height: false,
        },
        approval: None,
    });
    assert!(r.accepted, "{r:?}");
    e
}

#[test]
fn four_layer_board_passes_drc_and_exports_every_layer() {
    let mut e = board(4);
    // ADR-0048: every net rated, so no voltage or current rule is UNKNOWN (that blocks release).
    fixtures::declare_undeclared_nets(&mut e).expect("net ratings");
    let p = e.project();
    assert_eq!(p.pcb.board.stackup.copper_layers(), 4);
    let kinds: Vec<ViaKind> = p.pcb.vias.values().map(|v| v.kind()).collect();
    for k in [ViaKind::Through, ViaKind::Blind, ViaKind::Buried] {
        assert!(kinds.contains(&k), "no {k:?} via in {kinds:?}");
    }
    let (erc_r, drc_r, mech_r) = (erc::run(p), drc::run(p), mech::run(p));
    for f in &drc_r.findings {
        eprintln!("{} {:?} {}", f.rule_id, f.severity, f.message);
    }
    assert_eq!(drc_r.errors, 0, "DRC errors on the 4-layer board");
    assert!(drc_r.findings.iter().all(|f| f.rule_id != "DRC-017"));

    let dir = std::env::temp_dir().join(format!("eak-ml4-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    // The built-in Fabricator profile is 1-2 layer; this board needs a 4-layer profile, else DFM-008.
    assert!(dfm::run(p, &dfm::fabricator_standard())
        .findings
        .iter()
        .any(|f| f.rule_id == "DFM-008"));
    let mut fab4 = dfm::fabricator_standard();
    fab4.max_layers = 4;
    let dfm_r = dfm::run(p, &fab4);
    assert_eq!(dfm_r.errors, 0, "{:?}", dfm_r.findings);
    let m = compile(p, &[&erc_r, &drc_r, &mech_r, &dfm_r], &dir).expect("compile");
    assert_eq!(
        m.status,
        PackageStatus::InternallyVerifiedReleaseCandidate,
        "{:?}",
        m.blocking_reasons
    );
    let names: Vec<&str> = m.files.iter().map(|f| f.name.as_str()).collect();
    for ext in [
        "F_Cu.gtl",
        "In1_Cu.g2",
        "In2_Cu.g3",
        "B_Cu.gbl",
        "stackup.json",
        "PTH.drl",
    ] {
        assert!(
            names.iter().any(|n| n.ends_with(ext)),
            "{ext} missing in {names:?}"
        );
    }
    // One drill file for each of the blind and the buried span.
    assert!(
        names.iter().any(|n| n.ends_with("F_Cu-In1_Cu.drl")),
        "{names:?}"
    );
    assert!(
        names.iter().any(|n| n.ends_with("In1_Cu-In2_Cu.drl")),
        "{names:?}"
    );
    assert!(verify_package(&dir).unwrap().is_empty());

    // Inner plane Gerbers carry the plane copper; the buried via's pad is only on In1/In2.
    let read = |ext: &str| {
        let n = names.iter().find(|n| n.ends_with(ext)).unwrap();
        std::fs::read_to_string(dir.join(n)).unwrap()
    };
    assert!(read("In1_Cu.g2").contains("Copper,L2,Inr"));
    assert!(read("In1_Cu.g2").contains("G36*"), "GND plane has no fill");
    assert!(read("In2_Cu.g3").contains("Copper,L3,Inr"));
    assert!(read("B_Cu.gbl").contains("Copper,L4,Bot"));
    let st: serde_json::Value = serde_json::from_str(&read("stackup.json")).unwrap();
    assert_eq!(st["copper_layers"], 4);
    assert!(stackup::validate(&read("stackup.json")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fabricator_check_reports_blind_vias_and_unverified_limits() {
    let e = board(4);
    let c = stackup::fabricator_check(e.project());
    assert!(c.limits_unverified && !c.not_checked.is_empty());
    assert_eq!(c.copper_layers, 4);
    assert!(
        c.findings.iter().any(|f| f.id == "Fab-STK-005"),
        "{:?}",
        c.findings
    );
    assert_eq!(c.errors(), 0, "{:?}", c.findings);
}

#[test]
fn layer_count_changes_are_refused_while_layers_are_in_use() {
    let mut e = board(4);
    let rev = e.project().revision;
    let r = e.execute(eak_runtime::CommandRequest {
        expected_revision: rev,
        actor: eak_runtime::Actor::human(),
        command: eak_runtime::Command::SetLayerCount {
            count: 2,
            board_thickness: None,
        },
        approval: None,
    });
    assert!(
        !r.accepted,
        "dropping layers that hold zones must be refused"
    );
    assert_eq!(e.project().pcb.board.stackup.copper_layers(), 4);
}

#[test]
fn six_and_eight_layer_boards_validate_and_export() {
    for n in [6u8, 8] {
        let e = board(n);
        let p = e.project();
        assert_eq!(p.pcb.board.stackup.copper_layers(), n as usize);
        assert!(p.pcb.board.stackup.validate().is_empty());
        let drc_r = drc::run(p);
        assert_eq!(drc_r.errors, 0, "{n} layers: {:?}", drc_r.findings);
        let layers = p.pcb.board.stackup.layer_ids();
        assert_eq!(layers.first(), Some(&CopperLayer::Top));
        assert_eq!(layers.last(), Some(&CopperLayer::Bottom));
        for l in layers {
            let g = eak_manufacturing::gerber::copper_layer(p, l);
            assert!(
                eak_manufacturing::gerber::validate(&g).ok,
                "{n} layers {l:?}"
            );
        }
    }
}
