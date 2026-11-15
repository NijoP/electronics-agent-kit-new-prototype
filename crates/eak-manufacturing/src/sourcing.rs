//! LCSC sourcing records, offline-first.
//!
//! This module performs no network access. The fetch boundary is [`ingest_product_detail`]:
//! it takes the raw response bytes, the URL they came from and the retrieval time, and
//! returns a [`SourcingRecord`] carrying that provenance and the SHA-256 of the raw bytes.
//! Whoever obtained the bytes (a future permitted client, or a file the user saved) is
//! outside this crate. See `docs/engineering/sourcing.md` for the access and terms analysis.
//!
//! Rules enforced here:
//! * Everything in the raw input is untrusted data. It is never executed or interpreted
//!   as instructions; a field that is absent or fails validation is recorded as missing or
//!   rejected, never guessed, coerced or repaired.
//! * A freshly ingested record is a search hit. It becomes datasheet-matched only with a
//!   hashed datasheet file that names the MPN, and verified only when a human reviewer
//!   attests it against that exact datasheet hash. Nothing in the input can raise the state.
//! * Supplier data never changes library ratings, pins or footprints ([`apply_identity`]
//!   copies identity fields only), and nothing here feeds ERC, DRC or the release gate.

use eak_core::{ComponentDef, OfferPrice, SupplierOffer};
use eak_runtime::{Actor, ActorKind};
use eak_validation::sourcing::is_lcsc_part_number;
pub use eak_validation::sourcing::{SupplierPart, VerificationState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Largest product-detail response accepted.
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
/// Largest datasheet file accepted.
pub const MAX_DATASHEET_BYTES: usize = 64 * 1024 * 1024;
const MAX_TEXT: usize = 256;
const MAX_DESCRIPTION: usize = 1024;
const MAX_URL: usize = 2048;
const MAX_SPECS: usize = 256;
const MAX_PRICE_BREAKS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourcingError {
    /// Input larger than the accepted limit (bytes).
    TooLarge(usize),
    /// Not UTF-8 JSON, or nested too deeply.
    Malformed(String),
    /// JSON of an unexpected shape (not a product-detail document).
    UnexpectedShape(String),
    /// The provider answered with an error instead of a product.
    Provider(String),
    /// The provenance URL is not an https URL on the provider's domain.
    BadSourceUrl(String),
    /// The datasheet bytes or URL were rejected.
    BadDatasheet(String),
    /// A verification precondition is not met.
    NotVerifiable(String),
    Io(String),
}

impl std::fmt::Display for SourcingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourcingError::TooLarge(n) => write!(f, "input of {n} bytes exceeds the size limit"),
            SourcingError::Malformed(m) => write!(f, "malformed input: {m}"),
            SourcingError::UnexpectedShape(m) => write!(f, "unexpected document shape: {m}"),
            SourcingError::Provider(m) => write!(f, "provider error: {m}"),
            SourcingError::BadSourceUrl(m) => write!(f, "source URL rejected: {m}"),
            SourcingError::BadDatasheet(m) => write!(f, "datasheet rejected: {m}"),
            SourcingError::NotVerifiable(m) => write!(f, "cannot verify: {m}"),
            SourcingError::Io(m) => write!(f, "I/O error: {m}"),
        }
    }
}

impl std::error::Error for SourcingError {}

/// One parametric attribute, name and value exactly as the supplier wrote them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spec {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceBreak {
    pub min_quantity: u64,
    /// Unit price as parsed from the JSON number, in decimal text; never rounded or converted.
    pub unit_price: String,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasheetEvidence {
    pub url: String,
    pub retrieved_at_unix_s: u64,
    pub sha256: String,
    pub bytes: u64,
    /// The record's MPN occurs in the file's uncompressed bytes.
    pub names_mpn: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewerAttestation {
    pub reviewer: String,
    pub at_unix_s: u64,
    /// The datasheet hash the reviewer checked the record against.
    pub datasheet_sha256: String,
}

/// One part candidate from a supplier, with provenance and confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourcingRecord {
    pub provider: String,
    pub source_url: String,
    /// When the raw response was retrieved (supplied by the caller, Unix seconds).
    pub retrieved_at_unix_s: u64,
    /// SHA-256 of the raw response bytes exactly as received.
    pub raw_sha256: String,
    pub raw_bytes: u64,
    pub lcsc_part: Option<String>,
    pub mpn: Option<String>,
    pub manufacturer: Option<String>,
    pub package: Option<String>,
    /// Catalogue path as LCSC wrote it (`parent / sub` category), when stated.
    #[serde(default)]
    pub category: Option<String>,
    pub description: Option<String>,
    pub datasheet_url: Option<String>,
    pub specs: Vec<Spec>,
    /// Present only when the response stated it.
    pub stock: Option<u64>,
    pub price_breaks: Vec<PriceBreak>,
    /// Record fields the response did not provide.
    pub missing: Vec<String>,
    /// Response fields that were present but failed validation, with the reason.
    pub rejected: Vec<String>,
    pub datasheet: Option<DatasheetEvidence>,
    pub attestation: Option<ReviewerAttestation>,
    /// Derived from the evidence above by this module; never taken from the input.
    pub state: VerificationState,
}

