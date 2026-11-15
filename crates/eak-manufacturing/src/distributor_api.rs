//! Distributor OpenAPI client (the documented "agent" API, v1 product module) — ADR-0032.
//!
//! One API, one authentication scheme: the header-signed REST API described at
//! <https://distributor.invalid/agent/authentication> and `.../agent/basic-rules` (production host
//! `https://distributor.invalid`, paths `/rest/api/agent/product/v1/...`). Every request carries the
//! headers `key`, `nonce`, `timestamp` (Unix seconds, valid for 60 s) and `signature`, where
//!
//! ```text
//! signature = lowercase_hex(SHA-256(
//!     "key=KEY&nonce=NONCE&secret=SECRET&timestamp=TS" + "&" + sorted_business_params))
//! ```
//!
//! and `sorted_business_params` are the query parameters sorted by key (ASCII ascending),
//! each `key=value` percent-encoded the way Python's `quote_plus` does (space → `+`, only
//! `A-Za-z0-9_.-~` unescaped). The secret never leaves the process: it is not sent, not
//! logged, not part of `Debug` output and not stored in any record.
//!
//! The older `distributor.invalid/rest/wmsc2agent` OpenAPI (key/signature as *query* parameters, no
//! documented signing rule) and the website's `distributor.invalid` endpoints (HTTP 403) are not
//! used and are not mixed in.
//!
//! What is and is not known about the wire format (see ADR-0032): the authentication rule,
//! the envelope (`code`, `message`, `success`, `result`), the pagination rule (`limit` 1–500,
//! `offset` starting at 1), the status/error-code table and the keyword-search parameters
//! (`keyword`, `offset`, `limit`, `returnInformation`, `currency`, `inStockOnly`) are public.
//! The per-product and per-category field names are not retrievable without an API account, so
//! the parsers accept the documented Distributor field-name variants, record what is absent as
//! `missing`, and refuse (never guess) a response shape they do not recognise.
//!
//! Network access is the [`Fetch`] trait, so everything here is tested without a network.

use crate::distributor::{Fetch, HttpResponse};
use crate::provider::{ErrorKind, ProviderError, Served, SupplierProvider};
use crate::sourcing::{PriceBreak, SourcingRecord, Spec, VerificationState};
use eak_validation::sourcing::is_distributor_part_number;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Production host of the documented API.
pub const PRODUCTION_BASE: &str = "https://distributor.invalid";
/// Documented sandbox host (used by tests and by anyone holding sandbox keys).
pub const SANDBOX_BASE: &str = "https://distributor.invalid";
pub const PATH_CATEGORIES: &str = "/rest/api/agent/product/v1/categories";
pub const PATH_CATEGORY_ITEMS: &str = "/rest/api/agent/product/v1/categoricalitemlist";
pub const PATH_KEYWORD_SEARCH: &str = "/rest/api/agent/product/v1/keywordsearch";
pub const PATH_MANUFACTURERS: &str = "/rest/api/agent/product/v1/manufacturers";
/// Documented page-size range.
pub const MAX_LIMIT: u32 = 500;
pub const DEFAULT_LIMIT: u32 = 50;
/// Where a user applies for credentials and whitelists an IP address.
pub const APPLY_URL: &str = "https://distributor.invalid/agent/apply";

const MAX_TEXT: usize = 256;
const MAX_DESCRIPTION: usize = 1024;
const MAX_NODES: usize = 20_000;
const MAX_DEPTH: usize = 6;
const MAX_SPECS: usize = 256;
const MAX_PRICE_BREAKS: usize = 64;

// ---------------------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------------------

/// An API key and secret. `Debug` never shows the secret; there is no `Serialize`.
#[derive(Clone)]
pub struct Credentials {
    key: String,
    secret: String,
}

impl Credentials {
    /// Trim and sanity-check a key/secret pair (non-empty, no whitespace or control
    /// characters, bounded length). The pair is not verified against Distributor here.
    pub fn new(key: &str, secret: &str) -> Result<Credentials, String> {
        let check = |what: &str, v: &str| -> Result<String, String> {
            let v = v.trim();
            if v.is_empty() {
                return Err(format!("the API {what} is empty"));
            }
            if v.len() > 256 || v.chars().any(|c| c.is_control() || c.is_whitespace()) {
                return Err(format!(
                    "the API {what} contains whitespace or control characters, or is too long"
                ));
            }
            Ok(v.to_string())
        };
        Ok(Credentials {
            key: check("key", key)?,
            secret: check("secret", secret)?,
        })
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    /// The last four characters of the key, for "is this the key I meant" display.
    pub fn key_hint(&self) -> String {
        let n = self.key.chars().count();
        self.key.chars().skip(n.saturating_sub(4)).collect()
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Credentials {{ key: …{}, secret: <redacted> }}",
            self.key_hint()
        )
    }
}

/// Where credentials come from (the OS credential store in the desktop app). Read on every
/// call, so saving new credentials takes effect without a restart.
pub trait CredentialSource: Send + Sync {
    /// `Ok(None)` = nothing configured; `Err` = the store itself failed.
    fn credentials(&self) -> Result<Option<Credentials>, String>;
}

/// A fixed in-memory source (tests).
pub struct FixedCredentials(pub Option<Credentials>);

impl CredentialSource for FixedCredentials {
    fn credentials(&self) -> Result<Option<Credentials>, String> {
        Ok(self.0.clone())
    }
}

// ---------------------------------------------------------------------------------------
// Signing
// ---------------------------------------------------------------------------------------

/// Percent-encode like Python's `urllib.parse.quote_plus` (the documented reference).
pub fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b'-' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `k1=v1&k2=v2` with keys sorted ascending and values encoded. `BTreeMap` iterates in byte
/// (= ASCII) order, which is the documented sort.
pub fn canonical_query(params: &BTreeMap<String, String>) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// The exact string that is hashed (documented steps 1–3).
pub fn string_to_sign(
    key: &str,
    nonce: &str,
    secret: &str,
    timestamp: u64,
    params: &BTreeMap<String, String>,
) -> String {
    let base = format!("key={key}&nonce={nonce}&secret={secret}&timestamp={timestamp}");
    if params.is_empty() {
        base
    } else {
        format!("{base}&{}", canonical_query(params))
    }
}

/// Lowercase-hex SHA-256 of [`string_to_sign`] (documented step 4).
pub fn sign(
    key: &str,
    nonce: &str,
    secret: &str,
    timestamp: u64,
    params: &BTreeMap<String, String>,
) -> String {
    format!(
        "{:x}",
        Sha256::digest(string_to_sign(key, nonce, secret, timestamp, params).as_bytes())
    )
}

fn system_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A 16-character lowercase alphanumeric nonce, unique per call (time + counter + pid hashed).
/// Uniqueness is what the API asks for; it is not a secret.
fn random_nonce() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    const ALPHABET: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = Sha256::new();
    h.update(nanos.to_le_bytes());
    h.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    h.update(std::process::id().to_le_bytes());
    let d = h.finalize();
    d.iter()
        .take(16)
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect()
}

// ---------------------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------------------

