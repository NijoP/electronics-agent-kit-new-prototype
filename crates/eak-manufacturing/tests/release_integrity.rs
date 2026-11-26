//! Manufacturing integrity (ADR-0054): failing ERC/DRC reports, UNKNOWN rules and parts whose
//! footprint does not verify each keep a package DRAFT, nothing is ever called
//! manufacturing-ready, and release packaging refuses a package exported from another revision.

use eak_core::Project;
use eak_manufacturing::{
    compile, footprint_blockers, release_gates, statement_of, status_of, verify_package_for,
    PackageStatus,
};
use eak_runtime::{fixtures, Actor, Command, CommandRequest, Engine};
use eak_validation::{dfm, drc, erc, mech, CheckState, Outcome};

fn execute(e: &mut Engine, command: Command) {
    let r = e.execute(CommandRequest {
        expected_revision: e.project().revision,
        actor: Actor::human(),
        command,
        approval: None,
    });
    assert!(r.accepted, "{r:?}");
}

/// The reference board with a height limit; `rated` also declares every net's voltage/current.
fn board(rated: bool) -> Engine {
    let mut e = Engine::in_memory(Project::new("integrity"));
    fixtures::build_schematic(&mut e).expect("schematic");
    fixtures::build_pcb(&mut e).expect("pcb");
    execute(
        &mut e,
        Command::SetDesignRules {
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
    );
    if rated {
        fixtures::declare_undeclared_nets(&mut e).expect("net ratings");
    }
    e
}

#[test]
fn unverified_footprints_block_release_and_name_the_parts() {
    let e = board(true);
    let p = e.project();
    let blockers = footprint_blockers(p);
    // Every definition on the schematic is named: none has a land-pattern table on record.
    let defs: std::collections::BTreeSet<&str> = p
        .schematic
        .symbols
        .values()
        .map(|s| s.component_id.as_str())
        .collect();
    assert_eq!(blockers.len(), defs.len(), "{blockers:?}");
    assert!(blockers.iter().all(|b| b.contains("is not verified")));
    let reports = [
        erc::run(p),
        drc::run(p),
        mech::run(p),
        dfm::run(p, &dfm::fabricator_standard()),
    ];
    let refs: Vec<_> = reports.iter().collect();
    let (_, blocking) = release_gates(p, &refs);
    for b in &blockers {
        assert!(blocking.contains(b), "{b} not in {blocking:?}");
    }
    assert_eq!(status_of(&blocking), PackageStatus::Draft);
}

#[test]
fn failing_and_unknown_checks_keep_the_package_draft() {
    // Unrated nets: DRC evaluates the voltage rule as UNKNOWN, which is never a pass.
    let e = board(false);
    let p = e.project();
    let erc_r = erc::run(p);
    let drc_r = drc::run(p);
    let mech_r = mech::run(p);
    let dfm_r = dfm::run(p, &dfm::fabricator_standard());
    assert!(drc_r
        .rule_results
        .iter()
        .any(|r| r.state == CheckState::Unknown));
    let (_, blocking) = release_gates(p, &[&erc_r, &drc_r, &mech_r, &dfm_r]);
    assert!(
        blocking
            .iter()
            .any(|b| b.starts_with("DRC: ") && b.contains("is UNKNOWN")),
        "{blocking:?}"
    );
    // A failing ERC report blocks on its own.
    let mut failing = erc_r.clone();
    failing.outcome = Outcome::Errors;
    failing.errors = 1;
    let (_, blocking) = release_gates(p, &[&failing, &drc_r, &mech_r, &dfm_r]);
    assert!(blocking.contains(&"ERC reports 1 error(s)".to_string()));
    // A missing DRC report blocks too.
    let (_, blocking) = release_gates(p, &[&erc_r, &mech_r, &dfm_r]);
    assert!(blocking.contains(&"DRC report not supplied".to_string()));
    assert_eq!(status_of(&blocking), PackageStatus::Draft);
    // No status is ever worded as manufacturing-ready.
    for s in [
        PackageStatus::Draft,
        PackageStatus::InternallyVerifiedReleaseCandidate,
    ] {
        let text = statement_of(s).to_lowercase();
        assert!(
            !text.contains("manufacturing-ready") || text.contains("not a manufacturing-ready"),
            "{text}"
        );
    }
}

#[test]
fn packaging_refuses_a_package_from_another_revision() {
    let mut e = board(true);
    let dir = std::env::temp_dir().join(format!("eak-mfg-integrity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    compile(e.project(), &[], &dir).expect("compile");
    assert!(verify_package_for(&dir, e.project()).unwrap().is_empty());
    execute(
        &mut e,
        Command::SetProjectName {
            name: "changed".into(),
        },
    );
    let problems = verify_package_for(&dir, e.project()).unwrap();
    assert!(
        problems.iter().any(|p| p.contains("export again")),
        "{problems:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
