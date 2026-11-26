//! MPN discovery and manufacturer identification.
//!
//! * [`identify_manufacturer`] maps a manufacturer name as someone wrote it to one canonical
//!   manufacturer through a small, hand-authored alias table ([`MANUFACTURERS`]). Matching is
//!   exact after `normalize_manufacturer`; anything not in the table is
//!   [`ManufacturerId::Unknown`]. Nothing is guessed: no fuzzy name matching, and no
//!   manufacturer is ever inferred from an MPN prefix.
//! * [`discover`] searches every ready [`PartSource`] for a full or partial MPN and keeps only
//!   records whose normalised MPN equals, starts with, or contains the normalised query, best
//!   first. A source that is not ready is never called and is reported as skipped.

use crate::part::{PartKey, PartRecord};
use crate::source::{PartQuery, PartSource, SourceOutcome, SourceStatus};
use eak_manufacturing::matching::{normalize_manufacturer, normalize_mpn};
use serde::{Deserialize, Serialize};

/// One manufacturer in the alias table, with where its names were taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ManufacturerAlias {
    /// The manufacturer's own name for itself.
    pub canonical: &'static str,
    /// Other names in common use for the same company (short names, legal-entity suffixes).
    /// Only names that refer to this company alone; brands of acquired companies are not
    /// folded in.
    pub aliases: &'static [&'static str],
    /// The manufacturer's own website, where the canonical name is published.
    pub source: &'static str,
}

/// The alias table. Deliberately short and conservative: an absent manufacturer is reported
/// as unknown, which is always safe; a wrong alias would merge two companies' parts.
pub const MANUFACTURERS: &[ManufacturerAlias] = &[
    ManufacturerAlias {
        canonical: "Texas Instruments",
        aliases: &[
            "TI",
            "Texas Instruments Incorporated",
            "Texas Instruments Inc",
        ],
        source: "https://www.ti.com",
    },
    ManufacturerAlias {
        canonical: "Analog Devices",
        aliases: &["ADI", "Analog Devices Inc", "Analog Devices, Inc."],
        source: "https://www.analog.com",
    },
    ManufacturerAlias {
        canonical: "STMicroelectronics",
        aliases: &["STMicro", "ST Microelectronics"],
        source: "https://www.st.com",
    },
    ManufacturerAlias {
        canonical: "Microchip Technology",
        aliases: &["Microchip", "Microchip Technology Inc"],
        source: "https://www.microchip.com",
    },
    ManufacturerAlias {
        canonical: "NXP Semiconductors",
        aliases: &["NXP"],
        source: "https://www.nxp.com",
    },
    ManufacturerAlias {
        canonical: "Infineon Technologies",
        aliases: &["Infineon", "Infineon Technologies AG"],
        source: "https://www.infineon.com",
    },
    ManufacturerAlias {
        canonical: "onsemi",
        aliases: &["ON Semiconductor", "ON Semi"],
        source: "https://www.onsemi.com",
    },
    ManufacturerAlias {
        canonical: "Renesas Electronics",
        aliases: &["Renesas"],
        source: "https://www.renesas.com",
    },
    ManufacturerAlias {
        canonical: "Espressif Systems",
        aliases: &["Espressif"],
        source: "https://www.espressif.com",
    },
    ManufacturerAlias {
        canonical: "Nordic Semiconductor",
        aliases: &["Nordic Semiconductor ASA"],
        source: "https://www.nordicsemi.com",
    },
    ManufacturerAlias {
        canonical: "Murata Manufacturing",
        aliases: &["Murata", "Murata Electronics"],
        source: "https://www.murata.com",
    },
    ManufacturerAlias {
        canonical: "Vishay Intertechnology",
        aliases: &["Vishay"],
        source: "https://www.vishay.com",
    },
    ManufacturerAlias {
        canonical: "ROHM Semiconductor",
        aliases: &["ROHM", "ROHM Co., Ltd."],
        source: "https://www.rohm.com",
    },
    ManufacturerAlias {
        canonical: "Diodes Incorporated",
        aliases: &["Diodes Inc"],
        source: "https://www.diodes.com",
    },
    ManufacturerAlias {
        canonical: "Würth Elektronik",
        aliases: &["Wurth Elektronik", "Wuerth Elektronik"],
        source: "https://www.we-online.com",
    },
    ManufacturerAlias {
        canonical: "Raspberry Pi Ltd",
        aliases: &["Raspberry Pi"],
        source: "https://www.raspberrypi.com",
    },
];

