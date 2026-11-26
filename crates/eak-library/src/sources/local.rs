//! A part source over a part list the user supplied (CSV or JSON).
//!
//! The file is untrusted input. It is bounded in size, row count and field length, must be
//! UTF-8, and every row is validated on its own: a row without a manufacturer and MPN, with a
//! control character, a non-http(s) datasheet URL, a numeric specification that is not a
//! finite number, or a key already seen is rejected with its line or index, and the rest of the
//! file is still used. Nothing in the file is fetched or opened (a datasheet is kept as a URL
//! reference only), and nothing is verified by being imported: specifications are recorded with
//! this source as their origin.
//!
//! CSV: a header row, then one part per row. Known columns (case-insensitive): `manufacturer`,
//! `mpn`, `description`, `category`, `package`, `lifecycle`, `datasheet_url`,
//! `datasheet_revision`. A column named `spec:<name>` holds a text specification and
//! `spec:<name> [<unit>]` a numeric one. Other columns are reported as ignored.
//!
//! JSON: an array of objects with the same field names, and `specs` as an array of
//! `{ "name", "value", "unit"? }` (a unit makes the value numeric). Unknown fields reject the
//! row.

use super::text_matches;
use crate::part::{DatasheetRef, PartKey, PartRecord, Provenance, SourceId, Spec, SpecOrigin};
use crate::source::{
    Capabilities, PartQuery, PartSource, SourceAnswer, SourceError, SourceKind, SourceStatus,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Largest file accepted.
pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;
/// Most part rows accepted.
pub const MAX_ROWS: usize = 50_000;
/// Longest field accepted, in characters.
pub const MAX_FIELD_CHARS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListFormat {
    Csv,
    Json,
}

/// What an import used and what it refused, for the UI to show.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ImportReport {
    pub accepted: usize,
    /// One entry per refused row: where it is and why.
    pub rejected: Vec<String>,
    /// CSV columns that are not known and were not read.
    pub ignored_columns: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    #[serde(default)]
    manufacturer: String,
    #[serde(default)]
    mpn: String,
    description: Option<String>,
    category: Option<String>,
    package: Option<String>,
    lifecycle: Option<String>,
    datasheet_url: Option<String>,
    datasheet_revision: Option<String>,
    #[serde(default)]
    specs: Vec<RowSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RowSpec {
    name: String,
    value: String,
    #[serde(default)]
    unit: Option<String>,
}

/// Rows as `(where, row or why it could not be read)`.
type Rows = Vec<(String, Result<Row, String>)>;

/// A user-supplied part list as a part source.
pub struct LocalListSource {
    id: SourceId,
    reference: String,
    status: SourceStatus,
    records: Vec<PartRecord>,
    report: ImportReport,
}

impl LocalListSource {
    /// Import a part list from its bytes. `reference` names the file for provenance (for
    /// example its path); the bytes' SHA-256 is recorded on every record. Never fails: an
    /// unusable file gives a source whose status is `Error` with the reason.
    pub fn import(
        id: &str,
        reference: &str,
        format: ListFormat,
        bytes: &[u8],
        retrieved_at_unix_s: Option<u64>,
    ) -> LocalListSource {
        let mut s = LocalListSource {
            id: SourceId::new(id),
            reference: reference.to_string(),
            status: SourceStatus::Ready,
            records: Vec::new(),
            report: ImportReport::default(),
        };
        let rows = match Self::rows(format, bytes, &mut s.report) {
            Ok(rows) => rows,
            Err(why) => {
                s.status = SourceStatus::Error(why);
                return s;
            }
        };
        let raw_sha256 = format!("{:x}", Sha256::digest(bytes));
        let mut seen: BTreeMap<PartKey, String> = BTreeMap::new();
        for (at, row) in rows {
            let result = row.and_then(|row| s.record(&at, row, &raw_sha256, retrieved_at_unix_s));
            match result {
                Ok(r) => {
                    if let Some(first) = seen.get(&r.key) {
                        let why = format!("{at}: same manufacturer and MPN as {first}");
                        s.report.rejected.push(why);
                    } else {
                        seen.insert(r.key.clone(), at);
                        s.records.push(r);
                    }
                }
                Err(why) => s.report.rejected.push(format!("{at}: {why}")),
            }
        }
        s.report.accepted = s.records.len();
        if s.records.is_empty() {
            s.status = SourceStatus::Error("the file has no usable part rows".into());
        }
        s
    }

    /// The import's report.
    pub fn report(&self) -> &ImportReport {
        &self.report
    }

    /// Every accepted record, in file order.
    pub fn records(&self) -> &[PartRecord] {
        &self.records
    }

    fn rows(format: ListFormat, bytes: &[u8], report: &mut ImportReport) -> Result<Rows, String> {
        if bytes.len() > MAX_FILE_BYTES {
            return Err(format!("the file is larger than {MAX_FILE_BYTES} bytes"));
        }
        let text = std::str::from_utf8(bytes).map_err(|_| "the file is not UTF-8".to_string())?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let rows = match format {
            ListFormat::Json => json_rows(text)?,
            ListFormat::Csv => csv_rows(text, report)?,
        };
        if rows.len() > MAX_ROWS {
            return Err(format!("the file has more than {MAX_ROWS} rows"));
        }
        Ok(rows)
    }

    fn record(
        &self,
        at: &str,
        row: Row,
        raw_sha256: &str,
        retrieved_at_unix_s: Option<u64>,
    ) -> Result<PartRecord, String> {
        let texts = [
            Some(&row.manufacturer),
            Some(&row.mpn),
            row.description.as_ref(),
            row.category.as_ref(),
            row.package.as_ref(),
            row.lifecycle.as_ref(),
            row.datasheet_url.as_ref(),
            row.datasheet_revision.as_ref(),
        ];
        let spec_texts = row
            .specs
            .iter()
            .flat_map(|s| [Some(&s.name), Some(&s.value), s.unit.as_ref()]);
        for t in texts.into_iter().chain(spec_texts).flatten() {
            check_text(t)?;
        }
        let mut r = PartRecord::new(
            &row.manufacturer,
            &row.mpn,
            Provenance {
                source: self.id.clone(),
                reference: format!("{}#{at}", self.reference),
                retrieved_at_unix_s,
                raw_sha256: Some(raw_sha256.to_string()),
            },
        )
        .map_err(|e| e.to_string())?;
        let opt = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        r.description = opt(row.description);
        r.category = opt(row.category);
        r.package = opt(row.package);
        r.lifecycle = opt(row.lifecycle);
        let revision = opt(row.datasheet_revision);
        match opt(row.datasheet_url) {
            Some(url) => {
                let lower = url.to_ascii_lowercase();
                if !(lower.starts_with("https://") || lower.starts_with("http://")) {
                    return Err(format!("datasheet_url {url:?} is not an http(s) URL"));
                }
                let mut d = DatasheetRef::url(&url);
                d.revision = revision;
                d.retrieved_at_unix_s = retrieved_at_unix_s;
                r.datasheets.push(d);
            }
            None if revision.is_some() => {
                return Err("datasheet_revision is given without datasheet_url".into());
            }
            None => {}
        }
        let origin = SpecOrigin::Source(self.id.clone());
        for s in row.specs {
            let unit = s.unit.as_deref().map(str::trim).unwrap_or("");
            let spec = if unit.is_empty() {
                Spec::text(&s.name, &s.value, origin.clone())
            } else {
                Spec::number(&s.name, &s.value, unit, origin.clone())
            };
            r.specs.push(spec.map_err(|e| format!("spec {:?}: {e}", s.name))?);
        }
        Ok(r)
    }
}

/// Refuse over-long text and control characters.
fn check_text(t: &str) -> Result<(), String> {
    if t.chars().count() > MAX_FIELD_CHARS {
        return Err(format!("a field is longer than {MAX_FIELD_CHARS} characters"));
    }
    if t.chars().any(char::is_control) {
        return Err("a field contains a control character".into());
    }
    Ok(())
}

fn json_rows(text: &str) -> Result<Rows, String> {
    let doc: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("the file is not valid JSON: {e}"))?;
    let serde_json::Value::Array(items) = doc else {
        return Err("the JSON file must be an array of part objects".into());
    };
    Ok(items
        .into_iter()
        .enumerate()
        .map(|(i, v)| {
            let row = serde_json::from_value::<Row>(v).map_err(|e| e.to_string());
            (format!("item {}", i + 1), row)
        })
        .collect())
}

