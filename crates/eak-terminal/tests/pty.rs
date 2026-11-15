//! Real PTY tests: every session here is a genuine OS process on a pseudo-terminal. Tests only
//! ever kill processes they spawned (through the manager that spawned them).
#![cfg(unix)]

use eak_terminal::api::{dispatch, request, serve};
use eak_terminal::{CreateOptions, Manager, OutputEvent, Source, State, WaitFor};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const T: Duration = Duration::from_secs(20);

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "eak-term-test-{name}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&d).unwrap();
    // Canonical form so `pwd` output compares equal (macOS /var -> /private/var).
    std::fs::canonicalize(d).unwrap()
}

fn manager() -> (Arc<Manager>, Arc<Mutex<Vec<OutputEvent>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = Arc::clone(&seen);
    (
        Manager::new(Arc::new(move |e| s.lock().unwrap().push(e))),
        seen,
    )
}

fn sh(m: &Arc<Manager>, cwd: &PathBuf) -> u64 {
    let info = m
        .create(CreateOptions {
            cwd: Some(cwd.clone()),
            shell: Some("/bin/sh".into()),
            ..Default::default()
        })
        .unwrap();
    // The shell is up once it is the terminal's foreground process at its prompt.
    let (st, met) = m
        .wait(info.id, &WaitFor::Exit, None, Duration::from_millis(300))
        .unwrap();
    assert!(!met, "shell exited immediately: {st:?}");
    info.id
}

