//! Agent sessions started from EAK: headless child processes, whose output is captured to a log
//! file and parsed into events, and terminal tabs (the desktop owns those PTYs and records
//! them here so one list shows every session).
//!
//! Durable (ADR-0058): every state change is appended to an index in the log folder,
//! [`INDEX_FILE`], and the next app run replays it. A session that was still running when EAK
//! closed comes back as [`SessionState::Unknown`]: EAK no longer supervises its process and does
//! not know how it ended, so it never reports it as finished.

use crate::{AgentEvent, LaunchPlan, Parser, Surface};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Output lines kept in memory per session (the log file has all of them).
const KEEP_LINES: usize = 2000;
/// Parsed events kept per session.
const KEEP_EVENTS: usize = 200;

/// The session index in the log folder: one JSON [`SessionView`] per line, appended on every
/// state change; the last line of an id is its state.
pub const INDEX_FILE: &str = "sessions.jsonl";

/// Detail of a session restored as [`SessionState::Unknown`].
pub const RESTORED_RUNNING: &str =
    "EAK closed while this session was running; it is not supervised any more and its outcome was not recorded";
/// Why a session of an earlier app run cannot be stopped.
pub const NOT_SUPERVISED: &str =
    "started before EAK last closed: EAK no longer supervises its process, so it cannot stop it";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Running,
    /// The process exited with code 0 (and its stream did not report a failure).
    Completed,
    Failed,
    /// Stopped from EAK.
    Stopped,
    /// Recorded as running when EAK last closed: no longer supervised, outcome not known.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// The session's WEFT agent id, `agent:<EAK_AGENT_NAME>@<EAK_SESSION_ID>` (the journal
    /// actor label), when the launch plan names the agent.
    #[serde(default)]
    pub weft_id: Option<String>,
    /// Restored from the index of an earlier app run: it has no process, tab or output here.
    #[serde(default)]
    pub from_earlier_run: bool,
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

/// The append-only session index ([`INDEX_FILE`]).
struct Index {
    path: PathBuf,
    /// Serialises appends from the API and the exit watchers.
    write: Mutex<()>,
}

impl Index {
    /// Append one record. A failed write costs only durability (the next run would not see this
    /// state); the session itself is unaffected, so it is not an error for the caller.
    fn append(&self, view: &SessionView) {
        let Ok(mut line) = serde_json::to_string(view) else {
            return;
        };
        line.push('\n');
        let _w = lock(&self.write);
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .and_then(|mut f| f.write_all(line.as_bytes()));
    }
}

/// Every session recorded in an index file, oldest first, each as its last record. A line that
/// does not parse is skipped, never guessed at.
pub fn read_index(path: &Path) -> Vec<SessionView> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut order: Vec<String> = Vec::new();
    let mut last: HashMap<String, SessionView> = HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<SessionView>(line) else {
            continue;
        };
        if !last.contains_key(&v.id) {
            order.push(v.id.clone());
        }
        last.insert(v.id.clone(), v);
    }
    order
        .into_iter()
        .filter_map(|id| last.remove(&id))
        .collect()
}

/// Every session started from EAK: this app run's, and those restored from the index.
pub struct Sessions {
    entries: Mutex<Vec<Entry>>,
    counter: Mutex<u64>,
    log_dir: Option<PathBuf>,
    index: Option<Arc<Index>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// The WEFT agent id of a session: the journal actor label `agent:<name>@<session>` that
/// `eak_runtime::Actor::agent_in_session(..).label()` gives (the name is lower-cased there too).
fn weft_label(plan: &LaunchPlan, session: &str) -> Option<String> {
    plan.env
        .iter()
        .find(|(k, _)| k == "EAK_AGENT_NAME")
        .map(|(_, name)| format!("agent:{}@{session}", name.to_lowercase()))
}

fn plan_session_id(plan: &LaunchPlan) -> Option<&str> {
    plan.env
        .iter()
        .find(|(k, _)| k == "EAK_SESSION_ID")
        .map(|(_, v)| v.as_str())
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
        weft_id: None,
        from_earlier_run: false,
    }
}

