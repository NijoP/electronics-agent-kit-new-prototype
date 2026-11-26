//! The adapters against fake `claude` and `codex` binaries on a temp search path: probe
//! (found / version / signed-in), connect (idempotent) and disconnect through the tool's own
//! `mcp` commands, and a headless session whose stream becomes events and a log file.
//!
//! One test function on purpose: writing and then executing scripts from parallel test threads
//! can fail with "text file busy" on Linux. The fakes are shell scripts, so this runs on Unix;
//! the parsers it relies on are unit-tested on every OS.
#![cfg(unix)]

use eak_agents::sessions::{SessionState, SessionView, Sessions, Start};
use eak_agents::{builtin_tools, find_tool, launch_plan, probe, AgentEvent, Locator, Surface};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-agents-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn script(dir: &Path, name: &str, body: &str) {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\nD=$(dirname \"$0\")\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Claude Code as its `--help` documents it; the registration lives in `$D/claude.reg`.
const CLAUDE: &str = r#"
case "$1 $2" in
  "--version "*) echo "9.1.0 (Claude Code)" ;;
  "auth status") if [ -f "$D/claude.auth" ]; then echo '{"loggedIn":true,"authMethod":"claude.ai","email":"x@example.com"}'; else echo '{"loggedIn":false}'; exit 1; fi ;;
  "mcp get") if [ -f "$D/claude.reg" ]; then echo "eak:"; echo "  Scope: Local config (private to you in this project)"; echo "  Args: $(cat "$D/claude.reg")"; else echo 'No MCP server named "eak". Configured servers: none' >&2; exit 1; fi ;;
  "mcp add") if [ -f "$D/claude.reg" ]; then echo "MCP server eak already exists in local config" >&2; exit 1; fi; pwd -P > "$D/claude.cwd"; : > "$D/claude.env"; while [ "$1" != "--" ]; do [ "$1" = "-e" ] && echo "$2" >> "$D/claude.env"; shift; done; shift; echo "$*" > "$D/claude.reg"; echo "Added stdio MCP server eak" ;;
  "mcp remove") if [ -f "$D/claude.reg" ]; then rm "$D/claude.reg"; else echo 'No MCP server found with name: "eak"' >&2; exit 1; fi ;;
  "--allowedTools "*)
    echo "$EAK_SESSION_ID" > "$D/claude.session"
    echo '{"type":"system","subtype":"init","model":"fake"}'
    echo '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__eak__design_report","input":{}}]}}'
    echo "progress on stderr" >&2
    echo '{"type":"result","subtype":"success","is_error":false,"result":"done: '"$EAK_AGENT_NAME"' '"$EAK_TASK_ID"'"}' ;;
  *) echo "unexpected: $*" >&2; exit 2 ;;
esac
"#;

/// Codex as its `--help` documents it: user-level registration in `$D/codex.reg`.
const CODEX: &str = r#"
if [ "$1" = "-c" ]; then echo "$2 $EAK_SESSION_ID" > "$D/codex.session"; shift 2; fi
case "$1 $2" in
  "--version "*) echo "codex-cli 0.200.0" ;;
  "login status") echo "Logged in using ChatGPT" ;;
  "mcp get") if [ -f "$D/codex.reg" ]; then printf '{"name":"eak","transport":{"type":"stdio","command":"x","args":[%s]}}\n' "$(cat "$D/codex.reg")"; else echo "No MCP server named 'eak' found." >&2; exit 1; fi ;;
  "mcp add") : > "$D/codex.env"; while [ "$1" != "--" ]; do [ "$1" = "--env" ] && echo "$2" >> "$D/codex.env"; shift; done; shift; out=""; for a in "$@"; do out="$out${out:+,}\"$a\""; done; echo "$out" > "$D/codex.reg"; echo "Added global MCP server 'eak'." ;;
  "mcp remove") rm -f "$D/codex.reg"; echo "Removed global MCP server 'eak'." ;;
  "exec --json") echo '{"type":"thread.started","thread_id":"t"}'; echo '{"type":"turn.failed","error":{"message":"quota"}}' ;;
  *) echo "unexpected: $*" >&2; exit 2 ;;
