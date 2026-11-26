//! Adapters that put existing EAK data behind [`PartSource`].
//!
//! * [`BuiltinSource`]: the built-in starter library (`eak_core::builtin`). Local, always ready.
//!   Its definitions are linked at [`LibraryMaturity::Draft`]: the starter library carries no
//!   verification evidence, so none is claimed.
//! * [`SupplierRecordSource`]: any supplier integration that already produces
//!   [`SourcingRecord`]s (the existing signed distributor client's catalogue search, a file the
//!   user supplied through `sourcing`). This crate adds no network code and no new supplier call:
//!   the caller hands in the search function it already has, plus the status it already knows.
//!   A source built with a status other than `Ready` (for example no credentials) is never
//!   called.
//! * [`local::LocalListSource`]: a part list the user supplied (CSV or JSON), validated row by row.
//! * [`lcsc::LcscSlot`]: the LCSC slot, always disabled and "not connected"; it makes no requests.

pub mod lcsc;
pub mod local;

use crate::part::{
    DatasheetRef, DefKind, DefLink, OfferSnapshot, Parameter, PartKey, PartRecord, PriceBreak,
    Provenance, SourceId,
};
use crate::source::{
    Capabilities, PartQuery, PartSource, SourceAnswer, SourceError, SourceKind, SourceStatus,
};
use eak_core::libmeta::LibraryMaturity;
use eak_core::ComponentDef;
use eak_manufacturing::matching::normalize_mpn;
use eak_manufacturing::sourcing::SourcingRecord;
use sha2::{Digest, Sha256};

/// SHA-256 (lower-case hex) of a value's JSON form.
pub fn content_hash<T: serde::Serialize>(value: &T) -> Option<String> {
    let bytes = serde_json::to_vec(value).ok()?;
    Some(format!("{:x}", Sha256::digest(bytes)))
}

/// Whether free text `q` matches `hay` (case-insensitive substring, or MPN-normalised
/// substring so `tps 2051` finds `TPS2051B`).
fn text_matches(q: &str, hay: &str) -> bool {
    let qn = normalize_mpn(q);
    hay.to_lowercase().contains(&q.to_lowercase())
        || (!qn.is_empty() && normalize_mpn(hay).contains(&qn))
}

fn datasheet_link(url: &str) -> DatasheetRef {
    DatasheetRef::url(url)
}

/// The built-in starter library as a part source.
pub struct BuiltinSource {
    components: Vec<ComponentDef>,
}

impl BuiltinSource {
    pub const ID: &'static str = "eak-builtin";

    pub fn new() -> BuiltinSource {
        BuiltinSource::from_components(eak_core::builtin::components())
    }

    /// The same adapter over a given set of definitions.
    pub fn from_components(components: Vec<ComponentDef>) -> BuiltinSource {
        BuiltinSource { components }
    }

    fn record(c: &ComponentDef) -> Result<PartRecord, String> {
        let (Some(maker), Some(mpn)) = (c.manufacturer.as_deref(), c.mpn.as_deref()) else {
            return Err(format!(
                "{}: no manufacturer and MPN, so it has no part identity",
                c.id
            ));
        };
        let mut r = PartRecord::new(
            maker,
            mpn,
            Provenance {
                source: SourceId::new(Self::ID),
                reference: c.id.clone(),
                retrieved_at_unix_s: None,
                raw_sha256: None,
            },
        )
        .map_err(|e| format!("{}: {e}", c.id))?;
        let some = |s: &str| Some(s.to_string()).filter(|s| !s.trim().is_empty());
        r.description = some(&c.name);
        r.category = some(&c.category);
        r.package = some(&c.footprint.name);
        r.datasheets = c
            .datasheet
            .iter()
            .map(|u| datasheet_link(u.as_str()))
            .collect();
        r.definitions = vec![DefLink {
            kind: DefKind::Component,
            item_id: c.id.clone(),
            source: SourceId::new(Self::ID),
            maturity: LibraryMaturity::Draft,
            content_hash: content_hash(c),
        }];
        Ok(r)
    }
}

impl Default for BuiltinSource {
    fn default() -> Self {
        BuiltinSource::new()
    }
}

