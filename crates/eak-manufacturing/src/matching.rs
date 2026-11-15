//! Deterministic, explainable matching of supplier listings against what the user asked for.
//!
//! Order of confidence (best first), each candidate carrying the reasons that put it there:
//!
//! 1. [`MatchKind::ExactLcsc`]: the query is an LCSC part number and the listing has it.
//! 2. [`MatchKind::ExactMpnManufacturer`]: the MPN is equal after normalisation **and** the
//!    manufacturer hint is equal after normalisation.
//! 3. [`MatchKind::ExactMpn`]: the MPN is equal after normalisation; the manufacturer was not
//!    given, or differs (`manufacturer_conflict`). Two manufacturers' products with the same
//!    MPN text are never merged: each stays its own candidate.
//! 4. [`MatchKind::Fuzzy`]: the supplier returned it for a loose reason only. Fuzzy results are
//!    suggestions; none of them is ever approved for engineering use by this module
//!    ([`Candidate::engineering_status`] is always `supplier_listing_only`).
//! 5. [`MatchKind::Browse`]: no query to match against (category browsing).
//!
//! Normalisation (`normalize_mpn`) folds case and removes whitespace and `-`, `_`, `/`, `.`. It is used only to *compare*; the manufacturer part number
//! stored on a record is never rewritten.

use crate::sourcing::SourcingRecord;
use eak_validation::sourcing::is_lcsc_part_number;
use serde::Serialize;

/// Comparison key: folds case and drops whitespace and the separators `-`, `_`, `/`, `.`
/// (including Unicode dash/minus variants), so `abc 123`, `ABC-123`, `abc_123` and `abc123` agree.
/// The stored and displayed MPN is never rewritten.
pub fn normalize_mpn(s: &str) -> String {
    s.chars()
        .filter(|c| {
            !c.is_whitespace()
                && !matches!(
                    c,
                    '-' | '_' | '/' | '.' | '\u{2010}'..='\u{2015}' | '\u{2212}'
                )
        })
        .flat_map(char::to_uppercase)
        .collect()
}

/// A manufacturer comparison key: lowercase alphanumerics only ("Texas Instruments" and
/// "TEXAS-INSTRUMENTS" agree; "TI" does not agree with either, on purpose).
pub fn normalize_manufacturer(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    ExactLcsc,
    ExactMpnManufacturer,
    ExactMpn,
    Fuzzy,
    Browse,
}

/// What the caller already knows and wants compatibility checked against. All optional.
#[derive(Debug, Clone, Default)]
pub struct Hints {
    pub manufacturer: Option<String>,
    pub package: Option<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub record: SourcingRecord,
    pub kind: MatchKind,
    /// `true` for the first three kinds; fuzzy and browse results are not exact.
    pub exact: bool,
    /// Why it is ranked where it is, one plain sentence each.
    pub reasons: Vec<String>,
    /// The MPN matched but the manufacturer hint disagrees.
    pub manufacturer_conflict: bool,
    pub package_match: Option<bool>,
    pub category_match: Option<bool>,
    /// A listing is not proof of suitability: always `supplier_listing_only`.
    pub engineering_status: &'static str,
}

