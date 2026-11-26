//! Claude Code. Commands confirmed with `claude --help` of Claude Code 2.1:
//! - `claude --version` → `2.1.332 (Claude Code)`;
//! - `claude auth status --json` → `{"loggedIn": true, "authMethod": "claude.ai", ...}` (EAK reads
//!   only `loggedIn` and `authMethod`; account fields are never read into EAK);
//! - `claude auth login` → the tool's own sign-in;
//! - `claude mcp add -s local <name> -- <command> [args...]`, `claude mcp get <name>`,
//!   `claude mcp remove <name> -s local`; local scope is keyed to the folder the command runs in
//!   and kept in the user's Claude config, so nothing is written into the project folder;
//! - headless: `claude -p <prompt> --output-format stream-json --verbose`, one JSON event per line.

use super::{check_prompt, line_value};
use crate::probe::{self, Output, CHANGE_TIMEOUT, PROBE_TIMEOUT};
use crate::{
    clip, server_command, AgentEvent, AgentTool, Auth, Parser, Registration, Surface,
    MCP_SERVER_NAME,
};
use serde_json::Value;
use std::path::Path;

pub struct Claude;

impl AgentTool for Claude {
    fn id(&self) -> &'static str {
        "claude"
    }
    fn label(&self) -> &'static str {
        "Claude Code"
    }
    fn program(&self) -> &'static str {
        "claude"
    }
    fn install_url(&self) -> &'static str {
        "https://docs.anthropic.com/en/docs/claude-code/setup"
    }

    fn auth(&self, bin: &Path) -> Auth {
        match probe::run(bin, &["auth", "status", "--json"], None, PROBE_TIMEOUT) {
            Ok(o) => parse_auth(&o),
            Err(e) => Auth {
                signed_in: None,
                detail: Some(e),
            },
        }
    }

    fn connect_scope(&self) -> Option<&'static str> {
        Some("Claude Code local scope: this project folder, private to you")
    }

    fn registration(&self, bin: &Path, project: &Path) -> Registration {
        match probe::run(
            bin,
            &["mcp", "get", MCP_SERVER_NAME],
            Some(project),
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
        // Replace an earlier local registration so it always points at this app's server.
        let _ = probe::run(
            bin,
            &["mcp", "remove", MCP_SERVER_NAME, "-s", "local"],
            Some(project),
            CHANGE_TIMEOUT,
        );
        let mut args: Vec<String> = ["mcp", "add", "-s", "local", MCP_SERVER_NAME, "--"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        args.extend(server_command(server, project, "claude-code"));
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let o = probe::run(bin, &a, Some(project), CHANGE_TIMEOUT)?;
        if o.ok() {
            Ok(())
        } else {
            Err(format!("claude mcp add failed: {}", o.message()))
        }
    }

    fn disconnect(&self, bin: &Path, project: &Path) -> Result<(), String> {
        let o = probe::run(
            bin,
            &["mcp", "remove", MCP_SERVER_NAME, "-s", "local"],
            Some(project),
            CHANGE_TIMEOUT,
        )?;
        if o.ok() {
            Ok(())
        } else {
            Err(format!("claude mcp remove failed: {}", o.message()))
        }
    }

    fn login_args(&self) -> Option<Vec<String>> {
        Some(vec!["auth".into(), "login".into()])
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
                // EAK's own MCP tools are allowed without a prompt (nobody can answer one in a
                // headless run); the harness still bounds them (standing approval, no
                // destructive change, every design change through `Engine::execute`).
                Ok(vec![
                    "--allowedTools".into(),
                    format!("mcp__{MCP_SERVER_NAME}"),
                    "--output-format".into(),
                    "stream-json".into(),
                    "--verbose".into(),
                    "-p".into(),
                    p.to_string(),
                ])
            }
        }
    }

    fn progress_parser(&self) -> Option<Parser> {
        Some(parse_stream_json)
    }
}

/// `claude auth status --json`. Only `loggedIn` and `authMethod` are read.
pub fn parse_auth(o: &Output) -> Auth {
    let v: Option<Value> = serde_json::from_str(o.stdout.trim()).ok();
    let logged_in = v
        .as_ref()
        .and_then(|v| v.get("loggedIn"))
        .and_then(Value::as_bool);
    match logged_in {
        Some(true) => Auth {
            signed_in: Some(true),
            detail: v
                .as_ref()
                .and_then(|v| v.get("authMethod"))
                .and_then(Value::as_str)
                .map(method_label),
        },
        Some(false) => Auth {
            signed_in: Some(false),
            detail: Some("not signed in: use Sign in (claude auth login)".into()),
        },
        None => Auth {
            signed_in: None,
            detail: Some(format!(
                "`claude auth status` gave no answer EAK can read: {}",
                clip(&o.message(), 160)
            )),
        },
    }
}

fn method_label(m: &str) -> String {
    let plain = m.len() <= 32
        && m.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
    match m {
        "claude.ai" => "signed in with a claude.ai account".into(),
        "api_key" | "apiKey" | "console" => "signed in with an Anthropic API key".into(),
        other if plain => format!("signed in ({other})"),
        _ => "signed in".into(),
    }
}

