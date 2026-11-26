//! Agent sessions started from EAK: headless child processes, whose output is captured to a log
//! file and parsed into events, and terminal tabs (the desktop owns those PTYs and records
//! them here so one list shows every session).

use crate::{AgentEvent, LaunchPlan, Parser, Surface};
use serde::Serialize;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Output lines kept in memory per session (the log file has all of them).
const KEEP_LINES: usize = 2000;
/// Parsed events kept per session.
const KEEP_EVENTS: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Running,
    /// The process exited with code 0 (and its stream did not report a failure).
    Completed,
    Failed,
    /// Stopped from EAK.
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    /// Also the session's task id (`EAK_TASK_ID`).
    pub id: String,
    pub tool: String,
    pub label: String,
    pub surface: Surface,
    pub project: String,
    pub prompt: Option<String>,
    /// Unix seconds.
    pub started_at: u64,
    pub state: SessionState,
    pub detail: String,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
    /// Terminal workspace session id, for a terminal-surface session.
    pub terminal_session: Option<u64>,
    pub log_file: Option<String>,
    pub events: Vec<AgentEvent>,
}

/// What a new session is: its id, tool, plan and first message.
pub struct Start<'a> {
    pub id: String,
    pub tool: &'a str,
    pub label: &'a str,
    pub plan: &'a LaunchPlan,
    pub prompt: Option<&'a str>,
}

struct Shared {
    view: SessionView,
    lines: VecDeque<String>,
}

struct Entry {
    shared: Arc<Mutex<Shared>>,
    child: Option<Arc<Mutex<Child>>>,
}

/// Every session started in this app run.
pub struct Sessions {
    entries: Mutex<Vec<Entry>>,
    counter: Mutex<u64>,
    log_dir: Option<PathBuf>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn new_view(s: &Start<'_>, surface: Surface, detail: &str) -> SessionView {
    SessionView {
        id: s.id.clone(),
        tool: s.tool.into(),
        label: s.label.into(),
        surface,
        project: s.plan.cwd.display().to_string(),
        prompt: s.prompt.map(str::to_string),
        started_at: now_secs(),
        state: SessionState::Running,
        detail: detail.into(),
        exit_code: None,
        pid: None,
        terminal_session: None,
        log_file: None,
        events: Vec::new(),
    }
}

impl Sessions {
    /// Headless logs go to `log_dir/<id>.log` (the app data folder, never a project folder).
    pub fn new(log_dir: Option<PathBuf>) -> Sessions {
        Sessions {
            entries: Mutex::new(Vec::new()),
            counter: Mutex::new(0),
            log_dir,
        }
    }

    /// A new session id, unique across app runs (it names the log file).
    pub fn next_id(&self, tool: &str) -> String {
        let mut n = lock(&self.counter);
        *n += 1;
        format!("{tool}-{}-{}", now_secs(), *n)
    }

    /// Start a headless session and capture its output.
    pub fn spawn_headless(
        &self,
        start: Start<'_>,
        parser: Option<Parser>,
    ) -> Result<SessionView, String> {
        let (program, args) = start
            .plan
            .argv
            .split_first()
            .ok_or("the launch plan has no program")?;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(&start.plan.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &start.plan.env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("could not start {}: {e}", start.label))?;
        let log = self.log_dir.as_ref().and_then(|d| {
            std::fs::create_dir_all(d).ok()?;
            let p = d.join(format!("{}.log", start.id));
            File::create(&p).ok().map(|f| (p, f))
        });
        let mut view = new_view(&start, Surface::Headless, "running");
        view.pid = Some(child.id());
        view.log_file = log.as_ref().map(|(p, _)| p.display().to_string());
        let writer = log.map(|(_, f)| Arc::new(Mutex::new(f)));
        let shared = Arc::new(Mutex::new(Shared {
            view: view.clone(),
            lines: VecDeque::new(),
        }));
        let mut readers = Vec::new();
        if let Some(out) = child.stdout.take() {
            readers.push(pump(out, "", parser, Arc::clone(&shared), writer.clone()));
        }
        if let Some(err) = child.stderr.take() {
            readers.push(pump(err, "stderr: ", None, Arc::clone(&shared), writer));
        }
        let child = Arc::new(Mutex::new(child));
        wait_for_exit(Arc::clone(&child), Arc::clone(&shared), readers);
        lock(&self.entries).push(Entry {
            shared,
            child: Some(child),
        });
        Ok(view)
    }

    /// Record a session the desktop started in a terminal tab.
    pub fn register_terminal(
        &self,
        start: Start<'_>,
        terminal_session: u64,
        pid: Option<u32>,
    ) -> SessionView {
        let mut view = new_view(&start, Surface::Terminal, "running in a terminal tab");
        view.pid = pid;
        view.terminal_session = Some(terminal_session);
        lock(&self.entries).push(Entry {
            shared: Arc::new(Mutex::new(Shared {
                view: view.clone(),
                lines: VecDeque::new(),
            })),
            child: None,
        });
        view
    }

