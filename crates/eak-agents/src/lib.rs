//! AI tool adapters (Settings › AI tools; Traycer review §11.3, remap slice W2-E).
//!
//! EAK does not reimplement coding agents. It finds the ones installed on this computer
//! (Claude Code, Codex, ...), reads their real state with their own commands, registers EAK's
//! MCP server (`eak-mcp`) with them through their own CLI, and starts sessions either in a tab
//! of the existing Terminal workspace (the desktop does that with a [`LaunchPlan`]) or headless
//! with the output captured ([`sessions`]).
//!
//! Honesty rules (AGENTS.md):
//! - sign-in state comes from the tool's own status command; when the tool cannot tell us, the
//!   state is `None` and shown as "unknown", never as "signed in";
//! - EAK reads no credential file and stores no key for CLI tools (ADR-0017): each tool keeps
//!   its own sign-in;
//! - a tool whose connect or headless flags are not confirmed against its docs reports
//!   [`Unavailable::Unsupported`] for that action instead of guessing.
//!
//! No IR or `Command` change: design work by a connected agent still goes through `eak-mcp`,
//! the harness and `Engine::execute`.

pub mod adapters;
pub mod probe;
pub mod sessions;

pub use probe::{find_mcp_server, Locator};

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Name EAK registers its MCP server under in every tool.
pub const MCP_SERVER_NAME: &str = "eak";

/// Where a session runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// A normal tab in the existing Terminal workspace.
    Terminal,
    /// A background process whose output EAK captures and parses.
    Headless,
}

/// Why a tool (or one action of it) cannot be used now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum Unavailable {
    MissingBinary,
    NotSignedIn,
    /// EAK has no confirmed way to do this with this tool.
    Unsupported(String),
    Other(String),
}

/// One row of Settings › AI tools.
#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub id: String,
    pub label: String,
    /// Install page of the tool, for the "not found" row.
    pub install_url: String,
    pub binary: Option<String>,
    pub version: Option<String>,
    /// Found, ran, and not known to be signed out.
    pub available: bool,
    pub reason: Option<Unavailable>,
    /// `None` = the tool could not tell us (shown as "unknown", never as "yes").
    pub signed_in: Option<bool>,
    /// How the tool says it is signed in (e.g. "claude.ai account"); never an account name.
    pub auth_detail: Option<String>,
    /// EAK's MCP server registered with this tool for the open project. `None` = unknown or no
    /// project open (see `connection_detail`).
    pub connected: Option<bool>,
    pub connection_detail: Option<String>,
    /// Where the registration lives (e.g. "Claude Code local scope for this project").
    pub connection_scope: Option<String>,
    pub can_connect: bool,
    pub can_login: bool,
    pub surfaces: Vec<Surface>,
}

/// How a tool reports its sign-in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Auth {
    pub signed_in: Option<bool>,
    pub detail: Option<String>,
}

/// Whether EAK's MCP server is registered with a tool for a project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registration {
    pub connected: Option<bool>,
    pub detail: Option<String>,
}

/// A program to start: the tool binary and its arguments, in the project folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchPlan {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: PathBuf,
    pub surface: Surface,
}

/// What a headless session reported, parsed from one line of its output stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    Started { detail: String },
    Message { text: String },
    ToolCall { name: String },
    Finished { ok: bool, summary: String },
    Error { message: String },
}

/// Turns one line of a headless session's output into an event.
pub type Parser = fn(&str) -> Option<AgentEvent>;

