//! `Engine::write_project_record` contract (ADR-0039): the only writer of the non-design
//! record areas in a project folder. Journalled, never touches the design revision, refuses
//! agents, path escapes and unknown areas.

use eak_runtime::engine::*;
use eak_runtime::CommandError;
use std::path::PathBuf;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-records-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn a_record_is_written_journalled_and_leaves_the_design_revision_alone() {
    let d = tmpdir("ok");
    let mut e = Engine::create(&d, "Records").unwrap();
    let rev = e.project().revision;
    let path = e
        .write_project_record(
            &Actor::human(),
            "sourcing",
            "assembly/C25804.json",
            b"{}",
            "user attested basic",
        )
        .unwrap();
    assert_eq!(
        path,
        d.join("sourcing").join("assembly").join("C25804.json")
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"{}");
    assert_eq!(e.project().revision, rev);
    assert!(!e.can_undo(), "a record write is not a design undo step");

    let journal = read_record_journal(&d, "sourcing");
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0].file, "assembly/C25804.json");
    assert_eq!(journal[0].actor, "human:user");
    assert_eq!(journal[0].revision, rev);
    assert_eq!(journal[0].bytes, 2);
    assert!(e
        .events_since(0)
        .iter()
        .any(|ev| ev.kind == "project_record_written" && ev.op_id == journal[0].op_id));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn escapes_unknown_areas_agents_and_folderless_projects_are_refused() {
    let d = tmpdir("refused");
    let mut e = Engine::create(&d, "Records").unwrap();
    for bad in [
        "../project.eak.json",
        "/etc/x.json",
        "a/../../x.json",
        ".hidden.json",
        "C1.txt",
        "",
        "a//b.json",
        "a\\b.json",
    ] {
        assert!(
            matches!(
                e.write_project_record(&Actor::human(), "sourcing", bad, b"{}", "x"),
                Err(CommandError::Invalid(_))
            ),
            "{bad:?}"
        );
    }
    assert!(matches!(
        e.write_project_record(&Actor::human(), "evidence", "x.json", b"{}", "x"),
        Err(CommandError::Invalid(_))
    ));
    assert!(matches!(
        e.write_project_record(&Actor::agent("ai"), "sourcing", "C1.json", b"{}", "x"),
        Err(CommandError::ApprovalRequired(_))
    ));
    assert!(read_record_journal(&d, "sourcing").is_empty());
    assert!(!d.join("sourcing").exists());

    let mut mem = Engine::in_memory(e.project().clone());
    assert!(matches!(
        mem.write_project_record(&Actor::human(), "sourcing", "C1.json", b"{}", "x"),
        Err(CommandError::Precondition(_))
    ));
    let _ = std::fs::remove_dir_all(&d);
}
