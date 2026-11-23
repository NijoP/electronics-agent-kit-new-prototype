//! Full vertical slice through the shared command engine:
//! schematic → ERC → PCB sync/place/route/pour (thermal reliefs) → DRC/MECH → manufacturing
//! (stroke-font silkscreen clipped at mask openings) → re-verify.

use eak_core::{CopperLayer, Point, Project, Side};
use eak_engineering::{copper, silk};
use eak_manufacturing::{compile, verify_package, PackageStatus};
use eak_runtime::{fixtures, Engine};
use eak_validation::drc::silk_over_openings;
use eak_validation::{dfm, drc, erc, mech, Severity};

/// Declares a 25 mm component height limit so MECH-001 is evaluated (unknown fit blocks release).
pub fn set_height_limit(e: &mut Engine) {
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
        },
        approval: None,
    });
    assert!(r.accepted, "{r:?}");
}

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
    set_height_limit(&mut e);
    // ADR-0048: every net rated, so no voltage or current rule is UNKNOWN (that blocks release).
    fixtures::declare_undeclared_nets(&mut e).expect("net ratings");
    eprintln!("{notes:#?}");
    let erc_r = erc::run(e.project());
    assert_eq!(erc_r.errors, 0);
    let drc_r = drc::run(e.project());
    print(&drc_r);
    let mech_r = mech::run(e.project());
    print(&mech_r);
    assert_eq!(drc_r.errors, 0, "DRC errors on reference board");
    assert_eq!(mech_r.errors, 0);
    let dfm_r = dfm::run(e.project(), &dfm::fabricator_standard());
    print(&dfm_r);
    assert_eq!(dfm_r.errors, 0, "DFM errors on reference board");
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
    let m = compile(e.project(), &[&erc_r, &drc_r, &mech_r, &dfm_r], &dir).expect("compile");
    assert_eq!(
        m.status,
        PackageStatus::InternallyVerifiedReleaseCandidate,
        "{:?}",
        m.blocking_reasons
    );
    assert_eq!(m.files.len(), 15); // 2 copper, 4 mask/paste, 2 legend, edge, drill, BOM, CPL, stackup, README, fab pack
                                   // The fab pack holds every Gerber and drill file plus the README of layers.
    let zip = m.files.iter().find(|f| f.kind == "fab-pack").unwrap();
    let names =
        eak_manufacturing::fabpack::list(&std::fs::read(dir.join(&zip.name)).unwrap()).unwrap();
    assert_eq!(names.len(), 11, "{names:?}");
    assert!(names.contains(&"README.txt".to_string()));
    let readme = m.files.iter().find(|f| f.kind == "readme").unwrap();
    let readme = std::fs::read_to_string(dir.join(&readme.name)).unwrap();
    assert!(
        readme.contains("Top copper")
            && readme.contains("Board outline")
            && readme.contains("Plated through holes"),
        "{readme}"
    );
    assert!(!readme
        .to_lowercase()
        .contains("manufacturing-ready claim: yes"));
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
    let dfm_r = dfm::run(e.project(), &dfm::fabricator_standard());
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
    let m = compile(e.project(), &[&erc_r, &drc_r, &mech_r, &dfm_r], &dir).unwrap();
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
            arc_deg: None,
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

#[test]
fn bom_groups_by_value_footprint_and_mpn_and_cpl_lists_every_part() {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).unwrap();
    fixtures::build_pcb(&mut e).unwrap();
    let p = e.project();
    let bom = eak_manufacturing::bom_csv(p);
    let mut lines = bom.lines();
    assert_eq!(
        lines.next().unwrap(),
        "Comment,Designator,Footprint,Distributor Part #,Manufacturer,MPN,Quantity"
    );
    let mut keys = std::collections::BTreeSet::new();
    let mut total = 0;
    for l in lines {
        // Quoted fields: Comment, Designator, Footprint, Distributor, Manufacturer, MPN; then qty.
        let f: Vec<&str> = l.split("\",\"").collect();
        assert_eq!(f.len(), 6, "{l}");
        let qty: usize = l.rsplit(',').next().unwrap().parse().unwrap();
        assert_eq!(f[1].split(',').count(), qty, "{l}");
        assert!(
            keys.insert((f[0], f[2], f[5].rsplit_once("\",").unwrap().0)),
            "duplicate group {l}"
        );
        total += qty;
    }
    assert_eq!(total, p.schematic.symbols.len());
    // Two parts with the same value and footprint but different MPNs are separate lines.
    let mut q = p.clone();
    let (sid, comp) = q
        .schematic
        .symbols
        .values()
        .map(|s| (s.id.clone(), s.component_id.clone()))
        .next()
        .unwrap();
    let mut alt = q.library[&comp].clone();
    alt.id = format!("{comp}-alt");
    alt.mpn = Some("ALT-MPN-1".into());
    q.library.insert(alt.id.clone(), alt.clone());
    let base_lines = eak_manufacturing::bom_csv(&q).lines().count();
    q.schematic.symbols.get_mut(&sid).unwrap().component_id = alt.id.clone();
    let split = eak_manufacturing::bom_csv(&q);
    assert!(split.contains("ALT-MPN-1"), "{split}");
    assert!(split.lines().count() >= base_lines, "{split}");

    let cpl = eak_manufacturing::cpl_csv(p);
    assert!(cpl.starts_with("Designator,Mid X,Mid Y,Layer,Rotation\n"));
    assert_eq!(cpl.lines().count(), 1 + p.pcb.footprints.len());
}

#[test]
fn unknown_mechanical_fit_keeps_the_package_draft() {
    let mut e = Engine::in_memory(Project::new("ref"));
    fixtures::build_schematic(&mut e).unwrap();
    fixtures::build_pcb(&mut e).unwrap();
    let run = |p: &Project| {
        (
            erc::run(p),
            drc::run(p),
            mech::run(p),
            dfm::run(p, &dfm::fabricator_standard()),
        )
    };
    // No height limit: MECH-001 checked nothing, so the package stays DRAFT.
    let (a, b, m, d) = run(e.project());
    assert!(m.limitations.iter().any(|l| l.contains("MECH-001")));
    let dir = std::env::temp_dir().join(format!("eak-mfg-mech-{}", std::process::id()));
    let out = compile(e.project(), &[&a, &b, &m, &d], &dir).unwrap();
    assert_eq!(out.status, PackageStatus::Draft);
    assert!(
        out.blocking_reasons.iter().any(|r| r.contains("MECH-001")),
        "{:?}",
        out.blocking_reasons
    );
    // A part without body data (MECH-003) also blocks, even with a limit declared.
    set_height_limit(&mut e);
    let (a, b, mut m, d) = run(e.project());
    assert!(!m.limitations.iter().any(|l| l.contains("MECH-001")));
    let fp = e
        .project()
        .pcb
        .footprints
        .values()
        .next()
        .unwrap()
        .id
        .clone();
    m.findings.push(eak_validation::Finding {
        id: "MECH-003-test".into(),
        rule_id: "MECH-003".into(),
        severity: Severity::Warning,
        message: "no 3D body height".into(),
        objects: vec![fp],
        location: None,
        measured: None,
        required: None,
        suggested_fix: None,
    });
    let out = compile(e.project(), &[&a, &b, &m, &d], &dir).unwrap();
    assert_eq!(out.status, PackageStatus::Draft);
    assert!(
        out.blocking_reasons.iter().any(|r| r.contains("MECH-003")),
        "{:?}",
        out.blocking_reasons
    );
    let _ = std::fs::remove_dir_all(&dir);
}