/// One coding-agent CLI. Defaults describe a tool EAK can only find and start in a terminal.
pub trait AgentTool: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    /// Executable name looked up on `PATH`.
    fn program(&self) -> &'static str;
    fn install_url(&self) -> &'static str;

    /// Sign-in state from the tool's own status command.
    fn auth(&self, _bin: &Path) -> Auth {
        Auth {
            signed_in: None,
            detail: Some("this tool has no sign-in status command EAK can read".into()),
        }
    }

    /// `Some(scope)` when EAK can register its MCP server with this tool.
    fn connect_scope(&self) -> Option<&'static str> {
        None
    }
    fn registration(&self, _bin: &Path, _project: &Path) -> Registration {
        Registration {
            connected: None,
            detail: Some(unsupported_connect()),
        }
    }
    fn connect(&self, _bin: &Path, _project: &Path, _server: &Path) -> Result<(), String> {
        Err(unsupported_connect())
    }
    fn disconnect(&self, _bin: &Path, _project: &Path) -> Result<(), String> {
        Err(unsupported_connect())
    }

    /// Arguments (after the binary) of the tool's own interactive sign-in.
    fn login_args(&self) -> Option<Vec<String>> {
        None
    }

    fn surfaces(&self) -> Vec<Surface> {
        vec![Surface::Terminal]
    }
    /// Arguments (after the binary) for a session; `prompt` is the first message, if any.
    fn launch_args(&self, surface: Surface, prompt: Option<&str>) -> Result<Vec<String>, String> {
        match surface {
            Surface::Terminal => Ok(prompt.map(|p| vec![p.to_string()]).unwrap_or_default()),
            Surface::Headless => Err(format!(
                "EAK has no confirmed headless mode for {}; launch it in a terminal tab",
                self.label()
            )),
        }
    }
    /// Parser for one line of headless output → an event, when the tool documents its stream.
    fn progress_parser(&self) -> Option<Parser> {
        None
    }
}

fn unsupported_connect() -> String {
    "EAK cannot register its MCP server with this tool yet (no confirmed command); add it by hand in the tool's own MCP settings: eak-mcp --project <folder>".into()
}

/// The built-in tools, in the order Settings shows them.
pub fn builtin_tools() -> Vec<Box<dyn AgentTool>> {
    vec![
        Box::new(adapters::claude::Claude),
        Box::new(adapters::codex::Codex),
        Box::new(adapters::generic::Generic::opencode()),
        Box::new(adapters::generic::Generic::gemini()),
    ]
}

/// Probe one tool: binary, version, sign-in and (with a project) its EAK registration.
pub fn probe(tool: &dyn AgentTool, loc: &Locator, project: Option<&Path>) -> ToolStatus {
    let mut st = ToolStatus {
        id: tool.id().into(),
        label: tool.label().into(),
        install_url: tool.install_url().into(),
        binary: None,
        version: None,
        available: false,
        reason: None,
        signed_in: None,
        auth_detail: None,
        connected: None,
        connection_detail: None,
        connection_scope: tool.connect_scope().map(str::to_string),
        can_connect: tool.connect_scope().is_some(),
        can_login: tool.login_args().is_some(),
        surfaces: tool.surfaces(),
    };
    let Some(bin) = loc.find(tool.program()) else {
        st.reason = Some(Unavailable::MissingBinary);
        st.connection_detail = Some(format!("{} is not installed", tool.label()));
        return st;
    };
    st.binary = Some(bin.display().to_string());
    match probe::run(&bin, &["--version"], None, probe::PROBE_TIMEOUT) {
        Ok(o) => {
            st.version = probe::parse_version(&o.stdout).or_else(|| probe::parse_version(&o.stderr))
        }
        Err(e) => {
            st.reason = Some(Unavailable::Other(e));
            return st;
        }
    }
    let auth = tool.auth(&bin);
    st.signed_in = auth.signed_in;
    st.auth_detail = auth.detail;
    if st.signed_in == Some(false) {
        st.reason = Some(Unavailable::NotSignedIn);
    }
    st.available = st.signed_in != Some(false);
    match project {
        Some(p) => {
            let r = tool.registration(&bin, p);
            st.connected = r.connected;
            st.connection_detail = r.detail;
        }
        None => st.connection_detail = Some("open a project to connect it".into()),
    }
    st
}

/// Find a built-in tool by id.
pub fn find_tool<'a>(
    tools: &'a [Box<dyn AgentTool>],
    id: &str,
) -> Result<&'a dyn AgentTool, String> {
    tools
        .iter()
        .find(|t| t.id() == id)
        .map(|t| t.as_ref())
        .ok_or_else(|| format!("no AI tool named {id}"))
}

