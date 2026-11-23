//! ECO preview contract (Phase 1 C2, ADR-0041): `sync_preview` is read-only, returns exactly the
//! delta that applying `SyncSchematicToPcb` (as the dialog's `Batch`) journals, and refuses a
//! stale revision.

use eak_core::{Point, Rotation};
use eak_desktop::eco::{sync_preview, sync_preview_export};
use eak_desktop::workspace::operation_changes;
use eak_desktop::*;
use eak_runtime::command::Command as C;
use eak_runtime::delta::Change;
use eak_runtime::index::Index;
use eak_runtime::CommandError;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-eco-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn session() -> Session {
    Session::new(Index::in_memory())
}

fn path(d: &Path) -> String {
    d.display().to_string()
}

fn place(x: f64, y: f64) -> C {
    C::PlaceSymbol {
        sheet_id: None,
        component_id: "R_0603".into(),
        position: Point::mm(x, y),
        rotation: Rotation::default(),
        refdes: None,
        value: None,
    }
}

fn outline() -> C {
    C::SetBoardOutline {
        points: vec![
            Point::mm(0.0, 0.0),
            Point::mm(30.0, 0.0),
            Point::mm(30.0, 20.0),
            Point::mm(0.0, 20.0),
        ],
    }
}

/// What the dialog sends for "Apply": the sync as a one-step Batch.
fn apply_all() -> C {
    C::Batch {
        commands: vec![C::SyncSchematicToPcb],
        label: Some("Update PCB (ECO)".into()),
    }
}

type Row = (Vec<String>, Option<String>, Option<Value>, Option<Value>);

/// Footprints the sync creates get a fresh random id each time it runs, so compare footprint
/// changes by the symbol they realise and drop the id inside the value. Every other field must
/// match exactly.
fn normalized(changes: &[Change]) -> Vec<Row> {
    let strip = |v: &Option<Value>| {
        v.clone().map(|mut v| {
            if let Some(o) = v.as_object_mut() {
                o.remove("id");
            }
            v
        })
    };
    let mut out: Vec<Row> = changes
        .iter()
        .map(|c| {
            if c.path == ["pcb", "footprints"] {
                let sym = c
                    .after
                    .as_ref()
                    .or(c.before.as_ref())
                    .and_then(|v| v.get("symbol_id"))
                    .and_then(Value::as_str)
                    .map(String::from);
                (c.path.clone(), sym, strip(&c.before), strip(&c.after))
            } else {
                (
                    c.path.clone(),
                    c.key.clone(),
                    c.before.clone(),
                    c.after.clone(),
                )
            }
        })
        .collect();
    out.sort_by_key(|r| format!("{:?}{:?}", r.0, r.1));
    out
}

/// Small deterministic generator (xorshift64), so a failure reproduces from the seed.
struct Rng(u64);
impl Rng {
    fn bits(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.bits() % n as u64) as usize
    }
}

