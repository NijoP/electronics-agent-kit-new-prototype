//! Provider-independent part sources and the resolver that merges their answers.
//!
//! Every place parts can come from (the built-in library, a user or imported library, a
//! supplier catalogue the user has configured) sits behind one trait, [`PartSource`]. None is
//! required. A source that is not [`SourceStatus::Ready`] is **never called**: the resolver
//! reports it as skipped with its reason, so an unconfigured network source makes zero requests
//! and an unavailable capability is shown as unavailable, never simulated.
//!
//! The resolver decides match quality itself from the normalised [`PartKey`]; it does not trust a
//! source's own ranking. Exact hits merge into [`Part`]s by key. Fuzzy hits are suggestions and
//! are never merged into a part.

use crate::part::{Part, PartKey, PartRecord, SourceId};
use eak_manufacturing::matching::{normalize_manufacturer, normalize_mpn};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Reads only local data (no network).
    Local,
    /// Talks to a remote service. Off unless the user configured it.
    Network,
}

/// What a source can provide. A UI shows a missing capability as unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Capabilities {
    pub definitions: bool,
    pub parameters: bool,
    pub offers: bool,
    pub datasheets: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", content = "reason", rename_all = "snake_case")]
pub enum SourceStatus {
    Ready,
    /// The source needs something from the user first (credentials, a folder, a file).
    NeedsSetup(String),
    Disabled,
    Error(String),
}

/// A search. `text` is an MPN or free text; `manufacturer` narrows exact matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartQuery {
    pub text: String,
    pub manufacturer: Option<String>,
    /// Upper bound on records used per source.
    pub limit: usize,
}

impl PartQuery {
    pub const DEFAULT_LIMIT: usize = 50;

    pub fn new(text: &str) -> PartQuery {
        PartQuery {
            text: text.trim().to_string(),
            manufacturer: None,
            limit: Self::DEFAULT_LIMIT,
        }
    }

    pub fn with_manufacturer(mut self, manufacturer: &str) -> PartQuery {
        self.manufacturer = Some(manufacturer.trim().to_string());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum SourceError {
    /// The query cannot be answered by this source (too long, empty, wrong form).
    BadQuery(String),
    /// The source could not be reached or read.
    Unavailable(String),
    /// The source answered with something unusable.
    Failed(String),
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceError::BadQuery(s) => write!(f, "bad query: {s}"),
            SourceError::Unavailable(s) => write!(f, "unavailable: {s}"),
            SourceError::Failed(s) => write!(f, "failed: {s}"),
        }
    }
}

impl std::error::Error for SourceError {}

/// A source's answer: keyed records plus a note for every listing it could not key.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SourceAnswer {
    pub records: Vec<PartRecord>,
    /// Listings the source returned but could not turn into a record (for example no
    /// manufacturer), each with the reason. Reported, never dropped silently.
    pub unkeyed: Vec<String>,
}

/// One place parts come from.
pub trait PartSource: Send + Sync {
    fn id(&self) -> SourceId;
    fn kind(&self) -> SourceKind;
    fn capabilities(&self) -> Capabilities;
    fn status(&self) -> SourceStatus;
    /// Licence or terms text shown next to anything this source supplied.
    fn terms(&self) -> String;
    /// Bounded search. Only called when `status()` is `Ready`.
    fn search(&self, q: &PartQuery) -> Result<SourceAnswer, SourceError>;
    /// The source's record for one exact key, if it has one. Only called when `Ready`.
    fn fetch(&self, key: &PartKey) -> Result<Option<PartRecord>, SourceError>;
}

/// How a record matched the query, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    /// MPN and manufacturer both equal after normalisation.
    ExactMpnManufacturer,
    /// MPN equal after normalisation; no manufacturer asked for.
    ExactMpn,
    /// Anything else the source returned. A suggestion only.
    Fuzzy,
}

/// Classify a record against a query, from the normalised keys only.
pub fn classify(q: &PartQuery, key: &PartKey) -> MatchKind {
    if normalize_mpn(&q.text) != key.mpn() {
        return MatchKind::Fuzzy;
    }
    match q
        .manufacturer
        .as_deref()
        .map(normalize_manufacturer)
        .filter(|m| !m.is_empty())
    {
        None => MatchKind::ExactMpn,
        Some(m) if m == key.manufacturer() => MatchKind::ExactMpnManufacturer,
        // Same MPN text from another manufacturer: a different part, so only a suggestion.
        Some(_) => MatchKind::Fuzzy,
    }
}

/// A merged exact part with how it matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedPart {
    pub part: Part,
    pub kind: MatchKind,
}

/// What happened with one source that was skipped, failed, or reported unkeyed listings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceOutcome {
    pub source: SourceId,
    pub status: SourceStatus,
    pub error: Option<SourceError>,
    pub unkeyed: Vec<String>,
}

/// Everything a resolution found, and everything it did not ask or could not use.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Resolution {
    /// Exact parts, best match kind first, then by key.
    pub parts: Vec<ResolvedPart>,
    /// Fuzzy records, never merged into a part.
    pub suggestions: Vec<PartRecord>,
    pub sources: Vec<SourceOutcome>,
}

/// Ask every ready source, classify every record, merge exact records by key.
pub fn resolve(sources: &[&dyn PartSource], q: &PartQuery) -> Resolution {
    let mut out = Resolution::default();
    let mut exact: Vec<(MatchKind, PartRecord)> = Vec::new();
    for s in sources {
        let status = s.status();
        if status != SourceStatus::Ready {
            out.sources.push(SourceOutcome {
                source: s.id(),
                status,
                error: None,
                unkeyed: Vec::new(),
            });
            continue;
        }
        match s.search(q) {
            Ok(answer) => {
                let mut records = answer.records;
                let mut unkeyed = answer.unkeyed;
                if records.len() > q.limit {
                    unkeyed.push(format!(
                        "{} records over the limit of {} were not used",
                        records.len() - q.limit,
                        q.limit
                    ));
                    records.truncate(q.limit);
                }
                for r in records {
                    match classify(q, &r.key) {
                        MatchKind::Fuzzy => out.suggestions.push(r),
                        k => exact.push((k, r)),
                    }
                }
                if !unkeyed.is_empty() {
                    out.sources.push(SourceOutcome {
                        source: s.id(),
                        status: SourceStatus::Ready,
                        error: None,
                        unkeyed,
                    });
                }
            }
            Err(e) => out.sources.push(SourceOutcome {
                source: s.id(),
                status: SourceStatus::Ready,
                error: Some(e),
                unkeyed: Vec::new(),
            }),
        }
    }
    let kinds: BTreeMap<PartKey, MatchKind> =
        exact.iter().map(|(k, r)| (r.key.clone(), *k)).collect();
    let mut parts: Vec<ResolvedPart> = Part::merge(exact.into_iter().map(|(_, r)| r).collect())
        .into_iter()
        .map(|part| ResolvedPart {
            kind: kinds[&part.key],
            part,
        })
        .collect();
    parts.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.part.key.cmp(&b.part.key))
    });
    out.parts = parts;
    out
}