esac
"#;

fn wait_done(s: &Sessions, id: &str) -> SessionView {
    let start = Instant::now();
    loop {
        let v = s.get(id).unwrap();
        if v.state != SessionState::Running || start.elapsed() > Duration::from_secs(20) {
            return v;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn fake_tools_probe_connect_disconnect_and_run_headless() {
    let bin = tmpdir("bin");
    let project = tmpdir("project");
    let other = tmpdir("other");
    let server = bin.join("eak-mcp");
    script(&bin, "claude", CLAUDE);
    script(&bin, "codex", CODEX);
    script(&bin, "eak-mcp", "exit 0");
    script(&bin, "sleepy", "exec sleep 30");
    let loc = Locator::only(vec![bin.clone()]);
    let tools = builtin_tools();
    let claude = find_tool(&tools, "claude").unwrap();
    let codex = find_tool(&tools, "codex").unwrap();

    // Probe: found, version, signed out (the tool says so), not connected.
    let st = probe(claude, &loc, Some(&project));
    assert_eq!(st.version.as_deref(), Some("9.1.0"));
    assert_eq!(st.signed_in, Some(false));
    assert!(!st.available);
    assert_eq!(st.connected, Some(false));
    std::fs::write(bin.join("claude.auth"), "").unwrap();
    let st = probe(claude, &loc, Some(&project));
    assert_eq!(st.signed_in, Some(true));
    assert!(st.available);
    assert!(!st.auth_detail.unwrap().contains("example.com"));
    // A tool that is not installed is "not found", never guessed.
    let oc = probe(find_tool(&tools, "opencode").unwrap(), &loc, Some(&project));
    assert!(oc.binary.is_none() && !oc.available && oc.signed_in.is_none());

    // Connect writes the registration with the tool's own `mcp add`, run in the project folder.
    let bin_claude = loc.find("claude").unwrap();
    claude.connect(&bin_claude, &project, &server).unwrap();
    let reg = std::fs::read_to_string(bin.join("claude.reg")).unwrap();
    assert!(reg.contains("--project") && reg.contains(&project.display().to_string()));
    // The agent identity reaches the MCP registration (WEFT registry: agent:<name>@<session>).
    assert_eq!(
        std::fs::read_to_string(bin.join("claude.env")).unwrap(),
        "EAK_AGENT_NAME=claude-code\nEAK_AGENT_TOOL=claude-code\n"
    );
    assert!(reg.contains("--agent claude-code"));
    assert_eq!(
        std::fs::read_to_string(bin.join("claude.cwd"))
            .unwrap()
            .trim(),
        project.canonicalize().unwrap().display().to_string()
    );
    assert_eq!(probe(claude, &loc, Some(&project)).connected, Some(true));
    // Idempotent: a second Connect succeeds and leaves one registration.
    claude.connect(&bin_claude, &project, &server).unwrap();
    assert_eq!(probe(claude, &loc, Some(&project)).connected, Some(true));
    // Another project is not connected by this registration.
    assert_eq!(probe(claude, &loc, Some(&other)).connected, Some(false));
    claude.disconnect(&bin_claude, &project).unwrap();
    assert_eq!(probe(claude, &loc, Some(&project)).connected, Some(false));
    assert!(claude.disconnect(&bin_claude, &project).is_err());

    // Codex: one user-level registration; disconnect refuses to remove another project's.
    let bin_codex = loc.find("codex").unwrap();
    let st = probe(codex, &loc, Some(&project));
    assert_eq!(st.signed_in, Some(true));
    assert_eq!(st.connected, Some(false));
    codex.connect(&bin_codex, &other, &server).unwrap();
    let st = probe(codex, &loc, Some(&project));
    assert_eq!(st.connected, Some(false));
    assert!(st.connection_detail.unwrap().contains("another project"));
    assert!(codex.disconnect(&bin_codex, &project).is_err());
    codex.connect(&bin_codex, &project, &server).unwrap();
    assert_eq!(probe(codex, &loc, Some(&project)).connected, Some(true));
    assert_eq!(
        std::fs::read_to_string(bin.join("codex.env")).unwrap(),
        "EAK_AGENT_NAME=codex\nEAK_AGENT_TOOL=codex\n"
    );
    codex.disconnect(&bin_codex, &project).unwrap();
    assert_eq!(probe(codex, &loc, Some(&project)).connected, Some(false));

    // Headless Claude: events parsed from its stream, stderr kept, log file written, and the
    // task id and agent name reach the process.
    let sessions = Sessions::new(Some(tmpdir("logs")));
    let id = sessions.next_id("claude");
    let plan = launch_plan(
        claude,
        &bin_claude,
        &project,
        Surface::Headless,
        Some("check the board"),
        &id,
    )
    .unwrap();
    let start = Start {
        id: id.clone(),
        tool: "claude",
        label: "Claude Code",
        plan: &plan,
        prompt: Some("check the board"),
    };
    let v = sessions
        .spawn_headless(start, claude.progress_parser())
        .unwrap();
    assert_eq!(v.state, SessionState::Running);
    let done = wait_done(&sessions, &id);
    assert_eq!(done.state, SessionState::Completed, "{done:?}");
    assert_eq!(done.exit_code, Some(0));
    assert!(done.events.contains(&AgentEvent::ToolCall {
        name: "mcp__eak__design_report".into()
    }));
    let want = format!("claude-code {id}");
    // Claude passes its environment on to the eak-mcp server it starts.
    assert_eq!(
        std::fs::read_to_string(bin.join("claude.session"))
            .unwrap()
            .trim(),
        id
    );
    assert!(
        done.events.iter().any(|e| matches!(
            e,
            AgentEvent::Finished { ok: true, summary } if summary.contains(&want)
        )),
        "{:?}",
        done.events
    );
    let tail = sessions.tail(&id, 50).unwrap();
    assert!(tail.iter().any(|l| l == "stderr: progress on stderr"));
    let log = std::fs::read_to_string(done.log_file.unwrap()).unwrap();
    assert!(log.contains("\"type\":\"result\""));

    // Headless Codex whose stream reports a failed turn is Failed even with exit code 0.
    let id2 = sessions.next_id("codex");
    let plan2 = launch_plan(
        codex,
        &bin_codex,
        &project,
        Surface::Headless,
        Some("route it"),
        &id2,
    )
    .unwrap();
    let start2 = Start {
        id: id2.clone(),
        tool: "codex",
        label: "Codex",
        plan: &plan2,
        prompt: Some("route it"),
    };
    sessions
        .spawn_headless(start2, codex.progress_parser())
        .unwrap();
    let done2 = wait_done(&sessions, &id2);
    assert_eq!(done2.state, SessionState::Failed, "{done2:?}");
    // Codex is told to pass the session id on to its eak-mcp server, for this run only.
    assert_eq!(
        std::fs::read_to_string(bin.join("codex.session"))
            .unwrap()
            .trim(),
        format!("mcp_servers.eak.env_vars=[\"EAK_SESSION_ID\"] {id2}")
    );
    assert_eq!(sessions.list().first().map(|v| v.id.clone()), Some(id2));

    // A long-running session can be stopped.
    let plan3 = eak_agents::LaunchPlan {
        argv: vec![bin.join("sleepy").display().to_string()],
        env: vec![],
        cwd: project.clone(),
        surface: Surface::Headless,
    };
    let id3 = sessions.next_id("sleepy");
    let start3 = Start {
        id: id3.clone(),
        tool: "sleepy",
        label: "Sleepy",
        plan: &plan3,
        prompt: None,
    };
    sessions.spawn_headless(start3, None).unwrap();
    let stopped = sessions.stop(&id3).unwrap();
    assert_eq!(stopped.state, SessionState::Stopped);
    assert_eq!(wait_done(&sessions, &id3).state, SessionState::Stopped);
}
