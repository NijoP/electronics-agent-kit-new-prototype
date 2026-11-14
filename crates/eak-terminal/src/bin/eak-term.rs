//! `eak-term`: command-line client for the EAK terminal control API.
//!
//! Inside an EAK terminal the API file is found through `EAK_TERMINAL_API_FILE`; elsewhere
//! pass `--api-file` or rely on the default location. Output is JSON on stdout; errors go
//! to stderr with exit code 1 (exit code 2 when `wait` times out). See docs/terminal-api.md.

use eak_terminal::api::{default_api_file, request};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: eak-term [--api-file PATH] <command> [args]

commands:
  methods                                   list API methods
  shells                                    list installed shells
  list                                      tabs, panes and every session with status
  new [--cwd DIR] [--name N] [--shell S] [-- PROGRAM ARGS...]
  split ID [--down] [--cwd DIR] [--name N] [-- PROGRAM ARGS...]
  get ID                                    one session with status
  send ID TEXT                              type TEXT exactly (use \\r for Enter)
  run ID COMMAND...                         type COMMAND + Enter; prints its output cursor
  read ID [--lines N]                       plain-text output
  wait ID [--until exit|idle|output] [--pattern P] [--since CURSOR] [--timeout SECONDS]
  focus ID | rename ID NAME | restart ID | close ID
  report ID --source S --state idle|working|blocked [--agent A] [--message M]
  release ID --source S";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok((v, code)) => {
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            ExitCode::from(code)
        }
        Err(e) => {
            eprintln!("eak-term: {e}");
            ExitCode::from(1)
        }
    }
}

struct Args {
    rest: Vec<String>,
    tail: Vec<String>,
}

impl Args {
    fn flag(&mut self, name: &str) -> bool {
        let before = self.rest.len();
        self.rest.retain(|a| a != name);
        self.rest.len() != before
    }
    fn value(&mut self, name: &str) -> Result<Option<String>, String> {
        let Some(i) = self.rest.iter().position(|a| a == name) else {
            return Ok(None);
        };
        if i + 1 >= self.rest.len() {
            return Err(format!("{name} needs a value"));
        }
        let v = self.rest.remove(i + 1);
        self.rest.remove(i);
        Ok(Some(v))
    }
    fn pos(&mut self, what: &str) -> Result<String, String> {
        if self.rest.is_empty() {
            return Err(format!("missing {what}\n{USAGE}"));
        }
        Ok(self.rest.remove(0))
    }
    fn id(&mut self) -> Result<u64, String> {
        let s = self.pos("session ID")?;
        s.parse()
            .map_err(|_| format!("session ID must be a number, got `{s}`"))
    }
}

fn run(argv: Vec<String>) -> Result<(Value, u8), String> {
    let (rest, tail) = match argv.iter().position(|a| a == "--") {
        Some(i) => (argv[..i].to_vec(), argv[i + 1..].to_vec()),
        None => (argv, Vec::new()),
    };
    let mut a = Args { rest, tail };
    let api_file = match a.value("--api-file")? {
        Some(p) => PathBuf::from(p),
        None => std::env::var_os("EAK_TERMINAL_API_FILE")
            .map(PathBuf::from)
            .or_else(default_api_file)
            .ok_or("cannot find the EAK API file; pass --api-file")?,
    };
    if a.rest.is_empty() || a.flag("--help") || a.flag("-h") {
        return Err(USAGE.into());
    }
    let cmd = a.pos("command")?;
    let call = |method: &str, params: Value| request(&api_file, method, params).map(|v| (v, 0));
    let create = |a: &mut Args, mut p: Value| -> Result<Value, String> {
        for (flag, key) in [("--cwd", "cwd"), ("--name", "name"), ("--shell", "shell")] {
            if let Some(v) = a.value(flag)? {
                p[key] = json!(v);
            }
        }
        if !a.tail.is_empty() {
            p["command"] = json!(a.tail);
        }
        Ok(p)
    };
    match cmd.as_str() {
        "methods" => call("methods", json!({})),
        "shells" => call("shells.list", json!({})),
        "list" => call("session.list", json!({})),
        "new" => {
            let p = create(&mut a, json!({}))?;
            call("session.create", p)
        }
        "split" => {
            let id = a.id()?;
            let dir = if a.flag("--down") { "column" } else { "row" };
            let p = create(&mut a, json!({"id": id, "direction": dir}))?;
            call("session.split", p)
        }
        "get" => call("session.get", json!({"id": a.id()?})),
        "send" => {
            let id = a.id()?;
            let text = a.pos("TEXT")?.replace("\\r", "\r").replace("\\n", "\n");
            call("session.send_input", json!({"id": id, "text": text}))
        }
        "run" => {
            let id = a.id()?;
            let command = a.rest.join(" ");
            if command.is_empty() {
                return Err("missing COMMAND".into());
            }
            call("session.run", json!({"id": id, "command": command}))
        }
        "read" => {
            let id = a.id()?;
            let lines = a
                .value("--lines")?
                .map(|n| n.parse::<usize>())
                .transpose()
                .map_err(|_| "--lines needs a number")?;
            let v = request(&api_file, "session.read", json!({"id": id, "lines": lines}))?;
            // Plain text is more useful than JSON for reading output.
            print!("{}", v["text"].as_str().unwrap_or_default());
            std::process::exit(0);
        }
        "wait" => {
            let id = a.id()?;
            let until = a.value("--until")?.unwrap_or_else(|| "exit".into());
            let pattern = a.value("--pattern")?;
            let since = a
                .value("--since")?
                .map(|s| s.parse::<u64>())
                .transpose()
                .map_err(|_| "--since needs a number")?;
            let secs: f64 = a
                .value("--timeout")?
                .map(|s| s.parse())
                .transpose()
                .map_err(|_| "--timeout needs seconds")?
                .unwrap_or(30.0);
            let mut p = json!({"id": id, "until": until, "timeout_ms": (secs * 1000.0) as u64});
            if let Some(s) = since {
                p["since"] = json!(s);
            }
            if let Some(pat) = pattern {
                p["pattern"] = json!(pat);
            }
            let v = request(&api_file, "session.wait", p)?;
            let code = if v["met"] == Value::Bool(true) { 0 } else { 2 };
            Ok((v, code))
        }
        "focus" => call("session.focus", json!({"id": a.id()?})),
        "rename" => {
            let id = a.id()?;
            call("session.rename", json!({"id": id, "name": a.pos("NAME")?}))
        }
        "restart" => call("session.restart", json!({"id": a.id()?})),
        "close" => call("session.close", json!({"id": a.id()?})),
        "report" => {
            let id = a.id()?;
            let mut p = json!({"id": id});
            for (flag, key) in [
                ("--source", "source"),
                ("--state", "state"),
                ("--agent", "agent"),
                ("--message", "message"),
            ] {
                if let Some(v) = a.value(flag)? {
                    p[key] = json!(v);
                }
            }
            call("session.report_state", p)
        }
        "release" => {
            let id = a.id()?;
            let source = a.value("--source")?.ok_or("--source is required")?;
            call("session.release", json!({"id": id, "source": source}))
        }
        other => Err(format!("unknown command `{other}`\n{USAGE}")),
    }
}
