//! Terminal workspace IPC: thin wrappers over the `eak-terminal` session manager, which the
//! app owns for its whole life (sessions survive workspace switches and window reloads).
//! Terminal output streams to the UI as `terminal-output` events.
//!
//! Terminal sessions are user-run programs outside the design engine: nothing here touches
//! the EAK-IR or issues engine `Command`s.

use eak_terminal::api::ApiInfo;
use eak_terminal::{
    CreateOptions, Manager, Overview, SessionId, SessionInfo, Space, SpaceId, TabId,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Event name for streamed output (`eak_terminal::OutputEvent`).
pub const OUTPUT_EVENT: &str = "terminal-output";

pub struct TerminalState {
    pub manager: Arc<Manager>,
    pub api: Result<(PathBuf, ApiInfo), String>,
    pub launchers: Vec<Launcher>,
}

/// A coding-agent CLI found on this machine, offered as "new session running X". Built-in
/// names come from `eak_terminal::KNOWN_AGENTS`; users add their own in
/// `terminal-agents.json` (see docs/terminal-api.md) without any code change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Launcher {
    pub name: String,
    /// What is typed into a new shell session to start it.
    pub command: String,
    pub path: Option<String>,
}

#[derive(Deserialize)]
struct UserAgent {
    name: String,
    #[serde(default)]
    command: Option<String>,
}

/// User-level install folders that a GUI app's `PATH` often lacks but a login shell has.
fn extra_dirs() -> Vec<PathBuf> {
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    let Some(h) = home else { return vec![] };
    [
        ".local/bin",
        ".opencode/bin",
        ".bun/bin",
        ".npm-global/bin",
        ".cargo/bin",
        ".deno/bin",
        "bin",
    ]
    .iter()
    .map(|d| h.join(d))
    .collect()
}

pub fn locate(program: &str) -> Option<PathBuf> {
    eak_terminal::find_on_path(program).or_else(|| {
        extra_dirs().into_iter().find_map(|d| {
            let exe = if cfg!(windows) {
                [".exe", ".cmd", ""]
                    .iter()
                    .map(|e| d.join(format!("{program}{e}")))
                    .find(|p| p.is_file())
            } else {
                Some(d.join(program)).filter(|p| p.is_file())
            };
            exe
        })
    })
}

/// Agent CLIs that are actually installed (never a button that cannot work), plus any
/// declared in `agents_file`.
pub fn discover_launchers(agents_file: Option<&Path>) -> (Vec<Launcher>, Vec<String>) {
    let mut out: Vec<Launcher> = eak_terminal::KNOWN_AGENTS
        .iter()
        .filter_map(|n| {
            locate(n).map(|p| Launcher {
                name: n.to_string(),
                command: n.to_string(),
                path: Some(p.display().to_string()),
            })
        })
        .collect();
    let mut extra_names = Vec::new();
    if let Some(text) = agents_file.and_then(|f| std::fs::read_to_string(f).ok()) {
        match serde_json::from_str::<Vec<UserAgent>>(&text) {
            Ok(list) => {
                for a in list {
                    let command = a.command.unwrap_or_else(|| a.name.clone());
                    let program = command
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string();
                    extra_names.push(a.name.clone());
                    if let Some(p) = locate(&program) {
                        out.retain(|l| l.name != a.name);
                        out.push(Launcher {
                            name: a.name,
                            command,
                            path: Some(p.display().to_string()),
                        });
                    }
                }
            }
            Err(e) => eprintln!("eak-desktop: terminal-agents.json ignored: {e}"),
        }
    }
    (out, extra_names)
}

/// Location of the control API for display (the token is never sent to the UI).
#[derive(Debug, Clone, Serialize)]
pub struct ApiView {
    pub api_file: Option<String>,
    pub port: Option<u16>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScrollbackView {
    pub data: String,
    pub seq: u64,
}

pub mod cmd {
    use super::*;
    use tauri::State;

    type R<T> = Result<T, String>;

