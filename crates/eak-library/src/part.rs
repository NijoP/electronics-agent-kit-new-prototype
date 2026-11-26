//! MPN-keyed part records (library data, not EAK-IR).
//!
//! A part's identity is the pair (manufacturer, manufacturer part number), compared after the
//! normalisation in [`eak_manufacturing::matching`]. A distributor or supplier number is a
//! listing, never identity (ADR-0028): two manufacturers' products with the same MPN text are two
//! parts. A design never references a [`Part`]; it references `ComponentDef`s, and a part only
//! *points at* definition candidates through [`DefLink`].
//!
//! Each source contributes one [`PartRecord`]. A merged [`Part`] keeps every record as received:
//! when two sources disagree on a value, both values stay and [`Part::conflicts`] names the
//! field. Nothing is averaged or picked silently, and nothing here is verified by being stored.

use eak_core::libmeta::LibraryMaturity;
use eak_manufacturing::matching::{normalize_manufacturer, normalize_mpn};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Why a [`PartKey`] could not be built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKeyError {
    /// The MPN is empty after normalisation.
    EmptyMpn,
    /// The manufacturer is empty after normalisation. A part without a manufacturer has no
    /// identity, because the same MPN text can belong to several manufacturers.
    EmptyManufacturer,
}

impl std::fmt::Display for PartKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            PartKeyError::EmptyMpn => "manufacturer part number is empty",
            PartKeyError::EmptyManufacturer => "manufacturer is empty",
        })
    }
}

impl std::error::Error for PartKeyError {}

/// Normalised identity of a part. Built only through [`PartKey::new`], so the fields are always
/// the comparison keys of `matching::normalize_manufacturer` and `matching::normalize_mpn`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PartKey {
    manufacturer: String,
    mpn: String,
}

impl PartKey {
    pub fn new(manufacturer: &str, mpn: &str) -> Result<PartKey, PartKeyError> {
        let manufacturer = normalize_manufacturer(manufacturer);
        let mpn = normalize_mpn(mpn);
        if mpn.is_empty() {
            return Err(PartKeyError::EmptyMpn);
        }
        if manufacturer.is_empty() {
            return Err(PartKeyError::EmptyManufacturer);
        }
        Ok(PartKey { manufacturer, mpn })
    }

    /// Normalised manufacturer (comparison key, not for display).
    pub fn manufacturer(&self) -> &str {
        &self.manufacturer
    }

    /// Normalised MPN (comparison key, not for display).
    pub fn mpn(&self) -> &str {
        &self.mpn
    }
}

/// Stable id of a part source, for example `eak-builtin` or `supplier:distributor-api`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SourceId(pub String);

impl SourceId {
    pub fn new(id: &str) -> SourceId {
        SourceId(id.to_string())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One stated parameter, as the source wrote it (no unit conversion, no interpretation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub value: String,
}

/// A datasheet reference. A URL is a pointer only; nothing here fetches it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasheetRef {
    pub url: Option<String>,
    /// SHA-256 of a local copy, when one was supplied.
    pub sha256: Option<String>,
    pub revision: Option<String>,
}

/// What kind of definition a [`DefLink`] points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefKind {
    /// A whole `ComponentDef` (symbol + footprint + pin map).
    Component,
    Symbol,
    Footprint,
    Model3d,
}

/// A candidate definition for a part, from one source. Never verified by being linked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefLink {
    pub kind: DefKind,
    /// Definition id inside its source (for example a `ComponentDef::id` or a KiCad item id).
    pub item_id: String,
    pub source: SourceId,
    pub maturity: LibraryMaturity,
    /// SHA-256 of the definition's canonical JSON, when the source holds the definition itself.
    pub content_hash: Option<String>,
}

/// A price break exactly as the source stated it (decimal text, never rounded).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceBreak {
    pub min_quantity: u64,
    pub unit_price: String,
    pub currency: String,
}

