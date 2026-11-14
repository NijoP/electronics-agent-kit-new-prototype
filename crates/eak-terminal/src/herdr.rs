//! Herdr sidecar adapter (ADR-0016, docs/HERDR_INTEGRATION.md).
//!
//! EAK's Terminal workspace *is* Herdr: EAK supervises a Herdr server on its own dedicated
//! session, shows Herdr's real client UI in a PTY (rendered by xterm.js), and talks to the
//! server through Herdr's CLI/socket API for health, project spaces and agent status.
//! Terminal processes are owned by the Herdr server, never by EAK.
//!
//! Isolation: EAK never uses Herdr's `default` session, always passes `--session <name>`
//! on every call, and strips inherited `HERDR_*` variables so a Herdr-hosted parent (for
//! example a developer running EAK inside Herdr) can never be addressed by accident.

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Session EAK uses unless `EAK_HERDR_SESSION` names another one.
pub const DEFAULT_SESSION: &str = "eak-desktop";

/// Variables a parent Herdr pane exports; never passed on to EAK's own Herdr calls.
pub const INHERITED_VARS: &[&str] = &[
    "HERDR_SOCKET_PATH",
    "HERDR_PANE_ID",
    "HERDR_TAB_ID",
    "HERDR_WORKSPACE_ID",
    "HERDR_SESSION",
    "HERDR_ENV",
    "HERDR_BIN_PATH",
];

#[derive(Debug, Clone)]
pub struct Herdr {
    pub bin: PathBuf,
    pub session: String,
}

/// Session names EAK accepts: not Herdr's default, short and path-safe.
pub fn valid_session(name: &str) -> Result<(), String> {
    if name.is_empty() || name == "default" {
        return Err(format!(
            "refusing Herdr session `{name}`: EAK uses its own dedicated session"
        ));
    }
    if name.len() > 48
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("invalid Herdr session name `{name}`"));
    }
    Ok(())
}

impl Herdr {
    pub fn new(bin: PathBuf, session: &str) -> Result<Herdr, String> {
        valid_session(session)?;
        Ok(Herdr {
            bin,
            session: session.to_string(),
        })
    }

    /// From the environment: `EAK_HERDR_BIN` (else `herdr` found by `locate`) and
    /// `EAK_HERDR_SESSION` (else [`DEFAULT_SESSION`]). `None` when Herdr is not installed.
    pub fn from_env(locate: impl Fn(&str) -> Option<PathBuf>) -> Option<Result<Herdr, String>> {
        let bin = std::env::var_os("EAK_HERDR_BIN")
            .map(PathBuf::from)
            .or_else(|| locate("herdr"))?;
        let session = std::env::var("EAK_HERDR_SESSION").unwrap_or_else(|_| DEFAULT_SESSION.into());
        Some(Herdr::new(bin, &session))
    }

