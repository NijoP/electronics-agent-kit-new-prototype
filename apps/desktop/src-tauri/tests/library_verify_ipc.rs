//! Footprint and pin-to-pad verification of library parts and component create/edit in the
//! personal library, through the desktop command layer, headless. The store runs in a temporary
//! folder; nothing touches the real configuration directory.

use eak_core::builtin;
use eak_desktop::libstore::LibraryStore;
use eak_desktop::parts::{component_save_in, find_def, footprint_verify_in, pin_pad_verify_in};
use eak_validation::CheckState;
use std::collections::BTreeMap;

fn store(name: &str) -> LibraryStore {
    let d = std::env::temp_dir().join(format!("eak-libverify-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    LibraryStore::new(d.join("library"))
}

#[test]
fn a_builtin_part_without_a_land_pattern_is_never_footprint_verified() {
    let st = store("fp");
    let r = footprint_verify_in(&st, &BTreeMap::new(), "LDO_SOT-23-5").unwrap();
    assert_eq!(r.state_of("FP-001"), CheckState::Pass);
    assert_eq!(r.state_of("FP-002"), CheckState::Unknown);
    assert!(!r.verifies());
    assert_ne!(r.overall, CheckState::Pass);
    assert!(r.not_implemented.iter().any(|n| n.starts_with("FP-010")));
    assert!(footprint_verify_in(&st, &BTreeMap::new(), "NO_SUCH_PART").is_err());
}

#[test]
fn pin_pad_report_of_a_builtin_part_passes_and_a_broken_project_copy_fails() {
    let st = store("pp");
    let r = pin_pad_verify_in(&st, &BTreeMap::new(), "LMR33630_HSOIC-8").unwrap();
    assert!(r.verifies(), "{:?}", r.blocking());

    // A project definition with the same id wins over the built-in one.
    let mut broken = builtin::get("LMR33630_HSOIC-8").unwrap();
    broken.pin_map.insert("5".into(), vec!["9".into()]);
    broken.pin_map.insert("9".into(), vec!["5".into()]);
    let project = BTreeMap::from([(broken.id.clone(), broken)]);
    let found = find_def(&st, &project, "LMR33630_HSOIC-8").unwrap();
    assert_eq!(found.scope, "project");
    let r = pin_pad_verify_in(&st, &project, "LMR33630_HSOIC-8").unwrap();
    assert_eq!(r.state_of("PP-002"), CheckState::Fail);
    assert!(!r.verifies());
}

#[test]
fn a_created_component_is_saved_to_the_personal_library_and_can_be_edited() {
    let st = store("save");
    let mut def = builtin::get("LDO_SOT-23-5").unwrap();
    def.id = "MY_LDO".into();
    def.name = "My LDO".into();
    let saved = component_save_in(&st, def.clone()).unwrap();
    assert_eq!(saved.scope, "personal");
    assert!(!saved.replaced);
    assert!(saved.pin_pad.verifies());
    assert!(!saved.footprint.verifies());
    let found = find_def(&st, &BTreeMap::new(), "MY_LDO").unwrap();
    assert_eq!(found.scope, "personal");
    assert_eq!(found.def.name, "My LDO");

    def.name = "My LDO, edited".into();
    let saved = component_save_in(&st, def).unwrap();
    assert!(saved.replaced);
    let found = find_def(&st, &BTreeMap::new(), "MY_LDO").unwrap();
    assert_eq!(found.def.name, "My LDO, edited");
    assert_eq!(st.personal().0.len(), 1);
}

#[test]
fn a_component_whose_pin_map_is_wrong_is_refused_and_not_written() {
    let st = store("refuse");
    let mut def = builtin::get("LDO_SOT-23-5").unwrap();
    def.id = "BAD_LDO".into();
    def.pin_map.insert("3".into(), vec!["2".into()]); // EN onto the GND pad
    let e = component_save_in(&st, def).unwrap_err();
    assert!(e.contains("pin map"), "{e}");
    assert!(find_def(&st, &BTreeMap::new(), "BAD_LDO").is_err());
    assert!(st.personal().0.is_empty());
}
