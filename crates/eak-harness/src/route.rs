//! Route a tool call to the right engine: the running app's engine (through its endpoint) when
//! the app has the project open, otherwise a direct engine opened under the project's exclusive
//! lock for the duration of the call. Two engines never write the same project.

use crate::context::CallContext;
use crate::endpoint::{self, ClientError};
use crate::tools::{self, envelope_error, Category, ErrorCode};
use eak_runtime::Engine;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// How long a direct call waits for another harness process holding the project lock.
const LOCK_WAIT: Duration = Duration::from_secs(5);

/// Invoke one tool with an explicit context. Always returns the JSON envelope.
pub fn invoke(base: &CallContext, tool: &str, args: &Value) -> Value {
    let ctx = base.with_args(args);
    let Some(spec) = tools::find_spec(tool) else {
        return envelope_error(
            tool,
            &ctx,
            ErrorCode::UnknownTool,
            &format!("no tool named {tool}; call list_tools"),
            Value::Null,
        );
    };
    match spec.category {
        Category::Discovery => {
            let result = match tool {
                "guide" => json!({"markdown": crate::GUIDE}),
                "design_spec_schema" => crate::design::spec_schema(),
                "design_procedure" => crate::design::procedure(),
                _ => tools::list_tools_json(),
            };
            return json!({"ok": true, "tool": tool, "category": spec.category, "result": result});
        }
        Category::Project => return create_project(&ctx, args),
        _ => {}
    }
    let Some(dir) = ctx.project.dir.clone() else {
        return envelope_error(tool, &ctx, ErrorCode::NoProject, "no project: pass project_dir (or --project), or run inside an EAK terminal session that exports EAK_PROJECT_DIR", Value::Null);
    };
    if !dir.is_absolute() {
        return envelope_error(
            tool,
            &ctx,
            ErrorCode::InvalidArguments,
            "project_dir must be an absolute path",
            Value::Null,
        );
    }
    if !dir.join(eak_runtime::store::PROJECT_FILE).exists() {
        return envelope_error(
            tool,
            &ctx,
            ErrorCode::ProjectNotFound,
            &format!("{} holds no EAK project", dir.display()),
            Value::Null,
        );
    }
    // 1. The app has this project open: go through its engine.
    if let Some(rec) = endpoint::read_record(&dir) {
        match endpoint::call(&rec.endpoint, &rec.token_file, tool, args, &ctx) {
            Ok(v) => return v,
            Err(ClientError::Unreachable(_)) => {} // stale record: the app is gone, fall through
            Err(e) => {
                return envelope_error(
                    tool,
                    &ctx,
                    ErrorCode::AppUnreachable,
                    &format!(
                        "the EAK app has this project open but its harness endpoint failed: {e:?}"
                    ),
                    json!({"endpoint": rec.endpoint}),
                );
            }
        }
    }
    // 2. Direct engine under the project lock.
    let start = Instant::now();
    loop {
        match Engine::open(&dir, false) {
            Ok(mut engine) => return tools::handle(&mut engine, &ctx, "direct", tool, args),
            Err(e) if e.contains("locked") => {
                if start.elapsed() > LOCK_WAIT {
                    return envelope_error(
                        tool,
                        &ctx,
                        ErrorCode::ProjectLocked,
                        &e,
                        json!({"hint": "another process holds the project lock and serves no harness endpoint; if the EAK app is open on it, update the app; if it crashed, reopen the project in the app (recovery) to take over the lock"}),
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return envelope_error(tool, &ctx, ErrorCode::Storage, &e, Value::Null),
        }
    }
}

fn create_project(ctx: &CallContext, args: &Value) -> Value {
    let (Some(dir), Some(name)) = (
        args.get("dir").and_then(Value::as_str),
        args.get("name").and_then(Value::as_str),
    ) else {
        return envelope_error(
            "create_project",
            ctx,
            ErrorCode::InvalidArguments,
            "dir and name are required",
            Value::Null,
        );
    };
    let dir = std::path::PathBuf::from(dir);
    if !dir.is_absolute() || name.trim().is_empty() {
        return envelope_error(
            "create_project",
            ctx,
            ErrorCode::InvalidArguments,
            "dir must be absolute and name non-empty",
            Value::Null,
        );
    }
    match Engine::create(&dir, name.trim()) {
        Ok(engine) => {
            let mut v = json!({"ok": true, "tool": "create_project", "category": "project", "route": "direct",
                "session": {"id": ctx.session_id, "agent": ctx.agent}});
            let p = engine.project();
            v["project"] =
                json!({"id": p.id, "name": p.name, "dir": engine.dir(), "revision": p.revision});
            v["result"] = v["project"].clone();
            v
        }
        Err(e) => {
            let code = if e.contains("already contains") {
                ErrorCode::ProjectExists
            } else {
                ErrorCode::Storage
            };
            envelope_error("create_project", ctx, code, &e, Value::Null)
        }
    }
}
