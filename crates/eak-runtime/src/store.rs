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
//!   .lock              project write lock (pid)
//! ```

use crate::delta::Change;
use eak_core::Project;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = "project.eak.json";
pub const JOURNAL_FILE: &str = "journal.jsonl";

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

pub fn acquire_lock(dir: &Path, take_over: bool) -> Result<ProjectLock, String> {
    let path = dir.join(".lock");
    if take_over {
        let _ = fs::remove_file(&path);
    }
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut f) => {
            let _ = write!(f, "{}", std::process::id());
            Ok(ProjectLock { path })
        }
        Err(_) => {
            let holder = fs::read_to_string(&path).unwrap_or_default();
            Err(format!("project is locked by process {holder} (open elsewhere, or a previous session crashed — reopen with recovery to take over)"))
        }
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