/// Parse CSV text (RFC 4180: quoted fields, doubled quotes, CRLF or LF) into
/// `(line number, fields)`, skipping blank lines.
fn parse_csv(text: &str) -> Result<Vec<(usize, Vec<String>)>, String> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut quoted = false;
    let mut line = 1;
    let mut row_line = 1;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                if c == '\n' {
                    line += 1;
                }
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() && !quoted => {
                in_quotes = true;
                quoted = true;
            }
            '"' => return Err(format!("line {line}: a quote inside an unquoted field")),
            ',' => {
                row.push(std::mem::take(&mut field));
                quoted = false;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push((row_line, std::mem::take(&mut row)));
                quoted = false;
                line += 1;
                row_line = line;
            }
            _ if quoted => return Err(format!("line {line}: text after a closing quote")),
            _ => field.push(c),
        }
    }
    if in_quotes {
        return Err(format!("line {row_line}: a quoted field is not closed"));
    }
    if !field.is_empty() || !row.is_empty() || quoted {
        row.push(field);
        rows.push((row_line, row));
    }
    rows.retain(|(_, r)| !(r.len() == 1 && r[0].trim().is_empty()));
    Ok(rows)
}

/// What a CSV column holds.
enum Column {
    Field(&'static str),
    Spec { name: String, unit: Option<String> },
    Ignored,
}

const FIELDS: [&str; 8] = [
    "manufacturer",
    "mpn",
    "description",
    "category",
    "package",
    "lifecycle",
    "datasheet_url",
    "datasheet_revision",
];

fn column(header: &str) -> Column {
    let h = header.trim();
    if let Some(f) = FIELDS.iter().find(|f| f.eq_ignore_ascii_case(h)) {
        return Column::Field(*f);
    }
    if !h.get(..5).is_some_and(|p| p.eq_ignore_ascii_case("spec:")) {
        return Column::Ignored;
    }
    let rest = h[5..].trim();
    let (name, unit) = match rest.strip_suffix(']').and_then(|r| r.rsplit_once('[')) {
        Some((name, unit)) => (name.trim(), Some(unit.trim().to_string())),
        None => (rest, None),
    };
    if name.is_empty() {
        return Column::Ignored;
    }
    Column::Spec {
        name: name.to_string(),
        unit,
    }
}

fn csv_rows(text: &str, report: &mut ImportReport) -> Result<Rows, String> {
    let mut rows = parse_csv(text)?.into_iter();
    let Some((_, header)) = rows.next() else {
        return Err("the CSV file is empty".into());
    };
    let columns: Vec<Column> = header.iter().map(|h| column(h.as_str())).collect();
    let mut fields_seen: Vec<&'static str> = Vec::new();
    for (h, c) in header.iter().zip(&columns) {
        match c {
            Column::Field(f) if fields_seen.contains(f) => {
                return Err(format!("the CSV header names {f:?} twice"));
            }
            Column::Field(f) => fields_seen.push(*f),
            Column::Spec { .. } => {}
            Column::Ignored => report.ignored_columns.push(h.trim().to_string()),
        }
    }
    if !fields_seen.contains(&"manufacturer") || !fields_seen.contains(&"mpn") {
        return Err("the CSV header needs manufacturer and mpn columns".into());
    }
    Ok(rows
        .map(|(line, cells)| {
            let at = format!("line {line}");
            if cells.len() != columns.len() {
                let why = format!("{} cells, header has {}", cells.len(), columns.len());
                return (at, Err(why));
            }
            let mut row = Row::default();
            for (cell, c) in cells.into_iter().zip(&columns) {
                match c {
                    Column::Field("manufacturer") => row.manufacturer = cell,
                    Column::Field("mpn") => row.mpn = cell,
                    Column::Field("description") => row.description = Some(cell),
                    Column::Field("category") => row.category = Some(cell),
                    Column::Field("package") => row.package = Some(cell),
                    Column::Field("lifecycle") => row.lifecycle = Some(cell),
                    Column::Field("datasheet_url") => row.datasheet_url = Some(cell),
                    Column::Field(_) => row.datasheet_revision = Some(cell),
                    Column::Spec { name, unit } if !cell.trim().is_empty() => {
                        row.specs.push(RowSpec {
                            name: name.clone(),
                            value: cell,
                            unit: unit.clone(),
                        });
                    }
                    Column::Spec { .. } | Column::Ignored => {}
                }
            }
            (at, Ok(row))
        })
        .collect())
}

impl PartSource for LocalListSource {
    fn id(&self) -> SourceId {
        self.id.clone()
    }
    fn kind(&self) -> SourceKind {
        SourceKind::Local
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            definitions: false,
            parameters: true,
            offers: false,
            datasheets: true,
        }
    }
    fn status(&self) -> SourceStatus {
        self.status.clone()
    }
    fn terms(&self) -> String {
        format!(
            "User-supplied part list {}; values as stated in the file, unverified",
            self.reference
        )
    }

