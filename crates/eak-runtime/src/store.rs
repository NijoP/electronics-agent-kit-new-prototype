//! Project folder persistence: atomic project writes, append-only operation journal,
//! checkpoints, crash reconciliation and a project-scoped write lock.
//!
//! Layout of a project folder:
//! ```text
//! <name>.eak/
//!   project.eak.json   canonical EAK-IR (atomically replaced on every commit)
//!   journal.jsonl      operation journal (pending → committed/aborted records)
//!   checkpoints/       named full snapshots
//!   evidence/          verification reports bound to revisions
//!   outputs/           manufacturing packages
//!   .lock              project write lock (pid, host); see `inspect_lock`
//!   recovery.blocked   present while an UNKNOWN_OUTCOME recovery awaits acknowledgement
//! ```

use crate::delta::Change;
use eak_core::Project;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = "project.eak.json";
pub const JOURNAL_FILE: &str = "journal.jsonl";
/// Durable write block: survives process exit until the user acknowledges recovery.
pub const RECOVERY_BLOCK_FILE: &str = "recovery.blocked";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryState {
    Pending,
    Committed,
    Aborted,
    /// Outcome could not be determined during recovery (never silently replayed).
    UnknownOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Do,
    Undo,
    Redo,
    Restore,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub op_id: String,
    pub state: EntryState,
    pub kind: EntryKind,
    /// For undo/redo: the original operation being reversed/re-applied.
    #[serde(default)]
    pub target_op: Option<String>,
    pub revision_before: u64,
    pub revision_after: u64,
    pub actor: String,
    pub command: String,
    #[serde(default)]
    pub changes: Vec<Change>,
    pub timestamp_ms: u128,
}

pub fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[derive(Debug)]
pub struct ProjectLock {
    path: PathBuf,
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// What the project's `.lock` file means right now (ADR-0029). Locks are never deleted or overridden
/// automatically: only [`LockState::Interrupted`] may be taken over, and only on explicit request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LockState {
    /// No lock file: the project can be opened.
    Unlocked,
    /// A live process holds it (`this_app`: this very EAK process, e.g. a second window).
    OpenElsewhere { this_app: bool },
    /// The holder is provably gone (same host, process not running): a previous session ended
    /// without releasing. `pending_operations` is how many journalled operations will be reconciled
    /// on open (recovery available when non-zero). Takeover is safe.
    Interrupted { pending_operations: usize },
    /// A lock exists but its holder cannot be judged (other machine, unreadable or unknown
    /// format, liveness check unavailable). Never taken over automatically.
    Unverifiable { reason: String },
    /// The project folder cannot be inspected at all.
    Unavailable { reason: String },
}

/// Lock file content: `pid`, then optionally `host` on a second line.
fn host_id() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        fs::read_to_string("/proc/sys/kernel/hostname")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("COMPUTERNAME").ok().filter(|s| !s.is_empty())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        std::process::Command::new("hostname")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }
}

/// `Some(true/false)` when the process table answers, `None` when it cannot be asked.
fn pid_alive(pid: u32) -> Option<bool> {
    if pid == std::process::id() {
        return Some(true);
    }
    #[cfg(target_os = "linux")]
    {
        Some(Path::new(&format!("/proc/{pid}")).exists())
    }
    #[cfg(all(unix, not(target_os = "linux")))]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .ok()
            .map(|o| o.status.success())
    }
    #[cfg(windows)]
    {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\"")))
    }
}

/// Journal operations still pending (not committed/aborted) that `recover` will reconcile.
pub fn pending_operations(dir: &Path) -> usize {
    let mut open: Vec<String> = vec![];
    for e in read_journal(dir) {
        match e.state {
            EntryState::Pending => open.push(e.op_id),
            _ => open.retain(|o| *o != e.op_id),
        }
    }
    open.len()
}

/// Read-only: classify the project's lock. A reused process id looks alive, which errs on the safe
/// side (reported as open elsewhere, never as takeover-safe).
pub fn inspect_lock(dir: &Path) -> LockState {
    let path = dir.join(".lock");
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return if dir.is_dir() {
                LockState::Unlocked
            } else {
                LockState::Unavailable {
                    reason: "the project folder does not exist or is not a folder".into(),
                }
            };
        }
        Err(e) => {
            return LockState::Unavailable {
                reason: format!("the lock file cannot be read: {e}"),
            }
        }
    };
    let mut lines = text.lines();
    let Some(pid) = lines.next().and_then(|l| l.trim().parse::<u32>().ok()) else {
        // Empty or unparsable: a writer may be mid-write, or the file is foreign. Do not judge.
        return LockState::Unverifiable {
            reason: "the lock file does not name its holder".into(),
        };
    };
    if let Some(host) = lines.next().map(str::trim).filter(|h| !h.is_empty()) {
        if host_id().as_deref() != Some(host) {
            return LockState::Unverifiable {
                reason: "the lock was written on another computer".into(),
            };
        }
    } else if host_id().is_none() {
        return LockState::Unverifiable {
            reason: "this computer could not be identified".into(),
        };
    }
    match pid_alive(pid) {
        Some(true) => LockState::OpenElsewhere {
            this_app: pid == std::process::id(),
        },
        Some(false) => LockState::Interrupted {
            pending_operations: pending_operations(dir),
        },
        None => LockState::Unverifiable {
            reason: "whether the holder is still running could not be checked".into(),
        },
    }
}

