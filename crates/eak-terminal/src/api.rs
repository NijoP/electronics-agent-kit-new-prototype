//! Local control API: newline-delimited JSON over TCP on `127.0.0.1`, authenticated by a
//! random token kept in a user-only file. The protocol is documented in
//! `docs/terminal-api.md`; the `eak-term` binary is a thin client over [`request`].
//!
//! Request:  `{"token": "...", "method": "session.run", "params": {"id": 1, "command": "ls"}}`
//! Response: `{"ok": true, "result": ...}` or `{"ok": false, "error": "..."}`

use crate::{CreateOptions, Direction, Manager, Report, ReportedState, SessionId, WaitFor};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Protocol version written to the API file.
pub const PROTOCOL: u32 = 1;
/// Largest accepted request line.
const MAX_LINE: u64 = 1024 * 1024;
/// Longest `session.wait` a client may ask for.
const MAX_WAIT_MS: u64 = 10 * 60 * 1000;

/// Contents of the API file: where to connect and the token to send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiInfo {
    pub protocol: u32,
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

/// Default API file location, matching the desktop app's data directory
/// (`dev.eak.desktop`).
pub fn default_api_file() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.map(|b| b.join("dev.eak.desktop").join("terminal-api.json"))
}

fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Constant-time comparison so response timing does not leak the token.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(path);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    // On Windows the file lives in the user's own profile directory, whose ACL already
    // excludes other users.
    let mut f = opts.open(path)?;
    f.write_all(bytes)
}

/// Start the API server on an ephemeral loopback port and write `api_file`.
pub fn serve(manager: Arc<Manager>, api_file: &Path) -> std::io::Result<ApiInfo> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let info = ApiInfo {
        protocol: PROTOCOL,
        port: listener.local_addr()?.port(),
        token: new_token(),
        pid: std::process::id(),
    };
    write_private(api_file, &serde_json::to_vec_pretty(&info)?)?;
    let token = info.token.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let m = Arc::clone(&manager);
            let t = token.clone();
            std::thread::spawn(move || {
                let _ = handle(stream, &m, &t);
            });
        }
    });
    Ok(info)
}

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    token: String,
    method: String,
    #[serde(default)]
    params: Value,
}

fn handle(stream: TcpStream, m: &Arc<Manager>, token: &str) -> std::io::Result<()> {
    let mut out = stream.try_clone()?;
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        let n = (&mut reader).take(MAX_LINE).read_line(&mut line)?;
        if n == 0 {
            return Ok(());
        }
        if line.trim().is_empty() {
            continue;
        }
        let too_long = n as u64 >= MAX_LINE && !line.ends_with('\n');
        let reply = if too_long {
            json!({"ok": false, "error": "request too large"})
        } else {
            match serde_json::from_str::<Request>(&line) {
                Err(e) => json!({"ok": false, "error": format!("bad request: {e}")}),
                Ok(r) if !same(&r.token, token) => {
                    json!({"ok": false, "error": "unauthorized: wrong or missing token"})
                }
                Ok(r) => match dispatch(m, &r.method, r.params) {
                    Ok(v) => json!({"ok": true, "result": v}),
                    Err(e) => json!({"ok": false, "error": e}),
                },
            }
        };
        let mut bytes = serde_json::to_vec(&reply)?;
        bytes.push(b'\n');
        out.write_all(&bytes)?;
        if too_long {
            return Ok(());
        }
    }
}

fn arg<T: for<'de> Deserialize<'de>>(params: &Value, key: &str) -> Result<T, String> {
    let v = params.get(key).cloned().unwrap_or(Value::Null);
    serde_json::from_value(v).map_err(|e| format!("parameter `{key}`: {e}"))
}

fn opt<T: for<'de> Deserialize<'de>>(params: &Value, key: &str) -> Result<Option<T>, String> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| format!("parameter `{key}`: {e}")),
    }
}

fn to_value<T: Serialize>(v: T) -> Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// Every method the API understands (also returned by the `methods` call).
pub const METHODS: &[&str] = &[
    "methods",
    "shells.list",
    "session.list",
    "space.create",
    "space.rename",
    "space.close",
    "tab.move",
    "session.create",
    "session.split",
    "session.get",
    "session.send_input",
    "session.run",
    "session.read",
    "session.focus",
    "session.rename",
    "session.restart",
    "session.close",
    "session.wait",
    "session.report_state",
    "session.release",
];

