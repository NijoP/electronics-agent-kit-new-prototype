//! WEFT end to end (ADR-0053): two scripted MCP clients, each a real `eak-mcp` process with its
//! own agent identity in propose mode, coordinate on one project only through the WEFT tools:
//! register, delegate with a dependency, refuse out-of-order and non-owner status changes,
//! propose, request a review, give a verdict, and find a conflict. The engineer applies one
//! proposal (the library call the app makes); the conflicting one is then stale and refused, so
//! nothing is overwritten silently. Every step is read back from the journal.

use eak_harness::proposal;
use eak_runtime::{Actor, Engine};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-weft-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// One MCP client: an `eak-mcp` process speaking JSON-RPC over stdio.
struct Client {
    child: Child,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
    id: u64,
}

impl Client {
    fn start(dir: &Path, agent: &str, session: &str) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_eak-mcp"))
            .env("EAK_PROJECT_DIR", dir)
            .env("EAK_SESSION_ID", session)
            .env("EAK_AGENT_NAME", agent)
            .env("EAK_HARNESS_APPLY_MODE", "propose")
            .env_remove("EAK_PROJECT_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let out = BufReader::new(child.stdout.take().unwrap());
        let mut c = Client {
            child,
            stdin,
            out,
            id: 0,
        };
        let init = c.rpc("initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": agent, "version": "0"}}));
        assert_eq!(init["result"]["serverInfo"]["name"], "eak-harness");
        c
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let msg = json!({"jsonrpc": "2.0", "id": self.id, "method": method, "params": params});
        writeln!(self.stdin, "{msg}").unwrap();
        let mut line = String::new();
        self.out.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    /// A tool call: the harness envelope.
    fn call(&mut self, tool: &str, args: Value) -> Value {
        let r = self.rpc("tools/call", json!({"name": tool, "arguments": args}));
        r["result"]["structuredContent"].clone()
    }

    fn ok(&mut self, tool: &str, args: Value) -> Value {
        let env = self.call(tool, args);
        assert_eq!(env["ok"], true, "{tool}: {env}");
        env["result"].clone()
    }

    fn refused(&mut self, tool: &str, args: Value, code: &str) -> Value {
        let env = self.call(tool, args);
        assert_eq!(env["ok"], false, "{tool} should be refused: {env}");
        assert_eq!(env["error"]["code"], code, "{tool}: {env}");
        env
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const PLANNER: &str = "agent:planner@s-plan";
const LAYOUT: &str = "agent:layout@s-layout";

#[test]
fn two_mcp_agents_delegate_review_and_meet_a_conflict_over_the_real_bus() {
    let root = temp_dir("bus");
    let dir = root.join("w.eak");
    let mut planner = Client::start(&dir, "planner", "s-plan");
    planner.ok("create_project", json!({"dir": dir, "name": "WEFT"}));
    let mut layout = Client::start(&dir, "layout", "s-layout");

    // Nothing is accepted from an unregistered session.
    planner.refused(
        "weft_send",
        json!({"kind": "finding", "body": "x"}),
        "precondition",
    );
    let me = planner.ok("weft_register", json!({"tool": "claude-code", "model": "test-model", "capabilities": ["planning", "review"]}));
    assert_eq!(me["agent"]["id"], PLANNER);
    layout.ok(
        "weft_register",
        json!({"tool": "codex", "capabilities": ["schematic"]}),
    );
    let agents = layout.ok("weft_agents", json!({}));
    assert_eq!(agents["agents"].as_array().unwrap().len(), 2, "{agents}");

    // Delegation with a dependency: the label task waits for the part task.
    let d1 = planner.ok(
        "weft_send",
        json!({"kind": "delegate", "to": LAYOUT, "title": "Place the input capacitor"}),
    );
    let t1 = d1["task"]["task"]["id"].as_str().unwrap().to_string();
    assert_eq!(d1["task"]["ready"], true);
    let d2 = planner.ok(
        "weft_send",
        json!({"kind": "delegate", "to": LAYOUT, "title": "Tighten clearance", "depends_on": [t1]}),
    );
    let t2 = d2["task"]["task"]["id"].as_str().unwrap().to_string();
    assert_eq!(d2["task"]["ready"], false);
    assert_eq!(d2["task"]["waiting_on"], json!([t1]));
    planner.refused(
        "weft_send",
        json!({"kind": "delegate", "to": "agent:nobody@x", "title": "t"}),
        "not_found",
    );

    // The delegate reads both tasks in its inbox.
    let inbox = layout.ok("weft_inbox", json!({}));
    let kinds: Vec<&str> = inbox["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["message"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["delegate", "delegate"], "{inbox}");
    let since = inbox["next_since"].as_u64().unwrap();

    // Execution follows the dependencies, and only the owner moves its task.
    layout.refused(
        "weft_send",
        json!({"kind": "status", "task_id": t2, "status": "in_progress"}),
        "precondition",
    );
    planner.refused(
        "weft_send",
        json!({"kind": "status", "task_id": t1, "status": "done"}),
        "approval_required",
    );
    layout.ok("weft_send", json!({"kind": "status", "task_id": t1, "status": "in_progress", "to": PLANNER, "body": "placing C1"}));

    // The delegate's edits are a proposal: the project does not move.
    let placed = layout.ok(
        "PlaceSymbol",
        json!({"expected_revision": 0, "component_id": "C_0805", "position": {"x": 0, "y": 0}}),
    );
    assert_eq!(placed["proposed"], true, "{placed}");
    let p1 = placed["proposal_id"].as_str().unwrap().to_string();
    layout.ok(
        "SetDesignRules",
        json!({"expected_revision": 0, "min_clearance": 200000}),
    );
    layout.ok("proposal_submit", json!({"proposal_id": p1}));
    layout.refused(
        "weft_send",
        json!({"kind": "review_request", "to": LAYOUT, "proposal_id": p1}),
        "precondition",
    );
    layout.ok("weft_send", json!({"kind": "review_request", "to": PLANNER, "proposal_id": p1, "body": "C1 and 0.2 mm clearance"}));
    layout.ok(
        "weft_send",
        json!({"kind": "review_request", "to": "engineer", "proposal_id": p1}),
    );
    layout.ok(
        "weft_send",
        json!({"kind": "status", "task_id": t1, "status": "done", "to": PLANNER}),
    );

    // The reviewer is asked, answers; the author cannot review itself.
    let asked = planner.ok("weft_inbox", json!({}));
    let req = asked["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["message"]["kind"] == "review_request")
        .expect("review request delivered");
    assert_eq!(req["message"]["proposal_id"], p1.as_str());
    let verdict = planner.ok("weft_send", json!({"kind": "review_verdict", "proposal_id": p1, "verdict": "approve", "body": "ERC clean on the copy"}));
    assert_eq!(verdict["sent"]["to"], LAYOUT);
    assert!(verdict["note"]
        .as_str()
        .unwrap()
        .contains("only the engineer applies"));
    layout.refused(
        "weft_send",
        json!({"kind": "review_verdict", "proposal_id": p1, "verdict": "approve"}),
        "precondition",
    );
    let news = layout.ok("weft_inbox", json!({"since": since}));
    assert!(
        news["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["message"]["kind"] == "review_verdict"
                && m["message"]["verdict"] == "approve"),
        "{news}"
    );

    // The dependency is met now.
    let tasks = layout.ok("weft_tasks", json!({"mine": true}));
    let t2v = tasks["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["task"]["id"] == t2.as_str())
        .unwrap()
        .clone();
    assert_eq!(t2v["ready"], true, "{tasks}");
    layout.ok(
        "weft_send",
        json!({"kind": "status", "task_id": t2, "status": "in_progress"}),
    );

    // Conflict: the planner proposes another clearance on the same revision.
    let other = planner.ok(
        "SetDesignRules",
        json!({"expected_revision": 0, "min_clearance": 250000}),
    );
    let p2 = other["proposal_id"].as_str().unwrap().to_string();
    let c = planner.ok("weft_conflicts", json!({}));
    let overlap = c["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["kind"] == "overlap")
        .expect("overlap flagged")
        .clone();
    let pair: Vec<&str> = overlap["proposals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        pair.contains(&p1.as_str()) && pair.contains(&p2.as_str()),
        "{c}"
    );
    assert!(
        overlap["objects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o.as_str().unwrap().starts_with("constraints")),
        "{c}"
    );

    // The engineer applies the reviewed proposal; the other one is then stale and refused.
    {
        let mut engine = Engine::open(&dir, false).unwrap();
        let out = proposal::apply(&mut engine, &p1, 0, None, &Actor::human()).unwrap();
        assert!(out.response.accepted);
        assert_eq!(engine.project().revision, 1);
        let err = proposal::apply(&mut engine, &p2, 1, None, &Actor::human()).unwrap_err();
        assert!(err.to_string().contains("stale"), "{err}");
        assert_eq!(engine.project().revision, 1, "nothing overwritten");
    }
    let c = layout.ok("weft_conflicts", json!({}));
    let kinds: Vec<&str> = c["conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["stale"], "{c}");
    assert_eq!(c["conflicts"][0]["proposals"], json!([p2]));

    // The history is the journal: every event with its agent and the revision it was sent at.
    let h = planner.ok("weft_history", json!({}));
    let entries = h["entries"].as_array().unwrap();
    assert_eq!(h["unreadable"], 0);
    let count = |event: &str| entries.iter().filter(|e| e["event"] == event).count();
    assert_eq!(count("agent_registered"), 2);
    assert_eq!(count("task_created"), 2);
    assert!(entries
        .windows(2)
        .all(|w| w[0]["seq"].as_u64() < w[1]["seq"].as_u64()));
    assert!(
        entries.iter().any(|e| e["by"] == PLANNER) && entries.iter().any(|e| e["by"] == LAYOUT)
    );
    let last = entries.last().unwrap();
    assert_eq!(last["event"], "message");
    assert_eq!(last["message"]["kind"], "status");
    assert_eq!(last["revision"], 0, "sent before the engineer applied");
    drop(planner);
    drop(layout);
    let _ = std::fs::remove_dir_all(&root);
}