fn sha(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

/// Accept only `https://<host>[:port]/...` without user-info, and return the host.
fn https_host(url: &str) -> Result<&str, String> {
    if url.len() > MAX_URL || url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("too long or contains control / whitespace characters".into());
    }
    let rest = url.strip_prefix("https://").ok_or("scheme is not https")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return Err("user-info in the authority".into());
    }
    let host = authority.split(':').next().unwrap_or("");
    let valid = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    if !valid {
        return Err("invalid host".into());
    }
    Ok(host)
}

fn is_lcsc_host(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h == "lcsc.com" || h.ends_with(".lcsc.com")
}

/// A bounded, printable, non-empty text field, returned exactly as written.
fn bounded_text(v: &Value, max: usize) -> Result<String, String> {
    let s = v.as_str().ok_or("not a string")?;
    if s.trim().is_empty() {
        return Err("empty".into());
    }
    if s.chars().count() > max {
        return Err(format!("longer than {max} characters"));
    }
    if s.chars().any(char::is_control) {
        return Err("contains control characters".into());
    }
    Ok(s.to_string())
}

fn text(v: &Value) -> Result<String, String> {
    bounded_text(v, MAX_TEXT)
}

struct Fields<'a> {
    product: &'a serde_json::Map<String, Value>,
    missing: Vec<String>,
    rejected: Vec<String>,
}

impl Fields<'_> {
    /// Read `key` with `parse`; absent/null -> missing (when required), invalid -> rejected.
    fn get<T>(
        &mut self,
        key: &str,
        record_field: Option<&str>,
        parse: impl FnOnce(&Value) -> Result<T, String>,
    ) -> Option<T> {
        match self.product.get(key) {
            None | Some(Value::Null) => {}
            Some(v) => match parse(v) {
                Ok(t) => return Some(t),
                Err(why) => self.rejected.push(format!("{key}: {why}")),
            },
        }
        if let Some(field) = record_field {
            self.missing.push(field.to_string());
        }
        None
    }
}

fn price_break(v: &Value) -> Result<PriceBreak, String> {
    let min_quantity = v
        .get("ladder")
        .and_then(Value::as_u64)
        .filter(|q| *q >= 1)
        .ok_or("ladder is not a positive integer")?;
    let price = v
        .get("usdPrice")
        .and_then(Value::as_number)
        .ok_or("usdPrice is not a number")?;
    if !price.as_f64().is_some_and(|p| p.is_finite() && p >= 0.0) {
        return Err("usdPrice is negative or not finite".into());
    }
    Ok(PriceBreak {
        min_quantity,
        unit_price: price.to_string(),
        currency: "USD".into(),
    })
}

fn spec(v: &Value) -> Result<Spec, String> {
    let field = |k: &str| {
        v.get(k)
            .ok_or(format!("{k} absent"))
            .and_then(|x| text(x).map_err(|e| format!("{k} {e}")))
    };
    Ok(Spec {
        name: field("paramNameEn")?,
        value: field("paramValueEn")?,
    })
}

/// Parse a bounded list, rejecting individual bad entries rather than the whole record.
fn list<T>(
    v: &Value,
    key: &str,
    max: usize,
    item: impl Fn(&Value) -> Result<T, String>,
    rejected: &mut Vec<String>,
) -> Vec<T> {
    let Some(items) = v.as_array() else {
        rejected.push(format!("{key}: not an array"));
        return vec![];
    };
    if items.len() > max {
        rejected.push(format!("{key}: more than {max} entries"));
        return vec![];
    }
    let mut out = vec![];
    for (i, it) in items.iter().enumerate() {
        match item(it) {
            Ok(t) => out.push(t),
            Err(why) => rejected.push(format!("{key}[{i}]: {why}")),
        }
    }
    out
}

