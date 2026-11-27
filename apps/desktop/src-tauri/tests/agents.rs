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
    assert_eq!(ids, ["claude", "codex", "opencode", "gemini", "kimi"]);
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
        "#!/bin/sh\ncase \"$1 $2\" in\n  \"auth login\") echo \"fake sign-in for $EAK_AGENT_NAME\"; read x ;;\n  \"--allowedTools \"*) echo '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"ok\"}' ;;\n  *) echo \"fake claude session $EAK_TASK_ID $EAK_HARNESS_APPLY_MODE\"; read x ;;\nesac\n",
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
    // Sessions started from Settings run the harness in propose mode.
    assert!(
        log.contains(&format!("fake claude session {} propose", v.id)),
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

    // Restart: the same tool, surface and first message as a new session.
    let again = agents::restart(&s, &manager, &h.id).unwrap();
    assert_ne!(again.id, h.id);
    assert_eq!(again.surface, Surface::Headless);
    assert_eq!(again.prompt.as_deref(), Some("check"));
    let start = Instant::now();
    let done = loop {
        let v = agents::sessions(&s, &manager)
            .into_iter()
            .find(|x| x.id == again.id)
            .unwrap();
        if v.state != SessionState::Running || start.elapsed() > Duration::from_secs(10) {
            break v;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(done.state, SessionState::Completed, "{done:?}");
    // A stopped terminal session restarts in a new tab, which Stop closes again.
    let tab = agents::restart(&s, &manager, &v.id).unwrap();
    assert_eq!(tab.surface, Surface::Terminal);
    let new_term = tab.terminal_session.unwrap();
    assert_ne!(new_term, term);
    assert!(manager.info(new_term).is_ok());
    agents::stop(&s, &manager, &tab.id).unwrap();
    assert!(manager.info(new_term).is_err(), "the new tab is closed");
    assert!(agents::restart(&s, &manager, "no-such-session").is_err());
    manager.shutdown();
}

/// The Agents workspace's WEFT view (ADR-0058) replays the project's real journal file: the
/// registry with registration times and revisions, tasks with their creation revision, last
/// status change and unmet dependencies, every message, and broken lines counted, not guessed.
#[test]
fn weft_view_replays_the_project_journal_with_times_revisions_and_waits() {
    use eak_harness::weft::{AgentCard, Entry, Event, Message, MessageKind, Task, TaskStatus};
    let dir = tmpdir("weft");
    let empty = agents::weft_view(&dir, 0);
    assert!(empty.agents.is_empty() && empty.tasks.is_empty() && empty.messages.is_empty());
    assert_eq!((empty.entries, empty.unreadable), (0, 0));

    let card = |name: &str, session: &str| AgentCard {
        id: format!("agent:{name}@{session}"),
        name: name.into(),
        session_id: session.into(),
        tool: "codex".into(),
        model: String::new(),
        capabilities: vec!["plan".into()],
    };
    let task = |id: &str, deps: &[&str]| Task {
        id: id.into(),
        title: format!("task {id}"),
        owner: "agent:layout@s2".into(),
        created_by: "agent:plan@s1".into(),
        depends_on: deps.iter().map(|d| d.to_string()).collect(),
        parent: None,
        status: TaskStatus::Pending,
    };
    let entry = |seq: u64, ts_ms: u64, revision: u64, by: &str, event: Event| {
        serde_json::to_string(&Entry {
            seq,
            ts_ms,
            by: by.into(),
            revision,
            event,
        })
        .unwrap()
    };
    let delegate = Message {
        id: "m1".into(),
        kind: MessageKind::Delegate,
        from: "agent:plan@s1".into(),
        to: "agent:layout@s2".into(),
        task_id: Some("t1".into()),
        proposal_id: None,
        body: "place the input capacitor".into(),
        verdict: None,
        status: None,
    };
    let lines = [
        entry(
            1,
            1000,
            2,
            "agent:plan@s1",
            Event::AgentRegistered {
                agent: card("plan", "s1"),
            },
        ),
        entry(
            2,
            1100,
            2,
            "agent:layout@s2",
            Event::AgentRegistered {
                agent: card("layout", "s2"),
            },
        ),
        entry(
            3,
            1200,
            3,
            "agent:plan@s1",
            Event::TaskCreated {
                task: task("t1", &[]),
            },
        ),
        entry(
            4,
            1300,
            3,
            "agent:plan@s1",
            Event::TaskCreated {
                task: task("t2", &["t1"]),
            },
        ),
        entry(
            5,
            1300,
            3,
            "agent:plan@s1",
            Event::Message { message: delegate },
        ),
        entry(
            6,
            1400,
            4,
            "agent:layout@s2",
            Event::TaskStatus {
                task_id: "t1".into(),
                status: TaskStatus::InProgress,
            },
        ),
    ];
    std::fs::create_dir_all(dir.join("weft")).unwrap();
    std::fs::write(
        dir.join("weft").join("journal.jsonl"),
        format!("{}\n{{broken\n", lines.join("\n")),
    )
    .unwrap();

    let v = agents::weft_view(&dir, 5);
    assert_eq!(v.revision, 5);
    assert_eq!((v.entries, v.unreadable), (6, 1));
    let ids: Vec<&str> = v.agents.iter().map(|a| a.card.id.as_str()).collect();
    assert_eq!(ids, ["agent:layout@s2", "agent:plan@s1"]);
    let plan = &v.agents[1];
    assert_eq!((plan.registered_ms, plan.registered_revision), (1000, 2));

    let t1 = v.tasks.iter().find(|t| t.task.id == "t1").unwrap();
    assert_eq!(t1.task.status, TaskStatus::InProgress);
    assert_eq!(
        (t1.created_ms, t1.created_revision, t1.updated_ms),
        (1200, 3, 1400)
    );
    assert!(t1.waiting_on.is_empty() && !t1.ready);
    let t2 = v.tasks.iter().find(|t| t.task.id == "t2").unwrap();
    assert_eq!(t2.waiting_on, ["t1"]);
    assert!(!t2.ready, "t2 waits for t1");
    assert_eq!(t2.updated_ms, 1300, "never changed: its creation time");

    assert_eq!(v.messages.len(), 1);
    let m = &v.messages[0];
    assert_eq!((m.seq, m.ts_ms, m.revision), (5, 1300, 3));
    assert_eq!(m.message.kind, MessageKind::Delegate);
    assert_eq!(m.message.task_id.as_deref(), Some("t1"));
}
