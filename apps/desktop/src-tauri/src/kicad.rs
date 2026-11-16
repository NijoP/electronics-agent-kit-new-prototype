//! Optional, one-way "Import from KiCad": run EAK's own importer (`eak_core::kicad_import`) on a
//! file the user picked and return its report. EAK never needs a KiCad install or KiCad
//! libraries; this is one interchange option. It stores nothing: converting into EAK-native
//! library definitions with provenance is the library manager's job.

use eak_core::kicad_import::{import_footprint, import_symbol_lib, ImportReport};
use eak_core::sexpr::Limits;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct KicadImportView {
    /// File name that was read (not the full path).
    pub file: String,
    pub report: ImportReport,
    pub imported: usize,
    pub rejected: usize,
}

/// Parse a `.kicad_sym` (symbol library) or `.kicad_mod` (footprint) file and report what the
/// importer accepted, rejected and did not interpret.
pub fn import_report(path: &str) -> Result<KicadImportView, String> {
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "kicad_sym" && ext != "kicad_mod" {
        return Err("choose a .kicad_sym (symbol library) or .kicad_mod (footprint) file".into());
    }
    let limits = Limits::default();
    let meta = std::fs::metadata(p).map_err(|e| format!("cannot read the file: {e}"))?;
    if meta.len() > limits.max_bytes as u64 {
        return Err(format!(
            "the file is {} bytes, over the {} byte import limit",
            meta.len(),
            limits.max_bytes
        ));
    }
    let src = std::fs::read_to_string(p).map_err(|e| format!("cannot read the file: {e}"))?;
    let report = if ext == "kicad_sym" {
        import_symbol_lib(&src, limits)
            .map_err(|e| e.to_string())?
            .report
    } else {
        import_footprint(&src, limits)
            .map_err(|e| e.to_string())?
            .report
    };
    Ok(KicadImportView {
        file: p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        imported: report.imported(),
        rejected: report.rejected(),
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_two_interchange_extensions_are_accepted() {
        let e = import_report("/nonexistent/board.zip").unwrap_err();
        assert!(e.contains(".kicad_sym"));
    }

    #[test]
    fn a_missing_file_is_a_readable_error() {
        let e = import_report("/nonexistent/lib.kicad_sym").unwrap_err();
        assert!(e.contains("cannot read"));
    }
}
