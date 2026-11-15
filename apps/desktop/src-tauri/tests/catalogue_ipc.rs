//! LCSC catalogue settings and browsing through the desktop command layer, headless. The LCSC
//! API is replaced by a scripted transport; the network is never contacted and no real
//! credential store is touched.

use eak_desktop::catalogue::{
    categories, search, test_connection, CatalogueQuery, Settings, StoredCredentials,
};
use eak_desktop::models::{KeyStore, MemoryKeyStore};
use eak_manufacturing::lcsc::{Fetch, HttpResponse};
use eak_manufacturing::lcsc_api::{Catalogue, CredentialSource, LcscApi, PRODUCTION_BASE};
use std::sync::{Arc, Mutex};

const SECRET: &str = "very-secret-value-123";

type Reply = Result<HttpResponse, String>;

struct Canned {
    replies: Mutex<Vec<Reply>>,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Fetch for Canned {
    fn get(&self, _url: &str) -> Result<HttpResponse, String> {
        Err("headers required".into())
    }
    fn get_with_headers(
        &self,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<HttpResponse, String> {
        // The secret must never be on the wire in the clear.
        assert!(!url.contains(SECRET));
        assert!(headers.iter().all(|(_, v)| !v.contains(SECRET)));
        self.seen.lock().unwrap().push(url.to_string());
        let mut q = self.replies.lock().unwrap();
        if q.is_empty() {
            Err("no scripted reply".into())
        } else {
            q.remove(0)
        }
    }
}

fn ok(body: &str) -> Reply {
    Ok(HttpResponse {
        status: 200,
        body: body.as_bytes().to_vec(),
    })
}

const PRODUCTS: &str = r#"{"code":200,"message":"ok","success":true,"result":{"productCount":120,"products":[
  {"productCode":"C25804","productModel":"0603WAF1002T5E","brandNameEn":"UNI-ROYAL","encapStandard":"0603","stockNumber":5},
  {"productCode":"C77777","productModel":"0603WAF1002T5E","brandNameEn":"OTHER CO","encapStandard":"0603","stockNumber":9},
  {"productCode":"C88888","productModel":"RC0603FR-0710KL","brandNameEn":"YAGEO","encapStandard":"0805","stockNumber":1}
]}}"#;

struct Rig {
    settings: Settings,
    cat: Catalogue<Canned>,
    seen: Arc<Mutex<Vec<String>>>,
    keys: Arc<MemoryKeyStore>,
}

fn rig(replies: Vec<Reply>, file: Option<std::path::PathBuf>) -> Rig {
    let keys = Arc::new(MemoryKeyStore::default());
    let settings = Settings::new(keys.clone(), file);
    let seen = Arc::new(Mutex::new(vec![]));
    let api = LcscApi::new(
        Canned {
            replies: Mutex::new(replies),
            seen: seen.clone(),
        },
        PRODUCTION_BASE,
        settings.source(),
    );
    Rig {
        settings,
        cat: Catalogue::new(api).with_sleep(|_| {}),
        seen,
        keys,
    }
}

