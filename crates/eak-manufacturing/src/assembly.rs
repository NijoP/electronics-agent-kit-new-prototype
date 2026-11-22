//! Fabricator assembly availability, kept separate from the general Distributor listing (ADR-0034).
//!
//! A part being listed at Distributor does not make it assemblable by Fabricator, and the classification
//! (basic / extended / preferred extended) is **never inferred** from a category or from the
//! part's presence in the Distributor catalogue. Distributor's documented OpenAPI carries no such field, so a
//! classification only enters EAK as an explicit [`AssemblyRecord`] with its source and
//! verification time: either attested by the user, or read from a parts-library CSV the user
//! obtained from Fabricator ([`parse_parts_csv`]). Without a record the class is `Unknown` and the
//! BOM warns; nothing here orders, quotes or uploads anything.
//!
//! [`warnings`] turns a BOM into structured, fail-closed warnings: missing Distributor number, missing
//! manufacturer, ambiguous MPN matching, missing footprint, unverified assembly compatibility
//! and missing or stale supplier data.

use crate::bom::BomLine;
use crate::matching::normalize_mpn;
use crate::sourcing::SourcingRecord;
use eak_validation::sourcing::is_distributor_part_number;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A supplier snapshot or assembly record older than this is reported as stale (30 days).
pub const STALE_AFTER_S: u64 = 30 * 24 * 3600;
const MAX_ROWS: usize = 500_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyClass {
    /// No record: not known, never assumed.
    Unknown,
    Basic,
    Extended,
    PreferredExtended,
    /// A record states Fabricator cannot assemble it.
    NotAssemblable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssemblyRecord {
    pub distributor: String,
    pub class: AssemblyClass,
    /// `user_attested` or `fabricator_parts_file:<file name>`.
    pub source: String,
    pub verified_at_unix_s: u64,
    pub stock: Option<u64>,
    /// Decimal text as written by the source.
    pub unit_price: Option<String>,
}

// ---------------------------------------------------------------------------------------
// Parts-library CSV
// ---------------------------------------------------------------------------------------

/// Split one CSV line (RFC 4180 quoting, `""` escapes).
fn split_csv(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => out.push(std::mem::take(&mut cur)),
            (c, _) => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

fn key(h: &str) -> String {
    h.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn class_of(value: &str, preferred: bool) -> AssemblyClass {
    match key(value).as_str() {
        "basic" => AssemblyClass::Basic,
        "preferredextended" | "preferred" => AssemblyClass::PreferredExtended,
        "extended" | "expansion" => {
            if preferred {
                AssemblyClass::PreferredExtended
            } else {
                AssemblyClass::Extended
            }
        }
        _ => AssemblyClass::Unknown,
    }
}

/// Read a Fabricator parts-library CSV. The header row is located by column names (any of:
/// `Distributor Part` / `Distributor Part #`, `Library Type` / `Basic/Extended` / `Type`, optional `Preferred`,
/// `Stock`, `Price`); a file without an Distributor column and a type column is refused rather than
/// guessed. Only rows whose Distributor number is in `wanted` are kept (when given). A row whose type
/// text is not recognised becomes `Unknown`, never a guess.
pub fn parse_parts_csv(
    text: &str,
    source_name: &str,
    now: u64,
    wanted: Option<&BTreeSet<String>>,
) -> Result<Vec<AssemblyRecord>, String> {
    let mut lines = text.lines();
    let header = lines.next().ok_or("the file is empty")?;
    let cols: Vec<String> = split_csv(header.trim_start_matches('\u{feff}'))
        .iter()
        .map(|h| key(h))
        .collect();
    let find = |names: &[&str]| cols.iter().position(|c| names.contains(&c.as_str()));
    let distributor_col = find(&["distributorpart", "distributorpartnumber", "distributor"])
        .ok_or(
            "no Distributor part column (expected `Distributor Part` or `Distributor Part #`)",
        )?;
    let type_col = find(&["librarytype", "basicextended", "type", "partclass"])
        .ok_or("no library-type column (expected `Library Type` or `Basic/Extended`)")?;
    let preferred_col = find(&["preferred", "ispreferred"]);
    let stock_col = find(&["stock", "quantity"]);
    let price_col = find(&["price", "unitprice"]);
    let mut out = vec![];
    for (n, line) in lines.enumerate() {
        if n >= MAX_ROWS {
            return Err(format!("more than {MAX_ROWS} rows"));
        }
        if line.trim().is_empty() {
            continue;
        }
        let f = split_csv(line);
        let Some(distributor) = f.get(distributor_col).map(|s| s.to_ascii_uppercase()) else {
            continue;
        };
        if !is_distributor_part_number(&distributor)
            || wanted.is_some_and(|w| !w.contains(&distributor))
        {
            continue;
        }
        let preferred = preferred_col
            .and_then(|c| f.get(c))
            .is_some_and(|v| matches!(key(v).as_str(), "yes" | "true" | "1" | "y"));
        out.push(AssemblyRecord {
            class: class_of(f.get(type_col).map_or("", String::as_str), preferred),
            distributor,
            source: format!("fabricator_parts_file:{source_name}"),
            verified_at_unix_s: now,
            stock: stock_col
                .and_then(|c| f.get(c))
                .and_then(|s| s.replace(',', "").parse().ok()),
            unit_price: price_col
                .and_then(|c| f.get(c))
                .filter(|p| p.parse::<f64>().is_ok_and(|v| v.is_finite() && v >= 0.0))
                .cloned(),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------
// Store (`<project>/sourcing/assembly/<Distributor>.json`)
// ---------------------------------------------------------------------------------------

pub fn assembly_dir(sourcing_dir: &Path) -> PathBuf {
    sourcing_dir.join("assembly")
}

/// The file a record is stored as, relative to the sourcing folder
/// (`assembly/<Distributor>.json`), and its bytes. In a project, hand both to
/// `Engine::write_project_record` (ADR-0039).
pub fn record_file(rec: &AssemblyRecord) -> Result<(String, Vec<u8>), String> {
    if !is_distributor_part_number(&rec.distributor) {
        return Err(format!(
            "{:?} is not an Distributor part number",
            rec.distributor
        ));
    }
    let text = serde_json::to_string_pretty(rec).map_err(|e| e.to_string())?;
    Ok((
        format!("assembly/{}.json", rec.distributor),
        text.into_bytes(),
    ))
}

/// Store a record in a sourcing folder that is not an open project's (tests, exports).
/// Project folders go through the engine.
pub fn write_record(sourcing_dir: &Path, rec: &AssemblyRecord) -> Result<(), String> {
    let (_, bytes) = record_file(rec)?;
    let dir = assembly_dir(sourcing_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(dir.join(format!("{}.json", rec.distributor)), bytes)
        .map_err(|e| format!("cannot write assembly record: {e}"))
}

/// Every readable record; unreadable files are skipped (and so count as "no record": Unknown).
pub fn read_records(sourcing_dir: &Path) -> Vec<AssemblyRecord> {
    let Ok(rd) = std::fs::read_dir(assembly_dir(sourcing_dir)) else {
        return vec![];
    };
    let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    files.sort();
    files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|t| serde_json::from_str::<AssemblyRecord>(&t).ok())
        .filter(|r| is_distributor_part_number(&r.distributor))
        .collect()
}

// ---------------------------------------------------------------------------------------
// Warnings
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    MissingDistributor,
    MissingManufacturer,
    AmbiguousMpn,
    MissingFootprint,
    UnverifiedAssembly,
    NotAssemblable,
    MissingSupplierData,
    StaleSupplierData,
    StaleAssemblyData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    pub kind: WarningKind,
    pub component_id: String,
    pub designators: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AssemblyRow {
    pub distributor: String,
    pub class: Option<AssemblyClass>,
    pub source: Option<String>,
    pub verified_at_unix_s: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AssemblyReport {
    pub rows: Vec<AssemblyRow>,
    pub warnings: Vec<Warning>,
}

/// Fail-closed warnings for the lines Fabricator would assemble (surface mount, placed on the
/// board) plus footprint problems for any line.
pub fn warnings(
    lines: &[BomLine],
    supplier: &[SourcingRecord],
    assembly: &[AssemblyRecord],
    now: u64,
) -> AssemblyReport {
    let by_distributor: BTreeMap<&str, &AssemblyRecord> = assembly
        .iter()
        .map(|a| (a.distributor.as_str(), a))
        .collect();
    let mut out = vec![];
    let mut rows = BTreeMap::new();
    for l in lines {
        let mut push = |kind, message: String| {
            out.push(Warning {
                kind,
                component_id: l.component_id.clone(),
                designators: l.designators.clone(),
                message,
            })
        };
        if l.footprint.trim().is_empty() || l.placed < l.quantity {
            push(
                WarningKind::MissingFootprint,
                format!(
                    "{} of {} designator(s) have no footprint on the PCB",
                    l.quantity - l.placed.min(l.quantity),
                    l.quantity
                ),
            );
        }
        if !l.assembled() {
            continue;
        }
        if l.manufacturer
            .as_deref()
            .is_none_or(|m| m.trim().is_empty())
        {
            push(
                WarningKind::MissingManufacturer,
                "no manufacturer recorded".into(),
            );
        }
        if let Some(mpn) = l.mpn.as_deref().filter(|m| !m.trim().is_empty()) {
            let n = normalize_mpn(mpn);
            let matches: BTreeSet<&str> = supplier
                .iter()
                .filter(|r| r.mpn.as_deref().map(normalize_mpn).as_deref() == Some(n.as_str()))
                .filter_map(|r| r.distributor_part.as_deref())
                .collect();
            if matches.len() > 1 {
                push(
                    WarningKind::AmbiguousMpn,
                    format!(
                        "MPN {mpn} matches {} stored Distributor listings ({}); the chosen number must be confirmed",
                        matches.len(),
                        matches.iter().copied().collect::<Vec<_>>().join(", ")
                    ),
                );
            } else if l.distributor.is_none() && matches.len() == 1 {
                push(
                    WarningKind::AmbiguousMpn,
                    format!(
                        "MPN {mpn} matches stored Distributor listing {} but no Distributor number is set on the part",
                        matches.iter().next().copied().unwrap_or("")
                    ),
                );
            }
        }
        let Some(distributor) = l.distributor.as_deref() else {
            push(
                WarningKind::MissingDistributor,
                "no Distributor part number: Fabricator cannot match this line".into(),
            );
            continue;
        };
        match l.price_as_of_unix_s {
            None => push(
                WarningKind::MissingSupplierData,
                format!("no stored supplier record for {distributor}: stock and price unknown"),
            ),
            Some(t) if now.saturating_sub(t) > STALE_AFTER_S => push(
                WarningKind::StaleSupplierData,
                format!(
                    "the stored supplier record for {distributor} is {} days old",
                    now.saturating_sub(t) / 86_400
                ),
            ),
            Some(_) => {}
        }
        let rec = by_distributor.get(distributor);
        rows.insert(
            distributor.to_string(),
            AssemblyRow {
                distributor: distributor.to_string(),
                class: rec.map(|r| r.class),
                source: rec.map(|r| r.source.clone()),
                verified_at_unix_s: rec.map(|r| r.verified_at_unix_s),
            },
        );
        match rec.map(|r| r.class) {
            None | Some(AssemblyClass::Unknown) => push(
                WarningKind::UnverifiedAssembly,
                format!(
                    "Fabricator assembly compatibility of {distributor} is not verified (basic/extended class unknown; \
                     being listed at Distributor does not make a part assemblable)"
                ),
            ),
            Some(AssemblyClass::NotAssemblable) => push(
                WarningKind::NotAssemblable,
                format!("a record states Fabricator cannot assemble {distributor}"),
            ),
            Some(_) => {
                if rec.is_some_and(|r| now.saturating_sub(r.verified_at_unix_s) > STALE_AFTER_S) {
                    push(
                        WarningKind::StaleAssemblyData,
                        format!("the assembly record for {distributor} is older than 30 days"),
                    );
                }
            }
        }
    }
    AssemblyReport {
        rows: rows.into_values().collect(),
        warnings: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sourcing::VerificationState;

    fn line(id: &str, distributor: Option<&str>) -> BomLine {
        BomLine {
            component_id: id.into(),
            value: "10k".into(),
            designators: vec!["R1".into(), "R2".into()],
            quantity: 2,
            footprint: "R_0603".into(),
            distributor: distributor.map(String::from),
            manufacturer: Some("UNI-ROYAL".into()),
            mpn: None,
            smd: true,
            placed: 2,
            stock: None,
            unit_price_usd: None,
            price_as_of_unix_s: Some(1_000),
            issues: vec![],
        }
    }

    fn supplier(distributor: &str, mpn: &str) -> SourcingRecord {
        SourcingRecord {
            provider: "Distributor".into(),
            source_url: "https://distributor.invalid/x".into(),
            retrieved_at_unix_s: 1_000,
            raw_sha256: distributor.into(),
            raw_bytes: 1,
            distributor_part: Some(distributor.into()),
            mpn: Some(mpn.into()),
            manufacturer: None,
            package: None,
            category: None,
            description: None,
            datasheet_url: None,
            specs: vec![],
            stock: None,
            price_breaks: vec![],
            missing: vec![],
            rejected: vec![],
            datasheet: None,
            attestation: None,
            state: VerificationState::SearchHit,
        }
    }

    fn kinds(r: &AssemblyReport) -> Vec<WarningKind> {
        r.warnings.iter().map(|w| w.kind).collect()
    }

    fn rec(distributor: &str, class: AssemblyClass, at: u64) -> AssemblyRecord {
        AssemblyRecord {
            distributor: distributor.into(),
            class,
            source: "user_attested".into(),
            verified_at_unix_s: at,
            stock: None,
            unit_price: None,
        }
    }

    #[test]
    fn a_listed_part_without_an_assembly_record_is_unverified_never_assumed_basic() {
        let r = warnings(&[line("R", Some("C25804"))], &[], &[], 2_000);
        assert!(kinds(&r).contains(&WarningKind::UnverifiedAssembly));
        assert_eq!(r.rows[0].class, None);
    }

    #[test]
    fn a_fresh_basic_record_clears_the_assembly_warning_only() {
        let r = warnings(
            &[line("R", Some("C25804"))],
            &[],
            &[rec("C25804", AssemblyClass::Basic, 1_900)],
            2_000,
        );
        assert!(!kinds(&r).contains(&WarningKind::UnverifiedAssembly));
        assert!(!kinds(&r).contains(&WarningKind::StaleAssemblyData));
        assert_eq!(r.rows[0].class, Some(AssemblyClass::Basic));
    }

    #[test]
    fn missing_distributor_manufacturer_footprint_and_supplier_data_each_warn() {
        let mut l = line("R", None);
        l.manufacturer = None;
        l.placed = 1;
        let r = warnings(&[l], &[], &[], 2_000);
        let k = kinds(&r);
        assert!(k.contains(&WarningKind::MissingDistributor));
        assert!(k.contains(&WarningKind::MissingManufacturer));
        assert!(k.contains(&WarningKind::MissingFootprint));
        let mut m = line("R", Some("C1"));
        m.price_as_of_unix_s = None;
        assert!(kinds(&warnings(&[m], &[], &[], 2_000)).contains(&WarningKind::MissingSupplierData));
    }

    #[test]
    fn stale_supplier_and_assembly_data_are_flagged() {
        let now = 1_000 + STALE_AFTER_S + 86_400;
        let r = warnings(
            &[line("R", Some("C1"))],
            &[],
            &[rec("C1", AssemblyClass::Extended, 1_000)],
            now,
        );
        let k = kinds(&r);
        assert!(k.contains(&WarningKind::StaleSupplierData));
        assert!(k.contains(&WarningKind::StaleAssemblyData));
    }

    #[test]
    fn an_mpn_matching_several_listings_is_ambiguous_and_one_listing_without_a_choice_warns() {
        let mut l = line("R", Some("C1"));
        l.mpn = Some("abc-1".into());
        let two = [supplier("C1", "ABC-1"), supplier("C2", "abc-1")];
        assert!(
            kinds(&warnings(&[l.clone()], &two, &[], 2_000)).contains(&WarningKind::AmbiguousMpn)
        );
        let mut none = line("R", None);
        none.mpn = Some("ABC-1".into());
        let one = [supplier("C1", "abc-1")];
        let r = warnings(&[none], &one, &[], 2_000);
        assert!(r
            .warnings
            .iter()
            .any(|w| w.kind == WarningKind::AmbiguousMpn && w.message.contains("C1")));
    }

    #[test]
    fn a_not_assemblable_record_warns_and_unplaced_lines_are_not_assembly_checked() {
        let r = warnings(
            &[line("R", Some("C1"))],
            &[],
            &[rec("C1", AssemblyClass::NotAssemblable, 1_900)],
            2_000,
        );
        assert!(kinds(&r).contains(&WarningKind::NotAssemblable));
        let mut tht = line("J", None);
        tht.smd = false;
        assert!(warnings(&[tht], &[], &[], 2_000).warnings.is_empty());
    }

    #[test]
    fn the_parts_csv_is_read_by_column_name_and_filtered_to_wanted_parts() {
        let csv =
            "\u{feff}Distributor Part,First Category,\"Library Type\",Preferred,Stock,Price\n\
                   C25804,Resistors,Basic,No,\"1,200\",0.0021\n\
                   C99999,ICs,Extended,Yes,5,1.5\n\
                   C11111,ICs,Extended,No,7,abc\n\
                   C22222,ICs,Mystery,No,,\n\
                   X1,bad,Basic,No,1,1\n";
        let wanted: BTreeSet<String> = ["C25804", "C99999", "C11111", "C22222"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let r = parse_parts_csv(csv, "parts.csv", 5, Some(&wanted)).unwrap();
        assert_eq!(r.len(), 4, "the malformed Distributor number is skipped");
        assert_eq!(r[0].class, AssemblyClass::Basic);
        assert_eq!(r[0].stock, Some(1200));
        assert_eq!(r[1].class, AssemblyClass::PreferredExtended);
        assert_eq!(r[2].unit_price, None, "a non-numeric price is dropped");
        assert_eq!(
            r[3].class,
            AssemblyClass::Unknown,
            "unrecognised text is never guessed"
        );
        assert_eq!(r[0].source, "fabricator_parts_file:parts.csv");
        let only = BTreeSet::from(["C25804".to_string()]);
        assert_eq!(parse_parts_csv(csv, "p", 5, Some(&only)).unwrap().len(), 1);
        assert!(parse_parts_csv("Name,Value\na,b\n", "p", 1, None).is_err());
        assert!(parse_parts_csv("Distributor Part\nC1\n", "p", 1, None).is_err());
    }

    #[test]
    fn records_round_trip_through_the_project_store() {
        let d = std::env::temp_dir().join(format!("eak-asm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        write_record(&d, &rec("C25804", AssemblyClass::Basic, 7)).unwrap();
        assert!(write_record(&d, &rec("../x", AssemblyClass::Basic, 7)).is_err());
        let back = read_records(&d);
        assert_eq!(back, vec![rec("C25804", AssemblyClass::Basic, 7)]);
    }
}
