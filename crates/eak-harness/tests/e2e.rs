//! End-to-end: drive a fresh project through the real `eak` binary (separate processes, real
//! files), then the MCP stdio server, then routing through an app-style endpoint.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "eak-harness-{tag}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn eak(project: &Path, args: &[&str]) -> (i32, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_eak"))
        .args(["--json", "--project"])
        .arg(project)
        .args(["--session", "test-session", "--agent", "e2e-agent"])
        .args(args)
        .env_remove("EAK_PROJECT_ID")
        .env_remove("EAK_PROJECT_DIR")
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

fn exec(project: &Path, rev: u64, cmd: Value) -> Value {
    let (code, v) = eak(project, &["exec", &rev.to_string(), &cmd.to_string()]);
    assert_eq!(code, 0, "{cmd} failed: {v}");
    assert_eq!(v["ok"], true);
    assert_eq!(v["category"], "design");
    assert_eq!(v["result"]["revision_before"], rev);
    v
}

fn rev(v: &Value) -> u64 {
    v["project"]["revision"].as_u64().unwrap()
}

#[test]
fn cli_drives_a_fresh_project_from_create_to_checks() {
    let root = temp_dir("cli");
    let dir = root.join("board.eak");
    let (code, v) = eak(&dir, &["new", dir.to_str().unwrap(), "Harness E2E"]);
    assert_eq!(code, 0, "{v}");
    let pid = v["project"]["id"].as_str().unwrap().to_string();
    assert_eq!(rev(&v), 0);

    // Discovery and context echo.
    let (_, tools) = eak(&dir, &["tools"]);
    let cats = &tools["result"]["by_category"];
    for c in [
        "read",
        "design",
        "simulation",
        "validation",
        "manufacturing",
    ] {
        assert!(
            cats[c].as_array().is_some_and(|a| !a.is_empty()),
            "no {c} tools"
        );
    }
    let (_, info) = eak(&dir, &["info"]);
    assert_eq!(info["project"]["id"], pid.as_str());
    assert_eq!(info["route"], "direct");

    // Place two parts and connect them.
    let r = exec(
        &dir,
        0,
        json!({"type": "PlaceSymbol", "component_id": "CONN_1x02_P2.54", "position": {"x": 20000000, "y": 40000000}}),
    );
    let j1 = r["result"]["created"][0].as_str().unwrap().to_string();
    let r = exec(
        &dir,
        rev(&r),
        json!({"type": "PlaceSymbol", "component_id": "C_0805", "position": {"x": 60000000, "y": 40000000}, "value": "100n"}),
    );
    let c1 = r["result"]["created"][0].as_str().unwrap().to_string();
    let r = exec(
        &dir,
        rev(&r),
        json!({"type": "LabelPin", "symbol_id": j1, "pin": "1", "name": "VIN"}),
    );
    let r = exec(
        &dir,
        rev(&r),
        json!({"type": "LabelPin", "symbol_id": c1, "pin": "1", "name": "VIN"}),
    );
    let r = exec(
        &dir,
        rev(&r),
        json!({"type": "LabelPin", "symbol_id": j1, "pin": "2", "name": "GND", "kind": "power"}),
    );
    let r = exec(
        &dir,
        rev(&r),
        json!({"type": "LabelPin", "symbol_id": c1, "pin": "2", "name": "GND", "kind": "power"}),
    );

    // Read back the schematic: pins carry nets.
    let (_, syms) = eak(&dir, &["call", "list_symbols"]);
    let nets: Vec<&str> = syms["result"]["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|s| {
            s["pins"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|p| p["net"].as_str())
        })
        .collect();
    assert!(nets.contains(&"VIN") && nets.contains(&"GND"), "{syms}");

    // Stale revision is rejected with a structured error and exit code 4; nothing changes.
    let stale = rev(&r) - 1;
    let (code, v) = eak(
        &dir,
        &["exec", &stale.to_string(), r#"{"type":"Annotate"}"#],
    );
    assert_eq!(code, 4, "{v}");
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["code"], "stale_revision");
    assert_eq!(v["error"]["details"]["current_revision"], rev(&r));
    assert_eq!(rev(&v), rev(&r));
    // Missing expected_revision and a wrong project id are refused too.
    let (_, v) = eak(&dir, &["call", "Annotate", "{}"]);
    assert_eq!(v["error"]["code"], "missing_expected_revision");
    let (_, v) = eak(&dir, &["--project-id", "prj_other", "info"]);
    assert_eq!(v["error"]["code"], "project_mismatch");
    // Destructive commands need the user's opt-in.
    let (_, v) = eak(
        &dir,
        &[
            "exec",
            &rev(&r).to_string(),
            &json!({"type": "DeleteSymbol", "symbol_id": c1}).to_string(),
        ],
    );
    assert_eq!(v["error"]["code"], "approval_required", "{v}");

    // PCB: outline, sync, place, route.
    let r = exec(
        &dir,
        rev(&r),
        json!({"type": "SetBoardOutline", "points": [
        {"x": 0, "y": 0}, {"x": 40000000, "y": 0}, {"x": 40000000, "y": 30000000}, {"x": 0, "y": 30000000}]}),
    );
    // Gate (ADR-0036): PCB synchronisation is refused until ERC has run on the schematic.
    let (code, v) = eak(
        &dir,
        &[
            "exec",
            &rev(&r).to_string(),
            r#"{"type":"SyncSchematicToPcb"}"#,
        ],
    );
    assert_ne!(code, 0, "{v}");
    assert_eq!(v["error"]["code"], "precondition", "{v}");
    assert_eq!(v["error"]["details"]["gate"], "pcb_sync", "{v}");
    assert_eq!(rev(&v), rev(&r), "a refused gate must not move the revision");
    let (_, task) = eak(&dir, &["call", "agent_task_status"]);
    assert_eq!(
        task["result"]["session_task"]["phase"], "blocked",
        "{task}"
    );
    // ERC evidence is revision-bound; the outline change before it is not a schematic change.
    let (_, erc) = eak(&dir, &["check", "erc"]);
    assert_eq!(erc["category"], "validation", "{erc}");
    let r = exec(&dir, rev(&r), json!({"type": "SyncSchematicToPcb"}));
    let (_, task) = eak(&dir, &["call", "agent_task_status"]);
    assert_eq!(
        task["result"]["session_task"]["phase"], "syncing_pcb",
        "{task}"
    );
    assert!(task["result"]["session_task"]["starting_revision"].is_u64());
    let r = exec(&dir, rev(&r), json!({"type": "AutoPlace"}));
    let r = exec(&dir, rev(&r), json!({"type": "AutoRoute"}));
    assert!(r["result"]["rerun_checks"]
        .as_array()
        .unwrap()
        .contains(&json!("DRC")));
    let (_, fps) = eak(&dir, &["call", "list_footprints"]);
    assert_eq!(fps["result"]["footprints"].as_array().unwrap().len(), 2);
    assert!(
        fps["result"]["footprints"][0]["pads"][0]["net"].is_string(),
        "{fps}"
    );
    let (_, board) = eak(&dir, &["call", "get_board"]);
    assert_eq!(board["result"]["outline"].as_array().unwrap().len(), 4);
    assert!(
        !board["result"]["tracks"].as_array().unwrap().is_empty(),
        "AutoRoute laid no track"
    );

    // Validation: structured findings, not_implemented and evidence.
    let (code, v) = eak(&dir, &["check", "all"]);
    assert!(code == 0 || code == 3, "{v}");
    assert_eq!(v["category"], "validation");
    let reports = v["result"]["reports"].as_array().unwrap();
    assert_eq!(reports.len(), 4);
    for rep in reports {
        assert!(rep["not_implemented"].is_array() && rep["limitations"].is_array());
        assert_eq!(rep["revision"], rev(&r));
        assert!(Path::new(rep["evidence_path"].as_str().unwrap()).exists());
        for f in rep["findings"].as_array().unwrap() {
            assert!(f["rule_id"].is_string() && f["severity"].is_string());
        }
    }

    // Provenance: journal actor carries agent + session; calls are logged.
    let (_, h) = eak(&dir, &["history"]);
    let actors: Vec<&str> = h["result"]["journal"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["actor"].as_str().unwrap())
        .collect();
    assert!(!actors.is_empty());
    assert!(
        actors.iter().all(|a| *a == "agent:e2e-agent@test-session"),
        "{actors:?}"
    );
    assert!(h["result"]["tool_calls"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["error"] == "stale_revision"));
    assert!(!h["result"]["evidence"].as_array().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn mcp_stdio_round_trip() {
    let root = temp_dir("mcp");
    let dir = root.join("m.eak");
    let mut child = Command::new(env!("CARGO_BIN_EXE_eak-mcp"))
        .env("EAK_PROJECT_DIR", &dir)
        .env("EAK_SESSION_ID", "mcp-session")
        .env("EAK_AGENT_NAME", "mcp-agent")
        .env_remove("EAK_PROJECT_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut rpc = |msg: Value| -> Value {
        writeln!(stdin, "{msg}").unwrap();
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    let init = rpc(
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}}),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "eak-harness");
    let list = rpc(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let tools = list["result"]["tools"].as_array().unwrap();
    let place = tools.iter().find(|t| t["name"] == "PlaceSymbol").unwrap();
    assert!(place["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("expected_revision")));
    let call = |name: &str, args: Value| json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": name, "arguments": args}});
    let created = rpc(call("create_project", json!({"dir": dir, "name": "MCP"})));
    assert_eq!(created["result"]["isError"], false, "{created}");
    let placed = rpc(call(
        "PlaceSymbol",
        json!({"expected_revision": 0, "component_id": "C_0805", "position": {"x": 0, "y": 0}}),
    ));
    let env = &placed["result"]["structuredContent"];
    assert_eq!(env["ok"], true, "{placed}");
    assert_eq!(env["project"]["revision"], 1);
    let stale = rpc(call(
        "PlaceSymbol",
        json!({"expected_revision": 0, "component_id": "C_0805", "position": {"x": 0, "y": 0}}),
    ));
    assert_eq!(stale["result"]["isError"], true);
    assert_eq!(
        stale["result"]["structuredContent"]["error"]["code"],
        "stale_revision"
    );
    let erc = rpc(call("run_check", json!({"check": "erc"})));
    assert_eq!(
        erc["result"]["structuredContent"]["result"]["reports"][0]["check"],
        "ERC"
    );
    drop(stdin);
    assert!(child.wait().unwrap().success());
    let _ = std::fs::remove_dir_all(root);
}

/// The app holds the project lock and serves an endpoint: the CLI must route through it (one
/// writer), and the app's engine sees the change.
#[test]
fn cli_routes_through_the_app_endpoint_when_the_project_is_open() {
    let root = temp_dir("app");
    let dir = root.join("a.eak");
    let engine = Arc::new(Mutex::new(
        eak_runtime::Engine::create(&dir, "App").unwrap(),
    ));
    let e2 = engine.clone();
    let handler: eak_harness::endpoint::Handler =
        Arc::new(move |req: &eak_harness::endpoint::Request| {
            let mut e = e2.lock().unwrap();
            eak_harness::handle(&mut e, &req.context, "app", &req.tool, &req.args)
        });
    let state = root.join("state");
    #[cfg_attr(not(unix), allow(unused_mut))]
    let mut servers =
        vec![eak_harness::endpoint::Server::start_tcp_in(&state, handler.clone()).unwrap()];
    #[cfg(unix)]
    servers.push(eak_harness::endpoint::Server::start(&state, handler).unwrap());
    let pid = engine.lock().unwrap().project().id.clone();
    for (i, server) in servers.iter().enumerate() {
        server.write_record(&dir, &pid).unwrap();
        let (code, v) = eak(
            &dir,
            &[
                "exec",
                &i.to_string(),
                r#"{"type":"AddRequirement","text":"via app"}"#,
            ],
        );
        assert_eq!(code, 0, "{v}");
        assert_eq!(v["route"], "app", "{}: {v}", server.endpoint);
        assert_eq!(engine.lock().unwrap().project().revision, i as u64 + 1);
    }
    // A wrong token is refused.
    let rec = eak_harness::endpoint::read_record(&dir).unwrap();
    let bad = root.join("bad-token");
    std::fs::write(&bad, "nope").unwrap();
    let ctx = eak_harness::CallContext::from_env();
    let v =
        eak_harness::endpoint::call(&rec.endpoint, &bad, "project_info", &json!({}), &ctx).unwrap();
    assert_eq!(v["error"]["code"], "auth_failed");
    // App gone + lock still held by the app's engine: the CLI reports the lock, never opens a second writer.
    drop(servers);
    let (code, v) = eak(&dir, &["info"]);
    assert_eq!(code, 1);
    assert_eq!(v["error"]["code"], "project_locked", "{v}");
    let _ = std::fs::remove_dir_all(root);
}

/// Regression: an app-endpoint client (MCP, any forwarder) that sends its project context must
/// still get the discovery tools, not "does not run against an open project".
#[test]
fn discovery_tools_answer_while_a_project_is_open() {
    let root = temp_dir("discovery");
    let mut engine = eak_runtime::Engine::create(&root.join("d.eak"), "Disc").unwrap();
    let ctx = eak_harness::CallContext::from_env();
    for tool in ["list_tools", "guide", "design_spec_schema"] {
        let v = eak_harness::handle(&mut engine, &ctx, "app", tool, &json!({}));
        assert_eq!(v["ok"], true, "{tool}: {v}");
    }
}