/// The result of identifying a manufacturer name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManufacturerId {
    /// The name is in the alias table.
    Known {
        canonical: String,
        /// The table entry the input matched (the canonical name or one alias).
        matched: String,
        /// Where the table entry's names come from.
        source: String,
    },
    /// The name is not in the alias table. Not an error, and never replaced by a guess.
    Unknown { input: String },
}

impl ManufacturerId {
    /// The canonical name when known.
    pub fn canonical(&self) -> Option<&str> {
        match self {
            ManufacturerId::Known { canonical, .. } => Some(canonical),
            ManufacturerId::Unknown { .. } => None,
        }
    }
}

/// Identify a manufacturer from a name as written, by exact match after normalisation against
/// the canonical names and aliases in [`MANUFACTURERS`].
pub fn identify_manufacturer(name: &str) -> ManufacturerId {
    let n = normalize_manufacturer(name);
    if !n.is_empty() {
        for m in MANUFACTURERS {
            for alias in std::iter::once(&m.canonical).chain(m.aliases.iter()) {
                if normalize_manufacturer(alias) == n {
                    return ManufacturerId::Known {
                        canonical: m.canonical.to_string(),
                        matched: alias.to_string(),
                        source: m.source.to_string(),
                    };
                }
            }
        }
    }
    ManufacturerId::Unknown {
        input: name.trim().to_string(),
    }
}

/// Whether two manufacturer names name the same company: same canonical entry when both are in
/// the table, otherwise the same normalised text.
pub fn same_manufacturer(a: &str, b: &str) -> bool {
    match (
        identify_manufacturer(a).canonical(),
        identify_manufacturer(b).canonical(),
    ) {
        (Some(x), Some(y)) => x == y,
        _ => {
            let (x, y) = (normalize_manufacturer(a), normalize_manufacturer(b));
            !x.is_empty() && x == y
        }
    }
}

/// How a record's MPN matched the query, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MpnMatch {
    /// Normalised MPN equals the normalised query.
    Exact,
    /// Normalised MPN starts with the query (a base part number without its suffix).
    Prefix,
    /// Normalised MPN contains the query elsewhere.
    Contains,
}

/// Classify a record's key against a normalised MPN query; `None` when it does not match.
pub fn match_mpn(query_norm: &str, key: &PartKey) -> Option<MpnMatch> {
    if query_norm.is_empty() {
        None
    } else if key.mpn() == query_norm {
        Some(MpnMatch::Exact)
    } else if key.mpn().starts_with(query_norm) {
        Some(MpnMatch::Prefix)
    } else if key.mpn().contains(query_norm) {
        Some(MpnMatch::Contains)
    } else {
        None
    }
}

/// One discovered record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MpnHit {
    pub matched: MpnMatch,
    /// The record's manufacturer, identified through the alias table.
    pub manufacturer: ManufacturerId,
    pub record: PartRecord,
}

/// Everything a discovery found and everything it did not ask or could not use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Discovery {
    /// The query after MPN normalisation.
    pub query: String,
    /// The requested manufacturer, identified, when one was given.
    pub manufacturer: Option<ManufacturerId>,
    /// Matching records, best match first, then by key and source.
    pub hits: Vec<MpnHit>,
    /// Records a source returned whose MPN does not contain the query (for example a
    /// description match). Counted, not shown as MPN hits.
    pub not_mpn_matches: usize,
    /// Records dropped because their manufacturer is not the requested one.
    pub other_manufacturer: usize,
    /// Sources skipped (not ready), failed, or that reported unusable listings.
    pub sources: Vec<SourceOutcome>,
    /// Why nothing was searched, when the query was refused.
    pub refused: Option<String>,
}

/// The shortest normalised query accepted; shorter partial MPNs match too much to be useful.
pub const MIN_QUERY_CHARS: usize = 3;

