//! LCSC provider, BOM and JLCPCB preparation tests.
//!
//! The LCSC provider is exercised over real HTTP against a local test server on 127.0.0.1;
//! the live site is never contacted. The product-detail fixture has the shape of a real
//! response captured from LCSC's public endpoint on 2026-11-14 (trimmed to the fields the
//! ingest boundary reads). The keyword-search response shape could not be observed (LCSC
//! answers 403), so the only keyword-search success test uses an assumed shape and says so.

use eak_core::{builtin, Point, Project};
use eak_manufacturing::bom;
use eak_manufacturing::jlcpcb::{self, Status};
use eak_manufacturing::lcsc::{self, LcscError, LcscProvider, UreqFetch};
use eak_manufacturing::sourcing::SourcingRecord;
use eak_runtime::{fixtures, Actor, Command, Engine};
use eak_validation::{drc, erc, mech};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn fixture(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

type Route = (&'static str, u16, &'static str, Vec<u8>);

/// Serve `routes` (path-and-query prefix, status, content type, body) on an ephemeral port.
fn serve(routes: Vec<Route>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let routes = Arc::new(routes);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let routes = routes.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let target = req
                    .lines()
                    .next()
                    .and_then(|l| l.split(' ').nth(1))
                    .unwrap_or("/")
                    .to_string();
                let hit = routes.iter().find(|r| target.starts_with(r.0));
                let (status, ctype, body) = match hit {
                    Some(r) => (r.1, r.2, r.3.clone()),
                    None => (404, "text/plain", b"no route".to_vec()),
                };
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(head.as_bytes());
                let _ = s.write_all(&body);
            });
        }
    });
    format!("http://{addr}")
}

fn provider(base: &str) -> LcscProvider<UreqFetch> {
    let mut p = LcscProvider::new(UreqFetch::default(), base);
    p.pause = Duration::from_millis(1);
    p
}

fn lcsc_site() -> String {
    serve(vec![
        (
            "/ftps/wm/product/detail?productCode=C25804",
            200,
            "application/json",
            fixture("lcsc/live_shape_C25804.json"),
        ),
        (
            "/ftps/wm/product/detail?productCode=C0",
            200,
            "application/json",
            br#"{"code":200,"msg":null,"result":null,"ok":true}"#.to_vec(),
        ),
        (
            "/ftps/wm/search/global?keyword=blocked",
            403,
            "text/html",
            b"<HTML><HEAD><TITLE>Access Denied</TITLE></HEAD><BODY>Access Denied</BODY></HTML>"
                .to_vec(),
        ),
        // ASSUMED shape (LCSC refuses the real endpoint): product objects under result.
        (
            "/ftps/wm/search/global?keyword=10k",
            200,
            "application/json",
            br#"{"code":200,"result":{"productSearchResultVO":{"productList":[{"productCode":"C25804"},{"productCode":"C0"},{"productCode":"C25804"}]}}}"#.to_vec(),
        ),
    ])
}

fn resistor_record() -> SourcingRecord {
    provider(&lcsc_site()).lookup("c25804").expect("lookup")
}

#[test]
fn lookup_by_part_number_returns_a_search_hit_with_provenance() {
    let r = resistor_record();
    assert_eq!(r.lcsc_part.as_deref(), Some("C25804"));
    assert_eq!(r.mpn.as_deref(), Some("0603WAF1002T5E"));
    assert_eq!(r.package.as_deref(), Some("0603"));
    assert_eq!(r.stock, Some(1_004_300));
    assert_eq!(r.price_breaks[0].min_quantity, 100);
    assert_eq!(
        r.category.as_deref(),
        Some("Resistors / Chip Resistor - Surface Mount")
    );
    assert!(r
        .datasheet_url
        .as_deref()
        .unwrap()
        .starts_with("https://datasheet.lcsc.com/"));
    // Provenance names LCSC even though a local server answered; the raw bytes are hashed.
    assert!(r.source_url.starts_with("https://wmsc.lcsc.com/"));
    assert_eq!(r.raw_sha256.len(), 64);
    assert_eq!(format!("{:?}", r.state), "SearchHit");
}