fn query(k: &str) -> CatalogueQuery {
    CatalogueQuery {
        keyword: Some(k.into()),
        ..CatalogueQuery::default()
    }
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("eak-lcsc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn without_credentials_the_reply_says_what_is_needed_and_carries_no_rows() {
    let r = rig(vec![ok(PRODUCTS)], None);
    let reply = search(&r.cat, &query("C25804"));
    assert_eq!(reply.status, "needs_credentials");
    assert!(reply.data.is_none());
    assert!(reply.message.contains("API key and secret"));
    assert!(reply.message.contains("whitelist"));
    assert!(
        r.seen.lock().unwrap().is_empty(),
        "no request without credentials"
    );
    let view = r.settings.view(0);
    assert!(!view.has_credentials);
    assert!(view.needs.is_some());
    assert_eq!(view.status.state, "untested");
}

#[test]
fn saved_credentials_live_only_in_the_key_store_and_never_in_the_view_or_status_file() {
    let file = tmp("status").join("lcsc-connection.json");
    let r = rig(vec![ok(PRODUCTS)], Some(file.clone()));
    r.settings.save("KEY-ABCD1234", SECRET).unwrap();
    assert_eq!(r.keys.get("api-secret").unwrap().as_deref(), Some(SECRET));

    let view = r.settings.view(3);
    assert!(view.has_credentials);
    assert_eq!(view.key_hint.as_deref(), Some("1234"));
    let shown = serde_json::to_string(&view).unwrap();
    assert!(!shown.contains(SECRET) && !shown.contains("KEY-ABCD1234"));

    let reply = search(&r.cat, &query("C25804"));
    assert_eq!(reply.status, "ok");
    let written = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(!written.contains(SECRET));
    let reply_json = serde_json::to_string(&reply).unwrap();
    assert!(!reply_json.contains(SECRET));

    assert!(r.settings.save("", SECRET).is_err());
    assert!(r.settings.save("key", "has space").is_err());

    r.settings.clear().unwrap();
    assert!(!r.settings.view(0).has_credentials);
    assert_eq!(r.keys.get("api-secret").unwrap(), None);
}

#[test]
fn a_connection_test_records_success_and_failure_with_the_reason() {
    let file = tmp("test").join("lcsc-connection.json");
    let bad = ok(
        r#"{"code":4003,"message":"Authentication failed: invalid signature","success":false,"result":null}"#,
    );
    let r = rig(vec![ok(PRODUCTS), bad], Some(file.clone()));
    r.settings.save("KEY-ABCD1234", SECRET).unwrap();

    let v = test_connection(&r.settings, &r.cat);
    assert_eq!(v.status.state, "ok");
    assert!(v.status.last_success_unix_s.is_some());

    let v2 = test_connection(&r.settings, &r.cat);
    assert_eq!(v2.status.state, "failed");
    assert!(v2.status.message.contains("rejected the credentials"));
    assert!(
        v2.status.last_success_unix_s.is_some(),
        "the earlier success is kept"
    );

    // The status survives a restart; the secret is not in it.
    let again = Settings::new(r.keys.clone(), Some(file.clone()));
    assert_eq!(again.view(0).status.state, "failed");
    assert!(!std::fs::read_to_string(&file).unwrap().contains(SECRET));
}

#[test]
fn search_ranks_exact_matches_first_and_reports_why() {
    let r = rig(vec![ok(PRODUCTS)], None);
    r.settings.save("KEY-ABCD1234", SECRET).unwrap();
    let reply = search(&r.cat, &query("0603WAF1002T5E"));
    let data = reply.data.expect("results");
    assert_eq!(data.total, Some(120));
    assert!(data.has_more);
    assert_eq!(data.candidates.len(), 3);
    // Same MPN text from two manufacturers: two separate exact candidates, neither merged,
    // both ahead of the unrelated listing.
    assert!(data.candidates[0].exact && data.candidates[1].exact);
    let mut exact: Vec<_> = data.candidates[..2]
        .iter()
        .map(|c| c.record.lcsc_part.clone().unwrap())
        .collect();
    exact.sort();
    assert_eq!(exact, ["C25804", "C77777"]);
    assert!(data.candidates[0]
        .reasons
        .iter()
        .any(|r| r.contains("no manufacturer given")));
    assert!(
        !data.candidates[2].exact,
        "the unrelated listing is a fuzzy suggestion"
    );
    assert!(data
        .candidates
        .iter()
        .all(|c| c.engineering_status == "supplier_listing_only"));
}

#[test]
fn manufacturer_and_package_filters_apply_to_the_loaded_page_and_are_counted() {
    let r = rig(vec![ok(PRODUCTS)], None);
    r.settings.save("KEY-ABCD1234", SECRET).unwrap();
    let mut q = query("0603WAF1002T5E");
    q.manufacturer = Some("uni royal".into());
    q.package = Some("0603".into());
    let data = search(&r.cat, &q).data.expect("results");
    assert_eq!(data.candidates.len(), 1);
    assert_eq!(
        data.candidates[0].kind,
        eak_manufacturing::matching::MatchKind::ExactMpnManufacturer
    );
    assert_eq!(data.filtered_out, 2);
}

#[test]
fn paging_arguments_reach_lcsc_using_the_documented_offset_rule() {
    let r = rig(vec![ok(PRODUCTS)], None);
    r.settings.save("KEY-ABCD1234", SECRET).unwrap();
    let mut q = query("resistor");
    q.page = Some(3);
    q.limit = Some(100);
    let _ = search(&r.cat, &q);
    let url = r.seen.lock().unwrap()[0].clone();
    assert!(
        url.contains("offset=201") && url.contains("limit=100"),
        "{url}"
    );
}

#[test]
fn an_empty_query_is_a_bad_query_not_a_request() {
    let r = rig(vec![ok(PRODUCTS)], None);
    r.settings.save("KEY-ABCD1234", SECRET).unwrap();
    let reply = search(&r.cat, &CatalogueQuery::default());
    assert_eq!(reply.status, "bad_query");
    assert!(r.seen.lock().unwrap().is_empty());
}

#[test]
fn an_outage_after_a_good_answer_is_shown_as_stale_never_as_live() {
    let r = rig(
        vec![
            ok(
                r#"{"code":200,"success":true,"result":[{"categoryId":1,"categoryName":"Resistors","productCount":4}]}"#,
            ),
            Err("down".into()),
            Err("down".into()),
        ],
        None,
    );
    r.settings.save("KEY-ABCD1234", SECRET).unwrap();
    let first = categories(&r.cat, false);
    assert_eq!(first.status, "ok");
    let again = categories(&r.cat, true);
    assert_eq!(again.status, "stale");
    assert!(again.message.contains("earlier answer"));
    assert_eq!(again.data.unwrap()[0].name, "Resistors");
}

#[test]
fn the_credential_source_returns_nothing_until_both_halves_exist() {
    let keys: Arc<dyn KeyStore> = Arc::new(MemoryKeyStore::default());
    let src = StoredCredentials(keys.clone());
    assert!(src.credentials().unwrap().is_none());
    keys.set("api-key", "K").unwrap();
    assert!(src.credentials().unwrap().is_none());
    keys.set("api-secret", "S").unwrap();
    assert!(src.credentials().unwrap().is_some());
}