/// Rank `records` for `query` (an LCSC number, an MPN or free text; empty = browsing).
/// Records with the same LCSC number are kept once. The order is total and deterministic.
pub fn rank(query: &str, hints: &Hints, records: Vec<SourcingRecord>) -> Vec<Candidate> {
    let q = query.trim();
    let q_lcsc = is_lcsc_part_number(&q.to_ascii_uppercase()).then(|| q.to_ascii_uppercase());
    let q_norm = normalize_mpn(q);
    let hint_maker = hints
        .manufacturer
        .as_deref()
        .map(normalize_manufacturer)
        .filter(|m| !m.is_empty());
    let mut seen = std::collections::BTreeSet::new();
    let mut out: Vec<Candidate> = vec![];
    for record in records {
        if let Some(l) = &record.lcsc_part {
            if !seen.insert(l.clone()) {
                continue;
            }
        }
        let mut reasons = vec![];
        let mut conflict = false;
        let mpn_norm = record.mpn.as_deref().map(normalize_mpn);
        let maker_norm = record.manufacturer.as_deref().map(normalize_manufacturer);

        let kind = if q.is_empty() {
            reasons.push("category browse: no query to match".to_string());
            MatchKind::Browse
        } else if q_lcsc.is_some() && record.lcsc_part == q_lcsc {
            reasons.push(format!(
                "LCSC part number {} matches exactly",
                q.to_ascii_uppercase()
            ));
            MatchKind::ExactLcsc
        } else if mpn_norm.as_deref() == Some(q_norm.as_str()) {
            let literal = record.mpn.as_deref() == Some(q);
            reasons.push(if literal {
                "manufacturer part number is identical".to_string()
            } else {
                format!(
                    "manufacturer part number equals the query after case/space/dash \
                     normalisation (LCSC text {:?} kept unchanged)",
                    record.mpn.as_deref().unwrap_or("")
                )
            });
            match (&hint_maker, &maker_norm) {
                (Some(h), Some(m)) if h == m => {
                    reasons.push(format!(
                        "manufacturer {:?} matches the one you gave",
                        record.manufacturer.as_deref().unwrap_or("")
                    ));
                    MatchKind::ExactMpnManufacturer
                }
                (Some(_), Some(_)) => {
                    conflict = true;
                    reasons.push(format!(
                        "manufacturer is {:?}, not {:?}; the same MPN text from another \
                         manufacturer is a different product",
                        record.manufacturer.as_deref().unwrap_or(""),
                        hints.manufacturer.as_deref().unwrap_or("")
                    ));
                    MatchKind::ExactMpn
                }
                (Some(_), None) => {
                    conflict = true;
                    reasons.push(
                        "the listing names no manufacturer, so the one you gave cannot be \
                         confirmed"
                            .to_string(),
                    );
                    MatchKind::ExactMpn
                }
                (None, _) => {
                    reasons.push(
                        "no manufacturer given: check which manufacturer's part you want"
                            .to_string(),
                    );
                    MatchKind::ExactMpn
                }
            }
        } else {
            match &mpn_norm {
                Some(m) if q_norm.len() >= 3 && m.contains(&q_norm) => reasons.push(format!(
                    "the query is part of the manufacturer part number {:?}",
                    record.mpn.as_deref().unwrap_or("")
                )),
                Some(m) if m.len() >= 3 && q_norm.contains(m.as_str()) => reasons.push(format!(
                    "manufacturer part number {:?} is part of the query",
                    record.mpn.as_deref().unwrap_or("")
                )),
                _ => reasons.push(
                    "returned by LCSC's keyword search on its listing text; the manufacturer \
                     part number does not match the query"
                        .to_string(),
                ),
            }
            reasons.push("fuzzy suggestion: not an exact match".to_string());
            MatchKind::Fuzzy
        };

        let package_match = hints
            .package
            .as_deref()
            .filter(|p| !p.trim().is_empty())
            .map(|p| {
                record
                    .package
                    .as_deref()
                    .is_some_and(|rp| rp.trim().to_lowercase() == p.trim().to_lowercase())
            });
        if package_match == Some(false) {
            reasons.push(format!(
                "package {:?} differs from the requested {:?}",
                record.package.as_deref().unwrap_or("not stated"),
                hints.package.as_deref().unwrap_or("")
            ));
        }
        let category_match = hints
            .category
            .as_deref()
            .filter(|c| !c.trim().is_empty())
            .map(|c| {
                record
                    .category
                    .as_deref()
                    .is_some_and(|rc| rc.to_lowercase().contains(&c.trim().to_lowercase()))
            });
        if category_match == Some(false) {
            reasons.push("category differs from the requested one".to_string());
        }
        out.push(Candidate {
            record,
            exact: kind <= MatchKind::ExactMpn,
            kind,
            reasons,
            manufacturer_conflict: conflict,
            package_match,
            category_match,
            engineering_status: "supplier_listing_only",
        });
    }
    out.sort_by_cached_key(|c| {
        (
            c.kind,
            c.manufacturer_conflict,
            c.package_match == Some(false),
            c.category_match == Some(false),
            c.record.manufacturer.clone().unwrap_or_default(),
            c.record.lcsc_part.clone().unwrap_or_default(),
            c.record.raw_sha256.clone(),
        )
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sourcing::VerificationState;

    fn rec(lcsc: &str, mpn: &str, maker: &str, package: &str) -> SourcingRecord {
        SourcingRecord {
            provider: "LCSC".into(),
            source_url: "https://api.lcsc.com/x".into(),
            retrieved_at_unix_s: 1,
            raw_sha256: format!("sha-{lcsc}"),
            raw_bytes: 1,
            lcsc_part: Some(lcsc.into()),
            mpn: Some(mpn.into()),
            manufacturer: Some(maker.into()),
            package: Some(package.into()),
            category: Some("Resistors / Chip Resistor - Surface Mount".into()),
            description: None,
            datasheet_url: None,
            specs: vec![],
            stock: Some(1),
            price_breaks: vec![],
            missing: vec![],
            rejected: vec![],
            datasheet: None,
            attestation: None,
            state: VerificationState::SearchHit,
        }
    }

    #[test]
    fn normalisation_folds_case_space_and_dashes_only() {
        assert_eq!(normalize_mpn(" stm32 f103–c8t6 "), "STM32F103C8T6");
        for v in ["abc 123", "ABC-123", "abc_123", "abc123", "a.b/c-123"] {
            assert_eq!(normalize_mpn(v), "ABC123", "{v}");
        }
        assert_eq!(normalize_mpn("0603WAF1002T5E"), "0603WAF1002T5E");
        assert_eq!(
            normalize_manufacturer("Texas-Instruments"),
            "texasinstruments"
        );
        assert_ne!(
            normalize_manufacturer("TI"),
            normalize_manufacturer("Texas Instruments")
        );
    }

    #[test]
    fn an_lcsc_number_beats_everything() {
        let c = rank(
            "c25804",
            &Hints::default(),
            vec![
                rec("C1", "C25804X", "A", "0603"),
                rec("C25804", "R1", "B", "0603"),
            ],
        );
        assert_eq!(c[0].kind, MatchKind::ExactLcsc);
        assert_eq!(c[0].record.lcsc_part.as_deref(), Some("C25804"));
        assert!(c[0].exact);
        assert_eq!(c[1].kind, MatchKind::Fuzzy);
        assert!(!c[1].exact);
    }

    #[test]
    fn exact_mpn_with_the_right_manufacturer_ranks_above_exact_mpn_alone() {
        let hints = Hints {
            manufacturer: Some("uni-royal".into()),
            ..Hints::default()
        };
        let c = rank(
            "0603WAF1002T5E",
            &hints,
            vec![
                rec("C2", "0603WAF1002T5E", "OTHER CO", "0603"),
                rec("C1", "0603WAF1002T5E", "UNI ROYAL", "0603"),
            ],
        );
        assert_eq!(c[0].kind, MatchKind::ExactMpnManufacturer);
        assert_eq!(c[0].record.lcsc_part.as_deref(), Some("C1"));
        assert_eq!(c[1].kind, MatchKind::ExactMpn);
        assert!(c[1].manufacturer_conflict);
        assert!(c[1].reasons.iter().any(|r| r.contains("different product")));
        assert_eq!(c.len(), 2, "two manufacturers are never merged");
    }

    #[test]
    fn a_normalised_equal_mpn_is_exact_and_says_the_text_was_kept() {
        let c = rank(
            "stm32f103 c8t6",
            &Hints::default(),
            vec![rec("C8734", "STM32F103C8T6", "ST", "LQFP-48")],
        );
        assert_eq!(c[0].kind, MatchKind::ExactMpn);
        assert!(c[0].reasons[0].contains("kept unchanged"));
        assert_eq!(c[0].record.mpn.as_deref(), Some("STM32F103C8T6"));
    }

    #[test]
    fn partial_mpn_hits_are_fuzzy_and_never_approved() {
        let c = rank(
            "STM32F103",
            &Hints::default(),
            vec![rec("C8734", "STM32F103C8T6", "ST", "LQFP-48")],
        );
        assert_eq!(c[0].kind, MatchKind::Fuzzy);
        assert!(!c[0].exact);
        assert_eq!(c[0].engineering_status, "supplier_listing_only");
        assert!(c[0]
            .reasons
            .iter()
            .any(|r| r.contains("part of the manufacturer part number")));
    }

    #[test]
    fn package_and_category_hints_demote_but_do_not_hide() {
        let hints = Hints {
            package: Some("0805".into()),
            category: Some("capacitor".into()),
            ..Hints::default()
        };
        let c = rank(
            "R1",
            &hints,
            vec![rec("C2", "R1", "A", "0603"), rec("C1", "R1", "A", "0805")],
        );
        assert_eq!(c[0].record.lcsc_part.as_deref(), Some("C1"));
        assert_eq!(c[0].package_match, Some(true));
        assert_eq!(c[1].package_match, Some(false));
        assert_eq!(c[1].category_match, Some(false));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn an_empty_query_is_a_browse_and_duplicates_collapse() {
        let c = rank(
            "",
            &Hints::default(),
            vec![
                rec("C1", "A", "M", "0603"),
                rec("C1", "A", "M", "0603"),
                rec("C2", "B", "M", "0603"),
            ],
        );
        assert_eq!(c.len(), 2);
        assert!(c.iter().all(|x| x.kind == MatchKind::Browse && !x.exact));
        assert_eq!(c[0].record.lcsc_part.as_deref(), Some("C1"));
    }

    #[test]
    fn ranking_is_deterministic_regardless_of_input_order() {
        let a = vec![
            rec("C3", "X", "M", "0603"),
            rec("C1", "X", "M", "0603"),
            rec("C2", "X", "M", "0603"),
        ];
        let mut b = a.clone();
        b.reverse();
        let ids = |v: Vec<Candidate>| {
            v.into_iter()
                .map(|c| c.record.lcsc_part.unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(rank("X", &Hints::default(), a)),
            ids(rank("X", &Hints::default(), b))
        );
    }
}