fn wait_output(m: &Manager, id: u64, pattern: &str) {
    let deadline = std::time::Instant::now() + T;
    while std::time::Instant::now() < deadline {
        if m.read(id, None).unwrap().contains(pattern) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!(
        "`{pattern}` never appeared in session {id}:\n{}",
        m.read(id, None).unwrap()
    );
}

fn wait_state(m: &Manager, id: u64, want: State) -> eak_terminal::Status {
    let deadline = std::time::Instant::now() + T;
    loop {
        let st = m.info(id).unwrap().status;
        if st.state == want {
            return st;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "session {id} never reached {want:?}; last {st:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn concurrent_shells_are_isolated_processes() {
    let (m, seen) = manager();
    let dirs: Vec<PathBuf> = (0..4).map(|i| tmp(&format!("iso{i}"))).collect();
    let ids: Vec<u64> = dirs.iter().map(|d| sh(&m, d)).collect();
    let pids: Vec<u32> = ids
        .iter()
        .map(|id| m.info(*id).unwrap().pid.unwrap())
        .collect();
    let mut unique = pids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        4,
        "each session must be its own process: {pids:?}"
    );

    for (i, id) in ids.iter().enumerate() {
        m.run(*id, &format!("EAKV=val{i}; echo marker-$EAKV; pwd"))
            .unwrap();
    }
    for (i, id) in ids.iter().enumerate() {
        wait_output(&m, *id, &format!("marker-val{i}"));
        wait_output(&m, *id, &dirs[i].display().to_string());
        let out = m.read(*id, None).unwrap();
        for j in 0..4 {
            if j != i {
                assert!(
                    !out.contains(&format!("marker-val{j}")),
                    "session {i} saw session {j}'s output"
                );
            }
        }
    }
    // A variable set in one shell is invisible to the others.
    m.run(ids[1], "echo seen-${EAKV:-unset}").unwrap();
    wait_output(&m, ids[1], "seen-val1");
    // Output was pushed to the sink tagged with the right session.
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .any(|e| e.id == ids[2] && e.data.contains("marker-val2")));
    for id in ids {
        m.close(id).unwrap();
    }
}

#[test]
fn status_follows_the_foreground_process() {
    let (m, _) = manager();
    let id = sh(&m, &tmp("fg"));
    let st = wait_state(&m, id, State::Idle);
    assert_eq!(st.source, Source::Process);
    m.run(id, "sleep 3").unwrap();
    let st = wait_state(&m, id, State::Running);
    assert_eq!(st.source, Source::Process);
    if cfg!(target_os = "linux") {
        assert_eq!(st.foreground.as_deref(), Some("sleep"));
    }
    let (st, met) = m.wait(id, &WaitFor::Idle, None, T).unwrap();
    assert!(met, "never returned to the prompt: {st:?}");
    assert_eq!(st.state, State::Idle);
    m.close(id).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn a_program_reading_the_terminal_is_waiting_for_input() {
    let (m, _) = manager();
    let id = sh(&m, &tmp("cat"));
    m.run(id, "cat").unwrap();
    let st = wait_state(&m, id, State::WaitingForInput);
    assert_eq!(
        st.source,
        Source::Process,
        "kernel wait state, not a text guess: {st:?}"
    );
    m.write(id, b"\x04").unwrap(); // EOF ends cat
    wait_state(&m, id, State::Idle);
    m.close(id).unwrap();
}

#[test]
fn direct_commands_report_exit_codes() {
    let (m, _) = manager();
    let dir = tmp("exit");
    let ok = m
        .create(CreateOptions {
            cwd: Some(dir.clone()),
            command: Some(vec!["/bin/sh".into(), "-c".into(), "echo done-ok".into()]),
            ..Default::default()
        })
        .unwrap();
    let bad = m
        .create(CreateOptions {
            cwd: Some(dir.clone()),
            command: Some(vec!["/bin/sh".into(), "-c".into(), "exit 3".into()]),
            ..Default::default()
        })
        .unwrap();
    let (st, met) = m.wait(ok.id, &WaitFor::Exit, None, T).unwrap();
    assert!(met);
    assert_eq!((st.state, st.exit_code), (State::Completed, Some(0)));
    wait_output(&m, ok.id, "done-ok");
    let (st, _) = m.wait(bad.id, &WaitFor::Exit, None, T).unwrap();
    assert_eq!((st.state, st.exit_code), (State::Failed, Some(3)));
    // Input to a finished process is refused, not silently dropped.
    assert!(m.write(bad.id, b"x").is_err());

    let missing = m
        .create(CreateOptions {
            cwd: Some(dir),
            command: Some(vec!["/definitely/not/a/program".into()]),
            ..Default::default()
        })
        .unwrap();
    let (st, _) = m.wait(missing.id, &WaitFor::Exit, None, T).unwrap();
    assert_eq!(st.state, State::Failed);
}

#[test]
fn close_kills_only_its_own_process_and_restart_starts_a_new_one() {
    let (m, _) = manager();
    let dir = tmp("kill");
    let a = sh(&m, &dir);
    let b = sh(&m, &dir);
    let pid_a = m.info(a).unwrap().pid.unwrap();
    let pid_b = m.info(b).unwrap().pid.unwrap();
    m.restart(a).unwrap();
    let new_a = m.info(a).unwrap().pid.unwrap();
    assert_ne!(pid_a, new_a);
    wait_output(&m, a, "--- restarted ---");
    m.close(a).unwrap();
    // b is untouched and still answers.
    m.run(b, "echo still-alive").unwrap();
    wait_output(&m, b, "still-alive");
    assert_eq!(m.info(b).unwrap().pid, Some(pid_b));
    assert!(m.info(a).is_err());
    m.close(b).unwrap();
}

#[test]
fn sessions_keep_the_project_they_were_opened_in() {
    let (m, _) = manager();
    let (pa, pb) = (tmp("projA"), tmp("projB"));
    m.set_context(
        Some(pa.clone()),
        Some("Project A".into()),
        Some("id-A".into()),
    );
    let a = m
        .create(CreateOptions {
            shell: Some("/bin/sh".into()),
            ..Default::default()
        })
        .unwrap();
    // Opening another project only changes where *new* sessions start.
    m.set_context(
        Some(pb.clone()),
        Some("Project B".into()),
        Some("id-B".into()),
    );
    let b = m
        .create(CreateOptions {
            shell: Some("/bin/sh".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(m.info(a.id).unwrap().spec.cwd, pa);
    assert_eq!(
        m.info(a.id).unwrap().spec.project.as_deref(),
        Some("Project A")
    );
    assert_eq!(m.info(b.id).unwrap().spec.cwd, pb);
    m.run(a.id, "pwd").unwrap();
    wait_output(&m, a.id, &pa.display().to_string());
    m.close(a.id).unwrap();
    m.close(b.id).unwrap();
}

#[test]
fn splits_share_a_tab_and_close_collapses_it() {
    let (m, _) = manager();
    let dir = tmp("split");
    let a = sh(&m, &dir);
    let b = m
        .create(CreateOptions {
            cwd: Some(dir.clone()),
            shell: Some("/bin/sh".into()),
            split_of: Some(a),
            direction: Some(eak_terminal::Direction::Column),
            ..Default::default()
        })
        .unwrap();
    let o = m.overview();
    assert_eq!(o.tabs.len(), 1);
    assert_eq!(o.tabs[0].panes, vec![a, b.id]);
    assert_eq!(o.tabs[0].direction, eak_terminal::Direction::Column);
    assert_eq!(o.focused, Some(b.id));
    m.close(b.id).unwrap();
    assert_eq!(m.overview().tabs[0].panes, vec![a]);
    m.close(a).unwrap();
    assert!(m.overview().tabs.is_empty());
}

#[test]
fn integration_reports_win_over_process_state_until_released() {
    let (m, _) = manager();
    let id = sh(&m, &tmp("report"));
    m.run(id, "cat").unwrap();
    wait_state(&m, id, State::WaitingForInput.min_for_os());
    dispatch(
        &m,
        "session.report_state",
        json!({"id": id, "source": "test-agent", "state": "blocked", "message": "approve edit?"}),
    )
    .unwrap();
    let st = m.info(id).unwrap().status;
    assert_eq!((st.state, st.source), (State::Blocked, Source::Integration));
    assert!(st.detail.contains("approve edit?"));
    // A different source cannot release someone else's report.
    dispatch(&m, "session.release", json!({"id": id, "source": "other"})).unwrap();
    assert_eq!(m.info(id).unwrap().status.state, State::Blocked);
    dispatch(
        &m,
        "session.release",
        json!({"id": id, "source": "test-agent"}),
    )
    .unwrap();
    assert_ne!(m.info(id).unwrap().status.source, Source::Integration);
    m.write(id, b"\x04").unwrap();
    // Back at the prompt any leftover report would be ignored anyway.
    wait_state(&m, id, State::Idle);
    m.close(id).unwrap();
}

trait ForOs {
    fn min_for_os(self) -> State;
}
impl ForOs for State {
    /// Without /proc the kernel wait state is unknown, so `cat` just shows as running.
    fn min_for_os(self) -> State {
        if cfg!(target_os = "linux") {
            self
        } else {
            State::Running
        }
    }
}

#[test]
fn ended_sessions_are_listed_after_a_restart_and_can_be_restarted() {
    let dir = tmp("manifest");
    let manifest = dir.join("terminal-sessions.json");
    {
        let m = Manager::with_manifest(Arc::new(|_| {}), manifest.clone());
        let id = sh(&m, &dir);
        m.rename(id, "build").unwrap();
        m.shutdown();
    }
    let m = Manager::with_manifest(Arc::new(|_| {}), manifest);
    let o = m.overview();
    assert_eq!(o.sessions.len(), 1);
    let s = &o.sessions[0];
    assert_eq!((s.name.as_str(), s.status.state), ("build", State::Ended));
    assert!(s.pid.is_none(), "an ended session has no process");
    assert!(m.write(s.id, b"x").is_err());
    m.restart(s.id).unwrap();
    m.run(s.id, "echo back-again").unwrap();
    wait_output(&m, s.id, "back-again");
    m.close(s.id).unwrap();
}

#[test]
fn api_requires_the_token_and_drives_sessions() {
    let (m, _) = manager();
    let dir = tmp("api");
    let file = dir.join("terminal-api.json");
    let info = serve(Arc::clone(&m), &file).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "token file must be user-only");
    }
    // Wrong token: refused.
    {
        use std::io::{BufRead, BufReader, Write};
        let mut s = std::net::TcpStream::connect(("127.0.0.1", info.port)).unwrap();
        s.write_all(b"{\"token\":\"nope\",\"method\":\"session.list\"}\n")
            .unwrap();
        let mut line = String::new();
        BufReader::new(s).read_line(&mut line).unwrap();
        assert!(line.contains("unauthorized"), "{line}");
    }
    let created = request(
        &file,
        "session.create",
        json!({"cwd": dir, "shell": "/bin/sh", "name": "api"}),
    )
    .unwrap();
    let id = created["id"].as_u64().unwrap();
    // The shell is ready once it is the foreground process at its prompt.
    wait_state(&m, id, State::Idle);
    let since = request(
        &file,
        "session.run",
        json!({"id": id, "command": "echo via-api-$((40+2))"}),
    )
    .unwrap()["since"]
        .clone();
    let r = request(&file, "session.wait", json!({"id": id, "until": "output", "pattern": "via-api-42", "since": since, "timeout_ms": 10000})).unwrap();
    assert_eq!(r["met"], true, "{r}");
    let r = request(
        &file,
        "session.wait",
        json!({"id": id, "until": "idle", "since": since, "timeout_ms": 10000}),
    )
    .unwrap();
    assert_eq!(r["status"]["state"], "idle", "{r}");
    let text = request(&file, "session.read", json!({"id": id, "lines": 5})).unwrap();
    assert!(text["text"].as_str().unwrap().contains("via-api-42"));
    let split = request(
        &file,
        "session.split",
        json!({"id": id, "direction": "column", "shell": "/bin/sh"}),
    )
    .unwrap();
    let list = request(&file, "session.list", json!({})).unwrap();
    assert_eq!(list["tabs"][0]["panes"], json!([id, split["id"]]));
    request(&file, "session.focus", json!({"id": id})).unwrap();
    assert_eq!(
        request(&file, "session.list", json!({})).unwrap()["focused"],
        json!(id)
    );
    assert!(request(&file, "no.such", json!({}))
        .unwrap_err()
        .contains("unknown method"));
    request(&file, "session.close", json!({"id": split["id"]})).unwrap();
    request(&file, "session.close", json!({"id": id})).unwrap();
}

#[test]
fn spaces_isolate_roots_and_sessions() {
    let (m, _) = manager();
    let (eak, desk) = (tmp("EAK"), tmp("DESK"));
    let a = m.create_space("EAK", eak.clone()).unwrap();
    let d = m.create_space("DESK", desk.clone()).unwrap();
    let new_in = |sp: u64| {
        m.create(CreateOptions {
            space: Some(sp),
            shell: Some("/bin/sh".into()),
            ..Default::default()
        })
        .unwrap()
        .id
    };
    let (a1, a2, d1) = (new_in(a.id), new_in(a.id), new_in(d.id));
    for (id, root) in [(a1, &eak), (a2, &eak), (d1, &desk)] {
        assert_eq!(&m.info(id).unwrap().spec.cwd, root);
        m.run(id, "pwd").unwrap();
        wait_output(&m, id, &root.display().to_string());
    }
    // Each session keeps its own cwd: cd in one does not move its neighbour.
    let sub = eak.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    m.run(a1, "cd sub && pwd").unwrap();
    wait_output(&m, a1, &sub.display().to_string());
    m.run(a2, "echo here-$(pwd)").unwrap();
    wait_output(&m, a2, &format!("here-{}", eak.display()));

    let o = m.overview();
    let tabs_of = |sp: u64| {
        o.tabs
            .iter()
            .filter(|t| t.space == sp)
            .map(|t| t.panes[0])
            .collect::<Vec<_>>()
    };
    assert_eq!(tabs_of(a.id), vec![a1, a2]);
    assert_eq!(tabs_of(d.id), vec![d1]);

    // Reorder: a2's tab first within EAK.
    let t2 = m.info(a2).unwrap().tab;
    m.move_tab(t2, 0).unwrap();
    let o = m.overview();
    assert_eq!(
        o.tabs
            .iter()
            .filter(|t| t.space == a.id)
            .map(|t| t.panes[0])
            .collect::<Vec<_>>(),
        vec![a2, a1]
    );

    // Tab rename: a custom name is kept; empty restores the default.
    m.rename_tab(t2, "  Build  ").unwrap();
    assert_eq!(
        m.overview()
            .tabs
            .iter()
            .find(|t| t.id == t2)
            .unwrap()
            .name
            .as_deref(),
        Some("Build")
    );
    m.rename_tab(t2, "").unwrap();
    assert!(m
        .overview()
        .tabs
        .iter()
        .find(|t| t.id == t2)
        .unwrap()
        .name
        .is_none());
    assert!(m.rename_tab(u64::MAX, "x").is_err());

    // Closing DESK terminates only DESK's sessions.
    let d_pid = m.info(d1).unwrap().pid.unwrap();
    m.close_space(d.id).unwrap();
    assert!(m.info(d1).is_err());
    assert!(
        !std::path::Path::new(&format!("/proc/{d_pid}")).exists()
            || !cfg!(target_os = "linux")
            || {
                std::thread::sleep(Duration::from_millis(500));
                !std::path::Path::new(&format!("/proc/{d_pid}")).exists()
            }
    );
    m.run(a1, "echo eak-alive").unwrap();
    wait_output(&m, a1, "eak-alive");
    m.close_space(a.id).unwrap();
    assert!(m.overview().spaces.is_empty());
}

#[test]
fn layout_and_metadata_survive_a_restart_but_processes_do_not() {
    let dir = tmp("layout");
    let manifest = dir.join("terminal-sessions.json");
    {
        let m = Manager::with_manifest(Arc::new(|_| {}), manifest.clone());
        let sp = m.create_space("Board", dir.clone()).unwrap();
        let a = m
            .create(CreateOptions {
                space: Some(sp.id),
                shell: Some("/bin/sh".into()),
                name: Some("left".into()),
                ..Default::default()
            })
            .unwrap();
        m.create(CreateOptions {
            split_of: Some(a.id),
            direction: Some(eak_terminal::Direction::Column),
            shell: Some("/bin/sh".into()),
            name: Some("right".into()),
            ..Default::default()
        })
        .unwrap();
        m.shutdown();
    }
    let m = Manager::with_manifest(Arc::new(|_| {}), manifest);
    let o = m.overview();
    assert_eq!(o.spaces.len(), 1);
    assert_eq!(
        (o.spaces[0].name.as_str(), &o.spaces[0].root),
        ("Board", &dir)
    );
    assert_eq!(o.tabs.len(), 1);
    assert_eq!(o.tabs[0].direction, eak_terminal::Direction::Column);
    let names: Vec<_> = o.tabs[0]
        .panes
        .iter()
        .map(|p| m.info(*p).unwrap().name)
        .collect();
    assert_eq!(names, vec!["left", "right"]);
    assert!(o
        .sessions
        .iter()
        .all(|s| s.status.state == State::Ended && s.pid.is_none()));
}

/// Real agent CLIs in their own PTYs (opt-in: `EAK_TERM_AGENT_TESTS=1`). Each must start
/// its own UI; then the session is closed, which hangs up only that process.
#[test]
#[ignore = "needs pi and opencode installed; run with EAK_TERM_AGENT_TESTS=1 -- --ignored"]
fn real_agent_clis_start_in_their_own_sessions() {
    if std::env::var("EAK_TERM_AGENT_TESTS").is_err() {
        return;
    }
    let (m, _) = manager();
    let dir = tmp("agents");
    let sp = m.create_space("agents", dir.clone()).unwrap();
    let mut ids = Vec::new();
    for agent in ["pi", "opencode"] {
        let id = m
            .create(CreateOptions {
                space: Some(sp.id),
                shell: Some("/bin/bash".into()),
                cols: Some(120),
                rows: Some(40),
                ..Default::default()
            })
            .unwrap()
            .id;
        wait_state(&m, id, State::Idle);
        let since = m.run(id, agent).unwrap();
        let mut from = 0;
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            let info = m.info(id).unwrap();
            answer_queries(&m, id, &mut from);
            let text = m.read(id, None).unwrap();
            let painted = text
                .split_once(&format!("$ {agent}"))
                .map(|x| x.1)
                .unwrap_or("")
                .chars()
                .filter(|c| c.is_alphanumeric())
                .count();
            if info.status.agent.as_deref() == Some(agent)
                && info.output_seq - since > 3000
                && painted > 40
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{agent} UI never started: {:?}\nraw: {:?}",
                info.status,
                m.scrollback(id)
                    .unwrap()
                    .0
                    .chars()
                    .rev()
                    .take(600)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            );
            std::thread::sleep(Duration::from_millis(200));
        }
        let painted: String = m
            .read(id, None)
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!(
            "--- {agent} painted text (tail) ---\n{}",
            painted
                .chars()
                .rev()
                .take(500)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        );
        ids.push((agent, id));
    }
    for (agent, id) in ids {
        let pid = m.info(id).unwrap().pid.unwrap();
        m.close(id).unwrap();
        std::thread::sleep(Duration::from_millis(800));
        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "{agent} shell {pid} still running"
        );
    }
}

