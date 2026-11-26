//! Settings › AI tools at the IPC level (remap W2-E): rows are honest when nothing is installed
//! (CI has no Claude Code or Codex), Sign in opens the tool's own login as a tab of the existing
//! Terminal workspace, and launched sessions are listed with their state and log.

use eak_agents::sessions::Sessions;
use eak_agents::{builtin_tools, Locator, Surface, Unavailable};
use eak_desktop::agents::{self, AgentsState};
use eak_terminal::Manager;
use std::path::PathBuf;
use std::sync::Arc;

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-desktop-agents-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn state(dirs: Vec<PathBuf>) -> AgentsState {
    AgentsState {
        tools: builtin_tools(),
        locator: Locator::only(dirs),
        sessions: Sessions::new(None),
        server: None,
    }
}

#[test]
fn nothing_installed_shows_not_found_and_every_action_says_why() {
    let s = state(vec![]);
    let project = tmpdir("none");
    let rows = agents::tools(&s, Some(&project));
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["claude", "codex", "opencode", "gemini"]);
    for r in &rows {
        assert_eq!(r.reason, Some(Unavailable::MissingBinary), "{}", r.id);
        assert!(!r.available);
        assert_eq!(r.signed_in, None, "unknown, never signed in");
        assert_eq!(r.connected, None);
    }
    let manager = Manager::new(Arc::new(|_: eak_terminal::OutputEvent| {}));
    let e = agents::connect(&s, "claude", Some(&project)).unwrap_err();
    assert!(e.contains("eak-mcp"), "{e}");
    let e = agents::login(&s, &manager, "claude", Some(&project)).unwrap_err();
    assert!(e.contains("not installed"), "{e}");
    let e = agents::launch(&s, &manager, "codex", None, Surface::Terminal, None).unwrap_err();
    assert!(e.contains("open a project"), "{e}");
    assert!(agents::launch(
        &s,
        &manager,
        "nope",
        Some(&project),
        Surface::Terminal,
        None
    )
    .is_err());
    assert!(agents::sessions(&s, &manager).is_empty());
}

/// A fake `claude`: `auth login` prints and waits for Enter, like a real sign-in prompt.
#[cfg(unix)]
#[test]
fn sign_in_and_launch_open_terminal_tabs_and_sessions_are_listed() {
    use eak_agents::sessions::SessionState;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let bin = tmpdir("bin");
    let project = tmpdir("project");
    let fake = bin.join("claude");
    std::fs::write(
        &fake,
        "#!/bin/sh\ncase \"$1 $2\" in\n  \"auth login\") echo \"fake sign-in for $EAK_AGENT_NAME\"; read x ;;\n  \"--allowedTools \"*) echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"ok\"}' ;;\n  *) echo \"fake claude session $EAK_TASK_ID\"; read x ;;\nesac\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let s = state(vec![bin.clone()]);
    let manager = Manager::new(Arc::new(|_: eak_terminal::OutputEvent| {}));

    // Sign in: a new tab running the tool's own login, in the project's terminal space.
    let info = agents::login(&s, &manager, "claude", Some(&project)).unwrap();
    let spec = serde_json::to_value(&info.spec).unwrap();
    let argv: Vec<String> = serde_json::from_value(spec["program"]["argv"].clone()).unwrap();
    assert_eq!(
        argv,
        [fake.display().to_string(), "auth".into(), "login".into()]
    );
    let ov = manager.overview();
    let space = ov.spaces.iter().find(|sp| sp.id == info.space).unwrap();
    assert_eq!(space.root, project);
    assert!(ov.tabs.iter().any(|t| t.panes.contains(&info.id)));

    // Launch in a terminal tab: listed, its log is the tab's output, Stop closes the tab.
    let v = agents::launch(
        &s,
        &manager,
        "claude",
        Some(&project),
        Surface::Terminal,
        None,
    )
    .unwrap();
    assert_eq!(v.surface, Surface::Terminal);
    let term = v.terminal_session.unwrap();
    assert_eq!(
        manager
            .overview()
            .spaces
            .iter()
            .filter(|sp| sp.root == project)
            .count(),
        1,
        "the login and the session share the project's space"
    );
    let start = Instant::now();
    let log = loop {
        let log = agents::session_log(&s, &manager, &v.id, 20).unwrap();
        if log.contains("fake claude session") || start.elapsed() > Duration::from_secs(10) {
            break log;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(
        log.contains(&format!("fake claude session {}", v.id)),
        "{log}"
    );
    let listed = agents::sessions(&s, &manager);
    assert_eq!(listed[0].id, v.id);
    assert_eq!(listed[0].state, SessionState::Running);
    let stopped = agents::stop(&s, &manager, &v.id).unwrap();
    assert_eq!(stopped.state, SessionState::Stopped);
    assert!(manager.info(term).is_err(), "the tab is closed");

    // Headless: a background process whose result is parsed.
    let h = agents::launch(
        &s,
        &manager,
        "claude",
        Some(&project),
        Surface::Headless,
        Some("check"),
    )
    .unwrap();
    let start = Instant::now();
    let done = loop {
        let v = agents::sessions(&s, &manager)
            .into_iter()
            .find(|x| x.id == h.id)
            .unwrap();
        if v.state != SessionState::Running || start.elapsed() > Duration::from_secs(10) {
            break v;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(done.state, SessionState::Completed, "{done:?}");
    manager.shutdown();
}
