//! KiCad folder import into the library database, search, and user library export/import,
//! headless (library increment A4). Everything runs in temporary folders.

use eak_core::libmeta::LibraryMaturity;
use eak_desktop::library_import::{export_file_name, export_to, import_from, import_into, search};
use eak_library::db::LibraryDb;
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
    (symbol "EAK_ORPHAN_0_1" (rectangle (start -1 -2) (end 1 2) (stroke (width 0))))
    (symbol "EAK_ORPHAN_1_1"
      (pin input line (at -3.81 0 0) (length 1.27) (name "A") (number "1")))))
"#;

const FP: &str = r#"(footprint "R_A" (version 20240108) (generator "eak-test-fixture") (layer "F.Cu")
  (descr "resistor demo")
  (fp_line (start -1.7 -0.95) (end 1.7 -0.95) (layer "F.CrtYd") (width 0.05))
  (fp_line (start 1.7 -0.95) (end 1.7 0.95) (layer "F.CrtYd") (width 0.05))
  (pad "1" smd rect (at -0.9 0) (size 1 1.4) (layers "F.Cu"))
  (pad "2" smd rect (at 0.9 0) (size 1 1.4) (layers "F.Cu"))
  (model "${KIPRJMOD}/R_A.step"))
"#;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-libimport-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixture(name: &str) -> PathBuf {
    let d = tmp(name);
    std::fs::create_dir_all(d.join("DemoFP.pretty")).unwrap();
    std::fs::write(d.join("Demo.kicad_sym"), SYM).unwrap();
    std::fs::write(d.join("DemoFP.pretty").join("R_A.kicad_mod"), FP).unwrap();
    d
}

#[test]
fn folder_import_stores_items_with_licence_source_and_reports_gaps() {
    let dir = fixture("folder");
    let mut db = LibraryDb::open(&dir.join("db.sqlite")).unwrap();
    assert!(
        import_into(&mut db, &dir, "", "src").is_err(),
        "licence required"
    );
    assert!(
        import_into(&mut db, &dir, "CC0-1.0", " ").is_err(),
        "source required"
    );

    let sum = import_into(&mut db, &dir, "CC0-1.0", "eak test fixture").unwrap();
    assert_eq!(
        (sum.symbols, sum.footprints, sum.stored),
        (2, 1, 3),
        "{sum:?}"
    );
    assert_eq!(sum.unpaired, vec!["Demo:EAK_ORPHAN".to_string()]);
    assert_eq!(sum.models_found, 0, "no model file was written");
    assert_eq!(sum.models_missing, vec!["DemoFP:R_A".to_string()]);
    assert!(sum.problems.is_empty(), "{:?}", sum.problems);

    let found = search(&db, "eak", Some("symbol")).unwrap();
    let names: Vec<_> = found.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, vec!["EAK_ORPHAN", "EAK_R"]);
    for i in &found.items {
        assert_eq!(i.maturity, LibraryMaturity::Imported);
        assert_eq!(i.licence, "CC0-1.0");
    }
    // The paired footprint reference is searchable from the symbol.
    let by_fp = search(&db, "demofp r_a", Some("symbol")).unwrap();
    assert_eq!(by_fp.items.len(), 1);
    assert_eq!(search(&db, "resistor", None).unwrap().items[0].name, "R_A");
    assert!(search(&db, "", Some("nonsense")).is_err());
    assert_eq!(found.libraries.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn user_library_file_round_trips_between_databases() {
    let dir = fixture("roundtrip");
    let mut a = LibraryDb::open(&dir.join("a.sqlite")).unwrap();
    import_into(&mut a, &dir, "CC0-1.0", "eak test fixture").unwrap();
    let out = tmp("roundtrip-out");
    let path = export_to(&a, "Demo", &out).unwrap();
    assert!(path.ends_with(&export_file_name("Demo")));
    assert!(export_to(&a, "Nothing", &out).is_err());

    let mut b = LibraryDb::open(&dir.join("b.sqlite")).unwrap();
    let r = import_from(&mut b, std::path::Path::new(&path), Some("Copy")).unwrap();
    assert_eq!((r.library.as_str(), r.imported), ("Copy", 2));
    assert!(r.rejected.is_empty());
    let copy = search(&b, "", None).unwrap();
    assert_eq!(copy.items.len(), 2);
    assert!(copy.items.iter().all(|i| i.library == "Copy"));
    assert_eq!(
        export_file_name("my lib/../x"),
        "my_lib____x.eak-library.json"
    );
    drop((a, b));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&out);
}