    /// A Herdr command for this session: `--session` is appended last (after the
    /// subcommand's own options), as Herdr's CLI expects.
    pub fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(&self.bin);
        c.args(args).arg("--session").arg(&self.session);
        for v in INHERITED_VARS {
            c.env_remove(v);
        }
        c.stdin(Stdio::null());
        c
    }

    /// argv for an interactive Herdr client attached to this session (run in a PTY).
    pub fn client_argv(&self) -> Vec<String> {
        vec![
            self.bin.display().to_string(),
            "--session".into(),
            self.session.clone(),
        ]
    }

    fn output(&self, args: &[&str], timeout: Duration) -> Result<String, String> {
        let mut child = self
            .command(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run {}: {e}", self.bin.display()))?;
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(_) => break,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("herdr {} timed out", args.join(" ")));
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        if out.status.success() {
            Ok(text)
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!(
                "herdr {}: {}",
                args.join(" "),
                if err.trim().is_empty() {
                    text.trim()
                } else {
                    err.trim()
                }
            ))
        }
    }

    /// One API call through Herdr's CLI; returns the `result` object of its JSON reply.
    pub fn call(&self, args: &[&str]) -> Result<Value, String> {
        let text = self.output(args, Duration::from_secs(10))?;
        let v: Value = serde_json::from_str(text.trim())
            .map_err(|e| format!("herdr {}: unexpected reply ({e})", args.join(" ")))?;
        match v.get("error") {
            Some(e) if !e.is_null() => Err(format!("herdr {}: {e}", args.join(" "))),
            _ => Ok(v.get("result").cloned().unwrap_or(v)),
        }
    }

    /// `status server`: whether this session's server runs, and its version.
    pub fn status(&self) -> ServerStatus {
        match self.output(&["status", "server"], Duration::from_secs(5)) {
            Ok(text) => parse_status(&text),
            Err(e) => ServerStatus {
                running: false,
                version: None,
                error: Some(e),
            },
        }
    }

    /// Start this session's server as a child of EAK (Herdr's `server` runs in the
    /// foreground, which is what a supervisor wants).
    pub fn spawn_server(&self) -> Result<Child, String> {
        self.command(&["server"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot start herdr server: {e}"))
    }

    /// The Herdr workspace for a project: an existing one with the same label whose root
    /// pane sits in `dir`, otherwise a new one created there with the EAK context
    /// variables in its root shell. Returns the workspace id.
    pub fn ensure_workspace(
        &self,
        label: &str,
        dir: &Path,
        env: &[(String, String)],
    ) -> Result<String, String> {
        let dir_s = dir.display().to_string();
        let panes = self.call(&["pane", "list"])?;
        let spaces = self.call(&["workspace", "list"])?;
        let in_dir: Vec<&str> = panes["panes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["cwd"].as_str() == Some(dir_s.as_str()))
            .filter_map(|p| p["workspace_id"].as_str())
            .collect();
        if let Some(w) = spaces["workspaces"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|w| {
                w["label"].as_str() == Some(label)
                    && w["workspace_id"]
                        .as_str()
                        .is_some_and(|id| in_dir.contains(&id))
            })
        {
            let id = w["workspace_id"].as_str().unwrap_or_default().to_string();
            self.call(&["workspace", "focus", &id])?;
            return Ok(id);
        }
        let mut args: Vec<String> = vec![
            "workspace".into(),
            "create".into(),
            "--cwd".into(),
            dir_s,
            "--label".into(),
            label.into(),
            "--focus".into(),
        ];
        for (k, v) in env {
            args.push("--env".into());
            args.push(format!("{k}={v}"));
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let r = self.call(&refs)?;
        r["workspace"]["workspace_id"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| "herdr workspace create: no workspace id in reply".into())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ServerStatus {
    pub running: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

fn parse_status(text: &str) -> ServerStatus {
    let field = |k: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}:")))
            .map(|v| v.trim().to_string())
    };
    ServerStatus {
        running: field("status").as_deref() == Some("running"),
        version: field("version"),
        error: None,
    }
}

/// Supervisor health as shown in the Terminal workspace.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Health {
    pub session: String,
    pub bin: String,
    pub running: bool,
    pub version: Option<String>,
    /// True when this EAK process started the server (false: it was already running,
    /// e.g. left from an earlier EAK run — Herdr keeps sessions alive across detach).
    pub started_by_eak: bool,
    pub restarts: u32,
    pub error: Option<String>,
}

/// Keeps the session's Herdr server running: checks health, starts it when missing,
/// restarts it (with backoff) if it dies, and stops only a server it started itself.
pub struct Supervisor {
    pub herdr: Herdr,
    child: Mutex<Option<Child>>,
    health: Mutex<Health>,
}

impl Supervisor {
    pub fn new(herdr: Herdr) -> Arc<Supervisor> {
        let health = Health {
            session: herdr.session.clone(),
            bin: herdr.bin.display().to_string(),
            running: false,
            version: None,
            started_by_eak: false,
            restarts: 0,
            error: None,
        };
        Arc::new(Supervisor {
            herdr,
            child: Mutex::new(None),
            health: Mutex::new(health),
        })
    }

    pub fn health(&self) -> Health {
        self.health
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// One supervision step: probe, and start the server if it is not running.
    pub fn check(&self) -> Health {
        let st = self.herdr.status();
        let mut h = self.health.lock().unwrap_or_else(|e| e.into_inner());
        h.running = st.running;
        h.version = st.version.or(h.version.take());
        if st.running {
            h.error = None;
            return h.clone();
        }
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = child.as_mut() {
            if matches!(c.try_wait(), Ok(None)) {
                // Started and still booting.
                return h.clone();
            }
            h.restarts += 1;
        }
        match self.herdr.spawn_server() {
            Ok(c) => {
                *child = Some(c);
                h.started_by_eak = true;
                h.error = None;
            }
            Err(e) => h.error = Some(e),
        }
        h.clone()
    }

    /// Wait until the server answers (after [`Supervisor::check`] started it).
    pub fn wait_ready(&self, timeout: Duration) -> Health {
        let deadline = Instant::now() + timeout;
        loop {
            let h = self.check();
            if h.running || Instant::now() >= deadline {
                return h;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Background supervision every `every`, with the restart count capped by backoff.
    pub fn watch(self: &Arc<Self>, every: Duration) {
        let me = Arc::clone(self);
        std::thread::spawn(move || loop {
            let h = me.check();
            let pause = if h.running {
                every
            } else {
                every * (1 + h.restarts.min(6))
            };
            std::thread::sleep(pause);
        });
    }

    /// Stop the server only if this EAK process started it. Returns whether it did.
    pub fn stop_owned(&self) -> bool {
        let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
        match child.take() {
            Some(mut c) => {
                let _ = c.kill();
                let _ = c.wait();
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_and_odd_session_names_are_refused() {
        assert!(valid_session("default").is_err());
        assert!(valid_session("").is_err());
        assert!(valid_session("a/b").is_err());
        assert!(valid_session(DEFAULT_SESSION).is_ok());
        assert!(valid_session("fm-lab-eak-1-2").is_ok());
    }

    #[test]
    fn every_call_names_the_session_and_drops_inherited_herdr_vars() {
        let h = Herdr::new(PathBuf::from("herdr"), "eak-desktop").unwrap();
        let c = h.command(&["workspace", "list"]);
        let args: Vec<_> = c
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, ["workspace", "list", "--session", "eak-desktop"]);
        for v in INHERITED_VARS {
            assert!(
                c.get_envs().any(|(k, val)| k == *v && val.is_none()),
                "{v} must be removed"
            );
        }
        assert_eq!(
            h.client_argv()[1..],
            ["--session".to_string(), "eak-desktop".into()]
        );
    }

    #[test]
    fn status_text_is_parsed() {
        let s = parse_status("status: running\nversion: 0.7.4\nprotocol: 16\n");
        assert_eq!((s.running, s.version.as_deref()), (true, Some("0.7.4")));
        assert!(!parse_status("status: stopped\n").running);
    }
}