    /// Newest first.
    pub fn list(&self) -> Vec<SessionView> {
        let mut out: Vec<SessionView> = lock(&self.entries)
            .iter()
            .map(|e| lock(&e.shared).view.clone())
            .collect();
        out.reverse();
        out
    }

    pub fn get(&self, id: &str) -> Option<SessionView> {
        self.with(id, |s| s.view.clone())
    }

    /// Update a terminal session from the terminal engine's status.
    pub fn set_state(&self, id: &str, state: SessionState, detail: &str, exit_code: Option<i32>) {
        self.with(id, |s| {
            if s.view.state != SessionState::Stopped {
                s.view.state = state;
                s.view.detail = detail.to_string();
            }
            s.view.exit_code = exit_code;
        });
    }

    /// The last `n` captured output lines of a headless session.
    pub fn tail(&self, id: &str, n: usize) -> Result<Vec<String>, String> {
        self.with(id, |s| {
            let skip = s.lines.len().saturating_sub(n);
            s.lines.iter().skip(skip).cloned().collect()
        })
        .ok_or_else(|| format!("no agent session {id}"))
    }

    /// Stop a headless session (kill its process). Terminal sessions are stopped by the desktop.
    pub fn stop(&self, id: &str) -> Result<SessionView, String> {
        let (shared, child) = {
            let entries = lock(&self.entries);
            let e = entries
                .iter()
                .find(|e| lock(&e.shared).view.id == id)
                .ok_or_else(|| format!("no agent session {id}"))?;
            (Arc::clone(&e.shared), e.child.clone())
        };
        let child = child.ok_or("a terminal session is stopped through the terminal workspace")?;
        {
            let mut s = lock(&shared);
            if s.view.state != SessionState::Running {
                return Ok(s.view.clone());
            }
            s.view.state = SessionState::Stopped;
            s.view.detail = "stopped from EAK".into();
        }
        lock(&child)
            .kill()
            .map_err(|e| format!("could not stop the session: {e}"))?;
        let view = lock(&shared).view.clone();
        Ok(view)
    }

    /// Mark a session stopped (the desktop closed its terminal tab).
    pub fn mark_stopped(&self, id: &str) {
        self.with(id, |s| {
            s.view.state = SessionState::Stopped;
            s.view.detail = "stopped from EAK".into();
        });
    }

    fn with<T>(&self, id: &str, f: impl FnOnce(&mut Shared) -> T) -> Option<T> {
        let entries = lock(&self.entries);
        let e = entries.iter().find(|e| lock(&e.shared).view.id == id)?;
        let mut s = lock(&e.shared);
        Some(f(&mut s))
    }
}

/// Copy a stream line by line into the log, the in-memory tail and (stdout) the event list.
fn pump<R: Read + Send + 'static>(
    r: R,
    prefix: &'static str,
    parser: Option<Parser>,
    shared: Arc<Mutex<Shared>>,
    writer: Option<Arc<Mutex<File>>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        for line in BufReader::new(r).lines() {
            let Ok(line) = line else { break };
            if let Some(w) = &writer {
                let _ = writeln!(lock(w), "{prefix}{line}");
            }
            let event = parser.and_then(|p| p(&line));
            let mut s = lock(&shared);
            s.lines.push_back(format!("{prefix}{line}"));
            if s.lines.len() > KEEP_LINES {
                s.lines.pop_front();
            }
            if let Some(e) = event {
                s.view.events.push(e);
                if s.view.events.len() > KEEP_EVENTS {
                    s.view.events.remove(0);
                }
            }
        }
    })
}

/// Did the session's own stream report a failed result?
fn reported_failure(events: &[AgentEvent]) -> bool {
    events
        .iter()
        .rev()
        .find_map(|e| match e {
            AgentEvent::Finished { ok, .. } => Some(!ok),
            _ => None,
        })
        .unwrap_or(false)
}

fn wait_for_exit(
    child: Arc<Mutex<Child>>,
    shared: Arc<Mutex<Shared>>,
    readers: Vec<JoinHandle<()>>,
) {
    std::thread::spawn(move || loop {
        let status = lock(&child).try_wait();
        match status {
            Ok(Some(st)) => {
                // Let the readers take the last lines (the final result event) first; a
                // grandchild holding the pipes open must not stall the state, so wait briefly.
                let until = Instant::now() + Duration::from_secs(3);
                while readers.iter().any(|r| !r.is_finished()) && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(20));
                }
                let mut s = lock(&shared);
                s.view.exit_code = st.code();
                if s.view.state == SessionState::Running {
                    if st.success() && !reported_failure(&s.view.events) {
                        s.view.state = SessionState::Completed;
                        s.view.detail = "finished".into();
                    } else {
                        s.view.state = SessionState::Failed;
                        s.view.detail = match st.code() {
                            Some(c) => format!("exited with code {c}"),
                            None => "ended by a signal".into(),
                        };
                    }
                }
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => {
                let mut s = lock(&shared);
                s.view.state = SessionState::Failed;
                s.view.detail = format!("lost track of the process: {e}");
                break;
            }
        }
    });
}
