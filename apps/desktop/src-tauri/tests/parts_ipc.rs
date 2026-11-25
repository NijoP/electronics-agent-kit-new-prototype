//! Parts search / import / BOM / Fabricator preparation through the desktop command layer,
//! headless. Distributor is replaced by a local HTTP server; the live site is never contacted.

use eak_desktop::parts::{
    fabricator_prepare, manufacturing_preview, part_standardisation, parts_import, parts_search,
};
use eak_desktop::*;
use eak_manufacturing::distributor::{DistributorProvider, UreqFetch};
use eak_manufacturing::provider::Resilient;
use eak_runtime::index::Index;
use eak_runtime::{fixtures, Engine};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;

/// A configured (non-reserved) distributor host: the live path runs against the local fake.
const HOST: &str = "https://api.distributor.example";

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-parts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn session() -> Session {
    Session::new(Index::in_memory())
}

fn resistor_json() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../../crates/eak-manufacturing/tests/fixtures/distributor/live_shape_C25804.json",
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn distributor_site() -> Resilient<DistributorProvider<UreqFetch>> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let detail = resistor_json();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let detail = detail.clone();
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
                let (status, ctype, body): (u16, &str, Vec<u8>) = if target
                    .starts_with("/ftps/wm/product/detail?productCode=C25804")
                {
                    (200, "application/json", detail)
                } else if target.starts_with("/ftps/wm/product/detail?productCode=C99") {
                    // A part the library has no template for.
                    let mcu = br#"{"code":200,"result":{"productCode":"C99","productModel":"XYZ-MCU","brandNameEn":"Acme","parentCatalogName":"Embedded Processors & Controllers","catalogName":"Microcontrollers","encapStandard":"QFN-48"}}"#;
                    (200, "application/json", mcu.to_vec())
                } else if target.starts_with("/ftps/wm/search/global") {
                    (
                        403,
                        "text/html",
                        b"<HTML><TITLE>Access Denied</TITLE></HTML>".to_vec(),
                    )
                } else {
                    (404, "text/plain", b"no".to_vec())
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
    let mut p = DistributorProvider::new(UreqFetch::default(), &format!("http://{addr}"));
    p.pause = Duration::from_millis(1);
    Resilient::new(p).with_sleep(|_| {})
}

fn open_empty(name: &str) -> (Session, PathBuf) {
    let d = tmpdir(name);
    let s = session();
    create_project(&s, &d.display().to_string(), "Parts test").unwrap();
    (s, d)
}

#[test]
fn search_reports_a_blocked_live_search_and_still_returns_library_hits() {
    let (s, _d) = open_empty("search");
    let p = distributor_site();
    let r = parts_search(&s, &p, HOST, "resistor").unwrap();
    assert_eq!(r.live.status, "blocked");
    assert!(
        r.live.message.contains("refused automated access"),
        "{}",
        r.live.message
    );
    assert!(r.live.records.is_empty(), "no invented results");
    assert!(r.library.iter().any(|h| h.id == "R_0603"));

    let by_number = parts_search(&s, &p, HOST, "c25804").unwrap();
    assert_eq!(by_number.live.status, "ok");
    assert_eq!(
        by_number.live.records[0].mpn.as_deref(),
        Some("0603WAF1002T5E")
    );
    assert!(parts_search(&s, &p, HOST, "   ").is_err());
    // The same query again is served from the process cache, not a second request.
    let cached = parts_search(&s, &p, HOST, "C25804").unwrap();
    assert!(matches!(
        cached.live.served,
        Some(eak_manufacturing::provider::Served::Cache { .. })
    ));
}

