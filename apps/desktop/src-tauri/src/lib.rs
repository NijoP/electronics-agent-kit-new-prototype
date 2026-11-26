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

pub mod agents;
pub mod ai;
pub mod catalogue;
pub mod eco;
pub mod home;
pub mod kicad;
pub mod libcatalog;
pub mod library_import;
pub mod libstore;
pub mod models;
pub mod parts;
pub mod resources;
pub mod sync;
pub mod terminal;
pub mod workspace;

use eak_core::{Id, Nm, PadKind, PadShape, Point, Project, Rotation, Side};
use eak_runtime::index::{Index, RecentProject};
use eak_runtime::store::{LockState, RecoveryReport};
use eak_runtime::{Actor, Command, CommandRequest, CommandResponse, Engine, Event};
use eak_validation::{drc, erc, Report};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

/// Every project open in this window plus which one is active. All IPC commands act on the
/// active engine only, so contexts never mix; switching changes which engine that is.
#[derive(Default)]
pub struct Open {
    engines: Vec<Engine>,
    active: Option<usize>,
}

impl Open {
    fn active_engine(&mut self) -> Option<&mut Engine> {
        let i = self.active?;
        self.engines.get_mut(i)
    }
}

/// Process-wide desktop state: the open project engines (one active) and the metadata index.
pub struct Session {
    engine: Mutex<Open>,
    /// SQLite recent-project index. It is a rebuildable cache, so failing to open it
    /// disables the recent list but never blocks project work.
    index: Result<Mutex<Index>, String>,
    /// Harness endpoint for terminal agents (ADR-0018); advertised in the open project's folder.
    harness: Mutex<Option<eak_harness::endpoint::Server>>,
    /// Immutable copy of the active project at one revision, shared by read-only analysis so
    /// DRC, connectivity, route previews and exports run without holding the engine lock
    /// (`docs/development/PERFORMANCE_LEDGER.md` L1). Cloned at most once per revision.
    shared: Mutex<Option<(ProjectKey, Arc<Project>)>>,
}

/// Which project state a shared copy or a background result belongs to: folder, id, revision.
/// A result is applied (evidence saved) only while the active engine still has the same key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectKey {
    dir: Option<PathBuf>,
    id: Id,
    revision: u64,
}

impl ProjectKey {
    fn of(e: &Engine) -> ProjectKey {
        ProjectKey {
            dir: e.dir().map(Path::to_path_buf),
            id: e.project().id.clone(),
            revision: e.project().revision,
        }
    }
}

impl Session {
    pub fn new(index: Result<Index, String>) -> Session {
        Session {
            engine: Mutex::new(Open::default()),
            index: index.map(Mutex::new),
            harness: Mutex::new(None),
            shared: Mutex::new(None),
        }
    }

    /// Advertise the harness endpoint in the open project's folder, withdrawing it from the
    /// previously open one.
    fn advertise(&self, engine: Option<&Engine>, previous: Option<PathBuf>) {
        if let Some(dir) = previous {
            eak_harness::endpoint::remove_record(&dir);
        }
        let (Some(e), Ok(guard)) = (engine, self.harness.lock()) else {
            return;
        };
        if let (Some(server), Some(dir)) = (guard.as_ref(), e.dir()) {
            if let Err(err) = server.write_record(dir, &e.project().id) {
                eprintln!("eak-desktop: harness endpoint not advertised: {err}");
            }
        }
    }

    fn engine(&self) -> Result<MutexGuard<'_, Open>, String> {
        self.engine
            .lock()
            .map_err(|_| "engine state is poisoned; restart EAK".to_string())
    }

    fn with_engine<T>(&self, f: impl FnOnce(&mut Engine) -> T) -> Result<T, String> {
        let mut guard = self.engine()?;
        let engine = guard.active_engine().ok_or("no project is open")?;
        Ok(f(engine))
    }

    /// The active project at its current revision, as a shared immutable copy. The engine lock
    /// is held only to compare the key (and, once per revision, to clone); callers then compute
    /// without it, so edits are never queued behind a long read.
    fn shared_project(&self) -> Result<(ProjectKey, Arc<Project>), String> {
        let mut guard = self.engine()?;
        let engine = guard.active_engine().ok_or("no project is open")?;
        let key = ProjectKey::of(engine);
        // Lock order: engine, then shared (nothing takes them the other way round).
        let mut shared = self.shared.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((k, p)) = shared.as_ref() {
            if *k == key {
                return Ok((key, Arc::clone(p)));
            }
        }
        let project = Arc::new(engine.project().clone());
        *shared = Some((key.clone(), Arc::clone(&project)));
        Ok((key, project))
    }

    /// Run `f` on the active engine only if it is still the state `key` describes; a result
    /// computed for an older revision (or another project) is rejected, never applied.
    fn with_engine_at<T>(
        &self,
        key: &ProjectKey,
        f: impl FnOnce(&mut Engine) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self.engine()?;
        let engine = guard.active_engine().ok_or("no project is open")?;
        if ProjectKey::of(engine) != *key {
            return Err(format!(
                "the project changed while this ran (it was computed for revision {}); run it again",
                key.revision
            ));
        }
        f(engine)
    }

    /// Drop the shared copy when a project closes, so its memory is not retained.
    fn release_shared(&self) {
        *self.shared.lock().unwrap_or_else(|p| p.into_inner()) = None;
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
    /// Design-for-manufacture against the built-in Fabricator standard profile.
    Dfm,
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

/// Normalise a folder path typed or pasted by the user: trim whitespace and strip one pair
/// of matching surrounding quotes. Windows Explorer's "Copy as path" yields `"C:\dir name"`,
/// and a literal `"` is never valid in a Windows path, so the quotes are never meaningful.
pub(crate) fn clean_path_input(path: &str) -> &str {
    let path = path.trim();
    for q in ['"', '\''] {
        if let Some(inner) = path.strip_prefix(q).and_then(|rest| rest.strip_suffix(q)) {
            return inner.trim();
        }
    }
    path
}

/// Project folders must be given as absolute paths so nothing is written relative to
/// whatever directory the app happened to start in.
fn project_dir(path: &str) -> Result<PathBuf, String> {
    let path = clean_path_input(path);
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
fn install(
    s: &Session,
    dir: &Path,
    open: impl FnOnce() -> Result<Engine, String>,
) -> Result<Snapshot, String> {
    let mut guard = s.engine()?;
    // Opening a folder that is already open here re-reads it from disk (its lock is ours, so it
    // is dropped first). Other open projects stay open and untouched.
    if let Some(i) = guard
        .engines
        .iter()
        .position(|e| e.dir().is_some_and(|d| d == dir))
    {
        guard.engines.remove(i);
        guard.active = guard.engines.len().checked_sub(1);
    }
    let engine = open()?;
    s.touch_recent(&engine);
    s.advertise(Some(&engine), None);
    let snap = snapshot(&engine);
    guard.engines.push(engine);
    guard.active = Some(guard.engines.len() - 1);
    Ok(snap)
}

/// Serve one harness request from a terminal agent on the open project's engine. The request
/// must target the project the app has open; anything else is refused, never redirected.
/// `emit` receives a [`sync::RevisionChange`] for every call that moved the project's revision,
/// so the UI learns of agent edits at once (docs/AGENT_HARNESS.md, "Live synchronisation").
pub fn harness_call(
    s: &Session,
    req: &eak_harness::endpoint::Request,
    emit: &dyn Fn(sync::RevisionChange),
) -> serde_json::Value {
    use eak_harness::{envelope_error, ErrorCode};
    let ctx = &req.context;
    let mut guard = match s.engine() {
        Ok(g) => g,
        Err(e) => {
            return envelope_error(
                &req.tool,
                ctx,
                ErrorCode::Internal,
                &e,
                serde_json::Value::Null,
            )
        }
    };
    let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    };
    if guard.engines.is_empty() {
        return envelope_error(
            &req.tool,
            ctx,
            ErrorCode::NoProject,
            "the EAK app has no project open",
            serde_json::Value::Null,
        );
    }
    let wanted = ctx.project.dir.as_deref();
    let found = guard
        .engines
        .iter_mut()
        .find(|e| matches!((e.dir(), wanted), (Some(o), Some(w)) if same(o, w)));
    match found {
        Some(engine) => {
            let before = engine.project().revision;
            let out = eak_harness::handle(engine, ctx, "app", &req.tool, &req.args);
            if let Some(change) =
                sync::revision_change(engine, before, "harness", Some(&ctx.session_id))
            {
                emit(change);
            }
            out
        }
        None => {
            let open: Vec<PathBuf> = guard
                .engines
                .iter()
                .filter_map(|e| e.dir().map(Path::to_path_buf))
                .collect();
            envelope_error(
                &req.tool,
                ctx,
                ErrorCode::ProjectMismatch,
                "the EAK app does not have the project this call targets open",
                serde_json::json!({"app_project_dirs": open}),
            )
        }
    }
}

/// Start the harness endpoint (state files under `state_dir`). The handler reaches the session
/// through `session`, which must outlive the server (the app keeps both for its lifetime).
pub fn start_harness(
    s: &Session,
    state_dir: &Path,
    handler: eak_harness::endpoint::Handler,
) -> Result<(), String> {
    let server =
        eak_harness::endpoint::Server::start(state_dir, handler).map_err(|e| e.to_string())?;
    *s.harness.lock().map_err(|_| "harness lock poisoned")? = Some(server);
    Ok(())
}

/// Endpoint and token-file path for the terminal workspace to export as `EAK_HARNESS_SOCKET` /
/// `EAK_HARNESS_TOKEN_FILE`; `None` when the endpoint is not running.
#[derive(Debug, Clone, Serialize)]
pub struct HarnessEndpoint {
    pub endpoint: String,
    pub token_file: String,
}

pub fn harness_endpoint(s: &Session) -> Option<HarnessEndpoint> {
    let guard = s.harness.lock().ok()?;
    guard.as_ref().map(|srv| HarnessEndpoint {
        endpoint: srv.endpoint.clone(),
        token_file: srv.token_file.display().to_string(),
    })
}

pub fn create_project(s: &Session, path: &str, name: &str) -> Result<Snapshot, String> {
    let dir = project_dir(path)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("enter a project name".into());
    }
    install(s, &dir, || Engine::create(&dir, name))
}