impl Sessions {
    /// Headless logs go to `log_dir/<id>.log` (the app data folder, never a project folder), and
    /// the session index to `log_dir/sessions.jsonl`. The sessions of earlier runs are restored
    /// from it: without their process or terminal tab (that tab id belongs to the earlier run and
    /// is never used again), and a session recorded as running becomes `Unknown`.
    pub fn new(log_dir: Option<PathBuf>) -> Sessions {
        let index = log_dir.as_ref().map(|d| {
            Arc::new(Index {
                path: d.join(INDEX_FILE),
                write: Mutex::new(()),
            })
        });
        let mut entries = Vec::new();
        if let Some(ix) = &index {
            for mut view in read_index(&ix.path) {
                view.from_earlier_run = true;
                view.terminal_session = None;
                view.pid = None;
                if view.state == SessionState::Running {
                    view.state = SessionState::Unknown;
                    view.detail = RESTORED_RUNNING.into();
                    ix.append(&view);
                }
                entries.push(Entry {
                    shared: Arc::new(Mutex::new(Shared {
                        view,
                        lines: VecDeque::new(),
                    })),
                    child: None,
                });
            }
        }
        Sessions {
            entries: Mutex::new(entries),
            counter: Mutex::new(0),
            log_dir,
            index,
        }
    }

