//! IDE-shell backend: open-project switching, the project file explorer, and the engine's
//! per-operation change list (used by the Update PCB report).
//!
//! The file operations are the only place the desktop app touches project folders outside the
//! engine, so they are deliberately narrow:
//! * every path is a project-relative path that is validated component by component and then
//!   re-checked after canonicalisation, so `..`, absolute paths, drive prefixes and symlinks
//!   that point out of the project root are all refused;
//! * every call names the root it was made for: an open project's id, or `dir:<absolute path>`
//!   for a terminal space that is not an open project. A call can only ever act inside that one
//!   root, so a late call from one project's UI can never act on another project's folder;
//! * in an EAK project folder, files the engine owns (the design, its journal, checkpoints, evidence, the lock) can be read
//!   but never created, renamed or deleted here: design changes are `Engine::execute` commands.

use crate::{snapshot, Session, Snapshot};
use eak_runtime::delta::Change;
use eak_runtime::store::{self, EntryState};
use serde::Serialize;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Largest file the viewer reads into memory.
pub const MAX_READ_BYTES: u64 = 512 * 1024;

/// Top-level names the engine owns inside a project folder.
const ENGINE_OWNED: [&str; 5] = [
    store::PROJECT_FILE,
    store::JOURNAL_FILE,
    ".lock",
    "checkpoints",
    "evidence",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OpenProject {
    pub id: String,
    pub name: String,
    pub dir: Option<String>,
    pub active: bool,
}

pub fn open_projects(s: &Session) -> Result<Vec<OpenProject>, String> {
    let g = s.engine()?;
    Ok(g.engines
        .iter()
        .enumerate()
        .map(|(i, e)| OpenProject {
            id: e.project().id.to_string(),
            name: e.project().name.clone(),
            dir: e.dir().map(|d| d.display().to_string()),
            active: g.active == Some(i),
        })
        .collect())
}

/// Make an already-open project the active one.
pub fn switch_project(s: &Session, project_id: &str) -> Result<Snapshot, String> {
    let mut g = s.engine()?;
    let i = g
        .engines
        .iter()
        .position(|e| e.project().id == project_id)
        .ok_or_else(|| format!("project {project_id} is not open"))?;
    g.active = Some(i);
    Ok(snapshot(&g.engines[i]))
}

/// Close one open project (not necessarily the active one). Returns the snapshot of whichever
/// project is active afterwards, or `None` when no project is open any more.
pub fn close_project_by_id(s: &Session, project_id: &str) -> Result<Option<Snapshot>, String> {
    let mut g = s.engine()?;
    let i = g
        .engines
        .iter()
        .position(|e| e.project().id == project_id)
        .ok_or_else(|| format!("project {project_id} is not open"))?;
    let was_active = g.active == Some(i);
    let gone = g.engines.remove(i);
    s.advertise(None, gone.dir().map(Path::to_path_buf));
    g.active = match g.active {
        _ if g.engines.is_empty() => None,
        Some(a) if !was_active && a > i => Some(a - 1),
        Some(a) if !was_active => Some(a),
        _ => Some(g.engines.len() - 1),
    };
    Ok(g.active.map(|a| snapshot(&g.engines[a])))
}

/// The changes the engine journalled for one committed operation. This is the engine's own
/// before/after delta, so reports built from it cannot disagree with what was applied.
pub fn operation_changes(
    s: &Session,
    project_id: &str,
    op_id: &str,
) -> Result<Vec<Change>, String> {
    let dir = active_dir(s, project_id)?;
    // The engine journals the changes on the pending record and clears them on the committed one,
    // so: require a committed record, then read the changes from the pending record.
    let journal = store::read_journal(&dir);
    if !journal
        .iter()
        .any(|e| e.op_id == op_id && e.state == EntryState::Committed)
    {
        return Err(format!(
            "operation {op_id} is not in the journal as committed"
        ));
    }
    journal
        .into_iter()
        .find(|e| e.op_id == op_id && e.state == EntryState::Pending)
        .map(|e| e.changes)
        .ok_or_else(|| format!("operation {op_id} has no recorded changes"))
}

// ---------------------------------------------------------------------------------------------
// File explorer
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    /// Project-relative path with `/` separators on every platform.
    pub rel: String,
    pub is_dir: bool,
    pub size: u64,
    /// Engine-owned (design, history, evidence, lock): viewable, never changed from here.
    pub protected: bool,
    /// What the entry is in the project: `design`, `history`, `evidence`, `generated`, `library`,
    /// `config`, `lock`, `folder` or `file`.
    pub role: String,
    pub modified_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FileContent {
    pub rel: String,
    pub size: u64,
    /// `None` for binary files and for files larger than [`MAX_READ_BYTES`].
    pub text: Option<String>,
    pub binary: bool,
    pub truncated: bool,
}

/// Folder of a root: an open project (any of them, not only the active one) or `dir:<path>`.
fn active_dir(s: &Session, root_id: &str) -> Result<PathBuf, String> {
    if let Some(p) = root_id.strip_prefix("dir:") {
        let path = Path::new(p);
        if !path.is_absolute() || !path.is_dir() {
            return Err(format!("{p:?} is not an existing absolute folder"));
        }
        return Ok(path.to_path_buf());
    }
    let g = s.engine()?;
    g.engines
        .iter()
        .find(|e| e.project().id == root_id)
        .ok_or_else(|| format!("project {root_id} is not open"))?
        .dir()
        .map(Path::to_path_buf)
        .ok_or_else(|| "this project has no folder on disk".to_string())
}

/// Engine-owned names are only special inside an EAK project folder.
fn engine_root(root: &Path) -> bool {
    root.join(store::PROJECT_FILE).is_file()
}

fn bad_name(name: &str) -> Option<String> {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if name.is_empty() || name == "." || name == ".." {
        return Some("enter a file name".into());
    }
    if name
        .chars()
        .any(|c| c.is_control() || "<>:\"|?*/\\".contains(c))
    {
        return Some(format!(
            "\"{name}\" contains a character that is not allowed in a file name"
        ));
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Some("a file name cannot end with a space or a dot".into());
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        return Some(format!("\"{name}\" is a reserved name on Windows"));
    }
    None
}

/// Split a project-relative path into validated components. `""` is the project root.
fn components(rel: &str) -> Result<Vec<String>, String> {
    if Path::new(rel)
        .components()
        .any(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
        || rel.starts_with(['/', '\\'])
    {
        return Err(format!("\"{rel}\" is not a project-relative path"));
    }
    let mut out = vec![];
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        if part == "." {
            continue;
        }
        if part == ".." {
            return Err(format!("\"{rel}\" leaves the project folder"));
        }
        if part.contains(':') || part.chars().any(char::is_control) {
            return Err(format!("\"{rel}\" is not a project-relative path"));
        }
        out.push(part.to_string());
    }
    Ok(out)
}

fn is_protected(root: &Path, parts: &[String]) -> bool {
    engine_root(root)
        && parts
            .first()
            .is_some_and(|f| ENGINE_OWNED.contains(&f.as_str()))
}

/// Resolve `rel` under `root`. The deepest existing ancestor must canonicalise inside the
/// canonical root, which rejects symlinks that escape it.
fn resolve(root: &Path, parts: &[String]) -> Result<PathBuf, String> {
    let root_c = root
        .canonicalize()
        .map_err(|e| format!("project folder unavailable: {e}"))?;
    let mut path = root_c.clone();
    path.extend(parts);
    let mut probe = path.clone();
    loop {
        match probe.canonicalize() {
            Ok(c) => {
                if !c.starts_with(&root_c) {
                    return Err("that path resolves outside the project folder".into());
                }
                break;
            }
            Err(_) => {
                if !probe.pop() || !probe.starts_with(&root_c) {
                    return Err("that path resolves outside the project folder".into());
                }
            }
        }
    }
    Ok(path)
}

fn role_of(parent_role: &str, name: &str, is_dir: bool) -> String {
    let top = match name {
        n if n == store::PROJECT_FILE => "design",
        n if n == store::JOURNAL_FILE || n == "checkpoints" => "history",
        "evidence" => "evidence",
        "output" => "generated",
        "sourcing" => "library",
        ".lock" => "lock",
        _ => "",
    };
    if !top.is_empty() {
        return top.into();
    }
    if matches!(
        parent_role,
        "history" | "evidence" | "generated" | "library"
    ) {
        return parent_role.into();
    }
    let lower = name.to_ascii_lowercase();
    if !is_dir
        && (lower.ends_with(".toml")
            || lower.ends_with(".yaml")
            || lower.ends_with(".yml")
            || lower == ".gitignore"
            || lower == ".gitattributes")
    {
        "config".into()
    } else if is_dir {
        "folder".into()
    } else {
        "file".into()
    }
}

/// List one directory of the active project (`rel == ""` is the project root).
pub fn project_files(s: &Session, project_id: &str, rel: &str) -> Result<Vec<FileEntry>, String> {
    let root = active_dir(s, project_id)?;
    let parts = components(rel)?;
    let dir = resolve(&root, &parts)?;
    let parent_role = parts
        .iter()
        .fold(String::from("folder"), |role, p| role_of(&role, p, true));
    let mut out = vec![];
    for entry in fs::read_dir(&dir).map_err(|e| format!("cannot list {rel:?}: {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // symlink_metadata: a symlink is listed as a leaf and never descended into.
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let mut child = parts.clone();
        child.push(name.clone());
        out.push(FileEntry {
            rel: child.join("/"),
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
            protected: is_protected(&root, &child),
            role: role_of(&parent_role, &name, meta.is_dir()),
            modified_ms: meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX)),
            name,
        });
    }
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// Read a file for the viewer: text up to [`MAX_READ_BYTES`], otherwise a description only.
pub fn project_file_read(s: &Session, project_id: &str, rel: &str) -> Result<FileContent, String> {
    use std::io::Read;
    let root = active_dir(s, project_id)?;
    let parts = components(rel)?;
    if parts.is_empty() {
        return Err("choose a file, not the project folder".into());
    }
    let path = resolve(&root, &parts)?;
    let meta = fs::metadata(&path).map_err(|e| format!("cannot open {rel:?}: {e}"))?;
    if meta.is_dir() {
        return Err(format!("{rel:?} is a folder"));
    }
    let size = meta.len();
    let rel = parts.join("/");
    if size > MAX_READ_BYTES {
        return Ok(FileContent {
            rel,
            size,
            text: None,
            binary: false,
            truncated: true,
        });
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    fs::File::open(&path)
        .and_then(|f| f.take(MAX_READ_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("cannot read {rel:?}: {e}"))?;
    let head = &bytes[..bytes.len().min(8192)];
    if head.contains(&0) {
        return Ok(FileContent {
            rel,
            size,
            text: None,
            binary: true,
            truncated: false,
        });
    }
    match String::from_utf8(bytes) {
        Ok(text) => Ok(FileContent {
            rel,
            size,
            text: Some(text),
            binary: false,
            truncated: false,
        }),
        Err(_) => Ok(FileContent {
            rel,
            size,
            text: None,
            binary: true,
            truncated: false,
        }),
    }
}

fn mutable_target(root: &Path, rel: &str) -> Result<(Vec<String>, PathBuf), String> {
    let parts = components(rel)?;
    let Some(name) = parts.last() else {
        return Err("that is the project folder itself".into());
    };
    if let Some(why) = bad_name(name) {
        return Err(why);
    }
    if is_protected(root, &parts) {
        return Err(format!(
            "{rel:?} is owned by the EAK engine; change the design through its commands, not the file"
        ));
    }
    // Resolve the parent only: the target itself may be a symlink, which delete and rename must
    // act on (unlink / move the link), never follow.
    let parent = resolve(root, &parts[..parts.len() - 1])?;
    Ok((parts.clone(), parent.join(name)))
}

/// Create an empty file or a folder. Fails if something already exists there.
pub fn project_file_create(
    s: &Session,
    project_id: &str,
    rel: &str,
    is_dir: bool,
) -> Result<FileEntry, String> {
    let root = active_dir(s, project_id)?;
    let (parts, path) = mutable_target(&root, rel)?;
    let parent = path.parent().ok_or("invalid path")?;
    if !parent.is_dir() {
        return Err("the parent folder does not exist".into());
    }
    if fs::symlink_metadata(&path).is_ok() {
        return Err(format!("{rel:?} already exists"));
    }
    if is_dir {
        fs::create_dir(&path)
    } else {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map(|_| ())
    }
    .map_err(|e| format!("cannot create {rel:?}: {e}"))?;
    let name = parts.last().cloned().unwrap_or_default();
    Ok(FileEntry {
        rel: parts.join("/"),
        is_dir,
        size: 0,
        protected: false,
        role: role_of("folder", &name, is_dir),
        modified_ms: None,
        name,
    })
}

/// Rename or move within the project. Never overwrites.
pub fn project_file_rename(
    s: &Session,
    project_id: &str,
    from: &str,
    to: &str,
) -> Result<(), String> {
    let root = active_dir(s, project_id)?;
    let (_, src) = mutable_target(&root, from)?;
    let (_, dst) = mutable_target(&root, to)?;
    if fs::symlink_metadata(&src).is_err() {
        return Err(format!("{from:?} does not exist"));
    }
    if fs::symlink_metadata(&dst).is_ok() {
        return Err(format!("{to:?} already exists"));
    }
    if !dst.parent().is_some_and(Path::is_dir) {
        return Err("the destination folder does not exist".into());
    }
    fs::rename(&src, &dst).map_err(|e| format!("cannot rename {from:?}: {e}"))
}

/// Delete a file or folder. A non-empty folder needs `recursive`. A symlink is unlinked, never
/// followed.
pub fn project_file_delete(
    s: &Session,
    project_id: &str,
    rel: &str,
    recursive: bool,
) -> Result<(), String> {
    let root = active_dir(s, project_id)?;
    let (_, path) = mutable_target(&root, rel)?;
    let meta = fs::symlink_metadata(&path).map_err(|e| format!("cannot delete {rel:?}: {e}"))?;
    if meta.is_dir() {
        let empty = fs::read_dir(&path)
            .map_err(|e| e.to_string())?
            .next()
            .is_none();
        if !empty && !recursive {
            return Err(format!(
                "{rel:?} is not empty; confirm deleting its contents"
            ));
        }
        fs::remove_dir_all(&path)
    } else {
        fs::remove_file(&path)
    }
    .map_err(|e| format!("cannot delete {rel:?}: {e}"))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitEntry {
    /// Root-relative path, `/` separated.
    pub rel: String,
    /// Two-letter porcelain code, e.g. ` M`, `??`, `A `.
    pub code: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GitStatus {
    /// False when git is not installed or the root is not inside a repository.
    pub available: bool,
    pub branch: Option<String>,
    pub entries: Vec<GitEntry>,
}

/// Git status of a root, from the `git` on PATH (read-only). Never an error: missing git or a
/// non-repository is reported as `available: false`.
pub fn project_git_status(s: &Session, project_id: &str) -> Result<GitStatus, String> {
    let root = active_dir(s, project_id)?;
    let none = GitStatus {
        available: false,
        branch: None,
        entries: vec![],
    };
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
    };
    let Some(out) = run(&[
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=normal",
        ".",
    ]) else {
        return Ok(none);
    };
    // Paths in porcelain output are relative to the repository root unless asked otherwise.
    let prefix = run(&["rev-parse", "--show-prefix"])
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let branch = run(&["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|b| !b.is_empty());
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut entries = vec![];
    let mut parts = text.split('\0');
    while let Some(rec) = parts.next() {
        if rec.len() < 4 {
            continue;
        }
        let code = rec[..2].to_string();
        let mut path = rec[3..].to_string();
        if code.contains('R') || code.contains('C') {
            parts.next(); // the original path of a rename/copy follows as its own record
        }
        if let Some(p) = path.strip_prefix(prefix.as_str()) {
            path = p.to_string();
        }
        entries.push(GitEntry {
            rel: path.trim_end_matches('/').to_string(),
            code,
        });
    }
    Ok(GitStatus {
        available: true,
        branch,
        entries,
    })
}

/// Tauri command wrappers: argument decoding only, no logic.
pub mod cmd {
    use super::*;
    use tauri::State;

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn open_projects(state: State<'_, Session>) -> Result<Vec<OpenProject>, String> {
        super::open_projects(&state)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn switch_project(
        state: State<'_, Session>,
        project_id: String,
    ) -> Result<Snapshot, String> {
        super::switch_project(&state, &project_id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn close_project_by_id(
        state: State<'_, Session>,
        project_id: String,
    ) -> Result<Option<Snapshot>, String> {
        super::close_project_by_id(&state, &project_id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn operation_changes(
        state: State<'_, Session>,
        project_id: String,
        op_id: String,
    ) -> Result<Vec<Change>, String> {
        super::operation_changes(&state, &project_id, &op_id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn project_files(
        state: State<'_, Session>,
        project_id: String,
        rel: String,
    ) -> Result<Vec<FileEntry>, String> {
        super::project_files(&state, &project_id, &rel)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn project_git_status(
        state: State<'_, Session>,
        project_id: String,
    ) -> Result<GitStatus, String> {
        super::project_git_status(&state, &project_id)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn project_file_read(
        state: State<'_, Session>,
        project_id: String,
        rel: String,
    ) -> Result<FileContent, String> {
        super::project_file_read(&state, &project_id, &rel)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn project_file_create(
        state: State<'_, Session>,
        project_id: String,
        rel: String,
        is_dir: bool,
    ) -> Result<FileEntry, String> {
        super::project_file_create(&state, &project_id, &rel, is_dir)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn project_file_rename(
        state: State<'_, Session>,
        project_id: String,
        from: String,
        to: String,
    ) -> Result<(), String> {
        super::project_file_rename(&state, &project_id, &from, &to)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn project_file_delete(
        state: State<'_, Session>,
        project_id: String,
        rel: String,
        recursive: bool,
    ) -> Result<(), String> {
        super::project_file_delete(&state, &project_id, &rel, recursive)
    }
}
