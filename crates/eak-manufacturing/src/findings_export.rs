//! Findings export (Phase 1 slice B5): one ERC/DRC/DFM report written as CSV or JSON into
//! `<project>/output/rev-<N>/findings/`.
//!
//! The JSON file is the whole report (rules, per-rule states, not-implemented checks and
//! limitations), so nothing the report says about its own coverage is lost. The CSV file has one
//! row per finding; its first columns name the check, the revision and the input hash so a row
//! copied out of the file still says which design it speaks for.

use crate::bom::q;
use eak_core::geom::to_mm;
use eak_validation::{Bound, Report, Severity};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingsFormat {
    Csv,
    Json,
}

impl FindingsFormat {
    pub fn extension(self) -> &'static str {
        match self {
            FindingsFormat::Csv => "csv",
            FindingsFormat::Json => "json",
        }
    }
}

/// Column headers of [`findings_csv`].
pub const CSV_HEADERS: [&str; 13] = [
    "Check",
    "Revision",
    "Input SHA-256",
    "Finding",
    "Rule",
    "Severity",
    "Message",
    "Objects",
    "X (mm)",
    "Y (mm)",
    "Measured",
    "Required",
    "Suggested fix",
];

fn severity_word(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

/// One row per finding, in the report's order (errors first). Every cell is quoted and
/// neutralised against spreadsheet formula injection.
pub fn findings_csv(r: &Report) -> String {
    let mut out = CSV_HEADERS.map(q).join(",");
    out.push('\n');
    for f in &r.findings {
        let (x, y) = f
            .location
            .map(|p| (to_mm(p.x).to_string(), to_mm(p.y).to_string()))
            .unwrap_or_default();
        let measured = f
            .measured
            .as_ref()
            .map(|m| format!("{} {}", m.value, m.unit))
            .unwrap_or_default();
        let required = f
            .required
            .as_ref()
            .map(|l| {
                let b = match l.bound {
                    Bound::Min => ">=",
                    Bound::Max => "<=",
                };
                format!("{b} {} {}", l.value, l.unit)
            })
            .unwrap_or_default();
        let cells = [
            r.check.clone(),
            r.revision.to_string(),
            r.input_sha256.clone(),
            f.id.clone(),
            f.rule_id.clone(),
            severity_word(f.severity).to_string(),
            f.message.clone(),
            f.objects.join(" "),
            x,
            y,
            measured,
            required,
            f.suggested_fix.clone().unwrap_or_default(),
        ];
        out.push_str(&cells.iter().map(|c| q(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

/// Write `r` as `<check>-findings.<ext>` into `out_dir` and return the path written.
pub fn export_findings(
    r: &Report,
    out_dir: &Path,
    format: FindingsFormat,
) -> Result<PathBuf, String> {
    let bytes = match format {
        FindingsFormat::Csv => findings_csv(r).into_bytes(),
        FindingsFormat::Json => serde_json::to_string_pretty(r)
            .map_err(|e| e.to_string())?
            .into_bytes(),
    };
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let path = out_dir.join(format!(
        "{}-findings.{}",
        r.check.to_lowercase(),
        format.extension()
    ));
    eak_runtime::store::atomic_write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::{Point, Project};

    fn report() -> Report {
        let mut p = Project::new("csv");
        p.revision = 7;
        eak_validation::erc::run(&p)
    }

    #[test]
    fn csv_has_provenance_columns_and_one_row_per_finding() {
        let mut r = report();
        r.findings = vec![eak_validation::Finding {
            id: "ERC-001-abc".into(),
            rule_id: "ERC-001".into(),
            severity: Severity::Error,
            message: "=HYPERLINK(\"x\") pin, \"quoted\"".into(),
            objects: vec!["sym_1".into(), "sym_2".into()],
            location: Some(Point::mm(1.5, -2.0)),
            measured: None,
            required: None,
            suggested_fix: Some("Wire it".into()),
        }];
        let csv = findings_csv(&r);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("\"Check\",\"Revision\",\"Input SHA-256\""));
        assert!(lines[1].starts_with("\"ERC\",\"7\","));
        assert!(lines[1].contains(&r.input_sha256));
        // Formula injection is neutralised and quotes are doubled.
        assert!(lines[1].contains("\"'=HYPERLINK(\"\"x\"\") pin, \"\"quoted\"\"\""));
        assert!(lines[1].contains("\"sym_1 sym_2\""));
        assert!(lines[1].contains("\"1.5\",\"'-2\""));
        assert!(lines[1].contains("\"error\""));
    }

    #[test]
    fn json_export_keeps_the_whole_report_including_what_was_not_checked() {
        let r = report();
        let dir = std::env::temp_dir().join(format!("eak-findings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = export_findings(&r, &dir, FindingsFormat::Json).unwrap();
        assert!(path.ends_with("erc-findings.json"));
        let back: Report = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(back, r);
        assert!(!back.not_implemented.is_empty());
        let csv = export_findings(&r, &dir, FindingsFormat::Csv).unwrap();
        assert!(csv.ends_with("erc-findings.csv"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
