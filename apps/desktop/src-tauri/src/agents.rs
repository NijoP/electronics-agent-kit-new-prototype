//! Settings › AI tools IPC (remap slice W2-E): probe, connect, sign in, launch, and the session
//! list for coding-agent CLIs installed on this computer, over `eak-agents`.
//!
//! Nothing here writes a project folder. Each tool's own CLI writes its MCP registration into
//! its own config; sign-in runs the tool's own login in a tab of the existing Terminal
//! workspace; headless logs go to the app data folder. A connected agent changes the design only
//! through `eak-mcp`, the harness and `Engine::execute`.
//!
//! The Agents workspace (ADR-0058) reads the same sessions and, through [`weft_view`], the open
//! project's WEFT journal (ADR-0053): read only, nothing here sends on the bus.

use crate::Session;
use eak_agents::sessions::{SessionState, SessionView, Sessions, Start};
use eak_agents::{AgentTool, LaunchPlan, Locator, Surface, ToolStatus};
use eak_harness::weft;
use eak_terminal::{CreateOptions, Manager, SessionInfo};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct AgentsState {
    pub tools: Vec<Box<dyn AgentTool>>,
    pub locator: Locator,
    pub sessions: Sessions,
    /// EAK's MCP stdio server; Connect refuses (and says why) without it.
    pub server: Option<PathBuf>,
}

impl AgentsState {
    /// The built-in tools, looked up on `PATH` and the usual install folders.
    pub fn system(log_dir: Option<PathBuf>) -> AgentsState {
        let locator = Locator::system();
        AgentsState {
            server: eak_agents::find_mcp_server(&locator),
            tools: eak_agents::builtin_tools(),
            locator,
            sessions: Sessions::new(log_dir),
        }
    }
}

const NO_PROJECT: &str = "open a project first: EAK connects an AI tool to one project";

fn tool<'a>(state: &'a AgentsState, id: &str) -> Result<&'a dyn AgentTool, String> {
    eak_agents::find_tool(&state.tools, id)
}

fn binary(state: &AgentsState, tool: &dyn AgentTool) -> Result<PathBuf, String> {
    state.locator.find(tool.program()).ok_or_else(|| {
        format!(
            "{} is not installed (looked on PATH and in the usual install folders); install it from {}",
            tool.label(),
            tool.install_url()
        )
    })
}

/// Every tool's row, probed in parallel (each runs a few short commands of its own).
pub fn tools(state: &AgentsState, project: Option<&Path>) -> Vec<ToolStatus> {
    std::thread::scope(|s| {
        let running: Vec<_> = state
            .tools
            .iter()
            .map(|t| s.spawn(move || eak_agents::probe(t.as_ref(), &state.locator, project)))
            .collect();
        running
            .into_iter()
            .zip(&state.tools)
            .map(|(h, t)| {
                h.join().unwrap_or_else(|_| {
                    let mut st = eak_agents::probe(t.as_ref(), &Locator::default(), None);
                    st.reason = Some(eak_agents::Unavailable::Other("the probe failed".into()));
                    st
                })
            })
            .collect()
    })
}

/// Register `eak-mcp` for `project` with the tool's own CLI, then probe again.
pub fn connect(
    state: &AgentsState,
    id: &str,
    project: Option<&Path>,
) -> Result<ToolStatus, String> {
    let tool = tool(state, id)?;
    let project = project.ok_or(NO_PROJECT)?;
    let server = state.server.as_deref().ok_or(
        "EAK's MCP server (eak-mcp) was not found beside the app or on PATH, so there is nothing to register",
    )?;
    let bin = binary(state, tool)?;
    tool.connect(&bin, project, server)?;
    Ok(eak_agents::probe(tool, &state.locator, Some(project)))
}

/// Remove EAK's registration for `project` with the tool's own CLI, then probe again.
pub fn disconnect(
    state: &AgentsState,
    id: &str,
    project: Option<&Path>,
) -> Result<ToolStatus, String> {
    let tool = tool(state, id)?;
    let project = project.ok_or(NO_PROJECT)?;
    let bin = binary(state, tool)?;
    tool.disconnect(&bin, project)?;
    Ok(eak_agents::probe(tool, &state.locator, Some(project)))
}

/// The terminal space for `dir` (the one the Terminal workspace uses for that folder).
fn space_for(manager: &Manager, dir: &Path) -> Result<u64, String> {
    if let Some(s) = manager.overview().spaces.iter().find(|s| s.root == dir) {
        return Ok(s.id);
    }
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string());
    manager.create_space(&name, dir.to_path_buf()).map(|s| s.id)
}

