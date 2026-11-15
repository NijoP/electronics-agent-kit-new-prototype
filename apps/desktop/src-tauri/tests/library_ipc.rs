//! Library scopes (personal, imported open-source), user metadata, merged browse and the
//! supplier-record save, headless (ADR-0033). The stores run in temporary folders; nothing
//! touches the real configuration directory.

use eak_core::builtin;
use eak_desktop::libstore::{browse, clean_tags, safe_name, BrowseQuery, LibraryStore};
use eak_desktop::parts::parts_save_record;
use eak_desktop::*;
use eak_manufacturing::library_link::Standing;
use eak_manufacturing::provider::{ErrorKind, ProviderError, Resilient, SupplierProvider};
use eak_manufacturing::sourcing::{SourcingRecord, VerificationState};
use eak_runtime::index::Index;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-lib-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn store(name: &str) -> LibraryStore {
    LibraryStore::new(tmp(name).join("library"))
}

fn resistor() -> eak_core::ComponentDef {
    builtin::get("R_0603").expect("R_0603 is built in")
}

fn write_def(dir: &std::path::Path, def: &eak_core::ComponentDef) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join(format!("{}.json", def.id)),
        serde_json::to_string_pretty(def).unwrap(),
    )
    .unwrap();
}

#[test]
fn names_that_could_escape_the_library_folder_are_refused() {
    for bad in ["", "../x", "a/b", "a\\b", ".hidden", "x y", "é"] {
        assert!(safe_name(bad).is_err(), "{bad:?}");
    }
    assert_eq!(
        safe_name("DISTRIBUTOR_C25804").unwrap(),
        "DISTRIBUTOR_C25804"
    );
}

#[test]
fn personal_library_stores_only_consistent_definitions_and_lists_them() {
    let st = store("personal");
    let mut d = resistor();
    d.id = "MY_R".into();
    st.personal_save(d.clone()).unwrap();
    let (defs, problems) = st.personal();
    assert!(problems.is_empty());
    assert_eq!(defs.len(), 1);
    assert!(defs[0].provenance.contains("saved to the personal library"));

    // A definition whose pin map points at a missing pad is refused, never stored.
    let mut broken = resistor();
    broken.id = "BROKEN".into();
    broken
        .pin_map
        .insert("1".into(), vec!["no-such-pad".into()]);
    let e = st.personal_save(broken).unwrap_err();
    assert!(e.contains("not consistent"), "{e}");
    assert_eq!(st.personal().0.len(), 1);

    let mut path_like = resistor();
    path_like.id = "../evil".into();
    assert!(st.personal_save(path_like).is_err());

    st.personal_remove("MY_R").unwrap();
    st.personal_remove("MY_R").unwrap();
    assert!(st.personal().0.is_empty());
}

#[test]
fn an_unreadable_personal_file_is_reported_not_hidden() {
    let st = store("badfile");
    let mut d = resistor();
    d.id = "OK".into();
    st.personal_save(d).unwrap();
    let dir = tmp_keep("badfile").join("library").join("personal");
    std::fs::write(dir.join("junk.json"), "{ not json").unwrap();
    let (defs, problems) = st.personal();
    assert_eq!(defs.len(), 1);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("junk.json"));
}

/// The same folder `tmp` created, without deleting it.
fn tmp_keep(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("eak-lib-{name}-{}", std::process::id()))
}

