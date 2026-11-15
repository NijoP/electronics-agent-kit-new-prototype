//! Distributor catalogue browsing and API-credential settings for the desktop shell (ADR-0032).
//!
//! * Credentials (API key and secret) live **only** in the operating-system credential store
//!   (service `eak-desktop.distributor`), never in a file, the IR, a log, a search record or the
//!   frontend; the UI only ever sees `has_credentials` and the last four characters of the key.
//! * The non-secret connection status (last test result and time, last success) is plain JSON
//!   next to the model-provider settings.
//! * Browsing is lazy and bounded (`eak_manufacturing::distributor_api::Catalogue`): the category tree
//!   and manufacturer list are fetched when first opened and cached; a page of products is
//!   fetched only when asked for. Nothing is crawled in the background.
//! * Every reply says how it was obtained (`live`, `cache`, `stale_cache`) and when Distributor produced
//!   the data. Without credentials the reply says exactly what is needed and carries no rows.
//!
//! Plain functions over a [`Catalogue`] and [`Settings`], tested headless; [`cmd`] holds the
//! thin Tauri wrappers.

use crate::models::KeyStore;
use eak_core::ComponentDef;
use eak_manufacturing::distributor::{Fetch, UreqFetch};
use eak_manufacturing::distributor_api::{
    ApiError, Catalogue, CategoryNode, CredentialSource, Credentials, DistributorApi, Fetched,
    Manufacturer, SearchRequest, APPLY_URL, DEFAULT_LIMIT, MAX_LIMIT, PRODUCTION_BASE,
};
use eak_manufacturing::library_link::{link_record, Link, Scope};
use eak_manufacturing::matching::{self, Candidate, Hints};
use eak_manufacturing::provider::{ErrorKind, Served};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const KEYRING_SERVICE: &str = "eak-desktop.distributor";
const ID_KEY: &str = "api-key";
const ID_SECRET: &str = "api-secret";

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------
// Credential storage
// ---------------------------------------------------------------------------------------

/// The operating-system credential store under the Distributor service name.
pub struct DistributorKeyStore;

fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, id).map_err(|e| format!("credential store: {e}"))
}

impl KeyStore for DistributorKeyStore {
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        entry(id)?
            .set_password(secret)
            .map_err(|e| format!("credential store refused the value: {e}"))
    }
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        match entry(id)?.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("credential store unavailable: {e}")),
        }
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("credential store unavailable: {e}")),
        }
    }
}

/// Reads the key/secret pair from a [`KeyStore`] on every call.
pub struct StoredCredentials(pub Arc<dyn KeyStore>);

impl CredentialSource for StoredCredentials {
    fn credentials(&self) -> Result<Option<Credentials>, String> {
        let key = self.0.get(ID_KEY)?;
        let secret = self.0.get(ID_SECRET)?;
        match (key, secret) {
            (Some(k), Some(s)) if !k.trim().is_empty() && !s.trim().is_empty() => {
                Credentials::new(&k, &s).map(Some)
            }
            _ => Ok(None),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Settings and connection status
// ---------------------------------------------------------------------------------------

/// Non-secret connection facts, persisted as JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StatusFile {
    /// `untested`, `ok` or `failed`.
    pub state: String,
    pub message: String,
    pub tested_at_unix_s: Option<u64>,
    pub last_success_unix_s: Option<u64>,
}

impl Default for StatusFile {
    fn default() -> Self {
        StatusFile {
            state: "untested".into(),
            message: String::new(),
            tested_at_unix_s: None,
            last_success_unix_s: None,
        }
    }
}

/// What the settings panel shows. Never contains the secret or the full key.
#[derive(Debug, Clone, Serialize)]
pub struct DistributorView {
    pub has_credentials: bool,
    /// Last four characters of the key.
    pub key_hint: Option<String>,
    pub store_error: Option<String>,
    pub status: StatusFile,
    /// Requests that reached Distributor in this session (documented quotas are per account).
    pub requests_this_session: u64,
    pub apply_url: &'static str,
    pub host: &'static str,
    /// What the user must do, when credentials are missing.
    pub needs: Option<String>,
}

pub struct Settings {
    keys: Arc<dyn KeyStore>,
    file: Option<PathBuf>,
    status: Mutex<StatusFile>,
}

impl Settings {
    pub fn new(keys: Arc<dyn KeyStore>, file: Option<PathBuf>) -> Settings {
        let status = file
            .as_deref()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .and_then(|t| serde_json::from_str::<StatusFile>(&t).ok())
            .unwrap_or_default();
        Settings {
            keys,
            file,
            status: Mutex::new(status),
        }
    }

    pub fn source(&self) -> Arc<dyn CredentialSource> {
        Arc::new(StoredCredentials(self.keys.clone()))
    }

    fn persist(&self, s: &StatusFile) {
        let Some(f) = &self.file else { return };
        if let Some(dir) = f.parent() {
            if std::fs::create_dir_all(dir).is_err() {
                return;
            }
        }
        if let Ok(text) = serde_json::to_string_pretty(s) {
            let tmp = f.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, f);
            }
        }
    }

