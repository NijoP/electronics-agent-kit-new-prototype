//! Project Home (slice S1): what the start screen shows about a project before it is opened, and
//! the three ways Home starts one (a new design, a design brief, the sample board).
//!
//! Everything here is either read-only or goes through the engine:
//! * [`project_facts`] reads the project file and the evidence the engine saved for its current
//!   revision. Copper connectivity is recomputed by `eak-engineering` and the release check is the
//!   same gate evaluation the Release check dialog shows. A check with no evidence for this
//!   revision is reported as not run, never as passing.
//! * [`create_project_with_brief`] creates the project and records the brief as `AddRequirement`
//!   commands in one `Batch` (one revision). No AI is involved: the lines are the engineer's text.
//! * [`create_sample_project`] builds the reference design of `eak_runtime::fixtures` in a new
//!   folder, every object through a typed command, and opens it. The bundled design is never
//!   modified in place.
//! * [`default_projects_dir`] and [`suggest_folder`] only compute paths; nothing is created until
//!   the engine creates the project.

use crate::{install, project_dir, release_check_of, Session, Snapshot};
use eak_runtime::{Actor, Command, CommandRequest, Engine};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Folder name of the projects folder Home proposes for new designs.
pub const PROJECTS_FOLDER: &str = "EAK Projects";
/// Most brief lines one Home brief may record (a pasted document is not a brief).
pub const MAX_BRIEF_LINES: usize = 40;
/// Longest single brief line, in characters.
pub const MAX_BRIEF_LINE: usize = 2_000;
/// Most unrouted net names listed in the facts (the count is always complete).
const MAX_LISTED: usize = 8;

/// Counts read from the project content, the same quantities the Overview panel's `designFacts`
/// derives from the snapshot.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DesignCounts {
    pub sheets: usize,
    pub parts: usize,
    pub nets: usize,
    pub footprints: usize,
    /// Schematic parts with no footprint on the board (Update PCB not run for them).
    pub missing_footprints: usize,
    pub tracks: usize,
    pub vias: usize,
    pub zones: usize,
    pub outline: bool,
    pub bom_lines: usize,
    /// Placed parts whose definition has no supplier part number (legacy field or an offer).
    pub without_supplier: usize,
    /// Placed parts whose definition names no manufacturer part number.
    pub without_mpn: usize,
    /// Brief lines recorded in the project (`AddRequirement`).
    pub requirements: usize,
}

/// One check's saved evidence for the project's current revision.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckFact {
    /// `ERC`, `DRC`, `MECH` or `DFM`.
    pub check: String,
    pub errors: usize,
    pub warnings: usize,
    /// Rules the report could not evaluate (missing input such as an undeclared net current).
    pub unknown_rules: usize,
}

/// The release check over the saved evidence (read-only, nothing is run).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReleaseFact {
    /// `DRAFT` unless every required gate passes.
    pub status: eak_manufacturing::PackageStatus,
    pub required: usize,
    pub passing: usize,
    /// The first blocking reasons, as the Release check dialog words them.
    pub blocking: Vec<String>,
}

/// What Project Home shows about a project folder.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HomeFacts {
    pub path: String,
    pub name: String,
    pub revision: u64,
    /// Stage named after the furthest design content present (`status::phase_of`).
    pub phase: String,
    pub design: DesignCounts,
    /// Evidence for the current revision only; a check missing here has not run on it.
    pub checks: Vec<CheckFact>,
    /// Nets whose pads are not all joined by copper yet.
    pub unrouted_nets: usize,
    /// The first unrouted net names (at most eight).
    pub unrouted_names: Vec<String>,
    /// Unrouted pad-to-pad connections (ratsnest lines).
    pub unrouted_connections: usize,
    pub shorts: usize,
    pub release: ReleaseFact,
}

/// Read-only facts for the project in `path` (it may be open in this app; nothing is written).
pub fn project_facts(path: &str) -> Result<HomeFacts, String> {
    let dir = project_dir(path)?;
    let p = eak_runtime::store::read_project(&dir)?;
    Ok(facts_of(&dir, &p))
}