    #[tauri::command]
    pub fn terminal_overview(state: State<'_, TerminalState>) -> Overview {
        state.manager.overview()
    }
    #[tauri::command(async)]
    pub fn terminal_shells() -> Vec<String> {
        eak_terminal::available_shells()
    }
    #[tauri::command]
    pub fn terminal_launchers(state: State<'_, TerminalState>) -> Vec<Launcher> {
        state.launchers.clone()
    }
    #[tauri::command]
    pub fn terminal_api(state: State<'_, TerminalState>) -> ApiView {
        match &state.api {
            Ok((file, info)) => ApiView {
                api_file: Some(file.display().to_string()),
                port: Some(info.port),
                error: None,
            },
            Err(e) => ApiView {
                api_file: None,
                port: None,
                error: Some(e.clone()),
            },
        }
    }
    // `project_id` arrives snake_case from `terminal/api.ts`, like every other EAK command.
    #[tauri::command(rename_all = "snake_case")]
    pub fn terminal_set_context(
        state: State<'_, TerminalState>,
        dir: Option<String>,
        project: Option<String>,
        project_id: Option<String>,
    ) {
        state
            .manager
            .set_context(dir.map(PathBuf::from), project, project_id);
    }
    #[tauri::command(async)]
    pub fn terminal_create(state: State<'_, TerminalState>, opts: CreateOptions) -> R<SessionInfo> {
        state.manager.create(opts)
    }
    #[tauri::command]
    pub fn terminal_write(state: State<'_, TerminalState>, id: SessionId, data: String) -> R<()> {
        state.manager.write(id, data.as_bytes())
    }
    #[tauri::command(async)]
    pub fn terminal_run(state: State<'_, TerminalState>, id: SessionId, command: String) -> R<u64> {
        state.manager.run(id, &command)
    }
    #[tauri::command]
    pub fn terminal_resize(
        state: State<'_, TerminalState>,
        id: SessionId,
        cols: u16,
        rows: u16,
    ) -> R<()> {
        state.manager.resize(id, cols, rows)
    }
    #[tauri::command]
    pub fn terminal_scrollback(
        state: State<'_, TerminalState>,
        id: SessionId,
    ) -> R<ScrollbackView> {
        state
            .manager
            .scrollback(id)
            .map(|(data, seq)| ScrollbackView { data, seq })
    }
    #[tauri::command]
    pub fn terminal_rename(state: State<'_, TerminalState>, id: SessionId, name: String) -> R<()> {
        state.manager.rename(id, &name)
    }
    #[tauri::command]
    pub fn terminal_focus(state: State<'_, TerminalState>, id: SessionId) -> R<()> {
        state.manager.focus(id)
    }
    #[tauri::command(async)]
    pub fn terminal_restart(state: State<'_, TerminalState>, id: SessionId) -> R<SessionInfo> {
        state.manager.restart(id)
    }
    #[tauri::command(async)]
    pub fn terminal_close(state: State<'_, TerminalState>, id: SessionId) -> R<()> {
        state.manager.close(id)
    }
    #[tauri::command]
    pub fn terminal_space_create(
        state: State<'_, TerminalState>,
        name: String,
        root: String,
    ) -> R<Space> {
        state.manager.create_space(&name, PathBuf::from(root))
    }
    #[tauri::command]
    pub fn terminal_space_rename(
        state: State<'_, TerminalState>,
        id: SpaceId,
        name: String,
    ) -> R<()> {
        state.manager.rename_space(id, &name)
    }
    #[tauri::command(async)]
    pub fn terminal_space_close(state: State<'_, TerminalState>, id: SpaceId) -> R<()> {
        state.manager.close_space(id)
    }
    #[tauri::command]
    pub fn terminal_move_tab(state: State<'_, TerminalState>, tab: TabId, index: usize) -> R<()> {
        state.manager.move_tab(tab, index)
    }
}

/// IPC command names this module registers (kept in sync with `api.ts`).
pub const TERMINAL_COMMANDS: [&str; 18] = [
    "terminal_overview",
    "terminal_shells",
    "terminal_launchers",
    "terminal_api",
    "terminal_set_context",
    "terminal_create",
    "terminal_write",
    "terminal_run",
    "terminal_resize",
    "terminal_scrollback",
    "terminal_rename",
    "terminal_focus",
    "terminal_restart",
    "terminal_close",
    "terminal_space_create",
    "terminal_space_rename",
    "terminal_space_close",
    "terminal_move_tab",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agents_file_adds_installed_launchers_only() {
        let dir = std::env::temp_dir().join(format!("eak-agents-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("terminal-agents.json");
        let sh = if cfg!(windows) { "cmd" } else { "sh" };
        std::fs::write(&f, format!(r#"[{{"name":"my-agent","command":"{sh} -c true"}},{{"name":"ghost","command":"definitely-not-installed-xyz"}}]"#)).unwrap();
        let (launchers, names) = discover_launchers(Some(&f));
        assert!(launchers.iter().any(|l| l.name == "my-agent"));
        assert!(
            !launchers.iter().any(|l| l.name == "ghost"),
            "no button for a missing program"
        );
        assert_eq!(names, vec!["my-agent", "ghost"]);
    }
}