    fn set_status(&self, f: impl FnOnce(&mut StatusFile)) {
        let mut g = self.status.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut g);
        self.persist(&g);
    }

    pub fn view(&self, requests: u64) -> DistributorView {
        let (creds, store_error) = match self.source().credentials() {
            Ok(c) => (c, None),
            Err(e) => (None, Some(e)),
        };
        let has_credentials = creds.is_some();
        DistributorView {
            has_credentials,
            key_hint: creds.as_ref().map(Credentials::key_hint),
            store_error,
            status: self
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            requests_this_session: requests,
            apply_url: APPLY_URL,
            host: PRODUCTION_BASE,
            needs: (!has_credentials).then(|| ApiError::NeedsCredentials.to_string()),
        }
    }

    /// Store a new pair (validated for shape only), reset the status to untested.
    pub fn save(&self, key: &str, secret: &str) -> Result<(), String> {
        let c = Credentials::new(key, secret)?;
        self.keys.set(ID_KEY, c.key())?;
        // `Credentials::new` trimmed the pair; store the secret trimmed the same way.
        self.keys.set(ID_SECRET, secret.trim())?;
        self.set_status(|s| *s = StatusFile::default());
        Ok(())
    }

    pub fn clear(&self) -> Result<(), String> {
        self.keys.delete(ID_KEY)?;
        self.keys.delete(ID_SECRET)?;
        self.set_status(|s| *s = StatusFile::default());
        Ok(())
    }

    fn record_test(&self, result: &Result<(), ApiError>) {
        let now = now_s();
        self.set_status(|s| {
            s.tested_at_unix_s = Some(now);
            match result {
                Ok(()) => {
                    s.state = "ok".into();
                    s.message = "Distributor accepted the credentials".into();
                    s.last_success_unix_s = Some(now);
                }
                Err(e) => {
                    s.state = "failed".into();
                    s.message = e.to_string();
                }
            }
        });
    }
}

/// Run the authentication test (one small signed search) and record the outcome.
pub fn test_connection<F: Fetch>(settings: &Settings, cat: &Catalogue<F>) -> DistributorView {
    let result = cat.api().test_connection();
    settings.record_test(&result);
    settings.view(cat.requests_made())
}

// ---------------------------------------------------------------------------------------
// Replies
// ---------------------------------------------------------------------------------------

/// One answer to the UI: a status word, a message, how it was served, and the data (only on
/// success). Failures never carry rows.
#[derive(Debug, Clone, Serialize)]
pub struct Reply<T> {
    /// `ok`, `stale`, `needs_credentials`, `unauthorized`, `rate_limited`, `blocked`,
    /// `not_found`, `unreachable`, `bad_query` or `error`.
    pub status: &'static str,
    pub message: String,
    pub served: Option<Served>,
    pub fetched_at_unix_s: Option<u64>,
    pub data: Option<T>,
}

pub fn status_word(k: ErrorKind) -> &'static str {
    crate::parts::live_status(k)
}

pub fn reply<T>(r: Result<Fetched<T>, ApiError>) -> Reply<T> {
    match r {
        Ok(f) => {
            let (status, message) = match f.served {
                Served::StaleCache { age_s, because } => (
                    "stale",
                    format!(
                        "Distributor is not answering ({because:?}); showing an earlier answer from \
                         {age_s} s ago. Stock and prices may have changed."
                    ),
                ),
                _ => ("ok", String::new()),
            };
            Reply {
                status,
                message,
                served: Some(f.served),
                fetched_at_unix_s: Some(f.fetched_at_unix_s),
                data: Some(f.data),
            }
        }
        Err(e) => Reply {
            status: status_word(e.kind()),
            message: e.to_string(),
            served: None,
            fetched_at_unix_s: None,
            data: None,
        },
    }
}