    fn record(&self, view: &SessionView) {
        if let Some(ix) = &self.index {
            ix.append(view);
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
        view.weft_id = plan_session_id(start.plan).and_then(|s| weft_label(start.plan, s));
        self.record(&view);
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
        wait_for_exit(
            Arc::clone(&child),
            Arc::clone(&shared),
            readers,
            self.index.clone(),
        );
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
        // The tab exports its own terminal session id as `EAK_SESSION_ID`.
        view.weft_id = weft_label(start.plan, &terminal_session.to_string());
        self.record(&view);
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

    /// Update a terminal session from the terminal engine's status. A session of an earlier run
    /// has no terminal here and is left as restored. The index records a change of state or exit
    /// code (not every change of the detail text).
    pub fn set_state(&self, id: &str, state: SessionState, detail: &str, exit_code: Option<i32>) {
        let index = self.index.clone();
        self.with(id, |s| {
            if s.view.from_earlier_run {
                return;
            }
            let before = (s.view.state, s.view.exit_code);
            if s.view.state != SessionState::Stopped {
                s.view.state = state;
                s.view.detail = detail.to_string();
            }
            s.view.exit_code = exit_code;
            if before != (s.view.state, s.view.exit_code) {
                if let Some(ix) = &index {
                    ix.append(&s.view);
                }
            }
        });
    }

    /// The last `n` output lines of a headless session: captured in memory in this run, read
    /// back from its log file for a session of an earlier run.
    pub fn tail(&self, id: &str, n: usize) -> Result<Vec<String>, String> {
        let (lines, earlier, log) = self
            .with(id, |s| {
                let skip = s.lines.len().saturating_sub(n);
                let lines: Vec<String> = s.lines.iter().skip(skip).cloned().collect();
                (lines, s.view.from_earlier_run, s.view.log_file.clone())
            })
            .ok_or_else(|| format!("no agent session {id}"))?;
        if !earlier {
            return Ok(lines);
        }
        let log = log.ok_or(
            "this session ran before EAK last closed and has no log file: a terminal tab's output is not kept after EAK closes",
        )?;
        let text = std::fs::read_to_string(&log)
            .map_err(|e| format!("could not read the session log {log}: {e}"))?;
        let all: Vec<&str> = text.lines().collect();
        Ok(all[all.len().saturating_sub(n)..]
            .iter()
            .map(|l| l.to_string())
            .collect())
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
        if lock(&shared).view.from_earlier_run {
            return Err(NOT_SUPERVISED.into());
        }
        let child = child.ok_or("a terminal session is stopped through the terminal workspace")?;
        {
            let mut s = lock(&shared);
            if s.view.state != SessionState::Running {
                return Ok(s.view.clone());
            }
            s.view.state = SessionState::Stopped;
            s.view.detail = "stopped from EAK".into();
            self.record(&s.view);
        }
        lock(&child)
            .kill()
            .map_err(|e| format!("could not stop the session: {e}"))?;
        let view = lock(&shared).view.clone();
        Ok(view)
    }

    /// Mark a session stopped (the desktop closed its terminal tab).
    pub fn mark_stopped(&self, id: &str) {
        let index = self.index.clone();
        self.with(id, |s| {
            if s.view.from_earlier_run {
                return;
            }
            s.view.state = SessionState::Stopped;
            s.view.detail = "stopped from EAK".into();
            if let Some(ix) = &index {
                ix.append(&s.view);
            }
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
    index: Option<Arc<Index>>,
) {
    let record = move |view: &SessionView| {
        if let Some(ix) = &index {
            ix.append(view);
        }
    };
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
                record(&s.view);
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => {
                let mut s = lock(&shared);
                s.view.state = SessionState::Failed;
                s.view.detail = format!("lost track of the process: {e}");
                record(&s.view);
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("eak-agents-index-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn plan(dir: &Path) -> LaunchPlan {
        LaunchPlan {
            argv: vec!["claude".into()],
            env: vec![("EAK_AGENT_NAME".into(), "claude-code".into())],
            cwd: dir.to_path_buf(),
            surface: Surface::Terminal,
        }
    }

    fn start<'a>(id: &str, plan: &'a LaunchPlan) -> Start<'a> {
        Start {
            id: id.into(),
            tool: "claude",
            label: "Claude Code",
            plan,
            prompt: Some("check the rails"),
        }
    }

    fn index_lines(dir: &Path) -> usize {
        std::fs::read_to_string(dir.join(INDEX_FILE))
            .unwrap()
            .lines()
            .count()
    }

    #[test]
    fn sessions_survive_a_restart_and_a_running_one_comes_back_unknown() {
        let dir = tmpdir("restart");
        let p = plan(&dir);
        {
            let s = Sessions::new(Some(dir.clone()));
            let a = s.register_terminal(start("claude-1-1", &p), 7, Some(4242));
            assert_eq!(a.weft_id.as_deref(), Some("agent:claude-code@7"));
            s.register_terminal(start("claude-1-2", &p), 8, None);
            s.mark_stopped("claude-1-2");
            // A detail-only change of a running tab is not a new record.
            s.set_state("claude-1-1", SessionState::Running, "idle", None);
        }
        assert_eq!(index_lines(&dir), 3);

        let s = Sessions::new(Some(dir.clone()));
        let listed = s.list();
        assert_eq!(listed.len(), 2);
        let a = s.get("claude-1-1").unwrap();
        assert_eq!(a.state, SessionState::Unknown);
        assert_eq!(a.detail, RESTORED_RUNNING);
        assert!(a.from_earlier_run);
        assert_eq!(a.terminal_session, None, "the old tab id is never reused");
        assert_eq!(a.pid, None);
        assert_eq!(a.prompt.as_deref(), Some("check the rails"));
        assert_eq!(a.weft_id.as_deref(), Some("agent:claude-code@7"));
        let b = s.get("claude-1-2").unwrap();
        assert_eq!(b.state, SessionState::Stopped, "a finished state is kept");
        assert!(b.from_earlier_run);

        let e = s.stop("claude-1-1").unwrap_err();
        assert_eq!(e, NOT_SUPERVISED);
        let e = s.tail("claude-1-1", 10).unwrap_err();
        assert!(e.contains("not kept after EAK closes"), "{e}");
        // Restored sessions ignore terminal updates (that tab id now means another tab).
        s.set_state("claude-1-1", SessionState::Completed, "done", Some(0));
        s.mark_stopped("claude-1-1");
        assert_eq!(s.get("claude-1-1").unwrap().state, SessionState::Unknown);
        // Only the reconciliation of the running session was appended.
        assert_eq!(index_lines(&dir), 4);
        drop(s);

        // A third run reads the same states and appends nothing.
        let s = Sessions::new(Some(dir.clone()));
        assert_eq!(s.get("claude-1-1").unwrap().state, SessionState::Unknown);
        assert_eq!(index_lines(&dir), 4);
    }

    #[test]
    fn a_restored_headless_session_reads_its_log_file_and_bad_lines_are_skipped() {
        let dir = tmpdir("headless");
        let log = dir.join("codex-1-1.log");
        std::fs::write(&log, "one\ntwo\nthree\n").unwrap();
        let view = SessionView {
            id: "codex-1-1".into(),
            tool: "codex".into(),
            label: "Codex".into(),
            surface: Surface::Headless,
            project: dir.display().to_string(),
            prompt: Some("review".into()),
            started_at: 1,
            state: SessionState::Completed,
            detail: "finished".into(),
            exit_code: Some(0),
            pid: Some(1),
            terminal_session: None,
            log_file: Some(log.display().to_string()),
            events: vec![AgentEvent::Finished {
                ok: true,
                summary: "done".into(),
            }],
            weft_id: Some("agent:codex@codex-1-1".into()),
            from_earlier_run: false,
        };
        let text = format!("{}\nnot json\n", serde_json::to_string(&view).unwrap());
        std::fs::write(dir.join(INDEX_FILE), text).unwrap();
        let s = Sessions::new(Some(dir.clone()));
        let v = s.get("codex-1-1").unwrap();
        assert_eq!(v.state, SessionState::Completed);
        assert_eq!(v.events, view.events);
        assert_eq!(s.tail("codex-1-1", 2).unwrap(), ["two", "three"]);
        assert_eq!(read_index(&dir.join(INDEX_FILE)).len(), 1);
    }

    #[test]
    fn without_a_log_folder_nothing_is_recorded() {
        let s = Sessions::new(None);
        let dir = tmpdir("none");
        let p = plan(&dir);
        s.register_terminal(start("claude-2-1", &p), 1, None);
        assert!(!dir.join(INDEX_FILE).exists());
        assert_eq!(s.list().len(), 1);
    }
}
