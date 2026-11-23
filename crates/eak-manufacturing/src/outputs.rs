//! Output documents (slice D3): configurable BOM (CSV or XLSX), schematic PDF and SVG, and the
//! IPC-D-356 and JSON netlists, written into one folder with a revision-bound manifest.
//!
//! The manifest is the same [`Manifest`] the manufacturing package uses: every file written is
//! listed with its size and SHA-256, and the status comes from the same release gate
//! ([`crate::release_gates`]). The folder is DRAFT unless that gate passes, and the documents
//! that can carry a label (PDF/SVG title block, IPC-D-356 comment, JSON `status`) say so.

use crate::bom_config::{self, BomConfig};
use crate::sourcing::SourcingRecord;
use crate::{
    file_stem, netlist_export, not_verified, release_gates, schematic_doc, sha, statement_of,
    status_of, FileEntry, Manifest, PackageStatus,
};
use eak_core::Project;
use eak_validation::{report::project_hash, Report};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BomFormat {
    #[default]
    Csv,
    Xlsx,
}

/// The BOM part of a request: the configuration and the file format.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BomRequest {
    pub config: BomConfig,
    #[serde(default)]
    pub format: BomFormat,
}

/// Which documents to write.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OutputRequest {
    #[serde(default)]
    pub bom: Option<BomRequest>,
    #[serde(default)]
    pub schematic_pdf: bool,
    #[serde(default)]
    pub schematic_svg: bool,
    #[serde(default)]
    pub ipc_d_356: bool,
    #[serde(default)]
    pub netlist_json: bool,
}

impl OutputRequest {
    pub fn is_empty(&self) -> bool {
        self.bom.is_none()
            && !self.schematic_pdf
            && !self.schematic_svg
            && !self.ipc_d_356
            && !self.netlist_json
    }
}

/// Label written into the documents themselves.
pub fn status_label(status: PackageStatus) -> &'static str {
    match status {
        PackageStatus::Draft => "DRAFT - NOT FOR FABRICATION",
        PackageStatus::InternallyVerifiedReleaseCandidate => {
            "INTERNAL CHECKS PASSED - engineering and fabricator review required"
        }
    }
}

