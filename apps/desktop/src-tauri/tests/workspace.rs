//! IDE-shell backend contract: several open projects with one active, the sandboxed project file
//! explorer, and the engine's journalled change list behind the Update PCB report.

use eak_core::{Point, Rotation};
use eak_desktop::workspace::*;
use eak_desktop::*;
use eak_runtime::command::Command as C;
use eak_runtime::index::Index;
use std::path::{Path, PathBuf};

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-shell-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn session() -> Session {
    Session::new(Index::in_memory())
}

fn path(d: &Path) -> String {
    d.display().to_string()
}

fn place(x: f64) -> C {
    C::PlaceSymbol {
        sheet_id: None,
        component_id: "R_0603".into(),
        position: Point::mm(x, -50.0),
        rotation: Rotation::default(),
        refdes: None,
        value: None,
    }
}

fn names(v: &[FileEntry]) -> Vec<&str> {
    v.iter().map(|e| e.name.as_str()).collect()
}

#[test]
fn two_projects_stay_open_and_commands_only_reach_the_active_one() {
    let (da, db) = (tmpdir("pa"), tmpdir("pb"));
    let s = session();
    let a = create_project(&s, &path(&da), "Alpha").unwrap();
    let b = create_project(&s, &path(&db), "Beta").unwrap();
    assert_ne!(a.project.id, b.project.id);

    let open = open_projects(&s).unwrap();
    assert_eq!(open.len(), 2);
    assert_eq!(open.iter().filter(|p| p.active).count(), 1);
    assert_eq!(current_project(&s).unwrap().project.name, "Beta");

    // A command goes to the active project (Beta) only.
    let r = execute_command(&s, b.project.revision, place(10.0)).unwrap();
    assert!(r.accepted);
    let a_snap = switch_project(&s, &a.project.id.to_string()).unwrap();
    assert_eq!(a_snap.project.name, "Alpha");
    assert_eq!(
        a_snap.project.revision, a.project.revision,
        "Alpha was not touched"
    );
    assert!(a_snap.project.schematic.symbols.is_empty());
    assert_eq!(current_project(&s).unwrap().project.name, "Alpha");

    // The same revision-checked command now lands in Alpha, leaving Beta alone.
    let r = execute_command(&s, a.project.revision, place(20.0)).unwrap();
    assert!(r.accepted);
    let b_again = switch_project(&s, &b.project.id.to_string()).unwrap();
    assert_eq!(b_again.project.schematic.symbols.len(), 1);
    assert!(switch_project(&s, "nope").is_err());

    // Closing a project that is not active keeps the active one.
    let after = close_project_by_id(&s, &a.project.id.to_string()).unwrap();
    assert_eq!(after.unwrap().project.name, "Beta");
    assert_eq!(open_projects(&s).unwrap().len(), 1);
    // Closing the last one leaves nothing open.
    assert!(close_project_by_id(&s, &b.project.id.to_string())
        .unwrap()
        .is_none());
    assert!(current_project(&s).is_err());
    let _ = std::fs::remove_dir_all(&da);
    let _ = std::fs::remove_dir_all(&db);
}

#[test]
fn closing_the_active_project_activates_the_previous_one_and_reopening_does_not_duplicate() {
    let (da, db) = (tmpdir("ca"), tmpdir("cb"));
    let s = session();
    create_project(&s, &path(&da), "Alpha").unwrap();
    let b = create_project(&s, &path(&db), "Beta").unwrap();
    close_project(&s).unwrap();
    assert_eq!(current_project(&s).unwrap().project.name, "Alpha");
    // Opening a folder that is already open re-reads it instead of adding a second copy.
    open_project(&s, &path(&db), false).unwrap();
    open_project(&s, &path(&db), false).unwrap();
    let open = open_projects(&s).unwrap();
    assert_eq!(open.len(), 2);
    assert!(open
        .iter()
        .any(|p| p.id == b.project.id.to_string() && p.active));
    let _ = std::fs::remove_dir_all(&da);
    let _ = std::fs::remove_dir_all(&db);
}