/// Everything that can go wrong, kept distinct so the UI can say what to do about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum ApiError {
    /// No API key/secret is configured.
    NeedsCredentials,
    /// The credential store itself failed.
    CredentialStore(String),
    /// The request was rejected as malformed (codes 4001/4002) or failed local validation.
    BadRequest(String),
    /// No network path (DNS, TLS, timeout).
    Transport(String),
    /// A gateway/WAF refused the request before the API saw it (HTTP 401/403).
    HttpRefused { status: u16, note: String },
    /// Credentials/signature rejected (code 4003 authentication, 5005).
    Unauthorized { code: i64, message: String },
    /// The request timestamp was outside the 60 s validity window: the local clock is off.
    ClockSkew(String),
    /// Code 5002: this machine's public IP is not on the account's whitelist.
    IpNotAuthorized(String),
    /// Code 5003: the account has no access to this API module.
    NoPermission(String),
    /// HTTP/code 429 (per-minute) or 430 (daily quota).
    RateLimited { daily: bool, message: String },
    /// Code 5004: previous operation still in progress.
    Busy(String),
    /// Resource not found (code 4003 wording) or an empty exact lookup.
    NotFound(String),
    /// Other business error (4003) or an unlisted code.
    Business { code: i64, message: String },
    /// Code 5001.
    Server(String),
    /// A response that is not the documented envelope, or a result shape that is not recognised.
    Unusable(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiError::NeedsCredentials => write!(
                f,
                "Distributor API credentials are not configured. The Distributor catalogue needs an approved \
                 API key and secret ({APPLY_URL}; review takes 5-7 business days) and this \
                 machine's public IP on the account's whitelist. Enter them under Settings → \
                 Distributor. No results are shown rather than inventing any."
            ),
            ApiError::CredentialStore(m) => write!(f, "the credential store failed: {m}"),
            ApiError::BadRequest(m) => write!(f, "Distributor rejected the request as malformed: {m}"),
            ApiError::Transport(m) => write!(f, "could not reach Distributor: {m}"),
            ApiError::HttpRefused { status, note } => write!(
                f,
                "Distributor's gateway refused the request (HTTP {status}{}). This is usually a wrong \
                 host or an IP that is not whitelisted.",
                if note.is_empty() {
                    String::new()
                } else {
                    format!(": {note}")
                }
            ),
            ApiError::Unauthorized { code, message } => write!(
                f,
                "Distributor rejected the credentials (code {code}: {message}). Check the API key and \
                 secret."
            ),
            ApiError::ClockSkew(m) => write!(
                f,
                "Distributor rejected the request timestamp ({m}). Requests are valid for 60 seconds, \
                 so this computer's clock must be accurate."
            ),
            ApiError::IpNotAuthorized(m) => write!(
                f,
                "this machine's IP address is not authorised for the Distributor API (code 5002: {m}). \
                 Add its public IP to the whitelist on your Distributor API account page."
            ),
            ApiError::NoPermission(m) => write!(
                f,
                "this Distributor account has no access to that API (code 5003: {m}). Ask Distributor to \
                 enable the Products module."
            ),
            ApiError::RateLimited { daily, message } => write!(
                f,
                "Distributor rate limit reached ({}; {message}). Nothing was retried; wait before \
                 searching again.",
                if *daily {
                    "daily quota"
                } else {
                    "per-minute quota"
                }
            ),
            ApiError::Busy(m) => {
                write!(f, "Distributor is busy with a previous operation (code 5004: {m})")
            }
            ApiError::NotFound(m) => write!(f, "not found: {m}"),
            ApiError::Business { code, message } => write!(f, "Distributor error {code}: {message}"),
            ApiError::Server(m) => write!(f, "Distributor reported an internal error (code 5001: {m})"),
            ApiError::Unusable(m) => write!(f, "unusable Distributor response: {m}"),
        }
    }
}

impl std::error::Error for ApiError {}

impl ApiError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            ApiError::NeedsCredentials | ApiError::CredentialStore(_) => {
                ErrorKind::NeedsCredentials
            }
            ApiError::BadRequest(_) => ErrorKind::BadQuery,
            ApiError::Transport(_) => ErrorKind::Offline,
            ApiError::HttpRefused { .. }
            | ApiError::IpNotAuthorized(_)
            | ApiError::NoPermission(_) => ErrorKind::Blocked,
            ApiError::Unauthorized { .. } | ApiError::ClockSkew(_) => ErrorKind::Unauthorized,
            ApiError::RateLimited { .. } | ApiError::Busy(_) => ErrorKind::RateLimited,
            ApiError::NotFound(_) => ErrorKind::NotFound,
            ApiError::Business { .. } | ApiError::Server(_) | ApiError::Unusable(_) => {
                ErrorKind::Unusable
            }
        }
    }

    pub fn to_provider_error(&self) -> ProviderError {
        ProviderError {
            provider: "distributor".into(),
            kind: self.kind(),
            message: self.to_string(),
        }
    }
}

/// Map a non-200 envelope `code` and its message (documented table in `basic-rules`).
fn map_code(code: i64, message: &str) -> ApiError {
    let m = message.to_string();
    let lower = message.to_ascii_lowercase();
    match code {
        4001 | 4002 => ApiError::BadRequest(m),
        4003 => {
            if lower.contains("timestamp") || lower.contains("expire") {
                ApiError::ClockSkew(m)
            } else if lower.contains("not found")
                || lower.contains("no product")
                || lower.contains("does not exist")
            {
                ApiError::NotFound(m)
            } else if ["sign", "key", "auth", "secret", "credential"]
                .iter()
                .any(|w| lower.contains(w))
            {
                ApiError::Unauthorized { code, message: m }
            } else {
                ApiError::Business { code, message: m }
            }
        }
        429 => ApiError::RateLimited {
            daily: false,
            message: m,
        },
        430 => ApiError::RateLimited {
            daily: true,
            message: m,
        },
        5001 => ApiError::Server(m),
        5002 => ApiError::IpNotAuthorized(m),
        5003 => ApiError::NoPermission(m),
        5004 => ApiError::Busy(m),
        5005 => ApiError::Unauthorized { code, message: m },
        _ => ApiError::Business { code, message: m },
    }
}