#[test]
fn import_is_one_engine_command_with_a_revision_check_and_stores_the_record() {
    let (s, d) = open_empty("import");
    let p = distributor_site();
    let rev = current_project(&s).unwrap().project.revision;

    // A stale revision is rejected by the engine; nothing is stored.
    let stale = parts_import(&s, &p, HOST, rev + 5, "C25804").unwrap();
    assert!(!stale.response.accepted);
    assert!(!d.join("sourcing").join("C25804.json").exists());

    let ok = parts_import(&s, &p, HOST, rev, "C25804").unwrap();
    assert!(ok.response.accepted, "{:?}", ok.response.error);
    assert_eq!(ok.component_id, "DISTRIBUTOR_C25804");
    assert_eq!(ok.response.revision_after, rev + 1);
    let snap = current_project(&s).unwrap();
    let def = &snap.project.library["DISTRIBUTOR_C25804"];
    assert_eq!(def.distributor.as_deref(), Some("C25804"));
    // Commercial data is a supplier offer, separate from identity (ADR-0028).
    assert_eq!(def.supplier_part("distributor"), Some("C25804"));
    assert_eq!(def.supplier_offers.len(), 1);
    assert!(def.supplier_offers[0].fetched_at_unix_s.is_some());
    assert_eq!(def.mpn.as_deref(), Some("0603WAF1002T5E"));
    assert!(d.join("sourcing").join("C25804.json").is_file());
    // The record was written by the engine and journalled (ADR-0039), not by a direct write.
    let journal = eak_runtime::engine::read_record_journal(&d, "sourcing");
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].file, "C25804.json");
    assert_eq!(journal[0].actor, "human:user");

    // The stored record now answers an offline search.
    let again = parts_search(&s, &p, HOST, "0603WAF1002T5E").unwrap();
    assert_eq!(again.stored.len(), 1);

    // No template: refused with the reason, project untouched.
    let before = current_project(&s).unwrap().project.revision;
    let err = parts_import(&s, &p, HOST, before, "C99").unwrap_err();
    assert!(err.contains("no EAK symbol/footprint template"), "{err}");
    assert_eq!(current_project(&s).unwrap().project.revision, before);
}

fn routed_project(name: &str) -> (Session, PathBuf) {
    let d = tmpdir(name);
    {
        let mut e = Engine::create(&d, "Routed").unwrap();
        fixtures::build_schematic(&mut e).unwrap();
        fixtures::build_pcb(&mut e).unwrap();
    }
    let s = session();
    open_project(&s, &d.display().to_string(), false).unwrap();
    (s, d)
}

#[test]
fn preview_lists_bom_and_dfm_without_writing_anything() {
    let (s, d) = routed_project("preview");
    let pv = manufacturing_preview(&s).unwrap();
    assert!(!pv.bom.is_empty());
    assert!(pv.dfm.checks.iter().any(|c| c.id == "Fab-010"));
    assert!(pv.dfm.statement.contains("PREPARED FOR REVIEW"));
    assert!(!d.join("output").exists());
}

#[test]
fn prepare_writes_the_bundle_only_to_a_safe_user_folder() {
    let (s, _d) = routed_project("prepare");
    // Relative and foreign non-empty folders are refused.
    assert!(fabricator_prepare(&s, "relative/dir")
        .unwrap_err()
        .contains("absolute"));
    let foreign = tmpdir("foreign");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("keep.txt"), "mine").unwrap();
    let err = fabricator_prepare(&s, &foreign.display().to_string()).unwrap_err();
    assert!(err.contains("not empty"), "{err}");
    assert_eq!(
        std::fs::read_to_string(foreign.join("keep.txt")).unwrap(),
        "mine"
    );

    let out = tmpdir("bundle");
    let r = fabricator_prepare(&s, &out.display().to_string()).unwrap();
    assert!(out.join("fabricator-prep-manifest.json").is_file());
    assert!(r.prep.manifest.statement.contains("PREPARED FOR REVIEW"));
    assert!(r
        .prep
        .manifest
        .files
        .iter()
        .any(|f| f.name.ends_with("-Fabricator-gerbers.zip")));
    assert_eq!(r.reports.len(), 3);
    // Re-running into an earlier bundle is allowed.
    fabricator_prepare(&s, &out.display().to_string()).unwrap();
    // The default folder is inside the project.
    let dflt = fabricator_prepare(&s, "").unwrap();
    assert!(dflt.prep.out_dir.contains("fabricator-rev-"));
}

#[test]
fn nothing_is_prepared_without_an_open_project() {
    let s = session();
    assert!(manufacturing_preview(&s).is_err());
    assert!(fabricator_prepare(&s, "").is_err());
}

