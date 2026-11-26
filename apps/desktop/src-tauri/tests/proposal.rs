//! Proposal review IPC (slice S9, ADR-0051): an agent session in propose mode, reaching the app
//! through its own harness endpoint handler, stores edits as a proposal without moving the
//! project; the person's review, export, apply and discard commands then act on it.

use eak_desktop::proposal::{
    proposal_apply, proposal_discard, proposal_export, proposal_list, proposal_review,
};
use eak_desktop::*;
use eak_harness::endpoint::Request;
use eak_harness::proposal::ApplyMode;
use eak_harness::CallContext;
use eak_runtime::index::Index;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn request(dir: &Path, id: &str, tool: &str, args: Value) -> Request {
    let mut context = CallContext::from_env();
    context.project.dir = Some(dir.to_path_buf());
    context.project.id = Some(id.into());
    context.session_id = "sess-prop".into();
    context.agent = "ipc-agent".into();
    context.apply_mode = ApplyMode::Propose;
    Request {
        token: String::new(),
        tool: tool.into(),
        args,
        context,
    }
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("eak-prop-ipc-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Two independent parts proposed by the agent: the project stays at revision 0.
fn proposed(s: &Session, dir: &Path, id: &str) -> String {
    let call = |tool: &str, args: Value| harness_call(s, &request(dir, id, tool, args), &|_| {});
    let a = call(
        "PlaceSymbol",
        json!({"expected_revision": 0, "component_id": "C_0805", "position": {"x": 0, "y": 0}}),
    );
    assert_eq!(a["ok"], json!(true), "{a}");
    assert_eq!(a["result"]["proposed"], json!(true), "{a}");
    let b = call(
        "PlaceSymbol",
        json!({"expected_revision": 0, "component_id": "R_0805", "position": {"x": 0, "y": 20000000}}),
    );
    assert_eq!(b["ok"], json!(true), "{b}");
    assert_eq!(current_project(s).unwrap().project.revision, 0);
    a["result"]["proposal_id"].as_str().unwrap().to_string()
}

#[test]
fn the_person_reviews_exports_and_applies_a_proposal_as_one_undo_step() {
    let dir = temp("apply");
    let s = Session::new(Index::in_memory());
    let snap = create_project(&s, &dir.display().to_string(), "Proposal IPC").expect("create");
    let pid = proposed(&s, &dir, &snap.project.id);

    let list = proposal_list(&s).unwrap();
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(list[0]["id"], pid.as_str());
    assert_eq!(list[0]["steps"], 2);
    assert_eq!(list[0]["stale"], false);

    // The review is EAK's: both steps, their total delta, ERC and DRC before/after on a copy.
    let all = proposal_review(&s, &pid, None).unwrap();
    assert_eq!(all.selected, vec![0, 1]);
    assert_eq!(all.steps.len(), 2);
    assert!(!all.changes.is_empty());
    let checks: Vec<&str> = all.checks.iter().map(|c| c.check.as_str()).collect();
    assert!(
        checks.contains(&"ERC") && checks.contains(&"DRC"),
        "{checks:?}"
    );
    assert!(all.problem.is_none(), "{:?}", all.problem);
    // The steps are independent, so one alone is a valid subset with a smaller delta.
    let one = proposal_review(&s, &pid, Some(vec![1])).unwrap();
    assert_eq!(one.selected, vec![1]);
    assert!(one.problem.is_none(), "{:?}", one.problem);
    assert!(one.changes.len() < all.changes.len());
    let bad = proposal_review(&s, &pid, Some(vec![7])).unwrap();
    assert!(bad.problem.is_some());

    // Export saves the engine's review as evidence; nothing is applied.
    let path = proposal_export(&s, &pid, Some(vec![1])).unwrap();
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["kind"], "proposal_review");
    assert_eq!(saved["review"]["selected"], json!([1]));
    assert_eq!(current_project(&s).unwrap().project.revision, 0);

    // A wrong expected revision is refused and writes nothing.
    let stale = proposal_apply(&s, &pid, 3, None).unwrap_err();
    assert!(stale.contains("stale"), "{stale}");
    assert_eq!(current_project(&s).unwrap().project.revision, 0);

    // Apply the one selected step as the person: one revision, a checkpoint, one undo step.
    let out = proposal_apply(&s, &pid, 0, Some(vec![1])).unwrap();
    assert!(out.response.accepted, "{:?}", out.response);
    assert_eq!(out.response.revision_after, 1);
    assert!(
        out.checkpoint.contains(&format!("before-{pid}")),
        "{}",
        out.checkpoint
    );
    assert_eq!(
        current_project(&s).unwrap().project.schematic.symbols.len(),
        1
    );
    assert!(proposal_apply(&s, &pid, 1, None).is_err(), "applied twice");
    assert_eq!(proposal_list(&s).unwrap()[0]["status"], "applied");
    undo(&s, 1).unwrap();
    assert!(current_project(&s)
        .unwrap()
        .project
        .schematic
        .symbols
        .is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_discarded_proposal_cannot_be_applied_and_a_stale_one_is_reported() {
    let dir = temp("discard");
    let s = Session::new(Index::in_memory());
    let snap = create_project(&s, &dir.display().to_string(), "Proposal discard").expect("create");
    let pid = proposed(&s, &dir, &snap.project.id);

    // The person edits the project: the proposal is now stale and the review says why.
    let r = execute_command(
        &s,
        0,
        serde_json::from_value(json!({"type": "AddSheet", "name": "Power"})).unwrap(),
    )
    .unwrap();
    assert!(r.accepted, "{r:?}");
    let review = proposal_review(&s, &pid, None).unwrap();
    assert!(review.stale);
    assert!(review.problem.is_some());
    assert_eq!(proposal_list(&s).unwrap()[0]["stale"], true);
    assert!(proposal_apply(&s, &pid, 1, None).is_err());

    let gone = proposal_discard(&s, &pid).unwrap();
    assert_eq!(gone["status"], "discarded");
    assert!(proposal_apply(&s, &pid, 1, None).is_err());
    assert_eq!(current_project(&s).unwrap().project.revision, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
