//! Model Context Protocol server over stdio (JSON-RPC 2.0, one message per line). Exposes the
//! same tools as the CLI; each `tools/call` returns the harness envelope as text and as
//! `structuredContent`, with `isError` set when the envelope is not ok.

use crate::context::CallContext;
use crate::route;
use crate::tools::tool_specs;
use serde_json::{json, Value};
use std::io::{BufRead, Write};

const PROTOCOL: &str = "2025-06-18";

fn tools_list() -> Value {
    let tools: Vec<Value> = tool_specs()
        .into_iter()
        .map(|t| {
            json!({
                "name": t.name,
                "description": format!("[{}] {}", serde_json::to_value(t.category).unwrap_or_default().as_str().unwrap_or(""), t.description),
                "inputSchema": t.input_schema,
            })
        })
        .collect();
    json!({"tools": tools})
}

/// Handle one JSON-RPC message; `None` for notifications.
pub fn handle_message(ctx: &CallContext, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "initialize" => {
            let v = msg
                .pointer("/params/protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL);
            Ok(json!({
                "protocolVersion": v,
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "eak-harness", "version": env!("CARGO_PKG_VERSION")},
                "instructions": "EAK engineering harness. Call project_info first, pass its revision as expected_revision to design tools, verify with run_check, and read `guide` for the workflow. Lengths are integer nanometres.",
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list()),
        "tools/call" => {
            let name = msg
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = msg
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or(json!({}));
            let env = route::invoke(ctx, name, &args);
            let is_error = env["ok"] != json!(true);
            Ok(json!({
                "content": [{"type": "text", "text": env.to_string()}],
                "structuredContent": env,
                "isError": is_error,
            }))
        }
        m if m.starts_with("notifications/") => return None,
        _ => Err(json!({"code": -32601, "message": format!("method not found: {method}")})),
    };
    id.as_ref()?;
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err(e) => json!({"jsonrpc": "2.0", "id": id, "error": e}),
    })
}

/// Serve MCP on stdin/stdout until EOF.
/// Launch registration (ADR-0053): when the launcher set `EAK_AGENT_TOOL`, register the session
/// on the WEFT bus through the same `weft_register` tool path as any call. Tried after
/// `initialize` and, while it has not succeeded (for example the project does not exist yet),
/// after every tool call. Returns whether it is still pending.
fn try_register(ctx: &CallContext, pending: &Option<Value>) -> bool {
    let Some(args) = pending else {
        return false;
    };
    let env = route::invoke(ctx, "weft_register", args);
    if env["ok"] == json!(true) {
        eprintln!(
            "eak-mcp: registered on the WEFT bus as {}",
            env["result"]["agent"]["id"].as_str().unwrap_or("?")
        );
        false
    } else {
        true
    }
}

pub fn serve(ctx: &CallContext) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    let mut launch = crate::weft::launch_registration(|k| std::env::var(k).ok());
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => {
                let reply = handle_message(ctx, &msg);
                let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
                if launch.is_some()
                    && matches!(method, "initialize" | "tools/call")
                    && !try_register(ctx, &launch)
                {
                    launch = None;
                }
                reply
            }
            Err(e) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {e}")}}),
            ),
        };
        if let Some(r) = reply {
            writeln!(out, "{r}")?;
            out.flush()?;
        }
    }
    Ok(())
}
