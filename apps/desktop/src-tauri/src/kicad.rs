//! Optional, one-way "Import from KiCad": run EAK's own importer (`eak_core::kicad_import`) on a
//! file the user picked and return its report, or commit valid definitions directly into EAK's
//! native library store with provenance and version metadata. EAK never needs a KiCad install or
//! KiCad libraries; this is an interchange option.

use crate::libstore::{safe_name, store, Manifest};
use eak_core::kicad_import::{import_footprint, import_symbol_lib, ImportReport};
use eak_core::pinmap::suggest_by_number;
use eak_core::sexpr::Limits;
use eak_core::{
    ComponentDef, FootprintDef, PadDef, PadKind, PadShape, PinType, Point, SymbolDef, SymbolPin,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KicadImportView {
    /// File name that was read (not the full path).
    pub file: String,
    pub report: ImportReport,
    pub imported: usize,
    pub rejected: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KicadCommitResult {
    pub library_name: String,
    /// Licence and source the user stated for the library (recorded in its manifest).
    pub licence: String,
    pub source_url: String,
    /// True for a symbol library: each footprint is an EAK placeholder (one pad per pin,
    /// numbered like the pins), not the real package, and must be replaced before layout.
    pub placeholder_footprints: bool,
    pub components_created: usize,
    pub component_ids: Vec<String>,
    pub rejected: Vec<String>,
    pub report: ImportReport,
}

/// Generates a valid footprint definition with pads corresponding to the symbol pins.
pub fn default_footprint_for_pins(name: &str, pins: &[SymbolPin]) -> FootprintDef {
    let mut pads = Vec::new();
    let count = pins.len();
    if count <= 2 {
        pads.push(PadDef {
            number: pins
                .first()
                .map(|p| p.number.clone())
                .unwrap_or_else(|| "1".into()),
            kind: PadKind::Smd,
            shape: PadShape::Rect,
            position: Point::new(-1_000_000, 0),
            size_x: 1_000_000,
            size_y: 1_200_000,
            drill: None,
        });
        if count == 2 {
            pads.push(PadDef {
                number: pins[1].number.clone(),
                kind: PadKind::Smd,
                shape: PadShape::Rect,
                position: Point::new(1_000_000, 0),
                size_x: 1_000_000,
                size_y: 1_200_000,
                drill: None,
            });
        }
    } else {
        let half = count.div_ceil(2);
        let pitch = 2_540_000i64; // 2.54 mm
        let span = 5_000_000i64; // 5 mm between columns
        for (i, p) in pins.iter().enumerate() {
            let (x, y) = if i < half {
                (-span / 2, (half as i64 / 2 - i as i64) * pitch)
            } else {
                let idx = i - half;
                (span / 2, (idx as i64 - half as i64 / 2) * pitch)
            };
            pads.push(PadDef {
                number: p.number.clone(),
                kind: PadKind::Smd,
                shape: PadShape::Rect,
                position: Point::new(x, y),
                size_x: 1_500_000,
                size_y: 1_200_000,
                drill: None,
            });
        }
    }
    let (max_x, max_y) = pads
        .iter()
        .fold((1_500_000i64, 1_500_000i64), |(mx, my), p| {
            (
                mx.max(p.position.x.abs() + p.size_x / 2),
                my.max(p.position.y.abs() + p.size_y / 2),
            )
        });
    FootprintDef {
        name: name.to_string(),
        pads,
        courtyard_half_w: max_x + 500_000,
        courtyard_half_h: max_y + 500_000,
        body_height: None,
    }
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

/// Check the licence and source the user gave for a KiCad library before anything is written
/// (finding F2): a library is never imported without a named licence and an http(s) source,
/// and "unspecified"-style placeholders are not a licence.
pub fn check_provenance(
    licence: Option<&str>,
    source: Option<&str>,
) -> Result<(String, String), String> {
    let licence = licence.map(str::trim).unwrap_or("");
    if licence.is_empty() {
        return Err(
            "name the licence the KiCad library is distributed under (for example \
                    CC-BY-SA-4.0): without a licence nothing is imported"
                .into(),
        );
    }
    let lower = licence.to_ascii_lowercase();
    if licence.chars().count() > 120
        || licence.chars().any(char::is_control)
        || ["unspecified", "unknown", "none", "n/a", "?"]
            .iter()
            .any(|p| lower == *p || lower.starts_with(&format!("{p} ")))
    {
        return Err(format!(
            "{licence:?} is not a licence: name the licence the library is distributed under"
        ));
    }
    let source = source.map(str::trim).unwrap_or("");
    if !(source.starts_with("https://") || source.starts_with("http://"))
        || source.len() > 2048
        || source.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(
            "give the http(s) address the KiCad library was obtained from as its source".into(),
        );
    }
    Ok((licence.to_string(), source.to_string()))
}

/// Parses and commits imported KiCad definitions into EAK's library store (`imported/` scope),
/// with the licence and source the user named (refused without them, see [`check_provenance`]).
pub fn import_commit(
    path: &str,
    licence: Option<&str>,
    source: Option<&str>,
) -> Result<KicadCommitResult, String> {
    import_commit_into(store(), path, licence, source)
}

pub fn import_commit_into(
    store_ref: &crate::libstore::LibraryStore,
    path: &str,
    licence: Option<&str>,
    source: Option<&str>,
) -> Result<KicadCommitResult, String> {
    let (licence, source_url) = check_provenance(licence, source)?;
    let p = Path::new(path);
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "kicad_sym" && ext != "kicad_mod" {
        return Err("choose a .kicad_sym (symbol library) or .kicad_mod (footprint) file".into());
    }
    let file_name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "imported".into());
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("kicad");
    let safe_stem = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    let lib_name = format!("kicad_{safe_stem}");
    safe_name(&lib_name)?;

    let limits = Limits::default();
    let src = std::fs::read_to_string(p).map_err(|e| format!("cannot read the file: {e}"))?;

    let target_dir = store_ref.root().join("imported").join(&lib_name);
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("cannot create target library dir: {e}"))?;

    let mut component_ids = Vec::new();
    let mut rejected = Vec::new();

    let report = if ext == "kicad_sym" {
        let sym_lib = import_symbol_lib(&src, limits).map_err(|e| e.to_string())?;
        for sym in sym_lib.symbols {
            let mut id = sym.item_id.clone();
            if safe_name(&id).is_err() {
                id = id
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
            }
            if safe_name(&id).is_err() {
                rejected.push(format!("{}: invalid id {:?}", sym.name, id));
                continue;
            }
            let footprint = default_footprint_for_pins(&format!("{id}_FP"), &sym.def.pins);
            let pin_map = suggest_by_number(&sym.def, &footprint);
            let def = ComponentDef {
                id: id.clone(),
                name: sym.name.clone(),
                category: "Imported".into(),
                refdes_prefix: sym.refdes_prefix.unwrap_or_else(|| "U".into()),
                default_value: sym.value.unwrap_or_else(|| sym.name.clone()),
                manufacturer: None,
                mpn: None,
                distributor: None,
                supplier_offers: vec![],
                datasheet: sym.datasheet,
                symbol: sym.def,
                footprint,
                pin_map,
                ratings: vec![],
                model3d: eak_core::Model3d {
                    kind: "box".into(),
                    color: "#333333".into(),
                    external_ref: None,
                    model_ref: None,
                },
                provenance: format!(
                    "Imported KiCad symbol library {file_name} (licence {licence}; source \
                     {source_url}); footprint is an EAK placeholder with one pad per pin, not \
                     the real package; unverified by EAK"
                ),
            };
            let problems = def.validate();
            if !problems.is_empty() {
                rejected.push(format!("{}: {}", def.name, problems.join("; ")));
                continue;
            }
            let json = serde_json::to_string_pretty(&def).map_err(|e| e.to_string())?;
            let file_path = target_dir.join(format!("{id}.json"));
            std::fs::write(&file_path, json)
                .map_err(|e| format!("cannot write component file: {e}"))?;
            component_ids.push(id);
        }
        sym_lib.report
    } else {
        let fp_lib = import_footprint(&src, limits).map_err(|e| e.to_string())?;
        if let Some(fp_imp) = fp_lib.footprint {
            let mut id = fp_imp.item_id.clone();
            if safe_name(&id).is_err() {
                id = id
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
            }
            if safe_name(&id).is_ok() {
                let symbol_pins: Vec<SymbolPin> = fp_imp
                    .def
                    .pads
                    .iter()
                    .enumerate()
                    .map(|(i, pad)| SymbolPin {
                        number: pad.number.clone(),
                        name: pad.number.clone(),
                        electrical_type: PinType::Passive,
                        position: Point::new(-5_000_000, i as i64 * 2_540_000),
                    })
                    .collect();
                let pin_map = suggest_by_number(
                    &SymbolDef {
                        pins: symbol_pins.clone(),
                        body_half_w: 3_000_000,
                        body_half_h: (symbol_pins.len() as i64 * 2_540_000 / 2).max(2_000_000),
                    },
                    &fp_imp.def,
                );
                let def = ComponentDef {
                    id: id.clone(),
                    name: fp_imp.def.name.clone(),
                    category: "Imported".into(),
                    refdes_prefix: "U".into(),
                    default_value: fp_imp.def.name.clone(),
                    manufacturer: None,
                    mpn: None,
                    distributor: None,
                    supplier_offers: vec![],
                    datasheet: None,
                    symbol: SymbolDef {
                        pins: symbol_pins,
                        body_half_w: 3_000_000,
                        body_half_h: 3_000_000,
                    },
                    footprint: fp_imp.def,
                    pin_map,
                    ratings: vec![],
                    model3d: eak_core::Model3d {
                        kind: "box".into(),
                        color: "#333333".into(),
                        external_ref: None,
                        model_ref: None,
                    },
                    provenance: format!(
                        "Imported KiCad footprint {file_name} (licence {licence}; source \
                         {source_url}); symbol has one passive pin per pad; unverified by EAK"
                    ),
                };
                let problems = def.validate();
                if problems.is_empty() {
                    let json = serde_json::to_string_pretty(&def).map_err(|e| e.to_string())?;
                    let file_path = target_dir.join(format!("{id}.json"));
                    std::fs::write(&file_path, json)
                        .map_err(|e| format!("cannot write component file: {e}"))?;
                    component_ids.push(id);
                } else {
                    rejected.push(format!("{}: {}", def.name, problems.join("; ")));
                }
            }
        }
        fp_lib.report
    };

    let manifest = Manifest {
        name: lib_name.clone(),
        licence: licence.clone(),
        source_url: source_url.clone(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(target_dir.join("manifest.json"), manifest_json)
        .map_err(|e| format!("cannot write manifest.json: {e}"))?;

    Ok(KicadCommitResult {
        library_name: lib_name,
        licence,
        source_url,
        placeholder_footprints: ext == "kicad_sym",
        components_created: component_ids.len(),
        component_ids,
        rejected,
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

    #[test]
    fn default_footprint_generation_validates() {
        let pins = vec![
            SymbolPin {
                number: "1".into(),
                name: "IN".into(),
                electrical_type: PinType::Passive,
                position: Point::new(-5_000_000, 0),
            },
            SymbolPin {
                number: "2".into(),
                name: "OUT".into(),
                electrical_type: PinType::Passive,
                position: Point::new(5_000_000, 0),
            },
        ];
        let fp = default_footprint_for_pins("TEST_FP", &pins);
        assert_eq!(fp.pads.len(), 2);
        assert_eq!(fp.pads[0].number, "1");
        assert_eq!(fp.pads[1].number, "2");
    }

    const TEST_SYM: &str = r#"(kicad_symbol_lib (version 20231120) (generator "x")
      (symbol "RES_0805" (in_bom yes) (on_board yes)
        (property "Reference" "R" (at 0 0 0))
        (property "Value" "10k" (at 0 0 0))
        (symbol "RES_0805_1_1"
          (pin passive line (at 0 3.81 270) (length 1.27) (name "~") (number "1"))
          (pin passive line (at 0 -3.81 90) (length 1.27) (name "~") (number "2")))))"#;

    /// A KiCad file and an empty library store, both in a fresh temporary folder.
    fn fixture(name: &str) -> (std::path::PathBuf, crate::libstore::LibraryStore, String) {
        let tmp = std::env::temp_dir().join(format!("kicad_test_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let sym_file = tmp.join("test_pass.kicad_sym");
        std::fs::write(&sym_file, TEST_SYM).unwrap();
        let st = crate::libstore::LibraryStore::new(tmp.join("library"));
        (tmp, st, sym_file.display().to_string())
    }

    #[test]
    fn import_commit_creates_component_def_and_manifest_with_the_stated_licence() {
        let (tmp, st, file) = fixture("ok");
        let res =
            import_commit_into(&st, &file, Some(" MIT "), Some("https://example.com/lib")).unwrap();
        assert_eq!(res.components_created, 1);
        assert!(res.component_ids.contains(&"RES_0805".into()));
        assert_eq!(res.licence, "MIT");
        assert_eq!(res.source_url, "https://example.com/lib");
        assert!(
            res.placeholder_footprints,
            "symbol imports get placeholder footprints"
        );
        let (m, defs, problems) = st.imported().into_iter().next().unwrap();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(m.licence, "MIT");
        assert_eq!(m.source_url, "https://example.com/lib");
        assert!(defs[0].provenance.contains("licence MIT"));
        assert!(defs[0].provenance.contains("placeholder"));
        let _ = std::fs::remove_dir_all(tmp);
    }

    /// Regression (F2): a commit without a real licence or source used to record
    /// "Unspecified (KiCad import)" and https://kicad.org. It is now refused and writes nothing.
    #[test]
    fn import_commit_without_a_licence_or_source_is_refused_and_writes_nothing() {
        let (tmp, st, file) = fixture("refused");
        let cases: [(Option<&str>, Option<&str>); 8] = [
            (None, Some("https://example.com")),
            (Some("   "), Some("https://example.com")),
            (
                Some("Unspecified (KiCad import)"),
                Some("https://example.com"),
            ),
            (Some("unknown"), Some("https://example.com")),
            (Some("MIT"), None),
            (Some("MIT"), Some("kicad.org")),
            (Some("MIT"), Some("https://exa mple.com")),
            (Some("MIT"), Some("file:///etc/passwd")),
        ];
        for (licence, source) in cases {
            assert!(
                import_commit_into(&st, &file, licence, source).is_err(),
                "{licence:?} {source:?}"
            );
        }
        assert!(!st.root().join("imported").exists(), "nothing was written");
        let _ = std::fs::remove_dir_all(tmp);
    }
}