fn facts_of(dir: &Path, p: &eak_core::Project) -> HomeFacts {
    let symbols: Vec<_> = p.schematic.symbols.values().collect();
    let placed: std::collections::BTreeSet<&str> = p
        .pcb
        .footprints
        .values()
        .map(|f| f.symbol_id.as_str())
        .collect();
    let def = |id: &str| p.library.get(id);
    let design = DesignCounts {
        sheets: p.schematic.sheets.len(),
        parts: symbols.len(),
        nets: eak_engineering::netlist::build(p).nets.len(),
        footprints: p.pcb.footprints.len(),
        missing_footprints: symbols
            .iter()
            .filter(|s| !placed.contains(s.id.as_str()))
            .count(),
        tracks: p.pcb.tracks.len(),
        vias: p.pcb.vias.len(),
        zones: p.pcb.zones.len(),
        outline: p.pcb.board.outline.len() >= 3,
        bom_lines: symbols
            .iter()
            .map(|s| (s.component_id.as_str(), s.value.as_str()))
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        without_supplier: symbols
            .iter()
            .filter(|s| {
                !def(s.component_id.as_str())
                    .is_some_and(|d| d.distributor.is_some() || !d.supplier_offers.is_empty())
            })
            .count(),
        without_mpn: symbols
            .iter()
            .filter(|s| !def(s.component_id.as_str()).is_some_and(|d| d.mpn.is_some()))
            .count(),
        requirements: p.requirements.len(),
    };
    let unknown = |r: &eak_validation::Report| {
        let u = eak_validation::CheckState::Unknown;
        r.rule_results
            .iter()
            .filter(|rr| rr.state == u || rr.default_state == Some(u))
            .count()
    };
    let checks = ["ERC", "DRC", "MECH", "DFM"]
        .iter()
        .filter_map(|c| crate::current_evidence(Some(dir), c, p))
        .map(|r| CheckFact {
            check: r.check.clone(),
            errors: r.errors,
            warnings: r.warnings,
            unknown_rules: unknown(&r),
        })
        .collect();
    let conn = eak_engineering::copper::analyse(p);
    let mut unrouted: Vec<String> = conn
        .net_groups
        .iter()
        .filter(|(_, groups)| **groups > 1)
        .map(|(n, _)| n.clone())
        .collect();
    let unrouted_nets = unrouted.len();
    unrouted.truncate(MAX_LISTED);
    let rc = release_check_of(Some(dir), p);
    let required: Vec<_> = rc.gates.iter().filter(|g| g.required).collect();
    HomeFacts {
        path: dir.display().to_string(),
        name: p.name.clone(),
        revision: p.revision,
        phase: eak_runtime::status::phase_of(p).into(),
        design,
        checks,
        unrouted_nets,
        unrouted_names: unrouted,
        unrouted_connections: conn.ratsnest.len(),
        shorts: conn.shorts.len(),
        release: ReleaseFact {
            status: rc.status,
            required: required.len(),
            passing: required.iter().filter(|g| g.state == "PASS").count(),
            blocking: rc.blocking.iter().take(5).cloned().collect(),
        },
    }
}

/// `<documents>/EAK Projects`, else `<home>/EAK Projects`; `None` when neither is known.
/// The folder is not created here: the engine creates it with the first project.
pub fn default_projects_dir(documents: Option<PathBuf>, home: Option<PathBuf>) -> Option<String> {
    documents
        .or(home)
        .map(|d| d.join(PROJECTS_FOLDER).display().to_string())
}

/// A folder name that is valid on all three desktop systems.
fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    // Windows refuses names ending in a dot or a space.
    let cleaned = cleaned.trim_end_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        "Untitled board".into()
    } else {
        cleaned
    }
}

/// `<parent>/<name>`, or `<parent>/<name> 2`, `… 3` when that path already exists, so a new
/// project never lands in an existing folder. Read-only.
pub fn suggest_folder(parent: &str, name: &str) -> Result<String, String> {
    let parent = project_dir(parent)?;
    let base = clean_name(name);
    for n in 1..1000 {
        let leaf = if n == 1 {
            base.clone()
        } else {
            format!("{base} {n}")
        };
        let candidate = parent.join(&leaf);
        if !candidate.exists() {
            return Ok(candidate.display().to_string());
        }
    }
    Err(format!(
        "no free folder name for {base} in {}",
        parent.display()
    ))
}

