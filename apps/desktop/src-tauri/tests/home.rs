//! Project Home (slice S1): facts read from a project folder, the brief recorded through the
//! engine, the sample board and the folder suggestions. Headless, like `tests/ipc.rs`.

use eak_desktop::home::{
    create_project_with_brief, create_sample_project, default_projects_dir, project_facts,
    suggest_folder, MAX_BRIEF_LINES, PROJECTS_FOLDER,
};
use eak_desktop::*;
use eak_runtime::index::Index;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-home-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn session() -> Session {
    Session::new(Index::in_memory())
}

fn path(d: &std::path::Path) -> String {
    d.display().to_string()
}

#[test]
fn a_new_project_reports_every_check_as_not_run_and_the_release_as_draft() {
    let d = tmpdir("fresh");
    let s = session();
    create_project(&s, &path(&d), "Fresh").unwrap();
    let f = project_facts(&path(&d)).unwrap();
    assert_eq!(f.name, "Fresh");
    assert_eq!(f.revision, 0);
    assert_eq!(f.phase, "Empty project");
    // No evidence for this revision: nothing is reported, and nothing passes.
    assert!(f.checks.is_empty());
    assert_eq!(
        f.release.status,
        eak_manufacturing::PackageStatus::Draft,
        "a project with no reports is never a release candidate"
    );
    assert!(f.release.required > 0);
    assert!(f.release.passing < f.release.required);
    assert_eq!(f.design.parts, 0);
    assert!(!f.design.outline);
    assert_eq!(f.unrouted_nets, 0);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_brief_is_one_batch_of_requirements_through_the_engine() {
    let d = tmpdir("brief");
    let s = session();
    let brief = vec![
        "  5 V input from a 2-pin header  ".to_string(),
        String::new(),
        "3.3 V rail, at most 200 mA".to_string(),
    ];
    let snap = create_project_with_brief(&s, &path(&d), "Briefed", &brief).unwrap();
    // One revision for the whole brief (one undo step), blank lines dropped, text trimmed.
    assert_eq!(snap.project.revision, 1);
    assert!(snap.can_undo);
    let texts: Vec<&str> = snap
        .project
        .requirements
        .iter()
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(
        texts,
        [
            "5 V input from a 2-pin header",
            "3.3 V rail, at most 200 mA"
        ]
    );
    // It is on disk: the facts read it back from the folder.
    assert_eq!(project_facts(&path(&d)).unwrap().design.requirements, 2);
    // Undo removes the whole brief in one step.
    let u = undo(&s, 1).unwrap();
    assert!(u.accepted);
    assert!(current_project(&s).unwrap().project.requirements.is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_oversized_brief_is_refused_before_any_folder_is_created() {
    let d = tmpdir("toolong");
    let s = session();
    let brief: Vec<String> = (0..=MAX_BRIEF_LINES).map(|i| format!("line {i}")).collect();
    let err = create_project_with_brief(&s, &path(&d), "Too long", &brief).unwrap_err();
    assert!(err.contains("at most"), "{err}");
    assert!(!d.exists(), "nothing may be created for a refused brief");
    assert!(create_project_with_brief(&s, &path(&d), "  ", &[]).is_err());
}

#[test]
fn the_sample_board_is_built_by_commands_and_reported_from_its_folder() {
    let d = tmpdir("sample");
    let s = session();
    let snap = create_sample_project(&s, &path(&d)).unwrap();
    assert!(
        snap.project.revision > 10,
        "built step by step through the engine"
    );
    assert!(!snap.project.schematic.symbols.is_empty());
    let f = project_facts(&path(&d)).unwrap();
    assert_eq!(f.design.parts, snap.project.schematic.symbols.len());
    assert_eq!(f.design.missing_footprints, 0, "every part is on the board");
    assert!(f.design.outline);
    assert!(f.design.tracks > 0);
    assert_eq!(f.design.requirements, 2);
    // ERC run here saves evidence for this revision; Home then reports it.
    let erc = run_check(&s, Check::Erc).unwrap();
    let f = project_facts(&path(&d)).unwrap();
    let e = f
        .checks
        .iter()
        .find(|c| c.check == "ERC")
        .expect("ERC evidence");
    assert_eq!(e.errors, erc.report.errors);
    assert_eq!(e.warnings, erc.report.warnings);
    // The folder now holds a project: a second sample there is refused, never merged into it.
    assert!(create_sample_project(&s, &path(&d)).is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn facts_of_a_missing_folder_are_an_error_not_an_empty_project() {
    let d = tmpdir("missing");
    assert!(project_facts(&path(&d)).is_err());
    assert!(project_facts("relative/path").is_err());
}

#[test]
fn folder_suggestions_never_reuse_an_existing_folder() {
    let d = tmpdir("suggest");
    std::fs::create_dir_all(d.join("Board A")).unwrap();
    let first = suggest_folder(&path(&d), "Board A").unwrap();
    assert_eq!(first, path(&d.join("Board A 2")));
    // Characters no desktop system allows in a folder name are replaced.
    let odd = suggest_folder(&path(&d), "a/b:c?.").unwrap();
    assert_eq!(odd, path(&d.join("a-b-c-")));
    assert_eq!(
        suggest_folder(&path(&d), "   ").unwrap(),
        path(&d.join("Untitled board"))
    );
    assert!(suggest_folder("relative", "x").is_err());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_default_projects_folder_prefers_documents_then_home() {
    let docs = PathBuf::from("/u/Documents");
    let home = PathBuf::from("/u");
    assert_eq!(
        default_projects_dir(Some(docs), Some(home.clone())),
        Some(path(&PathBuf::from("/u/Documents").join(PROJECTS_FOLDER)))
    );
    assert_eq!(
        default_projects_dir(None, Some(home)),
        Some(path(&PathBuf::from("/u").join(PROJECTS_FOLDER)))
    );
    assert_eq!(default_projects_dir(None, None), None);
}
