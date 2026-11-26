//! Folder import over a fixture folder written by the test (no KiCad library file is copied):
//! one `.kicad_sym`, one `.pretty` with two footprints, and an `fp-lib-table`.

use eak_core::libmeta::LibraryMaturity;
use eak_library::kicad_lib::{
    import_folder, scan_folder, ImportOptions, LibOrigin, ModelFormat, Pairing,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

const SYM: &str = r#"(kicad_symbol_lib (version 20231120) (generator "eak-test-fixture")
  (symbol "EAK_R" (in_bom yes) (on_board yes)
    (property "Reference" "R" (at 0 0 0))
    (property "Value" "EAK_R" (at 0 0 0))
    (property "Footprint" "DemoFP:R_A" (at 0 0 0))
    (symbol "EAK_R_0_1" (rectangle (start -1 -2) (end 1 2) (stroke (width 0))))
    (symbol "EAK_R_1_1"
      (pin passive line (at 0 3.81 270) (length 1.27) (name "~") (number "1"))
      (pin passive line (at 0 -3.81 90) (length 1.27) (name "~") (number "2"))))
  (symbol "EAK_ORPHAN" (in_bom yes) (on_board yes)
    (property "Reference" "U" (at 0 0 0))
    (property "Footprint" "DemoFP:Not_There" (at 0 0 0))
    (symbol "EAK_ORPHAN_0_1" (rectangle (start -1 -2) (end 1 2) (stroke (width 0))))
    (symbol "EAK_ORPHAN_1_1"
      (pin input line (at -3.81 0 0) (length 1.27) (name "A") (number "1")))))
"#;

const FP_A: &str = r#"(footprint "R_A" (version 20240108) (generator "eak-test-fixture") (layer "F.Cu")
  (fp_line (start -1.7 -0.95) (end 1.7 -0.95) (layer "F.CrtYd") (width 0.05))
  (fp_line (start 1.7 -0.95) (end 1.7 0.95) (layer "F.CrtYd") (width 0.05))
  (pad "1" smd rect (at -0.9 0) (size 1 1.4) (layers "F.Cu" "F.Paste" "F.Mask"))
  (pad "2" smd rect (at 0.9 0) (size 1 1.4) (layers "F.Cu" "F.Paste" "F.Mask"))
  (model "${KIPRJMOD}/models/R_A.wrl" (offset (xyz 0 0 0))))
"#;

const FP_B: &str = r#"(footprint "R_B" (version 20240108) (generator "eak-test-fixture") (layer "F.Cu")
  (fp_line (start -1.7 -0.95) (end 1.7 -0.95) (layer "F.CrtYd") (width 0.05))
  (fp_line (start 1.7 -0.95) (end 1.7 0.95) (layer "F.CrtYd") (width 0.05))
  (pad "1" smd rect (at -0.9 0) (size 1 1.4) (layers "F.Cu"))
  (pad "2" smd rect (at 0.9 0) (size 1 1.4) (layers "F.Cu"))
  (model "${EAK_TEST_UNSET_3DMODEL_DIR}/R_B.step"))
"#;

const FP_TABLE: &str = r#"(fp_lib_table (version 7)
  (lib (name "DemoFP")(type "KiCad")(uri "${KIPRJMOD}/DemoFP.pretty")(options "")(descr "fixture"))
  (lib (name "Gone")(type "KiCad")(uri "${KIPRJMOD}/Gone.pretty")(options "")(descr ""))
)
"#;

/// Write the fixture folder; returns its path. Each test gets its own folder.
fn fixture(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eak-kicad-lib-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("DemoFP.pretty")).unwrap();
    std::fs::create_dir_all(dir.join("models")).unwrap();
    std::fs::write(dir.join("Demo.kicad_sym"), SYM).unwrap();
    std::fs::write(dir.join("DemoFP.pretty").join("R_A.kicad_mod"), FP_A).unwrap();
    std::fs::write(dir.join("DemoFP.pretty").join("R_B.kicad_mod"), FP_B).unwrap();
    std::fs::write(dir.join("fp-lib-table"), FP_TABLE).unwrap();
    // Only the STEP twin of R_A's VRML model exists.
    std::fs::write(dir.join("models").join("R_A.step"), "ISO-10303-21;\n").unwrap();
    dir
}

fn opts() -> ImportOptions {
    ImportOptions {
        vars: BTreeMap::new(),
        licence: "CC0-1.0".into(),
        source: "eak test fixture".into(),
    }
}

