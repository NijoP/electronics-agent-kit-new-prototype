//! Distributor provider: read-only part lookup and import into the EAK library.
//!
//! * [`DistributorProvider::lookup`] fetches one public product-detail document by Distributor part number
//!   and turns it into a [`SourcingRecord`] through the audited
//!   [`ingest_product_detail`] boundary (bounded, untrusted-input handling, SHA-256 of the
//!   raw bytes, search-hit state).
//! * [`DistributorProvider::search`] resolves an Distributor part number directly. A keyword query is sent
//!   to Distributor's public search endpoint; if Distributor refuses automated access (it currently
//!   answers HTTP 403 "Access Denied") the failure is returned as [`DistributorError::Blocked`]
//!   and is never replaced by invented results.
//! * [`import_plan`] maps a record onto a symbol + footprint template that already exists in
//!   the library, for submission through `Command::UpsertComponentDefinition`. A part with no
//!   matching template is refused with a reason; no footprint is ever guessed.
//!
//! Network access is a [`Fetch`] implementation, so the whole module is tested against a
//! local test HTTP server. Nothing here sends design data anywhere: requests carry only the
//! part number or keyword the user typed.

use crate::sourcing::{ingest_product_detail, SourcingError, SourcingRecord};
use eak_core::{builtin, ComponentDef};
use eak_validation::sourcing::is_distributor_part_number;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Distributor's public catalogue host (the endpoints used are the ones its own web shop calls).
pub const DEFAULT_BASE: &str = "https://distributor.invalid";
/// Most hits a keyword search returns (each hit costs one detail request).
pub const MAX_HITS: usize = 10;
#[cfg(feature = "live-http")]
const MAX_BODY: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// One blocking HTTP GET. `Err` is a transport failure (DNS, TLS, timeout, oversize).
pub trait Fetch: Send + Sync {
    fn get(&self, url: &str) -> Result<HttpResponse, String>;
    /// GET with extra request headers (the signed Distributor OpenAPI client needs them). An
    /// implementation that cannot send headers must fail rather than drop them silently.
    fn get_with_headers(
        &self,
        _url: &str,
        _headers: &[(String, String)],
    ) -> Result<HttpResponse, String> {
        Err("this transport cannot send request headers".into())
    }
}

/// HTTPS client (rustls) with a global timeout and a bounded body.
#[cfg(feature = "live-http")]
pub struct UreqFetch {
    pub timeout: Duration,
}

#[cfg(feature = "live-http")]
impl Default for UreqFetch {
    fn default() -> Self {
        UreqFetch {
            timeout: Duration::from_secs(12),
        }
    }
}

#[cfg(feature = "live-http")]
impl Fetch for UreqFetch {
    fn get(&self, url: &str) -> Result<HttpResponse, String> {
        self.get_with_headers(url, &[])
    }

    fn get_with_headers(
        &self,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<HttpResponse, String> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .http_status_as_error(false)
            .user_agent(concat!(
                "eak-desktop/",
                env!("CARGO_PKG_VERSION"),
                " (read-only part lookup)"
            ))
            .build()
            .into();
        let mut req = agent.get(url);
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let mut resp = req.call().map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_vec()
            .map_err(|e| e.to_string())?;
        Ok(HttpResponse { status, body })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum DistributorError {
    /// The query is empty or unusable.
    BadQuery(String),
    /// No network path to Distributor (DNS, TLS, timeout).
    Transport(String),
    /// Distributor refused automated access (HTTP 401/403/429). Reported as-is; no fallback data.
    Blocked { status: u16, note: String },
    /// Distributor answered, but with no such part.
    NotFound(String),
    /// Distributor answered with something that is not a usable product document.
    Unusable(String),
}

impl std::fmt::Display for DistributorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistributorError::BadQuery(m) => write!(f, "bad query: {m}"),
            DistributorError::Transport(m) => write!(f, "could not reach Distributor: {m}"),
            DistributorError::Blocked { status, note } => write!(
                f,
                "Distributor refused automated access (HTTP {status}{}). No results are shown \
                 rather than inventing any.",
                if note.is_empty() {
                    String::new()
                } else {
                    format!(": {note}")
                }
            ),
            DistributorError::NotFound(m) => write!(f, "not found: {m}"),
            DistributorError::Unusable(m) => write!(f, "unusable Distributor response: {m}"),
        }
    }
}

impl std::error::Error for DistributorError {}

pub struct DistributorProvider<F: Fetch> {
    fetch: F,
    base: String,
    /// Pause between the detail requests of one keyword search.
    pub pause: Duration,
}