#[test]
fn an_open_source_library_is_imported_only_with_a_licence_and_is_marked_unverified() {
    let st = store("import");
    let src = tmp("import-src");
    let mut ok = resistor();
    ok.id = "OSS_R".into();
    write_def(&src, &ok);
    let mut broken = resistor();
    broken.id = "OSS_BAD".into();
    broken.pin_map.insert("1".into(), vec!["nope".into()]);
    write_def(&src, &broken);

    // No manifest, then a manifest without a licence: refused.
    assert!(st
        .import_library(&src)
        .unwrap_err()
        .contains("manifest.json"));
    std::fs::write(
        src.join("manifest.json"),
        r#"{"name":"kicad-lite","licence":"  ","source_url":"https://example.invalid/lib"}"#,
    )
    .unwrap();
    assert!(st.import_library(&src).unwrap_err().contains("no licence"));
    assert!(st.imported_summary().is_empty(), "nothing was imported");

    std::fs::write(
        src.join("manifest.json"),
        r#"{"name":"kicad-lite","licence":"CC-BY-SA-4.0","source_url":"https://example.invalid/lib"}"#,
    )
    .unwrap();
    let summary = st.import_library(&src).unwrap();
    assert_eq!(summary.accepted, ["OSS_R"]);
    assert_eq!(
        summary.rejected.len(),
        1,
        "the inconsistent file is listed, not imported"
    );
    let libs = st.imported();
    assert_eq!(libs.len(), 1);
    let def = &libs[0].1[0];
    assert!(def.provenance.contains("CC-BY-SA-4.0"));
    assert!(def.provenance.contains("not verified by EAK"));
    assert!(def.provenance.contains("example.invalid"));

    // Importing the same library name twice is refused; removing then works.
    assert!(st
        .import_library(&src)
        .unwrap_err()
        .contains("already imported"));
    st.remove_imported("kicad-lite").unwrap();
    assert!(st.imported().is_empty());
}

#[test]
fn favourites_tags_and_recents_are_normalised_bounded_and_persistent() {
    let st = store("meta");
    st.set_favourite("R_0603", true).unwrap();
    st.set_tags(
        "R_0603",
        &[
            " Passive ".into(),
            "PASSIVE".into(),
            "smd".into(),
            "".into(),
        ],
    )
    .unwrap();
    for i in 0..25 {
        st.touch(&format!("P{i}")).unwrap();
    }
    st.touch("P3").unwrap();
    let m = st.meta();
    assert!(m.favourites.contains("R_0603"));
    assert_eq!(
        m.tags["R_0603"].iter().cloned().collect::<Vec<_>>(),
        ["passive", "smd"]
    );
    assert_eq!(m.recent.len(), 20);
    assert_eq!(m.recent[0], "P3", "most recent first, no duplicate");
    assert_eq!(m.recent.iter().filter(|x| *x == "P3").count(), 1);

    assert!(clean_tags(&(0..9).map(|i| format!("t{i}")).collect::<Vec<_>>()).is_err());
    assert!(clean_tags(&["x".repeat(25)]).is_err());
    assert!(st.set_favourite("../x", true).is_err());

    st.set_favourite("R_0603", false).unwrap();
    st.set_tags("R_0603", &[]).unwrap();
    let m = LibraryStore::new(tmp_keep("meta").join("library")).meta();
    assert!(!m.favourites.contains("R_0603") && !m.tags.contains_key("R_0603"));
}

