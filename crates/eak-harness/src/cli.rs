//! The `eak` command line. Every subcommand is a harness tool call; `--json` prints the single
//! JSON envelope, otherwise a one-line summary followed by the pretty-printed envelope.
//!
//! Exit codes: 0 ok, 1 tool error, 2 usage, 3 validation found errors, 4 stale revision.

use crate::context::CallContext;
use crate::route;
use eak_runtime::OperationClass;
use serde_json::{json, Value};
use std::path::PathBuf;

pub const USAGE: &str = "eak — EAK engineering harness CLI

USAGE: eak [OPTIONS] <COMMAND> [ARGS]

OPTIONS (default from the EAK terminal session environment):
  --project <DIR>       project folder            [$EAK_PROJECT_DIR]
  --project-id <ID>     refuse any other project  [$EAK_PROJECT_ID]
  --session <ID>        session id for the journal [$EAK_SESSION_ID]
  --agent <NAME>        agent name for the journal [$EAK_AGENT_NAME]
  --allow-destructive   allow destructive commands [$EAK_HARNESS_ALLOW_DESTRUCTIVE=1]
  --json                print one JSON object

COMMANDS:
  tools                         list tools (category, schema)
  guide                         print the agent guide
  new <DIR> <NAME>              create a project
  info                          project id, revision, counts
  call <TOOL> [ARGS_JSON|-]     invoke any tool with a JSON object of arguments
  exec <REV> <COMMAND_JSON|->   execute one typed command ({\"type\":\"PlaceSymbol\",...}) at revision REV
  check <erc|drc|mech|dfm|all>  run verification; findings + evidence
  history [SINCE_REV]           journal, tool calls, evidence
  export [REV]                  compile the manufacturing package
  design schema                 requirement spec schema, metrics, evidence labels
  design spec [FILE|JSON|-]     set the spec (no argument: show it)
  design plan <FILE|JSON|->     record a plan of typed commands (nothing runs)
  design apply <PLAN_ID> <REV>  execute a plan as one batch at revision REV
  design validate               checks + spec evaluation (findings, labels)
  design claim <FILE|JSON|->    record a claim and its basis
  design log                    iteration record
  design report                 evidence report with labels and approval state
  design approve <REV> <FP>     human approval of a report (interactive terminal only)
  rework inspect [PROJECT_ID]   read-only inspection of an existing project
  rework baseline <PROJECT_ID> immutable baseline; opens a rework session
  rework audit                  classify issues (category, severity, affected objects)
  rework protect <FILE|JSON|-> {\"ids\":[..],\"reason\":\"..\"}: protect objects
  rework plan <FILE|JSON|->     repair plan of typed commands (nothing runs)
  rework apply <PLAN_ID> <REV>  execute a human-approved plan with readback
  rework report                 baseline diff, integrity, issues, labelled readiness
  rework approve <PLAN_ID> <FP> human approval of a plan (interactive terminal only)
  mcp                           serve MCP over stdio";

fn read_arg(s: &str) -> Result<Value, String> {
    let text = if s == "-" {
        let mut t = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut t).map_err(|e| e.to_string())?;
        t
    } else {
        s.to_string()
    };
    serde_json::from_str(&text).map_err(|e| format!("invalid JSON: {e}"))
}

/// A JSON argument: inline JSON, `-` for stdin, or a path to a JSON file.
fn read_json_or_file(s: &str) -> Result<Value, String> {
    let t = s.trim_start();
    if s == "-" || t.starts_with('{') || t.starts_with('[') {
        return read_arg(s);
    }
    let text = std::fs::read_to_string(s).map_err(|e| format!("{s}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{s}: invalid JSON: {e}"))
}

fn exit_code(v: &Value) -> i32 {
    if v["ok"] != json!(true) {
        return if v.pointer("/error/code") == Some(&json!("stale_revision")) {
            4
        } else {
            1
        };
    }
    let gated = v["category"] == json!("validation")
        || v["tool"] == json!("design_validate")
        || v["tool"] == json!("design_report");
    if gated
        && v.pointer("/result/errors")
            .or_else(|| v.pointer("/result/body/errors"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
    {
        return 3;
    }
    0
}

/// Run the CLI; returns the process exit code.
pub fn main_with(args: Vec<String>) -> i32 {
    let mut ctx = CallContext::from_env();
    let mut json_out = false;
    let mut rest = vec![];
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut val = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        let r: Result<(), String> = match a.as_str() {
            "--json" => {
                json_out = true;
                Ok(())
            }
            "--project" => val("--project").map(|v| {
                if ctx.project.dir.as_deref() != Some(std::path::Path::new(&v)) {
                    ctx.project.id = None;
                }
                ctx.project.dir = Some(PathBuf::from(v))
            }),
            "--project-id" => val("--project-id").map(|v| ctx.project.id = Some(v)),
            "--session" => val("--session").map(|v| ctx.session_id = v),
            "--agent" => val("--agent").map(|v| ctx.agent = v),
            "--allow-destructive" => {
                ctx.max_class = OperationClass::Destructive;
                Ok(())
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return 0;
            }
            _ => {
                rest.push(a);
                Ok(())
            }
        };
        if let Err(e) = r {
            eprintln!("eak: {e}\n\n{USAGE}");
            return 2;
        }
    }
    let usage = |m: &str| {
        eprintln!("eak: {m}\n\n{USAGE}");
        2
    };
    let Some(cmd) = rest.first().cloned() else {
        return usage("missing command");
    };
    let a = |i: usize| rest.get(i).cloned();
    let (tool, args): (String, Value) = match cmd.as_str() {
        "mcp" => {
            return match crate::mcp::serve(&ctx) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("eak mcp: {e}");
                    1
                }
            };
        }
        "guide" if !json_out => {
            println!("{}", crate::GUIDE);
            return 0;
        }
        "guide" => ("guide".into(), json!({})),
        "tools" => ("list_tools".into(), json!({})),
        "info" => ("project_info".into(), json!({})),
        "new" => match (a(1), a(2)) {
            (Some(d), Some(n)) => {
                let d = std::path::absolute(&d).unwrap_or_else(|_| PathBuf::from(&d));
                ("create_project".into(), json!({"dir": d, "name": n}))
            }
            _ => return usage("new <DIR> <NAME>"),
        },
        "check" => match a(1) {
            Some(c) => ("run_check".into(), json!({"check": c})),
            None => return usage("check <erc|drc|mech|dfm|all>"),
        },
        "history" => (
            "get_history".into(),
            match a(1).and_then(|s| s.parse::<u64>().ok()) {
                Some(r) => json!({"since_revision": r}),
                None => json!({}),
            },
        ),
        "export" => (
            "export_manufacturing".into(),
            match a(1).and_then(|s| s.parse::<u64>().ok()) {
                Some(r) => json!({"expected_revision": r}),
                None => json!({}),
            },
        ),
        "call" => {
            let Some(t) = a(1) else {
                return usage("call <TOOL> [ARGS_JSON|-]");
            };
            let args = match a(2) {
                Some(s) => match read_arg(&s) {
                    Ok(v) => v,
                    Err(e) => return usage(&e),
                },
                None => json!({}),
            };
            (t, args)
        }
        "exec" => {
            let (Some(rev), Some(c)) = (a(1).and_then(|s| s.parse::<u64>().ok()), a(2)) else {
                return usage("exec <REV> <COMMAND_JSON|->");
            };
            let mut v = match read_arg(&c) {
                Ok(v) => v,
                Err(e) => return usage(&e),
            };
            let Some(t) = v
                .as_object_mut()
                .and_then(|m| m.remove("type"))
                .and_then(|t| t.as_str().map(String::from))
            else {
                return usage("command JSON needs a \"type\" naming the command");
            };
            v["expected_revision"] = json!(rev);
            (t, v)
        }
        "design" => {
            let json_arg = |i: usize| -> Result<Value, String> {
                a(i).ok_or_else(|| "missing JSON argument".to_string())
                    .and_then(|s| read_json_or_file(&s))
            };
            match a(1).as_deref() {
                Some("schema") => ("design_spec_schema".into(), json!({})),
                Some("spec") if a(2).is_none() => ("design_spec_get".into(), json!({})),
                Some("spec") => match json_arg(2) {
                    Ok(s) => ("design_spec_set".into(), json!({"spec": s})),
                    Err(e) => return usage(&e),
                },
                Some(sub @ ("plan" | "claim")) => match json_arg(2) {
                    Ok(v) => (format!("design_{sub}"), v),
                    Err(e) => return usage(&e),
                },
                Some("apply") => match (a(2), a(3).and_then(|s| s.parse::<u64>().ok())) {
                    (Some(id), Some(rev)) => (
                        "design_apply".into(),
                        json!({"plan_id": id, "expected_revision": rev}),
                    ),
                    _ => return usage("design apply <PLAN_ID> <REV>"),
                },
                Some("validate") => ("design_validate".into(), json!({})),
                Some("log") => ("design_iteration_log".into(), json!({})),
                Some("report") => ("design_report".into(), json!({})),
                Some("approve") => {
                    return match (a(2).and_then(|s| s.parse::<u64>().ok()), a(3)) {
                        (Some(rev), Some(fp)) => crate::design::approve_cli(&ctx, rev, &fp),
                        _ => usage("design approve <REV> <FINGERPRINT>"),
                    };
                }
                _ => {
                    return usage(
                        "design <schema|spec|plan|apply|validate|claim|log|report|approve>",
                    )
                }
            }
        }
        "rework" => {
            let json_arg = |i: usize| -> Result<Value, String> {
                a(i).ok_or_else(|| "missing JSON argument".to_string())
                    .and_then(|s| read_json_or_file(&s))
            };
            match a(1).as_deref() {
                Some("inspect") => (
                    "rework_inspect".into(),
                    a(2).map_or(json!({}), |id| json!({"expect_project_id": id})),
                ),
                Some("baseline") => match a(2) {
                    Some(id) => ("rework_baseline".into(), json!({"confirm_project_id": id})),
                    None => return usage("rework baseline <PROJECT_ID>"),
                },
                Some("audit") => ("rework_audit".into(), json!({})),
                Some(sub @ ("protect" | "plan")) => match json_arg(2) {
                    Ok(v) => (format!("rework_{sub}"), v),
                    Err(e) => return usage(&e),
                },
                Some("apply") => match (a(2), a(3).and_then(|s| s.parse::<u64>().ok())) {
                    (Some(id), Some(rev)) => (
                        "rework_apply".into(),
                        json!({"plan_id": id, "expected_revision": rev}),
                    ),
                    _ => return usage("rework apply <PLAN_ID> <REV>"),
                },
                Some("report") => ("rework_report".into(), json!({})),
                Some("approve") => {
                    return match (a(2), a(3)) {
                        (Some(plan), Some(fp)) => crate::rework::approve_cli(&ctx, &plan, &fp),
                        _ => usage("rework approve <PLAN_ID> <FINGERPRINT>"),
                    };
                }
                _ => return usage("rework <inspect|baseline|audit|protect|plan|apply|report|approve>"),
            }
        }
        other => return usage(&format!("unknown command {other}")),
    };
    let env = route::invoke(&ctx, &tool, &args);
    if json_out {
        println!("{env}");
    } else {
        let status = if env["ok"] == json!(true) {
            "ok".to_string()
        } else {
            format!(
                "error {}",
                env.pointer("/error/code")
                    .and_then(Value::as_str)
                    .unwrap_or("?")
            )
        };
        println!(
            "{status}: {tool} project={} revision={} route={}",
            env.pointer("/project/id")
                .and_then(Value::as_str)
                .unwrap_or("-"),
            env.pointer("/project/revision")
                .map(|v| v.to_string())
                .unwrap_or("-".into()),
            env["route"].as_str().unwrap_or("-"),
        );
        println!("{}", serde_json::to_string_pretty(&env).unwrap_or_default());
    }
    exit_code(&env)
}
