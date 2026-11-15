//! The desktop app's harness endpoint: a local, token-authenticated socket served by the app
//! backend while it has a project open, so external agents act through the app's own engine
//! (one writer per project; the editor sees each change through its engine events).
//!
//! * Unix (Linux, macOS): a Unix domain socket inside a directory created with mode 0700, so only
//!   the user can connect; the token file is mode 0600.
//! * Windows: TCP on 127.0.0.1 with an ephemeral port; the token file lives in the user's
//!   app-data folder (user-only ACL by default). Any local process could connect, so the token is
//!   the authentication.
//!
//! Wire protocol: one JSON request line → one JSON response line per connection.
//! Request `{"token", "tool", "args", "context"}`; the response is the tool envelope.
//!
//! While the app serves a project it writes `<project>/.harness.json` (endpoint, token file path,
//! pid, project id — never the token itself) so a CLI invoked on that folder finds the app.

use crate::context::CallContext;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Endpoint record file inside a project folder.
pub const RECORD_FILE: &str = ".harness.json";
const MAX_REQUEST: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub token: String,
    pub tool: String,
    #[serde(default)]
    pub args: Value,
    pub context: CallContext,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// `unix:<path>` or `tcp:<host:port>`.
    pub endpoint: String,
    pub token_file: PathBuf,
    pub pid: u32,
    pub project_id: String,
}

pub type Handler = Arc<dyn Fn(&Request) -> Value + Send + Sync>;

/// A running endpoint. Dropping it stops accepting and removes the socket and token files.
pub struct Server {
    pub endpoint: String,
    pub token_file: PathBuf,
    stop: Arc<AtomicBool>,
    socket_path: Option<PathBuf>,
}

fn new_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Create `dir` readable only by the user (0700 on Unix).
fn private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_private(path: &Path, data: &str) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)?.write_all(data.as_bytes())
}

fn serve_conn<S: Read + Write>(stream: S, token: &str, handler: &Handler) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let read = (&mut reader).take(MAX_REQUEST).read_line(&mut line);
    let resp = match read {
        Err(e) => {
            json!({"ok": false, "error": {"code": "invalid_arguments", "message": format!("request not readable: {e}")}})
        }
        Ok(_) => match serde_json::from_str::<Request>(&line) {
            Err(e) => {
                json!({"ok": false, "error": {"code": "invalid_arguments", "message": format!("malformed request: {e}")}})
            }
            Ok(req) if !constant_time_eq(req.token.as_bytes(), token.as_bytes()) => {
                json!({"ok": false, "tool": req.tool, "error": {"code": "auth_failed", "message": "harness token rejected"}})
            }
            Ok(req) => handler(&req),
        },
    };
    let mut stream = reader.into_inner();
    let _ = writeln!(stream, "{resp}");
    let _ = stream.flush();
}

impl Server {
    /// Start serving in `state_dir` (created private). The handler runs on a worker thread per
    /// connection and must do its own engine locking.
    pub fn start(state_dir: &Path, handler: Handler) -> std::io::Result<Server> {
        private_dir(state_dir)?;
        let token = new_token();
        let token_file = state_dir.join(format!("token-{}", std::process::id()));
        write_private(&token_file, &token)?;
        let stop = Arc::new(AtomicBool::new(false));
        #[cfg(unix)]
        {
            let path = state_dir.join(format!("eak-{}.sock", std::process::id()));
            let _ = std::fs::remove_file(&path);
            let listener = std::os::unix::net::UnixListener::bind(&path)?;
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            }
            let st = stop.clone();
            std::thread::Builder::new()
                .name("eak-harness-endpoint".into())
                .spawn(move || {
                    for conn in listener.incoming() {
                        if st.load(Ordering::SeqCst) {
                            break;
                        }
                        let Ok(conn) = conn else { continue };
                        let (h, t) = (handler.clone(), token.clone());
                        let _ = conn.set_read_timeout(Some(Duration::from_secs(30)));
                        std::thread::spawn(move || serve_conn(conn, &t, &h));
                    }
                })?;
            Ok(Server {
                endpoint: format!("unix:{}", path.display()),
                token_file,
                stop,
                socket_path: Some(path),
            })
        }
        #[cfg(not(unix))]
        {
            Server::start_tcp(token, token_file, stop, handler)
        }
    }

    /// TCP loopback endpoint (the Windows transport; also usable elsewhere for tests).
    pub fn start_tcp_in(state_dir: &Path, handler: Handler) -> std::io::Result<Server> {
        private_dir(state_dir)?;
        let token = new_token();
        let token_file = state_dir.join(format!("token-tcp-{}", std::process::id()));
        write_private(&token_file, &token)?;
        Server::start_tcp(token, token_file, Arc::new(AtomicBool::new(false)), handler)
    }

    fn start_tcp(
        token: String,
        token_file: PathBuf,
        stop: Arc<AtomicBool>,
        handler: Handler,
    ) -> std::io::Result<Server> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let st = stop.clone();
        std::thread::Builder::new()
            .name("eak-harness-endpoint".into())
            .spawn(move || {
                for conn in listener.incoming() {
                    if st.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(conn) = conn else { continue };
                    let (h, t) = (handler.clone(), token.clone());
                    let _ = conn.set_read_timeout(Some(Duration::from_secs(30)));
                    std::thread::spawn(move || serve_conn(conn, &t, &h));
                }
            })?;
        Ok(Server {
            endpoint: format!("tcp:{addr}"),
            token_file,
            stop,
            socket_path: None,
        })
    }

    /// Advertise this endpoint in a project folder (no secret is written there).
    pub fn write_record(&self, project_dir: &Path, project_id: &str) -> std::io::Result<()> {
        let rec = Record {
            endpoint: self.endpoint.clone(),
            token_file: self.token_file.clone(),
            pid: std::process::id(),
            project_id: project_id.into(),
        };
        eak_runtime::store::atomic_write(
            &project_dir.join(RECORD_FILE),
            serde_json::to_string_pretty(&rec)
                .unwrap_or_default()
                .as_bytes(),
        )
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop so the thread exits.
        let _ = connect(&self.endpoint, Duration::from_millis(200));
        if let Some(p) = &self.socket_path {
            let _ = std::fs::remove_file(p);
        }
        let _ = std::fs::remove_file(&self.token_file);
    }
}