/// Property: for random schematic edit sequences, the preview at revision R equals the change
/// list the engine journals when the dialog's Batch is applied at R, and the preview itself
/// changes nothing.
#[test]
fn preview_equals_the_applied_delta_for_random_edit_sequences() {
    for seed in [
        0x9e37_79b9_7f4a_7c15_u64,
        0x0123_4567_89ab_cdef,
        0xdead_beef_cafe_f00d,
    ] {
        let d = tmpdir(&format!("prop-{seed:x}"));
        let s = session();
        let snap = create_project(&s, &path(&d), "ECO property").unwrap();
        let id = snap.project.id.to_string();
        let mut rng = Rng(seed);
        let mut rev = execute_command(&s, 0, outline()).unwrap().revision_after;
        let mut compared = 0;
        for step in 0..40 {
            let symbols: Vec<String> = current_project(&s)
                .unwrap()
                .project
                .schematic
                .symbols
                .keys()
                .cloned()
                .collect();
            // One random schematic edit (refused edits are fine: they leave the project as is).
            let pick = |rng: &mut Rng| symbols[rng.below(symbols.len())].clone();
            let edit = match rng.below(if symbols.len() < 2 { 1 } else { 5 }) {
                0 => place(20.0 + rng.below(200) as f64, -40.0 - rng.below(100) as f64),
                1 => C::ConnectPins {
                    a_symbol: pick(&mut rng),
                    a_pin: "1".into(),
                    b_symbol: pick(&mut rng),
                    b_pin: "2".into(),
                },
                2 => C::SetRefdes {
                    symbol_id: pick(&mut rng),
                    refdes: format!("R{}", 100 + step),
                },
                3 => C::DeleteSymbol {
                    symbol_id: pick(&mut rng),
                },
                _ => C::SyncSchematicToPcb,
            };
            let r = execute_command(&s, rev, edit).unwrap();
            if r.accepted {
                rev = r.revision_after;
            }
            if rng.below(3) != 0 {
                continue;
            }
            let before = current_project(&s).unwrap().project;
            let preview = sync_preview(&s, rev).unwrap();
            assert_eq!(preview.revision, rev);
            assert_eq!(
                current_project(&s).unwrap().project,
                before,
                "the preview is read-only"
            );
            let applied = execute_command(&s, rev, apply_all()).unwrap();
            if preview.changes.is_empty() {
                assert!(
                    !applied.accepted,
                    "an empty preview means there is nothing to apply"
                );
                assert!(matches!(applied.error, Some(CommandError::NothingTo(_))));
                continue;
            }
            assert!(
                applied.accepted,
                "seed {seed:x} step {step}: {:?}",
                applied.error
            );
            let journalled = operation_changes(&s, &id, &applied.op_id).unwrap();
            assert_eq!(
                normalized(&preview.changes),
                normalized(&journalled),
                "seed {seed:x} step {step}: preview and applied delta differ"
            );
            compared += 1;
            rev = applied.revision_after;
            // After applying, the PCB matches the schematic: the next preview is empty.
            assert!(sync_preview(&s, rev).unwrap().changes.is_empty());
        }
        assert!(
            compared >= 3,
            "seed {seed:x}: only {compared} non-empty previews compared"
        );
        close_project(&s).unwrap();
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[test]
fn stale_preview_and_stale_apply_are_rejected() {
    let d = tmpdir("stale");
    let s = session();
    create_project(&s, &path(&d), "ECO stale").unwrap();
    let mut rev = 0;
    for c in [outline(), place(40.0, -50.0), place(80.0, -50.0)] {
        let r = execute_command(&s, rev, c).unwrap();
        assert!(r.accepted, "{:?}", r.error);
        rev = r.revision_after;
    }
    let preview = sync_preview(&s, rev).unwrap();
    assert_eq!(
        preview
            .changes
            .iter()
            .filter(|c| c.path == ["pcb", "footprints"] && c.before.is_none())
            .count(),
        2,
        "two footprints to add: {:?}",
        preview.changes
    );
    // The project moves on (another window, a terminal agent): the old preview is stale.
    let moved = execute_command(&s, rev, place(120.0, -50.0)).unwrap();
    assert!(moved.accepted);
    let err = sync_preview(&s, rev).unwrap_err();
    assert!(err.contains("stale revision"), "{err}");
    let late = execute_command(&s, preview.revision, apply_all()).unwrap();
    assert!(!late.accepted);
    assert!(matches!(
        late.error,
        Some(CommandError::StaleRevision { .. })
    ));
    assert!(current_project(&s)
        .unwrap()
        .project
        .pcb
        .footprints
        .is_empty());
    assert!(
        sync_preview_export(&s, rev, vec![]).is_err(),
        "export is stale too"
    );
    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn export_saves_the_engines_preview_as_evidence_without_applying() {
    let d = tmpdir("export");
    let s = session();
    create_project(&s, &path(&d), "ECO export").unwrap();
    let mut rev = 0;
    for c in [outline(), place(40.0, -50.0)] {
        let r = execute_command(&s, rev, c).unwrap();
        assert!(r.accepted, "{:?}", r.error);
        rev = r.revision_after;
    }
    let file = sync_preview_export(&s, rev, vec!["fp:R1".into()]).unwrap();
    assert!(file.ends_with(&format!("eco-r{rev:06}.json")), "{file}");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(v["kind"], "eco_preview");
    assert_eq!(v["revision"], rev);
    assert_eq!(v["excluded"][0], "fp:R1");
    assert_eq!(v["changes"].as_array().unwrap().len(), 1);
    let now = current_project(&s).unwrap().project;
    assert_eq!(now.revision, rev, "exporting applies nothing");
    assert!(now.pcb.footprints.is_empty());
    assert!(sync_preview_export(&s, rev, vec!["x".repeat(300)]).is_err());
    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}