/// The fetch boundary: turn one raw LCSC product-detail response into a record.
///
/// `raw` is the response body exactly as received, `source_url` the https URL on
/// `lcsc.com` it came from, `retrieved_at_unix_s` when it was retrieved. Accepted shapes
/// are the product object itself or the `{"code": 200, "result": {...}}` envelope. The
/// field mapping is listed in `docs/engineering/sourcing.md`; it has not been checked
/// against LCSC's own API documentation.
pub fn ingest_product_detail(
    raw: &[u8],
    source_url: &str,
    retrieved_at_unix_s: u64,
) -> Result<SourcingRecord, SourcingError> {
    let host = https_host(source_url).map_err(SourcingError::BadSourceUrl)?;
    if !is_lcsc_host(host) {
        return Err(SourcingError::BadSourceUrl(format!(
            "{host} is not an lcsc.com host"
        )));
    }
    if raw.len() > MAX_RESPONSE_BYTES {
        return Err(SourcingError::TooLarge(raw.len()));
    }
    let doc: Value =
        serde_json::from_slice(raw).map_err(|e| SourcingError::Malformed(e.to_string()))?;
    let root = doc
        .as_object()
        .ok_or_else(|| SourcingError::UnexpectedShape("top level is not an object".into()))?;
    let product = if root.contains_key("productCode") {
        root
    } else {
        match root.get("code").and_then(Value::as_i64) {
            Some(200) => {}
            Some(code) => return Err(SourcingError::Provider(format!("code {code}"))),
            None => {
                return Err(SourcingError::UnexpectedShape(
                    "neither a product object nor a result envelope".into(),
                ))
            }
        }
        root.get("result")
            .and_then(Value::as_object)
            .ok_or_else(|| SourcingError::Provider("code 200 without a result object".into()))?
    };

    let mut f = Fields {
        product,
        missing: vec![],
        rejected: vec![],
    };
    let lcsc_part = f.get("productCode", Some("lcsc_part"), |v| {
        text(v).and_then(|s| {
            if is_lcsc_part_number(&s) {
                Ok(s)
            } else {
                Err("not an LCSC part number (C followed by digits)".into())
            }
        })
    });
    let mpn = f.get("productModel", Some("mpn"), text);
    let manufacturer = f.get("brandNameEn", Some("manufacturer"), text);
    let package = f.get("encapStandard", Some("package"), text);
    let category = {
        let part = |k: &str| product.get(k).and_then(|v| text(v).ok());
        match (part("parentCatalogName"), part("catalogName")) {
            (Some(a), Some(b)) => Some(format!("{a} / {b}")),
            (a, b) => a.or(b),
        }
    };
    let description = f.get("productIntroEn", None, |v| bounded_text(v, MAX_DESCRIPTION));
    let datasheet_url = f.get("pdfUrl", Some("datasheet_url"), |v| {
        let s = v.as_str().ok_or("not a string")?;
        https_host(s).map(|_| s.to_string())
    });
    let stock = f.get("stockNumber", None, |v| {
        v.as_u64()
            .ok_or_else(|| "not a non-negative integer".to_string())
    });
    let mut list_rejects = vec![];
    let price_breaks = f
        .get("productPriceList", None, |v| {
            Ok(list(
                v,
                "productPriceList",
                MAX_PRICE_BREAKS,
                price_break,
                &mut list_rejects,
            ))
        })
        .unwrap_or_default();
    let specs = f
        .get("paramVOList", None, |v| {
            Ok(list(v, "paramVOList", MAX_SPECS, spec, &mut list_rejects))
        })
        .unwrap_or_default();
    if specs.is_empty() {
        f.missing.push("specs".into());
    }
    f.rejected.extend(list_rejects);

    Ok(SourcingRecord {
        provider: "LCSC".into(),
        source_url: source_url.to_string(),
        retrieved_at_unix_s,
        raw_sha256: sha(raw),
        raw_bytes: raw.len() as u64,
        lcsc_part,
        mpn,
        manufacturer,
        package,
        category,
        description,
        datasheet_url,
        specs,
        stock,
        price_breaks,
        missing: f.missing,
        rejected: f.rejected,
        datasheet: None,
        attestation: None,
        state: VerificationState::SearchHit,
    })
}

fn contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|w| w.eq_ignore_ascii_case(needle))
}