/// Stock and prices from one source at one moment. Always carries where and when it came from;
/// an offer without a retrieval time is not representable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OfferSnapshot {
    pub source: SourceId,
    /// The listing number at that supplier (not identity).
    pub supplier_pn: Option<String>,
    /// Present only when the source stated it.
    pub stock: Option<u64>,
    pub breaks: Vec<PriceBreak>,
    pub retrieved_at_unix_s: u64,
    /// The source's terms, shown next to the offer.
    pub terms_note: String,
}

/// Where a record came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: SourceId,
    /// The source's own reference for the record (an id, URL or file path as text).
    pub reference: String,
    pub retrieved_at_unix_s: Option<u64>,
    /// SHA-256 of the raw bytes the record was built from, when the source kept them.
    pub raw_sha256: Option<String>,
}

/// One source's record of a part.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartRecord {
    pub key: PartKey,
    /// MPN as the source wrote it.
    pub display_mpn: String,
    /// Manufacturer as the source wrote it.
    pub display_manufacturer: String,
    pub description: Option<String>,
    pub category: Option<String>,
    /// Package text as the source wrote it, for example "SOT-23-5".
    pub package: Option<String>,
    pub parameters: Vec<Parameter>,
    pub datasheets: Vec<DatasheetRef>,
    pub definitions: Vec<DefLink>,
    pub offers: Vec<OfferSnapshot>,
    /// Lifecycle text, only when the source states it.
    pub lifecycle: Option<String>,
    pub provenance: Provenance,
}

impl PartRecord {
    /// A record with only its identity and provenance; fill the rest field by field.
    pub fn new(
        manufacturer: &str,
        mpn: &str,
        provenance: Provenance,
    ) -> Result<PartRecord, PartKeyError> {
        Ok(PartRecord {
            key: PartKey::new(manufacturer, mpn)?,
            display_mpn: mpn.trim().to_string(),
            display_manufacturer: manufacturer.trim().to_string(),
            description: None,
            category: None,
            package: None,
            parameters: Vec::new(),
            datasheets: Vec::new(),
            definitions: Vec::new(),
            offers: Vec::new(),
            lifecycle: None,
            provenance,
        })
    }
}

/// Values of one field across a part's records, with the source of each.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldValues {
    pub field: String,
    /// `(value, source)` for every record that states the field.
    pub values: Vec<(String, SourceId)>,
}

/// A part merged across sources: every contributing record, kept as received.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub key: PartKey,
    pub records: Vec<PartRecord>,
}

impl Part {
    /// Merge records into parts by [`PartKey`]. Records with different keys never merge.
    /// Parts come out in key order; records keep their input order.
    pub fn merge(records: Vec<PartRecord>) -> Vec<Part> {
        let mut parts: BTreeMap<PartKey, Vec<PartRecord>> = BTreeMap::new();
        for r in records {
            parts.entry(r.key.clone()).or_default().push(r);
        }
        parts
            .into_iter()
            .map(|(key, records)| Part { key, records })
            .collect()
    }

    /// Sources that contributed, in record order, without repeats.
    pub fn sources(&self) -> Vec<SourceId> {
        let mut seen = BTreeSet::new();
        self.records
            .iter()
            .map(|r| r.provenance.source.clone())
            .filter(|s| seen.insert(s.clone()))
            .collect()
    }

    /// The MPN as the first record wrote it.
    pub fn display_mpn(&self) -> &str {
        self.records.first().map_or("", |r| r.display_mpn.as_str())
    }

    /// The manufacturer as the first record wrote it.
    pub fn display_manufacturer(&self) -> &str {
        self.records
            .first()
            .map_or("", |r| r.display_manufacturer.as_str())
    }

