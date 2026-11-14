//! # eak-terminal — the terminal workspace engine
//!
//! Real operating-system pseudo-terminals (via `portable-pty`: Unix PTYs, Windows ConPTY),
//! owned by one long-lived [`Manager`] so sessions outlive any UI that shows them: switching
//! workspaces, reloading the window or opening another project never stops or re-points a
//! running process.
//!
//! Status ([`Status`]) is derived from the process first (alive / exit code, and on Unix the
//! terminal's foreground process group), then from reports sent by integrated agents through
//! the control API, and only last from output heuristics, which are always labelled
//! [`Source::Heuristic`]. See `docs/terminal-api.md` and ADR-0019.
//!
//! What this does **not** do: a session never survives an app or machine restart. Sessions
//! that were open when the app last exited are listed again as [`State::Ended`] so the user
//! can restart them deliberately; their old processes are gone.

mod agents;
pub mod api;
mod proc_info;
mod text;

pub use text::strip_ansi;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Bytes of raw output kept per session for reattach, `read` and search.
pub const SCROLLBACK_BYTES: usize = 2 * 1024 * 1024;
/// Output silence after which a foreground job counts as quiet.
const QUIET: Duration = Duration::from_millis(1500);
/// Commands kept per session in its run history.
const HISTORY: usize = 200;
/// Quiet time before `wait --until idle` trusts an idle state.
const SETTLE: Duration = Duration::from_millis(400);

pub type SessionId = u64;
pub type TabId = u64;
pub type SpaceId = u64;

/// A space: a named working context bound to one root directory (a project, a worktree,
/// any folder). New sessions in a space start in its root; spaces never share sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Space {
    pub id: SpaceId,
    pub name: String,
    pub root: PathBuf,
    /// EAK project id when the space was opened for an EAK project.
    #[serde(default)]
    pub project_id: Option<String>,
}

/// What a session runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Program {
    /// An interactive shell; `None` uses the platform default ([`default_shell`]).
    Shell { path: Option<String> },
    /// A program run directly in the terminal (no shell around it).
    Command { argv: Vec<String> },
}

/// Everything needed to (re)start a session. The working directory is fixed at creation:
/// a session belongs to the project it was opened in for its whole life.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    pub program: Program,
    pub cwd: PathBuf,
    /// Display name of the project the session was opened in, if any.
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

/// Session lifecycle state shown to users and returned by the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// A job (or the session's own program) is producing output or working.
    Running,
    /// A shell sits at its prompt: the shell itself is the terminal's foreground process.
    Idle,
    /// The foreground process is blocked reading the terminal (or looks like it is; see
    /// [`Status::source`]).
    WaitingForInput,
    /// An integrated agent reported that it needs a human decision.
    Blocked,
    /// The process exited with code 0.
    Completed,
    /// The process exited non-zero, was killed by a signal, or could not start.
    Failed,
    /// The session was open when EAK last exited. Its process is gone; restart it.
    Ended,
}

/// Where a status came from, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Process facts: alive/exited, exit code, foreground process group, kernel wait state.
    Process,
    /// A report from an agent integration through the control API.
    Integration,
    /// A guess from output timing or text. Never authoritative.
    Heuristic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    pub state: State,
    pub source: Source,
    /// Human explanation, e.g. "foreground: cargo" or "exit code 2".
    pub detail: String,
    /// Name of the terminal's foreground process when the OS exposes it.
    pub foreground: Option<String>,
    /// Known coding agent running in the foreground (from its command line, or as named by
    /// its integration report).
    pub agent: Option<String>,
    pub exit_code: Option<i64>,
}

/// A state reported by an agent integration (`session.report_state`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub source: String,
    pub state: ReportedState,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportedState {
    Idle,
    Working,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Panes side by side.
    #[default]
    Row,
    /// Panes stacked.
    Column,
}

/// A tab: one or more panes, each showing one session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tab {
    pub id: TabId,
    pub space: SpaceId,
    pub direction: Direction,
    pub panes: Vec<SessionId>,
}

/// Public view of a session.
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub name: String,
    pub space: SpaceId,
    pub tab: TabId,
    pub spec: Spec,
    pub pid: Option<u32>,
    pub status: Status,
    /// Total output bytes ever produced (a cursor for `read_since`).
    pub output_seq: u64,
    pub history: Vec<String>,
    pub report: Option<Report>,
    /// Unix seconds.
    pub started_at: u64,
}

/// Tabs, panes and focus: enough for any UI (or the API) to show the same layout.
#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub spaces: Vec<Space>,
    /// In display order (tabs of different spaces interleave; filter by `space`).
    pub tabs: Vec<Tab>,
    pub sessions: Vec<SessionInfo>,
    pub focused: Option<SessionId>,
    /// Increases whenever tabs, panes, focus or names change.
    pub layout_seq: u64,
}

/// Output pushed to the UI as it arrives. `data` is always whole UTF-8 characters.
#[derive(Debug, Clone, Serialize)]
pub struct OutputEvent {
    pub id: SessionId,
    pub data: String,
    /// Output cursor after this chunk (compare with `scrollback`'s cursor on reattach).
    pub seq: u64,
}

pub type OutputSink = Arc<dyn Fn(OutputEvent) + Send + Sync>;

/// What `wait` waits for.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "until", rename_all = "snake_case")]
pub enum WaitFor {
    /// The process exits.
    Exit,
    /// New output arrives and the session then reaches `idle` (shell back at its prompt),
    /// `waiting_for_input`, `blocked`, or exits.
    Idle,
    /// Output produced after the call contains `pattern` (ANSI codes stripped).
    Output { pattern: String },
}

