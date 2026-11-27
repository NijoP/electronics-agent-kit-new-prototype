//! Tools EAK can find, version and start in a terminal tab, but not yet connect or run
//! headless: their MCP-registration commands are not confirmed, so those actions say so.

use crate::AgentTool;

pub struct Generic {
    id: &'static str,
    label: &'static str,
    program: &'static str,
    install_url: &'static str,
    login: Option<&'static [&'static str]>,
}

impl Generic {
    /// OpenCode: `opencode auth login` is its sign-in (`opencode auth --help`, 1.x).
    pub fn opencode() -> Generic {
        Generic {
            id: "opencode",
            label: "OpenCode",
            program: "opencode",
            install_url: "https://opencode.ai/docs/",
            login: Some(&["auth", "login"]),
        }
    }
    /// Gemini CLI signs in on its first interactive run; it has no separate login command.
    pub fn gemini() -> Generic {
        Generic {
            id: "gemini",
            label: "Gemini CLI",
            program: "gemini",
            install_url: "https://github.com/google-gemini/gemini-cli",
            login: None,
        }
    }
    /// Kimi Code (Moonshot's `kimi` CLI) signs in from inside its interactive session; it has no
    /// separate login or sign-in status command EAK can read, so its sign-in stays unknown.
    pub fn kimi() -> Generic {
        Generic {
            id: "kimi",
            label: "Kimi Code",
            program: "kimi",
            install_url: "https://github.com/MoonshotAI/kimi-cli",
            login: None,
        }
    }
}

impl AgentTool for Generic {
    fn id(&self) -> &'static str {
        self.id
    }
    fn label(&self) -> &'static str {
        self.label
    }
    fn program(&self) -> &'static str {
        self.program
    }
    fn install_url(&self) -> &'static str {
        self.install_url
    }
    fn login_args(&self) -> Option<Vec<String>> {
        self.login
            .map(|a| a.iter().map(|s| s.to_string()).collect())
    }
}
