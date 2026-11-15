//! Live synchronisation contract: a harness call that commits through the app's engine emits a
//! typed revision-change event naming the revision range, the caller's session and the IR areas
//! that changed; reads and rejected commands emit nothing.

use eak_desktop::sync::RevisionChange;
use eak_desktop::*;
use eak_harness::endpoint::Request;
use eak_harness::CallContext;
use eak_runtime::index::Index;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::path::PathBuf;

fn request(dir: &std::path::Path, id: &str, tool: &str, args: Value) -> Request {
    let mut context = CallContext::from_env();
    context.project.dir = Some(dir.to_path_buf());
    context.project.id = Some(id.into());
    context.session_id = "sess-live".into();
    Request {
        token: String::new(),
        tool: tool.into(),
        args,
        context,
    }
}

#[test]
fn harness_commits_emit_one_typed_event_with_changed_areas() {
    let dir: PathBuf = std::env::temp_dir().join(format!("eak-live-sync-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let s = Session::new(Index::in_memory());
    let snap = create_project(&s, &dir.display().to_string(), "Live").expect("create");
    let id = snap.project.id.clone();

    let events: RefCell<Vec<RevisionChange>> = RefCell::new(vec![]);
    let call = |tool: &str, args: Value| {
        harness_call(&s, &request(&dir, &id, tool, args), &|c| {
            events.borrow_mut().push(c)
        })
    };

    // A read never moves the revision, so it emits nothing.
    let info = call("project_info", json!({}));
    assert_eq!(info["ok"], json!(true), "{info}");
    assert!(events.borrow().is_empty());

    // A committed command emits exactly one event for the revision it produced.
    let out = call("AddSheet", json!({"expected_revision": 0, "name": "Power"}));
    assert_eq!(out["ok"], json!(true), "{out}");
    {
        let ev = events.borrow();
        assert_eq!(ev.len(), 1, "{ev:?}");
        assert_eq!(ev[0].project_id, id);
        assert_eq!(ev[0].revision_before, 0);
        assert_eq!(ev[0].revision_after, 1);
        assert_eq!(ev[0].source, "harness");
        assert_eq!(ev[0].session_id.as_deref(), Some("sess-live"));
        assert_eq!(ev[0].areas, vec!["schematic".to_string()]);
    }

    // A stale expected revision is rejected by the engine and emits nothing.
    let stale = call("AddSheet", json!({"expected_revision": 0, "name": "Other"}));
    assert_eq!(stale["ok"], json!(false), "{stale}");
    assert_eq!(events.borrow().len(), 1);

    // The UI's own view of the project is the same engine: the snapshot is at the event's revision.
    assert_eq!(current_project(&s).unwrap().project.revision, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