struct Live {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
}

#[derive(Clone, Copy)]
struct Exit {
    code: i64,
    signal: bool,
}

struct Session {
    name: String,
    space: SpaceId,
    tab: TabId,
    spec: Spec,
    live: Option<Live>,
    exit: Option<Exit>,
    spawn_error: Option<String>,
    ended: bool,
    /// Bumped on every (re)start so output from a replaced process is ignored.
    generation: u64,
    scroll: VecDeque<u8>,
    seq: u64,
    pending_utf8: Vec<u8>,
    last_output: Instant,
    history: Vec<String>,
    report: Option<Report>,
    started_at: u64,
    size: PtySize,
}

struct Inner {
    spaces: Vec<Space>,
    sessions: BTreeMap<SessionId, Session>,
    tabs: Vec<Tab>,
    focused: Option<SessionId>,
    next_id: u64,
    layout_seq: u64,
    /// Directory and project name new sessions start in when the caller gives none.
    context: Option<(PathBuf, Option<String>, Option<String>)>,
    /// Executable names recognised as coding agents.
    agents: Vec<String>,
}

/// Owner of every terminal session in the process.
pub struct Manager {
    inner: Mutex<Inner>,
    changed: Condvar,
    sink: OutputSink,
    manifest: Option<PathBuf>,
    /// Extra environment for every session (e.g. the control-API location).
    base_env: Mutex<Vec<(String, String)>>,
}

/// Options for [`Manager::create`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CreateOptions {
    /// Space to open the session in; defaults to the focused session's space, then the
    /// active project's space (created on demand).
    #[serde(default)]
    pub space: Option<SpaceId>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    /// Shell path; ignored when `command` is set.
    #[serde(default)]
    pub shell: Option<String>,
    /// Run this program directly instead of a shell.
    #[serde(default)]
    pub command: Option<Vec<String>>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
    /// Put the new session in a pane next to this one instead of a new tab.
    #[serde(default)]
    pub split_of: Option<SessionId>,
    #[serde(default)]
    pub direction: Option<Direction>,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
}

/// Saved layout: spaces, their tabs and panes, and each session's name and program. The
/// processes themselves are never saved — they cannot outlive the app.
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    spaces: Vec<SavedSpace>,
}

#[derive(Serialize, Deserialize)]
struct SavedSpace {
    name: String,
    root: PathBuf,
    #[serde(default)]
    project_id: Option<String>,
    tabs: Vec<SavedTab>,
}

#[derive(Serialize, Deserialize)]
struct SavedTab {
    direction: Direction,
    panes: Vec<SavedPane>,
}

#[derive(Serialize, Deserialize)]
struct SavedPane {
    name: String,
    spec: Spec,
}

const MANIFEST_VERSION: u32 = 2;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The platform's default interactive shell.
pub fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/sh".into())
    }
}

/// Shells installed on this machine, default first.
pub fn available_shells() -> Vec<String> {
    let mut out = vec![default_shell()];
    #[cfg(windows)]
    let candidates: Vec<String> = ["pwsh.exe", "powershell.exe", "cmd.exe", "bash.exe"]
        .iter()
        .filter_map(|s| find_on_path(s).map(|p| p.display().to_string()))
        .collect();
    #[cfg(not(windows))]
    let candidates: Vec<String> = std::fs::read_to_string("/etc/shells")
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('/') && Path::new(l).exists())
        .map(String::from)
        .collect();
    let canon = |p: &str| std::fs::canonicalize(p).unwrap_or_else(|_| PathBuf::from(p));
    for c in candidates {
        if !out.iter().any(|o| o == &c || canon(o) == canon(&c)) {
            out.push(c);
        }
    }
    out
}