#[test]
fn unknown_part_is_not_found_and_bad_queries_never_reach_the_network() {
    let p = provider(&lcsc_site());
    assert!(matches!(p.lookup("C0"), Err(LcscError::NotFound(_))));
    for bad in ["", "  ", "C25804;DROP", "25804", "C"] {
        assert!(
            matches!(p.lookup(bad), Err(LcscError::BadQuery(_))),
            "{bad:?}"
        );
    }
    assert!(matches!(p.search("   "), Err(LcscError::BadQuery(_))));
    assert!(matches!(
        p.search(&"x".repeat(500)),
        Err(LcscError::BadQuery(_))
    ));
    // Nothing listening: a transport error, not an empty result.
    let dead = provider("http://127.0.0.1:1");
    assert!(matches!(
        dead.lookup("C25804"),
        Err(LcscError::Transport(_))
    ));
}

#[test]
fn blocked_keyword_search_is_reported_not_faked() {
    let p = provider(&lcsc_site());
    let err = p.search("blocked").unwrap_err();
    match &err {
        LcscError::Blocked { status, note } => {
            assert_eq!(*status, 403);
            assert!(note.contains("Access Denied"), "{note}");
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
    assert!(err.to_string().contains("refused automated access"));
}

#[test]
fn keyword_search_resolves_codes_through_the_detail_path_and_dedups() {
    let hits = provider(&lcsc_site()).search("10k").unwrap();
    // C25804 appears twice in the (assumed-shape) response, C0 does not exist: one record.
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].lcsc_part.as_deref(), Some("C25804"));
    // A part number needs no search endpoint at all.
    let direct = provider(&lcsc_site()).search(" c25804 ").unwrap();
    assert_eq!(direct.len(), 1);
}

#[test]
fn import_goes_through_the_command_engine_and_reaches_the_bom() {
    let rec = resistor_record();
    let plan = lcsc::import_plan(&rec).unwrap();
    assert_eq!(plan.definition.id, "LCSC_C25804");
    assert_eq!(plan.template_id, "R_0603");
    assert_eq!(plan.definition.default_value, "10k");
    assert_eq!(plan.definition.lcsc.as_deref(), Some("C25804"));
    assert_eq!(plan.definition.mpn.as_deref(), Some("0603WAF1002T5E"));
    assert!(plan.definition.provenance.contains(&rec.raw_sha256));
    // LCSC supplies identity only: ratings stay exactly the template's, unverified.
    let tpl = builtin::get("R_0603").unwrap();
    assert_eq!(plan.definition.footprint, tpl.footprint);
    assert_eq!(plan.definition.pin_map, tpl.pin_map);
    assert!(plan.definition.ratings.iter().all(|r| !r.verified));

    let mut e = Engine::in_memory(Project::new("import"));
    let mut b = fixtures::Builder {
        engine: &mut e,
        actor: Actor::human(),
    };
    b.run(Command::UpsertComponentDefinition {
        definition: Box::new(plan.definition),
    })
    .unwrap();
    b.run(Command::PlaceSymbol {
        sheet_id: None,
        component_id: "LCSC_C25804".into(),
        position: Point::mm(50.0, 50.0),
        rotation: eak_core::Rotation::R0,
        refdes: None,
        value: None,
    })
    .unwrap();
    let lines = bom::build(e.project(), &[rec]);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].lcsc.as_deref(), Some("C25804"));
    assert_eq!(lines[0].value, "10k");
    assert_eq!(lines[0].stock, Some(1_004_300));
    assert_eq!(lines[0].unit_price_usd.as_deref(), Some("0.0022")); // MOQ break, qty 1
    assert!(lines[0]
        .issues
        .iter()
        .any(|i| i.contains("no footprint on the PCB")));
}

#[test]
fn import_refuses_parts_without_a_template_and_guesses_nothing() {
    let mut rec = resistor_record();
    rec.package = Some("QFN-48".into());
    rec.category = Some("Embedded Processors & Controllers / Microcontrollers".into());
    rec.mpn = Some("NOT-IN-LIBRARY".into());
    let err = lcsc::import_plan(&rec).unwrap_err();
    assert!(err.contains("no EAK symbol/footprint template"), "{err}");
    assert!(err.contains("no footprint was guessed"));
    // A resistor in a package the library lacks is refused too.
    let mut odd = resistor_record();
    odd.package = Some("0201".into());
    odd.mpn = Some("NOT-IN-LIBRARY-2".into());
    assert!(lcsc::import_plan(&odd).is_err());
}

