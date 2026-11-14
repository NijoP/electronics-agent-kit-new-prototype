//! # eak-desktop — Tauri 2 shell over the shared command engine
//!
//! The IPC layer is a set of plain Rust functions over [`Session`], so the whole contract
//! is tested headless (`tests/ipc.rs`). The `#[tauri::command]` wrappers in [`cmd`] only
//! forward to them.
//!
//! Rules this layer enforces:
//! * every design mutation is an `Engine::execute` / `undo` / `redo` call with the
//!   **Human** actor and the revision the UI last rendered (stale requests are rejected
//!   by the engine, never rebased silently);
//! * the UI never receives a writable model: it renders the [`Snapshot`] (canonical
//!   EAK-IR plus geometry derived from it by `eak-core`) and re-fetches after each call;
//! * ERC/DRC run in `eak-validation`, and their revision-bound reports are stored as
//!   evidence by the engine.

pub mod ai;
pub mod models;

use eak_core::{Id, Nm, PadKind, PadShape, Point, Project, Rotation, Side};
use eak_runtime::index::{Index, RecentProject};
use eak_runtime::store::RecoveryReport;
use eak_runtime::{Actor, Command, CommandRequest, CommandResponse, Engine, Event};
use eak_validation::{drc, erc, Report};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Process-wide desktop state: at most one open project engine and the metadata index.
pub struct Session {
    engine: Mutex<Option<Engine>>,
    /// SQLite recent-project index. It is a rebuildable cache, so failing to open it
    /// disables the recent list but never blocks project work.
    index: Result<Mutex<Index>, String>,
}

impl Session {
    pub fn new(index: Result<Index, String>) -> Session {
        Session {
            engine: Mutex::new(None),
            index: index.map(Mutex::new),
        }
    }

    fn engine(&self) -> Result<MutexGuard<'_, Option<Engine>>, String> {
        self.engine
            .lock()
            .map_err(|_| "engine state is poisoned; restart EAK".to_string())
    }

    fn with_engine<T>(&self, f: impl FnOnce(&mut Engine) -> T) -> Result<T, String> {
        let mut guard = self.engine()?;
        let engine = guard.as_mut().ok_or("no project is open")?;
        Ok(f(engine))
    }

    fn touch_recent(&self, engine: &Engine) {
        let (Ok(index), Some(dir)) = (&self.index, engine.dir()) else {
            return;
        };
        let p = engine.project();
        let result = index
            .lock()
            .map_err(|_| "index lock poisoned".to_string())
            .and_then(|i| i.touch_project(&dir.display().to_string(), &p.id, &p.name, p.revision));
        if let Err(e) = result {
            // The index is a cache (Spec §2): a failed update is reported, not fatal.
            eprintln!("eak-desktop: recent-project index not updated: {e}");
        }
    }
}

/// Absolute pin position, computed by `eak-core` (`Project::pin_position`).
#[derive(Debug, Clone, Serialize)]
pub struct PinGeom {
    pub number: String,
    pub name: String,
    pub position: Point,
}

/// A schematic symbol with its body extents already turned by its rotation.
#[derive(Debug, Clone, Serialize)]
pub struct SymbolGeom {
    pub id: Id,
    pub sheet_id: Id,
    pub refdes: String,
    pub value: String,
    pub position: Point,
    pub half_w: Nm,
    pub half_h: Nm,
    pub pins: Vec<PinGeom>,
}

/// Absolute pad, computed by `eak-core` (`Project::pad_position`).
#[derive(Debug, Clone, Serialize)]
pub struct PadGeom {
    pub number: String,
    pub kind: PadKind,
    pub shape: PadShape,
    pub position: Point,
    pub size_x: Nm,
    pub size_y: Nm,
    pub drill: Option<Nm>,
    pub net: Option<String>,
}

/// A footprint with its courtyard extents already turned by its rotation.
#[derive(Debug, Clone, Serialize)]
pub struct FootprintGeom {
    pub id: Id,
    /// Cross-probe key shared with the schematic symbol (ADR-0002).
    pub symbol_id: Id,
    pub refdes: String,
    pub side: Side,
    pub position: Point,
    pub half_w: Nm,
    pub half_h: Nm,
    pub pads: Vec<PadGeom>,
}