/// Answer the terminal queries a TUI sends at start-up the way a real terminal (xterm.js in
/// the app) would, so a headless test sees the UI draw. Returns how many were answered.
fn answer_queries(m: &Manager, id: u64, from: &mut usize) -> usize {
    let raw = m.scrollback(id).unwrap().0;
    let new = &raw[(*from).min(raw.len())..];
    let mut replies = String::new();
    let mut n = 0;
    let mut rest = new;
    while let Some(i) = rest.find('\x1b') {
        rest = &rest[i..];
        let q = |p: &str| rest.starts_with(p);
        if q("\x1b[6n") {
            replies.push_str("\x1b[1;1R");
        } else if q("\x1b]10;?") {
            replies.push_str("\x1b]10;rgb:ffff/ffff/ffff\x07");
        } else if q("\x1b]11;?") {
            replies.push_str("\x1b]11;rgb:0000/0000/0000\x07");
        } else if q("\x1b[14t") {
            replies.push_str("\x1b[4;800;1200t");
        } else if q("\x1b[?u") {
            replies.push_str("\x1b[?0u");
        } else if q("\x1b[>0q") {
            replies.push_str("\x1bP>|xterm.js(5)\x1b\\");
        } else if q("\x1bP+q") {
            replies.push_str("\x1bP0+r\x1b\\");
        } else if q("\x1b[?")
            && rest[3..]
                .find("$p")
                .is_some_and(|e| rest[3..3 + e].bytes().all(|b| b.is_ascii_digit()))
        {
            let e = rest[3..].find("$p").unwrap();
            replies.push_str(&format!("\x1b[?{};0$y", &rest[3..3 + e]));
        } else if q("\x1b[c") || q("\x1b[0c") {
            replies.push_str("\x1b[?62;22c");
        } else if q("\x1b]4;") {
            let body = &rest[4..];
            if let Some(end) = body.find(";?") {
                replies.push_str(&format!("\x1b]4;{};rgb:8080/8080/8080\x07", &body[..end]));
            }
        } else {
            rest = &rest[1..];
            continue;
        }
        n += 1;
        rest = &rest[1..];
    }
    *from = raw.len();
    if !replies.is_empty() {
        m.write(id, replies.as_bytes()).unwrap();
    }
    n
}

