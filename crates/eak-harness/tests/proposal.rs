//! End-to-end propose mode (ADR-0051): an agent session started with `--propose` drives the real
//! `eak` binary (separate processes, real files), the project never moves, and a person applies
//! the proposal through `eak_harness::proposal::apply` as one checkpointed undo step.

use eak_harness::proposal::{self, Status};
use eak_runtime::{Actor, CommandError, Engine};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "eak-proposal-{tag}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// One `eak` call as the agent session; `propose` adds `--propose`.
fn eak(project: &Path, propose: bool, args: &[&str]) -> (i32, Value) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_eak"));
    c.args(["--json", "--project"]).arg(project).args([
        "--session",
        "prop-session",
        "--agent",
        "e2e-agent",
    ]);
    if propose {
        c.arg("--propose");
    }
    let out = c
        .args(args)
        .env_remove("EAK_PROJECT_ID")
        .env_remove("EAK_PROJECT_DIR")
        .env_remove("EAK_HARNESS_APPLY_MODE")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let v: Value = serde_json::from_str(text.trim()).unwrap_or_else(|e| {
        panic!(
            "not JSON ({e}): {text} / {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (out.status.code().unwrap_or(-1), v)
}

fn propose(project: &Path, rev: u64, cmd: Value) -> Value {
    let (code, v) = eak(project, true, &["exec", &rev.to_string(), &cmd.to_string()]);
    assert_eq!(code, 0, "{cmd} was not proposed: {v}");
    assert_eq!(v["result"]["proposed"], true, "{v}");
    assert_eq!(v["result"]["revision_after"], rev, "{v}");
    assert_eq!(
        v["project"]["revision"], rev,
        "a proposed edit moved the revision: {v}"
    );
    v
}

fn rev(v: &Value) -> u64 {
    v["project"]["revision"].as_u64().unwrap()
}

fn symbol_count(project: &Path, propose: bool) -> (usize, Value) {
    let (_, v) = eak(project, propose, &["call", "list_symbols"]);
    (v["result"]["symbols"].as_array().unwrap().len(), v)
}

#[test]
fn a_propose_session_never_moves_the_project_and_a_person_applies_it_as_one_step() {
    let root = temp_dir("apply");
    let dir = root.join("board.eak");
    let (code, v) = eak(&dir, false, &["new", dir.to_str().unwrap(), "Proposal E2E"]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(rev(&v), 0);

    // Four design calls: two parts, then two labels naming the parts the first calls created.
    let v = propose(
        &dir,
        0,
        json!({"type": "PlaceSymbol", "component_id": "CONN_1x02_P2.54", "position": {"x": 20000000, "y": 40000000}}),
    );
    let pid = v["result"]["proposal_id"].as_str().unwrap().to_string();
    let j1 = v["result"]["created"][0].as_str().unwrap().to_string();
    let v = propose(
        &dir,
        0,
        json!({"type": "PlaceSymbol", "component_id": "C_0805", "position": {"x": 60000000, "y": 40000000}, "value": "100n"}),
    );
    assert_eq!(
        v["result"]["proposal_id"],
        pid.as_str(),
        "one open proposal per session"
    );
    let c1 = v["result"]["created"][0].as_str().unwrap().to_string();
    propose(
        &dir,
        0,
        json!({"type": "LabelPin", "symbol_id": j1, "pin": "1", "name": "VIN"}),
    );
    let v = propose(
        &dir,
        0,
        json!({"type": "LabelPin", "symbol_id": c1, "pin": "1", "name": "VIN"}),
    );
    assert_eq!(v["result"]["steps"], 4);

    // The session reads its own pending edits on a copy; the project itself is untouched.
    let (n, v) = symbol_count(&dir, true);
    assert_eq!(n, 2, "{v}");
    assert_eq!(v["result"]["proposal_view"]["proposal_id"], pid.as_str());
    let (n, _) = symbol_count(&dir, false);
    assert_eq!(n, 0, "the project changed before Apply");
    let (_, info) = eak(&dir, true, &["info"]);
    assert_eq!(rev(&info), 0);
    assert_eq!(info["result"]["can_undo"], false);
    let (_, h) = eak(&dir, false, &["history"]);
    assert!(h["result"]["journal"].as_array().unwrap().is_empty(), "{h}");
    // Checks on the copy store no evidence (the copy is not a revision).
    let (_, erc) = eak(&dir, true, &["check", "erc"]);
    assert!(erc["result"]["proposal_view"].is_object(), "{erc}");
    assert!(
        erc["result"]["reports"][0]["evidence_path"].is_null(),
        "{erc}"
    );
    assert!(!dir.join("evidence").exists());
    // What would change the project is refused in propose mode.
    let (_, v) = eak(&dir, true, &["exec", "0", r#"{"type":"Undo"}"#]);
    assert_eq!(v["error"]["code"], "precondition", "{v}");
    let (_, v) = eak(
        &dir,
        true,
        &[
            "call",
            "rework_apply",
            r#"{"plan_id":"rwp-0001","expected_revision":0}"#,
        ],
    );
    assert_eq!(v["error"]["code"], "precondition", "{v}");
    // A wrong expected revision is refused, as in standing mode.
    let (code, v) = eak(&dir, true, &["exec", "5", r#"{"type":"Annotate"}"#]);
    assert_eq!(code, 4, "{v}");

    // Review: per-step delta, dependencies, ERC/DRC before/after and labels.
    let (_, v) = eak(
        &dir,
        true,
        &[
            "call",
            "proposal_begin",
            r#"{"title":"Add input connector and decoupling","rationale":"the spec asks for a VIN connector"}"#,
        ],
    );
    assert_eq!(v["result"]["proposal"]["id"], pid.as_str(), "{v}");
    let (_, v) = eak(&dir, true, &["call", "proposal_status", "{}"]);
    let r = &v["result"];
    assert_eq!(r["stale"], false, "{v}");
    assert!(r["problem"].is_null(), "{v}");
    assert_eq!(r["steps"].as_array().unwrap().len(), 4);
    assert_eq!(r["steps"][2]["requires"], json!([0]), "{v}");
    assert_eq!(r["steps"][3]["requires"], json!([1]), "{v}");
    assert!(!r["changes"].as_array().unwrap().is_empty());
    let checks: Vec<&str> = r["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["check"].as_str().unwrap())
        .collect();
    assert_eq!(checks, ["ERC", "DRC"], "{v}");
    for c in r["checks"].as_array().unwrap() {
        assert!(c["before"]["errors"].is_u64() && c["after"]["errors"].is_u64());
        assert!(c["label"].is_string() && c["not_implemented"].is_array());
    }
    let labels: Vec<&str> = r["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"assumed"), "the rationale is assumed: {v}");
    // A selection that drops a step another one needs is refused with the reason.
    let (_, v) = eak(&dir, true, &["call", "proposal_status", r#"{"steps":[2]}"#]);
    assert!(
        v["result"]["problem"]
            .as_str()
            .unwrap()
            .contains("select step 1"),
        "{v}"
    );
    let (_, v) = eak(&dir, true, &["call", "proposal_submit", "{}"]);
    assert_eq!(v["result"]["proposal"]["status"], "ready", "{v}");
    let (_, v) = eak(&dir, false, &["proposals"]);
    assert_eq!(v["result"]["proposals"][0]["steps"], 4, "{v}");

    // Apply is a person's: an agent is refused, a stale revision is refused, nothing changes.
    let mut engine = Engine::open(&dir, false).unwrap();
    let agent = Actor::agent_in_session("e2e-agent", "prop-session");
    assert!(matches!(
        proposal::apply(&mut engine, &pid, 0, None, &agent),
        Err(CommandError::ApprovalRequired(_))
    ));
    assert!(matches!(
        proposal::apply(&mut engine, &pid, 3, None, &Actor::human()),
        Err(CommandError::StaleRevision { .. })
    ));
    assert_eq!(engine.project().revision, 0);
    assert!(engine.list_checkpoints().is_empty());

    let out = proposal::apply(&mut engine, &pid, 0, None, &Actor::human()).unwrap();
    assert!(out.response.accepted, "{:?}", out.response);
    assert_eq!(out.response.revision_before, 0);
    assert_eq!(
        engine.project().revision,
        1,
        "one revision for the whole proposal"
    );
    assert_eq!(engine.list_checkpoints(), vec![out.checkpoint.clone()]);
    assert!(out.checkpoint.starts_with("r000000-before-prop-"));
    // The objects carry the ids the agent was shown, so its later steps hit them.
    let p = engine.project();
    assert!(p.schematic.symbols.contains_key(&j1) && p.schematic.symbols.contains_key(&c1));
    assert!(!p.schematic.labels.is_empty());
    assert_eq!(out.proposal.status, Status::Applied);
    let stored = proposal::load(&dir, &pid).unwrap();
    assert_eq!(stored.applied.as_ref().unwrap().revision_after, 1);
    assert!(matches!(
        proposal::apply(&mut engine, &pid, 1, None, &Actor::human()),
        Err(CommandError::Precondition(_))
    ));
    // Exactly one undo step takes the whole proposal back.
    let undo = engine.undo(1, &Actor::human());
    assert!(undo.accepted, "{undo:?}");
    assert!(engine.project().schematic.symbols.is_empty());
    assert!(engine.project().schematic.labels.is_empty());
    assert!(!engine.can_undo());
    drop(engine);

    // The journal names the agent as actor of the applied batch.
    let (_, h) = eak(&dir, false, &["history"]);
    let first = &h["result"]["journal"][0];
    assert_eq!(first["actor"], "agent:e2e-agent@prop-session", "{h}");
    assert_eq!(first["command"], "Batch", "{h}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_proposal_goes_stale_when_the_project_moves_and_cannot_be_applied() {
    let root = temp_dir("stale");
    let dir = root.join("board.eak");
    let (code, v) = eak(
        &dir,
        false,
        &["new", dir.to_str().unwrap(), "Stale proposal"],
    );
    assert_eq!(code, 0, "{v}");
    let v = propose(
        &dir,
        0,
        json!({"type": "PlaceSymbol", "component_id": "C_0805", "position": {"x": 0, "y": 0}}),
    );
    let pid = v["result"]["proposal_id"].as_str().unwrap().to_string();
    // Someone edits the project in standing mode.
    let place_r = json!({"type": "PlaceSymbol", "component_id": "R_0805", "position": {"x": 0, "y": 20000000}});
    let (code, v) = eak(&dir, false, &["exec", "0", &place_r.to_string()]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(rev(&v), 1);

    let (_, v) = eak(&dir, true, &["call", "proposal_status", "{}"]);
    assert_eq!(v["result"]["stale"], true, "{v}");
    assert!(
        v["result"]["problem"]
            .as_str()
            .unwrap()
            .contains("revision 0"),
        "{v}"
    );
    // A new step on a stale proposal is refused; reads fall back to the project.
    let place_c =
        json!({"type": "PlaceSymbol", "component_id": "C_0805", "position": {"x": 0, "y": 0}});
    let (code, v) = eak(&dir, true, &["exec", "1", &place_c.to_string()]);
    assert_eq!(code, 4, "{v}");
    assert_eq!(v["error"]["details"]["proposal_id"], pid.as_str(), "{v}");
    let (n, v) = symbol_count(&dir, true);
    assert_eq!(n, 1, "{v}");
    assert!(v["result"]["proposal_view"].is_null());

    let mut engine = Engine::open(&dir, false).unwrap();
    match proposal::apply(&mut engine, &pid, 1, None, &Actor::human()) {
        Err(CommandError::StaleRevision { expected, actual }) => {
            assert_eq!((expected, actual), (0, 1))
        }
        other => panic!("stale apply was not refused: {other:?}"),
    }
    assert_eq!(engine.project().revision, 1);
    assert!(
        engine.list_checkpoints().is_empty(),
        "no checkpoint for a refused apply"
    );
    let p = proposal::discard(&engine, &pid, &Actor::human(), None).unwrap();
    assert_eq!(p.status, Status::Discarded);
    drop(engine);
    // With the old one discarded the session starts a new proposal at the current revision.
    let v = propose(&dir, 1, place_c);
    assert_ne!(v["result"]["proposal_id"], pid.as_str());
    let _ = std::fs::remove_dir_all(root);
}
