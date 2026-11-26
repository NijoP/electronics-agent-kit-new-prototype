//! Part sources and the resolver: identity by (manufacturer, MPN), exact hits merged, fuzzy
//! hits kept apart, and a source that is not ready is never called.

use eak_library::part::{PartKey, SourceId};
use eak_library::source::{
    resolve, MatchKind, PartQuery, PartSource, SourceError, SourceKind, SourceStatus,
};
use eak_library::sources::{BuiltinSource, SupplierRecordSource};
use eak_manufacturing::sourcing::{PriceBreak, SourcingRecord, Spec, VerificationState};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn listing(part: &str, mpn: Option<&str>, maker: Option<&str>) -> SourcingRecord {
    SourcingRecord {
        provider: "test-supplier".into(),
        source_url: format!("https://supplier.invalid/{part}"),
        retrieved_at_unix_s: 1_700_000_000,
        raw_sha256: format!("sha-{part}"),
        raw_bytes: 1,
        distributor_part: Some(part.into()),
        mpn: mpn.map(str::to_string),
        manufacturer: maker.map(str::to_string),
        package: Some("SOT-223".into()),
        category: None,
        description: Some("LDO regulator".into()),
        datasheet_url: None,
        specs: vec![Spec {
            name: "Output voltage".into(),
            value: "3.3V".into(),
        }],
        stock: Some(120),
        price_breaks: vec![PriceBreak {
            min_quantity: 10,
            unit_price: "0.0712".into(),
            currency: "USD".into(),
        }],
        missing: vec![],
        rejected: vec![],
        datasheet: None,
        attestation: None,
        state: VerificationState::SearchHit,
    }
}

/// A supplier source whose search counts its calls and returns fixed listings.
fn counting_supplier(status: SourceStatus, calls: Arc<AtomicUsize>) -> SupplierRecordSource {
    SupplierRecordSource::new(
        "supplier:test",
        SourceKind::Network,
        status,
        "test terms: listings only",
        Box::new(move |_q: &PartQuery| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok::<_, SourceError>(vec![
                listing(
                    "C1",
                    Some("AMS1117-3.3"),
                    Some("Advanced Monolithic Systems"),
                ),
                listing("C2", Some("AMS1117-3.3"), Some("Other Maker Co")),
                listing(
                    "C3",
                    Some("AMS1117-5.0"),
                    Some("Advanced Monolithic Systems"),
                ),
                listing("C4", Some("AMS1117-3.3"), None),
            ])
        }),
    )
}

#[test]
fn a_source_that_needs_setup_is_reported_and_never_called() {
    let calls = Arc::new(AtomicUsize::new(0));
    let supplier = counting_supplier(
        SourceStatus::NeedsSetup("no credentials".into()),
        calls.clone(),
    );
    let builtin = BuiltinSource::new();
    let r = resolve(&[&builtin, &supplier], &PartQuery::new("AMS1117-3.3"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "zero requests without setup"
    );
    let skipped = r
        .sources
        .iter()
        .find(|s| s.source == SourceId::new("supplier:test"))
        .expect("the unready source is listed");
    assert_eq!(
        skipped.status,
        SourceStatus::NeedsSetup("no credentials".into())
    );
    // The builtin still answers on its own.
    assert_eq!(r.parts.len(), 1);
    assert_eq!(
        r.parts[0].part.sources(),
        vec![SourceId::new("eak-builtin")]
    );
    // Calling the source directly is refused too.
    assert!(matches!(
        supplier.search(&PartQuery::new("x")),
        Err(SourceError::Unavailable(_))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn exact_hits_merge_by_key_across_sources_and_fuzzy_hits_stay_suggestions() {
    let calls = Arc::new(AtomicUsize::new(0));
    let supplier = counting_supplier(SourceStatus::Ready, calls.clone());
    let builtin = BuiltinSource::new();
    let q = PartQuery::new("ams1117 3.3");
    let r = resolve(&[&builtin, &supplier], &q);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // Same MPN from two manufacturers: two parts, never merged.
    assert_eq!(r.parts.len(), 2, "{:#?}", r.parts);
    let ams = PartKey::new("Advanced Monolithic Systems", "AMS1117-3.3").unwrap();
    let merged = r.parts.iter().find(|p| p.part.key == ams).unwrap();
    assert_eq!(merged.kind, MatchKind::ExactMpn);
    assert_eq!(
        merged.part.sources(),
        vec![SourceId::new("eak-builtin"), SourceId::new("supplier:test")]
    );
    assert_eq!(merged.part.definitions().len(), 1, "the builtin definition");
    let offers = merged.part.offers();
    assert_eq!(offers.len(), 1, "only the supplier has an offer");
    assert_eq!(offers[0].stock, Some(120));
    assert_eq!(offers[0].breaks[0].unit_price, "0.0712");
    assert_eq!(offers[0].retrieved_at_unix_s, 1_700_000_000);
    assert_eq!(offers[0].supplier_pn.as_deref(), Some("C1"));

    // A different MPN is a suggestion only.
    assert_eq!(r.suggestions.len(), 1);
    assert_eq!(r.suggestions[0].display_mpn, "AMS1117-5.0");

    // The listing without a manufacturer is reported, not dropped.
    let unkeyed: Vec<&String> = r.sources.iter().flat_map(|s| &s.unkeyed).collect();
    assert_eq!(unkeyed.len(), 1, "{unkeyed:?}");
    assert!(unkeyed[0].starts_with("C4:"));
}

#[test]
fn a_manufacturer_hint_ranks_its_part_first_and_turns_the_other_maker_into_a_suggestion() {
    let calls = Arc::new(AtomicUsize::new(0));
    let supplier = counting_supplier(SourceStatus::Ready, calls);
    let mut q = PartQuery::new("AMS1117-3.3");
    q.manufacturer = Some("advanced-monolithic systems".into());
    let r = resolve(&[&supplier], &q);
    assert_eq!(r.parts.len(), 1);
    assert_eq!(r.parts[0].kind, MatchKind::ExactMpnManufacturer);
    assert_eq!(r.suggestions.len(), 2, "other maker and other MPN");
}

#[test]
fn the_builtin_source_links_definitions_without_claiming_verification() {
    let b = BuiltinSource::new();
    assert_eq!(b.status(), SourceStatus::Ready);
    assert_eq!(b.kind(), SourceKind::Local);
    let key = PartKey::new("Microchip", "ATTINY85-20SU").unwrap();
    let rec = b.fetch(&key).unwrap().expect("builtin has the ATtiny85");
    assert_eq!(rec.definitions.len(), 1);
    let d = &rec.definitions[0];
    assert_eq!(d.maturity, eak_core::libmeta::LibraryMaturity::Draft);
    assert_eq!(d.content_hash.as_ref().map(String::len), Some(64));
    assert!(!b.capabilities().offers);
    let all = b.search(&PartQuery::new("a")).unwrap();
    assert!(
        all.records.iter().all(|p| p.offers.is_empty()),
        "no offer is invented"
    );
}

#[test]
fn the_resolver_caps_each_source_at_the_query_limit_and_says_so() {
    let supplier = counting_supplier(SourceStatus::Ready, Arc::new(AtomicUsize::new(0)));
    let mut q = PartQuery::new("AMS1117-3.3");
    q.limit = 1;
    let r = resolve(&[&supplier], &q);
    assert_eq!(r.parts.len() + r.suggestions.len(), 1);
    let notes: Vec<&String> = r.sources.iter().flat_map(|s| &s.unkeyed).collect();
    assert!(
        notes.iter().any(|n| n.contains("over the limit")),
        "{notes:?}"
    );
}