/// Open `plan` as a new tab of the existing Terminal workspace.
fn open_tab(manager: &Arc<Manager>, plan: &LaunchPlan, name: &str) -> Result<SessionInfo, String> {
    let space = space_for(manager, &plan.cwd)?;
    manager.create(CreateOptions {
        space: Some(space),
        name: Some(name.to_string()),
        cwd: Some(plan.cwd.clone()),
        command: Some(plan.argv.clone()),
        env: plan.env.clone(),
        ..Default::default()
    })
}

/// Run the tool's own sign-in in a terminal tab (EAK never sees the credentials).
pub fn login(
    state: &AgentsState,
    manager: &Arc<Manager>,
    id: &str,
    project: Option<&Path>,
) -> Result<SessionInfo, String> {
    let tool = tool(state, id)?;
    let bin = binary(state, tool)?;
    let cwd = project
        .map(Path::to_path_buf)
        .or_else(eak_agents::probe::home_dir)
        .ok_or("no folder to run the sign-in in")?;
    let plan = eak_agents::login_plan(tool, &bin, &cwd)?;
    open_tab(manager, &plan, &format!("{} sign-in", tool.label()))
}

/// Start a session of the tool in the open project: a terminal tab or a headless process.
pub fn launch(
    state: &AgentsState,
    manager: &Arc<Manager>,
    id: &str,
    project: Option<&Path>,
    surface: Surface,
    prompt: Option<&str>,
) -> Result<SessionView, String> {
    let tool = tool(state, id)?;
    let project = project.ok_or(NO_PROJECT)?;
    let bin = binary(state, tool)?;
    let prompt = prompt.map(str::trim).filter(|p| !p.is_empty());
    let sid = state.sessions.next_id(tool.id());
    let plan = eak_agents::launch_plan(tool, &bin, project, surface, prompt, &sid)?;
    let start = Start {
        id: sid,
        tool: tool.id(),
        label: tool.label(),
        plan: &plan,
        prompt,
    };
    match surface {
        Surface::Headless => state.sessions.spawn_headless(start, tool.progress_parser()),
        Surface::Terminal => {
            let info = open_tab(manager, &plan, tool.label())?;
            Ok(state.sessions.register_terminal(start, info.id, info.pid))
        }
    }
}

fn from_terminal(status: &eak_terminal::Status) -> SessionState {
    use eak_terminal::State as T;
    match status.state {
        T::Completed => SessionState::Completed,
        T::Failed | T::Ended => SessionState::Failed,
        T::Running | T::Idle | T::WaitingForInput | T::Blocked => SessionState::Running,
    }
}

/// Every session started from EAK, newest first; terminal ones take their state from the
/// terminal engine (process facts), headless ones from their process and stream.
pub fn sessions(state: &AgentsState, manager: &Manager) -> Vec<SessionView> {
    for v in state.sessions.list() {
        let Some(t) = v.terminal_session else {
            continue;
        };
        if v.state != SessionState::Running {
            continue;
        }
        match manager.info(t) {
            Ok(info) => state.sessions.set_state(
                &v.id,
                from_terminal(&info.status),
                &info.status.detail,
                info.status.exit_code.and_then(|c| i32::try_from(c).ok()),
            ),
            Err(_) => state.sessions.set_state(
                &v.id,
                SessionState::Stopped,
                "its terminal tab was closed",
                None,
            ),
        }
    }
    state.sessions.list()
}

/// The last `lines` lines a session printed.
pub fn session_log(
    state: &AgentsState,
    manager: &Manager,
    id: &str,
    lines: usize,
) -> Result<String, String> {
    let v = state
        .sessions
        .get(id)
        .ok_or_else(|| format!("no agent session {id}"))?;
    match v.terminal_session {
        Some(t) => manager.read(t, Some(lines)),
        None => state.sessions.tail(id, lines).map(|l| l.join("\n")),
    }
}

/// Stop a session: kill a headless process, or close a terminal tab that is still running.
pub fn stop(state: &AgentsState, manager: &Manager, id: &str) -> Result<SessionView, String> {
    let v = state
        .sessions
        .get(id)
        .ok_or_else(|| format!("no agent session {id}"))?;
    let Some(t) = v.terminal_session else {
        return state.sessions.stop(id);
    };
    if v.state == SessionState::Running {
        manager.close(t)?;
        state.sessions.mark_stopped(id);
    }
    state
        .sessions
        .get(id)
        .ok_or_else(|| format!("no agent session {id}"))
}

