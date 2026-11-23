//! Sourcing checks: library identity, missing data, BOM-to-Distributor mapping and
//! symbol / footprint / supplier-record compatibility.
//!
//! This report is independent of ERC/DRC/MECH and never gates them: supplier data
//! (availability, price, a catalogue listing) cannot make a design electrically correct
//! or incorrect. Missing or unverified sourcing data is always reported; it is never a pass.
//! See `docs/engineering/sourcing.md`.

use crate::report::{finding, rule, Finding, Report, Severity::*};
use eak_core::{ComponentDef, Id, Project};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Confidence in a supplier record. A search hit is never a verified part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerificationState {
    /// Supplier catalogue data only. Nothing has been checked.
    #[default]
    SearchHit,
    /// A hashed datasheet file is recorded and names the manufacturer part number.
    DatasheetMatched,
    /// A human reviewer attested the record against the hashed datasheet.
    Verified,
}

/// The supplier-side facts this check needs about one part, keyed by Distributor part number.
/// `eak-manufacturing::sourcing::SourcingRecord` produces these; nothing here is inferred.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SupplierPart {
    pub distributor: String,
    pub mpn: Option<String>,
    pub package: Option<String>,
    pub datasheet_sha256: Option<String>,
    pub state: VerificationState,
}

/// One BOM line: a component definition at one value, with its Distributor mapping if it has one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BomRow {
    pub component_id: String,
    pub value: String,
    pub refs: Vec<String>,
    /// `None` = unmapped (no well-formed Distributor part number in the library definition).
    pub distributor: Option<String>,
}

/// Distributor part numbers are the letter `C` followed by decimal digits.
pub fn is_distributor_part_number(s: &str) -> bool {
    let b = s.as_bytes();
    (2..=13).contains(&b.len()) && b[0] == b'C' && b[1..].iter().all(u8::is_ascii_digit)
}