fn reference() -> (Engine, Vec<eak_validation::Report>) {
    let mut e = Engine::in_memory(Project::new("Ref Design"));
    fixtures::build_schematic(&mut e).unwrap();
    fixtures::build_pcb(&mut e).unwrap();
    let reports = vec![
        erc::run(e.project()),
        drc::run(e.project()),
        mech::run(e.project()),
    ];
    (e, reports)
}

#[test]
fn bom_marks_missing_lcsc_numbers_and_never_invents_them() {
    let (e, _) = reference();
    let lines = bom::build(e.project(), &[]);
    assert!(!lines.is_empty());
    for l in lines.iter().filter(|l| l.lcsc.is_none()) {
        assert!(l.issues.iter().any(|i| i.contains("no LCSC part number")));
    }
    let csv = bom::jlcpcb_csv(&lines);
    assert!(csv.starts_with("Comment,Designator,Footprint,LCSC Part #\n"));
    assert_eq!(
        csv.lines().count() - 1,
        lines.iter().filter(|l| l.assembled()).count()
    );
}

#[test]
fn bom_csv_neutralises_formula_injection() {
    let (mut e, _) = reference();
    let id = e
        .project()
        .schematic
        .symbols
        .values()
        .find(|s| s.refdes.starts_with('R'))
        .unwrap()
        .id
        .clone();
    let mut b = fixtures::Builder {
        engine: &mut e,
        actor: Actor::human(),
    };
    b.run(Command::SetSymbolValue {
        symbol_id: id,
        value: "=HYPERLINK(\"http://x\")".into(),
    })
    .unwrap();
    let csv = bom::detail_csv(&bom::build(e.project(), &[]));
    assert!(csv.contains("\"'=HYPERLINK(\"\"http://x\"\")\""), "{csv}");
}