/// Start a session again with the same tool, surface, project and first message (a new session
/// id); a session that is still running is stopped first.
pub fn restart(
    state: &AgentsState,
    manager: &Arc<Manager>,
    id: &str,
) -> Result<SessionView, String> {
    let v = state
        .sessions
        .get(id)
        .ok_or_else(|| format!("no agent session {id}"))?;
    if v.state == SessionState::Running {
        stop(state, manager, id)?;
    }
    launch(
        state,
        manager,
        &v.tool,
        Some(Path::new(&v.project)),
        v.surface,
        v.prompt.as_deref(),
    )
}

/// A registered WEFT agent, with when and at which project revision it (last) registered.
#[derive(Debug, Clone, Serialize)]
pub struct WeftAgentView {
    pub card: weft::AgentCard,
    pub registered_ms: u64,
    pub registered_revision: u64,
}

/// A WEFT task with its journal times and what it still waits for.
#[derive(Debug, Clone, Serialize)]
pub struct WeftTaskView {
    pub task: weft::Task,
    pub created_ms: u64,
    /// The project revision when the task was delegated.
    pub created_revision: u64,
    /// When its status last changed (its creation when it never did).
    pub updated_ms: u64,
    /// Dependencies that are not done yet.
    pub waiting_on: Vec<String>,
    /// Pending with nothing to wait for.
    pub ready: bool,
}

/// A WEFT message with its journal sequence number, time and revision.
#[derive(Debug, Clone, Serialize)]
pub struct WeftMessageView {
    pub seq: u64,
    pub ts_ms: u64,
    pub revision: u64,
    pub message: weft::Message,
}

/// What the open project's WEFT journal says, for the Agents workspace: the registry, the task
/// graph and every message, each with the time and revision the journal recorded.
#[derive(Debug, Clone, Serialize)]
pub struct WeftView {
    pub project_dir: String,
    /// The project's current revision (to tell how far behind a task or registration is).
    pub revision: u64,
    /// Journal entries read.
    pub entries: usize,
    /// Lines that did not parse (skipped, never guessed; a line being written reads as one).
    pub unreadable: usize,
    pub agents: Vec<WeftAgentView>,
    pub tasks: Vec<WeftTaskView>,
    pub messages: Vec<WeftMessageView>,
}

/// Replay `dir`'s WEFT journal into the workspace's view. Reads files only.
pub fn weft_view(dir: &Path, revision: u64) -> WeftView {
    let (entries, unreadable) = weft::entries(dir);
    let state = weft::replay(&entries);
    let mut registered: HashMap<&str, (u64, u64)> = HashMap::new();
    let mut created: HashMap<&str, (u64, u64)> = HashMap::new();
    let mut updated: HashMap<&str, u64> = HashMap::new();
    let mut messages = Vec::new();
    for e in &entries {
        match &e.event {
            weft::Event::AgentRegistered { agent } => {
                registered.insert(agent.id.as_str(), (e.ts_ms, e.revision));
            }
            weft::Event::TaskCreated { task } => {
                created.insert(task.id.as_str(), (e.ts_ms, e.revision));
            }
            weft::Event::TaskStatus { task_id, .. } => {
                updated.insert(task_id.as_str(), e.ts_ms);
            }
            weft::Event::Message { message } => messages.push(WeftMessageView {
                seq: e.seq,
                ts_ms: e.ts_ms,
                revision: e.revision,
                message: message.clone(),
            }),
        }
    }
    let agents = state
        .agents
        .values()
        .map(|a| {
            let (ms, rev) = registered.get(a.id.as_str()).copied().unwrap_or((0, 0));
            WeftAgentView {
                card: a.clone(),
                registered_ms: ms,
                registered_revision: rev,
            }
        })
        .collect();
    let tasks = state
        .tasks
        .values()
        .map(|t| {
            let (ms, rev) = created.get(t.id.as_str()).copied().unwrap_or((0, 0));
            let waiting_on = weft::waiting_on(&state, t);
            WeftTaskView {
                ready: t.status == weft::TaskStatus::Pending && waiting_on.is_empty(),
                waiting_on,
                created_ms: ms,
                created_revision: rev,
                updated_ms: updated.get(t.id.as_str()).copied().unwrap_or(ms),
                task: t.clone(),
            }
        })
        .collect();
    WeftView {
        project_dir: dir.display().to_string(),
        revision,
        entries: entries.len(),
        unreadable,
        agents,
        tasks,
        messages,
    }
}

