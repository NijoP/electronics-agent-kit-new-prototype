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
    let m = compile(e.project(), &[], &dir).expect("compile");
    assert!(verify_package_for(&dir, e.project()).unwrap().is_empty());
    // Every Gerber and drill file names the design hash and revision it was generated from.
    let stamp = format!(
        "EAK-IR sha256 {} revision {}",
        m.input_sha256,
        e.project().revision
    );
    for f in m
        .files
        .iter()
        .filter(|f| f.kind == "gerber" || f.kind == "drill")
    {
        let text = std::fs::read_to_string(dir.join(&f.name)).unwrap();
        assert!(text.contains(&stamp), "{} lacks {stamp}", f.name);
    }
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
    assert!(
        problems
            .iter()
            .any(|p| p.contains("was not generated from this design revision")),
        "{problems:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

mod footprint_inputs {
    use eak_core::datasheet::{DatasheetSource, DatasheetValue, Extraction, LimitClass, PartScope};
    use eak_core::geom::mm;
    use eak_core::{PadShape, Point, Project};
    use eak_manufacturing::{footprint_blockers, footprint_blockers_with, FootprintInputs};
    use eak_validation::footprint::{FabLimits, LandPatternSpec, PadSpec};

    const LDO: &str = "LDO_SOT-23-5";

    fn source() -> DatasheetSource {
        DatasheetSource {
            manufacturer: "Example".into(),
            document_sha256: "0".repeat(64),
            document_revision: Some("Rev. A".into()),
            page: Some(12),
            section: Some("Recommended land pattern".into()),
            table: None,
        }
    }

    fn val(parameter: &str, value: f64) -> DatasheetValue {
        DatasheetValue {
            parameter: parameter.into(),
            class: LimitClass::Characteristic,
            statistic: None,
            value,
            unit: "mm".into(),
            conditions: String::new(),
            scope: PartScope::default(),
            source: source(),
            extraction: Extraction::HumanVerified {
                reviewer: "A. Reviewer".into(),
                at_unix_s: 1_790_000_000,
            },
        }
    }

    /// The reviewed land pattern the builtin SOT-23-5 was drawn from (as in ADR-0052's tests).
    fn spec() -> LandPatternSpec {
        let pads = [
            ("1", -0.95, -1.1),
            ("2", 0.0, -1.1),
            ("3", 0.95, -1.1),
            ("4", 0.95, 1.1),
            ("5", -0.95, 1.1),
        ];
        LandPatternSpec {
            source: source(),
            entered_by: "A. Reviewer".into(),
            pads: pads
                .iter()
                .map(|(n, x, y)| PadSpec {
                    number: (*n).into(),
                    x: val("x", *x),
                    y: val("y", *y),
                    size_x: val("pad width", 0.6),
                    size_y: val("pad length", 1.1),
                    shape: Some(PadShape::Rect),
                    drill: None,
                    lead: None,
                })
                .collect(),
            pitch: Some(val("pitch", 0.95)),
            body: Some((val("body length", 2.9), val("body width", 1.6))),
            height_max: Some(val("height max", 1.45)),
            pin1: Some(Point::mm(-0.95, -1.1)),
        }
    }

    fn limits() -> FabLimits {
        FabLimits {
            min_clearance: mm(0.15),
            min_mask_web: mm(0.1),
            mask_expansion: mm(0.05),
            min_annular_ring: mm(0.15),
            drill_allowance: mm(0.2),
            courtyard_excess: Some(mm(0.25)),
        }
    }

    /// A project whose schematic uses only the SOT-23-5 regulator (one symbol of the reference
    /// design re-pointed at it; the gate reads only the symbol's definition and reference).
    fn ldo_project() -> Project {
        let mut e = eak_runtime::Engine::in_memory(Project::new("ldo"));
        eak_runtime::fixtures::build_schematic(&mut e).expect("schematic");
        let mut p = e.project().clone();
        let def = eak_core::builtin::components()
            .into_iter()
            .find(|c| c.id == LDO)
            .expect("builtin SOT-23-5");
        p.library.insert(LDO.into(), def);
        let keep = p
            .schematic
            .symbols
            .keys()
            .next()
            .cloned()
            .expect("a symbol");
        p.schematic.symbols.retain(|k, _| *k == keep);
        for s in p.schematic.symbols.values_mut() {
            s.component_id = LDO.into();
            s.refdes = "U1".into();
        }
        p
    }

    #[test]
    fn a_reviewed_land_pattern_on_record_lifts_the_footprint_blocker() {
        let p = ldo_project();
        let without = footprint_blockers(&p);
        assert_eq!(without.len(), 1, "{without:?}");
        assert!(without[0].contains("of U1 is not verified"), "{without:?}");
        let mut inputs = FootprintInputs {
            limits: Some(limits()),
            ..Default::default()
        };
        inputs.specs.insert(LDO.into(), spec());
        assert!(footprint_blockers_with(&p, &inputs).is_empty());
        // A table that no longer matches the footprint blocks again.
        let mut wrong = spec();
        wrong.pads[0].size_x = val("pad width", 0.9);
        inputs.specs.insert(LDO.into(), wrong);
        let blocked = footprint_blockers_with(&p, &inputs);
        assert_eq!(blocked.len(), 1, "{blocked:?}");
        assert!(blocked[0].contains("FP-002"), "{blocked:?}");
    }
}