#[test]
fn sessions_export_the_eak_context() {
    let (m, _) = manager();
    let dir = tmp("ctx");
    m.set_context(
        Some(dir.clone()),
        Some("Ctx".into()),
        Some("proj-123".into()),
    );
    let id = m
        .create(CreateOptions {
            shell: Some("/bin/sh".into()),
            env: vec![],
            ..Default::default()
        })
        .unwrap()
        .id;
    wait_state(&m, id, State::Idle);
    m.run(
        id,
        "echo ctx:$EAK_PROJECT_DIR:$EAK_PROJECT_ID:$EAK_SESSION_ID:end",
    )
    .unwrap();
    wait_output(&m, id, &format!("ctx:{}:proj-123:{id}:end", dir.display()));
    m.close(id).unwrap();
}

/// Spaces on the active project's folder carry its id, so their sessions export
/// `EAK_PROJECT_ID` for the harness; spaces elsewhere stay unbound.
#[test]
fn spaces_on_the_project_folder_are_bound_to_the_project_id() {
    let (m, _) = manager();
    let proj = tmp("bind-proj");
    let other = tmp("bind-other");
    let early = m.create_space("early", proj.clone()).unwrap();
    assert_eq!(early.project_id, None);
    m.set_context(Some(proj.clone()), Some("Board".into()), Some("p-1".into()));
    let late = m.create_space("late", proj.clone()).unwrap();
    let elsewhere = m.create_space("other", other).unwrap();
    assert_eq!(late.project_id.as_deref(), Some("p-1"));
    assert_eq!(elsewhere.project_id, None);
    let spaces = m.overview().spaces;
    let get = |id| {
        spaces
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .project_id
            .clone()
    };
    assert_eq!(
        get(early.id).as_deref(),
        Some("p-1"),
        "existing space bound by set_context"
    );
}

/// Regression (security review): per-session environment can hold credentials and must never
/// be written to the layout file. Written during the testing freeze; never run.
#[test]
fn session_environment_is_not_persisted() {
    let dir = tmp("env-secret");
    let manifest = dir.join("terminal-sessions.json");
    {
        let m = Manager::with_manifest(Arc::new(|_| {}), manifest.clone());
        m.create(CreateOptions {
            cwd: Some(dir.clone()),
            shell: Some("/bin/sh".into()),
            env: vec![("SECRET_TOKEN".into(), "do-not-persist-4f2a".into())],
            ..Default::default()
        })
        .unwrap();
        m.shutdown();
    }
    let saved = std::fs::read_to_string(&manifest).unwrap();
    assert!(!saved.contains("do-not-persist-4f2a"), "{saved}");
    assert!(!saved.contains("SECRET_TOKEN"), "{saved}");
}