#[test]
fn scan_lists_table_entries_and_folder_libraries() {
    let dir = fixture("scan");
    let scan = scan_folder(&dir, &BTreeMap::new());
    assert!(scan.problems.is_empty(), "{:?}", scan.problems);
    assert_eq!(scan.symbol_libs.len(), 1);
    assert_eq!(scan.symbol_libs[0].name, "Demo");
    assert_eq!(scan.symbol_libs[0].origin, LibOrigin::Folder);
    assert!(scan.symbol_libs[0].usable);
    // DemoFP from the table (not listed twice by the folder scan), and the missing one.
    assert_eq!(scan.footprint_libs.len(), 2, "{:?}", scan.footprint_libs);
    let demo = &scan.footprint_libs[0];
    assert_eq!(
        (demo.name.as_str(), demo.origin, demo.usable),
        ("DemoFP", LibOrigin::Table, true)
    );
    let gone = &scan.footprint_libs[1];
    assert_eq!(gone.name, "Gone");
    assert!(!gone.exists && !gone.usable);
    assert!(!gone.notes.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn import_pairs_reports_unpaired_and_carries_metadata() {
    let dir = fixture("import");
    let imp = import_folder(&dir, &opts()).unwrap();
    assert!(imp.files.iter().all(|f| f.error.is_none()), "{:?}", imp.files);
    assert_eq!(imp.files.len(), 3);
    assert_eq!(imp.footprints.len(), 2);
    assert_eq!(imp.symbols.len(), 2, "an unpaired symbol is kept, not dropped");

    let r = imp
        .symbols
        .iter()
        .find(|s| s.symbol.name == "EAK_R")
        .unwrap();
    assert_eq!(
        r.pairing,
        Pairing::Paired {
            library: "DemoFP".into(),
            footprint: "R_A".into()
        }
    );
    let orphan = imp
        .symbols
        .iter()
        .find(|s| s.symbol.name == "EAK_ORPHAN")
        .unwrap();
    assert_eq!(
        orphan.pairing,
        Pairing::NotFound {
            reference: "DemoFP:Not_There".into()
        }
    );
    assert_eq!(imp.unpaired, vec!["Demo:EAK_ORPHAN".to_string()]);

    for meta in imp
        .symbols
        .iter()
        .map(|s| &s.meta)
        .chain(imp.footprints.iter().map(|f| &f.meta))
    {
        assert_eq!(meta.maturity, LibraryMaturity::Imported);
        assert_eq!(meta.licence, "CC0-1.0");
        assert_eq!(meta.source, "eak test fixture");
        assert!(!meta.file.is_empty());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn model_references_are_found_only_on_disk() {
    let dir = fixture("models");
    let imp = import_folder(&dir, &opts()).unwrap();
    let a = imp.footprints.iter().find(|f| f.name == "R_A").unwrap();
    let ma = a.model.as_ref().unwrap();
    assert_eq!(ma.format, ModelFormat::Wrl);
    assert!(!ma.found, "the .wrl file was never written");
    assert!(ma.resolved.is_some());
    assert!(ma
        .step_alternative
        .as_deref()
        .is_some_and(|p| p.ends_with("R_A.step")));

    let b = imp.footprints.iter().find(|f| f.name == "R_B").unwrap();
    let mb = b.model.as_ref().unwrap();
    assert_eq!(mb.format, ModelFormat::Step);
    assert!(!mb.found);
    assert_eq!(mb.resolved, None);
    assert_eq!(
        mb.unresolved_vars,
        vec!["EAK_TEST_UNSET_3DMODEL_DIR".to_string()]
    );

    // Setting the variable resolves it, and the file is then found.
    let mut o = opts();
    o.vars.insert(
        "EAK_TEST_UNSET_3DMODEL_DIR".into(),
        dir.join("models").to_string_lossy().into_owned(),
    );
    std::fs::write(dir.join("models").join("R_B.step"), "ISO-10303-21;\n").unwrap();
    let imp = import_folder(&dir, &o).unwrap();
    let b = imp.footprints.iter().find(|f| f.name == "R_B").unwrap();
    assert!(b.model.as_ref().unwrap().found);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn licence_and_source_are_required() {
    let dir = fixture("required");
    let mut o = opts();
    o.licence = " ".into();
    assert!(import_folder(&dir, &o).is_err());
    let mut o = opts();
    o.source = String::new();
    assert!(import_folder(&dir, &o).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