/// Remove a project's endpoint record if it belongs to this process.
pub fn remove_record(project_dir: &Path) {
    if read_record(project_dir).is_some_and(|r| r.pid == std::process::id()) {
        let _ = std::fs::remove_file(project_dir.join(RECORD_FILE));
    }
}

pub fn read_record(project_dir: &Path) -> Option<Record> {
    let s = std::fs::read_to_string(project_dir.join(RECORD_FILE)).ok()?;
    serde_json::from_str(&s).ok()
}

trait Stream: Read + Write {}
impl<T: Read + Write> Stream for T {}

fn connect(endpoint: &str, timeout: Duration) -> std::io::Result<Box<dyn Stream>> {
    if let Some(addr) = endpoint.strip_prefix("tcp:") {
        let addr: std::net::SocketAddr = addr
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{e}")))?;
        if !addr.ip().is_loopback() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "harness endpoints are loopback-only",
            ));
        }
        let s = std::net::TcpStream::connect_timeout(&addr, timeout)?;
        s.set_read_timeout(Some(Duration::from_secs(600)))?;
        return Ok(Box::new(s));
    }
    #[cfg(unix)]
    {
        let path = endpoint.strip_prefix("unix:").unwrap_or(endpoint);
        let s = std::os::unix::net::UnixStream::connect(path)?;
        s.set_read_timeout(Some(Duration::from_secs(600)))?;
        Ok(Box::new(s))
    }
    #[cfg(not(unix))]
    {
        let _ = timeout;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!("unsupported endpoint {endpoint}"),
        ))
    }
}

#[derive(Debug)]
pub enum ClientError {
    /// Nothing is listening (app closed or crashed).
    Unreachable(String),
    Token(String),
    Protocol(String),
}

/// Send one tool request to an app endpoint.
pub fn call(
    endpoint: &str,
    token_file: &Path,
    tool: &str,
    args: &Value,
    context: &CallContext,
) -> Result<Value, ClientError> {
    // Connect before reading the token: an app that exited cleanly has already removed its
    // token file, and that must surface as Unreachable (stale record, fall through), not as a
    // token error. On Windows the TCP endpoint has no socket file to vanish first.
    let mut s = connect(endpoint, Duration::from_secs(2))
        .map_err(|e| ClientError::Unreachable(e.to_string()))?;
    let token = std::fs::read_to_string(token_file)
        .map_err(|e| ClientError::Token(format!("{}: {e}", token_file.display())))?;
    let req = Request {
        token: token.trim().into(),
        tool: tool.into(),
        args: args.clone(),
        context: context.clone(),
    };
    let line = serde_json::to_string(&req).map_err(|e| ClientError::Protocol(e.to_string()))?;
    writeln!(s, "{line}").map_err(|e| ClientError::Unreachable(e.to_string()))?;
    s.flush()
        .map_err(|e| ClientError::Unreachable(e.to_string()))?;
    let mut resp = String::new();
    BufReader::new(s)
        .read_line(&mut resp)
        .map_err(|e| ClientError::Protocol(e.to_string()))?;
    serde_json::from_str(&resp).map_err(|e| ClientError::Protocol(format!("bad response: {e}")))
}