#[test]
fn assembly_attestations_are_written_through_the_engine_and_journalled() {
    use eak_desktop::parts::assembly_attest;
    use eak_manufacturing::assembly::AssemblyClass;
    let (s, d) = open_empty("attest");
    let rev = current_project(&s).unwrap().project.revision;

    let rec = assembly_attest(&s, "c25804", AssemblyClass::Basic).unwrap();
    assert_eq!(rec.distributor, "C25804");
    assert!(d
        .join("sourcing")
        .join("assembly")
        .join("C25804.json")
        .is_file());
    let journal = eak_runtime::engine::read_record_journal(&d, "sourcing");
    assert_eq!(journal.len(), 1, "{journal:?}");
    assert_eq!(journal[0].file, "assembly/C25804.json");
    assert_eq!(journal[0].revision, rev);
    // Not a design change: no revision and no undo step.
    assert_eq!(current_project(&s).unwrap().project.revision, rev);

    // Refused inputs write nothing and add no journal line.
    assert!(assembly_attest(&s, "../evil", AssemblyClass::Basic).is_err());
    assert!(assembly_attest(&s, "C25804", AssemblyClass::Unknown).is_err());
    assert_eq!(
        eak_runtime::engine::read_record_journal(&d, "sourcing").len(),
        1
    );
}

/// Phase 1 D1 (gap #38): the standardisation view runs the SOURCING check over the placed parts,
/// offline and read-only, and says what it does not check.
#[test]
fn part_standardisation_runs_the_sourcing_check_without_writing() {
    let (s, d) = routed_project("standardise");
    let revision = current_project(&s).unwrap().project.revision;
    let r = part_standardisation(&s).unwrap();
    assert_eq!(r.check, "SOURCING");
    assert_eq!(r.revision, revision);
    // Fixture parts carry no supplier records: missing sourcing data is reported, never passed.
    assert!(!r.findings.is_empty());
    assert!(r.findings.iter().all(|f| f.rule_id.starts_with("SRC-")));
    assert!(!r.not_implemented.is_empty());
    assert!(!r.limitations.is_empty());
    assert!(!d.join("sourcing").exists());
    assert_eq!(current_project(&s).unwrap().project.revision, revision);
    // No project open: refused, not an empty pass.
    assert!(part_standardisation(&session()).is_err());
}

/// S20 regression: `parts_search` asked the reserved `https://distributor.invalid` host on every
/// search. With a reserved host no request is made at all: the live part says
/// `not_configured`, offline library hits still come back, and import / save are refused.
#[test]
fn search_sends_no_request_to_a_reserved_invalid_host() {
    use eak_desktop::parts::{parts_save_record, require_configured, reserved_host};
    use eak_manufacturing::provider::{ProviderError, SupplierProvider};
    use eak_manufacturing::sourcing::SourcingRecord;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct Counting(Arc<AtomicUsize>);
    impl SupplierProvider for Counting {
        fn name(&self) -> &str {
            "counting"
        }
        fn lookup(&self, part: &str) -> Result<SourcingRecord, ProviderError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            panic!("no lookup may reach a reserved host (asked for {part})")
        }
        fn search(&self, query: &str) -> Result<Vec<SourcingRecord>, ProviderError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            panic!("no search may reach a reserved host (asked for {query})")
        }
    }

    let (s, _d) = open_empty("reserved");
    let asked = Arc::new(AtomicUsize::new(0));
    let p = Resilient::new(Counting(asked.clone())).with_sleep(|_| {});
    for host in [
        "https://distributor.invalid",
        "https://api.distributor.invalid/rest",
        "https://INVALID",
    ] {
        assert!(reserved_host(host), "{host}");
        let r = parts_search(&s, &p, host, "resistor").unwrap();
        assert_eq!(r.live.status, "not_configured");
        assert!(
            r.live.message.contains("no request was sent"),
            "{}",
            r.live.message
        );
        assert!(r.live.records.is_empty());
        assert!(!r.library.is_empty(), "offline library hits still returned");
        assert!(parts_import(&s, &p, host, 0, "C25804").is_err());
        assert!(parts_save_record(&s, &p, host, "C25804").is_err());
        assert!(require_configured(host).is_err());
    }
    assert_eq!(
        asked.load(Ordering::SeqCst),
        0,
        "a request reached the provider"
    );
    // The build's own placeholder host is reserved, so the shipped commands never call it.
    assert!(reserved_host(
        eak_manufacturing::distributor_api::PRODUCTION_BASE
    ));
    assert!(!reserved_host(HOST));
    assert!(require_configured(HOST).is_ok());
}
