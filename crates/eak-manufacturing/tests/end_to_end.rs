//! Full vertical slice through the shared command engine:
//! schematic → ERC → PCB sync/place/route/pour → DRC/MECH → manufacturing → re-verify.

use eak_core::Project;
use eak_manufacturing::{compile, verify_package, PackageStatus};
use eak_runtime::{fixtures, Engine};
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
