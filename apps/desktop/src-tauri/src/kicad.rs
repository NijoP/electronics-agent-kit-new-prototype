//! Optional, one-way "Import from KiCad": run EAK's own importer (`eak_core::kicad_import`) on a
//! file the user picked and return its report, or commit valid definitions directly into EAK's
//! native library store with provenance and version metadata. EAK never needs a KiCad install or
//! KiCad libraries; this is an interchange option.

use crate::libstore::{safe_name, store, Manifest};
use eak_core::kicad_import::{import_footprint, import_symbol_lib, ImportReport};
use eak_core::pinmap::suggest_by_number;
use eak_core::sexpr::Limits;
use eak_core::{
    ComponentDef, FootprintDef, PadDef, PadKind, PadShape, PinDef, PinType, Point, SymbolDef,
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
    pub components_created: usize,
    pub component_ids: Vec<String>,
    pub rejected: Vec<String>,
    pub report: ImportReport,
}

/// Generates a valid footprint definition with pads corresponding to the symbol pins.
pub fn default_footprint_for_pins(name: &str, pins: &[PinDef]) -> FootprintDef {
    let mut pads = Vec::new();
    let count = pins.len();
    if count <= 2 {
        pads.push(PadDef {
            number: pins.first().map(|p| p.number.clone()).unwrap_or_else(|| "1".into()),
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
        let half = (count + 1) / 2;
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
    let (max_x, max_y) = pads.iter().fold((1_500_000i64, 1_500_000i64), |(mx, my), p| {
        (mx.max(p.position.x.abs() + p.size_x / 2), my.max(p.position.y.abs() + p.size_y / 2))
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

/// Parses and commits imported KiCad definitions directly into EAK's library store (`imported/` scope).
pub fn import_commit(
    path: &str,
    licence: Option<&str>,
    source: Option<&str>,
) -> Result<KicadCommitResult, String> {
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
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("kicad");
    let safe_stem = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') { c } else { '_' })
        .collect::<String>();
    let lib_name = format!("kicad_{safe_stem}");
    safe_name(&lib_name)?;

    let limits = Limits::default();
    let src = std::fs::read_to_string(p).map_err(|e| format!("cannot read the file: {e}"))?;

    let store_ref = store();
    let target_dir = store_ref.root().join("imported").join(&lib_name);
    std::fs::create_dir_all(&target_dir).map_err(|e| format!("cannot create target library dir: {e}"))?;

    let mut component_ids = Vec::new();
    let mut rejected = Vec::new();

    let report = if ext == "kicad_sym" {
        let sym_lib = import_symbol_lib(&src, limits).map_err(|e| e.to_string())?;
        for sym in sym_lib.symbols {
            let mut id = sym.item_id.clone();
            if safe_name(&id).is_err() {
                id = id
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') { c } else { '_' })
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
                model3d: eak_core::Model3d::default(),
                provenance: format!("Imported KiCad symbol library {file_name}; unverified by EAK"),
            };
            let problems = def.validate();
            if !problems.is_empty() {
                rejected.push(format!("{}: {}", def.name, problems.join("; ")));
                continue;
            }
            let json = serde_json::to_string_pretty(&def).map_err(|e| e.to_string())?;
            let file_path = target_dir.join(format!("{id}.json"));
            std::fs::write(&file_path, json).map_err(|e| format!("cannot write component file: {e}"))?;
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
                    .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') { c } else { '_' })
                    .collect();
            }
            if safe_name(&id).is_ok() {
                let symbol_pins: Vec<PinDef> = fp_imp
                    .def
                    .pads
                    .iter()
                    .enumerate()
                    .map(|(i, pad)| PinDef {
                        number: pad.number.clone(),
                        name: pad.number.clone(),
                        electrical_type: PinType::Passive,
                        position: Point::new(-5_000_000, i as i64 * 2_540_000),
                    })
                    .collect();
                let pin_map = suggest_by_number(&SymbolDef {
                    pins: symbol_pins.clone(),
                    body_half_w: 3_000_000,
                    body_half_h: (symbol_pins.len() as i64 * 2_540_000 / 2).max(2_000_000),
                }, &fp_imp.def);
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
                    model3d: eak_core::Model3d::default(),
                    provenance: format!("Imported KiCad footprint {file_name}; unverified by EAK"),
                };
                let problems = def.validate();
                if problems.is_empty() {
                    let json = serde_json::to_string_pretty(&def).map_err(|e| e.to_string())?;
                    let file_path = target_dir.join(format!("{id}.json"));
                    std::fs::write(&file_path, json).map_err(|e| format!("cannot write component file: {e}"))?;
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
        licence: licence.unwrap_or("Unspecified (KiCad import)").to_string(),
        source_url: source.unwrap_or("https://kicad.org").to_string(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(target_dir.join("manifest.json"), manifest_json)
        .map_err(|e| format!("cannot write manifest.json: {e}"))?;

    Ok(KicadCommitResult {
        library_name: lib_name,
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
            PinDef {
                number: "1".into(),
                name: "IN".into(),
                electrical_type: PinType::Input,
                position: Point::new(-5_000_000, 0),
            },
            PinDef {
                number: "2".into(),
                name: "OUT".into(),
                electrical_type: PinType::Output,
                position: Point::new(5_000_000, 0),
            },
        ];
        let fp = default_footprint_for_pins("TEST_FP", &pins);
        assert_eq!(fp.pads.len(), 2);
        assert_eq!(fp.pads[0].number, "1");
        assert_eq!(fp.pads[1].number, "2");
    }

    #[test]
    fn import_commit_creates_component_def_and_manifest() {
        let tmp = std::env::temp_dir().join(format!("kicad_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let sym_file = tmp.join("test_pass.kicad_sym");
        const TEST_SYM: &str = r#"(kicad_symbol_lib (version 20231120) (generator "x")
          (symbol "RES_0805" (in_bom yes) (on_board yes)
            (property "Reference" "R" (at 0 0 0))
            (property "Value" "10k" (at 0 0 0))
            (symbol "RES_0805_1_1"
              (pin passive line (at 0 3.81 270) (length 1.27) (name "~") (number "1"))
              (pin passive line (at 0 -3.81 90) (length 1.27) (name "~") (number "2")))))"#;
        std::fs::write(&sym_file, TEST_SYM).unwrap();

        let res = import_commit(
            sym_file.to_str().unwrap(),
            Some("MIT"),
            Some("https://example.com"),
        )
        .unwrap();
        assert_eq!(res.components_created, 1);
        assert!(res.component_ids.contains(&"RES_0805".into()));
        let _ = std::fs::remove_dir_all(tmp);
    }
}