impl SourcingRecord {
    /// The state the recorded evidence supports. Stored state is always this value.
    fn evidence_state(&self) -> VerificationState {
        let Some(ds) = &self.datasheet else {
            return VerificationState::SearchHit;
        };
        let attested = self
            .attestation
            .as_ref()
            .is_some_and(|a| a.datasheet_sha256 == ds.sha256 && !a.reviewer.trim().is_empty());
        if attested && self.missing.is_empty() {
            VerificationState::Verified
        } else if ds.names_mpn && self.mpn.is_some() {
            VerificationState::DatasheetMatched
        } else {
            VerificationState::SearchHit
        }
    }

    /// Record a retrieved datasheet file: its SHA-256, size, URL and retrieval time.
    ///
    /// The record becomes datasheet-matched only if the file is a PDF and names the MPN.
    /// Attaching a datasheet discards any earlier reviewer attestation, which was bound to
    /// the previous file.
    pub fn attach_datasheet(
        &mut self,
        raw: &[u8],
        url: &str,
        retrieved_at_unix_s: u64,
    ) -> Result<(), SourcingError> {
        https_host(url).map_err(SourcingError::BadDatasheet)?;
        if raw.len() > MAX_DATASHEET_BYTES {
            return Err(SourcingError::TooLarge(raw.len()));
        }
        // A PDF file begins with the header `%PDF-` (ISO 32000-1, file header).
        if !raw.starts_with(b"%PDF-") {
            return Err(SourcingError::BadDatasheet(
                "not a PDF file (no %PDF- header)".into(),
            ));
        }
        // shortcut: the MPN is searched in the file's uncompressed bytes only (document
        // metadata, uncompressed text); upgrade when a native PDF text extractor exists.
        let names_mpn = self
            .mpn
            .as_deref()
            .is_some_and(|m| m.len() >= 4 && contains_ignore_ascii_case(raw, m.as_bytes()));
        self.datasheet = Some(DatasheetEvidence {
            url: url.to_string(),
            retrieved_at_unix_s,
            sha256: sha(raw),
            bytes: raw.len() as u64,
            names_mpn,
        });
        self.attestation = None;
        self.state = self.evidence_state();
        Ok(())
    }

    /// Record a human reviewer's attestation that this record was checked against the
    /// datasheet with hash `datasheet_sha256`. Agents cannot verify parts.
    pub fn verify(
        &mut self,
        reviewer: &Actor,
        datasheet_sha256: &str,
        at_unix_s: u64,
    ) -> Result<(), SourcingError> {
        let refuse = |m: &str| Err(SourcingError::NotVerifiable(m.to_string()));
        if reviewer.kind != ActorKind::Human || reviewer.name.trim().is_empty() {
            return refuse("only a named human reviewer can verify a part");
        }
        let Some(ds) = &self.datasheet else {
            return refuse("no datasheet file is recorded");
        };
        if ds.sha256 != datasheet_sha256 {
            return refuse("the reviewed datasheet hash is not the recorded datasheet hash");
        }
        if !self.missing.is_empty() {
            return Err(SourcingError::NotVerifiable(format!(
                "record has missing fields: {}",
                self.missing.join(", ")
            )));
        }
        self.attestation = Some(ReviewerAttestation {
            reviewer: reviewer.name.clone(),
            at_unix_s,
            datasheet_sha256: datasheet_sha256.to_string(),
        });
        self.state = self.evidence_state();
        Ok(())
    }

    /// This record's commercial side as a [`SupplierOffer`] (ADR-0028): LCSC number, stock,
    /// price breaks and fetch time, none of it identity. `None` without an LCSC number. The
    /// record carries no lifecycle field, so the offer's `lifecycle` stays `None` (not reported).
    pub fn offer(&self) -> Option<SupplierOffer> {
        let part = self.lcsc_part.clone()?;
        let mut offer = SupplierOffer::bare("lcsc", &part);
        offer.url = Some(self.source_url.clone());
        offer.stock = self.stock;
        offer.price_breaks = self
            .price_breaks
            .iter()
            .map(|b| OfferPrice {
                min_quantity: b.min_quantity,
                unit_price: b.unit_price.clone(),
                currency: b.currency.clone(),
            })
            .collect();
        offer.fetched_at_unix_s = Some(self.retrieved_at_unix_s);
        offer.source_sha256 = Some(self.raw_sha256.clone());
        Some(offer)
    }

    /// The facts the `SOURCING` verification report consumes. `None` without an LCSC number.
    pub fn supplier_part(&self) -> Option<SupplierPart> {
        Some(SupplierPart {
            lcsc: self.lcsc_part.clone()?,
            mpn: self.mpn.clone(),
            package: self.package.clone(),
            datasheet_sha256: self.datasheet.as_ref().map(|d| d.sha256.clone()),
            state: self.evidence_state(),
        })
    }
}