/// Search every ready source for a full or partial MPN, optionally narrowed to one
/// manufacturer (matched through the alias table, so "TI" finds "Texas Instruments" parts).
pub fn discover(
    sources: &[&dyn PartSource],
    mpn: &str,
    manufacturer: Option<&str>,
    limit: usize,
) -> Discovery {
    let query = normalize_mpn(mpn);
    let wanted = manufacturer
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(|m| (m.to_string(), identify_manufacturer(m)));
    let mut out = Discovery {
        query: query.clone(),
        manufacturer: wanted.as_ref().map(|(_, id)| id.clone()),
        hits: Vec::new(),
        not_mpn_matches: 0,
        other_manufacturer: 0,
        sources: Vec::new(),
        refused: None,
    };
    if query.chars().count() < MIN_QUERY_CHARS {
        out.refused = Some(format!(
            "an MPN query needs at least {MIN_QUERY_CHARS} characters after normalisation"
        ));
        return out;
    }
    let mut q = PartQuery::new(mpn);
    q.limit = limit;
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
        match s.search(&q) {
            Ok(answer) => {
                let mut unkeyed = answer.unkeyed;
                let total = answer.records.len();
                if total > limit {
                    unkeyed.push(format!(
                        "{} records over the limit of {limit} were not used",
                        total - limit
                    ));
                }
                for r in answer.records.into_iter().take(limit) {
                    let Some(matched) = match_mpn(&query, &r.key) else {
                        out.not_mpn_matches += 1;
                        continue;
                    };
                    let maker = &r.display_manufacturer;
                    let other = wanted
                        .as_ref()
                        .is_some_and(|(name, _)| !same_manufacturer(name, maker));
                    if other {
                        out.other_manufacturer += 1;
                        continue;
                    }
                    out.hits.push(MpnHit {
                        matched,
                        manufacturer: identify_manufacturer(&r.display_manufacturer),
                        record: r,
                    });
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
    out.hits.sort_by(|a, b| {
        a.matched
            .cmp(&b.matched)
            .then_with(|| a.record.key.cmp(&b.record.key))
            .then_with(|| a.record.provenance.source.cmp(&b.record.provenance.source))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::part::{Provenance, SourceId};
    use crate::source::{Capabilities, SourceAnswer, SourceError, SourceKind};
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Fixed {
        id: &'static str,
        status: SourceStatus,
        records: Vec<PartRecord>,
    }

    impl PartSource for Fixed {
        fn id(&self) -> SourceId {
            SourceId::new(self.id)
        }
        fn kind(&self) -> SourceKind {
            SourceKind::Local
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
        fn status(&self) -> SourceStatus {
            self.status.clone()
        }
        fn terms(&self) -> String {
            "test".into()
        }
        fn search(&self, _q: &PartQuery) -> Result<SourceAnswer, SourceError> {
            assert_eq!(
                self.status,
                SourceStatus::Ready,
                "a non-ready source was called"
            );
            Ok(SourceAnswer {
                records: self.records.clone(),
                unkeyed: Vec::new(),
            })
        }
        fn fetch(&self, _key: &PartKey) -> Result<Option<PartRecord>, SourceError> {
            Ok(None)
        }
    }

    fn rec(src: &str, maker: &str, mpn: &str) -> PartRecord {
        PartRecord::new(
            maker,
            mpn,
            Provenance {
                source: SourceId::new(src),
                reference: mpn.into(),
                retrieved_at_unix_s: None,
                raw_sha256: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn known_names_and_aliases_identify_one_manufacturer() {
        for name in ["TI", "texas instruments", "Texas-Instruments Inc."] {
            assert_eq!(
                identify_manufacturer(name).canonical(),
                Some("Texas Instruments"),
                "{name}"
            );
        }
        let on = identify_manufacturer("ON Semi");
        assert_eq!(on.canonical(), Some("onsemi"));
        let w = "Würth Elektronik";
        assert_eq!(identify_manufacturer(w).canonical(), Some(w));
    }

    #[test]
    fn unknown_names_are_unknown_never_guessed() {
        for name in ["Texas", "Acme Widgets", "T.I.X", "", "  "] {
            assert!(
                matches!(identify_manufacturer(name), ManufacturerId::Unknown { .. }),
                "{name:?} must not be identified"
            );
        }
        assert_eq!(
            identify_manufacturer(" Acme "),
            ManufacturerId::Unknown {
                input: "Acme".into()
            }
        );
    }

    #[test]
    fn alias_table_is_unambiguous_and_sourced() {
        let mut seen: BTreeMap<String, &str> = BTreeMap::new();
        for m in MANUFACTURERS {
            assert!(m.source.starts_with("https://"), "{}", m.canonical);
            for name in std::iter::once(&m.canonical).chain(m.aliases.iter()) {
                let n = normalize_manufacturer(name);
                assert!(!n.is_empty(), "{name}");
                if let Some(other) = seen.insert(n, m.canonical) {
                    assert_eq!(other, m.canonical, "{name} names two manufacturers");
                }
            }
        }
    }

    #[test]
    fn same_manufacturer_uses_aliases_then_exact_text() {
        assert!(same_manufacturer("TI", "Texas Instruments"));
        assert!(!same_manufacturer("TI", "Analog Devices"));
        assert!(same_manufacturer("Acme Co", "ACME-CO"));
        assert!(!same_manufacturer("Acme", "Acme Co"));
        assert!(!same_manufacturer("", " "));
    }

    #[test]
    fn mpn_match_kinds() {
        let k = PartKey::new("M", "TPS2051B-DBVR").unwrap();
        assert_eq!(match_mpn("TPS2051BDBVR", &k), Some(MpnMatch::Exact));
        assert_eq!(match_mpn("TPS2051", &k), Some(MpnMatch::Prefix));
        assert_eq!(match_mpn("2051B", &k), Some(MpnMatch::Contains));
        assert_eq!(match_mpn("LM358", &k), None);
        assert_eq!(match_mpn("", &k), None);
    }

    #[test]
    fn discovery_ranks_filters_by_manufacturer_and_skips_unready_sources() {
        let ready = Fixed {
            id: "local",
            status: SourceStatus::Ready,
            records: vec![
                rec("local", "Texas Instruments", "TPS2051B-DBVR"),
                rec("local", "Texas Instruments", "TPS2051"),
                rec("local", "Acme", "XTPS2051"),
                rec("local", "Texas Instruments", "LM358"),
            ],
        };
        let unready = Fixed {
            id: "remote",
            status: SourceStatus::NeedsSetup("no API key".into()),
            records: vec![rec("remote", "TI", "TPS2051")],
        };
        let d = discover(&[&ready, &unready], "tps-2051", None, 50);
        assert_eq!(d.query, "TPS2051");
        let got: Vec<(MpnMatch, &str)> = d
            .hits
            .iter()
            .map(|h| (h.matched, h.record.display_mpn.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                (MpnMatch::Exact, "TPS2051"),
                (MpnMatch::Prefix, "TPS2051B-DBVR"),
                (MpnMatch::Contains, "XTPS2051"),
            ]
        );
        assert_eq!(d.not_mpn_matches, 1);
        assert_eq!(d.sources.len(), 1);
        assert_eq!(d.sources[0].source, SourceId::new("remote"));
        assert!(matches!(
            d.hits[2].manufacturer,
            ManufacturerId::Unknown { .. }
        ));

        let d = discover(&[&ready], "TPS2051", Some("TI"), 50);
        assert_eq!(d.hits.len(), 2);
        assert_eq!(d.other_manufacturer, 1);
        assert_eq!(
            d.manufacturer.as_ref().and_then(|m| m.canonical()),
            Some("Texas Instruments")
        );
    }

    struct Counting(AtomicU32);

    impl PartSource for Counting {
        fn id(&self) -> SourceId {
            SourceId::new("counting")
        }
        fn kind(&self) -> SourceKind {
            SourceKind::Local
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
        fn status(&self) -> SourceStatus {
            SourceStatus::Ready
        }
        fn terms(&self) -> String {
            "test".into()
        }
        fn search(&self, _q: &PartQuery) -> Result<SourceAnswer, SourceError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(SourceAnswer::default())
        }
        fn fetch(&self, _key: &PartKey) -> Result<Option<PartRecord>, SourceError> {
            Ok(None)
        }
    }

    #[test]
    fn short_queries_are_refused_and_no_source_is_called() {
        let s = Counting(AtomicU32::new(0));
        let d = discover(&[&s], " t-p ", None, 50);
        assert!(d.refused.is_some());
        assert!(d.hits.is_empty() && d.sources.is_empty());
        assert_eq!(s.0.load(Ordering::SeqCst), 0);
        discover(&[&s], "tps", None, 50);
        assert_eq!(s.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn limit_is_reported_not_silent() {
        let s = Fixed {
            id: "local",
            status: SourceStatus::Ready,
            records: vec![rec("local", "M", "ABC1"), rec("local", "M", "ABC2")],
        };
        let d = discover(&[&s], "ABC", None, 1);
        assert_eq!(d.hits.len(), 1);
        assert_eq!(d.sources.len(), 1);
        assert!(d.sources[0].unkeyed[0].contains("over the limit"));
    }
}
