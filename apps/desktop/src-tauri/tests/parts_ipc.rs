//! Parts search / import / BOM / JLCPCB preparation through the desktop command layer,
//! headless. LCSC is replaced by a local HTTP server; the live site is never contacted.

use eak_desktop::parts::{jlcpcb_prepare, manufacturing_preview, parts_import, parts_search};
use eak_desktop::*;
use eak_manufacturing::lcsc::{LcscProvider, UreqFetch};
use eak_manufacturing::provider::Resilient;
use eak_runtime::index::Index;
use eak_runtime::{fixtures, Engine};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-parts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn session() -> Session {
    Session::new(Index::in_memory())
}

fn resistor_json() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../crates/eak-manufacturing/tests/fixtures/lcsc/live_shape_C25804.json");
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn lcsc_site() -> Resilient<LcscProvider<UreqFetch>> {
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
    let mut p = LcscProvider::new(UreqFetch::default(), &format!("http://{addr}"));
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
    let p = lcsc_site();
    let r = parts_search(&s, &p, "resistor").unwrap();
    assert_eq!(r.live.status, "blocked");
    assert!(
        r.live.message.contains("refused automated access"),
        "{}",
        r.live.message
    );
    assert!(r.live.records.is_empty(), "no invented results");
    assert!(r.library.iter().any(|h| h.id == "R_0603"));

    let by_number = parts_search(&s, &p, "c25804").unwrap();
    assert_eq!(by_number.live.status, "ok");
    assert_eq!(
        by_number.live.records[0].mpn.as_deref(),
        Some("0603WAF1002T5E")
    );
    assert!(parts_search(&s, &p, "   ").is_err());
    // The same query again is served from the process cache, not a second request.
    let cached = parts_search(&s, &p, "C25804").unwrap();
    assert!(matches!(
        cached.live.served,
        Some(eak_manufacturing::provider::Served::Cache { .. })
    ));
}

#[test]
fn import_is_one_engine_command_with_a_revision_check_and_stores_the_record() {
    let (s, d) = open_empty("import");
    let p = lcsc_site();
    let rev = current_project(&s).unwrap().project.revision;

    // A stale revision is rejected by the engine; nothing is stored.
    let stale = parts_import(&s, &p, rev + 5, "C25804").unwrap();
    assert!(!stale.response.accepted);
    assert!(!d.join("sourcing").join("C25804.json").exists());

    let ok = parts_import(&s, &p, rev, "C25804").unwrap();
    assert!(ok.response.accepted, "{:?}", ok.response.error);
    assert_eq!(ok.component_id, "LCSC_C25804");
    assert_eq!(ok.response.revision_after, rev + 1);
    let snap = current_project(&s).unwrap();
    let def = &snap.project.library["LCSC_C25804"];
    assert_eq!(def.lcsc.as_deref(), Some("C25804"));
    // Commercial data is a supplier offer, separate from identity (ADR-0027).
    assert_eq!(def.supplier_part("lcsc"), Some("C25804"));
    assert_eq!(def.supplier_offers.len(), 1);
    assert!(def.supplier_offers[0].fetched_at_unix_s.is_some());
    assert_eq!(def.mpn.as_deref(), Some("0603WAF1002T5E"));
    assert!(d.join("sourcing").join("C25804.json").is_file());

    // The stored record now answers an offline search.
    let again = parts_search(&s, &p, "0603WAF1002T5E").unwrap();
    assert_eq!(again.stored.len(), 1);

    // No template: refused with the reason, project untouched.
    let before = current_project(&s).unwrap().project.revision;
    let err = parts_import(&s, &p, before, "C99").unwrap_err();
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
    assert!(pv.dfm.checks.iter().any(|c| c.id == "JLC-010"));
    assert!(pv.dfm.statement.contains("PREPARED FOR REVIEW"));
    assert!(!d.join("output").exists());
}

#[test]
fn prepare_writes_the_bundle_only_to_a_safe_user_folder() {
    let (s, _d) = routed_project("prepare");
    // Relative and foreign non-empty folders are refused.
    assert!(jlcpcb_prepare(&s, "relative/dir")
        .unwrap_err()
        .contains("absolute"));
    let foreign = tmpdir("foreign");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("keep.txt"), "mine").unwrap();
    let err = jlcpcb_prepare(&s, &foreign.display().to_string()).unwrap_err();
    assert!(err.contains("not empty"), "{err}");
    assert_eq!(
        std::fs::read_to_string(foreign.join("keep.txt")).unwrap(),
        "mine"
    );

    let out = tmpdir("bundle");
    let r = jlcpcb_prepare(&s, &out.display().to_string()).unwrap();
    assert!(out.join("jlcpcb-prep-manifest.json").is_file());
    assert!(r.prep.manifest.statement.contains("PREPARED FOR REVIEW"));
    assert!(r
        .prep
        .manifest
        .files
        .iter()
        .any(|f| f.name.ends_with("-JLCPCB-gerbers.zip")));
    assert_eq!(r.reports.len(), 3);
    // Re-running into an earlier bundle is allowed.
    jlcpcb_prepare(&s, &out.display().to_string()).unwrap();
    // The default folder is inside the project.
    let dflt = jlcpcb_prepare(&s, "").unwrap();
    assert!(dflt.prep.out_dir.contains("jlcpcb-rev-"));
}

#[test]
fn nothing_is_prepared_without_an_open_project() {
    let s = session();
    assert!(manufacturing_preview(&s).is_err());
    assert!(jlcpcb_prepare(&s, "").is_err());
}