#[cfg(feature = "live-http")]
impl DistributorProvider<UreqFetch> {
    pub fn live() -> Self {
        DistributorProvider::new(UreqFetch::default(), DEFAULT_BASE)
    }
}

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Percent-encode everything but unreserved characters.
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl<F: Fetch> DistributorProvider<F> {
    /// `base` is the host serving `/ftps/wm/...` (no trailing slash). Provenance URLs always
    /// name [`DEFAULT_BASE`], the host the records are about, even when a test server
    /// stands in for it.
    pub fn new(fetch: F, base: &str) -> Self {
        DistributorProvider {
            fetch,
            base: base.trim_end_matches('/').to_string(),
            pause: Duration::from_millis(250),
        }
    }

    fn get_checked(&self, url: &str) -> Result<Vec<u8>, DistributorError> {
        let r = self.fetch.get(url).map_err(DistributorError::Transport)?;
        match r.status {
            200 => Ok(r.body),
            s @ (401 | 403 | 429) => Err(DistributorError::Blocked {
                status: s,
                note: strip_tags(&String::from_utf8_lossy(&r.body)),
            }),
            404 => Err(DistributorError::NotFound(url.to_string())),
            s => Err(DistributorError::Unusable(format!("HTTP {s}"))),
        }
    }

    /// Fetch one product by Distributor part number (`C` + digits).
    pub fn lookup(&self, part: &str) -> Result<SourcingRecord, DistributorError> {
        let code = part.trim().to_ascii_uppercase();
        if !is_distributor_part_number(&code) {
            return Err(DistributorError::BadQuery(format!(
                "{part:?} is not an Distributor part number (C followed by digits)"
            )));
        }
        let body = self.get_checked(&format!(
            "{}/ftps/wm/product/detail?productCode={code}",
            self.base
        ))?;
        let provenance = format!("{DEFAULT_BASE}/ftps/wm/product/detail?productCode={code}");
        match ingest_product_detail(&body, &provenance, now_s()) {
            Ok(r) if r.distributor_part.as_deref() == Some(code.as_str()) => Ok(r),
            Ok(r) => Err(DistributorError::Unusable(format!(
                "asked for {code}, Distributor returned {:?}",
                r.distributor_part
            ))),
            // Distributor answers an unknown part number with `{"code":200,"result":null}`.
            Err(SourcingError::Provider(m)) if m.contains("without a result") => {
                Err(DistributorError::NotFound(format!("Distributor has no part {code}")))
            }
            Err(e) => Err(DistributorError::Unusable(e.to_string())),
        }
    }

    /// Search by Distributor part number (exact lookup) or keyword/MPN (Distributor public search).
    pub fn search(&self, query: &str) -> Result<Vec<SourcingRecord>, DistributorError> {
        let q = query.trim();
        if q.is_empty() {
            return Err(DistributorError::BadQuery("empty query".into()));
        }
        if q.chars().count() > 120 || q.chars().any(char::is_control) {
            return Err(DistributorError::BadQuery(
                "query too long or contains control characters".into(),
            ));
        }
        if is_distributor_part_number(&q.to_ascii_uppercase()) {
            return self.lookup(q).map(|r| vec![r]);
        }
        let body = self.get_checked(&format!(
            "{}/ftps/wm/search/global?keyword={}",
            self.base,
            encode(q)
        ))?;
        // The response shape of this endpoint has not been observed (it is refused today),
        // so only product codes are taken from it; every hit is then fetched through the
        // verified detail path.
        let doc: Value = serde_json::from_slice(&body)
            .map_err(|e| DistributorError::Unusable(format!("search response is not JSON: {e}")))?;
        let mut codes = vec![];
        collect_product_codes(&doc, 0, &mut codes);
        let mut seen = std::collections::BTreeSet::new();
        codes.retain(|c| seen.insert(c.clone()));
        let mut out = vec![];
        for (i, code) in codes.into_iter().take(MAX_HITS).enumerate() {
            if i > 0 {
                std::thread::sleep(self.pause);
            }
            match self.lookup(&code) {
                Ok(r) => out.push(r),
                Err(DistributorError::NotFound(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }
}

fn collect_product_codes(v: &Value, depth: usize, out: &mut Vec<String>) {
    if depth > 6 {
        return;
    }
    match v {
        Value::Object(m) => {
            if let Some(c) = m.get("productCode").and_then(Value::as_str) {
                if is_distributor_part_number(c) {
                    out.push(c.to_string());
                    return;
                }
            }
            for x in m.values() {
                collect_product_codes(x, depth + 1, out);
            }
        }
        Value::Array(a) => a
            .iter()
            .for_each(|x| collect_product_codes(x, depth + 1, out)),
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------
// Import into the EAK library
// ---------------------------------------------------------------------------------------

/// A definition ready for `Command::UpsertComponentDefinition`, with what was inferred.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPlan {
    pub definition: ComponentDef,
    /// Library definition whose symbol, footprint and pin map were reused.
    pub template_id: String,
    pub notes: Vec<String>,
}

/// Library id a part is imported under.
pub fn import_id(distributor: &str) -> String {
    format!("DISTRIBUTOR_{distributor}")
}

fn spec<'a>(rec: &'a SourcingRecord, names: &[&str]) -> Option<&'a str> {
    rec.specs
        .iter()
        .find(|s| names.iter().any(|n| s.name.eq_ignore_ascii_case(n)))
        .map(|s| s.value.as_str())
}

/// Convert an Distributor display value (`10kΩ`, `100nF`, `4.7µF`) into the short form the EAK
/// library uses (`10k`, `100n`, `4.7u`).
fn short_value(raw: &str, unit_suffixes: &[&str]) -> String {
    let mut s = raw.trim().to_string();
    for u in unit_suffixes {
        if let Some(t) = s.strip_suffix(u) {
            s = t.trim().to_string();
        }
    }
    s.replace(['µ', 'μ'], "u")
}

/// Template for a chip package: `R_0603`, `C_0805`, `LED_0603`, ...
fn passive_template(prefix: &str, package: &str) -> Option<ComponentDef> {
    builtin::get(&format!("{prefix}_{}", package.trim()))
}

/// Decide how a record maps into the library, or refuse with the reason.
pub fn import_plan(rec: &SourcingRecord) -> Result<ImportPlan, String> {
    let distributor = rec
        .distributor_part
        .as_deref()
        .ok_or("the record has no Distributor part number")?;
    let mpn = rec.mpn.as_deref().ok_or("the record has no MPN")?;
    let package = rec
        .package
        .as_deref()
        .ok_or("the record names no package, so no footprint can be chosen")?;
    let cat = rec.category.as_deref().unwrap_or("").to_ascii_lowercase();
    let desc = rec
        .description
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut notes = vec![];

    let (template, value) = if cat.contains("resistor") && !cat.contains("network") {
        let v = spec(rec, &["Resistance"]).map(|v| short_value(v, &["Ω", "ohm", "Ohm"]));
        (passive_template("R", package), v)
    } else if cat.contains("capacitor") && cat.contains("multilayer") {
        let v = spec(rec, &["Capacitance"]).map(|v| short_value(v, &["F"]));
        (passive_template("C", package), v)
    } else if cat.contains("led") || desc.contains(" led") {
        (passive_template("LED", package), None)
    } else {
        (None, None)
    };

    // Any other part: reuse an existing library definition only when its MPN matches
    // exactly; the supplier then supplies identity, never pins or pads.
    let template = template
        .or_else(|| {
            builtin::components().into_iter().find(|c| {
                c.mpn
                    .as_deref()
                    .is_some_and(|m| m.eq_ignore_ascii_case(mpn))
            })
        })
        .ok_or_else(|| {
            format!(
                "no EAK symbol/footprint template for {mpn} in package {package} \
                 (category {:?}). Supported: chip resistors, MLCC capacitors and LEDs in a \
                 package the library holds, and parts whose MPN already exists in the \
                 library. Author the symbol and footprint in the editors, then attach \
                 {distributor} to it. Nothing was imported and no footprint was guessed.",
                rec.category.as_deref().unwrap_or("not stated")
            )
        })?;

    let mut def = template.clone();
    def.id = import_id(distributor);
    def.name = format!("{mpn} ({package})");
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        def.default_value = v;
    } else {
        notes.push("default value kept from the template; set it on the symbol".into());
    }
    def.manufacturer = rec.manufacturer.clone();
    def.mpn = Some(mpn.to_string());
    // Commercial data goes into a supplier offer, not into the component's identity.
    def.supplier_offers.clear();
    def.set_offer(
        rec.offer()
            .unwrap_or_else(|| eak_core::SupplierOffer::bare("distributor", distributor)),
    );
    def.datasheet = rec.datasheet_url.clone();
    def.provenance = format!(
        "Distributor {distributor} (sourcing record sha256 {}, state {:?}); symbol and footprint reused from library template {}",
        rec.raw_sha256, rec.state, template.id
    );
    notes.push(format!(
        "symbol, footprint and pin map reused from {}; electrical ratings were NOT taken from Distributor and stay unverified",
        template.id
    ));
    let problems = def.validate();
    if !problems.is_empty() {
        return Err(format!(
            "imported definition is invalid: {}",
            problems.join("; ")
        ));
    }
    Ok(ImportPlan {
        definition: def,
        template_id: template.id,
        notes,
    })
}

/// Visible text of an HTML/plain error body, whitespace-collapsed and cut to 120 characters.
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