/// Visible text of an HTML/plain body, whitespace-collapsed, cut to 120 characters.
fn strip_tags(body: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in body.chars() {
        match c {
            '<' => {
                in_tag = true;
                text.push(' ');
            }
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect()
}

// ---------------------------------------------------------------------------------------
// Typed results
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CategoryNode {
    pub id: i64,
    pub name: String,
    pub product_count: Option<u64>,
    pub children: Vec<CategoryNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manufacturer {
    pub id: Option<i64>,
    pub name: String,
    pub product_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductPage {
    pub records: Vec<SourcingRecord>,
    /// Total matches, when the response states it.
    pub total: Option<u64>,
    pub page: u32,
    pub limit: u32,
    pub has_more: bool,
    /// `keyword`, `category_id` or `category_name_keyword` (the documented fallback).
    pub via: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchRequest {
    pub keyword: String,
    /// 1-based.
    pub page: u32,
    pub limit: u32,
    pub in_stock_only: bool,
    /// Three-letter code (USD, CNY, EUR, HKD).
    pub currency: String,
}

impl SearchRequest {
    pub fn new(keyword: &str) -> SearchRequest {
        SearchRequest {
            keyword: keyword.trim().to_string(),
            page: 1,
            limit: DEFAULT_LIMIT,
            in_stock_only: false,
            currency: "USD".into(),
        }
    }
}

fn check_paging(page: u32, limit: u32) -> Result<(), ApiError> {
    if page == 0 {
        return Err(ApiError::BadRequest("page numbers start at 1".into()));
    }
    if limit == 0 || limit > MAX_LIMIT {
        return Err(ApiError::BadRequest(format!(
            "page size must be 1..={MAX_LIMIT}"
        )));
    }
    Ok(())
}

fn check_currency(c: &str) -> Result<(), ApiError> {
    if c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(())
    } else {
        Err(ApiError::BadRequest(format!(
            "{c:?} is not a three-letter currency code"
        )))
    }
}

/// Documented: page 1 → offset 1, page 2 → limit + 1; `offset = (page - 1) * limit + 1`.
pub fn offset_for(page: u32, limit: u32) -> u64 {
    u64::from(page.saturating_sub(1)) * u64::from(limit) + 1
}

// ---------------------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------------------

pub struct DistributorApi<F: Fetch> {
    fetch: F,
    base: String,
    creds: Arc<dyn CredentialSource>,
    now: fn() -> u64,
    nonce: fn() -> String,
}

/// A decoded answer: the `result` value and the request URL (business parameters only).
struct Answer {
    result: Value,
    url: String,
}

impl<F: Fetch> DistributorApi<F> {
    pub fn new(fetch: F, base: &str, creds: Arc<dyn CredentialSource>) -> Self {
        DistributorApi {
            fetch,
            base: base.trim_end_matches('/').to_string(),
            creds,
            now: system_now,
            nonce: random_nonce,
        }
    }

    /// Replace the clock and nonce source (tests use fixed values).
    pub fn with_clock(mut self, now: fn() -> u64, nonce: fn() -> String) -> Self {
        self.now = now;
        self.nonce = nonce;
        self
    }

    /// One signed GET. Never logs the secret; the URL holds only business parameters.
    fn call(&self, path: &str, params: BTreeMap<String, String>) -> Result<Answer, ApiError> {
        let creds = self
            .creds
            .credentials()
            .map_err(ApiError::CredentialStore)?
            .ok_or(ApiError::NeedsCredentials)?;
        let timestamp = (self.now)();
        let nonce = (self.nonce)();
        let signature = sign(&creds.key, &nonce, &creds.secret, timestamp, &params);
        let query = canonical_query(&params);
        let url = if query.is_empty() {
            format!("{}{path}", self.base)
        } else {
            format!("{}{path}?{query}", self.base)
        };
        let headers = vec![
            ("key".to_string(), creds.key.clone()),
            ("nonce".to_string(), nonce),
            ("timestamp".to_string(), timestamp.to_string()),
            ("signature".to_string(), signature),
        ];
        let HttpResponse { status, body } = self
            .fetch
            .get_with_headers(&url, &headers)
            .map_err(ApiError::Transport)?;
        match status {
            200 => {}
            429 => {
                return Err(ApiError::RateLimited {
                    daily: false,
                    message: "HTTP 429".into(),
                })
            }
            430 => {
                return Err(ApiError::RateLimited {
                    daily: true,
                    message: "HTTP 430".into(),
                })
            }
            s @ (401 | 403) => {
                return Err(ApiError::HttpRefused {
                    status: s,
                    note: strip_tags(&String::from_utf8_lossy(&body)),
                })
            }
            s => return Err(ApiError::Unusable(format!("HTTP {s}"))),
        }
        let doc: Value = serde_json::from_slice(&body)
            .map_err(|e| ApiError::Unusable(format!("not JSON: {e}")))?;
        let code = doc
            .get("code")
            .and_then(Value::as_i64)
            .ok_or_else(|| ApiError::Unusable("no numeric `code` in the response".into()))?;
        if code != 200 {
            let message = doc.get("message").and_then(Value::as_str).unwrap_or("");
            return Err(map_code(code, message));
        }
        Ok(Answer {
            result: doc.get("result").cloned().unwrap_or(Value::Null),
            url,
        })
    }

    /// Keyword / MPN / Distributor-number / manufacturer / category search, one page.
    pub fn search(&self, req: &SearchRequest) -> Result<ProductPage, ApiError> {
        let kw = req.keyword.trim();
        if kw.is_empty() {
            return Err(ApiError::BadRequest("empty keyword".into()));
        }
        if kw.chars().count() > 120 || kw.chars().any(char::is_control) {
            return Err(ApiError::BadRequest(
                "keyword too long or contains control characters".into(),
            ));
        }
        check_paging(req.page, req.limit)?;
        check_currency(&req.currency)?;
        let mut p = BTreeMap::new();
        p.insert("keyword".into(), kw.to_string());
        p.insert("offset".into(), offset_for(req.page, req.limit).to_string());
        p.insert("limit".into(), req.limit.to_string());
        p.insert("returnInformation".into(), "All".into());
        p.insert("currency".into(), req.currency.clone());
        p.insert("inStockOnly".into(), req.in_stock_only.to_string());
        let a = self.call(PATH_KEYWORD_SEARCH, p)?;
        page_from(&a, req.page, req.limit, &req.currency, "keyword")
    }

    /// One page of a category by its id (`categoricalitemlist`). If Distributor rejects the
    /// parameters (4001/4002) the documented fallback is used: the category *name* as a
    /// keyword, which the Categories API documents as directly searchable; the page says so
    /// in `via`.
    pub fn category_products(
        &self,
        category_id: i64,
        category_name: Option<&str>,
        page: u32,
        limit: u32,
        in_stock_only: bool,
        currency: &str,
    ) -> Result<ProductPage, ApiError> {
        check_paging(page, limit)?;
        check_currency(currency)?;
        let mut p = BTreeMap::new();
        p.insert("categoryId".into(), category_id.to_string());
        p.insert("offset".into(), offset_for(page, limit).to_string());
        p.insert("limit".into(), limit.to_string());
        p.insert("returnInformation".into(), "All".into());
        p.insert("currency".into(), currency.to_string());
        p.insert("inStockOnly".into(), in_stock_only.to_string());
        match self
            .call(PATH_CATEGORY_ITEMS, p)
            .and_then(|a| page_from(&a, page, limit, currency, "category_id"))
        {
            Err(ApiError::BadRequest(why)) => match category_name {
                Some(name) if !name.trim().is_empty() => {
                    let mut r = SearchRequest::new(name);
                    r.page = page;
                    r.limit = limit;
                    r.in_stock_only = in_stock_only;
                    r.currency = currency.to_string();
                    let mut pg = self.search(&r)?;
                    pg.via = format!("category_name_keyword (categoryId was rejected: {why})");
                    Ok(pg)
                }
                _ => Err(ApiError::BadRequest(why)),
            },
            other => other,
        }
    }

    /// The category tree (up to four levels).
    pub fn categories(&self) -> Result<Vec<CategoryNode>, ApiError> {
        let a = self.call(PATH_CATEGORIES, BTreeMap::new())?;
        categories_from(&a.result)
    }

    pub fn manufacturers(&self) -> Result<Vec<Manufacturer>, ApiError> {
        let a = self.call(PATH_MANUFACTURERS, BTreeMap::new())?;
        manufacturers_from(&a.result)
    }

    /// Exact lookup of one Distributor part number: the documented keyword search ("matches Distributor
    /// part #") filtered to the record whose number is exactly `part`.
    pub fn product(&self, part: &str, currency: &str) -> Result<SourcingRecord, ApiError> {
        let code = part.trim().to_ascii_uppercase();
        if !is_distributor_part_number(&code) {
            return Err(ApiError::BadRequest(format!(
                "{part:?} is not an Distributor part number (C followed by digits)"
            )));
        }
        let mut r = SearchRequest::new(&code);
        r.limit = 20;
        r.currency = currency.to_string();
        let page = self.search(&r)?;
        page.records
            .into_iter()
            .find(|rec| rec.distributor_part.as_deref() == Some(code.as_str()))
            .ok_or_else(|| ApiError::NotFound(format!("Distributor has no part {code}")))
    }

    /// The cheapest authenticated call: one record of a keyword search. Success means the
    /// key, secret, signature, clock and IP whitelist were all accepted.
    pub fn test_connection(&self) -> Result<(), ApiError> {
        let mut r = SearchRequest::new("C25804");
        r.limit = 1;
        self.search(&r).map(|_| ())
    }
}

// ---------------------------------------------------------------------------------------
// Response parsing (tolerant of documented name variants, strict about shape)
// ---------------------------------------------------------------------------------------

fn pick<'a>(o: &'a Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    names
        .iter()
        .find_map(|n| o.get(*n).filter(|v| !v.is_null()))
}

fn bounded(s: &str, max: usize) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t.chars().count() > max || t.chars().any(char::is_control) {
        None
    } else {
        Some(t.to_string())
    }
}

/// A text value, or the `name`-like member of an object value.
fn text_of(v: &Value, max: usize) -> Option<String> {
    match v {
        Value::String(s) => bounded(s, max),
        Value::Object(o) => pick(
            o,
            &[
                "name",
                "nameEn",
                "brandNameEn",
                "brandName",
                "categoryName",
                "pdf",
                "url",
            ],
        )
        .and_then(|x| text_of(x, max)),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn text_field(o: &Map<String, Value>, names: &[&str], max: usize) -> Option<String> {
    pick(o, names).and_then(|v| text_of(v, max))
}

fn u64_of(v: &Value) -> Option<u64> {
    match v {
        Value::Number(n) => n.as_u64().or_else(|| {
            n.as_f64()
                .filter(|f| f.is_finite() && *f >= 0.0 && f.fract() == 0.0)
                .map(|f| f as u64)
        }),
        Value::String(s) => s.trim().replace(',', "").parse().ok(),
        _ => None,
    }
}

fn i64_of(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

const QTY: &[&str] = &[
    "startNumber",
    "minQty",
    "min_qty",
    "ladder",
    "quantity",
    "qty",
    "startQty",
];
const PRICE: &[&str] = &["productPrice", "unitPrice", "price", "usdPrice"];
const COUNT: &[&str] = &[
    "productCount",
    "product_quantity",
    "productQuantity",
    "count",
    "total",
];

fn price_break(v: &Value, currency: &str) -> Result<PriceBreak, String> {
    let o = v.as_object().ok_or("not an object")?;
    let min_quantity = pick(o, QTY)
        .and_then(u64_of)
        .filter(|q| *q >= 1)
        .ok_or("no positive minimum quantity")?;
    let unit_price = match pick(o, PRICE).ok_or("no unit price")? {
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            let t = s.trim();
            if t.parse::<f64>().is_ok_and(|p| p.is_finite() && p >= 0.0) {
                t.to_string()
            } else {
                return Err("unit price is not a non-negative number".into());
            }
        }
        _ => return Err("unit price is not a number".into()),
    };
    let cur =
        text_field(o, &["currency", "currencyCode"], 8).unwrap_or_else(|| currency.to_string());
    Ok(PriceBreak {
        min_quantity,
        unit_price,
        currency: cur,
    })
}

fn specs_of(v: &Value, rejected: &mut Vec<String>) -> Vec<Spec> {
    let mut out = vec![];
    match v {
        Value::Array(items) => {
            for (i, it) in items.iter().take(MAX_SPECS).enumerate() {
                let spec = it.as_object().and_then(|o| {
                    Some(Spec {
                        name: text_field(
                            o,
                            &["paramNameEn", "name", "paramName", "attributeName"],
                            MAX_TEXT,
                        )?,
                        value: text_field(
                            o,
                            &["paramValueEn", "value", "paramValue", "attributeValue"],
                            MAX_TEXT,
                        )?,
                    })
                });
                match spec {
                    Some(s) => out.push(s),
                    None => rejected.push(format!("attributes[{i}]: no name/value pair")),
                }
            }
        }
        Value::Object(m) => {
            for (k, val) in m.iter().take(MAX_SPECS) {
                match (bounded(k, MAX_TEXT), text_of(val, MAX_TEXT)) {
                    (Some(name), Some(value)) => out.push(Spec { name, value }),
                    _ => rejected.push(format!("attributes.{k}: not a text value")),
                }
            }
        }
        _ => rejected.push("attributes: not an array or object".into()),
    }
    out
}

fn category_text(o: &Map<String, Value>) -> Option<String> {
    let parent = text_field(o, &["parentCategoryName", "parentCatalogName"], MAX_TEXT);
    let own = text_field(o, &["categoryName", "catalogName"], MAX_TEXT).or_else(|| {
        pick(o, &["category"]).and_then(|c| match c {
            Value::Object(co) => {
                let a = text_field(co, &["name1"], MAX_TEXT);
                let b = text_field(co, &["name2"], MAX_TEXT);
                match (a, b) {
                    (Some(a), Some(b)) => Some(format!("{a} / {b}")),
                    (a, b) => a.or(b),
                }
            }
            other => text_of(other, MAX_TEXT),
        })
    });
    match (parent, own) {
        (Some(a), Some(b)) => Some(format!("{a} / {b}")),
        (a, b) => a.or(b),
    }
}

/// Build a sourcing record from one product object. The hash is of the product object as
/// parsed (the response carries many products, so per-record raw slices do not exist); the
/// record stays `SearchHit` — a supplier listing is never proof of suitability.
fn record_from_product(
    product: &Value,
    source_url: &str,
    currency: &str,
    now: u64,
) -> Option<SourcingRecord> {
    let o = product.as_object()?;
    let mut missing = vec![];
    let mut rejected = vec![];

    let distributor_raw = text_field(
        o,
        &[
            "productCode",
            "distributorPartNumber",
            "productNumber",
            "number",
            "distributorNumber",
        ],
        32,
    )
    .map(|s| s.to_ascii_uppercase());
    let distributor_part = match distributor_raw {
        Some(s) if is_distributor_part_number(&s) => Some(s),
        Some(_) => {
            rejected.push("distributor part number: not C followed by digits".to_string());
            None
        }
        None => None,
    };
    if distributor_part.is_none() {
        missing.push("distributor_part".to_string());
    }
    let mut field = |names: &[&str], what: &str, max: usize| {
        let v = text_field(o, names, max);
        if v.is_none() {
            missing.push(what.to_string());
        }
        v
    };
    let mpn = field(
        &["productModel", "mpn", "manufacturerPartNumber", "model"],
        "mpn",
        MAX_TEXT,
    );
    let manufacturer = field(
        &[
            "brandNameEn",
            "brandName",
            "manufacturer",
            "manufacturerName",
            "brand",
        ],
        "manufacturer",
        MAX_TEXT,
    );
    let package = field(
        &["encapStandard", "package", "packageName"],
        "package",
        MAX_TEXT,
    );
    let datasheet_url = pick(o, &["pdfUrl", "datasheetUrl", "datasheet"])
        .and_then(|v| text_of(v, 2048))
        .filter(|u| u.starts_with("https://") && !u.chars().any(char::is_whitespace));
    if datasheet_url.is_none() {
        missing.push("datasheet_url".into());
    }
    let description = text_field(
        o,
        &[
            "productIntroEn",
            "description",
            "productDescription",
            "title",
        ],
        MAX_DESCRIPTION,
    );
    let stock = pick(
        o,
        &["stockNumber", "stock", "quantity", "inventory", "stockQty"],
    )
    .and_then(u64_of);

    let mut price_breaks = vec![];
    if let Some(Value::Array(items)) = pick(o, &["productPriceList", "prices", "priceList"]) {
        for (i, it) in items.iter().take(MAX_PRICE_BREAKS).enumerate() {
            match price_break(it, currency) {
                Ok(b) => price_breaks.push(b),
                Err(why) => rejected.push(format!("price[{i}]: {why}")),
            }
        }
    }
    let specs = match pick(o, &["paramVOList", "attributes", "parameters", "paramList"]) {
        Some(v) => specs_of(v, &mut rejected),
        None => vec![],
    };
    if specs.is_empty() {
        missing.push("specs".into());
    }

    let canonical = serde_json::to_vec(product).ok()?;
    Some(SourcingRecord {
        provider: "Distributor".into(),
        source_url: source_url.to_string(),
        retrieved_at_unix_s: now,
        raw_sha256: format!("{:x}", Sha256::digest(&canonical)),
        raw_bytes: canonical.len() as u64,
        distributor_part,
        mpn,
        manufacturer,
        package,
        category: category_text(o),
        description,
        datasheet_url,
        specs,
        stock,
        price_breaks,
        missing,
        rejected,
        datasheet: None,
        attestation: None,
        state: VerificationState::SearchHit,
    })
}

const LIST_KEYS: &[&str] = &[
    "products",
    "productList",
    "product_list",
    "list",
    "records",
    "items",
    "data",
];

/// The list inside a `result`, or a clear refusal when the shape is not recognised.
fn list_of<'a>(result: &'a Value, keys: &[&str], what: &str) -> Result<&'a [Value], ApiError> {
    match result {
        Value::Null => Ok(&[]),
        Value::Array(a) => Ok(a),
        Value::Object(o) => {
            if let Some(Value::Array(a)) = pick(o, keys) {
                return Ok(a);
            }
            if o.values()
                .all(|v| v.is_null() || v == &Value::Array(vec![]))
            {
                return Ok(&[]);
            }
            let mut names: Vec<&str> = o.keys().map(String::as_str).take(12).collect();
            names.sort_unstable();
            Err(ApiError::Unusable(format!(
                "unrecognised {what} shape (result keys: {})",
                names.join(", ")
            )))
        }
        _ => Err(ApiError::Unusable(format!(
            "{what} is not a list or object"
        ))),
    }
}

fn page_from(
    a: &Answer,
    page: u32,
    limit: u32,
    currency: &str,
    via: &str,
) -> Result<ProductPage, ApiError> {
    let items = list_of(&a.result, LIST_KEYS, "product list")?;
    let now = system_now();
    let mut records = vec![];
    for it in items {
        let r = record_from_product(it, &a.url, currency, now)
            .ok_or_else(|| ApiError::Unusable("a product entry is not an object".into()))?;
        records.push(r);
    }
    let total = a
        .result
        .as_object()
        .and_then(|o| pick(o, COUNT))
        .and_then(u64_of);
    let has_more = match total {
        Some(t) => u64::from(page) * u64::from(limit) < t,
        None => records.len() as u64 >= u64::from(limit),
    };
    Ok(ProductPage {
        records,
        total,
        page,
        limit,
        has_more,
        via: via.to_string(),
    })
}

fn node_from(v: &Value, depth: usize, budget: &mut usize) -> Option<CategoryNode> {
    if depth > MAX_DEPTH || *budget == 0 {
        return None;
    }
    *budget -= 1;
    let o = v.as_object()?;
    let id = pick(o, &["categoryId", "id", "category_id"]).and_then(i64_of)?;
    let name = text_field(o, &["categoryName", "name", "category_name"], MAX_TEXT)?;
    let product_count = pick(o, COUNT).and_then(u64_of);
    let children = match pick(
        o,
        &[
            "children",
            "subCategories",
            "childList",
            "childCategories",
            "child",
        ],
    ) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|c| node_from(c, depth + 1, budget))
            .collect(),
        _ => vec![],
    };
    Some(CategoryNode {
        id,
        name,
        product_count,
        children,
    })
}