    /// Every stated value of the scalar fields (`description`, `category`, `package`,
    /// `lifecycle`) and of each named parameter (`parameter:<name>`), with its source.
    pub fn field_values(&self) -> Vec<FieldValues> {
        let mut out: BTreeMap<String, Vec<(String, SourceId)>> = BTreeMap::new();
        for r in &self.records {
            let src = &r.provenance.source;
            for (field, v) in [
                ("description", &r.description),
                ("category", &r.category),
                ("package", &r.package),
                ("lifecycle", &r.lifecycle),
            ] {
                if let Some(v) = v {
                    out.entry(field.to_string())
                        .or_default()
                        .push((v.clone(), src.clone()));
                }
            }
            for p in &r.parameters {
                out.entry(format!("parameter:{}", p.name))
                    .or_default()
                    .push((p.value.clone(), src.clone()));
            }
        }
        out.into_iter()
            .map(|(field, values)| FieldValues { field, values })
            .collect()
    }

    /// Fields whose stated values disagree between records (compared after trimming and
    /// case folding). Each conflict keeps every value and its source, for the UI to show.
    pub fn conflicts(&self) -> Vec<FieldValues> {
        self.field_values()
            .into_iter()
            .filter(|f| {
                f.values
                    .iter()
                    .map(|(v, _)| v.trim().to_lowercase())
                    .collect::<BTreeSet<_>>()
                    .len()
                    > 1
            })
            .collect()
    }

    /// Every definition candidate from every record.
    pub fn definitions(&self) -> Vec<&DefLink> {
        self.records.iter().flat_map(|r| &r.definitions).collect()
    }

    /// Every offer from every record (may be empty: no offer is ever invented).
    pub fn offers(&self) -> Vec<&OfferSnapshot> {
        self.records.iter().flat_map(|r| &r.offers).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prov(src: &str) -> Provenance {
        Provenance {
            source: SourceId::new(src),
            reference: "ref".into(),
            retrieved_at_unix_s: None,
            raw_sha256: None,
        }
    }

    #[test]
    fn key_normalises_and_requires_both_halves() {
        let a = PartKey::new("Texas Instruments", "TPS2051B-DBVR").unwrap();
        let b = PartKey::new("TEXAS-INSTRUMENTS", "tps2051bdbvr").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.mpn(), "TPS2051BDBVR");
        assert_eq!(PartKey::new("TI", " - "), Err(PartKeyError::EmptyMpn));
        assert_eq!(
            PartKey::new("  ", "TPS2051"),
            Err(PartKeyError::EmptyManufacturer)
        );
    }

    #[test]
    fn same_mpn_from_two_manufacturers_stays_two_parts() {
        let a = PartRecord::new("Maker A", "X100", prov("s1")).unwrap();
        let b = PartRecord::new("Maker B", "X100", prov("s1")).unwrap();
        let c = PartRecord::new("maker-a", "x-100", prov("s2")).unwrap();
        let parts = Part::merge(vec![a, b, c]);
        assert_eq!(parts.len(), 2);
        let a = parts
            .iter()
            .find(|p| p.key.manufacturer() == "makera")
            .unwrap();
        assert_eq!(a.records.len(), 2);
        assert_eq!(a.sources(), vec![SourceId::new("s1"), SourceId::new("s2")]);
        assert_eq!(a.display_mpn(), "X100");
    }

    #[test]
    fn disagreeing_values_are_kept_and_named_as_conflicts() {
        let mut a = PartRecord::new("M", "P1", prov("s1")).unwrap();
        a.package = Some("SOT-23-5".into());
        a.description = Some("Switch".into());
        let mut b = PartRecord::new("M", "P1", prov("s2")).unwrap();
        b.package = Some("SC-74A".into());
        b.description = Some("switch ".into());
        let part = Part::merge(vec![a, b]).remove(0);
        let conflicts = part.conflicts();
        assert_eq!(conflicts.len(), 1, "description differs only in case/space");
        assert_eq!(conflicts[0].field, "package");
        assert_eq!(
            conflicts[0].values,
            vec![
                ("SOT-23-5".to_string(), SourceId::new("s1")),
                ("SC-74A".to_string(), SourceId::new("s2")),
            ]
        );
        assert!(part.offers().is_empty(), "no offer is invented");
    }
}
