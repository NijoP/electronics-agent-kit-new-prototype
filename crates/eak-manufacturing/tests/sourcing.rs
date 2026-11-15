//! Offline Distributor sourcing tests. Every input is a synthetic fixture under `tests/fixtures`
//! (see its README); nothing here touches the network.

use eak_core::{builtin, Point, Project, Rotation};
use eak_manufacturing::bom_csv;
use eak_manufacturing::sourcing::{
    apply_identity, ingest_product_detail, read_records, search, write_record, SourcingError,
    SourcingRecord, VerificationState, MAX_RESPONSE_BYTES,
};
use eak_runtime::{fixtures, Actor, Command, Engine};
use eak_validation::{sourcing as check, Report, Severity};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

const URL: &str = "https://distributor.invalid/fixture/product-detail/C990000001";
const T0: u64 = 1_794_571_200;

fn fixture(rel: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn ingest(rel: &str) -> SourcingRecord {
    ingest_product_detail(&fixture(rel), URL, T0).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn sha(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eak-sourcing-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn count(r: &Report, component: &str, rule: &str, sev: Severity) -> usize {
    r.findings
        .iter()
        .filter(|f| f.rule_id == rule && f.severity == sev && f.message.starts_with(component))
        .count()
}

#[test]
fn ingest_records_provenance_and_fields_exactly_as_parsed() {
    let raw = fixture("distributor/product_detail_resistor.json");
    let r = ingest_product_detail(&raw, URL, T0).unwrap();
    assert_eq!(r.provider, "Distributor");
    assert_eq!(r.source_url, URL);
    assert_eq!(r.retrieved_at_unix_s, T0);
    assert_eq!(r.raw_bytes, raw.len() as u64);
    // Hash of the committed fixture, computed independently with `sha256sum`.
    assert_eq!(
        r.raw_sha256,
        "7154d439e729b112fca14e4951a600067389a3845c7e973c4d2a1faddbec41ff"
    );
    assert_eq!(r.raw_sha256, sha(&raw));
    assert_eq!(r.distributor_part.as_deref(), Some("C990000001"));
    assert_eq!(r.mpn.as_deref(), Some("EAKFIX-R0603-10K"));
    assert_eq!(r.manufacturer.as_deref(), Some("EAK Test Fixtures"));
    assert_eq!(r.package.as_deref(), Some("0603"));
    assert_eq!(
        r.datasheet_url.as_deref(),
        Some("https://distributor.invalid/fixture/EAKFIX-R0603-10K.pdf")
    );
    assert_eq!(r.stock, Some(12000));
    let prices: Vec<(u64, &str, &str)> = r
        .price_breaks
        .iter()
        .map(|p| (p.min_quantity, p.unit_price.as_str(), p.currency.as_str()))
        .collect();
    assert_eq!(prices, [(100, "0.0012", "USD"), (1000, "0.0009", "USD")]);
    let specs: Vec<(&str, &str)> = r
        .specs
        .iter()
        .map(|s| (s.name.as_str(), s.value.as_str()))
        .collect();
    assert_eq!(
        specs,
        [
            ("Resistance", "10kΩ"),
            ("Tolerance", "±1%"),
            ("Power(Watts)", "100mW")
        ]
    );
    assert!(r.missing.is_empty() && r.rejected.is_empty());
    // A freshly ingested record is a search hit, never a verified part.
    assert_eq!(r.state, VerificationState::SearchHit);
    assert!(r.datasheet.is_none() && r.attestation.is_none());
    assert_eq!(
        r.supplier_part().unwrap().state,
        VerificationState::SearchHit
    );
}

#[test]
fn stock_and_price_are_recorded_only_when_present() {
    let r = ingest("distributor/product_detail_bare_mcu.json");
    assert_eq!(r.distributor_part.as_deref(), Some("C990000002"));
    assert_eq!(r.package.as_deref(), Some("SOIC-8"));
    assert_eq!(r.stock, None);
    assert!(r.price_breaks.is_empty());
    assert!(r.missing.is_empty(), "{:?}", r.missing);
}

#[test]
fn missing_fields_are_reported_and_never_invented() {
    let mut r = ingest("distributor/product_detail_missing_fields.json");
    assert_eq!(
        r.missing,
        ["manufacturer", "package", "datasheet_url", "specs"]
    );
    assert_eq!(
        (&r.manufacturer, &r.package, &r.datasheet_url),
        (&None, &None, &None)
    );
    assert!(r.specs.is_empty() && r.stock.is_none() && r.price_breaks.is_empty());
    // Even a hashed datasheet and a human reviewer cannot verify an incomplete record.
    let pdf = fixture("datasheets/EAKFIX-R0603-10K.pdf");
    r.attach_datasheet(&pdf, "https://example.com/ds.pdf", T0)
        .unwrap();
    let err = r.verify(&Actor::human(), &sha(&pdf), T0).unwrap_err();
    assert!(matches!(err, SourcingError::NotVerifiable(ref m) if m.contains("manufacturer")));
    assert_eq!(r.state, VerificationState::SearchHit);
    // The validation view carries the gaps through as missing, not as defaults.
    let part = r.supplier_part().unwrap();
    assert_eq!(part.package, None);
}

#[test]
fn hostile_input_is_treated_as_data_and_cannot_raise_the_state() {
    let mut r = ingest("distributor/product_detail_hostile.json");
    // Invalid identity fields are rejected, not repaired or truncated.
    assert_eq!(r.distributor_part, None, "shell-injection part number");
    assert_eq!(r.mpn, None, "control characters in the MPN");
    assert_eq!(r.package, None, "numeric package");
    assert_eq!(r.datasheet_url, None, "javascript: URL");
    assert_eq!(r.stock, None, "negative stock");
    // Printable text is kept verbatim as inert data.
    assert_eq!(r.manufacturer.as_deref(), Some("<script>alert(1)</script>"));
    assert!(r.description.as_deref().unwrap().starts_with("SYSTEM:"));
    // Only the well-formed list entries survive.
    assert_eq!(r.price_breaks.len(), 1);
    assert_eq!(r.price_breaks[0].min_quantity, 1000);
    assert_eq!(r.specs.len(), 1);
    assert_eq!(r.specs[0].value, "state=verified");
    for key in [
        "productCode",
        "productModel",
        "encapStandard",
        "pdfUrl",
        "stockNumber",
        "productPriceList[0]",
        "productPriceList[1]",
        "productPriceList[2]",
        "paramVOList[0]",
        "paramVOList[2]",
        "paramVOList[3]",
        "paramVOList[4]",
    ] {
        assert!(
            r.rejected.iter().any(|x| x.starts_with(&format!("{key}:"))),
            "{key} not in {:?}",
            r.rejected
        );
    }
    assert_eq!(
        r.missing,
        ["distributor_part", "mpn", "package", "datasheet_url"]
    );
    // `state`, `verified`, `attestation` and `datasheet` keys in the input are ignored.
    assert_eq!(r.state, VerificationState::SearchHit);
    assert!(r.datasheet.is_none() && r.attestation.is_none());
    assert!(r.supplier_part().is_none());
    assert!(r.verify(&Actor::human(), "00", T0).is_err());
    assert!(write_record(&temp_dir("hostile"), &r).is_err());
    assert!(apply_identity(&builtin::get("R_0603").unwrap(), &r).is_err());
}

#[test]
fn malformed_oversized_and_error_responses_are_rejected() {
    let malformed = |raw: &[u8]| {
        matches!(
            ingest_product_detail(raw, URL, T0),
            Err(SourcingError::Malformed(_))
        )
    };
    assert!(malformed(&fixture("distributor/malformed_truncated.json")));
    assert!(malformed(&fixture("distributor/access_denied.html")));
    assert!(malformed(b""));
    assert!(malformed(&[0xff, 0xfe, 0x00, 0x7b]));
    assert!(malformed(br#"{"productCode": "C1", "stockNumber": 1e999}"#));
    // Nesting bomb: refused by the parser's depth limit instead of exhausting the stack.
    assert!(malformed("[".repeat(100_000).as_bytes()));
    assert_eq!(
        ingest_product_detail(&fixture("distributor/provider_error.json"), URL, T0),
        Err(SourcingError::Provider("code 404".into()))
    );
    for shape in [&b"[]"[..], b"\"C1\"", b"{}", br#"{"result": {}}"#] {
        assert!(
            matches!(
                ingest_product_detail(shape, URL, T0),
                Err(SourcingError::UnexpectedShape(_))
            ),
            "{}",
            String::from_utf8_lossy(shape)
        );
    }
    assert!(matches!(
        ingest_product_detail(br#"{"code": 200, "result": null}"#, URL, T0),
        Err(SourcingError::Provider(_))
    ));
    let big = vec![b' '; MAX_RESPONSE_BYTES + 1];
    assert_eq!(
        ingest_product_detail(&big, URL, T0),
        Err(SourcingError::TooLarge(MAX_RESPONSE_BYTES + 1))
    );
}

#[test]
fn source_url_must_be_https_on_the_provider_domain() {
    let raw = fixture("distributor/product_detail_resistor.json");
    for ok in [
        "https://distributor.invalid/x",
        "https://WWW.Distributor.INVALID:443/x?y#z",
    ] {
        assert!(ingest_product_detail(&raw, ok, T0).is_ok(), "{ok}");
    }
    for bad in [
        "",
        "http://distributor.invalid/x",
        "file:///etc/passwd",
        "https://distributor.invalid.evil.example/x",
        "https://evil-distributor.invalid/x",
        "https://distributor.invalid@evil.example/x",
        "https://distributor.invalid/x\nInjected: header",
        "https:///x",
    ] {
        assert!(
            matches!(
                ingest_product_detail(&raw, bad, T0),
                Err(SourcingError::BadSourceUrl(_))
            ),
            "{bad:?}"
        );
    }
}

#[test]
fn datasheet_hash_matching_and_human_verification() {
    let mut r = ingest("distributor/product_detail_resistor.json");
    let ds_url = r.datasheet_url.clone().unwrap();

    // A non-PDF body (an error page) or a non-https URL is refused and records nothing.
    let html = fixture("distributor/access_denied.html");
    assert!(matches!(
        r.attach_datasheet(&html, &ds_url, T0),
        Err(SourcingError::BadDatasheet(_))
    ));
    let pdf = fixture("datasheets/EAKFIX-R0603-10K.pdf");
    assert!(r
        .attach_datasheet(&pdf, "ftp://example.com/a.pdf", T0)
        .is_err());
    assert!(r.datasheet.is_none());
    assert!(r.verify(&Actor::human(), &sha(&pdf), T0).is_err());

    // A PDF that does not name the MPN: hash recorded, still only a search hit.
    let other = b"%PDF-1.4\n% some other family datasheet\n%%EOF\n";
    r.attach_datasheet(other, &ds_url, T0 + 1).unwrap();
    let ev = r.datasheet.clone().unwrap();
    assert_eq!(ev.sha256, sha(other));
    assert!(!ev.names_mpn);
    assert_eq!(r.state, VerificationState::SearchHit);

    // The datasheet naming the MPN: datasheet-matched, not verified.
    r.attach_datasheet(&pdf, &ds_url, T0 + 2).unwrap();
    let ev = r.datasheet.clone().unwrap();
    assert_eq!(
        ev.sha256,
        "e8ca991106cacf7343040e1ef16cb5a852bcad05d88912c7a1d488bb679530b2"
    );
    assert_eq!(
        (ev.bytes, ev.retrieved_at_unix_s),
        (pdf.len() as u64, T0 + 2)
    );
    assert!(ev.names_mpn);
    assert_eq!(r.state, VerificationState::DatasheetMatched);

    // Agents and unnamed reviewers cannot verify; nor can a different datasheet hash.
    for actor in [
        Actor::agent("lead"),
        Actor {
            kind: eak_runtime::ActorKind::Cli,
            name: "cli".into(),
            session: None,
        },
        Actor {
            kind: eak_runtime::ActorKind::Human,
            name: " ".into(),
            session: None,
        },
    ] {
        assert!(matches!(
            r.verify(&actor, &ev.sha256, T0 + 3),
            Err(SourcingError::NotVerifiable(_))
        ));
    }
    assert!(r.verify(&Actor::human(), &sha(other), T0 + 3).is_err());
    assert_eq!(r.state, VerificationState::DatasheetMatched);

    r.verify(&Actor::human(), &ev.sha256, T0 + 3).unwrap();
    assert_eq!(r.state, VerificationState::Verified);
    let a = r.attestation.clone().unwrap();
    assert_eq!((a.reviewer.as_str(), a.at_unix_s), ("user", T0 + 3));
    assert_eq!(
        r.supplier_part().unwrap().state,
        VerificationState::Verified
    );

    // Replacing the datasheet voids the attestation that was bound to the old file.
    r.attach_datasheet(other, &ds_url, T0 + 4).unwrap();
    assert!(r.attestation.is_none());
    assert_eq!(r.state, VerificationState::SearchHit);
}

#[test]
fn records_work_offline_and_a_stored_state_claim_is_not_trusted() {
    let dir = temp_dir("store");
    let mut resistor = ingest("distributor/product_detail_resistor.json");
    let pdf = fixture("datasheets/EAKFIX-R0603-10K.pdf");
    let ds_url = resistor.datasheet_url.clone().unwrap();
    resistor.attach_datasheet(&pdf, &ds_url, T0).unwrap();
    let mcu = ingest("distributor/product_detail_bare_mcu.json");
    let path = write_record(&dir, &resistor).unwrap();
    assert!(path.ends_with("C990000001.json"));
    write_record(&dir, &mcu).unwrap();

    // A hand-edited file claiming `verified` with no evidence behind it.
    let mut forged = mcu.clone();
    forged.distributor_part = Some("C990000009".into());
    forged.state = VerificationState::Verified;
    std::fs::write(
        dir.join("C990000009.json"),
        serde_json::to_string(&forged).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("notes.txt"), "not a record").unwrap();

    let loaded = read_records(&dir).unwrap();
    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded[0], resistor);
    assert_eq!(loaded[0].state, VerificationState::DatasheetMatched);
    assert_eq!(loaded[1], mcu);
    assert_eq!(loaded[2].state, VerificationState::SearchHit);

    // Offline search over the stored records.
    let hits = |q: &str| -> Vec<String> {
        search(&loaded, q)
            .iter()
            .map(|r| r.distributor_part.clone().unwrap())
            .collect()
    };
    assert_eq!(hits("eakfix 0603"), ["C990000001"]);
    assert_eq!(hits("SOIC-8"), ["C990000002", "C990000009"]);
    assert_eq!(hits("c990000001"), ["C990000001"]);
    assert!(hits("").is_empty() && hits("no-such-part").is_empty());

    // A corrupt record file is an error, not a silently skipped part.
    std::fs::write(dir.join("C990000010.json"), "{ truncated").unwrap();
    assert!(matches!(
        read_records(&dir),
        Err(SourcingError::Malformed(_))
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn distributor_selection_reaches_the_library_bom_and_report_through_commands() {
    let mut e = Engine::in_memory(Project::new("src"));
    fixtures::build_schematic(&mut e).expect("schematic");

    // Baseline: the generic passives are unmapped and reported, never passed.
    let rows = check::bom_rows(e.project());
    assert!(rows
        .iter()
        .any(|r| r.component_id == "R_0603" && r.distributor.is_none()));
    let base = check::run(e.project(), &[]);
    assert!(count(&base, "R_0603", "SRC-002", Severity::Error) == 1);
    assert!(count(&base, "AMS1117-3.3_SOT-223", "SRC-005", Severity::Error) == 1);

    // Select the fixture resistor into a value-specific library definition.
    let mut rec = ingest("distributor/product_detail_resistor.json");
    let generic = builtin::get("R_0603").unwrap();
    let mut specific = generic.clone();
    specific.id = "R_0603_EAKFIX_10K".into();
    let specific = apply_identity(&specific, &rec).unwrap();
    assert_eq!(specific.distributor.as_deref(), Some("C990000001"));
    assert_eq!(specific.mpn.as_deref(), Some("EAKFIX-R0603-10K"));
    assert_eq!(specific.manufacturer.as_deref(), Some("EAK Test Fixtures"));
    assert_eq!(specific.datasheet, rec.datasheet_url);
    assert!(specific.provenance.contains(&rec.raw_sha256));
    // Supplier data never touches library engineering data.
    assert_eq!(specific.ratings, generic.ratings);
    assert_eq!(specific.symbol, generic.symbol);
    assert_eq!(specific.footprint, generic.footprint);
    assert_eq!(specific.pin_map, generic.pin_map);

    // An existing, different identity is a conflict and is refused.
    let project_lib = &e.project().library;
    assert!(apply_identity(&project_lib["ATTINY85_SOIC-8"], &rec)
        .unwrap_err()
        .contains("MPN"));
    assert!(apply_identity(&project_lib["AMS1117-3.3_SOT-223"], &rec)
        .unwrap_err()
        .contains("Distributor part number"));

    // The library and schematic change only through the shared command engine.
    let mut b = fixtures::Builder {
        engine: &mut e,
        actor: Actor::human(),
    };
    b.run(Command::UpsertComponentDefinition {
        definition: Box::new(specific),
    })
    .unwrap();
    b.run(Command::PlaceSymbol {
        sheet_id: None,
        component_id: "R_0603_EAKFIX_10K".into(),
        position: Point::mm(200.0, 150.0),
        rotation: Rotation::R0,
        refdes: None,
        value: Some("10k".into()),
    })
    .unwrap();

    let rows = check::bom_rows(e.project());
    let row = rows
        .iter()
        .find(|r| r.component_id == "R_0603_EAKFIX_10K")
        .unwrap();
    assert_eq!(row.distributor.as_deref(), Some("C990000001"));
    assert!(
        bom_csv(e.project()).contains("\"C990000001\",\"EAK Test Fixtures\",\"EAKFIX-R0603-10K\"")
    );

    // Search hit: an error. Datasheet-matched: a warning. Verified by a human: clear.
    let c = "R_0603_EAKFIX_10K";
    let report = |rec: &SourcingRecord| check::run(e.project(), &[rec.supplier_part().unwrap()]);
    let r = report(&rec);
    assert_eq!(count(&r, c, "SRC-006", Severity::Error), 1);
    assert_eq!(count(&r, c, "SRC-011", Severity::Error), 1);
    for clean in [
        "SRC-001", "SRC-002", "SRC-003", "SRC-005", "SRC-007", "SRC-008", "SRC-010",
    ] {
        assert_eq!(count(&r, c, clean, Severity::Error), 0, "{clean}");
        assert_eq!(count(&r, c, clean, Severity::Warning), 0, "{clean}");
    }
    let pdf = fixture("datasheets/EAKFIX-R0603-10K.pdf");
    let ds_url = rec.datasheet_url.clone().unwrap();
    rec.attach_datasheet(&pdf, &ds_url, T0).unwrap();
    let r = report(&rec);
    assert_eq!(count(&r, c, "SRC-006", Severity::Warning), 1);
    assert_eq!(count(&r, c, "SRC-006", Severity::Error), 0);
    assert_eq!(count(&r, c, "SRC-011", Severity::Error), 0);
    rec.verify(&Actor::human(), &sha(&pdf), T0).unwrap();
    let r = report(&rec);
    assert!(!r
        .findings
        .iter()
        .any(|f| f.rule_id == "SRC-006" && f.message.starts_with(c)));
    // A verified supplier record does not verify the library's own ratings.
    assert_eq!(count(&r, c, "SRC-004", Severity::Warning), 1);
    // The report is bound to the revision it ran against and lists what it does not check.
    assert_eq!(r.check, "SOURCING");
    assert_eq!(r.revision, e.project().revision);
    assert!(!r.not_implemented.is_empty() && !r.limitations.is_empty());
}