    fn search(&self, q: &PartQuery) -> Result<SourceAnswer, SourceError> {
        if self.status != SourceStatus::Ready {
            return Err(SourceError::Unavailable(self.status_text()));
        }
        if q.text.is_empty() {
            return Err(SourceError::BadQuery("empty query".into()));
        }
        let records = self
            .records
            .iter()
            .filter(|r| {
                let desc = r.description.as_deref().unwrap_or("");
                text_matches(&q.text, &r.display_mpn) || text_matches(&q.text, desc)
            })
            .take(q.limit)
            .cloned()
            .collect();
        Ok(SourceAnswer {
            records,
            unkeyed: Vec::new(),
        })
    }

    fn fetch(&self, key: &PartKey) -> Result<Option<PartRecord>, SourceError> {
        if self.status != SourceStatus::Ready {
            return Err(SourceError::Unavailable(self.status_text()));
        }
        Ok(self.records.iter().find(|r| &r.key == key).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::part::SpecValue;
    use crate::source::resolve;

    const CSV: &str = "\u{feff}Manufacturer,MPN,Description,datasheet_url,datasheet_revision,\
spec:Vout [V],spec:Grade,Notes\r\n\
Texas Instruments,TPS2051B-DBVR,\"Power switch, \"\"USB\"\"\",https://www.ti.com/lit/ds/symlink/tps2051b.pdf,Rev. C,5,Industrial,x\r\n\
\r\n\
,NOMAKER,a,,,,,\r\n\
Acme,BADURL,a,file:///etc/passwd,,,,\r\n\
Acme,BADSPEC,a,,,five,,\r\n\
Acme,REVONLY,a,,A,,,\r\n\
TEXAS-INSTRUMENTS,tps2051bdbvr,dup,,,,,\r\n\
Acme,SHORT\r\n";

    fn import(format: ListFormat, text: &str) -> LocalListSource {
        LocalListSource::import("local:test", "parts.csv", format, text.as_bytes(), Some(7))
    }

    #[test]
    fn csv_rows_are_validated_one_by_one() {
        let s = import(ListFormat::Csv, CSV);
        assert_eq!(s.status(), SourceStatus::Ready);
        let rep = s.report();
        assert_eq!(rep.accepted, 1);
        assert_eq!(rep.ignored_columns, vec!["Notes".to_string()]);
        let rejected = rep.rejected.join("\n");
        for want in [
            "line 4: manufacturer is empty",
            "line 5: datasheet_url",
            "line 6: spec \"Vout\"",
            "line 7: datasheet_revision is given without datasheet_url",
            "line 8: same manufacturer and MPN as line 2",
            "line 9: 2 cells, header has 8",
        ] {
            assert!(rejected.contains(want), "missing {want:?} in\n{rejected}");
        }
        assert_eq!(rep.rejected.len(), 6);

        let r = &s.records()[0];
        assert_eq!(r.description.as_deref(), Some("Power switch, \"USB\""));
        assert_eq!(r.provenance.reference, "parts.csv#line 2");
        assert_eq!(r.provenance.retrieved_at_unix_s, Some(7));
        assert!(r.provenance.raw_sha256.is_some());
        assert_eq!(r.datasheets[0].revision.as_deref(), Some("Rev. C"));
        assert_eq!(r.datasheets[0].sha256, None, "not fetched");
        assert_eq!(r.specs.len(), 2);
        assert_eq!(
            r.specs[0].value,
            SpecValue::Number {
                value: "5".into(),
                unit: "V".into()
            }
        );
        assert_eq!(
            r.specs[1].origin,
            SpecOrigin::Source(SourceId::new("local:test"))
        );
    }

    #[test]
    fn imported_parts_are_searchable_and_resolvable() {
        let s = import(ListFormat::Csv, CSV);
        let res = resolve(&[&s], &PartQuery::new("TPS2051B-DBVR"));
        assert_eq!(res.parts.len(), 1);
        let key = PartKey::new("TI", "TPS2051BDBVR").unwrap();
        assert_eq!(s.fetch(&key).unwrap(), None, "not the stated maker");
        let key = PartKey::new("Texas Instruments", "TPS2051BDBVR").unwrap();
        assert!(s.fetch(&key).unwrap().is_some());
        let hits = s.search(&PartQuery::new("usb")).unwrap();
        assert_eq!(hits.records.len(), 1, "description matches too");
    }

    #[test]
    fn json_lists_are_validated_and_unknown_fields_rejected() {
        let json = r#"[
            {"manufacturer": "Acme", "mpn": "A1",
             "specs": [{"name": "Iq", "value": "55", "unit": "uA"}]},
            {"manufacturer": "Acme", "mpn": "A2", "verified": true},
            {"manufacturer": "Acme", "mpn": "A3\u0007"}
        ]"#;
        let s = import(ListFormat::Json, json);
        assert_eq!(s.status(), SourceStatus::Ready);
        assert_eq!(s.records().len(), 1);
        assert_eq!(s.records()[0].specs[0].display_value(), "55 uA");
        let rejected = &s.report().rejected;
        assert_eq!(rejected.len(), 2);
        assert!(rejected[0].starts_with("item 2: unknown field `verified`"));
        assert!(rejected[1].contains("control character"));
    }

    #[test]
    fn unusable_files_are_errors_and_are_never_searched() {
        for (format, text) in [
            (ListFormat::Json, "{\"mpn\": \"A1\"}"),
            (ListFormat::Json, "not json"),
            (ListFormat::Csv, ""),
            (ListFormat::Csv, "mpn,description\nA1,x\n"),
            (ListFormat::Csv, "manufacturer,mpn\n\"Acme,A1\n"),
            (ListFormat::Csv, "manufacturer,mpn,mpn\nAcme,A1,A1\n"),
            (ListFormat::Csv, "manufacturer,mpn\n,A1\n"),
        ] {
            let s = import(format, text);
            assert!(
                matches!(s.status(), SourceStatus::Error(_)),
                "{text:?} gave {:?}",
                s.status()
            );
            assert!(s.search(&PartQuery::new("A1")).is_err());
            let res = resolve(&[&s], &PartQuery::new("A1"));
            assert!(res.parts.is_empty() && res.suggestions.is_empty());
        }
        let big = vec![b'a'; MAX_FILE_BYTES + 1];
        let s = LocalListSource::import("l", "big.csv", ListFormat::Csv, &big, None);
        assert!(matches!(s.status(), SourceStatus::Error(_)));
        let s = LocalListSource::import("l", "bad.csv", ListFormat::Csv, &[0xff, 0xfe], None);
        assert!(matches!(s.status(), SourceStatus::Error(_)));
    }

    #[test]
    fn csv_parser_handles_quotes_and_line_endings() {
        let rows = parse_csv("a,\"b,\"\"c\"\"\"\r\n\r\n\"\",d\n").unwrap();
        assert_eq!(
            rows,
            vec![
                (1, vec!["a".to_string(), "b,\"c\"".to_string()]),
                (3, vec![String::new(), "d".to_string()]),
            ]
        );
        assert!(parse_csv("a\"b\n").is_err());
        assert!(parse_csv("\"a\"b\n").is_err());
    }
}