fn categories_from(result: &Value) -> Result<Vec<CategoryNode>, ApiError> {
    let items = list_of(
        result,
        &["categories", "categoryList", "list", "data", "children"],
        "category tree",
    )?;
    let mut budget = MAX_NODES;
    let nodes: Vec<CategoryNode> = items
        .iter()
        .filter_map(|c| node_from(c, 0, &mut budget))
        .collect();
    if nodes.is_empty() && !items.is_empty() {
        return Err(ApiError::Unusable(
            "category entries carry no recognisable id and name".into(),
        ));
    }
    Ok(nodes)
}

fn manufacturers_from(result: &Value) -> Result<Vec<Manufacturer>, ApiError> {
    let items = list_of(
        result,
        &["manufacturers", "brands", "brandList", "list", "data"],
        "manufacturer list",
    )?;
    let out: Vec<Manufacturer> = items
        .iter()
        .take(MAX_NODES)
        .filter_map(|v| {
            let o = v.as_object()?;
            Some(Manufacturer {
                id: pick(o, &["brandId", "manufacturerId", "id"]).and_then(i64_of),
                name: text_field(
                    o,
                    &["brandNameEn", "brandName", "manufacturerName", "name"],
                    MAX_TEXT,
                )?,
                product_count: pick(o, COUNT).and_then(u64_of),
            })
        })
        .collect();
    if out.is_empty() && !items.is_empty() {
        return Err(ApiError::Unusable(
            "manufacturer entries carry no recognisable name".into(),
        ));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------
// SupplierProvider adapter (the existing Resilient lookup/import path)
// ---------------------------------------------------------------------------------------

impl<F: Fetch> SupplierProvider for DistributorApi<F> {
    fn name(&self) -> &str {
        "distributor"
    }
    fn lookup(&self, part: &str) -> Result<SourcingRecord, ProviderError> {
        self.product(part, "USD").map_err(|e| e.to_provider_error())
    }
    fn search(&self, query: &str) -> Result<Vec<SourcingRecord>, ProviderError> {
        DistributorApi::search(self, &SearchRequest::new(query))
            .map(|p| p.records)
            .map_err(|e| e.to_provider_error())
    }
}

// ---------------------------------------------------------------------------------------
// Catalogue service: bounded cache, pacing, retry, stale labelling
// ---------------------------------------------------------------------------------------

/// A typed answer with how it was obtained and when Distributor produced it.
#[derive(Debug, Clone, Serialize)]
pub struct Fetched<T> {
    pub served: Served,
    /// Unix seconds at which the data was retrieved from Distributor (not when it was served).
    pub fetched_at_unix_s: u64,
    pub data: T,
}

struct Entry {
    at: Instant,
    at_unix: u64,
    value: Value,
}

struct CacheState {
    last_call: Option<Instant>,
    entries: HashMap<String, Entry>,
    requests: u64,
}

const MAX_ENTRIES: usize = 128;
pub const TTL_TREE: Duration = Duration::from_secs(24 * 3600);
pub const TTL_PAGE: Duration = Duration::from_secs(15 * 60);

/// Lazy, bounded, polite access to the catalogue: nothing is downloaded until asked for,
/// at most [`MAX_ENTRIES`] answers are kept, calls are spaced `min_interval` apart (far inside
/// the documented 200 calls/minute), only transport failures are retried, and a rate-limit
/// answer is never retried.
pub struct Catalogue<F: Fetch> {
    api: DistributorApi<F>,
    pub min_interval: Duration,
    pub max_attempts: u32,
    pub base_backoff: Duration,
    sleep: fn(Duration),
    state: Mutex<CacheState>,
}

impl<F: Fetch> Catalogue<F> {
    pub fn new(api: DistributorApi<F>) -> Self {
        Catalogue {
            api,
            min_interval: Duration::from_millis(350),
            max_attempts: 2,
            base_backoff: Duration::from_millis(500),
            sleep: std::thread::sleep,
            state: Mutex::new(CacheState {
                last_call: None,
                entries: HashMap::new(),
                requests: 0,
            }),
        }
    }

    pub fn with_sleep(mut self, sleep: fn(Duration)) -> Self {
        self.sleep = sleep;
        self
    }

    pub fn api(&self) -> &DistributorApi<F> {
        &self.api
    }

    /// Forget every cached answer (credentials changed, or the user asked for a refresh).
    pub fn clear(&self) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.entries.clear();
    }

    /// Requests that reached Distributor in this process (for the quota display).
    pub fn requests_made(&self) -> u64 {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .requests
    }

    fn pace(&self) {
        let wait = {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let wait = match s.last_call {
                Some(t) => self
                    .min_interval
                    .saturating_sub(now.saturating_duration_since(t)),
                None => Duration::ZERO,
            };
            s.last_call = Some(now + wait);
            wait
        };
        if !wait.is_zero() {
            (self.sleep)(wait);
        }
    }

    /// Serve `key` from cache when fresh, else call Distributor (retrying transport failures), else
    /// fall back to an older cached answer, labelled stale, for outages and rate limits.
    pub fn cached<T: Serialize + DeserializeOwned>(
        &self,
        key: &str,
        ttl: Duration,
        refresh: bool,
        f: impl Fn(&DistributorApi<F>) -> Result<T, ApiError>,
    ) -> Result<Fetched<T>, ApiError> {
        let cached = {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.entries
                .get(key)
                .map(|e| (e.at.elapsed(), e.at_unix, e.value.clone()))
        };
        let decode = |v: &Value| serde_json::from_value::<T>(v.clone()).ok();
        if !refresh {
            if let Some((age, at_unix, v)) = &cached {
                if *age < ttl {
                    if let Some(data) = decode(v) {
                        return Ok(Fetched {
                            served: Served::Cache {
                                age_s: age.as_secs(),
                            },
                            fetched_at_unix_s: *at_unix,
                            data,
                        });
                    }
                }
            }
        }
        let attempts = self.max_attempts.max(1);
        let mut backoff = self.base_backoff;
        let mut last = None;
        for attempt in 0..attempts {
            self.pace();
            {
                let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                s.requests += 1;
            }
            match f(&self.api) {
                Ok(data) => {
                    let now_unix = system_now();
                    if let Ok(value) = serde_json::to_value(&data) {
                        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                        if s.entries.len() >= MAX_ENTRIES && !s.entries.contains_key(key) {
                            let oldest = s
                                .entries
                                .iter()
                                .min_by_key(|(_, e)| e.at)
                                .map(|(k, _)| k.clone());
                            if let Some(oldest) = oldest {
                                s.entries.remove(&oldest);
                            }
                        }
                        s.entries.insert(
                            key.to_string(),
                            Entry {
                                at: Instant::now(),
                                at_unix: now_unix,
                                value,
                            },
                        );
                    }
                    return Ok(Fetched {
                        served: Served::Live,
                        fetched_at_unix_s: now_unix,
                        data,
                    });
                }
                Err(e @ ApiError::Transport(_)) if attempt + 1 < attempts => {
                    (self.sleep)(backoff);
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                    last = Some(e);
                }
                Err(e) => {
                    last = Some(e);
                    break;
                }
            }
        }
        let err = last.expect("at least one attempt ran");
        let kind = err.kind();
        if matches!(
            kind,
            ErrorKind::Offline | ErrorKind::Blocked | ErrorKind::RateLimited
        ) {
            if let Some((age, at_unix, v)) = cached {
                if let Some(data) = decode(&v) {
                    return Ok(Fetched {
                        served: Served::StaleCache {
                            age_s: age.as_secs(),
                            because: kind,
                        },
                        fetched_at_unix_s: at_unix,
                        data,
                    });
                }
            }
        }
        Err(err)
    }

    pub fn categories(&self, refresh: bool) -> Result<Fetched<Vec<CategoryNode>>, ApiError> {
        self.cached("categories", TTL_TREE, refresh, |a| a.categories())
    }

    pub fn manufacturers(&self, refresh: bool) -> Result<Fetched<Vec<Manufacturer>>, ApiError> {
        self.cached("manufacturers", TTL_TREE, refresh, |a| a.manufacturers())
    }

    pub fn search(
        &self,
        req: &SearchRequest,
        refresh: bool,
    ) -> Result<Fetched<ProductPage>, ApiError> {
        let key = format!(
            "search|{}|{}|{}|{}|{}",
            req.keyword.to_lowercase(),
            req.page,
            req.limit,
            req.in_stock_only,
            req.currency
        );
        self.cached(&key, TTL_PAGE, refresh, |a| a.search(req))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn category_products(
        &self,
        category_id: i64,
        category_name: Option<&str>,
        page: u32,
        limit: u32,
        in_stock_only: bool,
        currency: &str,
        refresh: bool,
    ) -> Result<Fetched<ProductPage>, ApiError> {
        let key = format!("cat|{category_id}|{page}|{limit}|{in_stock_only}|{currency}");
        self.cached(&key, TTL_PAGE, refresh, |a| {
            a.category_products(
                category_id,
                category_name,
                page,
                limit,
                in_stock_only,
                currency,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    // Test vectors: derived from the documented rule with an independent SHA-256
    // (`printf '%s' "$string" | sha256sum`), not copied from Distributor.
    const BASE: &str = "key=testkey&nonce=0123456789abcdef&secret=testsecret&timestamp=1700000000";

    fn params(kv: &[(&str, &str)]) -> BTreeMap<String, String> {
        kv.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn signing_vector_without_business_parameters() {
        assert_eq!(
            string_to_sign(
                "testkey",
                "0123456789abcdef",
                "testsecret",
                1_700_000_000,
                &params(&[])
            ),
            BASE
        );
        assert_eq!(
            sign(
                "testkey",
                "0123456789abcdef",
                "testsecret",
                1_700_000_000,
                &params(&[])
            ),
            "552911d82170dcad55f1fdf9f13246f1a1aae6510cf9b390e8fa5f6886819061"
        );
    }

    #[test]
    fn signing_vector_sorts_keys_and_uses_plus_for_spaces() {
        let p = params(&[
            ("keyword", "STM32F103 C8T6"),
            ("offset", "1"),
            ("limit", "10"),
            ("returnInformation", "All"),
            ("currency", "USD"),
            ("inStockOnly", "false"),
        ]);
        assert_eq!(
            string_to_sign("testkey", "0123456789abcdef", "testsecret", 1_700_000_000, &p),
            format!(
                "{BASE}&currency=USD&inStockOnly=false&keyword=STM32F103+C8T6&limit=10&offset=1&returnInformation=All"
            )
        );
        assert_eq!(
            sign(
                "testkey",
                "0123456789abcdef",
                "testsecret",
                1_700_000_000,
                &p
            ),
            "8fd51c8acca1cc29ffc20e0ec785b1ac6525a7e8828798371c5a5a61974aadd6"
        );
    }

    #[test]
    fn signing_vector_percent_encodes_utf8_and_reserved_characters() {
        let p = params(&[("keyword", "10kΩ/0603 1%")]);
        assert_eq!(
            string_to_sign(
                "testkey",
                "0123456789abcdef",
                "testsecret",
                1_700_000_000,
                &p
            ),
            format!("{BASE}&keyword=10k%CE%A9%2F0603+1%25")
        );
        assert_eq!(
            sign(
                "testkey",
                "0123456789abcdef",
                "testsecret",
                1_700_000_000,
                &p
            ),
            "b7ed5a2c9ecf4f51db33e8a1df4a85e1507f8bd94577b33ec82ff820c470b1d2"
        );
    }

    #[test]
    fn unreserved_characters_are_not_escaped() {
        assert_eq!(encode_component("a-b_c.d~E9"), "a-b_c.d~E9");
        assert_eq!(encode_component("a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn offsets_follow_the_documented_rule() {
        assert_eq!(offset_for(1, 100), 1);
        assert_eq!(offset_for(2, 100), 101);
        assert_eq!(offset_for(3, 50), 101);
    }

    #[test]
    fn debug_output_never_contains_the_secret() {
        let c = Credentials::new("AKEY1234", "s3cr3t-value").unwrap();
        let shown = format!("{c:?}");
        assert!(!shown.contains("s3cr3t"));
        assert!(shown.contains("1234"));
        assert!(Credentials::new("", "x").is_err());
        assert!(Credentials::new("a b", "x").is_err());
    }

    #[test]
    fn nonce_is_sixteen_lowercase_alphanumerics_and_varies() {
        let a = random_nonce();
        let b = random_nonce();
        assert_eq!(a.len(), 16);
        assert!(a
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        assert_ne!(a, b);
    }

    type Reply = Result<HttpResponse, String>;
    type Seen = Vec<(String, Vec<(String, String)>)>;

    /// Scripted transport: records every request, answers from a queue.
    struct Script {
        seen: StdMutex<Seen>,
        replies: StdMutex<Vec<Reply>>,
    }

    impl Script {
        fn new(replies: Vec<Reply>) -> Arc<Script> {
            Arc::new(Script {
                seen: StdMutex::new(vec![]),
                replies: StdMutex::new(replies),
            })
        }
    }

    struct Shared(Arc<Script>);

    impl Fetch for Shared {
        fn get(&self, _url: &str) -> Result<HttpResponse, String> {
            Err("headers required".into())
        }
        fn get_with_headers(
            &self,
            url: &str,
            headers: &[(String, String)],
        ) -> Result<HttpResponse, String> {
            self.0
                .seen
                .lock()
                .unwrap()
                .push((url.to_string(), headers.to_vec()));
            let mut q = self.0.replies.lock().unwrap();
            if q.is_empty() {
                Err("no scripted reply left".into())
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

    fn api(script: &Arc<Script>, creds: bool) -> DistributorApi<Shared> {
        let c = creds.then(|| Credentials::new("testkey", "testsecret").unwrap());
        DistributorApi::new(
            Shared(script.clone()),
            "https://api.example.invalid",
            Arc::new(FixedCredentials(c)),
        )
        .with_clock(|| 1_700_000_000, || "0123456789abcdef".to_string())
    }

    fn header<'a>(h: &'a [(String, String)], k: &str) -> &'a str {
        h.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
            .unwrap()
    }

    const PRODUCTS: &str = r#"{"code":200,"message":"ok","success":true,"result":{"productCount":3,"products":[
      {"productCode":"C25804","productModel":"0603WAF1002T5E","brandNameEn":"UNI-ROYAL(Uniroyal Elec)",
       "encapStandard":"0603","catalogName":"Chip Resistor - Surface Mount","parentCatalogName":"Resistors",
       "stockNumber":1200000,"pdfUrl":"https://distributor.invalid/x.pdf",
       "productPriceList":[{"startNumber":1,"productPrice":0.0123}],
       "paramVOList":[{"paramNameEn":"Resistance","paramValueEn":"10kΩ"}]},
      {"productCode":"C99999","productModel":"0603WAF1002T5E","brandNameEn":"OTHER CO","encapStandard":"0603","stockNumber":0}
    ]}}"#;

    #[test]
    fn a_search_sends_signed_headers_and_documented_parameters() {
        let s = Script::new(vec![ok(PRODUCTS)]);
        let a = api(&s, true);
        let mut r = SearchRequest::new("0603WAF1002T5E");
        r.limit = 2;
        r.page = 2;
        r.in_stock_only = true;
        let page = a.search(&r).unwrap();
        assert_eq!(page.records.len(), 2);
        assert_eq!(page.total, Some(3));
        assert!(!page.has_more, "2 pages of 2 cover 3 results");
        let seen = s.seen.lock().unwrap();
        let (url, headers) = &seen[0];
        assert!(
            url.starts_with("https://api.example.invalid/rest/api/agent/product/v1/keywordsearch?")
        );
        assert!(url.contains("keyword=0603WAF1002T5E"));
        assert!(url.contains("offset=3"), "{url}");
        assert!(url.contains("inStockOnly=true") && url.contains("limit=2"));
        assert!(
            !url.contains("testsecret"),
            "the secret is never in the URL"
        );
        assert_eq!(header(headers, "key"), "testkey");
        assert_eq!(header(headers, "nonce"), "0123456789abcdef");
        assert_eq!(header(headers, "timestamp"), "1700000000");
        let expect = sign(
            "testkey",
            "0123456789abcdef",
            "testsecret",
            1_700_000_000,
            &params(&[
                ("keyword", "0603WAF1002T5E"),
                ("offset", "3"),
                ("limit", "2"),
                ("returnInformation", "All"),
                ("currency", "USD"),
                ("inStockOnly", "true"),
            ]),
        );
        assert_eq!(header(headers, "signature"), expect);
        assert!(headers.iter().all(|(_, v)| !v.contains("testsecret")));
    }

    #[test]
    fn products_parse_with_provenance_and_stay_search_hits() {
        let s = Script::new(vec![ok(PRODUCTS)]);
        let page = api(&s, true).search(&SearchRequest::new("x")).unwrap();
        let r = &page.records[0];
        assert_eq!(r.distributor_part.as_deref(), Some("C25804"));
        assert_eq!(r.mpn.as_deref(), Some("0603WAF1002T5E"));
        assert_eq!(r.package.as_deref(), Some("0603"));
        assert_eq!(
            r.category.as_deref(),
            Some("Resistors / Chip Resistor - Surface Mount")
        );
        assert_eq!(r.stock, Some(1_200_000));
        assert_eq!(r.price_breaks[0].unit_price, "0.0123");
        assert_eq!(r.price_breaks[0].currency, "USD");
        assert_eq!(r.specs[0].name, "Resistance");
        assert_eq!(r.state, VerificationState::SearchHit);
        assert_ne!(page.records[0].raw_sha256, page.records[1].raw_sha256);
        // The second product lacks fields: recorded as missing, not invented.
        assert!(page.records[1]
            .missing
            .contains(&"datasheet_url".to_string()));
        assert!(page.records[1].price_breaks.is_empty());
    }

    #[test]
    fn missing_credentials_never_reach_the_network() {
        let s = Script::new(vec![]);
        let e = api(&s, false)
            .search(&SearchRequest::new("C25804"))
            .unwrap_err();
        assert_eq!(e, ApiError::NeedsCredentials);
        assert!(s.seen.lock().unwrap().is_empty());
        assert!(e.to_string().contains("API key and secret"));
    }

    fn envelope(code: i64, msg: &str) -> Reply {
        ok(&format!(
            r#"{{"code":{code},"message":"{msg}","success":false,"result":null}}"#
        ))
    }

    fn status(code: u16, body: &str) -> Reply {
        Ok(HttpResponse {
            status: code,
            body: body.as_bytes().to_vec(),
        })
    }

    fn expect_error(case: &str, reply: Reply, is_expected: impl Fn(&ApiError) -> bool) {
        let s = Script::new(vec![reply]);
        let e = api(&s, true).search(&SearchRequest::new("C1")).unwrap_err();
        assert!(is_expected(&e), "{case}: {e:?}");
        assert!(!e.to_string().contains("testsecret"), "{case}");
    }

    #[test]
    fn documented_error_codes_map_to_distinct_actionable_errors() {
        expect_error("4001", envelope(4001, "keyword is required."), |e| {
            matches!(e, ApiError::BadRequest(_))
        });
        expect_error("4002", envelope(4002, "bad limit"), |e| {
            matches!(e, ApiError::BadRequest(_))
        });
        expect_error(
            "4003 auth",
            envelope(4003, "Authentication failed: invalid signature"),
            |e| matches!(e, ApiError::Unauthorized { .. }),
        );
        expect_error("4003 clock", envelope(4003, "timestamp expired"), |e| {
            matches!(e, ApiError::ClockSkew(_))
        });
        expect_error("4003 missing", envelope(4003, "product not found"), |e| {
            matches!(e, ApiError::NotFound(_))
        });
        expect_error("5001", envelope(5001, "oops"), |e| {
            matches!(e, ApiError::Server(_))
        });
        expect_error("5002", envelope(5002, "1.2.3.4"), |e| {
            matches!(e, ApiError::IpNotAuthorized(_))
        });
        expect_error("5003", envelope(5003, "no module"), |e| {
            matches!(e, ApiError::NoPermission(_))
        });
        expect_error("5004", envelope(5004, "busy"), |e| {
            matches!(e, ApiError::Busy(_))
        });
        expect_error("5005", envelope(5005, "decrypt"), |e| {
            matches!(e, ApiError::Unauthorized { code: 5005, .. })
        });
        expect_error("429 envelope", envelope(429, "slow down"), |e| {
            matches!(e, ApiError::RateLimited { daily: false, .. })
        });
        expect_error("430 envelope", envelope(430, "tomorrow"), |e| {
            matches!(e, ApiError::RateLimited { daily: true, .. })
        });
        expect_error("429 http", status(429, ""), |e| {
            matches!(e, ApiError::RateLimited { daily: false, .. })
        });
        expect_error("430 http", status(430, ""), |e| {
            matches!(e, ApiError::RateLimited { daily: true, .. })
        });
        expect_error(
            "403 gateway",
            status(403, "<h1>403 ERROR</h1><p>Request blocked.</p>"),
            |e| {
                matches!(e, ApiError::HttpRefused { status: 403, note }
                    if note.contains("Request blocked"))
            },
        );
        expect_error("transport", Err("dns failure".into()), |e| {
            matches!(e, ApiError::Transport(_))
        });
        expect_error("not json", ok("<html>not json</html>"), |e| {
            matches!(e, ApiError::Unusable(_))
        });
        expect_error("no code", ok(r#"{"result":[]}"#), |e| {
            matches!(e, ApiError::Unusable(_))
        });
    }

    #[test]
    fn empty_results_are_empty_not_errors() {
        for body in [
            r#"{"code":200,"message":"ok","success":true,"result":{"productCount":0,"products":[]}}"#,
            r#"{"code":200,"message":"ok","success":true,"result":null}"#,
        ] {
            let s = Script::new(vec![ok(body)]);
            let p = api(&s, true).search(&SearchRequest::new("zzz")).unwrap();
            assert!(p.records.is_empty());
            assert!(!p.has_more);
        }
    }

    #[test]
    fn an_unrecognised_result_shape_is_refused_not_guessed() {
        let s = Script::new(vec![ok(
            r#"{"code":200,"success":true,"result":{"surprise":{"a":1}}}"#,
        )]);
        let e = api(&s, true).search(&SearchRequest::new("x")).unwrap_err();
        assert!(
            matches!(&e, ApiError::Unusable(m) if m.contains("surprise")),
            "{e:?}"
        );
    }

    #[test]
    fn paging_is_validated_before_any_request() {
        let s = Script::new(vec![]);
        let a = api(&s, true);
        let mut r = SearchRequest::new("x");
        r.limit = 501;
        assert!(matches!(a.search(&r), Err(ApiError::BadRequest(_))));
        r.limit = 10;
        r.page = 0;
        assert!(matches!(a.search(&r), Err(ApiError::BadRequest(_))));
        r.page = 1;
        r.currency = "usd".into();
        assert!(matches!(a.search(&r), Err(ApiError::BadRequest(_))));
        assert!(matches!(
            a.search(&SearchRequest::new("  ")),
            Err(ApiError::BadRequest(_))
        ));
        assert!(s.seen.lock().unwrap().is_empty());
    }

    #[test]
    fn product_lookup_returns_only_the_exact_distributor_number() {
        let s = Script::new(vec![ok(PRODUCTS)]);
        let r = api(&s, true).product("c25804", "USD").unwrap();
        assert_eq!(r.distributor_part.as_deref(), Some("C25804"));
        let s2 = Script::new(vec![ok(PRODUCTS)]);
        assert!(matches!(
            api(&s2, true).product("C1", "USD"),
            Err(ApiError::NotFound(_))
        ));
        assert!(matches!(
            api(&s2, true).product("R10", "USD"),
            Err(ApiError::BadRequest(_))
        ));
    }

    const TREE: &str = r#"{"code":200,"success":true,"result":[
      {"categoryId":1,"categoryName":"Resistors","productCount":10,"children":[
         {"categoryId":11,"categoryName":"Chip Resistor - Surface Mount","productCount":7,"children":[]}]},
      {"categoryId":2,"categoryName":"Capacitors","productCount":5}]}"#;

    #[test]
    fn the_category_tree_parses_with_nesting() {
        let s = Script::new(vec![ok(TREE)]);
        let t = api(&s, true).categories().unwrap();
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].children[0].id, 11);
        assert_eq!(t[0].children[0].name, "Chip Resistor - Surface Mount");
        assert_eq!(t[1].product_count, Some(5));
    }

    #[test]
    fn manufacturers_parse_and_keep_distinct_entries() {
        let s = Script::new(vec![ok(
            r#"{"code":200,"success":true,"result":[{"brandId":7,"brandNameEn":"TI","productCount":9},{"brandId":8,"brandNameEn":"Texas Instruments"}]}"#,
        )]);
        let m = api(&s, true).manufacturers().unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].id, Some(7));
        assert_eq!(m[1].product_count, None);
    }

    #[test]
    fn a_rejected_category_id_falls_back_to_the_documented_name_keyword() {
        let s = Script::new(vec![envelope(4002, "categoryId invalid"), ok(PRODUCTS)]);
        let p = api(&s, true)
            .category_products(
                11,
                Some("Chip Resistor - Surface Mount"),
                1,
                50,
                false,
                "USD",
            )
            .unwrap();
        assert!(p.via.starts_with("category_name_keyword"), "{}", p.via);
        let seen = s.seen.lock().unwrap();
        assert!(seen[0].0.contains("categoricalitemlist?"));
        assert!(seen[1].0.contains("keywordsearch?"));
        assert!(seen[1].0.contains("keyword=Chip+Resistor+-+Surface+Mount"));
    }

    fn catalogue(script: &Arc<Script>) -> Catalogue<Shared> {
        Catalogue::new(api(script, true)).with_sleep(|_| {})
    }

    #[test]
    fn the_second_identical_request_is_served_from_cache() {
        let s = Script::new(vec![ok(TREE)]);
        let c = catalogue(&s);
        assert_eq!(c.categories(false).unwrap().served, Served::Live);
        assert!(matches!(
            c.categories(false).unwrap().served,
            Served::Cache { .. }
        ));
        assert_eq!(s.seen.lock().unwrap().len(), 1);
        assert_eq!(c.requests_made(), 1);
    }

    #[test]
    fn transport_failures_are_retried_then_succeed() {
        let s = Script::new(vec![Err("timeout".into()), ok(TREE)]);
        let c = catalogue(&s);
        assert_eq!(c.categories(false).unwrap().served, Served::Live);
        assert_eq!(s.seen.lock().unwrap().len(), 2);
    }

    #[test]
    fn rate_limits_and_credential_errors_are_not_retried() {
        let s = Script::new(vec![envelope(429, "x"), ok(TREE)]);
        let c = catalogue(&s);
        assert!(matches!(
            c.categories(false),
            Err(ApiError::RateLimited { .. })
        ));
        assert_eq!(s.seen.lock().unwrap().len(), 1);
        let s2 = Script::new(vec![envelope(4003, "invalid signature"), ok(TREE)]);
        assert!(matches!(
            catalogue(&s2).categories(false),
            Err(ApiError::Unauthorized { .. })
        ));
        assert_eq!(s2.seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_outage_serves_the_older_answer_labelled_stale() {
        let s = Script::new(vec![ok(TREE), Err("down".into()), Err("down".into())]);
        let c = catalogue(&s);
        c.categories(false).unwrap();
        let again = c.categories(true).unwrap();
        assert!(matches!(
            again.served,
            Served::StaleCache {
                because: ErrorKind::Offline,
                ..
            }
        ));
        assert_eq!(again.data.len(), 2);
    }

    #[test]
    fn clearing_the_cache_forgets_everything() {
        let s = Script::new(vec![ok(TREE), ok(TREE)]);
        let c = catalogue(&s);
        c.categories(false).unwrap();
        c.clear();
        assert_eq!(c.categories(false).unwrap().served, Served::Live);
    }

    #[test]
    fn the_cache_is_bounded() {
        let replies = (0..(MAX_ENTRIES + 5)).map(|_| ok(PRODUCTS)).collect();
        let s = Script::new(replies);
        let c = catalogue(&s);
        for i in 0..(MAX_ENTRIES + 5) {
            c.search(&SearchRequest::new(&format!("q{i}")), false)
                .unwrap();
        }
        assert!(c.state.lock().unwrap().entries.len() <= MAX_ENTRIES);
    }
}