pub fn open_project(s: &Session, path: &str, take_over_lock: bool) -> Result<Snapshot, String> {
    let dir = project_dir(path)?;
    install(s, &dir, || Engine::open(&dir, take_over_lock))
}

/// Read-only: what the project's lock means (open elsewhere, interrupted, unverifiable, ...).
pub fn project_lock_state(path: &str) -> Result<LockState, String> {
    Ok(eak_runtime::store::inspect_lock(&project_dir(path)?))
}

/// Close the active project; the most recently opened remaining one (if any) becomes active.
pub fn close_project(s: &Session) -> Result<(), String> {
    let mut guard = s.engine()?;
    if let Some(i) = guard.active {
        let gone = guard.engines.remove(i);
        s.advertise(None, gone.dir().map(Path::to_path_buf));
        s.release_shared();
    }
    guard.active = guard.engines.len().checked_sub(1);
    Ok(())
}

pub fn current_project(s: &Session) -> Result<Snapshot, String> {
    s.with_engine(|e| snapshot(e))
}

/// One schematic net for the Netlist panel: computed by `eak-engineering` (the engine's own
/// connectivity, the same netlist ERC and PCB sync use), never rebuilt in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct NetRow {
    pub name: String,
    /// `R1.1`-style pin references.
    pub pins: Vec<String>,
    pub label_names: Vec<String>,
    pub has_power_port: bool,
}

/// Read-only netlist of the active project at its current revision.
pub fn netlist(s: &Session) -> Result<Vec<NetRow>, String> {
    s.with_engine(|e| {
        eak_engineering::netlist::build(e.project())
            .nets
            .into_values()
            .map(|n| NetRow {
                name: n.name,
                pins: n
                    .pins
                    .iter()
                    .map(|p| format!("{}.{}", p.refdes, p.pin))
                    .collect(),
                label_names: n.label_names,
                has_power_port: n.has_power_port,
            })
            .collect()
    })
}