/// Plain wording for a refused open; the structured state is what the UI acts on.
fn lock_message(s: &LockState) -> String {
    match s {
        LockState::OpenElsewhere { this_app: true } => {
            "project is locked: it is already open in this EAK app".into()
        }
        LockState::OpenElsewhere { .. } => {
            "project is locked: it is open in another EAK window or process".into()
        }
        LockState::Interrupted { .. } => {
            "project is locked: the previous session was interrupted; recover and open to continue"
                .into()
        }
        LockState::Unverifiable { reason } => {
            format!("project is locked and cannot be taken over safely: {reason}")
        }
        LockState::Unlocked => "project lock could not be created".into(),
        LockState::Unavailable { reason } => format!("project access unavailable: {reason}"),
    }
}

/// Take the project write lock. `take_over` removes an existing lock only when [`inspect_lock`]
/// proves the previous holder is gone; a live or unverifiable holder is never overridden.
pub fn acquire_lock(dir: &Path, take_over: bool) -> Result<ProjectLock, String> {
    let path = dir.join(".lock");
    if take_over {
        match inspect_lock(dir) {
            LockState::Interrupted { .. } => {
                let _ = fs::remove_file(&path);
            }
            LockState::Unlocked => {}
            other => return Err(lock_message(&other)),
        }
    }
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut f) => {
            let _ = write!(f, "{}", std::process::id());
            if let Some(h) = host_id() {
                let _ = write!(f, "\n{h}");
            }
            Ok(ProjectLock { path })
        }
        Err(_) => Err(lock_message(&inspect_lock(dir))),
    }
}

/// Write file contents atomically: temp file + fsync + rename.
pub fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

pub fn append_journal(dir: &Path, e: &JournalEntry) -> std::io::Result<()> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(JOURNAL_FILE))?;
    let line = serde_json::to_string(e).expect("journal entry serialisable");
    writeln!(f, "{line}")?;
    f.sync_all()
}

pub fn read_journal(dir: &Path) -> Vec<JournalEntry> {
    let Ok(f) = File::open(dir.join(JOURNAL_FILE)) else {
        return vec![];
    };
    // A torn final line (crash mid-append) is skipped, not fatal.
    BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str(&l).ok())
        .collect()
}

pub fn write_project(dir: &Path, p: &Project) -> std::io::Result<()> {
    atomic_write(&dir.join(PROJECT_FILE), eak_core::to_json(p).as_bytes())
}

pub fn read_project(dir: &Path) -> Result<Project, String> {
    let s = fs::read_to_string(dir.join(PROJECT_FILE))
        .map_err(|e| format!("cannot read project file: {e}"))?;
    eak_core::from_json(&s).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecoveryReport {
    /// Operations found pending at open, and how each was reconciled.
    pub reconciled: Vec<(String, EntryState)>,
    pub torn_temp_file_removed: bool,
}

/// Reconcile interrupted operations against the actual on-disk revision.
pub fn recover(dir: &Path, project: &Project) -> std::io::Result<RecoveryReport> {
    let mut rep = RecoveryReport::default();
    let tmp = dir.join(PROJECT_FILE).with_extension("tmp");
    if tmp.exists() {
        fs::remove_file(&tmp)?;
        rep.torn_temp_file_removed = true;
    }
    let journal = read_journal(dir);
    let mut open: Vec<&JournalEntry> = vec![];
    for e in &journal {
        match e.state {
            EntryState::Pending => open.push(e),
            _ => open.retain(|o| o.op_id != e.op_id),
        }
    }
    for e in open {
        let state = if project.revision == e.revision_after {
            EntryState::Committed
        } else if project.revision == e.revision_before {
            EntryState::Aborted
        } else {
            EntryState::UnknownOutcome
        };
        let mut r = e.clone();
        r.state = state;
        r.changes.clear();
        r.timestamp_ms = now_ms();
        append_journal(dir, &r)?;
        rep.reconciled.push((e.op_id.clone(), state));
    }
    Ok(rep)
}
