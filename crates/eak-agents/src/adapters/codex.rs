//! Codex CLI. Commands confirmed with `codex --help` of codex-cli 0.148:
//! - `codex --version` → `codex-cli 0.148.0`;
//! - `codex login status` → exit 0 when signed in, exit 1 and "Not logged in" when not (EAK
//!   reads only the exit code and whether the line names ChatGPT or an API key);
//! - `codex login` → the tool's own sign-in;
//! - `codex mcp add <name> -- <command>...`, `codex mcp get <name> --json`, `codex mcp remove
//!   <name>`. Codex keeps MCP servers in its user config (`~/.codex/config.toml`), not per
//!   project, so EAK's registration names one project folder at a time;
//! - headless: `codex exec --json <prompt>`, one JSON event per line.

use super::check_prompt;
use crate::probe::{self, Output, CHANGE_TIMEOUT, PROBE_TIMEOUT};
use crate::{
    clip, server_command, AgentEvent, AgentTool, Auth, Parser, Registration, Surface,
    MCP_SERVER_NAME,
};
use serde_json::Value;
use std::path::Path;

pub struct Codex;

impl AgentTool for Codex {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn label(&self) -> &'static str {
        "Codex"
    }
    fn program(&self) -> &'static str {
        "codex"
    }
    fn install_url(&self) -> &'static str {
        "https://developers.openai.com/codex/cli"
    }

    fn auth(&self, bin: &Path) -> Auth {
        match probe::run(bin, &["login", "status"], None, PROBE_TIMEOUT) {
            Ok(o) => parse_auth(&o),
            Err(e) => Auth {
                signed_in: None,
                detail: Some(e),
            },
        }
    }

    fn connect_scope(&self) -> Option<&'static str> {
        Some("Codex user config (~/.codex/config.toml): one EAK project at a time for every Codex session on this computer")
    }

    fn registration(&self, bin: &Path, project: &Path) -> Registration {
        match probe::run(
            bin,
            &["mcp", "get", MCP_SERVER_NAME, "--json"],
            None,
            PROBE_TIMEOUT,
        ) {
            Ok(o) => parse_registration(&o, project),
            Err(e) => Registration {
                connected: None,
                detail: Some(e),
            },
        }
    }

    fn connect(&self, bin: &Path, project: &Path, server: &Path) -> Result<(), String> {
        // One entry per name: remove first so the registration is exactly EAK's for this project.
        let _ = probe::run(
            bin,
            &["mcp", "remove", MCP_SERVER_NAME],
            None,
            CHANGE_TIMEOUT,
        );
        let mut args: Vec<String> = ["mcp", "add", MCP_SERVER_NAME, "--"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        args.extend(server_command(server, project, "codex"));
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let o = probe::run(bin, &a, Some(project), CHANGE_TIMEOUT)?;
        if o.ok() {
            Ok(())
        } else {
            Err(format!("codex mcp add failed: {}", o.message()))
        }
    }

    fn disconnect(&self, bin: &Path, project: &Path) -> Result<(), String> {
        // Codex's registration is shared by every project: only remove it when it is ours.
        let r = self.registration(bin, project);
        if r.connected != Some(true) {
            return Err(r
                .detail
                .unwrap_or_else(|| "EAK is not registered with Codex for this project".into()));
        }
        let o = probe::run(
            bin,
            &["mcp", "remove", MCP_SERVER_NAME],
            None,
            CHANGE_TIMEOUT,
        )?;
        if o.ok() {
            Ok(())
        } else {
            Err(format!("codex mcp remove failed: {}", o.message()))
        }
    }

    fn login_args(&self) -> Option<Vec<String>> {
        Some(vec!["login".into()])
    }

    fn surfaces(&self) -> Vec<Surface> {
        vec![Surface::Terminal, Surface::Headless]
    }

    fn launch_args(&self, surface: Surface, prompt: Option<&str>) -> Result<Vec<String>, String> {
        if let Some(p) = prompt {
            check_prompt(p)?;
        }
        match surface {
            Surface::Terminal => Ok(prompt.map(|p| vec![p.to_string()]).unwrap_or_default()),
            Surface::Headless => {
                let p = prompt.ok_or("a headless session needs a first message")?;
                Ok(vec![
                    "exec".into(),
                    "--json".into(),
                    "--skip-git-repo-check".into(),
                    p.to_string(),
                ])
            }
        }
    }

    fn progress_parser(&self) -> Option<Parser> {
        Some(parse_exec_json)
    }
}

/// `codex login status`: the exit code, plus which kind of sign-in the line names.
pub fn parse_auth(o: &Output) -> Auth {
    let text = format!("{}\n{}", o.stdout, o.stderr);
    if o.ok() {
        let how = if text.contains("ChatGPT") {
            "signed in with a ChatGPT account"
        } else if text.contains("API key") {
            "signed in with an OpenAI API key"
        } else {
            "signed in"
        };
        return Auth {
            signed_in: Some(true),
            detail: Some(how.into()),
        };
    }
    if text.to_ascii_lowercase().contains("not logged in") {
        return Auth {
            signed_in: Some(false),
            detail: Some("not signed in: use Sign in (codex login)".into()),
        };
    }
    Auth {
        signed_in: None,
        detail: Some(format!(
            "`codex login status` gave no answer EAK can read: {}",
            clip(&o.message(), 160)
        )),
    }
}