impl PartSource for BuiltinSource {
    fn id(&self) -> SourceId {
        SourceId::new(Self::ID)
    }
    fn kind(&self) -> SourceKind {
        SourceKind::Local
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            definitions: true,
            parameters: false,
            offers: false,
            datasheets: true,
        }
    }
    fn status(&self) -> SourceStatus {
        SourceStatus::Ready
    }
    fn terms(&self) -> String {
        "EAK built-in starter library; ratings are unverified".into()
    }

    fn search(&self, q: &PartQuery) -> Result<SourceAnswer, SourceError> {
        if q.text.is_empty() {
            return Err(SourceError::BadQuery("empty query".into()));
        }
        let mut answer = SourceAnswer::default();
        for c in &self.components {
            let hit = [
                c.mpn.as_deref().unwrap_or(""),
                c.name.as_str(),
                c.id.as_str(),
                c.default_value.as_str(),
            ]
            .iter()
            .any(|h| !h.is_empty() && text_matches(&q.text, h));
            if !hit {
                continue;
            }
            if answer.records.len() >= q.limit {
                break;
            }
            match Self::record(c) {
                Ok(r) => answer.records.push(r),
                Err(why) => answer.unkeyed.push(why),
            }
        }
        Ok(answer)
    }

    fn fetch(&self, key: &PartKey) -> Result<Option<PartRecord>, SourceError> {
        Ok(self
            .components
            .iter()
            .filter_map(|c| Self::record(c).ok())
            .find(|r| &r.key == key))
    }
}

/// The search function a supplier integration already has.
pub type RecordSearch =
    dyn Fn(&PartQuery) -> Result<Vec<SourcingRecord>, SourceError> + Send + Sync;

/// A supplier integration that yields [`SourcingRecord`]s, as a part source.
pub struct SupplierRecordSource {
    id: SourceId,
    kind: SourceKind,
    status: SourceStatus,
    terms: String,
    search: Box<RecordSearch>,
}

impl SupplierRecordSource {
    /// `status` is what the integration already knows about itself, for example
    /// `NeedsSetup("no credentials")`. Anything but `Ready` keeps `search` from being called.
    pub fn new(
        id: &str,
        kind: SourceKind,
        status: SourceStatus,
        terms: &str,
        search: Box<RecordSearch>,
    ) -> SupplierRecordSource {
        SupplierRecordSource {
            id: SourceId::new(id),
            kind,
            status,
            terms: terms.to_string(),
            search,
        }
    }

    /// One sourcing record as a part record; `Err` says why it has no identity.
    pub fn convert(&self, r: &SourcingRecord) -> Result<PartRecord, String> {
        let label = r
            .distributor_part
            .clone()
            .or_else(|| r.mpn.clone())
            .unwrap_or_else(|| r.source_url.clone());
        let (Some(maker), Some(mpn)) = (r.manufacturer.as_deref(), r.mpn.as_deref()) else {
            return Err(format!(
                "{label}: listing states no manufacturer and MPN, so it has no part identity"
            ));
        };
        let mut p = PartRecord::new(
            maker,
            mpn,
            Provenance {
                source: self.id.clone(),
                reference: r.source_url.clone(),
                retrieved_at_unix_s: Some(r.retrieved_at_unix_s),
                raw_sha256: Some(r.raw_sha256.clone()),
            },
        )
        .map_err(|e| format!("{label}: {e}"))?;
        p.description = r.description.clone();
        p.category = r.category.clone();
        p.package = r.package.clone();
        p.parameters = r
            .specs
            .iter()
            .map(|s| Parameter {
                name: s.name.clone(),
                value: s.value.clone(),
            })
            .collect();
        p.datasheets = r
            .datasheet_url
            .iter()
            .map(|u| datasheet_link(u.as_str()))
            .collect();
        if r.stock.is_some() || !r.price_breaks.is_empty() {
            p.offers.push(OfferSnapshot {
                source: self.id.clone(),
                supplier_pn: r.distributor_part.clone(),
                stock: r.stock,
                breaks: r
                    .price_breaks
                    .iter()
                    .map(|b| PriceBreak {
                        min_quantity: b.min_quantity,
                        unit_price: b.unit_price.clone(),
                        currency: b.currency.clone(),
                    })
                    .collect(),
                retrieved_at_unix_s: r.retrieved_at_unix_s,
                terms_note: self.terms.clone(),
            });
        }
        Ok(p)
    }
}

impl PartSource for SupplierRecordSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }
    fn kind(&self) -> SourceKind {
        self.kind
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            definitions: false,
            parameters: true,
            offers: true,
            datasheets: true,
        }
    }
    fn status(&self) -> SourceStatus {
        self.status.clone()
    }
    fn terms(&self) -> String {
        self.terms.clone()
    }

    fn search(&self, q: &PartQuery) -> Result<SourceAnswer, SourceError> {
        if self.status != SourceStatus::Ready {
            return Err(SourceError::Unavailable(format!(
                "source is not ready, so it was not called: {:?}",
                self.status
            )));
        }
        let mut answer = SourceAnswer::default();
        for r in (self.search)(q)? {
            match self.convert(&r) {
                Ok(p) => answer.records.push(p),
                Err(why) => answer.unkeyed.push(why),
            }
        }
        Ok(answer)
    }

    fn fetch(&self, key: &PartKey) -> Result<Option<PartRecord>, SourceError> {
        let answer = self.search(&PartQuery::new(key.mpn()))?;
        Ok(answer.records.into_iter().find(|r| &r.key == key))
    }
}