#[test]
fn prepared_bundle_has_jlcpcb_formats_a_valid_zip_and_honest_labels() {
    let (e, reports) = reference();
    let refs: Vec<&eak_validation::Report> = reports.iter().collect();
    let dir = std::env::temp_dir().join(format!("eak-jlc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = jlcpcb::prepare(e.project(), &refs, &[], &dir).expect("prepare");

    let stem: String = e
        .project()
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let read = |n: &str| std::fs::read_to_string(dir.join(n)).unwrap();
    let bom = read(&format!("{stem}-JLCPCB-BOM.csv"));
    assert!(bom.starts_with("Comment,Designator,Footprint,LCSC Part #\n"));
    let cpl = read(&format!("{stem}-JLCPCB-CPL.csv"));
    assert!(cpl.starts_with("Designator,Mid X,Mid Y,Layer,Rotation\n"));
    assert!(cpl
        .lines()
        .skip(1)
        .all(|l| l.split(',').count() == 5 && l.contains("mm,")));
    // Surface-mount only: the through-hole header must not be in the placement file.
    assert!(!cpl.lines().any(|l| l.starts_with("J1,")));

    let zip = std::fs::read(dir.join(format!("{stem}-JLCPCB-gerbers.zip"))).unwrap();
    let entries = eak_manufacturing::zip::read_stored(&zip).expect("valid zip");
    let names: Vec<_> = entries.iter().map(|(n, _)| n.as_str()).collect();
    for ext in [
        ".gtl", ".gbl", ".gts", ".gbs", ".gto", ".gbo", ".gm1", ".drl",
    ] {
        assert!(
            names.iter().any(|n| n.ends_with(ext)),
            "{ext} missing from {names:?}"
        );
    }
    assert!(entries.iter().all(|(_, d)| !d.is_empty()));

    // Sizes in the manifest match the files on disk.
    for f in &r.manifest.files {
        let data = std::fs::read(dir.join(&f.name)).unwrap();
        assert_eq!(f.bytes, data.len() as u64);
    }
    let m = read("jlcpcb-prep-manifest.json");
    assert!(m.contains("PREPARED FOR REVIEW"));
    let lower = r
        .manifest
        .statement
        .to_lowercase()
        .replace("not manufacturing-ready", "");
    assert!(
        !lower.contains("manufacturing-ready"),
        "{}",
        r.manifest.statement
    );
    assert!(r
        .manifest
        .not_verified
        .iter()
        .any(|s| s.contains("JLCPCB's own DFM")));
    // The reference design carries no LCSC numbers, so the assembly rule must fail loudly.
    let c = r.dfm.checks.iter().find(|c| c.id == "JLC-040").unwrap();
    assert_eq!(c.status, Status::Fail);
    assert!(r.manifest.statement.contains("BLOCKED"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dfm_flags_violations_and_never_reports_unchecked_rules_as_passing() {
    let (e, _) = reference();
    let profile = jlcpcb::Profile::two_layer_1oz();
    assert!(!profile.source_url.is_empty() && !profile.verified_on.is_empty());
    let base = jlcpcb::dfm(e.project(), &bom::build(e.project(), &[]), &profile);
    let get =
        |r: &jlcpcb::DfmReport, id: &str| r.checks.iter().find(|c| c.id == id).unwrap().status;
    // The routed reference board satisfies the measurable geometry rules.
    for id in [
        "JLC-001", "JLC-002", "JLC-010", "JLC-011", "JLC-012", "JLC-030",
    ] {
        assert_eq!(
            get(&base, id),
            Status::Pass,
            "{id}: {:?}",
            base.checks.iter().find(|c| c.id == id)
        );
    }
    for id in [
        "JLC-050", "JLC-051", "JLC-052", "JLC-053", "JLC-054", "JLC-055",
    ] {
        assert_eq!(get(&base, id), Status::NotChecked, "{id}");
    }
    assert_eq!(
        base.failures
            + base.warnings
            + base.not_checked
            + base
                .checks
                .iter()
                .filter(|c| c.status == Status::Pass)
                .count(),
        base.checks.len()
    );

    // Inject violations directly into a copy of the IR (the checker is a pure function).
    let mut bad = e.project().clone();
    let t = bad.pcb.tracks.values_mut().next().unwrap();
    t.width = 50_000; // 0.05 mm
    let v_id = bad.pcb.vias.keys().next().cloned();
    if let Some(id) = &v_id {
        let v = bad.pcb.vias.get_mut(id).unwrap();
        v.drill = 100_000; // 0.1 mm
        v.diameter = 150_000; // ring 0.025 mm
    }
    bad.pcb.board.stackup.copper_layers = 4;
    let r = jlcpcb::dfm(&bad, &bom::build(&bad, &[]), &profile);
    assert_eq!(get(&r, "JLC-010"), Status::Fail);
    assert_eq!(get(&r, "JLC-002"), Status::Fail);
    if v_id.is_some() {
        assert_eq!(get(&r, "JLC-020"), Status::Fail);
        assert_eq!(get(&r, "JLC-021"), Status::Fail);
    }
    assert!(r.failures >= 2);

    // No outline: the size rule fails and edge clearance is not claimed as measured.
    let mut none = e.project().clone();
    none.pcb.board.outline.clear();
    let r = jlcpcb::dfm(&none, &[], &profile);
    assert_eq!(get(&r, "JLC-001"), Status::Fail);
    assert_eq!(get(&r, "JLC-030"), Status::NotChecked);
}

#[test]
fn dfm_refuses_to_pass_a_multilayer_board_against_the_two_layer_profile() {
    let mut e = Engine::in_memory(Project::new("ml"));
    fixtures::build_schematic(&mut e).expect("schematic");
    fixtures::build_pcb_layers(&mut e, 4).expect("pcb");
    let profile = jlcpcb::Profile::two_layer_1oz();
    let r = jlcpcb::dfm(e.project(), &bom::build(e.project(), &[]), &profile);
    let c = r.checks.iter().find(|c| c.id == "JLC-002").unwrap();
    assert_eq!(c.status, Status::Fail);
    assert!(c.measured.starts_with("4 layers"), "{}", c.measured);
    assert!(
        c.detail.iter().any(|d| d.contains("limits for a 4-layer")),
        "{:?}",
        c.detail
    );
}