/// The agent task ledger of the active project (ADR-0036), oldest first.
pub fn agent_tasks(s: &Session) -> Result<Vec<eak_harness::agent_task::AgentTask>, String> {
    s.with_engine(|e| {
        let id = e.project().id.clone();
        e.dir()
            .map(|d| {
                eak_harness::agent_task::read(d)
                    .tasks
                    .into_iter()
                    .filter(|t| t.project_id == id)
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Execute one typed command as the Human actor. The UI cannot choose the actor or
/// attach an approval: agent requests never enter through this boundary.
pub fn execute_command(
    s: &Session,
    expected_revision: u64,
    command: Command,
) -> Result<CommandResponse, String> {
    s.with_engine(|e| {
        // Personal and imported parts are copied into the project library in the same Batch
        // as their placement (F8); the engine itself only knows project and built-in parts.
        let (command, notes) =
            libstore::resolve_scoped_parts(libstore::store(), &e.project().library, command)?;
        let mut response = e.execute(CommandRequest {
            expected_revision,
            actor: Actor::human(),
            command,
            approval: None,
        });
        if response.accepted {
            response.warnings.extend(notes);
        }
        Ok(response)
    })?
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
        e.acknowledge_recovery()?;
        Ok(snapshot(e))
    })?
}

/// Run ERC, DRC or DFM on the current revision and record the report as evidence.
pub fn run_check(s: &Session, check: Check) -> Result<CheckResult, String> {
    // The check runs on the shared copy without the engine lock, so edits stay responsive. Its
    // evidence is saved only if the project is still at the revision it checked: a stale result
    // is rejected, never recorded against a newer revision.
    let (key, p) = s.shared_project()?;
    let report = match check {
        Check::Erc => erc::run(&p),
        Check::Drc => drc::run(&p),
        Check::Dfm => eak_validation::dfm::run(&p, &eak_validation::dfm::fabricator_standard()),
    };
    let json = serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?;
    s.with_engine_at(&key, |e| {
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
    })
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
    let (_, p) = s.shared_project()?;
    Ok(DomainReport {
        revision: p.revision,
        classification: eak_intelligence::domain::classify_project(&p),
    })
}

/// Engineering intelligence summary of the open project (ADR-0027): product classification,
/// active domains, architecture, requirement coverage, missing information, conflicts, domain
/// check outcomes and risks. Read-only and recomputed per call from the design and the stored
/// spec; every value is computed (nothing is estimated for display). The revision is inside.
pub fn intelligence_summary(
    s: &Session,
) -> Result<eak_intelligence::systems::IntelligenceSummary, String> {
    s.with_engine(|e| eak_harness::intel::model(e).map(|m| m.summary()))?
}

/// Physical connectivity of the current revision: per-net copper groups, shorts and the
/// ratsnest (unrouted pad-to-pad connections). Read-only; computed by eak-engineering, the same
/// analysis DRC uses, so the airwires the UI draws are the ones DRC-004 reports.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectivityReport {
    pub revision: u64,
    #[serde(flatten)]
    pub connectivity: eak_engineering::copper::Connectivity,
    /// Each zone's fill (slice E5): the raster DRC and the connectivity above read, as
    /// rectangles the PCB editor draws. Derived, never stored.
    pub zone_fills: Vec<eak_engineering::fill_view::FillView>,
}

pub fn connectivity(s: &Session) -> Result<ConnectivityReport, String> {
    let (_, p) = s.shared_project()?;
    let g = eak_engineering::board::BoardGeometry::build(&p);
    Ok(ConnectivityReport {
        revision: p.revision,
        connectivity: g.connectivity(),
        zone_fills: g.fill_views,
    })
}

/// The board as fabricated for the current revision (ADR-0052): the one geometry DRC checks
/// (outline, cut-outs, copper, zone fills, holes, legend) plus the 3D bodies built from it. The
/// 2D and 3D views draw this; read-only and derived per call, never stored.
#[derive(Debug, Clone, Serialize)]
pub struct BoardGeometryReport {
    #[serde(flatten)]
    pub geometry: eak_engineering::board::BoardGeometry,
    #[serde(flatten)]
    pub scene: eak_engineering::scene3d::Scene,
}

pub fn board_geometry(s: &Session) -> Result<BoardGeometryReport, String> {
    let (_, p) = s.shared_project()?;
    let geometry = eak_engineering::board::BoardGeometry::build(&p);
    let scene = eak_engineering::scene3d::build(&p, &geometry);
    Ok(BoardGeometryReport { geometry, scene })
}

/// Simulation panel data: whether ngspice is installed plus the nets and elements an
/// `eak-simulation` export would use. Read-only; ngspice is optional.
#[derive(Debug, Clone, Serialize)]
pub struct SimulationStatus {
    pub ngspice: eak_simulation::NgspiceStatus,
    pub revision: u64,
    #[serde(flatten)]
    pub circuit: eak_simulation::Describe,
}

pub fn simulation_status(s: &Session) -> Result<SimulationStatus, String> {
    let (_, p) = s.shared_project()?;
    let (revision, circuit) = (p.revision, eak_simulation::describe(&p));
    Ok(SimulationStatus {
        ngspice: eak_simulation::probe(),
        revision,
        circuit,
    })
}

/// Runs ngspice on a snapshot of the current revision. The engine lock is released before the
/// subprocess starts, and nothing is written to the project (simulation is not a design edit).
pub fn run_simulation(
    s: &Session,
    req: &eak_simulation::SimRequest,
) -> Result<eak_simulation::SimResult, String> {
    let (_, project) = s.shared_project()?;
    eak_simulation::run(&project, req).map_err(|e| e.to_string())
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
    let (_, p) = s.shared_project()?;
    Ok(RouteStepReport {
        revision: p.revision,
        step: eak_engineering::interactive::route_step(&p, request),
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

/// Built-in parts for the Place tool, unless the built-in library is disabled in Libraries
/// (ADR-0035: a disabled library is hidden from search and placement).
pub fn library_components() -> Vec<LibraryEntry> {
    library_components_in(libstore::store())
}

pub fn library_components_in(store: &libstore::LibraryStore) -> Vec<LibraryEntry> {
    let off = libcatalog::disabled(store.root());
    if off.contains(&libcatalog::scope_id(
        &eak_manufacturing::library_link::Scope::Builtin,
    )) {
        return vec![];
    }
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

/// One library part as shown by the library panel: every enabled library (built-in, project,
/// personal, imported), one row per id. The row shows the definition a placement will use: the
/// project copy, then the built-in, then the personal copy, then an imported one. Read-only;
/// placing or editing goes through `PlaceSymbol` / `SetComponentProperties` /
/// `AssignComponentFootprint` (a personal or imported part is copied into the project in the
/// same `Batch` as its placement, see `libstore::resolve_scoped_parts`).
#[derive(Debug, Clone, Serialize)]
pub struct LibraryPart {
    pub id: String,
    /// `builtin`, `project`, `personal` or `imported:<library name>`.
    pub source: String,
    /// True when the project library already holds a copy (placing will not import).
    pub in_project: bool,
    pub def: eak_core::ComponentDef,
    /// Ratings still lacking a verified source; never reported as verified.
    pub unverified_ratings: usize,
}

/// Search the merged library. `source` is `all`, `builtin`, `project`, `personal`, `imported`
/// (any imported library) or `imported:<name>`; `category` an exact category or empty for any.
/// Terms are AND-matched by `ComponentDef::matches_query`. Disabled libraries are left out.
pub fn library_search(
    s: &Session,
    query: &str,
    category: &str,
    source: &str,
) -> Result<Vec<LibraryPart>, String> {
    let (_, p) = s.shared_project()?;
    Ok(merged_library(
        libstore::store(),
        &p.library,
        query,
        category,
        source,
    ))
}

pub fn merged_library(
    store: &libstore::LibraryStore,
    project: &std::collections::BTreeMap<String, eak_core::ComponentDef>,
    query: &str,
    category: &str,
    source: &str,
) -> Vec<LibraryPart> {
    use eak_manufacturing::library_link::Scope;
    let rank = |sc: &Scope| match sc {
        Scope::Project => 0,
        Scope::Builtin => 1,
        Scope::Personal => 2,
        Scope::Imported(_) => 3,
    };
    let mut defs = libstore::all_defs(store, project);
    defs.sort_by_key(|(sc, _)| rank(sc));
    let mut parts: std::collections::BTreeMap<String, LibraryPart> = Default::default();
    for (sc, def) in defs {
        if parts.contains_key(&def.id) {
            continue;
        }
        parts.insert(
            def.id.clone(),
            LibraryPart {
                id: def.id.clone(),
                source: sc.label(),
                in_project: project.contains_key(&def.id),
                unverified_ratings: def.ratings.iter().filter(|r| !r.verified).count(),
                def,
            },
        );
    }
    parts
        .into_values()
        .filter(|p| {
            source == "all"
                || source.is_empty()
                || p.source == source
                || (source == "imported" && p.source.starts_with("imported:"))
        })
        .filter(|p| category.is_empty() || p.def.category == category)
        .filter(|p| p.def.matches_query(query))
        .collect()
}

/// A footprint (with a pin map) that can be assigned to a component without breaking the
/// definition: assigning it passes `ComponentDef::validate`.
#[derive(Debug, Clone, Serialize)]
pub struct FootprintCandidate {
    pub name: String,
    pub donor: String,
    pub footprint: eak_core::FootprintDef,
    pub pin_map: std::collections::BTreeMap<String, Vec<String>>,
}

/// Largest 3D model file the viewer will load (guards the IPC channel and the WASM heap).
const MAX_MODEL_BYTES: u64 = 64 * 1024 * 1024;

/// Raw bytes of a user-chosen 3D model file (`.step`/`.stp`/`.wrl`/`.glb`). The path is the
/// one stored in the component's `ModelRef`; only an absolute path with a model extension
/// is read, so this cannot be used to read arbitrary project or system files.
pub fn read_model_file(path: &str) -> Result<Vec<u8>, String> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err("3D model path must be absolute".into());
    }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
    if eak_core::ModelFormat::from_extension(ext).is_none() {
        return Err("not a supported 3D model file (.step, .stp, .wrl, .glb)".into());
    }
    let meta = std::fs::metadata(p).map_err(|e| format!("cannot read 3D model: {e}"))?;
    if !meta.is_file() {
        return Err("3D model path is not a file".into());
    }
    if meta.len() > MAX_MODEL_BYTES {
        return Err(format!(
            "3D model is larger than {} MiB",
            MAX_MODEL_BYTES / 1024 / 1024
        ));
    }
    std::fs::read(p).map_err(|e| format!("cannot read 3D model: {e}"))
}

/// Footprints of other library parts whose pin map fits `component_id`'s symbol pins.
pub fn footprint_candidates(
    s: &Session,
    component_id: &str,
) -> Result<Vec<FootprintCandidate>, String> {
    let (_, shared) = s.shared_project()?;
    let project = &shared.library;
    let target = project
        .get(component_id)
        .cloned()
        .or_else(|| eak_core::builtin::get(component_id))
        .ok_or_else(|| format!("unknown component {component_id}"))?;
    let mut out = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for p in merged_library(libstore::store(), project, "", "", "all") {
        if p.id == target.id || !seen.insert(p.def.footprint.name.clone()) {
            continue;
        }
        let mut t = target.clone();
        t.footprint = p.def.footprint.clone();
        t.pin_map = p.def.pin_map.clone();
        if t.validate().is_empty() && p.def.footprint.name != target.footprint.name {
            out.push(FootprintCandidate {
                name: p.def.footprint.name.clone(),
                donor: p.id.clone(),
                footprint: t.footprint,
                pin_map: t.pin_map,
            });
        }
    }
    Ok(out)
}
/// Result of a manufacturing export: the manifest plus the revision-bound check reports.
#[derive(Debug, Clone, Serialize)]
pub struct ExportResult {
    pub out_dir: String,
    pub manifest: eak_manufacturing::Manifest,
    pub reports: Vec<Report>,
}

/// Run ERC, DRC, MECH and DFM on the current revision (saving each as evidence), then write
/// Gerber, Excellon, BOM, CPL and the zipped fab pack with its README into `<project>/output/rev-<N>`. The package status is
/// DRAFT unless every gate passes; it is never called manufacturing-ready.
pub fn export_manufacturing(s: &Session) -> Result<ExportResult, String> {
    // Checks and file generation run on the shared copy of one revision, without the engine
    // lock. The lock is taken once to save the evidence, and only if the project is still at
    // that revision; the output goes to `rev-<N>` of the revision that was checked.
    let (key, p) = s.shared_project()?;
    let dir = key
        .dir
        .clone()
        .ok_or("export needs a saved project folder")?;
    let reports = release_reports(s, &key, &p)?;
    let out = dir.join("output").join(format!("rev-{}", p.revision));
    let refs: Vec<&Report> = reports.iter().collect();
    let manifest = eak_manufacturing::compile(&p, &refs, &out)?;
    Ok(ExportResult {
        out_dir: out.display().to_string(),
        manifest,
        reports,
    })
}

/// The BOM table a configuration produces for the current revision (the BOM dialog's preview;
/// the same function writes the exported file).
pub fn bom_table(
    s: &Session,
    config: &eak_manufacturing::bom_config::BomConfig,
) -> Result<eak_manufacturing::bom_config::BomTable, String> {
    let (_, p) = s.shared_project()?;
    eak_manufacturing::bom_config::table(&p, &parts::stored_records(s), config)
}

/// Export a configured BOM (CSV or XLSX, plus its configuration JSON) into
/// `<project>/output/rev-<N>/bom` with a revision-bound manifest; DRAFT unless the release
/// gate (ERC, DRC, MECH, DFM on this revision) passes.
pub fn export_bom(
    s: &Session,
    bom: eak_manufacturing::outputs::BomRequest,
) -> Result<ExportResult, String> {
    let request = eak_manufacturing::outputs::OutputRequest {
        bom: Some(bom),
        ..Default::default()
    };
    export_outputs(s, "bom", &request)
}

/// Export schematic PDF/SVG and IPC-D-356/JSON netlists into
/// `<project>/output/rev-<N>/documents` with a revision-bound manifest. A BOM in the request
/// is ignored here (it has its own dialog and folder).
pub fn export_documents(
    s: &Session,
    mut request: eak_manufacturing::outputs::OutputRequest,
) -> Result<ExportResult, String> {
    request.bom = None;
    export_outputs(s, "documents", &request)
}

/// Result of [`export_findings`]: the file written and the check it wrote, so the Findings
/// panel shows exactly the report that was exported.
#[derive(Debug, Clone, Serialize)]
pub struct FindingsExport {
    pub path: String,
    pub result: CheckResult,
}

/// Phase 1 B5: run `check` on the current revision (saving its evidence, as a check run does) and
/// write that report as CSV or JSON into `<project>/output/rev-<N>/findings`. The report is the
/// engine's, recomputed here, never the UI's copy.
pub fn export_findings(
    s: &Session,
    check: Check,
    format: eak_manufacturing::findings_export::FindingsFormat,
) -> Result<FindingsExport, String> {
    let (key, _) = s.shared_project()?;
    let dir = key.dir.ok_or("export needs a saved project folder")?;
    let result = run_check(s, check)?;
    let out = dir
        .join("output")
        .join(format!("rev-{}", result.report.revision))
        .join("findings");
    let path = eak_manufacturing::findings_export::export_findings(&result.report, &out, format)?;
    Ok(FindingsExport {
        path: path.display().to_string(),
        result,
    })
}

fn export_outputs(
    s: &Session,
    folder: &str,
    request: &eak_manufacturing::outputs::OutputRequest,
) -> Result<ExportResult, String> {
    if request.is_empty() {
        return Err("choose at least one output to export".into());
    }
    let (key, p) = s.shared_project()?;
    let dir = key
        .dir
        .clone()
        .ok_or("export needs a saved project folder")?;
    let reports = release_reports(s, &key, &p)?;
    let out = dir
        .join("output")
        .join(format!("rev-{}", p.revision))
        .join(folder);
    let refs: Vec<&Report> = reports.iter().collect();
    let manifest =
        eak_manufacturing::outputs::export(&p, &refs, &parts::stored_records(s), &out, request)?;
    Ok(ExportResult {
        out_dir: out.display().to_string(),
        manifest,
        reports,
    })
}

/// ERC, DRC, MECH and DFM on the shared copy of one revision, each saved as evidence (under
/// the engine lock, only if the project is still at that revision).
fn release_reports(s: &Session, key: &ProjectKey, p: &Project) -> Result<Vec<Report>, String> {
    let reports = vec![
        erc::run(p),
        drc::run(p),
        eak_validation::mech::run(p),
        eak_validation::dfm::run(p, &eak_validation::dfm::fabricator_standard()),
    ];
    let mut evidence = Vec::with_capacity(reports.len());
    for r in &reports {
        evidence.push(serde_json::to_string_pretty(r).map_err(|err| err.to_string())?);
    }
    s.with_engine_at(key, |e| {
        for (r, json) in reports.iter().zip(&evidence) {
            e.save_evidence(&r.check, json)
                .ok_or_else(|| format!("{} evidence could not be saved", r.check))?;
        }
        Ok(())
    })?;
    Ok(reports)
}

/// One row of the release check (slice E7): a revision-bound check report or an engineering gate.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseGate {
    pub id: String,
    pub title: String,
    /// `PASS`, `FAIL`, `UNKNOWN`, `NOT_RUN`, or `DISABLED` (shown but not counted by the release).
    pub state: String,
    /// A required row blocks the release unless it is `PASS`; an advisory row is reported only.
    pub required: bool,
    pub reasons: Vec<String>,
    /// What the state rests on (the saved report, or the gate's own evidence).
    pub evidence: Vec<String>,
}

/// Read-only release check of the current revision (slice E7): the ERC, DRC, MECH and DFM reports
/// saved for exactly this revision and the fail-closed engineering gates (`eak_validation::gates`),
/// with the blocking reasons and the status an export would get from those same reports.
#[derive(Debug, Clone, Serialize)]
pub struct ReleaseCheck {
    pub revision: u64,
    pub input_sha256: String,
    pub gates: Vec<ReleaseGate>,
    /// `eak_manufacturing::release_gates` for this revision: the export's own blocking reasons.
    pub blocking: Vec<String>,
    /// DRAFT unless every required row is `PASS` and nothing blocks.
    pub status: eak_manufacturing::PackageStatus,
    pub not_verified: Vec<String>,
}

const RELEASE_REPORTS: [(&str, &str); 4] = [
    ("ERC", "Electrical rules check (ERC) on this revision"),
    ("DRC", "Design rules check (DRC) on this revision"),
    ("MECH", "Mechanical fit check (MECH) on this revision"),
    (
        "DFM",
        "Design for manufacture (DFM, Fabricator standard) on this revision",
    ),
];

/// The saved `check` report for exactly this project, revision and input; `None` when it was
/// never run, is unreadable, or speaks for another revision or input.
fn current_evidence(dir: Option<&Path>, check: &str, p: &Project) -> Option<Report> {
    let file = format!("{}-r{:06}.json", check.to_lowercase(), p.revision);
    let path = dir?.join("evidence").join(file);
    let text = std::fs::read_to_string(path).ok()?;
    let r: Report = serde_json::from_str(&text).ok()?;
    let bound = r.check == check
        && r.project_id == p.id
        && r.revision == p.revision
        && r.input_sha256 == eak_validation::report::project_hash(p);
    bound.then_some(r)
}

const FAB_GATE_SKIPPED: &str = "not counted by the release: the DFM report above checks the \
     Fabricator standard profile on this revision";

fn gate_state(s: eak_validation::GateState) -> &'static str {
    use eak_validation::GateState as G;
    match s {
        G::Pass => "PASS",
        G::Fail => "FAIL",
        G::Unknown => "UNKNOWN",
        G::NotRun => "NOT_RUN",
    }
}

fn release_check_of(dir: Option<&Path>, p: &Project) -> ReleaseCheck {
    let reports: Vec<Report> = RELEASE_REPORTS
        .iter()
        .filter_map(|(c, _)| current_evidence(dir, c, p))
        .collect();
    let refs: Vec<&Report> = reports.iter().collect();
    let (_, blocking, gate_report) = eak_manufacturing::release_gates_with_report(p, &refs);
    let mut gates = vec![];
    for (check, title) in RELEASE_REPORTS {
        // "DRC reports 2 error(s)" and the per-rule "DRC: DRC-002 is UNKNOWN: …" lines.
        let (prefix, rule_prefix) = (format!("{check} "), format!("{check}: "));
        let reasons: Vec<String> = blocking
            .iter()
            .filter(|b| {
                b.starts_with(&prefix)
                    || b.starts_with(&rule_prefix)
                    || (check == "MECH" && b.contains("MECH-0"))
            })
            .cloned()
            .collect();
        let report = reports.iter().find(|r| r.check == check);
        // ADR-0048: a report with an UNKNOWN rule is UNKNOWN, never PASS, even with 0 errors.
        let unknown = |r: &Report| {
            let u = eak_validation::CheckState::Unknown;
            r.rule_results
                .iter()
                .any(|rr| rr.state == u || rr.default_state == Some(u))
        };
        let state = match report {
            None => "NOT_RUN",
            Some(r) if r.outcome == eak_validation::Outcome::Errors => "FAIL",
            Some(r) if unknown(r) => "UNKNOWN",
            Some(_) if reasons.is_empty() => "PASS",
            Some(_) => "UNKNOWN",
        };
        let evidence = match (report, dir) {
            (Some(r), _) => format!(
                "{} report: {} error(s), {} warning(s); revision {}",
                r.check, r.errors, r.warnings, r.revision
            ),
            (None, Some(_)) => format!("no {check} report saved for revision {}", p.revision),
            (None, None) => "the project has no folder, so no report can be saved".into(),
        };
        gates.push(ReleaseGate {
            id: check.to_lowercase(),
            title: title.into(),
            state: state.into(),
            required: true,
            reasons,
            evidence: vec![evidence],
        });
    }
    for g in gate_report.gates {
        // The release covers fabricator capability with the revision-bound DFM report above
        // (eak_manufacturing::release_gates skips this gate), so it is shown but not counted.
        let disabled = g.id == "fabricator_capability";
        let (state, reasons) = match disabled {
            true => ("DISABLED", vec![FAB_GATE_SKIPPED.to_string()]),
            false => (gate_state(g.state), g.reasons),
        };
        let evidence: Vec<String> = g
            .evidence
            .iter()
            .map(|e| format!("{}: {}", e.source, e.detail))
            .collect();
        gates.push(ReleaseGate {
            id: g.id,
            title: g.title,
            state: state.into(),
            required: g.required && !disabled,
            reasons,
            evidence,
        });
    }
    // Fail closed: a required row that is not PASS keeps the release DRAFT whatever `blocking` says.
    let open = gates.iter().any(|g| g.required && g.state != "PASS");
    let status = if open {
        eak_manufacturing::PackageStatus::Draft
    } else {
        eak_manufacturing::status_of(&blocking)
    };
    ReleaseCheck {
        revision: p.revision,
        input_sha256: eak_validation::report::project_hash(p),
        gates,
        blocking,
        status,
        not_verified: eak_manufacturing::not_verified(),
    }
}

/// The release check of the current revision. Read-only: it reads the saved reports and
/// evaluates the gates, and runs no check whose report it would need (those rows are `NOT_RUN`).
pub fn release_check(s: &Session) -> Result<ReleaseCheck, String> {
    let (key, p) = s.shared_project()?;
    Ok(release_check_of(key.dir.as_deref(), &p))
}

/// Run ERC, DRC, MECH and DFM on the current revision and save each as evidence (what every
/// export does first), then return the release check over the saved reports. No design change.
pub fn run_release_checks(s: &Session) -> Result<ReleaseCheck, String> {
    let (key, p) = s.shared_project()?;
    if key.dir.is_none() {
        return Err(
            "the release checks save their reports as evidence: the project needs a folder".into(),
        );
    }
    release_reports(s, &key, &p)?;
    Ok(release_check_of(key.dir.as_deref(), &p))
}

pub fn recent_projects(s: &Session, limit: usize) -> Result<Vec<RecentProject>, String> {
    let index = s
        .index
        .as_ref()
        .map_err(|e| format!("recent-project index unavailable: {e}"))?;
    let mut rows = index
        .lock()
        .map_err(|_| "index lock poisoned".to_string())?
        .recent(limit)?;
    // Real, read-only status per project (last modified, stage, pending validation, errors);
    // `None` when the folder moved or is not readable.
    for r in &mut rows {
        r.status = eak_runtime::status::read(std::path::Path::new(&r.path));
    }
    Ok(rows)
}

/// Names of every IPC command registered with Tauri; `tests/ipc.rs` checks that the
/// frontend (`src/api.ts`) invokes exactly these.
pub const IPC_COMMANDS: [&str; 100] = [
    "ai_propose",
    "ai_accept",
    "ai_reject",
    "ai_rollback",
    "library_search",
    "library_footprint_candidates",
    "library_model_read",
    "simulation_status",
    "run_simulation",
    "connectivity",
    "board_geometry",
    "route_step",
    "parts_search",
    "parts_import",
    "distributor_settings_get",
    "distributor_settings_save",
    "distributor_settings_clear",
    "distributor_test_connection",
    "catalogue_categories",
    "catalogue_manufacturers",
    "catalogue_search",
    "parts_save_record",
    "library_browse",
    "library_catalog",
    "library_set_enabled",
    "library_meta",
    "library_personal_save",
    "library_personal_remove",
    "library_import_open_source",
    "library_imported_remove",
    "kicad_library_scan",
    "kicad_library_import",
    "library_db_search",
    "user_library_export",
    "user_library_import",
    "assembly_attest",
    "assembly_import_parts_file",
    "manufacturing_preview",
    "part_standardisation",
    "fabricator_prepare",
    "footprint_verify",
    "pin_pad_verify",
    "component_save",
    "library_components",
    "domain_classification",
    "intelligence_summary",
    "export_manufacturing",
    "bom_table",
    "export_bom",
    "export_documents",
    "export_findings",
    "release_check",
    "run_release_checks",
    "create_project",
    "open_project",
    "project_lock_state",
    "close_project",
    "current_project",
    "agent_tasks",
    "netlist",
    "kicad_import_report",
    "kicad_import_commit",
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
    "open_projects",
    "switch_project",
    "close_project_by_id",
    "operation_changes",
    "sync_preview",
    "sync_preview_export",
    "project_files",
    "project_file_read",
    "project_file_create",
    "project_file_rename",
    "project_file_delete",
    "project_git_status",
    "model_catalog",
    "model_set_default",
    "model_export_config",
    "model_import_config",
    "resource_profile",
    "performance_settings",
    "set_performance_settings",
    "home_project_facts",
    "home_default_dir",
    "home_suggest_folder",
    "create_project_with_brief",
    "create_sample_project",
];

/// Tauri command wrappers: argument decoding only, no logic.
///
/// Work that touches the disk or computes over the project runs on Tokio's blocking pool
/// ([`blocking`]), never on an async worker thread or the main thread. Validation and analysis
/// (P2) and background jobs (P3) wait for a slot sized by the resource profile
/// ([`resources::Resources`]); interactive reads (P0) and edits (P1) never queue. The JS side
/// awaits each edit, so edit order is unchanged.
pub mod cmd {
    use super::*;
    use crate::resources::{Priority, Resources};
    use tauri::{AppHandle, Manager, State};

    /// Run `f` on the blocking pool, after taking a slot of `priority`'s class (if bounded).
    pub(crate) async fn blocking<T: Send + 'static>(
        app: AppHandle,
        priority: Priority,
        f: impl FnOnce(&Session) -> Result<T, String> + Send + 'static,
    ) -> Result<T, String> {
        let limit = app.state::<Resources>().limit(priority);
        let permit = match limit {
            Some(sem) => Some(
                sem.acquire_owned()
                    .await
                    .map_err(|_| "the job queue is closed".to_string())?,
            ),
            None => None,
        };
        tauri::async_runtime::spawn_blocking(move || {
            let _slot = permit;
            f(&app.state::<Session>())
        })
        .await
        .map_err(|e| format!("background job failed: {e}"))?
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn create_project(
        app: AppHandle,
        path: String,
        name: String,
    ) -> Result<Snapshot, String> {
        blocking(app, Priority::P1, move |s| {
            super::create_project(s, &path, &name)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn open_project(
        app: AppHandle,
        path: String,
        take_over_lock: bool,
    ) -> Result<Snapshot, String> {
        blocking(app, Priority::P1, move |s| {
            super::open_project(s, &path, take_over_lock)
        })
        .await
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn project_lock_state(path: String) -> Result<LockState, String> {
        super::project_lock_state(&path)
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn close_project(state: State<'_, Session>) -> Result<(), String> {
        super::close_project(&state)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn current_project(app: AppHandle) -> Result<Snapshot, String> {
        blocking(app, Priority::P1, super::current_project).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn kicad_import_report(
        app: AppHandle,
        path: String,
    ) -> Result<super::kicad::KicadImportView, String> {
        blocking(app, Priority::P1, move |_| {
            super::kicad::import_report(&path)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn kicad_import_commit(
        app: AppHandle,
        path: String,
        licence: Option<String>,
        source: Option<String>,
    ) -> Result<super::kicad::KicadCommitResult, String> {
        blocking(app, Priority::P1, move |_| {
            super::kicad::import_commit(&path, licence.as_deref(), source.as_deref())
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn netlist(app: AppHandle) -> Result<Vec<super::NetRow>, String> {
        blocking(app, Priority::P1, super::netlist).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn agent_tasks(
        app: AppHandle,
    ) -> Result<Vec<eak_harness::agent_task::AgentTask>, String> {
        blocking(app, Priority::P1, super::agent_tasks).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn execute_command(
        app: AppHandle,
        expected_revision: u64,
        command: Command,
    ) -> Result<CommandResponse, String> {
        blocking(app, Priority::P1, move |s| {
            super::execute_command(s, expected_revision, command)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn undo(app: AppHandle, expected_revision: u64) -> Result<CommandResponse, String> {
        blocking(app, Priority::P1, move |s| {
            super::undo(s, expected_revision)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn redo(app: AppHandle, expected_revision: u64) -> Result<CommandResponse, String> {
        blocking(app, Priority::P1, move |s| {
            super::redo(s, expected_revision)
        })
        .await
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn events_since(state: State<'_, Session>, seq: u64) -> Result<Vec<Event>, String> {
        super::events_since(&state, seq)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn acknowledge_recovery(app: AppHandle) -> Result<Snapshot, String> {
        blocking(app, Priority::P1, super::acknowledge_recovery).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn run_check(app: AppHandle, check: Check) -> Result<CheckResult, String> {
        blocking(app, Priority::P2, move |s| super::run_check(s, check)).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn domain_classification(app: AppHandle) -> Result<DomainReport, String> {
        blocking(app, Priority::P2, super::domain_classification).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn intelligence_summary(
        app: AppHandle,
    ) -> Result<eak_intelligence::systems::IntelligenceSummary, String> {
        blocking(app, Priority::P2, super::intelligence_summary).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn connectivity(app: AppHandle) -> Result<ConnectivityReport, String> {
        blocking(app, Priority::P2, super::connectivity).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn board_geometry(app: AppHandle) -> Result<BoardGeometryReport, String> {
        blocking(app, Priority::P2, super::board_geometry).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_search(
        app: AppHandle,
        query: String,
        category: String,
        source: String,
    ) -> Result<Vec<LibraryPart>, String> {
        blocking(app, Priority::P0, move |s| {
            super::library_search(s, &query, &category, &source)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn library_footprint_candidates(
        app: AppHandle,
        component_id: String,
    ) -> Result<Vec<FootprintCandidate>, String> {
        blocking(app, Priority::P0, move |s| {
            super::footprint_candidates(s, &component_id)
        })
        .await
    }

    /// Bytes of a component's 3D model file, returned as a raw binary IPC response.
    #[tauri::command(async, rename_all = "snake_case")]
    pub fn library_model_read(path: String) -> Result<tauri::ipc::Response, String> {
        super::read_model_file(&path).map(tauri::ipc::Response::new)
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn simulation_status(app: AppHandle) -> Result<SimulationStatus, String> {
        // Probes for the ngspice executable (a process spawn), so it is background work.
        blocking(app, Priority::P3, super::simulation_status).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn run_simulation(
        app: AppHandle,
        request: eak_simulation::SimRequest,
    ) -> Result<eak_simulation::SimResult, String> {
        blocking(app, Priority::P3, move |s| {
            super::run_simulation(s, &request)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn route_step(
        app: AppHandle,
        request: eak_engineering::interactive::RouteStepRequest,
    ) -> Result<RouteStepReport, String> {
        // Interactive preview: never queued behind validation or exports.
        blocking(app, Priority::P0, move |s| super::route_step(s, &request)).await
    }

    #[tauri::command(async, rename_all = "snake_case")]
    pub fn library_components() -> Vec<LibraryEntry> {
        super::library_components()
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn export_manufacturing(app: AppHandle) -> Result<ExportResult, String> {
        blocking(app, Priority::P3, super::export_manufacturing).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn bom_table(
        app: AppHandle,
        config: eak_manufacturing::bom_config::BomConfig,
    ) -> Result<eak_manufacturing::bom_config::BomTable, String> {
        // The dialog preview: interactive, never queued behind exports.
        blocking(app, Priority::P0, move |s| super::bom_table(s, &config)).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn export_bom(
        app: AppHandle,
        bom: eak_manufacturing::outputs::BomRequest,
    ) -> Result<ExportResult, String> {
        blocking(app, Priority::P3, move |s| super::export_bom(s, bom)).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn export_documents(
        app: AppHandle,
        request: eak_manufacturing::outputs::OutputRequest,
    ) -> Result<ExportResult, String> {
        blocking(app, Priority::P3, move |s| {
            super::export_documents(s, request)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn export_findings(
        app: AppHandle,
        check: Check,
        format: eak_manufacturing::findings_export::FindingsFormat,
    ) -> Result<FindingsExport, String> {
        blocking(app, Priority::P2, move |s| {
            super::export_findings(s, check, format)
        })
        .await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn release_check(app: AppHandle) -> Result<ReleaseCheck, String> {
        blocking(app, Priority::P2, super::release_check).await
    }

    #[tauri::command(rename_all = "snake_case")]
    pub async fn run_release_checks(app: AppHandle) -> Result<ReleaseCheck, String> {
        blocking(app, Priority::P2, super::run_release_checks).await
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
    use tauri::{Emitter, Manager};
    tauri::Builder::default()
        // Native folder pickers for Project Home (open a project folder, choose a parent directory).
        .plugin(tauri_plugin_dialog::init())
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
            // One resource profile for the whole app (docs/development/PERFORMANCE.md section 3).
            app.manage(resources::Resources::load(
                app.path()
                    .app_config_dir()
                    .ok()
                    .map(|d| d.join("performance.json")),
            ));
            // Harness endpoint for terminal agents: they act through this app's engine while a
            // project is open here (single writer; ADR-0018).
            match app.path().app_data_dir() {
                Ok(dir) => {
                    let handle = app.handle().clone();
                    let handler: eak_harness::endpoint::Handler =
                        std::sync::Arc::new(move |req: &eak_harness::endpoint::Request| {
                            let emitter = handle.clone();
                            // The task ledger can move without a revision change (an ERC run),
                            // so it has its own event (ADR-0036).
                            let dir = req.context.project.dir.clone();
                            let before = dir.as_deref().map(eak_harness::agent_task::stamp);
                            let out =
                                harness_call(&handle.state::<Session>(), req, &move |change| {
                                    if let Err(e) = emitter.emit(sync::REVISION_EVENT, change) {
                                        eprintln!("eak-desktop: revision event not emitted: {e}");
                                    }
                                });
                            if let (Some(d), Some(b)) = (dir.as_deref(), before) {
                                if eak_harness::agent_task::stamp(d) != b {
                                    let change = sync::AgentTasksChanged {
                                        dir: d.display().to_string(),
                                        tasks: eak_harness::agent_task::read(d).tasks,
                                    };
                                    if let Err(e) = handle.emit(sync::AGENT_TASK_EVENT, change) {
                                        eprintln!("eak-desktop: task event not emitted: {e}");
                                    }
                                }
                            }
                            out
                        });
                    if let Err(e) =
                        start_harness(&app.state::<Session>(), &dir.join("harness"), handler)
                    {
                        eprintln!("eak-desktop: harness endpoint unavailable: {e}");
                    }
                }
                Err(e) => eprintln!("eak-desktop: harness endpoint unavailable: {e}"),
            }
            app.manage(ai::AiState::default());
            app.manage(terminal_state(app));
            // Settings › AI tools: locally installed coding-agent CLIs; headless session logs go to
            // the app data folder.
            app.manage(agents::AgentsState::system(
                app.path()
                    .app_data_dir()
                    .ok()
                    .map(|d| d.join("agent-sessions")),
            ));
            // Non-secret Distributor connection status lives beside the model settings; the Distributor API
            // key and secret go to the OS credential store only.
            if let Ok(dir) = app.path().app_config_dir() {
                catalogue::init(&dir);
                libstore::init(&dir);
                library_import::init(&dir);
            }
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
            cmd::project_lock_state,
            cmd::close_project,
            cmd::current_project,
            cmd::agent_tasks,
            cmd::netlist,
            cmd::kicad_import_report,
            cmd::kicad_import_commit,
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
            models::cmd::model_catalog,
            models::cmd::model_set_default,
            models::cmd::model_export_config,
            models::cmd::model_import_config,
            cmd::domain_classification,
            cmd::intelligence_summary,
            cmd::library_components,
            cmd::library_search,
            cmd::library_footprint_candidates,
            cmd::library_model_read,
            cmd::connectivity,
            cmd::board_geometry,
            cmd::simulation_status,
            cmd::run_simulation,
            cmd::route_step,
            cmd::export_manufacturing,
            cmd::bom_table,
            cmd::export_bom,
            cmd::export_documents,
            cmd::export_findings,
            cmd::release_check,
            cmd::run_release_checks,
            ai::cmd::ai_propose,
            ai::cmd::ai_accept,
            ai::cmd::ai_reject,
            ai::cmd::ai_rollback,
            parts::cmd::parts_search,
            parts::cmd::parts_import,
            catalogue::cmd::distributor_settings_get,
            catalogue::cmd::distributor_settings_save,
            catalogue::cmd::distributor_settings_clear,
            catalogue::cmd::distributor_test_connection,
            catalogue::cmd::catalogue_categories,
            catalogue::cmd::catalogue_manufacturers,
            catalogue::cmd::catalogue_search,
            parts::cmd::parts_save_record,
            libstore::cmd::library_browse,
            libcatalog::cmd::library_catalog,
            libcatalog::cmd::library_set_enabled,
            libstore::cmd::library_meta,
            libstore::cmd::library_personal_save,
            libstore::cmd::library_personal_remove,
            libstore::cmd::library_import_open_source,
            libstore::cmd::library_imported_remove,
            library_import::cmd::kicad_library_scan,
            library_import::cmd::kicad_library_import,
            library_import::cmd::library_db_search,
            library_import::cmd::user_library_export,
            library_import::cmd::user_library_import,
            parts::cmd::assembly_attest,
            parts::cmd::assembly_import_parts_file,
            parts::cmd::manufacturing_preview,
            parts::cmd::part_standardisation,
            parts::cmd::fabricator_prepare,
            parts::cmd::footprint_verify,
            parts::cmd::pin_pad_verify,
            parts::cmd::component_save,
            workspace::cmd::open_projects,
            workspace::cmd::switch_project,
            workspace::cmd::close_project_by_id,
            workspace::cmd::operation_changes,
            eco::cmd::sync_preview,
            eco::cmd::sync_preview_export,
            workspace::cmd::project_files,
            workspace::cmd::project_file_read,
            workspace::cmd::project_file_create,
            workspace::cmd::project_file_rename,
            workspace::cmd::project_file_delete,
            workspace::cmd::project_git_status,
            terminal::cmd::terminal_overview,
            terminal::cmd::terminal_shells,
            terminal::cmd::terminal_launchers,
            terminal::cmd::terminal_api,
            terminal::cmd::terminal_set_context,
            terminal::cmd::terminal_create,
            terminal::cmd::terminal_write,
            terminal::cmd::terminal_run,
            terminal::cmd::terminal_resize,
            terminal::cmd::terminal_scrollback,
            terminal::cmd::terminal_rename,
            terminal::cmd::terminal_focus,
            terminal::cmd::terminal_restart,
            terminal::cmd::terminal_close,
            terminal::cmd::terminal_space_create,
            terminal::cmd::terminal_space_rename,
            terminal::cmd::terminal_space_close,
            terminal::cmd::terminal_move_tab,
            terminal::cmd::terminal_tab_rename,
            resources::cmd::resource_profile,
            resources::cmd::performance_settings,
            resources::cmd::set_performance_settings,
            home::cmd::home_project_facts,
            home::cmd::home_default_dir,
            home::cmd::home_suggest_folder,
            home::cmd::create_project_with_brief,
            home::cmd::create_sample_project,
            agents::cmd::agent_tools,
            agents::cmd::agent_tool_connect,
            agents::cmd::agent_tool_disconnect,
            agents::cmd::agent_tool_login,
            agents::cmd::agent_tool_launch,
            agents::cmd::agent_sessions,
            agents::cmd::agent_session_log,
            agents::cmd::agent_tool_stop,
        ])
        .build(tauri::generate_context!())
        .expect("EAK desktop failed to start")
        .run(|app, event| {
            // Hang up every terminal session on exit; the saved layout lists them as ended
            // next time, because processes cannot outlive the app.
            if let tauri::RunEvent::Exit = event {
                if let Some(t) = app.try_state::<terminal::TerminalState>() {
                    t.manager.shutdown();
                }
            }
        });
}

/// Build the terminal workspace state: the session manager (layout saved in the app data
/// dir), output streamed as events, the authenticated control API, and agent launchers.
fn terminal_state(app: &tauri::App) -> terminal::TerminalState {
    use std::sync::Arc;
    use tauri::{Emitter, Manager};
    let handle = app.handle().clone();
    let sink: eak_terminal::OutputSink = Arc::new(move |e| {
        let _ = handle.emit(terminal::OUTPUT_EVENT, e);
    });
    let data = app.path().app_data_dir().ok();
    let manager = match &data {
        Some(d) => eak_terminal::Manager::with_manifest(sink, d.join("terminal-sessions.json")),
        None => eak_terminal::Manager::new(sink),
    };
    let agents_file = app
        .path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("terminal-agents.json"));
    let (launchers, extra) = terminal::discover_launchers(agents_file.as_deref());
    manager.add_agents(extra);
    let api = match &data {
        Some(d) => {
            let file = d.join("terminal-api.json");
            eak_terminal::api::serve(Arc::clone(&manager), &file)
                .map(|info| (file, info))
                .map_err(|e| format!("terminal control API not started: {e}"))
        }
        None => Err("terminal control API not started: no app data directory".into()),
    };
    // Every session gets the control-API file and the app's harness endpoint (started before
    // this in `setup`); the project dir/id come from the session's space (docs/AGENT_HARNESS.md).
    let mut base_env = Vec::new();
    match &api {
        Ok((file, _)) => {
            base_env.push(("EAK_TERMINAL_API_FILE".into(), file.display().to_string()))
        }
        Err(e) => eprintln!("eak-desktop: {e}"),
    }
    if let Some(h) = harness_endpoint(&app.state::<Session>()) {
        base_env.push(("EAK_HARNESS_SOCKET".into(), h.endpoint));
        base_env.push(("EAK_HARNESS_TOKEN_FILE".into(), h.token_file));
    }
    manager.set_base_env(base_env);
    terminal::TerminalState {
        manager,
        api,
        launchers,
    }
}