// ---------------------------------------------------------------------------------------
// Catalogue operations
// ---------------------------------------------------------------------------------------

pub fn categories<F: Fetch>(cat: &Catalogue<F>, refresh: bool) -> Reply<Vec<CategoryNode>> {
    reply(cat.categories(refresh))
}

pub fn manufacturers<F: Fetch>(cat: &Catalogue<F>, refresh: bool) -> Reply<Vec<Manufacturer>> {
    reply(cat.manufacturers(refresh))
}

/// A browse/search request from the UI. `manufacturer` and `package` are hard filters applied
/// to the loaded page (Distributor's documented search has no such parameters) and also drive the
/// ranking hints; `category` is a hint only.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CatalogueQuery {
    /// An Distributor number, an MPN or free text. Empty with `category_id` = browse a category.
    pub keyword: Option<String>,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    /// 1-based.
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub in_stock_only: Option<bool>,
    pub currency: Option<String>,
    pub manufacturer: Option<String>,
    pub package: Option<String>,
    pub category: Option<String>,
    pub refresh: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Results {
    /// Ranked, each with the reasons it matched; exact results come before fuzzy ones.
    pub candidates: Vec<Candidate>,
    /// Total matches Distributor reports for the whole query (all pages), when stated.
    pub total: Option<u64>,
    pub page: u32,
    pub limit: u32,
    pub has_more: bool,
    /// How the page was selected (`keyword`, `category_id`, or the documented name fallback).
    pub via: String,
    /// Rows of this page removed by the manufacturer/package filters.
    pub filtered_out: usize,
    /// What each listing is relative to the libraries, by Distributor number (existing part, supplier-only,
    /// needs a new symbol/footprint). A listing is never rejected for lacking a definition.
    pub links: std::collections::BTreeMap<String, Link>,
}

fn clean(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
}

pub fn search<F: Fetch>(cat: &Catalogue<F>, q: &CatalogueQuery) -> Reply<Results> {
    search_linked(cat, q, &[])
}

/// [`search`], with each listing classified against `defs` (every library scope).
pub fn search_linked<F: Fetch>(
    cat: &Catalogue<F>,
    q: &CatalogueQuery,
    defs: &[(Scope, &ComponentDef)],
) -> Reply<Results> {
    let page = q.page.unwrap_or(1);
    let limit = q.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let in_stock = q.in_stock_only.unwrap_or(false);
    let currency = clean(&q.currency).unwrap_or_else(|| "USD".into());
    let refresh = q.refresh.unwrap_or(false);
    let keyword = clean(&q.keyword);
    let fetched = match (&keyword, q.category_id) {
        (Some(k), _) => {
            let mut req = SearchRequest::new(k);
            req.page = page;
            req.limit = limit;
            req.in_stock_only = in_stock;
            req.currency = currency;
            cat.search(&req, refresh)
        }
        (None, Some(id)) => cat.category_products(
            id,
            clean(&q.category_name).as_deref(),
            page,
            limit,
            in_stock,
            &currency,
            refresh,
        ),
        (None, None) => Err(ApiError::BadRequest(
            "enter an Distributor part number, an MPN or a keyword, or choose a category".into(),
        )),
    };
    reply(fetched.map(|f| {
        let hints = Hints {
            manufacturer: clean(&q.manufacturer),
            package: clean(&q.package),
            category: clean(&q.category),
        };
        let before = f.data.records.len();
        let maker = hints
            .manufacturer
            .as_deref()
            .map(matching::normalize_manufacturer);
        let pkg = hints.package.as_deref().map(|p| p.to_lowercase());
        let kept: Vec<_> = f
            .data
            .records
            .into_iter()
            .filter(|r| {
                let maker_ok = match &maker {
                    None => true,
                    Some(m) => r
                        .manufacturer
                        .as_deref()
                        .is_some_and(|rm| &matching::normalize_manufacturer(rm) == m),
                };
                let package_ok = match &pkg {
                    None => true,
                    Some(p) => r
                        .package
                        .as_deref()
                        .is_some_and(|rp| rp.trim().to_lowercase() == *p),
                };
                maker_ok && package_ok
            })
            .collect();
        let filtered_out = before - kept.len();
        let query = keyword.clone().unwrap_or_default();
        let links = kept
            .iter()
            .filter_map(|r| {
                r.distributor_part
                    .clone()
                    .map(|l| (l, link_record(r, defs)))
            })
            .collect();
        Fetched {
            served: f.served,
            fetched_at_unix_s: f.fetched_at_unix_s,
            data: Results {
                candidates: matching::rank(&query, &hints, kept),
                total: f.data.total,
                page: f.data.page,
                limit: f.data.limit,
                has_more: f.data.has_more,
                via: f.data.via,
                filtered_out,
                links,
            },
        }
    }))
}