/// Copy a record's identity (manufacturer, MPN, LCSC part number, datasheet URL) into a
/// copy of a library definition, for submission through `Command::UpsertComponentDefinition`.
///
/// Ratings, pins, pads and the pin map are never touched: supplier data does not override
/// library engineering data. An identity field the definition already holds with a
/// different value is a conflict and is refused, not overwritten.
pub fn apply_identity(def: &ComponentDef, rec: &SourcingRecord) -> Result<ComponentDef, String> {
    let lcsc = rec
        .lcsc_part
        .as_ref()
        .ok_or("sourcing record has no LCSC part number")?;
    let mpn = rec.mpn.as_ref().ok_or("sourcing record has no MPN")?;
    let held = |slot: &Option<String>| {
        slot.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
    };
    for (name, old, new) in [
        (
            "LCSC part number",
            &def.supplier_part("lcsc").map(String::from),
            lcsc,
        ),
        ("MPN", &def.mpn, mpn),
    ] {
        if let Some(old) = held(old).filter(|old| !old.eq_ignore_ascii_case(new)) {
            return Err(format!(
                "{}: {name} is already {old:?}; refusing to replace it with {new:?} from {lcsc}",
                def.id
            ));
        }
    }
    let mut out = def.clone();
    out.set_offer(
        rec.offer()
            .ok_or("sourcing record has no LCSC part number")?,
    );
    out.mpn = held(&def.mpn).or_else(|| Some(mpn.clone()));
    // Manufacturer and datasheet only fill a gap; an existing library value is kept.
    out.manufacturer = held(&def.manufacturer).or_else(|| rec.manufacturer.clone());
    out.datasheet = held(&def.datasheet).or_else(|| rec.datasheet_url.clone());
    out.provenance = format!(
        "{}; sourcing identity from LCSC record {lcsc} (sha256 {}, state {:?})",
        def.provenance,
        rec.raw_sha256,
        rec.evidence_state()
    );
    Ok(out)
}

/// Case-insensitive offline search over records already held locally: every
/// whitespace-separated term must occur in the LCSC number, MPN, manufacturer, package or
/// description. Results are search hits in whatever state their evidence supports.
pub fn search<'a>(records: &'a [SourcingRecord], query: &str) -> Vec<&'a SourcingRecord> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    if terms.is_empty() {
        return vec![];
    }
    records
        .iter()
        .filter(|r| {
            let hay = [
                &r.lcsc_part,
                &r.mpn,
                &r.manufacturer,
                &r.package,
                &r.description,
            ]
            .iter()
            .filter_map(|o| o.as_deref())
            .collect::<Vec<_>>()
            .join("\n")
            .to_ascii_lowercase();
            terms.iter().all(|t| hay.contains(t.as_str()))
        })
        .collect()
}

/// Store a record as `<dir>/<LCSC part number>.json` (atomic write).
pub fn write_record(dir: &Path, rec: &SourcingRecord) -> Result<PathBuf, SourcingError> {
    let lcsc = rec
        .lcsc_part
        .as_deref()
        .filter(|l| is_lcsc_part_number(l))
        .ok_or_else(|| SourcingError::Io("record has no LCSC part number to store under".into()))?;
    let io = |e: std::io::Error| SourcingError::Io(e.to_string());
    std::fs::create_dir_all(dir).map_err(io)?;
    let path = dir.join(format!("{lcsc}.json"));
    let json = serde_json::to_string_pretty(rec).map_err(|e| SourcingError::Io(e.to_string()))?;
    eak_runtime::store::atomic_write(&path, json.as_bytes()).map_err(io)?;
    Ok(path)
}

/// Load every stored record in `dir`, sorted by file name. A stored file is untrusted
/// too: its state is recomputed from its evidence, so a bare `"state": "verified"` is not
/// honoured. Stored files are not signed; a forged attestation block is not detected.
pub fn read_records(dir: &Path) -> Result<Vec<SourcingRecord>, SourcingError> {
    let io = |e: std::io::Error| SourcingError::Io(e.to_string());
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(io)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    let mut out = vec![];
    for path in paths {
        let data = std::fs::read(&path).map_err(io)?;
        let mut rec: SourcingRecord = serde_json::from_slice(&data)
            .map_err(|e| SourcingError::Malformed(format!("{}: {e}", path.display())))?;
        rec.state = rec.evidence_state();
        out.push(rec);
    }
    Ok(out)
}