/// `claude mcp get eak`, run in the project folder.
pub fn parse_registration(o: &Output, project: &Path) -> Registration {
    if !o.ok() {
        let text = format!("{}{}", o.stdout, o.stderr);
        return if text.contains("No MCP server named") {
            Registration {
                connected: Some(false),
                detail: Some("EAK is not registered with Claude Code for this project".into()),
            }
        } else {
            Registration {
                connected: None,
                detail: Some(format!(
                    "`claude mcp get` failed: {}",
                    clip(&o.message(), 160)
                )),
            }
        };
    }
    let scope = line_value(&o.stdout, "Scope:").unwrap_or("scope not shown");
    let args = line_value(&o.stdout, "Args:").unwrap_or_default();
    if args.contains(&project.display().to_string()) {
        Registration {
            connected: Some(true),
            detail: Some(format!("registered: {scope}")),
        }
    } else {
        Registration {
            connected: Some(false),
            detail: Some(format!(
                "an `{MCP_SERVER_NAME}` server is registered ({scope}) but not for this project folder; Connect adds one for this project"
            )),
        }
    }
}

/// One line of `--output-format stream-json`.
pub fn parse_stream_json(line: &str) -> Option<AgentEvent> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    match v.get("type")?.as_str()? {
        "system" => Some(AgentEvent::Started {
            detail: v
                .get("model")
                .and_then(Value::as_str)
                .map(|m| format!("session started (model {m})"))
                .unwrap_or_else(|| "session started".into()),
        }),
        "assistant" => {
            let content = v.pointer("/message/content")?.as_array()?;
            if let Some(tool) = content
                .iter()
                .find(|c| c.get("type").and_then(Value::as_str) == Some("tool_use"))
            {
                return Some(AgentEvent::ToolCall {
                    name: tool.get("name")?.as_str()?.to_string(),
                });
            }
            let text = content
                .iter()
                .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|c| c.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" ");
            if text.trim().is_empty() {
                None
            } else {
                Some(AgentEvent::Message {
                    text: clip(&text, 300),
                })
            }
        }
        "result" => {
            let is_error = v.get("is_error").and_then(Value::as_bool).unwrap_or(false);
            let subtype = v.get("subtype").and_then(Value::as_str).unwrap_or("");
            let summary = v.get("result").and_then(Value::as_str).unwrap_or(subtype);
            Some(AgentEvent::Finished {
                ok: !is_error && subtype == "success",
                summary: clip(summary, 300),
            })
        }
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
    fn auth_reads_logged_in_and_method_only() {
        let a = parse_auth(&out(
            0,
            r#"{"loggedIn":true,"authMethod":"claude.ai","email":"someone@example.com"}"#,
        ));
        assert_eq!(a.signed_in, Some(true));
        let d = a.detail.unwrap();
        assert!(d.contains("claude.ai"));
        assert!(!d.contains("example.com"), "no account data in EAK: {d}");
        assert_eq!(
            parse_auth(&out(1, r#"{"loggedIn":false}"#)).signed_in,
            Some(false)
        );
        // An old Claude Code without `auth status`: unknown, never "yes".
        assert_eq!(
            parse_auth(&out(1, "error: unknown command 'auth'")).signed_in,
            None
        );
    }

    #[test]
    fn registration_must_name_this_project() {
        let p = Path::new("/work/board");
        let get = "eak:\n  Scope: Local config (private to you in this project)\n  Command: /opt/eak/eak-mcp\n  Args: --project /work/board --agent claude-code\n";
        assert_eq!(parse_registration(&out(0, get), p).connected, Some(true));
        let other = get.replace("/work/board", "/work/other");
        assert_eq!(
            parse_registration(&out(0, &other), p).connected,
            Some(false)
        );
        let missing = out(1, "No MCP server named \"eak\". Configured servers: x");
        assert_eq!(parse_registration(&missing, p).connected, Some(false));
        assert_eq!(parse_registration(&out(2, "boom"), p).connected, None);
    }

    #[test]
    fn stream_json_lines_become_events() {
        assert!(matches!(
            parse_stream_json(r#"{"type":"system","subtype":"init","model":"m"}"#),
            Some(AgentEvent::Started { .. })
        ));
        assert_eq!(
            parse_stream_json(
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__eak__design_report","input":{}}]}}"#
            ),
            Some(AgentEvent::ToolCall {
                name: "mcp__eak__design_report".into()
            })
        );
        assert_eq!(
            parse_stream_json(
                r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done."}]}}"#
            ),
            Some(AgentEvent::Message {
                text: "Done.".into()
            })
        );
        assert_eq!(
            parse_stream_json(
                r#"{"type":"result","subtype":"success","is_error":false,"result":"ok"}"#
            ),
            Some(AgentEvent::Finished {
                ok: true,
                summary: "ok".into()
            })
        );
        assert_eq!(parse_stream_json("not json"), None);
    }

    #[test]
    fn headless_launch_uses_print_mode_and_refuses_option_like_prompts() {
        let a = Claude
            .launch_args(Surface::Headless, Some("add a USB-C input"))
            .unwrap();
        assert_eq!(a.last().map(String::as_str), Some("add a USB-C input"));
        assert!(a.contains(&"stream-json".to_string()));
        assert!(Claude.launch_args(Surface::Headless, None).is_err());
        assert!(Claude
            .launch_args(Surface::Terminal, Some("--dangerous"))
            .is_err());
    }
}
