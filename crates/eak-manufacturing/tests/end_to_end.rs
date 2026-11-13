//! Full vertical slice through the shared command engine:
//! schematic → ERC → PCB sync/place/route/pour (thermal reliefs) → DRC/MECH → manufacturing
//! (stroke-font silkscreen clipped at mask openings) → re-verify.

use eak_core::{CopperLayer, Point, Project, Side};
use eak_engineering::{copper, silk};
use eak_manufacturing::{compile, verify_package, PackageStatus};
use eak_runtime::{fixtures, Engine};
use eak_validation::drc::silk_over_openings;
use eak_validation::{drc, erc, mech, Severity};

fn print(r: &eak_validation::Report) {
    for f in &r.findings {
        eprintln!("{} {:?} {}", f.rule_id, f.severity, f.message);
    }
}

#[test]
fn reference_design_end_to_end() {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).expect("schematic");
    let erc_r = erc::run(e.project());
    print(&erc_r);
    assert_eq!(erc_r.errors, 0, "ERC errors on reference schematic");

    let notes = fixtures::build_pcb(&mut e).expect("pcb");
    eprintln!("{notes:#?}");
    let erc_r = erc::run(e.project());
    assert_eq!(erc_r.errors, 0);
    let drc_r = drc::run(e.project());
    print(&drc_r);
    let mech_r = mech::run(e.project());
    print(&mech_r);
    assert_eq!(drc_r.errors, 0, "DRC errors on reference board");
    assert_eq!(mech_r.errors, 0);
    assert_eq!(drc_r.revision, e.project().revision);
    // Thermal reliefs, silkscreen text and their DRC rules are part of the verified slice.
    for id in ["DRC-014", "DRC-015"] {
        assert!(drc_r.rules.iter().any(|r| r.id == id), "{id} not declared");
        assert!(drc_r.findings.iter().all(|f| f.rule_id != id), "{id} fired");
    }
    let p = e.project();
    let items = copper::items(p);
    let zid = p.pcb.zones.keys().next().expect("GND pour");
    let fill = copper::fill_zone(p, zid, &items).unwrap();
    let relieved: Vec<String> = fill
        .reliefs
        .iter()
        .map(|r| format!("{}.{}", p.pcb.footprints[&r.footprint_id].refdes, r.pad))
        .collect();
    for pad in ["J1.2", "J2.2"] {
        assert!(relieved.contains(&pad.to_string()), "{pad}: {relieved:?}");
    }
    for r in &fill.reliefs {
        assert!(r.connected_spokes() >= drc::MIN_SPOKES, "{r:?}");
    }
    assert_eq!(copper::analyse(p).net_groups["GND"], 1);
    let raw = silk::raw_legend(p, Side::Top);
    let legend = silk::legend(p, Side::Top);
    let text: usize = p
        .pcb
        .footprints
        .values()
        .filter(|f| f.side == Side::Top)
        .map(|f| silk::text_strokes(&f.refdes, Point::default(), silk::TEXT_HEIGHT, false).len())
        .sum();
    assert!(text > 0 && raw.len() >= text + 5 * p.pcb.footprints.len());
    assert!(silk_over_openings(
        &legend,
        &silk::mask_openings(p, CopperLayer::Top),
        silk::MASK_CLEARANCE,
        Side::Top
    )
    .is_empty());

    let dir = std::env::temp_dir().join(format!("eak-mfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let m = compile(e.project(), &[&erc_r, &drc_r, &mech_r], &dir).expect("compile");
    assert_eq!(
        m.status,
        PackageStatus::InternallyVerifiedReleaseCandidate,
        "{:?}",
        m.blocking_reasons
    );
    assert_eq!(m.files.len(), 12);
    assert!(verify_package(&dir).unwrap().is_empty());
    // The written legend is exactly the clipped model: one draw per line, one flash per dot.
    let gto = m.files.iter().find(|f| f.name.ends_with(".gto")).unwrap();
    let gto = std::fs::read_to_string(dir.join(&gto.name)).unwrap();
    let count = |op: &str| gto.lines().filter(|l| l.ends_with(op)).count();
    assert_eq!(count("D01*"), legend.iter().filter(|s| s.a != s.b).count());
    assert_eq!(count("D03*"), legend.iter().filter(|s| s.a == s.b).count());

    // Tampering is detected.
    let gtl = m.files.iter().find(|f| f.name.ends_with(".gtl")).unwrap();
    std::fs::write(dir.join(&gtl.name), "G04 tampered*\nM02*\n").unwrap();
    assert!(!verify_package(&dir).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stale_or_failing_reports_produce_only_a_draft() {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).unwrap();
    fixtures::build_pcb(&mut e).unwrap();
    let erc_r = erc::run(e.project());
    let drc_r = drc::run(e.project());
    let mech_r = mech::run(e.project());
    // Change the design after verification: reports are now stale.
    let rev = e.project().revision;
    let r = e.execute(eak_runtime::CommandRequest {
        expected_revision: rev,
        actor: eak_runtime::Actor::human(),
        command: eak_runtime::Command::SetProjectName {
            name: "changed".into(),
        },
        approval: None,
    });
    assert!(r.accepted);
    let dir = std::env::temp_dir().join(format!("eak-mfg-stale-{}", std::process::id()));
    let m = compile(e.project(), &[&erc_r, &drc_r, &mech_r], &dir).unwrap();
    assert_eq!(m.status, PackageStatus::Draft);
    assert!(m.blocking_reasons.iter().any(|b| b.contains("re-run")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn drc_detects_injected_short_and_clearance_violation() {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).unwrap();
    fixtures::build_pcb(&mut e).unwrap();
    // Draw a +5V track straight across the board on top: it must collide with something.
    let rev = e.project().revision;
    let r = e.execute(eak_runtime::CommandRequest {
        expected_revision: rev,
        actor: eak_runtime::Actor::human(),
        command: eak_runtime::Command::AddTrack {
            layer: eak_core::CopperLayer::Top,
            a: eak_core::Point::mm(1.0, 15.0),
            b: eak_core::Point::mm(39.0, 15.0),
            width: None,
            net: "+5V".into(),
        },
        approval: None,
    });
    assert!(r.accepted);
    let d = drc::run(e.project());
    assert!(
        d.findings
            .iter()
            .any(|f| f.severity == Severity::Error
                && (f.rule_id == "DRC-002" || f.rule_id == "DRC-003")),
        "expected clearance/short errors"
    );
}