/// The plan for a session of `tool` in `project`, with the environment the harness reads.
pub fn launch_plan(
    tool: &dyn AgentTool,
    bin: &Path,
    project: &Path,
    surface: Surface,
    prompt: Option<&str>,
    task_id: &str,
) -> Result<LaunchPlan, String> {
    if !tool.surfaces().contains(&surface) {
        return Err(format!(
            "{} cannot run {}",
            tool.label(),
            match surface {
                Surface::Terminal => "in a terminal tab",
                Surface::Headless => "headless",
            }
        ));
    }
    let prompt = prompt.map(str::trim).filter(|p| !p.is_empty());
    if surface == Surface::Headless && prompt.is_none() {
        return Err("a headless session needs a first message (the task)".into());
    }
    let mut argv = vec![bin.display().to_string()];
    argv.extend(tool.launch_args(surface, prompt)?);
    Ok(LaunchPlan {
        argv,
        env: vec![
            ("EAK_PROJECT_DIR".into(), project.display().to_string()),
            ("EAK_AGENT_NAME".into(), tool.id().into()),
            ("EAK_TASK_ID".into(), task_id.into()),
        ],
        cwd: project.to_path_buf(),
        surface,
    })
}

/// The tool's own sign-in, to run in a terminal tab.
pub fn login_plan(tool: &dyn AgentTool, bin: &Path, cwd: &Path) -> Result<LaunchPlan, String> {
    let args = tool.login_args().ok_or_else(|| {
        format!(
            "{} has no sign-in command EAK knows; start it in a terminal and sign in there",
            tool.label()
        )
    })?;
    let mut argv = vec![bin.display().to_string()];
    argv.extend(args);
    Ok(LaunchPlan {
        argv,
        env: vec![("EAK_AGENT_NAME".into(), tool.id().into())],
        cwd: cwd.to_path_buf(),
        surface: Surface::Terminal,
    })
}

/// The command EAK registers: `eak-mcp --project <dir> --agent <tool>`.
pub fn server_command(server: &Path, project: &Path, agent: &str) -> Vec<String> {
    vec![
        server.display().to_string(),
        "--project".into(),
        project.display().to_string(),
        "--agent".into(),
        agent.into(),
    ]
}

/// Truncate to `n` characters, adding an ellipsis.
pub(crate) fn clip(s: &str, n: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= n {
        t.to_string()
    } else {
        let mut out: String = t.chars().take(n).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tool_that_is_not_installed_is_reported_as_not_found_on_every_os() {
        let loc = Locator::only(vec![]);
        for t in builtin_tools() {
            let st = probe(t.as_ref(), &loc, Some(Path::new("/work/board")));
            assert_eq!(st.reason, Some(Unavailable::MissingBinary), "{}", st.id);
            assert!(!st.available);
            assert_eq!(st.signed_in, None, "unknown, never yes");
            assert_eq!(st.connected, None);
            assert!(st.binary.is_none() && st.version.is_none());
        }
    }

    #[test]
    fn launch_plans_carry_the_agent_name_and_task_id() {
        let tools = builtin_tools();
        let claude = find_tool(&tools, "claude").unwrap();
        let p = launch_plan(
            claude,
            Path::new("/opt/claude"),
            Path::new("/work/board"),
            Surface::Terminal,
            None,
            "claude-1-1",
        )
        .unwrap();
        assert_eq!(p.argv, vec!["/opt/claude".to_string()]);
        assert!(p
            .env
            .contains(&("EAK_AGENT_NAME".to_string(), "claude".to_string())));
        assert!(p
            .env
            .contains(&("EAK_TASK_ID".to_string(), "claude-1-1".to_string())));
        assert_eq!(p.cwd, Path::new("/work/board"));
        // Headless needs a task; tools without a confirmed headless mode refuse it.
        assert!(launch_plan(
            claude,
            Path::new("/opt/claude"),
            Path::new("/w"),
            Surface::Headless,
            Some("  "),
            "x"
        )
        .is_err());
        let gemini = find_tool(&tools, "gemini").unwrap();
        assert!(launch_plan(
            gemini,
            Path::new("/opt/gemini"),
            Path::new("/w"),
            Surface::Headless,
            Some("go"),
            "x"
        )
        .is_err());
        assert!(login_plan(gemini, Path::new("/opt/gemini"), Path::new("/w")).is_err());
        let login = login_plan(claude, Path::new("/opt/claude"), Path::new("/w")).unwrap();
        assert_eq!(login.argv, vec!["/opt/claude", "auth", "login"]);
        assert!(find_tool(&tools, "nope").is_err());
    }
}