// ---------------------------------------------------------------------------------------
// Process-wide wiring
// ---------------------------------------------------------------------------------------

pub struct Distributor {
    pub settings: Settings,
    pub catalogue: Catalogue<UreqFetch>,
}

static STATUS_FILE: OnceLock<PathBuf> = OnceLock::new();
static Distributor: OnceLock<Distributor> = OnceLock::new();

/// Set where the non-secret status JSON lives. Call once at start-up, before first use.
pub fn init(dir: &Path) {
    let _ = STATUS_FILE.set(dir.join("distributor-connection.json"));
}

pub fn distributor() -> &'static Distributor {
    Distributor.get_or_init(|| {
        let settings = Settings::new(Arc::new(DistributorKeyStore), STATUS_FILE.get().cloned());
        let api = DistributorApi::new(UreqFetch::default(), PRODUCTION_BASE, settings.source());
        Distributor {
            settings,
            catalogue: Catalogue::new(api),
        }
    })
}

/// A second client sharing the same credential source, for the single-part lookup/import path.
pub fn lookup_api() -> DistributorApi<UreqFetch> {
    DistributorApi::new(
        UreqFetch::default(),
        PRODUCTION_BASE,
        distributor().settings.source(),
    )
}

/// Tauri command wrappers: argument decoding only. Network and credential-store work runs on
/// the blocking pool at supplier-refresh priority.
pub mod cmd {
    use super::*;
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use tauri::AppHandle;

    fn view() -> DistributorView {
        let l = distributor();
        l.settings.view(l.catalogue.requests_made())
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn distributor_settings_get(app: AppHandle) -> Result<DistributorView, String> {
        blocking(app, Priority::P4, |_| Ok(view())).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn distributor_settings_save(
        app: AppHandle,
        key: String,
        secret: String,
    ) -> Result<DistributorView, String> {
        blocking(app, Priority::P4, move |_| {
            let l = distributor();
            l.settings.save(&key, &secret)?;
            l.catalogue.clear();
            Ok(view())
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn distributor_settings_clear(app: AppHandle) -> Result<DistributorView, String> {
        blocking(app, Priority::P4, |_| {
            let l = distributor();
            l.settings.clear()?;
            l.catalogue.clear();
            Ok(view())
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn distributor_test_connection(app: AppHandle) -> Result<DistributorView, String> {
        blocking(app, Priority::P4, |_| {
            let l = distributor();
            Ok(test_connection(&l.settings, &l.catalogue))
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn catalogue_categories(
        app: AppHandle,
        refresh: bool,
    ) -> Result<Reply<Vec<CategoryNode>>, String> {
        blocking(app, Priority::P4, move |_| {
            Ok(categories(&distributor().catalogue, refresh))
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn catalogue_manufacturers(
        app: AppHandle,
        refresh: bool,
    ) -> Result<Reply<Vec<Manufacturer>>, String> {
        blocking(app, Priority::P4, move |_| {
            Ok(manufacturers(&distributor().catalogue, refresh))
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn catalogue_search(
        app: AppHandle,
        query: CatalogueQuery,
    ) -> Result<Reply<Results>, String> {
        blocking(app, Priority::P4, move |s| {
            let defs = crate::libstore::all_defs(
                crate::libstore::store(),
                &crate::libstore::project_library(s),
            );
            let refs: Vec<_> = defs.iter().map(|(sc, d)| (sc.clone(), d)).collect();
            Ok(search_linked(&distributor().catalogue, &query, &refs))
        })
        .await
    }
}