/// Execute one API method against the manager.
pub fn dispatch(m: &Arc<Manager>, method: &str, params: Value) -> Result<Value, String> {
    let id = || arg::<SessionId>(&params, "id");
    let create_opts =
        || serde_json::from_value::<CreateOptions>(params.clone()).map_err(|e| e.to_string());
    match method {
        "methods" => to_value(METHODS),
        "shells.list" => to_value(crate::available_shells()),
        "session.list" => to_value(m.overview()),
        "space.create" => {
            let root: PathBuf = arg(&params, "root")?;
            let name: Option<String> = opt(&params, "name")?;
            to_value(m.create_space(name.as_deref().unwrap_or(""), root)?)
        }
        "space.rename" => {
            let name: String = arg(&params, "name")?;
            m.rename_space(id()?, &name).map(|_| Value::Null)
        }
        "space.close" => m.close_space(id()?).map(|_| Value::Null),
        "tab.move" => {
            let index: usize = arg(&params, "index")?;
            m.move_tab(id()?, index).map(|_| Value::Null)
        }
        "session.create" => to_value(m.create(create_opts()?)?),
        "session.split" => {
            let mut opts = create_opts()?;
            opts.split_of = Some(id()?);
            opts.direction = Some(opt::<Direction>(&params, "direction")?.unwrap_or_default());
            to_value(m.create(opts)?)
        }
        "session.get" => to_value(m.info(id()?)?),
        "session.send_input" => {
            let text: String = arg(&params, "text")?;
            m.write(id()?, text.as_bytes()).map(|_| Value::Null)
        }
        "session.run" => {
            let command: String = arg(&params, "command")?;
            let since = m.run(id()?, &command)?;
            Ok(json!({ "since": since }))
        }
        "session.read" => {
            let lines: Option<usize> = opt(&params, "lines")?;
            Ok(json!({ "text": m.read(id()?, lines)? }))
        }
        "session.focus" => m.focus(id()?).map(|_| Value::Null),
        "session.rename" => {
            let name: String = arg(&params, "name")?;
            m.rename(id()?, &name).map(|_| Value::Null)
        }
        "session.restart" => to_value(m.restart(id()?)?),
        "session.close" => m.close(id()?).map(|_| Value::Null),
        "session.wait" => {
            let what: WaitFor =
                serde_json::from_value(params.clone()).map_err(|e| format!("wait: {e}"))?;
            let ms = opt::<u64>(&params, "timeout_ms")?
                .unwrap_or(30_000)
                .min(MAX_WAIT_MS);
            let since: Option<u64> = opt(&params, "since")?;
            let (status, met) = m.wait(id()?, &what, since, Duration::from_millis(ms))?;
            Ok(json!({ "met": met, "status": status }))
        }
        "session.report_state" => {
            let report = Report {
                source: arg(&params, "source")?,
                state: arg::<ReportedState>(&params, "state")?,
                message: opt(&params, "message")?,
                agent: opt(&params, "agent")?,
            };
            if report.source.trim().is_empty() {
                return Err("parameter `source` must not be empty".into());
            }
            m.report(id()?, report).map(|_| Value::Null)
        }
        "session.release" => {
            let source: String = arg(&params, "source")?;
            m.release(id()?, &source).map(|_| Value::Null)
        }
        other => Err(format!(
            "unknown method `{other}`; call `methods` for the list"
        )),
    }
}

/// Client: send one request using the API file and return its result.
pub fn request(api_file: &Path, method: &str, params: Value) -> Result<Value, String> {
    let text = std::fs::read_to_string(api_file)
        .map_err(|e| format!("cannot read {}: {e} (is EAK running?)", api_file.display()))?;
    let info: ApiInfo = serde_json::from_str(&text).map_err(|e| format!("bad API file: {e}"))?;
    let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, info.port)).map_err(|e| {
        format!(
            "cannot connect to EAK on port {}: {e} (is EAK running?)",
            info.port
        )
    })?;
    let mut w = stream.try_clone().map_err(|e| e.to_string())?;
    let req = json!({"token": info.token, "method": method, "params": params});
    let mut bytes = serde_json::to_vec(&req).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    w.write_all(&bytes).map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(stream)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let reply: Value = serde_json::from_str(&line).map_err(|e| format!("bad reply: {e}"))?;
    if reply["ok"] == Value::Bool(true) {
        Ok(reply["result"].clone())
    } else {
        Err(reply["error"]
            .as_str()
            .unwrap_or("request failed")
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_comparison_needs_exact_match() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "abcd"));
        assert!(!same("", "abc"));
    }

    #[test]
    fn tokens_are_long_and_unique() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }
}