/// Write the requested documents and `manifest.json` into `out_dir`.
pub fn export(
    p: &Project,
    reports: &[&Report],
    records: &[SourcingRecord],
    out_dir: &Path,
    req: &OutputRequest,
) -> Result<Manifest, String> {
    if req.is_empty() {
        return Err("choose at least one output to export".into());
    }
    let input = project_hash(p);
    let (gates, mut blocking) = release_gates(p, reports);
    if p.pcb.board.outline.len() < 3 {
        blocking.push("the board has no outline yet".into());
    }
    let status = status_of(&blocking);
    let label = status_label(status);
    let stem = file_stem(p);
    let mut files: Vec<(String, &'static str, Vec<u8>)> = vec![];
    if let Some(b) = &req.bom {
        let table = bom_config::table(p, records, &b.config)?;
        let (ext, kind, bytes) = match b.format {
            BomFormat::Csv => ("csv", "bom", bom_config::csv(&table).into_bytes()),
            BomFormat::Xlsx => ("xlsx", "bom-xlsx", bom_config::xlsx(&table)),
        };
        files.push((format!("{stem}-BOM.{ext}"), kind, bytes));
        files.push((
            format!("{stem}-BOM.config.json"),
            "bom-config",
            b.config.to_json().into_bytes(),
        ));
    }
    if req.schematic_pdf || req.schematic_svg {
        let tb = schematic_doc::TitleBlock {
            project: p.name.clone(),
            revision: p.revision,
            date: bom_config::utc_date((eak_runtime::store::now_ms() / 1000) as u64),
            status: label.into(),
            input_sha256: input.clone(),
        };
        let pages = schematic_doc::pages(p, &tb);
        let title = format!("{} schematic, revision {}", p.name, p.revision);
        if req.schematic_pdf {
            files.push((
                format!("{stem}-schematic.pdf"),
                "pdf",
                schematic_doc::pdf(&pages, &title),
            ));
        }
        if req.schematic_svg {
            for (i, page) in pages.iter().enumerate() {
                files.push((
                    format!("{stem}-schematic-{}.svg", i + 1),
                    "svg",
                    schematic_doc::svg(page, &format!("{title}, {}", page.sheet)).into_bytes(),
                ));
            }
        }
    }
    if req.ipc_d_356 {
        files.push((
            format!("{stem}-netlist.ipc"),
            "ipc-d-356",
            netlist_export::ipc_d_356(p, label).into_bytes(),
        ));
    }
    if req.netlist_json {
        let status_word = match status {
            PackageStatus::Draft => "DRAFT",
            PackageStatus::InternallyVerifiedReleaseCandidate => {
                "INTERNALLY_VERIFIED_RELEASE_CANDIDATE"
            }
        };
        files.push((
            format!("{stem}-netlist.json"),
            "netlist-json",
            netlist_export::json(p, status_word, &input).into_bytes(),
        ));
    }
    for (name, kind, bytes) in &files {
        let problems = validate_output(kind, bytes);
        if !problems.is_empty() {
            return Err(format!(
                "generated {name} failed self-validation: {}",
                problems.join("; ")
            ));
        }
    }
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let mut entries = vec![];
    for (name, kind, bytes) in &files {
        let path = out_dir.join(name);
        eak_runtime::store::atomic_write(&path, bytes).map_err(|e| e.to_string())?;
        // Re-read from disk so the hash covers exactly what was written.
        let written = std::fs::read(&path).map_err(|e| e.to_string())?;
        entries.push(FileEntry {
            name: name.clone(),
            bytes: written.len() as u64,
            sha256: sha(&written),
            kind: (*kind).to_string(),
        });
    }
    let manifest = Manifest {
        project_id: p.id.clone(),
        project_name: p.name.clone(),
        revision: p.revision,
        input_sha256: input,
        generator: format!("eak-manufacturing {}", env!("CARGO_PKG_VERSION")),
        generated_ms: eak_runtime::store::now_ms(),
        status,
        gates,
        blocking_reasons: blocking,
        files: entries,
        statement: statement_of(status),
        not_verified: not_verified(),
    };
    let mjson = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    eak_runtime::store::atomic_write(&out_dir.join("manifest.json"), mjson.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(manifest)
}

/// Syntax problems of an output document of `kind` (empty when well formed, and for kinds
/// this module does not write). Used before writing and by [`crate::verify_package`].
pub fn validate_output(kind: &str, data: &[u8]) -> Vec<String> {
    let text = || String::from_utf8_lossy(data).into_owned();
    match kind {
        "pdf" => schematic_doc::validate_pdf(data),
        "svg" => schematic_doc::validate_svg(&text()),
        "bom-xlsx" => crate::xlsx::validate(data),
        "ipc-d-356" => netlist_export::validate_ipc_d_356(&text()),
        "netlist-json" => match serde_json::from_slice::<serde_json::Value>(data) {
            Ok(_) => vec![],
            Err(e) => vec![format!("not JSON: {e}")],
        },
        "bom-config" => match BomConfig::from_json(&text()) {
            Ok(_) => vec![],
            Err(e) => vec![e],
        },
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("eak-outputs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn every_output_is_in_the_manifest_and_draft_without_passing_gates() {
        let p = crate::netlist_export::tests::board();
        let dir = tmp("all");
        let req = OutputRequest {
            bom: Some(BomRequest {
                config: BomConfig::default(),
                format: BomFormat::Xlsx,
            }),
            schematic_pdf: true,
            schematic_svg: true,
            ipc_d_356: true,
            netlist_json: true,
        };
        // No reports supplied: the release gate cannot pass.
        let m = export(&p, &[], &[], &dir, &req).unwrap();
        assert_eq!(m.status, PackageStatus::Draft);
        assert!(m
            .blocking_reasons
            .iter()
            .any(|b| b.contains("ERC report not supplied")));
        assert!(m.blocking_reasons.iter().any(|b| b.contains("no outline")));
        let kinds: Vec<&str> = m.files.iter().map(|f| f.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "bom-xlsx",
                "bom-config",
                "pdf",
                "svg",
                "ipc-d-356",
                "netlist-json"
            ]
        );
        for f in &m.files {
            assert!(
                dir.join(&f.name).is_file(),
                "{} listed but not written",
                f.name
            );
        }
        // Nothing else was written besides the manifest.
        let on_disk = std::fs::read_dir(&dir).unwrap().count();
        assert_eq!(on_disk, m.files.len() + 1);
        assert!(crate::verify_package(&dir).unwrap().is_empty());
        let pdf = std::fs::read_to_string(dir.join("BOM_config-schematic.pdf")).unwrap();
        assert!(pdf.contains("(Status: DRAFT - NOT FOR FABRICATION) Tj"));
        let ipc = std::fs::read_to_string(dir.join("BOM_config-netlist.ipc")).unwrap();
        assert!(ipc.contains("C  Status: DRAFT - NOT FOR FABRICATION"));
        let saved = std::fs::read_to_string(dir.join("BOM_config-BOM.config.json")).unwrap();
        assert_eq!(BomConfig::from_json(&saved).unwrap(), BomConfig::default());
        // A file changed after export no longer verifies.
        std::fs::write(dir.join("BOM_config-netlist.json"), "{}").unwrap();
        assert!(!crate::verify_package(&dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_request_or_bad_config_writes_nothing() {
        let p = crate::netlist_export::tests::board();
        let dir = tmp("none");
        assert!(export(&p, &[], &[], &dir, &OutputRequest::default()).is_err());
        let bad = OutputRequest {
            bom: Some(BomRequest {
                config: BomConfig {
                    columns: vec![],
                    ..BomConfig::default()
                },
                format: BomFormat::Csv,
            }),
            ..OutputRequest::default()
        };
        assert!(export(&p, &[], &[], &dir, &bad).is_err());
        assert!(!dir.exists());
    }
}