/// Placed geometry derived from the EAK-IR on every request; never stored.
#[derive(Debug, Clone, Serialize)]
pub struct Geometry {
    pub symbols: Vec<SymbolGeom>,
    pub footprints: Vec<FootprintGeom>,
}

/// Everything the UI renders. Re-fetched after each mutation; the UI keeps no model.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub project: Project,
    pub dir: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    /// Set while an interrupted operation with an unknown outcome blocks writes.
    pub blocked: Option<String>,
    pub recovery: RecoveryReport,
    pub geometry: Geometry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Check {
    Erc,
    Drc,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub report: Report,
    /// Revision-bound evidence file written by the engine.
    pub evidence_path: Option<String>,
}

fn quarter_turned(r: Rotation) -> bool {
    matches!(r, Rotation::R90 | Rotation::R270)
}

fn extents(rot: Rotation, w: Nm, h: Nm) -> (Nm, Nm) {
    if quarter_turned(rot) {
        (h, w)
    } else {
        (w, h)
    }
}

/// Derive placed geometry from the canonical project.
pub fn geometry(p: &Project) -> Geometry {
    let symbols = p
        .schematic
        .symbols
        .values()
        .map(|s| {
            let def = p.library.get(&s.component_id);
            let (half_w, half_h) = def
                .map(|d| extents(s.rotation, d.symbol.body_half_w, d.symbol.body_half_h))
                .unwrap_or((0, 0));
            let pins = def
                .map(|d| {
                    d.symbol
                        .pins
                        .iter()
                        .filter_map(|pin| {
                            Some(PinGeom {
                                number: pin.number.clone(),
                                name: pin.name.clone(),
                                position: p.pin_position(s, &pin.number)?,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            SymbolGeom {
                id: s.id.clone(),
                sheet_id: s.sheet_id.clone(),
                refdes: s.refdes.clone(),
                value: s.value.clone(),
                position: s.position,
                half_w,
                half_h,
                pins,
            }
        })
        .collect();
    let footprints = p
        .pcb
        .footprints
        .values()
        .map(|f| {
            let def = p.library.get(&f.component_id);
            let (half_w, half_h) = def
                .map(|d| {
                    extents(
                        f.rotation,
                        d.footprint.courtyard_half_w,
                        d.footprint.courtyard_half_h,
                    )
                })
                .unwrap_or((0, 0));
            let pads = def
                .map(|d| {
                    d.footprint
                        .pads
                        .iter()
                        .map(|pad| {
                            let (size_x, size_y) = extents(f.rotation, pad.size_x, pad.size_y);
                            PadGeom {
                                number: pad.number.clone(),
                                kind: pad.kind,
                                shape: pad.shape,
                                position: p.pad_position(f, pad),
                                size_x,
                                size_y,
                                drill: pad.drill,
                                net: f.pad_nets.get(&pad.number).cloned(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            FootprintGeom {
                id: f.id.clone(),
                symbol_id: f.symbol_id.clone(),
                refdes: f.refdes.clone(),
                side: f.side,
                position: f.position,
                half_w,
                half_h,
                pads,
            }
        })
        .collect();
    Geometry {
        symbols,
        footprints,
    }
}

pub fn snapshot(e: &Engine) -> Snapshot {
    Snapshot {
        project: e.project().clone(),
        dir: e.dir().map(|d| d.display().to_string()),
        can_undo: e.can_undo(),
        can_redo: e.can_redo(),
        blocked: e.blocked_reason().map(String::from),
        recovery: e.recovery.clone(),
        geometry: geometry(e.project()),
    }
}

/// Project folders must be given as absolute paths so nothing is written relative to
/// whatever directory the app happened to start in.
fn project_dir(path: &str) -> Result<PathBuf, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("enter a project folder path".into());
    }
    let dir = Path::new(path);
    if !dir.is_absolute() {
        return Err(format!("project folder must be an absolute path: {path}"));
    }
    Ok(dir.to_path_buf())
}

/// Replace the open project. The previous engine is dropped first so its project lock
/// is released (reopening the same folder must not collide with our own lock).
fn install(s: &Session, open: impl FnOnce() -> Result<Engine, String>) -> Result<Snapshot, String> {
    let mut guard = s.engine()?;
    *guard = None;
    let engine = open()?;
    s.touch_recent(&engine);
    let snap = snapshot(&engine);
    *guard = Some(engine);
    Ok(snap)
}

pub fn create_project(s: &Session, path: &str, name: &str) -> Result<Snapshot, String> {
    let dir = project_dir(path)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("enter a project name".into());
    }
    install(s, || Engine::create(&dir, name))
}

pub fn open_project(s: &Session, path: &str, take_over_lock: bool) -> Result<Snapshot, String> {
    let dir = project_dir(path)?;
    install(s, || Engine::open(&dir, take_over_lock))
}

pub fn close_project(s: &Session) -> Result<(), String> {
    *s.engine()? = None;
    Ok(())
}

pub fn current_project(s: &Session) -> Result<Snapshot, String> {
    s.with_engine(|e| snapshot(e))
}

/// Execute one typed command as the Human actor. The UI cannot choose the actor or
/// attach an approval: agent requests never enter through this boundary.
pub fn execute_command(
    s: &Session,
    expected_revision: u64,
    command: Command,
) -> Result<CommandResponse, String> {
    s.with_engine(|e| {
        e.execute(CommandRequest {
            expected_revision,
            actor: Actor::human(),
            command,
            approval: None,
        })
    })
}

pub fn undo(s: &Session, expected_revision: u64) -> Result<CommandResponse, String> {
    s.with_engine(|e| e.undo(expected_revision, &Actor::human()))
}

pub fn redo(s: &Session, expected_revision: u64) -> Result<CommandResponse, String> {
    s.with_engine(|e| e.redo(expected_revision, &Actor::human()))
}

pub fn events_since(s: &Session, seq: u64) -> Result<Vec<Event>, String> {
    s.with_engine(|e| e.events_since(seq))
}

pub fn acknowledge_recovery(s: &Session) -> Result<Snapshot, String> {
    s.with_engine(|e| {
        e.acknowledge_recovery();
        snapshot(e)
    })
}

/// Run ERC or DRC on the current revision and record the report as evidence.
pub fn run_check(s: &Session, check: Check) -> Result<CheckResult, String> {
    s.with_engine(|e| {
        let report = match check {
            Check::Erc => erc::run(e.project()),
            Check::Drc => drc::run(e.project()),
        };
        let json = serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?;
        let evidence_path = match e.dir() {
            Some(_) => Some(
                e.save_evidence(&report.check, &json)
                    .ok_or_else(|| {
                        format!("{} ran but its evidence could not be saved", report.check)
                    })?
                    .display()
                    .to_string(),
            ),
            None => None,
        };
        Ok(CheckResult {
            report,
            evidence_path,
        })
    })?
}

/// Domain classification of the open project (DESK domain intelligence, `eak-intelligence`).
/// Deterministic, needs no model or API key, and never evaluates DESK rules: the result lists
/// what is not implemented and caps guidance at each class's DESK maturity label.
/// The classification plus the revision it was computed from (the UI marks it stale otherwise).
#[derive(Debug, Clone, Serialize)]
pub struct DomainReport {
    pub revision: u64,
    pub classification: eak_intelligence::domain::DomainClassification,
}

pub fn domain_classification(s: &Session) -> Result<DomainReport, String> {
    s.with_engine(|e| DomainReport {
        revision: e.project().revision,
        classification: eak_intelligence::domain::classify_project(e.project()),
    })
}

/// Physical connectivity of the current revision: per-net copper groups, shorts and the
/// ratsnest (unrouted pad-to-pad connections). Read-only; computed by eak-engineering, the same
/// analysis DRC uses, so the airwires the UI draws are the ones DRC-004 reports.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectivityReport {
    pub revision: u64,
    #[serde(flatten)]
    pub connectivity: eak_engineering::copper::Connectivity,
}

pub fn connectivity(s: &Session) -> Result<ConnectivityReport, String> {
    s.with_engine(|e| ConnectivityReport {
        revision: e.project().revision,
        connectivity: eak_engineering::copper::analyse(e.project()),
    })
}

/// One interactive route step (read-only): a legal path for `net` on `layer` from the last
/// committed point to the cursor, computed by `eak_engineering::interactive::route_step` against
/// the current revision's exact copper geometry and net-class rules. The route tool commits the
/// accepted segments and via as one `Batch` of `AddTrack`/`AddVia` commands.
#[derive(Debug, Clone, Serialize)]
pub struct RouteStepReport {
    pub revision: u64,
    #[serde(flatten)]
    pub step: eak_engineering::interactive::RouteStepResult,
}

pub fn route_step(
    s: &Session,
    request: &eak_engineering::interactive::RouteStepRequest,
) -> Result<RouteStepReport, String> {
    s.with_engine(|e| RouteStepReport {
        revision: e.project().revision,
        step: eak_engineering::interactive::route_step(e.project(), request),
    })
}

/// A built-in library part the UI can place (read-only catalogue; placing it is a `PlaceSymbol` command).
#[derive(Debug, Clone, Serialize)]
pub struct LibraryEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub refdes_prefix: String,
    pub default_value: String,
    pub pins: Vec<(String, String)>,
}

pub fn library_components() -> Vec<LibraryEntry> {
    eak_core::builtin::components()
        .into_iter()
        .map(|c| LibraryEntry {
            pins: c
                .symbol
                .pins
                .iter()
                .map(|p| (p.number.clone(), p.name.clone()))
                .collect(),
            id: c.id,
            name: c.name,
            category: c.category,
            refdes_prefix: c.refdes_prefix,
            default_value: c.default_value,
        })
        .collect()
}

/// Result of a manufacturing export: the manifest plus the revision-bound check reports.
#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub out_dir: String,
    pub manifest: eak_manufacturing::Manifest,
    pub reports: Vec<Report>,
}

/// Run ERC, DRC and MECH on the current revision (saving each as evidence), then write
/// Gerber, Excellon, BOM and CPL into `<project>/output/rev-<N>`. The package status is
/// DRAFT unless every gate passes; it is never called manufacturing-ready.
pub fn export_manufacturing(s: &Session) -> Result<ExportResult, String> {
    s.with_engine(|e| {
        let dir = e
            .dir()
            .ok_or("export needs a saved project folder")?
            .to_path_buf();
        let reports = vec![
            erc::run(e.project()),
            drc::run(e.project()),
            eak_validation::mech::run(e.project()),
        ];
        for r in &reports {
            let json = serde_json::to_string_pretty(r).map_err(|err| err.to_string())?;
            e.save_evidence(&r.check, &json)
                .ok_or_else(|| format!("{} evidence could not be saved", r.check))?;
        }
        let out = dir
            .join("output")
            .join(format!("rev-{}", e.project().revision));
        let refs: Vec<&Report> = reports.iter().collect();
        let manifest = eak_manufacturing::compile(e.project(), &refs, &out)?;
        Ok(ExportResult {
            out_dir: out.display().to_string(),
            manifest,
            reports,
        })
    })?
}

pub fn recent_projects(s: &Session, limit: usize) -> Result<Vec<RecentProject>, String> {
    let index = s
        .index
        .as_ref()
        .map_err(|e| format!("recent-project index unavailable: {e}"))?;
    index
        .lock()
        .map_err(|_| "index lock poisoned".to_string())?
        .recent(limit)
}

/// Names of every IPC command registered with Tauri; `tests/ipc.rs` checks that the
/// frontend (`src/api.ts`) invokes exactly these.
pub const IPC_COMMANDS: [&str; 27] = [
    "ai_propose",
    "ai_accept",
    "ai_reject",
    "ai_rollback",
    "connectivity",
    "route_step",
    "library_components",
    "domain_classification",
    "export_manufacturing",
    "create_project",
    "open_project",
    "close_project",
    "current_project",
    "execute_command",
    "undo",
    "redo",
    "events_since",
    "acknowledge_recovery",
    "run_check",
    "recent_projects",
    "model_providers",
    "model_save_provider",
    "model_remove_provider",
    "model_set_key",
    "model_clear_key",
    "model_list_models",
    "model_test_connection",
];

/// Tauri command wrappers: argument decoding only, no logic.
pub mod cmd {
    use super::*;
    use tauri::State;

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn create_project(
        state: State<'_, Session>,
        path: String,
        name: String,
    ) -> Result<Snapshot, String> {
        super::create_project(&state, &path, &name)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn open_project(
        state: State<'_, Session>,
        path: String,
        take_over_lock: bool,
    ) -> Result<Snapshot, String> {
        super::open_project(&state, &path, take_over_lock)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn close_project(state: State<'_, Session>) -> Result<(), String> {
        super::close_project(&state)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn current_project(state: State<'_, Session>) -> Result<Snapshot, String> {
        super::current_project(&state)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn execute_command(
        state: State<'_, Session>,
        expected_revision: u64,
        command: Command,
    ) -> Result<CommandResponse, String> {
        super::execute_command(&state, expected_revision, command)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn undo(
        state: State<'_, Session>,
        expected_revision: u64,
    ) -> Result<CommandResponse, String> {
        super::undo(&state, expected_revision)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn redo(
        state: State<'_, Session>,
        expected_revision: u64,
    ) -> Result<CommandResponse, String> {
        super::redo(&state, expected_revision)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn events_since(state: State<'_, Session>, seq: u64) -> Result<Vec<Event>, String> {
        super::events_since(&state, seq)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn acknowledge_recovery(state: State<'_, Session>) -> Result<Snapshot, String> {
        super::acknowledge_recovery(&state)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn run_check(state: State<'_, Session>, check: Check) -> Result<CheckResult, String> {
        super::run_check(&state, check)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn domain_classification(state: State<'_, Session>) -> Result<DomainReport, String> {
        super::domain_classification(&state)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn connectivity(state: State<'_, Session>) -> Result<ConnectivityReport, String> {
        super::connectivity(&state)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn route_step(
        state: State<'_, Session>,
        request: eak_engineering::interactive::RouteStepRequest,
    ) -> Result<RouteStepReport, String> {
        super::route_step(&state, &request)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn library_components() -> Vec<LibraryEntry> {
        super::library_components()
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn export_manufacturing(state: State<'_, Session>) -> Result<ExportResult, String> {
        super::export_manufacturing(&state)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn recent_projects(
        state: State<'_, Session>,
        limit: usize,
    ) -> Result<Vec<RecentProject>, String> {
        super::recent_projects(&state, limit)
    }
}

/// Start the desktop application.
pub fn run() {
    use tauri::Manager;
    tauri::Builder::default()
        .setup(|app| {
            let index = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())
                .and_then(|d| Index::open(&d.join("index.sqlite")));
            if let Err(e) = &index {
                eprintln!("eak-desktop: recent-project index unavailable: {e}");
            }
            app.manage(Session::new(index));
            app.manage(ai::AiState::default());
            // Profiles are plain JSON in the config dir; keys go to the OS credential store.
            let profiles = app
                .path()
                .app_config_dir()
                .ok()
                .map(|d| d.join("model-providers.json"));
            match models::ModelSettings::system(profiles) {
                Ok(m) => {
                    app.manage(m);
                }
                Err(e) => {
                    eprintln!("eak-desktop: model settings unavailable: {e}");
                    app.manage(models::ModelSettings::system(None).map_err(|e| e.to_string())?);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cmd::create_project,
            cmd::open_project,
            cmd::close_project,
            cmd::current_project,
            cmd::execute_command,
            cmd::undo,
            cmd::redo,
            cmd::events_since,
            cmd::acknowledge_recovery,
            cmd::run_check,
            cmd::recent_projects,
            models::cmd::model_providers,
            models::cmd::model_save_provider,
            models::cmd::model_remove_provider,
            models::cmd::model_set_key,
            models::cmd::model_clear_key,
            models::cmd::model_list_models,
            models::cmd::model_test_connection,
            cmd::domain_classification,
            cmd::library_components,
            cmd::connectivity,
            cmd::route_step,
            cmd::export_manufacturing,
            ai::cmd::ai_propose,
            ai::cmd::ai_accept,
            ai::cmd::ai_reject,
            ai::cmd::ai_rollback,
        ])
        .run(tauri::generate_context!())
        .expect("EAK desktop failed to start");
}
