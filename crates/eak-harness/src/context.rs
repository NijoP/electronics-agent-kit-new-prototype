//! Explicit call context. Every harness call names its project (folder path, optionally the
//! expected project id) and its session; nothing is read from a process-wide "active project".
//!
//! Defaults come from the environment an EAK terminal session exports (context contract with
//! the terminal workspace): `EAK_PROJECT_DIR`, `EAK_PROJECT_ID`, `EAK_SESSION_ID`,
//! `EAK_HARNESS_SOCKET`, `EAK_HARNESS_TOKEN_FILE`; plus `EAK_AGENT_NAME` and
//! `EAK_HARNESS_ALLOW_DESTRUCTIVE=1`. Explicit flags or tool arguments always win, and every
//! response echoes the project id and revision it acted on.

use eak_runtime::OperationClass;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const ENV_PROJECT_DIR: &str = "EAK_PROJECT_DIR";
pub const ENV_PROJECT_ID: &str = "EAK_PROJECT_ID";
pub const ENV_SESSION_ID: &str = "EAK_SESSION_ID";
pub const ENV_AGENT_NAME: &str = "EAK_AGENT_NAME";
pub const ENV_SOCKET: &str = "EAK_HARNESS_SOCKET";
pub const ENV_TOKEN_FILE: &str = "EAK_HARNESS_TOKEN_FILE";
pub const ENV_ALLOW_DESTRUCTIVE: &str = "EAK_HARNESS_ALLOW_DESTRUCTIVE";

/// Which project a call targets.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectRef {
    /// Absolute project folder (the folder holding `project.eak.json`).
    pub dir: Option<PathBuf>,
    /// When set, the call is refused unless the folder holds exactly this project.
    pub id: Option<String>,
}

/// Who is calling and under which standing approval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallContext {
    pub project: ProjectRef,
    pub session_id: String,
    pub agent: String,
    /// Highest operation class the session may execute. Defaults to engineering-impacting;
    /// destructive commands (DeleteSymbol, ClearRouting) need an explicit opt-in by the user
    /// who starts the agent.
    pub max_class: OperationClass,
    /// Explicit app endpoint (socket + token file), if the terminal exported one.
    #[serde(default)]
    pub socket: Option<String>,
    #[serde(default)]
    pub token_file: Option<PathBuf>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl CallContext {
    /// Context from the environment only (flags are applied on top by the CLI).
    pub fn from_env() -> CallContext {
        let allow_destructive = env(ENV_ALLOW_DESTRUCTIVE).is_some_and(|v| v == "1" || v == "true");
        CallContext {
            project: ProjectRef {
                dir: env(ENV_PROJECT_DIR).map(PathBuf::from),
                id: env(ENV_PROJECT_ID),
            },
            session_id: env(ENV_SESSION_ID)
                .unwrap_or_else(|| format!("cli-{}", std::process::id())),
            agent: env(ENV_AGENT_NAME).unwrap_or_else(|| "external-agent".into()),
            max_class: if allow_destructive {
                OperationClass::Destructive
            } else {
                OperationClass::EngineeringImpacting
            },
            socket: env(ENV_SOCKET),
            token_file: env(ENV_TOKEN_FILE).map(PathBuf::from),
        }
    }

    /// Apply per-call overrides (`project_dir` / `project_id` tool arguments).
    pub fn with_args(&self, args: &serde_json::Value) -> CallContext {
        let mut c = self.clone();
        if let Some(d) = args.get("project_dir").and_then(|v| v.as_str()) {
            if c.project.dir.as_deref() != Some(std::path::Path::new(d)) {
                // A different folder: an id taken from the environment no longer applies.
                c.project.id = None;
            }
            c.project.dir = Some(PathBuf::from(d));
        }
        if let Some(i) = args.get("project_id").and_then(|v| v.as_str()) {
            c.project.id = Some(i.into());
        }
        c
    }
}