/// `codex mcp get eak --json`: connected when its arguments name this project folder.
pub fn parse_registration(o: &Output, project: &Path) -> Registration {
    if !o.ok() {
        return Registration {
            connected: Some(false),
            detail: Some("EAK is not registered with Codex".into()),
        };
    }
    let Ok(v) = serde_json::from_str::<Value>(o.stdout.trim()) else {
        return Registration {
            connected: None,
            detail: Some("`codex mcp get --json` gave no JSON EAK can read".into()),
        };
    };
    let args: Vec<&str> = v
        .pointer("/transport/args")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let dir = project.display().to_string();
    if args.contains(&dir.as_str()) {
        return Registration {
            connected: Some(true),
            detail: Some("registered in Codex's user config for this project".into()),
        };
    }
    let detail = match args
        .iter()
        .position(|a| *a == "--project")
        .and_then(|i| args.get(i + 1))
    {
        Some(d) => format!(
            "Codex's `{MCP_SERVER_NAME}` server points at another project ({d}); Connect moves it to this one"
        ),
        None => format!(
            "an `{MCP_SERVER_NAME}` server is registered in Codex but not for this project; Connect replaces it"
        ),
    };
    Registration {
        connected: Some(false),
        detail: Some(detail),
    }
}

/// One line of `codex exec --json`.
pub fn parse_exec_json(line: &str) -> Option<AgentEvent> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let s = |p: &str| v.pointer(p).and_then(Value::as_str);
    match s("/type")? {
        "thread.started" => Some(AgentEvent::Started {
            detail: "session started".into(),
        }),
        "item.started" => match s("/item/type")? {
            "mcp_tool_call" => Some(AgentEvent::ToolCall {
                name: format!(
                    "{}.{}",
                    s("/item/server").unwrap_or("mcp"),
                    s("/item/tool").unwrap_or("?")
                ),
            }),
            "command_execution" => Some(AgentEvent::ToolCall {
                name: format!("shell: {}", clip(s("/item/command").unwrap_or(""), 80)),
            }),
            _ => None,
        },
        "item.completed" => match s("/item/type")? {
            "agent_message" => Some(AgentEvent::Message {
                text: clip(s("/item/text").unwrap_or(""), 300),
            }),
            _ => None,
        },
        "turn.completed" => Some(AgentEvent::Finished {
            ok: true,
            summary: "turn completed".into(),
        }),
        "turn.failed" => Some(AgentEvent::Finished {
            ok: false,
            summary: clip(s("/error/message").unwrap_or("turn failed"), 300),
        }),
        "error" => Some(AgentEvent::Error {
            message: clip(s("/message").unwrap_or("error"), 300),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(code: i32, stdout: &str) -> Output {
        Output {
            code: Some(code),
            stdout: stdout.into(),
            stderr: String::new(),
        }
    }

    #[test]
    fn login_status_exit_code_decides() {
        let a = parse_auth(&out(0, "Logged in using ChatGPT"));
        assert_eq!(a.signed_in, Some(true));
        assert!(a.detail.unwrap().contains("ChatGPT"));
        // The masked key Codex prints is never copied into EAK.
        let k = parse_auth(&out(0, "Logged in using an API key - sk-proj-***ABCD"));
        assert!(!k.detail.unwrap().contains("sk-"));
        assert_eq!(parse_auth(&out(1, "Not logged in")).signed_in, Some(false));
        assert_eq!(parse_auth(&out(2, "unexpected")).signed_in, None);
    }

    #[test]
    fn registration_names_the_project_it_points_at() {
        let p = Path::new("/work/board");
        let get = r#"{"name":"eak","transport":{"type":"stdio","command":"/opt/eak/eak-mcp","args":["--project","/work/board","--agent","codex"]}}"#;
        assert_eq!(parse_registration(&out(0, get), p).connected, Some(true));
        let other = parse_registration(&out(0, &get.replace("/work/board", "/work/x")), p);
        assert_eq!(other.connected, Some(false));
        assert!(other.detail.unwrap().contains("/work/x"));
        assert_eq!(
            parse_registration(&out(1, "No MCP server named 'eak' found."), p).connected,
            Some(false)
        );
    }

    #[test]
    fn exec_json_lines_become_events() {
        assert_eq!(
            parse_exec_json(
                r#"{"type":"item.started","item":{"id":"1","type":"mcp_tool_call","server":"eak","tool":"design_report","status":"in_progress"}}"#
            ),
            Some(AgentEvent::ToolCall {
                name: "eak.design_report".into()
            })
        );
        assert_eq!(
            parse_exec_json(
                r#"{"type":"item.completed","item":{"id":"2","type":"agent_message","text":"Added U1."}}"#
            ),
            Some(AgentEvent::Message {
                text: "Added U1.".into()
            })
        );
        assert!(matches!(
            parse_exec_json(r#"{"type":"turn.failed","error":{"message":"quota"}}"#),
            Some(AgentEvent::Finished { ok: false, .. })
        ));
        assert_eq!(parse_exec_json(r#"{"type":"turn.started"}"#), None);
    }
}