/// Brief lines as they are recorded: trimmed, blank lines dropped, each line at most
/// [`MAX_BRIEF_LINE`] characters, at most [`MAX_BRIEF_LINES`] lines.
pub fn brief_lines(lines: &[String]) -> Result<Vec<String>, String> {
    let kept: Vec<String> = lines
        .iter()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    if kept.len() > MAX_BRIEF_LINES {
        return Err(format!(
            "a brief holds at most {MAX_BRIEF_LINES} lines; this one has {}",
            kept.len()
        ));
    }
    if let Some(l) = kept.iter().find(|l| l.chars().count() > MAX_BRIEF_LINE) {
        return Err(format!(
            "a brief line is at most {MAX_BRIEF_LINE} characters; one has {}",
            l.chars().count()
        ));
    }
    Ok(kept)
}

/// Create a project and record its brief: one `Batch` of `AddRequirement` commands through
/// `Engine::execute` (one revision, one undo step), with the human actor. An empty brief
/// creates the project only.
pub fn create_project_with_brief(
    s: &Session,
    path: &str,
    name: &str,
    brief: &[String],
) -> Result<Snapshot, String> {
    let lines = brief_lines(brief)?;
    let dir = project_dir(path)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("enter a project name".into());
    }
    install(s, &dir, || {
        let mut e = Engine::create(&dir, name)?;
        if !lines.is_empty() {
            let r = e.execute(CommandRequest {
                expected_revision: e.project().revision,
                actor: Actor::human(),
                command: Command::Batch {
                    commands: lines
                        .into_iter()
                        .map(|text| Command::AddRequirement { text })
                        .collect(),
                    label: Some("Design brief from Project Home".into()),
                },
                approval: None,
            });
            if !r.accepted {
                return Err(format!(
                    "the project was created, but its brief was refused: {:?}",
                    r.error
                ));
            }
        }
        Ok(e)
    })
}

/// Create the sample board (the `eak_runtime::fixtures` reference design: 5 V header, AMS1117
/// LDO, ATtiny85, LED and I/O header, placed, routed and poured on two layers) in `path` and
/// open it. Every object is made by a typed command, so its journal shows how it was built.
pub fn create_sample_project(s: &Session, path: &str) -> Result<Snapshot, String> {
    let dir = project_dir(path)?;
    install(s, &dir, || {
        let mut e = Engine::create(&dir, "Sample board")?;
        eak_runtime::fixtures::build_schematic(&mut e)?;
        eak_runtime::fixtures::build_pcb(&mut e)?;
        Ok(e)
    })
}

pub mod cmd {
    use crate::cmd::blocking;
    use crate::resources::Priority;
    use crate::Snapshot;
    use tauri::{AppHandle, Manager};

    #[tauri::command(rename_all = "snake_case")]
    pub async fn home_project_facts(
        app: AppHandle,
        path: String,
    ) -> Result<super::HomeFacts, String> {
        blocking(app, Priority::P2, move |_| super::project_facts(&path)).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn home_default_dir(app: AppHandle) -> Option<String> {
        let paths = app.path();
        super::default_projects_dir(paths.document_dir().ok(), paths.home_dir().ok())
    }

    #[tauri::command(rename_all = "snake_case")]
    pub fn home_suggest_folder(parent: String, name: String) -> Result<String, String> {
        super::suggest_folder(&parent, &name)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn create_project_with_brief(
        app: AppHandle,
        path: String,
        name: String,
        brief: Vec<String>,
    ) -> Result<Snapshot, String> {
        blocking(app, Priority::P1, move |s| {
            super::create_project_with_brief(s, &path, &name, &brief)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn create_sample_project(app: AppHandle, path: String) -> Result<Snapshot, String> {
        blocking(app, Priority::P1, move |s| {
            super::create_sample_project(s, &path)
        })
        .await
    }
}