#[test]
fn browse_merges_scopes_without_merging_entries_and_filters_by_source_package_and_favourite() {
    let st = store("browse");
    let mut mine = resistor();
    mine.id = "R_0603".into(); // same id as the built-in part: a different scope, not merged
    mine.manufacturer = Some("Mine Co".into());
    st.personal_save(mine).unwrap();
    st.set_favourite("R_0603", true).unwrap();
    st.set_tags("R_0603", &["bench".into()]).unwrap();

    let project: BTreeMap<String, eak_core::ComponentDef> = BTreeMap::new();
    let all = browse(&st, &project, &[], &BrowseQuery::default());
    let same_id: Vec<_> = all.entries.iter().filter(|e| e.id == "R_0603").collect();
    assert!(
        same_id.len() >= 2,
        "built-in and personal copies stay separate"
    );
    assert!(all.facets.sources.iter().any(|f| f.value == "personal"));
    assert!(all.facets.sources.iter().any(|f| f.value == "builtin"));
    // Favourites and tags are per part id, so both copies of R_0603 are starred and sort first.
    assert!(all.entries[0].favourite && all.entries[1].favourite);
    assert!(all.entries[0].id == "R_0603" && all.entries[1].id == "R_0603");

    let personal = browse(
        &st,
        &project,
        &[],
        &BrowseQuery {
            source: Some("personal".into()),
            ..BrowseQuery::default()
        },
    );
    assert_eq!(personal.total, 1);
    assert_eq!(personal.entries[0].tags, ["bench"]);

    let tagged = browse(
        &st,
        &project,
        &[],
        &BrowseQuery {
            tag: Some("BENCH".into()),
            favourites_only: Some(true),
            source: Some("personal".into()),
            ..BrowseQuery::default()
        },
    );
    assert_eq!(tagged.total, 1);

    let by_package = browse(
        &st,
        &project,
        &[],
        &BrowseQuery {
            package: Some(resistor().footprint.name),
            source: Some("builtin".into()),
            ..BrowseQuery::default()
        },
    );
    assert!(by_package.total >= 1);
    assert!(by_package
        .entries
        .iter()
        .all(|e| e.scope_label == "builtin"));
    // Facets count what the text query matched, before the other filters.
    assert!(by_package
        .facets
        .sources
        .iter()
        .any(|f| f.value == "personal"));
}

struct Fake;

fn record() -> SourcingRecord {
    SourcingRecord {
        provider: "Distributor".into(),
        source_url: "https://distributor.invalid/rest/api/agent/product/v1/keywordsearch".into(),
        retrieved_at_unix_s: 1_700_000_000,
        raw_sha256: "ab".repeat(32),
        raw_bytes: 10,
        distributor_part: Some("C99999".into()),
        mpn: Some("STM32F103C8T6".into()),
        manufacturer: Some("ST".into()),
        package: Some("LQFP-48".into()),
        category: Some("Microcontrollers".into()),
        description: None,
        datasheet_url: None,
        specs: vec![],
        stock: Some(10),
        price_breaks: vec![],
        missing: vec![],
        rejected: vec![],
        datasheet: None,
        attestation: None,
        state: VerificationState::SearchHit,
    }
}

impl SupplierProvider for Fake {
    fn name(&self) -> &str {
        "fake"
    }
    fn lookup(&self, part: &str) -> Result<SourcingRecord, ProviderError> {
        if part.eq_ignore_ascii_case("C99999") {
            Ok(record())
        } else {
            Err(ProviderError {
                provider: "fake".into(),
                kind: ErrorKind::NotFound,
                message: "no such part".into(),
            })
        }
    }
    fn search(&self, _q: &str) -> Result<Vec<SourcingRecord>, ProviderError> {
        Ok(vec![record()])
    }
}

#[test]
fn a_supplier_listing_is_saved_without_an_engineering_definition_and_says_it_needs_authoring() {
    let d = tmp("save");
    let s = Session::new(Index::in_memory());
    let snap = create_project(&s, &d.display().to_string(), "Save test").unwrap();
    let p = Resilient::new(Fake).with_sleep(|_| {});

    let saved = parts_save_record(&s, &p, "c99999").unwrap();
    // No definition exists and no template can carry a microcontroller: nothing is invented.
    assert_eq!(saved.link.standing, Standing::NeedsAuthoring);
    assert!(saved.link.import_template.is_none());
    assert!(saved.link.unverified);
    assert!(std::path::Path::new(&saved.path).is_file());
    assert!(saved.path.contains("sourcing"));

    // The project itself did not change: no definition, no revision bump.
    let again = current_project(&s).unwrap();
    assert_eq!(again.project.revision, snap.project.revision);
    assert!(!again.project.library.contains_key("DISTRIBUTOR_C99999"));

    let e = parts_save_record(&s, &p, "C1").unwrap_err();
    assert!(e.contains("no such part"));
}