fn present(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// BOM rows grouped exactly like the manufacturing BOM (component definition + value).
pub fn bom_rows(p: &Project) -> Vec<BomRow> {
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for s in p.schematic.symbols.values() {
        groups
            .entry((s.component_id.clone(), s.value.clone()))
            .or_default()
            .push(s.refdes.clone());
    }
    groups
        .into_iter()
        .map(|((component_id, value), mut refs)| {
            refs.sort();
            let distributor = p
                .library
                .get(&component_id)
                .and_then(|d| present(&d.distributor))
                .filter(|l| is_distributor_part_number(l))
                .map(String::from);
            BomRow {
                component_id,
                value,
                refs,
                distributor,
            }
        })
        .collect()
}

/// Package family and declared lead count of one designation token, e.g.
/// `SOIC-8` -> (`SOIC`, 8), `SOT-223-3` -> (`SOT-223`, 3), `SOT-223` -> (`SOT-223`, none),
/// `0603` -> (`0603`, 2). Only the designation text is read; no package table is consulted.
fn family_and_leads(token: &str) -> (String, Option<u32>) {
    // Families whose own name contains a number: the lead count is the part after it.
    const NUMBERED: [&str; 5] = ["SOT", "TO", "SOD", "SC", "DO"];
    let parts: Vec<&str> = token.split('-').collect();
    let num = |i: usize| parts.get(i).and_then(|s| s.parse::<u32>().ok());
    if NUMBERED.contains(&parts[0]) && parts.len() >= 2 {
        return (format!("{}-{}", parts[0], parts[1]), num(2));
    }
    if !parts[0].is_empty() && parts[0].chars().all(|c| c.is_ascii_alphabetic()) {
        return (parts[0].to_string(), num(1));
    }
    // EIA chip size codes (0402, 0603, ...) designate two-terminal chip components.
    if token.len() == 4 && token.chars().all(|c| c.is_ascii_digit()) {
        return (token.to_string(), Some(2));
    }
    (token.to_string(), None)
}

fn tokens(designation: &str) -> Vec<String> {
    designation
        .to_ascii_uppercase()
        .split(|c: char| c == '_' || c == ',' || c == '(' || c == ')' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

/// Textual comparison of a footprint name with a supplier package designation.
fn package_matches(footprint: &str, package: &str) -> bool {
    let Some(first) = tokens(package).into_iter().next() else {
        return false;
    };
    let (fam, leads) = family_and_leads(&first);
    tokens(footprint).iter().any(|t| {
        let (f, l) = family_and_leads(t);
        f == fam && (l == leads || l.is_none() || leads.is_none())
    })
}

fn check_component(
    def: &ComponentDef,
    rows: &[&BomRow],
    objects: &[Id],
    part: Option<&SupplierPart>,
    f: &mut Vec<Finding>,
) {
    let id = &def.id;
    let refs = rows
        .iter()
        .flat_map(|r| r.refs.iter().cloned())
        .collect::<Vec<_>>()
        .join(",");
    let mut push = |rule_id: &str, sev, msg: String| {
        f.push(finding(rule_id, sev, msg, objects.to_vec(), None));
    };

    // ---- library data that must exist (missing is reported, never assumed)
    let mpn = present(&def.mpn);
    if mpn.is_none() {
        push("SRC-001", Error, format!("{id} ({refs}): MPN is MISSING"));
    }
    let distributor = present(&def.distributor);
    match distributor {
        None => push(
            "SRC-002",
            Error,
            format!("{id} ({refs}): Distributor part number is MISSING; BOM row is unmapped"),
        ),
        Some(l) if !is_distributor_part_number(l) => push(
            "SRC-002",
            Error,
            format!(
                "{id} ({refs}): Distributor part number {l:?} is malformed; BOM row is unmapped"
            ),
        ),
        Some(_) => {}
    }
    if present(&def.datasheet).is_none() {
        push(
            "SRC-003",
            Error,
            format!("{id} ({refs}): datasheet reference is MISSING"),
        );
    }
    if def.ratings.is_empty() {
        push(
            "SRC-004",
            Error,
            format!("{id} ({refs}): electrical ratings are MISSING"),
        );
    } else {
        let unverified: Vec<&str> = def
            .ratings
            .iter()
            .filter(|r| !r.verified)
            .map(|r| r.name.as_str())
            .collect();
        if !unverified.is_empty() {
            push(
                "SRC-004",
                Warning,
                format!(
                    "{id} ({refs}): ratings not verified against a datasheet: {}",
                    unverified.join(", ")
                ),
            );
        }
    }

    // ---- symbol vs footprint
    for problem in def.validate() {
        push("SRC-009", Error, problem);
    }
    let mapped: BTreeSet<&String> = def.pin_map.values().flatten().collect();
    let pads: BTreeSet<&String> = def.footprint.pads.iter().map(|p| &p.number).collect();
    let unmapped: Vec<&str> = pads
        .iter()
        .filter(|n| !mapped.contains(*n))
        .map(|n| n.as_str())
        .collect();
    if !unmapped.is_empty() {
        push(
            "SRC-009",
            Warning,
            format!(
                "{id}: footprint pad(s) {} are not mapped to any symbol pin ({} symbol pins, {} pads)",
                unmapped.join(", "),
                def.symbol.pins.len(),
                pads.len()
            ),
        );
    }

    // ---- one orderable part has one value
    let values: BTreeSet<&str> = rows.iter().map(|r| r.value.as_str()).collect();
    if distributor.is_some() && values.len() > 1 {
        push(
            "SRC-012",
            Error,
            format!(
                "{id}: one Distributor part number is used for {} different values ({}); each value needs its own component definition",
                values.len(),
                values.into_iter().collect::<Vec<_>>().join(", ")
            ),
        );
    }

    // ---- supplier record
    let Some(distributor) = distributor.filter(|l| is_distributor_part_number(l)) else {
        return;
    };
    let Some(part) = part else {
        push(
            "SRC-005",
            Error,
            format!("{id} ({refs}): no sourcing record for {distributor}; the part is UNVERIFIED"),
        );
        return;
    };
    match part.state {
        VerificationState::SearchHit => push(
            "SRC-006",
            Error,
            format!("{id}: sourcing record for {distributor} is a search hit only, not a verified part"),
        ),
        VerificationState::DatasheetMatched => push(
            "SRC-006",
            Warning,
            format!(
                "{id}: sourcing record for {distributor} is datasheet-matched but not reviewer-verified"
            ),
        ),
        VerificationState::Verified => {}
    }
    match (mpn, present(&part.mpn)) {
        (_, None) => push(
            "SRC-007",
            Error,
            format!("{id}: sourcing record for {distributor} has no MPN (MISSING)"),
        ),
        (Some(lib), Some(rec)) if !lib.eq_ignore_ascii_case(rec) => push(
            "SRC-007",
            Error,
            format!("{id}: library MPN {lib:?} differs from sourcing record MPN {rec:?} ({distributor})"),
        ),
        _ => {}
    }
    match present(&part.package) {
        None => {
            push(
                "SRC-008",
                Error,
                format!("{id}: sourcing record for {distributor} has no package (MISSING)"),
            );
        }
        Some(pkg) => {
            if !package_matches(&def.footprint.name, pkg) {
                push(
                    "SRC-008",
                    Error,
                    format!(
                        "{id}: footprint {:?} does not match supplier package {pkg:?} ({distributor}); review against the datasheet",
                        def.footprint.name
                    ),
                );
            }
            let first = tokens(pkg).into_iter().next().unwrap_or_default();
            let exposed_pad = first.split('-').any(|s| s == "EP");
            let n = pads.len() as u32;
            match family_and_leads(&first).1 {
                None => push(
                    "SRC-010",
                    Warning,
                    format!(
                        "{id}: lead count is not stated by package designation {pkg:?}; pad count ({n}) is UNKNOWN against the package"
                    ),
                ),
                Some(leads) if n == leads && exposed_pad => push(
                    "SRC-010",
                    Warning,
                    format!(
                        "{id}: package {pkg:?} declares an exposed pad but the footprint has only {n} pads"
                    ),
                ),
                Some(leads) if n == leads + 1 && !exposed_pad => push(
                    "SRC-010",
                    Warning,
                    format!(
                        "{id}: footprint has {n} pads for {leads}-lead package {pkg:?}; confirm the extra pad (tab / exposed pad) against the datasheet"
                    ),
                ),
                Some(leads) if n != leads && n != leads + 1 => push(
                    "SRC-010",
                    Error,
                    format!("{id}: footprint has {n} pads but package {pkg:?} declares {leads} leads"),
                ),
                Some(_) => {}
            }
        }
    }
    if present(&part.datasheet_sha256).is_none() {
        push(
            "SRC-011",
            Error,
            format!("{id}: no datasheet file hash is recorded for {distributor}; the datasheet was not retrieved"),
        );
    }
}

/// Run the sourcing check for every component used in the schematic.
/// `parts` are the supplier records available offline; with none, every mapped row is
/// reported as having no sourcing record.
pub fn run(p: &Project, parts: &[SupplierPart]) -> Report {
    let rows = bom_rows(p);
    let mut f = Vec::new();
    let mut symbols: BTreeMap<&str, Vec<Id>> = BTreeMap::new();
    for s in p.schematic.symbols.values() {
        symbols
            .entry(s.component_id.as_str())
            .or_default()
            .push(s.id.clone());
    }
    for (component_id, objects) in &symbols {
        // Dangling component references are an integrity error, reported by eak-core.
        let Some(def) = p.library.get(*component_id) else {
            continue;
        };
        let own: Vec<&BomRow> = rows
            .iter()
            .filter(|r| r.component_id == *component_id)
            .collect();
        let part =
            present(&def.distributor).and_then(|l| parts.iter().find(|s| s.distributor == l));
        check_component(def, &own, objects, part, &mut f);
    }
    let unmapped = rows.iter().filter(|r| r.distributor.is_none()).count();
    let rules = vec![
        rule("SRC-001", "Manufacturer part number present", "", "EAK"),
        rule(
            "SRC-002",
            "BOM row mapped to an Distributor part number",
            format!("{} of {} BOM rows unmapped", unmapped, rows.len()),
            "EAK; Distributor part number format C<digits>",
        ),
        rule("SRC-003", "Datasheet reference present", "", "EAK"),
        rule(
            "SRC-004",
            "Electrical ratings present and verified",
            "missing = error, unverified = warning",
            "Library rating provenance (never supplier data)",
        ),
        rule(
            "SRC-005",
            "Sourcing record exists for the Distributor part number",
            format!("{} sourcing record(s) supplied", parts.len()),
            "EAK",
        ),
        rule(
            "SRC-006",
            "Sourcing record verification state",
            "search hit = error, datasheet-matched = warning",
            "EAK: a search hit is never a verified part",
        ),
        rule(
            "SRC-007",
            "Library MPN equals sourcing record MPN",
            "case-insensitive",
            "EAK",
        ),
        rule(
            "SRC-008",
            "Footprint package equals supplier package",
            "textual: family and lead count of the designation",
            "EAK; aliases are not resolved",
        ),
        rule(
            "SRC-009",
            "Symbol pins map to footprint pads",
            "library structural validation + unmapped pads",
            "EAK",
        ),
        rule(
            "SRC-010",
            "Footprint pad count equals package lead count",
            "lead count read from the package designation only",
            "EAK; EIA chip size codes are two-terminal",
        ),
        rule("SRC-011", "Datasheet file hash recorded", "SHA-256", "EAK"),
        rule(
            "SRC-012",
            "One Distributor part number per component value",
            "",
            "EAK",
        ),
    ];
    Report::new(
        "SOURCING",
        p,
        rules,
        f,
        vec![
            "Live Distributor / Fabricator retrieval (no network client in this build; records are supplied offline)".into(),
            "Datasheet content extraction: ratings, pinout and land-pattern dimensions are not compared against the datasheet".into(),
            "Stock, price and lifecycle (NRND / EOL) checks".into(),
            "Package alias resolution (JEDEC / EIAJ / manufacturer names)".into(),
            "Second-source and alternative part checks".into(),
        ],
        vec![
            "Package comparison is textual: differently named equivalents (for example SOP-8 and SOIC-8) are reported as mismatches for review".into(),
            "Lead counts are read from the package designation; designations that do not state one are reported as unknown".into(),
            "Sourcing records are supplied by the caller and are not authenticated by this check".into(),
            "No errors here does not mean the parts are in stock, orderable, or electrically suitable".into(),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Severity;
    use eak_core::{builtin, Point, Rotation, SymbolInstance};

    fn project(parts: &[(&str, &str, &str)]) -> Project {
        let mut p = Project::new("src");
        let sheet = p.first_sheet_id().unwrap().clone();
        for (i, (comp, refdes, value)) in parts.iter().enumerate() {
            p.library
                .entry(comp.to_string())
                .or_insert_with(|| builtin::get(comp).unwrap());
            let id = format!("sym_{i}");
            p.schematic.symbols.insert(
                id.clone(),
                SymbolInstance {
                    id,
                    sheet_id: sheet.clone(),
                    component_id: comp.to_string(),
                    refdes: refdes.to_string(),
                    value: value.to_string(),
                    position: Point::mm(0.0, 0.0),
                    rotation: Rotation::R0,
                    mirror: false,
                    field_visibility: Default::default(),
                },
            );
        }
        p
    }

    fn has(r: &Report, rule: &str, sev: Severity) -> bool {
        r.findings
            .iter()
            .any(|f| f.rule_id == rule && f.severity == sev)
    }

    fn ldo_part(state: VerificationState) -> SupplierPart {
        SupplierPart {
            distributor: "C6186".into(),
            mpn: Some("ams1117-3.3".into()),
            package: Some("SOT-223-3".into()),
            datasheet_sha256: Some("00".repeat(32)),
            state,
        }
    }

    #[test]
    fn distributor_part_number_format() {
        assert!(is_distributor_part_number("C6186"));
        for bad in [
            "",
            "C",
            "c6186",
            "C61 86",
            "6186",
            "C6186x",
            "C12345678901234",
        ] {
            assert!(!is_distributor_part_number(bad), "{bad}");
        }
    }

    #[test]
    fn package_designations_are_compared_textually() {
        assert!(package_matches("SOT-223-3_TabPin2", "SOT-223"));
        assert!(package_matches("SOT-223-3_TabPin2", "SOT-223-3"));
        assert!(package_matches("SOIC-8_3.9x4.9mm_P1.27mm", "soic-8"));
        assert!(package_matches("R_0603", "0603"));
        assert!(!package_matches("R_0603", "0805"));
        assert!(!package_matches("SOIC-8_3.9x4.9mm_P1.27mm", "SOIC-14"));
        // Aliases are not resolved: reported for review rather than assumed equal.
        assert!(!package_matches("SOIC-8_3.9x4.9mm_P1.27mm", "SOP-8"));
        assert!(!package_matches("SOT-223-3_TabPin2", ""));
        assert_eq!(family_and_leads("SOT-23-5"), ("SOT-23".into(), Some(5)));
        assert_eq!(family_and_leads("QFN-32-EP"), ("QFN".into(), Some(32)));
        assert_eq!(family_and_leads("SOD-123"), ("SOD-123".into(), None));
    }

    #[test]
    fn missing_data_is_reported_and_rows_are_unmapped() {
        let p = project(&[("R_0603", "R1", "10k"), ("CONN_1x04_P2.54", "J1", "IO")]);
        let rows = bom_rows(&p);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.distributor.is_none()));
        let r = run(&p, &[]);
        assert_eq!(r.check, "SOURCING");
        assert_eq!(r.outcome, crate::Outcome::Errors);
        for rule in ["SRC-001", "SRC-002", "SRC-003"] {
            assert_eq!(
                r.findings.iter().filter(|f| f.rule_id == rule).count(),
                2,
                "{rule}"
            );
        }
        // The header has no ratings at all; the resistor's rating exists but is unverified.
        assert!(has(&r, "SRC-004", Severity::Error));
        assert!(has(&r, "SRC-004", Severity::Warning));
        assert!(!r.not_implemented.is_empty() && !r.limitations.is_empty());
    }

    #[test]
    fn a_mapped_part_without_a_record_or_with_a_search_hit_is_an_error() {
        let p = project(&[("AMS1117-3.3_SOT-223", "U1", "AMS1117-3.3")]);
        assert_eq!(bom_rows(&p)[0].distributor.as_deref(), Some("C6186"));
        assert!(has(&run(&p, &[]), "SRC-005", Severity::Error));
        let hit = run(&p, &[ldo_part(VerificationState::SearchHit)]);
        assert!(has(&hit, "SRC-006", Severity::Error));
        let matched = run(&p, &[ldo_part(VerificationState::DatasheetMatched)]);
        assert!(has(&matched, "SRC-006", Severity::Warning));
        assert!(!has(&matched, "SRC-006", Severity::Error));
        let verified = run(&p, &[ldo_part(VerificationState::Verified)]);
        assert!(!verified.findings.iter().any(|f| f.rule_id == "SRC-006"));
        // Verified supplier data still does not supply the missing library datasheet
        // reference or verify the library ratings.
        assert!(has(&verified, "SRC-003", Severity::Error));
        assert!(has(&verified, "SRC-004", Severity::Warning));
        // SOT-223-3 with a tab pad: 4 pads for 3 leads is flagged for confirmation.
        assert!(has(&verified, "SRC-010", Severity::Warning));
        assert!(!verified
            .findings
            .iter()
            .any(|f| f.rule_id == "SRC-007" || f.rule_id == "SRC-008" || f.rule_id == "SRC-011"));
    }

    #[test]
    fn identity_package_and_pad_count_mismatches_are_errors() {
        let p = project(&[("AMS1117-3.3_SOT-223", "U1", "AMS1117-3.3")]);
        let mut part = ldo_part(VerificationState::Verified);
        part.mpn = Some("LM1117-3.3".into());
        part.package = Some("SOIC-8".into());
        part.datasheet_sha256 = None;
        let r = run(&p, &[part.clone()]);
        for rule in ["SRC-007", "SRC-008", "SRC-010", "SRC-011"] {
            assert!(has(&r, rule, Severity::Error), "{rule}");
        }
        part.mpn = None;
        part.package = None;
        let r = run(&p, &[part]);
        assert!(has(&r, "SRC-007", Severity::Error));
        assert!(has(&r, "SRC-008", Severity::Error));
    }

    #[test]
    fn symbol_footprint_inconsistency_and_shared_part_numbers_are_reported() {
        let mut p = project(&[("R_0603", "R1", "10k"), ("R_0603", "R2", "330")]);
        let def = p.library.get_mut("R_0603").unwrap();
        def.distributor = Some("C25804".into());
        def.pin_map.remove("2");
        let r = run(&p, &[]);
        assert!(has(&r, "SRC-012", Severity::Error));
        // Pin 2 unmapped (structural error) and pad 2 unreferenced (warning).
        assert!(has(&r, "SRC-009", Severity::Error));
        assert!(has(&r, "SRC-009", Severity::Warning));
        // Malformed Distributor numbers leave the row unmapped.
        p.library.get_mut("R_0603").unwrap().distributor = Some("25804".into());
        assert!(bom_rows(&p).iter().all(|r| r.distributor.is_none()));
        assert!(has(&run(&p, &[]), "SRC-002", Severity::Error));
    }
}