#[test]
fn explorer_lists_real_project_files_and_marks_engine_owned_ones() {
    let d = tmpdir("list");
    let s = session();
    let snap = create_project(&s, &path(&d), "Explorer").unwrap();
    let id = snap.project.id.to_string();
    std::fs::create_dir_all(d.join("output").join("rev-1")).unwrap();
    std::fs::write(d.join("output").join("rev-1").join("BOM.csv"), "a,b\n").unwrap();
    std::fs::write(d.join("notes.md"), "# notes\n").unwrap();
    std::fs::write(d.join("eak.toml"), "x = 1\n").unwrap();
    execute_command(&s, 0, place(10.0)).unwrap(); // the journal appears with the first command

    let root = project_files(&s, &id, "").unwrap();
    let n = names(&root);
    for want in [
        "project.eak.json",
        "journal.jsonl",
        "output",
        "notes.md",
        "eak.toml",
    ] {
        assert!(n.contains(&want), "{want} listed in {n:?}");
    }
    // Folders first, then files by name.
    assert!(root[0].is_dir);
    let design = root.iter().find(|e| e.name == "project.eak.json").unwrap();
    assert!(design.protected && design.role == "design");
    assert!(
        root.iter()
            .find(|e| e.name == "journal.jsonl")
            .unwrap()
            .protected
    );
    let toml = root.iter().find(|e| e.name == "eak.toml").unwrap();
    assert!(!toml.protected && toml.role == "config");
    let output = root.iter().find(|e| e.name == "output").unwrap();
    assert!(!output.protected && output.role == "generated");

    // Children inherit the "generated" role and use `/` separators.
    let rev = project_files(&s, &id, "output/rev-1").unwrap();
    assert_eq!(rev[0].rel, "output/rev-1/BOM.csv");
    assert_eq!(rev[0].role, "generated");
    // Backslash separators from Windows callers resolve to the same folder.
    assert_eq!(project_files(&s, &id, "output\\rev-1").unwrap(), rev);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn explorer_reads_text_binary_and_large_files_without_loading_everything() {
    let d = tmpdir("read");
    let s = session();
    let id = create_project(&s, &path(&d), "Read")
        .unwrap()
        .project
        .id
        .to_string();
    std::fs::write(d.join("a.txt"), "hello").unwrap();
    std::fs::write(d.join("b.bin"), [0u8, 1, 2, 3]).unwrap();
    std::fs::write(
        d.join("big.txt"),
        vec![b'x'; usize::try_from(MAX_READ_BYTES).unwrap() + 10],
    )
    .unwrap();
    let a = project_file_read(&s, &id, "a.txt").unwrap();
    assert_eq!(a.text.as_deref(), Some("hello"));
    let b = project_file_read(&s, &id, "b.bin").unwrap();
    assert!(b.binary && b.text.is_none());
    let big = project_file_read(&s, &id, "big.txt").unwrap();
    assert!(big.truncated && big.text.is_none());
    // The design file itself can be read (viewing is harmless).
    assert!(project_file_read(&s, &id, "project.eak.json")
        .unwrap()
        .text
        .is_some());
    assert!(project_file_read(&s, &id, "").is_err());
    assert!(project_file_read(&s, &id, "nope.txt").is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn explorer_creates_renames_and_deletes_inside_the_project() {
    let d = tmpdir("crud");
    let s = session();
    let id = create_project(&s, &path(&d), "Crud")
        .unwrap()
        .project
        .id
        .to_string();

    project_file_create(&s, &id, "scripts", true).unwrap();
    let f = project_file_create(&s, &id, "scripts/check.sh", false).unwrap();
    assert_eq!(f.rel, "scripts/check.sh");
    assert!(d.join("scripts").join("check.sh").is_file());
    assert!(
        project_file_create(&s, &id, "scripts/check.sh", false).is_err(),
        "no overwrite"
    );
    assert!(
        project_file_create(&s, &id, "missing/dir/x.txt", false).is_err(),
        "no parent"
    );

    project_file_rename(&s, &id, "scripts/check.sh", "scripts/validate.sh").unwrap();
    assert!(!d.join("scripts").join("check.sh").exists());
    assert!(d.join("scripts").join("validate.sh").is_file());
    project_file_create(&s, &id, "scripts/other.sh", false).unwrap();
    assert!(
        project_file_rename(&s, &id, "scripts/other.sh", "scripts/validate.sh").is_err(),
        "no overwrite"
    );

    // A non-empty folder needs explicit confirmation.
    assert!(project_file_delete(&s, &id, "scripts", false).is_err());
    assert!(d.join("scripts").exists());
    project_file_delete(&s, &id, "scripts", true).unwrap();
    assert!(!d.join("scripts").exists());
    assert!(project_file_delete(&s, &id, "scripts", true).is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn explorer_refuses_paths_outside_the_project_and_bad_names() {
    let d = tmpdir("escape");
    let outside = tmpdir("escape-outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.txt"), "secret").unwrap();
    let s = session();
    let id = create_project(&s, &path(&d), "Escape")
        .unwrap()
        .project
        .id
        .to_string();
    std::fs::write(d.join("keep.txt"), "k").unwrap();

    for rel in [
        "../x",
        "a/../../x",
        "..",
        "/etc/passwd",
        "\\windows\\x",
        "C:/x",
        "a\\..\\..\\x",
    ] {
        assert!(project_files(&s, &id, rel).is_err(), "list {rel}");
        assert!(project_file_read(&s, &id, rel).is_err(), "read {rel}");
        assert!(
            project_file_create(&s, &id, rel, false).is_err(),
            "create {rel}"
        );
        assert!(
            project_file_delete(&s, &id, rel, true).is_err(),
            "delete {rel}"
        );
        assert!(
            project_file_rename(&s, &id, "keep.txt", rel).is_err(),
            "rename to {rel}"
        );
    }
    for name in ["", "a|b", "a?b", "NUL", "con.txt", "x.", "x ", "a\u{1}b"] {
        assert!(
            project_file_create(&s, &id, name, false).is_err(),
            "name {name:?}"
        );
    }
    assert!(!outside.join("x").exists());
    assert_eq!(
        std::fs::read_to_string(outside.join("secret.txt")).unwrap(),
        "secret"
    );

    #[cfg(unix)]
    {
        // A symlink that points out of the project can be neither followed, read through, nor
        // used as a parent; deleting it removes only the link.
        std::os::unix::fs::symlink(&outside, d.join("link")).unwrap();
        assert!(project_files(&s, &id, "link").is_err());
        assert!(project_file_read(&s, &id, "link/secret.txt").is_err());
        assert!(project_file_create(&s, &id, "link/new.txt", false).is_err());
        let root = project_files(&s, &id, "").unwrap();
        assert!(
            !root.iter().find(|e| e.name == "link").unwrap().is_dir,
            "listed as a leaf"
        );
        project_file_delete(&s, &id, "link", true).unwrap();
        assert!(!d.join("link").exists());
        assert!(outside.join("secret.txt").exists(), "link target untouched");
    }
    let _ = std::fs::remove_dir_all(&d);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn explorer_cannot_change_engine_owned_files() {
    let d = tmpdir("owned");
    let s = session();
    let id = create_project(&s, &path(&d), "Owned")
        .unwrap()
        .project
        .id
        .to_string();
    execute_command(&s, 0, place(10.0)).unwrap();
    std::fs::create_dir_all(d.join("checkpoints")).unwrap();
    std::fs::write(d.join("checkpoints").join("c1.json"), "{}").unwrap();
    let before = std::fs::read_to_string(d.join("project.eak.json")).unwrap();

    for rel in [
        "project.eak.json",
        "journal.jsonl",
        ".lock",
        "checkpoints",
        "checkpoints/c1.json",
        "evidence",
    ] {
        assert!(
            project_file_delete(&s, &id, rel, true).is_err(),
            "delete {rel}"
        );
        assert!(
            project_file_rename(&s, &id, rel, "renamed").is_err(),
            "rename {rel}"
        );
        assert!(
            project_file_rename(&s, &id, "notes", rel).is_err(),
            "rename onto {rel}"
        );
        assert!(
            project_file_create(&s, &id, rel, false).is_err(),
            "create {rel}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(d.join("project.eak.json")).unwrap(),
        before
    );
    assert!(d.join("checkpoints").join("c1.json").exists());
    // The design is still intact and writable through the engine.
    assert!(execute_command(&s, 1, place(20.0)).unwrap().accepted);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn explorer_calls_only_ever_act_inside_the_root_they_name() {
    let (da, db) = (tmpdir("ma"), tmpdir("mb"));
    let s = session();
    let a = create_project(&s, &path(&da), "A")
        .unwrap()
        .project
        .id
        .to_string();
    let b = create_project(&s, &path(&db), "B")
        .unwrap()
        .project
        .id
        .to_string();
    // B is active; a call that names A still lands in A's folder only, never B's.
    project_file_create(&s, &a, "x.txt", false).unwrap();
    assert!(da.join("x.txt").exists() && !db.join("x.txt").exists());
    project_file_create(&s, &b, "y.txt", false).unwrap();
    assert!(db.join("y.txt").exists() && !da.join("y.txt").exists());
    // An unknown or closed project id is refused, not redirected to the active project.
    assert!(project_files(&s, "no-such-project", "").is_err());
    close_project_by_id(&s, &a).unwrap();
    assert!(project_file_create(&s, &a, "z.txt", false).is_err());
    assert!(!da.join("z.txt").exists() && !db.join("z.txt").exists());
    let _ = std::fs::remove_dir_all(&da);
    let _ = std::fs::remove_dir_all(&db);
}

#[test]
fn explorer_browses_any_folder_by_dir_root_without_engine_ownership_rules() {
    let d = tmpdir("dirroot");
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src").join("main.rs"), "fn main() {}").unwrap();
    // A folder that is not an EAK project: "journal.jsonl" is an ordinary file here.
    std::fs::write(d.join("journal.jsonl"), "x").unwrap();
    let s = session();
    let root = format!("dir:{}", path(&d));
    let list = project_files(&s, &root, "").unwrap();
    assert!(list.iter().all(|e| !e.protected));
    project_file_delete(&s, &root, "journal.jsonl", false).unwrap();
    assert!(!d.join("journal.jsonl").exists());
    assert!(project_files(&s, &root, "../x").is_err());
    assert!(project_files(&s, "dir:relative/path", "").is_err());
    assert!(project_files(&s, &format!("dir:{}", path(&d.join("missing"))), "").is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn git_status_reports_changes_or_honestly_says_unavailable() {
    let d = tmpdir("git");
    std::fs::create_dir_all(&d).unwrap();
    let s = session();
    let root = format!("dir:{}", path(&d));
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&d)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
    };
    // No git (or not a repository): unavailable, never an error and never invented data.
    if git(&["init", "-q"]).is_none() {
        assert!(!project_git_status(&s, &root).unwrap().available);
        return;
    }
    let before = project_git_status(&s, &root).unwrap();
    assert!(before.available && before.entries.is_empty());
    std::fs::create_dir_all(d.join("sub")).unwrap();
    std::fs::write(d.join("sub").join("a.txt"), "a").unwrap();
    std::fs::write(d.join("b.txt"), "b").unwrap();
    let st = project_git_status(&s, &root).unwrap();
    let codes: Vec<(&str, &str)> = st
        .entries
        .iter()
        .map(|e| (e.rel.as_str(), e.code.as_str()))
        .collect();
    assert!(codes.contains(&("b.txt", "??")), "{codes:?}");
    assert!(
        codes
            .iter()
            .any(|(p, c)| p.starts_with("sub") && *c == "??"),
        "{codes:?}"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn operation_changes_returns_the_engines_own_sync_delta() {
    let d = tmpdir("delta");
    let s = session();
    let snap = create_project(&s, &path(&d), "Delta").unwrap();
    let id = snap.project.id.to_string();
    let mut rev = 0;
    let mut run = |c: C| {
        let r = execute_command(&s, rev, c).unwrap();
        assert!(r.accepted, "{:?}", r.error);
        rev = r.revision_after;
        r
    };
    let a = run(place(40.0)).created[0].clone();
    let b = run(place(80.0)).created[0].clone();
    run(C::ConnectPins {
        a_symbol: a,
        a_pin: "2".into(),
        b_symbol: b,
        b_pin: "1".into(),
    });
    let sync = run(C::SyncSchematicToPcb);

    let changes = operation_changes(&s, &id, &sync.op_id).unwrap();
    let added: Vec<_> = changes
        .iter()
        .filter(|c| c.path == ["pcb", "footprints"] && c.before.is_none() && c.after.is_some())
        .collect();
    assert_eq!(
        added.len(),
        2,
        "two footprints created by the sync: {changes:?}"
    );
    assert!(added
        .iter()
        .all(|c| c.after.as_ref().unwrap().get("refdes").is_some()));

    // The report must match what the snapshot shows.
    let now = current_project(&s).unwrap();
    assert_eq!(now.project.pcb.footprints.len(), 2);
    assert!(operation_changes(&s, &id, "op-does-not-exist").is_err());
    assert!(operation_changes(&s, "other-project", &sync.op_id).is_err());
    let _ = std::fs::remove_dir_all(&d);
}