/// Look a program up on `PATH` (with `PATHEXT` on Windows).
pub fn find_on_path(program: &str) -> Option<PathBuf> {
    let p = Path::new(program);
    if p.components().count() > 1 {
        return p.is_file().then(|| p.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    #[cfg(windows)]
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
        .split(';')
        .map(|e| e.to_ascii_lowercase())
        .chain(std::iter::once(String::new()))
        .collect();
    #[cfg(not(windows))]
    let exts = [String::new()];
    for dir in std::env::split_paths(&path) {
        for ext in &exts {
            let cand = dir.join(format!("{program}{ext}"));
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}

/// Decode as much of `buf` as forms whole UTF-8 characters, keeping an incomplete trailing
/// sequence for the next chunk. Invalid bytes become U+FFFD.
fn take_utf8(buf: &mut Vec<u8>) -> String {
    let mut out = String::new();
    let mut rest: &[u8] = buf;
    loop {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                out.push_str(s);
                rest = &[];
                break;
            }
            Err(e) => {
                let (ok, tail) = rest.split_at(e.valid_up_to());
                out.push_str(std::str::from_utf8(ok).unwrap_or_default());
                match e.error_len() {
                    Some(n) => {
                        out.push('\u{FFFD}');
                        rest = &tail[n..];
                    }
                    None => {
                        rest = tail;
                        break;
                    }
                }
            }
        }
    }
    *buf = rest.to_vec();
    out
}

impl Session {
    fn push_output(&mut self, bytes: &[u8]) -> String {
        self.seq += bytes.len() as u64;
        self.last_output = Instant::now();
        self.scroll.extend(bytes);
        if self.scroll.len() > SCROLLBACK_BYTES {
            let drop = self.scroll.len() - SCROLLBACK_BYTES;
            self.scroll.drain(..drop);
            // Never start the buffer in the middle of a UTF-8 character.
            while self
                .scroll
                .front()
                .is_some_and(|b| (0x80..0xC0).contains(b))
            {
                self.scroll.pop_front();
            }
        }
        self.pending_utf8.extend_from_slice(bytes);
        take_utf8(&mut self.pending_utf8)
    }

    fn tail(&self, n: usize) -> Vec<u8> {
        let n = n.min(self.scroll.len());
        self.scroll
            .iter()
            .skip(self.scroll.len() - n)
            .copied()
            .collect()
    }

    fn scrollback(&self) -> String {
        String::from_utf8_lossy(&self.tail(self.scroll.len())).into_owned()
    }

    fn is_shell(&self) -> bool {
        matches!(self.spec.program, Program::Shell { .. })
    }

    fn status(&self, agents: &[String]) -> Status {
        let st = |state, source, detail: String| Status {
            state,
            source,
            detail,
            foreground: None,
            agent: None,
            exit_code: None,
        };
        if self.ended {
            return st(
                State::Ended,
                Source::Process,
                "process ended when EAK last exited; restart to run it again".into(),
            );
        }
        if let Some(err) = &self.spawn_error {
            return st(
                State::Failed,
                Source::Process,
                format!("could not start: {err}"),
            );
        }
        if let Some(exit) = self.exit {
            let ok = exit.code == 0 && !exit.signal;
            let detail = if exit.signal {
                "terminated by a signal".to_string()
            } else {
                format!("exit code {}", exit.code)
            };
            return Status {
                exit_code: Some(exit.code),
                ..st(
                    if ok { State::Completed } else { State::Failed },
                    Source::Process,
                    detail,
                )
            };
        }
        let Some(live) = &self.live else {
            return st(State::Failed, Source::Process, "no process".into());
        };
        let fg_pid = foreground_pid(live);
        let fg_name = fg_pid.and_then(proc_info::name);
        let shell_at_prompt = self.is_shell() && fg_pid.is_some() && fg_pid == live.pid;
        let quiet = self.last_output.elapsed() >= QUIET;
        // Which agent is in front: its own report names it best, then its command line.
        let agent = match (&self.report, shell_at_prompt) {
            (Some(r), false) => Some(r.agent.clone().unwrap_or_else(|| r.source.clone())),
            _ if shell_at_prompt => None,
            _ => fg_pid
                .or(live.pid)
                .and_then(|p| proc_info::agent(p, agents)),
        };
        let with_fg = |s: Status| Status {
            foreground: fg_name.clone(),
            agent: agent.clone(),
            ..s
        };
        // An agent integration speaks for its own process while that process runs; once
        // the shell is back in the foreground the report is stale and ignored.
        if let (Some(r), false) = (&self.report, shell_at_prompt) {
            let state = match r.state {
                ReportedState::Idle => State::Idle,
                ReportedState::Working => State::Running,
                ReportedState::Blocked => State::Blocked,
            };
            let who = r.agent.clone().unwrap_or_else(|| r.source.clone());
            let detail = match &r.message {
                Some(m) => format!("{who}: {m}"),
                None => format!("reported by {who}"),
            };
            return with_fg(st(state, Source::Integration, detail));
        }
        if shell_at_prompt {
            return with_fg(st(
                State::Idle,
                Source::Process,
                "shell at its prompt".into(),
            ));
        }
        if let Some(pid) = fg_pid {
            if quiet && proc_info::reading_terminal(pid) {
                return with_fg(st(
                    State::WaitingForInput,
                    Source::Process,
                    "foreground process is blocked reading the terminal".into(),
                ));
            }
        }
        if quiet && text::looks_like_prompt(&self.last_line()) {
            return with_fg(st(
                State::WaitingForInput,
                Source::Heuristic,
                "output stopped at what looks like a question".into(),
            ));
        }
        if fg_pid.is_none() && quiet && self.is_shell() {
            // Windows ConPTY exposes no foreground process group.
            return with_fg(st(
                State::Idle,
                Source::Heuristic,
                "no output for a while (foreground process unknown on this OS)".into(),
            ));
        }
        let detail = match &fg_name {
            Some(n) => format!("foreground: {n}"),
            None => "process running".into(),
        };
        with_fg(st(State::Running, Source::Process, detail))
    }

    fn last_line(&self) -> String {
        let text = strip_ansi(&String::from_utf8_lossy(&self.tail(512)));
        text.lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .to_string()
    }

    fn info(&self, id: SessionId, agents: &[String]) -> SessionInfo {
        SessionInfo {
            id,
            name: self.name.clone(),
            space: self.space,
            tab: self.tab,
            spec: self.spec.clone(),
            pid: self.live.as_ref().and_then(|l| l.pid),
            status: self.status(agents),
            output_seq: self.seq,
            history: self.history.clone(),
            report: self.report.clone(),
            started_at: self.started_at,
        }
    }
}

#[cfg(unix)]
fn foreground_pid(live: &Live) -> Option<u32> {
    live.master
        .process_group_leader()
        .and_then(|p| u32::try_from(p).ok())
}

#[cfg(not(unix))]
fn foreground_pid(_live: &Live) -> Option<u32> {
    None
}

impl Manager {
    /// A manager with no saved layout.
    pub fn new(sink: OutputSink) -> Arc<Manager> {
        Self::build(sink, None)
    }

    /// A manager that saves spaces, tabs and session metadata to `manifest` and, on start,
    /// lists the sessions left over from the previous run as [`State::Ended`].
    pub fn with_manifest(sink: OutputSink, manifest: PathBuf) -> Arc<Manager> {
        Self::build(sink, Some(manifest))
    }

    fn build(sink: OutputSink, manifest: Option<PathBuf>) -> Arc<Manager> {
        let m = Arc::new(Manager {
            inner: Mutex::new(Inner {
                spaces: Vec::new(),
                sessions: BTreeMap::new(),
                tabs: Vec::new(),
                focused: None,
                next_id: 1,
                layout_seq: 0,
                context: None,
                agents: KNOWN_AGENTS.iter().map(|s| s.to_string()).collect(),
            }),
            changed: Condvar::new(),
            sink,
            manifest,
            base_env: Mutex::new(Vec::new()),
        });
        m.restore_manifest();
        m
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic while holding the lock leaves plain data behind; keep serving it.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn bump(&self, g: &mut Inner) {
        g.layout_seq += 1;
        self.save_manifest(g);
    }

    fn restore_manifest(&self) {
        let Some(path) = &self.manifest else { return };
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let saved: Manifest = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                eprintln!(
                    "eak-terminal: saved layout ignored ({}): {e}",
                    path.display()
                );
                return;
            }
        };
        if saved.version != MANIFEST_VERSION {
            return;
        }
        let mut g = self.lock();
        for sp in saved.spaces {
            let space = g.alloc();
            g.spaces.push(Space {
                id: space,
                name: sp.name,
                root: sp.root,
                project_id: sp.project_id,
            });
            for t in sp.tabs {
                let tab = g.alloc();
                let mut panes = Vec::new();
                for p in t.panes {
                    let id = g.alloc();
                    let mut s = new_session(p.name, space, tab, p.spec);
                    s.ended = true;
                    g.sessions.insert(id, s);
                    panes.push(id);
                }
                if !panes.is_empty() {
                    g.tabs.push(Tab {
                        id: tab,
                        space,
                        direction: t.direction,
                        panes,
                    });
                }
            }
        }
        g.focused = g.tabs.first().and_then(|t| t.panes.first().copied());
        g.layout_seq += 1;
    }

    fn save_manifest(&self, g: &Inner) {
        let Some(path) = &self.manifest else { return };
        let manifest = Manifest {
            version: MANIFEST_VERSION,
            spaces: g
                .spaces
                .iter()
                .map(|sp| SavedSpace {
                    name: sp.name.clone(),
                    root: sp.root.clone(),
                    project_id: sp.project_id.clone(),
                    tabs: g
                        .tabs
                        .iter()
                        .filter(|t| t.space == sp.id)
                        .map(|t| SavedTab {
                            direction: t.direction,
                            panes: t
                                .panes
                                .iter()
                                .filter_map(|p| g.sessions.get(p))
                                .map(|s| SavedPane {
                                    name: s.name.clone(),
                                    spec: s.spec.clone(),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
        };
        let write = || -> std::io::Result<()> {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = path.with_extension("tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(&manifest)?)?;
            std::fs::rename(&tmp, path)
        };
        if let Err(e) = write() {
            eprintln!("eak-terminal: layout not saved: {e}");
        }
    }

    /// Environment added to every session started from now on.
    pub fn set_base_env(&self, env: Vec<(String, String)>) {
        *self.base_env.lock().unwrap_or_else(|e| e.into_inner()) = env;
    }

    /// Extra agent executables to recognise (e.g. from a user's agents file), added to
    /// the built-in list. Integrations need no change to this crate.
    pub fn add_agents(&self, names: impl IntoIterator<Item = String>) {
        let mut g = self.lock();
        for n in names {
            if !n.trim().is_empty() && !g.agents.contains(&n) {
                g.agents.push(n);
            }
        }
    }

    /// The active project: used to create a space on demand when a session is opened with
    /// no space, and to bind spaces whose root is the project folder to the project id (so
    /// their sessions export `EAK_PROJECT_ID`). Existing sessions are never moved.
    pub fn set_context(
        &self,
        dir: Option<PathBuf>,
        project: Option<String>,
        project_id: Option<String>,
    ) {
        let mut g = self.lock();
        let mut bound = false;
        if let (Some(d), Some(pid)) = (&dir, &project_id) {
            for sp in g.spaces.iter_mut().filter(|s| &s.root == d) {
                if sp.project_id.as_ref() != Some(pid) {
                    sp.project_id = Some(pid.clone());
                    bound = true;
                }
            }
        }
        g.context = dir.map(|d| (d, project, project_id));
        if bound {
            self.bump(&mut g);
            drop(g);
            self.changed.notify_all();
        }
    }

    /// Create a space bound to `root` (and to the active project when `root` is its folder).
    pub fn create_space(&self, name: &str, root: PathBuf) -> Result<Space, String> {
        if !root.is_dir() {
            return Err(format!("space root is not a directory: {}", root.display()));
        }
        let mut g = self.lock();
        let name = match name.trim() {
            "" => root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string()),
            n => n.to_string(),
        };
        let project_id = match &g.context {
            Some((dir, _, pid)) if *dir == root => pid.clone(),
            _ => None,
        };
        let id = g.alloc();
        let sp = Space {
            id,
            name,
            root,
            project_id,
        };
        g.spaces.push(sp.clone());
        self.bump(&mut g);
        drop(g);
        self.changed.notify_all();
        Ok(sp)
    }

    pub fn rename_space(&self, id: SpaceId, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("name must not be empty".into());
        }
        let mut g = self.lock();
        g.spaces
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| no_space(id))?
            .name = name.to_string();
        self.bump(&mut g);
        Ok(())
    }

    /// Remove a space and close (terminate) every session in it.
    pub fn close_space(&self, id: SpaceId) -> Result<(), String> {
        let sessions: Vec<SessionId> = {
            let g = self.lock();
            if !g.spaces.iter().any(|s| s.id == id) {
                return Err(no_space(id));
            }
            g.sessions
                .iter()
                .filter(|(_, s)| s.space == id)
                .map(|(k, _)| *k)
                .collect()
        };
        for s in sessions {
            self.close(s)?;
        }
        let mut g = self.lock();
        g.spaces.retain(|s| s.id != id);
        self.bump(&mut g);
        drop(g);
        self.changed.notify_all();
        Ok(())
    }

    /// Move a tab to position `index` among its space's tabs.
    pub fn move_tab(&self, tab: TabId, index: usize) -> Result<(), String> {
        let mut g = self.lock();
        let from = g
            .tabs
            .iter()
            .position(|t| t.id == tab)
            .ok_or_else(|| format!("no tab {tab}"))?;
        let t = g.tabs.remove(from);
        let slots: Vec<usize> = g
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, x)| x.space == t.space)
            .map(|(i, _)| i)
            .collect();
        let at = match slots.get(index) {
            Some(i) => *i,
            None => slots.last().map(|i| i + 1).unwrap_or(g.tabs.len()),
        };
        g.tabs.insert(at, t);
        self.bump(&mut g);
        Ok(())
    }

    fn resolve_space(&self, g: &mut Inner, opts: &CreateOptions) -> Result<SpaceId, String> {
        if let Some(target) = opts.split_of {
            return g
                .sessions
                .get(&target)
                .map(|s| s.space)
                .ok_or_else(|| no_session(target));
        }
        if let Some(sp) = opts.space {
            return if g.spaces.iter().any(|s| s.id == sp) {
                Ok(sp)
            } else {
                Err(no_space(sp))
            };
        }
        // The active project's space, created on first use.
        let (root, name, project_id) = match &g.context {
            Some((dir, name, pid)) => (dir.clone(), name.clone(), pid.clone()),
            None => match g.focused.and_then(|f| g.sessions.get(&f)) {
                Some(s) => return Ok(s.space),
                None => (home_dir(), Some("Home".to_string()), None),
            },
        };
        if let Some(sp) = g.spaces.iter().find(|s| s.root == root) {
            return Ok(sp.id);
        }
        let id = g.alloc();
        let name = name.unwrap_or_else(|| {
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string())
        });
        g.spaces.push(Space {
            id,
            name,
            root,
            project_id,
        });
        Ok(id)
    }

    pub fn create(self: &Arc<Self>, opts: CreateOptions) -> Result<SessionInfo, String> {
        let mut g = self.lock();
        let space = self.resolve_space(&mut g, &opts)?;
        let sp = g
            .spaces
            .iter()
            .find(|s| s.id == space)
            .cloned()
            .ok_or_else(|| no_space(space))?;
        let cwd = opts.cwd.clone().unwrap_or_else(|| sp.root.clone());
        if !cwd.is_dir() {
            return Err(format!(
                "working directory does not exist: {}",
                cwd.display()
            ));
        }
        let program = match opts.command {
            Some(argv) if !argv.is_empty() => Program::Command { argv },
            Some(_) => return Err("command must not be empty".into()),
            None => Program::Shell { path: opts.shell },
        };
        let spec = Spec {
            program,
            cwd,
            project: Some(sp.name.clone()),
            env: {
                // Shared context contract with the engineering harness (docs/terminal-api.md).
                let mut env = vec![("EAK_PROJECT_DIR".to_string(), sp.root.display().to_string())];
                if let Some(pid) = &sp.project_id {
                    env.push(("EAK_PROJECT_ID".to_string(), pid.clone()));
                }
                env.extend(opts.env);
                env
            },
        };
        let id = g.alloc();
        let tab = match opts.split_of {
            Some(target) => {
                let t = g
                    .tabs
                    .iter_mut()
                    .find(|t| t.panes.contains(&target))
                    .ok_or_else(|| no_session(target))?;
                if t.panes.len() == 1 {
                    t.direction = opts.direction.unwrap_or_default();
                }
                let at = t.panes.iter().position(|p| *p == target).unwrap_or(0) + 1;
                t.panes.insert(at, id);
                t.id
            }
            None => {
                let tab = g.alloc();
                g.tabs.push(Tab {
                    id: tab,
                    space,
                    direction: Direction::Row,
                    panes: vec![id],
                });
                tab
            }
        };
        let name = opts.name.unwrap_or_else(|| default_name(&spec.program, id));
        let mut s = new_session(name, space, tab, spec);
        s.size = PtySize {
            rows: opts.rows.unwrap_or(24).max(2),
            cols: opts.cols.unwrap_or(80).max(10),
            pixel_width: 0,
            pixel_height: 0,
        };
        g.sessions.insert(id, s);
        self.start(&mut g, id);
        g.focused = Some(id);
        self.bump(&mut g);
        let info = g.sessions[&id].info(id, &g.agents);
        drop(g);
        self.changed.notify_all();
        Ok(info)
    }

    /// Spawn the session's process (first start or restart). Errors are recorded on the
    /// session as a `failed` status rather than returned, so a bad shell path is visible.
    fn start(self: &Arc<Self>, g: &mut Inner, id: SessionId) {
        let base_env = self
            .base_env
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let Some(s) = g.sessions.get_mut(&id) else {
            return;
        };
        s.generation += 1;
        s.exit = None;
        s.spawn_error = None;
        s.ended = false;
        s.report = None;
        s.pending_utf8.clear();
        s.started_at = now_secs();
        s.last_output = Instant::now();
        let generation = s.generation;
        match spawn(&s.spec, s.size, id, &base_env) {
            Ok((live, reader, child)) => {
                s.live = Some(live);
                let me = Arc::clone(self);
                std::thread::spawn(move || me.pump(id, generation, reader));
                let me = Arc::clone(self);
                std::thread::spawn(move || me.reap(id, generation, child));
            }
            Err(e) => {
                s.live = None;
                s.spawn_error = Some(e);
            }
        }
    }

    fn pump(&self, id: SessionId, generation: u64, mut reader: Box<dyn Read + Send>) {
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let (text, seq) = {
                let mut g = self.lock();
                match g.sessions.get_mut(&id) {
                    Some(s) if s.generation == generation => {
                        let t = s.push_output(&buf[..n]);
                        (t, s.seq)
                    }
                    _ => break,
                }
            };
            self.changed.notify_all();
            if !text.is_empty() {
                (self.sink)(OutputEvent {
                    id,
                    data: text,
                    seq,
                });
            }
        }
    }

    fn reap(
        &self,
        id: SessionId,
        generation: u64,
        mut child: Box<dyn portable_pty::Child + Send + Sync>,
    ) {
        let status = child.wait();
        {
            let mut g = self.lock();
            if let Some(s) = g.sessions.get_mut(&id) {
                if s.generation == generation {
                    s.exit = Some(match status {
                        Ok(st) => Exit {
                            code: i64::from(st.exit_code()),
                            signal: st.signal().is_some(),
                        },
                        Err(_) => Exit {
                            code: -1,
                            signal: false,
                        },
                    });
                    s.report = None;
                    // `live` stays so the last output can still drain; the master closes
                    // when the session is restarted or closed.
                }
            }
        }
        self.changed.notify_all();
    }

    /// Everything a UI needs to draw the workspace.
    pub fn overview(&self) -> Overview {
        let g = self.lock();
        Overview {
            spaces: g.spaces.clone(),
            tabs: g.tabs.clone(),
            sessions: g
                .sessions
                .iter()
                .map(|(id, s)| s.info(*id, &g.agents))
                .collect(),
            focused: g.focused,
            layout_seq: g.layout_seq,
        }
    }

    pub fn info(&self, id: SessionId) -> Result<SessionInfo, String> {
        let g = self.lock();
        g.sessions
            .get(&id)
            .map(|s| s.info(id, &g.agents))
            .ok_or_else(|| no_session(id))
    }

    /// Raw scrollback (with terminal escape codes) for redrawing a terminal view, and the
    /// output cursor it ends at.
    pub fn scrollback(&self, id: SessionId) -> Result<(String, u64), String> {
        let g = self.lock();
        let s = g.sessions.get(&id).ok_or_else(|| no_session(id))?;
        Ok((s.scrollback(), s.seq))
    }

    /// Plain-text output (escape codes removed), the last `lines` lines.
    pub fn read(&self, id: SessionId, lines: Option<usize>) -> Result<String, String> {
        let (raw, _) = self.scrollback(id)?;
        let text = strip_ansi(&raw);
        Ok(match lines {
            Some(n) => {
                let all: Vec<&str> = text.lines().collect();
                all[all.len().saturating_sub(n)..].join("\n")
            }
            None => text,
        })
    }

    /// Write bytes to the session's terminal, exactly as typed.
    pub fn write(&self, id: SessionId, data: &[u8]) -> Result<(), String> {
        let writer = {
            let g = self.lock();
            let s = g.sessions.get(&id).ok_or_else(|| no_session(id))?;
            if s.exit.is_some() || s.ended {
                return Err(format!("session {id} has no running process; restart it"));
            }
            let live = s
                .live
                .as_ref()
                .ok_or_else(|| format!("session {id} is not running"))?;
            Arc::clone(&live.writer)
        };
        // Written outside the manager lock: a full PTY buffer must not stall output.
        let mut w = writer.lock().unwrap_or_else(|e| e.into_inner());
        w.write_all(data)
            .and_then(|_| w.flush())
            .map_err(|e| e.to_string())
    }

    /// Type `command` followed by Enter, and remember it in the session's history.
    /// Returns the output cursor from before the command, for `wait(.., since)`.
    pub fn run(&self, id: SessionId, command: &str) -> Result<u64, String> {
        let seq = {
            let mut g = self.lock();
            let s = g.sessions.get_mut(&id).ok_or_else(|| no_session(id))?;
            if s.history.last().map(String::as_str) != Some(command) {
                s.history.push(command.to_string());
                if s.history.len() > HISTORY {
                    s.history.remove(0);
                }
            }
            s.seq
        };
        self.write(id, format!("{command}\r").as_bytes())?;
        Ok(seq)
    }

    pub fn resize(&self, id: SessionId, cols: u16, rows: u16) -> Result<(), String> {
        let mut g = self.lock();
        let s = g.sessions.get_mut(&id).ok_or_else(|| no_session(id))?;
        s.size = PtySize {
            rows: rows.max(2),
            cols: cols.max(10),
            pixel_width: 0,
            pixel_height: 0,
        };
        if let Some(live) = &s.live {
            live.master.resize(s.size).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn rename(&self, id: SessionId, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("name must not be empty".into());
        }
        let mut g = self.lock();
        g.sessions.get_mut(&id).ok_or_else(|| no_session(id))?.name = name.to_string();
        self.bump(&mut g);
        Ok(())
    }

    pub fn focus(&self, id: SessionId) -> Result<(), String> {
        let mut g = self.lock();
        if !g.sessions.contains_key(&id) {
            return Err(no_session(id));
        }
        g.focused = Some(id);
        g.layout_seq += 1;
        drop(g);
        self.changed.notify_all();
        Ok(())
    }

    /// Stop the current process (if any) and start the same program again in the same
    /// directory, keeping the scrollback with a marker line.
    pub fn restart(self: &Arc<Self>, id: SessionId) -> Result<SessionInfo, String> {
        let old = {
            let mut g = self.lock();
            let s = g.sessions.get_mut(&id).ok_or_else(|| no_session(id))?;
            // Detach the old process first so its last output is ignored.
            s.generation += 1;
            s.live.take()
        };
        // Hanging up may wait out a short grace period; never under the manager lock.
        if let Some(live) = old {
            hang_up(live);
        }
        let mut g = self.lock();
        let s = g.sessions.get_mut(&id).ok_or_else(|| no_session(id))?;
        s.pending_utf8.clear();
        let text = s.push_output(b"\r\n\x1b[2m--- restarted ---\x1b[0m\r\n");
        let seq = s.seq;
        self.start(&mut g, id);
        g.layout_seq += 1;
        let info = g.sessions[&id].info(id, &g.agents);
        drop(g);
        (self.sink)(OutputEvent {
            id,
            data: text,
            seq,
        });
        self.changed.notify_all();
        Ok(info)
    }

    /// Close a session: hang up its terminal (SIGHUP to its process, then the PTY closes
    /// so the kernel hangs up the whole terminal session) and remove it. Only processes
    /// this manager spawned are signalled.
    pub fn close(&self, id: SessionId) -> Result<(), String> {
        let mut g = self.lock();
        let mut s = g.sessions.remove(&id).ok_or_else(|| no_session(id))?;
        let live = s.live.take();
        for t in &mut g.tabs {
            t.panes.retain(|p| *p != id);
        }
        g.tabs.retain(|t| !t.panes.is_empty());
        if g.focused == Some(id) {
            let space = s.space;
            g.focused = g
                .tabs
                .iter()
                .rev()
                .find(|t| t.space == space)
                .or(g.tabs.last())
                .and_then(|t| t.panes.first().copied());
        }
        self.bump(&mut g);
        drop(g);
        if let Some(live) = live {
            hang_up(live);
        }
        self.changed.notify_all();
        Ok(())
    }

    /// Hang up every running process this manager started (app shutdown). The layout stays
    /// saved so the next run can offer to restart the sessions.
    pub fn shutdown(&self) {
        let lives: Vec<Live> = {
            let mut g = self.lock();
            g.sessions
                .values_mut()
                .filter_map(|s| s.live.take())
                .collect()
        };
        lives.into_iter().for_each(hang_up);
    }

    pub fn report(&self, id: SessionId, report: Report) -> Result<(), String> {
        let mut g = self.lock();
        let s = g.sessions.get_mut(&id).ok_or_else(|| no_session(id))?;
        if s.live.is_none() || s.exit.is_some() {
            return Err(format!("session {id} has no running process"));
        }
        s.report = Some(report);
        drop(g);
        self.changed.notify_all();
        Ok(())
    }

    /// Drop the integration report from `source` (the agent exited or handed back).
    pub fn release(&self, id: SessionId, source: &str) -> Result<(), String> {
        let mut g = self.lock();
        let s = g.sessions.get_mut(&id).ok_or_else(|| no_session(id))?;
        if s.report.as_ref().is_some_and(|r| r.source == source) {
            s.report = None;
        }
        drop(g);
        self.changed.notify_all();
        Ok(())
    }

    /// Block until `what` happens or `timeout` passes. Returns the status at that moment
    /// and whether the condition was met. `since` is an output cursor (from [`Manager::run`]
    /// or `SessionInfo::output_seq`); only output after it counts. Default: now.
    pub fn wait(
        &self,
        id: SessionId,
        what: &WaitFor,
        since: Option<u64>,
        timeout: Duration,
    ) -> Result<(Status, bool), String> {
        let deadline = Instant::now() + timeout;
        let mut g = self.lock();
        let now_seq = g.sessions.get(&id).ok_or_else(|| no_session(id))?.seq;
        let start_seq = since.unwrap_or(now_seq).min(now_seq);
        loop {
            let s = g.sessions.get(&id).ok_or_else(|| no_session(id))?;
            let status = s.status(&g.agents);
            let done = matches!(
                status.state,
                State::Completed | State::Failed | State::Ended
            );
            let met = match what {
                WaitFor::Exit => done,
                WaitFor::Idle => {
                    // Settled: the prompt (or question) has been quiet briefly, so a shell
                    // that has not yet forked the command is not mistaken for finished.
                    done || (s.seq > start_seq
                        && s.last_output.elapsed() >= SETTLE
                        && matches!(
                            status.state,
                            State::Idle | State::WaitingForInput | State::Blocked
                        ))
                }
                WaitFor::Output { pattern } => {
                    let new = usize::try_from(s.seq - start_seq).unwrap_or(usize::MAX);
                    strip_ansi(&String::from_utf8_lossy(&s.tail(new))).contains(pattern.as_str())
                }
            };
            let now = Instant::now();
            if met || now >= deadline {
                return Ok((status, met));
            }
            // Status also changes with time (quiet periods), so wake regularly.
            let step = (deadline - now).min(Duration::from_millis(200));
            g = self
                .changed
                .wait_timeout(g, step)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

impl Inner {
    fn alloc(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// Terminate a session's process: SIGHUP (then SIGKILL after a grace period) to the
/// process we spawned, then drop the PTY master so the kernel hangs up the terminal and
/// any job still attached to it receives SIGHUP too.
fn hang_up(mut live: Live) {
    let _ = live.killer.kill();
    drop(live);
}

/// Agent executables recognised in a session's foreground command line. Users can add
/// more (`Manager::add_agents`) without changing this crate.
pub const KNOWN_AGENTS: &[&str] = &[
    "pi",
    "opencode",
    "claude",
    "codex",
    "gemini",
    "aider",
    "amp",
    "goose",
    "crush",
    "qwen",
    "droid",
    "cursor-agent",
    "copilot",
    "kimi",
    "kiro",
    "cline",
];

fn new_session(name: String, space: SpaceId, tab: TabId, spec: Spec) -> Session {
    Session {
        name,
        space,
        tab,
        spec,
        live: None,
        exit: None,
        spawn_error: None,
        ended: false,
        generation: 0,
        scroll: VecDeque::new(),
        seq: 0,
        pending_utf8: Vec::new(),
        last_output: Instant::now(),
        history: Vec::new(),
        report: None,
        started_at: now_secs(),
        size: PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        },
    }
}

fn default_name(p: &Program, id: SessionId) -> String {
    let prog = match p {
        Program::Shell { path } => path.clone().unwrap_or_else(default_shell),
        Program::Command { argv } => argv[0].clone(),
    };
    let base = Path::new(&prog)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(prog);
    format!("{base} {id}")
}

fn no_space(id: SpaceId) -> String {
    format!("no space {id}")
}

fn no_session(id: SessionId) -> String {
    format!("no terminal session {id}")
}

fn home_dir() -> PathBuf {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

type Spawned = (
    Live,
    Box<dyn Read + Send>,
    Box<dyn portable_pty::Child + Send + Sync>,
);

fn spawn(
    spec: &Spec,
    size: PtySize,
    id: SessionId,
    base_env: &[(String, String)],
) -> Result<Spawned, String> {
    let pair = native_pty_system()
        .openpty(size)
        .map_err(|e| e.to_string())?;
    let mut cmd = match &spec.program {
        Program::Shell { path } => CommandBuilder::new(path.clone().unwrap_or_else(default_shell)),
        Program::Command { argv } => {
            let mut c = CommandBuilder::new(&argv[0]);
            c.args(&argv[1..]);
            c
        }
    };
    cmd.cwd(&spec.cwd);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("EAK_TERMINAL", "1");
    cmd.env("EAK_TERMINAL_SESSION_ID", id.to_string());
    cmd.env("EAK_SESSION_ID", id.to_string());
    for (k, v) in base_env.iter().chain(&spec.env) {
        cmd.env(k, v);
    }
    let child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    // The child holds the slave now; dropping ours lets EOF reach the reader on exit.
    drop(pair.slave);
    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let live = Live {
        pid: child.process_id(),
        killer: child.clone_killer(),
        writer: Arc::new(Mutex::new(writer)),
        master: pair.master,
    };
    Ok((live, reader, child))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_split_across_chunks_is_kept_whole() {
        let bytes = "aé€".as_bytes();
        let mut buf = bytes[..2].to_vec(); // 'a' + first byte of 'é'
        assert_eq!(take_utf8(&mut buf), "a");
        buf.extend_from_slice(&bytes[2..]);
        assert_eq!(take_utf8(&mut buf), "é€");
        assert!(buf.is_empty());
    }

    #[test]
    fn invalid_utf8_is_replaced_not_dropped() {
        let mut buf = vec![b'x', 0xFF, b'y'];
        assert_eq!(take_utf8(&mut buf), "x\u{FFFD}y");
    }

    #[test]
    fn scrollback_is_bounded_and_starts_on_a_char_boundary() {
        let mut s = new_session("t".into(), 1, 1, test_spec());
        let chunk = "é".repeat(1024);
        for _ in 0..(SCROLLBACK_BYTES / chunk.len() + 10) {
            s.push_output(chunk.as_bytes());
        }
        assert!(s.scroll.len() <= SCROLLBACK_BYTES);
        assert!(!s.scrollback().contains('\u{FFFD}'));
    }

    #[test]
    fn ended_session_status_is_explicit() {
        let mut s = new_session("t".into(), 1, 1, test_spec());
        s.ended = true;
        assert_eq!(s.status(&[]).state, State::Ended);
    }

    #[test]
    fn exit_codes_map_to_completed_or_failed() {
        let mut s = new_session("t".into(), 1, 1, test_spec());
        s.exit = Some(Exit {
            code: 0,
            signal: false,
        });
        assert_eq!(s.status(&[]).state, State::Completed);
        s.exit = Some(Exit {
            code: 3,
            signal: false,
        });
        let st = s.status(&[]);
        assert_eq!((st.state, st.exit_code), (State::Failed, Some(3)));
    }

    fn test_spec() -> Spec {
        Spec {
            program: Program::Shell { path: None },
            cwd: PathBuf::from("."),
            project: None,
            env: vec![],
        }
    }
}