const NO_SAVED_PROJECT: &str =
    "open a saved project first: the WEFT bus is the project folder's weft/journal.jsonl";

pub mod cmd {
    use super::*;
    use crate::terminal::TerminalState;
    use tauri::State;

    type R<T> = Result<T, String>;

    /// The open project's folder, if any.
    fn project(s: &Session) -> Option<PathBuf> {
        s.with_engine(|e| e.dir().map(Path::to_path_buf))
            .ok()
            .flatten()
    }

    #[tauri::command(async)]
    pub fn agent_tools(
        state: State<'_, AgentsState>,
        session: State<'_, Session>,
    ) -> R<Vec<ToolStatus>> {
        Ok(tools(&state, project(&session).as_deref()))
    }
    #[tauri::command(async)]
    pub fn agent_tool_connect(
        state: State<'_, AgentsState>,
        session: State<'_, Session>,
        tool: String,
    ) -> R<ToolStatus> {
        connect(&state, &tool, project(&session).as_deref())
    }
    #[tauri::command(async)]
    pub fn agent_tool_disconnect(
        state: State<'_, AgentsState>,
        session: State<'_, Session>,
        tool: String,
    ) -> R<ToolStatus> {
        disconnect(&state, &tool, project(&session).as_deref())
    }
    #[tauri::command(async)]
    pub fn agent_tool_login(
        state: State<'_, AgentsState>,
        terminal: State<'_, TerminalState>,
        session: State<'_, Session>,
        tool: String,
    ) -> R<SessionInfo> {
        login(
            &state,
            &terminal.manager,
            &tool,
            project(&session).as_deref(),
        )
    }
    #[tauri::command(async)]
    pub fn agent_tool_launch(
        state: State<'_, AgentsState>,
        terminal: State<'_, TerminalState>,
        session: State<'_, Session>,
        tool: String,
        surface: Surface,
        prompt: Option<String>,
    ) -> R<SessionView> {
        launch(
            &state,
            &terminal.manager,
            &tool,
            project(&session).as_deref(),
            surface,
            prompt.as_deref(),
        )
    }
    #[tauri::command(async)]
    pub fn agent_sessions(
        state: State<'_, AgentsState>,
        terminal: State<'_, TerminalState>,
    ) -> R<Vec<SessionView>> {
        Ok(sessions(&state, &terminal.manager))
    }
    /// Why the session index could not be written this run (ADR-0058), or null.
    #[tauri::command]
    pub fn agent_session_index_error(state: State<'_, AgentsState>) -> R<Option<String>> {
        Ok(state.sessions.index_error())
    }
    #[tauri::command(async)]
    pub fn agent_session_log(
        state: State<'_, AgentsState>,
        terminal: State<'_, TerminalState>,
        id: String,
        lines: Option<usize>,
    ) -> R<String> {
        session_log(
            &state,
            &terminal.manager,
            &id,
            lines.unwrap_or(200).min(2000),
        )
    }
    #[tauri::command(async)]
    pub fn agent_session_restart(
        state: State<'_, AgentsState>,
        terminal: State<'_, TerminalState>,
        id: String,
    ) -> R<SessionView> {
        restart(&state, &terminal.manager, &id)
    }

    /// The open project's WEFT journal, replayed (Agents workspace; read only).
    #[tauri::command(async)]
    pub fn weft_state(session: State<'_, Session>) -> R<WeftView> {
        let (dir, revision) = session
            .with_engine(|e| {
                let revision = e.project().revision;
                e.dir().map(|d| (d.to_path_buf(), revision))
            })?
            .ok_or(NO_SAVED_PROJECT)?;
        Ok(weft_view(&dir, revision))
    }

    #[tauri::command(async)]
    pub fn agent_tool_stop(
        state: State<'_, AgentsState>,
        terminal: State<'_, TerminalState>,
        id: String,
    ) -> R<SessionView> {
        stop(&state, &terminal.manager, &id)
    }
}

/// IPC command names this module registers (kept in sync with `AGENT_IPC` in `api.ts`).
pub const AGENT_COMMANDS: [&str; 11] = [
    "agent_tools",
    "agent_tool_connect",
    "agent_tool_disconnect",
    "agent_tool_login",
    "agent_tool_launch",
    "agent_sessions",
    "agent_session_log",
    "agent_tool_stop",
    "agent_session_restart",
    "weft_state",
    "agent_session_index_error",
];
