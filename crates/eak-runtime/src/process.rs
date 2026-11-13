//! Minimal project-scoped process supervisor (concepts from Herdr; EAK-native code).
//!
//! Each process belongs to one project. It has a wall-clock timeout, bounded stdout/stderr
//! capture, cancellation that terminates the whole process tree, and an explicit final
//! state. Cancelling or crashing a process in one project never touches another project.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state", content = "detail")]
pub enum ProcState {
    Running,
    Exited(i32),
    Cancelled,
    TimedOut,
    FailedToStart(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcInfo {
    pub id: u64,
    pub project_id: String,
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub started_ms: u128,
    pub state: ProcState,
    pub stdout_tail: String,
    pub stderr_tail: String,
    pub truncated: bool,
}

const MAX_CAPTURE: usize = 256 * 1024;

/// After a process leaves Running, how long `wait` lets the capture threads drain the
/// pipes. Bounded because a detached grandchild may keep a pipe open indefinitely.
const DRAIN_GRACE: Duration = Duration::from_secs(2);

#[derive(Default)]
struct Bounded {
    buf: VecDeque<u8>,
    truncated: bool,
    /// The reader reached end-of-stream (every writer closed the pipe).
    eof: bool,
}

impl Bounded {
    fn push(&mut self, data: &[u8]) {
        self.buf.extend(data);
        while self.buf.len() > MAX_CAPTURE {
            self.buf.pop_front();
            self.truncated = true;
        }
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.buf.iter().copied().collect::<Vec<u8>>()).into_owned()
    }
}

struct Proc {
    info: ProcInfo,
    child: Option<Child>,
    out: Arc<Mutex<Bounded>>,
    err: Arc<Mutex<Bounded>>,
    deadline: Instant,
}

#[derive(Default)]
pub struct Supervisor {
    procs: BTreeMap<u64, Proc>,
    next: u64,
}

fn reader<R: Read + Send + 'static>(mut r: R, sink: Arc<Mutex<Bounded>>) {
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match r.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink.lock().expect("capture lock").push(&chunk[..n]),
            }
        }
        sink.lock().expect("capture lock").eof = true;
    });
}

/// Terminate a process and all of its children. Must only be called before the child
/// has been reaped, so its pid (and process-group id) cannot have been reused.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        // `/T` walks the parent-pid tree; `/F` forces termination (no graceful close).
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        // The child was started as the leader of its own process group (`process_group(0)`),
        // so its pid is the group id. SIGKILL the whole group — the Unix equivalent of
        // `taskkill /T /F` — directly via killpg(2) rather than relying on a `kill` binary
        // whose negative-pid argument parsing differs between platforms.
        if let Ok(pgid) = libc::pid_t::try_from(child.id()) {
            // SAFETY: plain syscall with a pgid we own; failure (ESRCH: group already
            // gone) is harmless and handled by the `child.kill()` fallback below.
            unsafe {
                libc::killpg(pgid, libc::SIGKILL);
            }
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

impl Supervisor {
    pub fn start(
        &mut self,
        project_id: &str,
        name: &str,
        program: &str,
        args: &[String],
        cwd: &str,
        timeout: Duration,
    ) -> u64 {
        self.next += 1;
        let id = self.next;
        let mut info = ProcInfo {
            id,
            project_id: project_id.into(),
            name: name.into(),
            program: program.into(),
            args: args.to_vec(),
            cwd: cwd.into(),
            started_ms: crate::store::now_ms(),
            state: ProcState::Running,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            truncated: false,
        };
        let out = Arc::new(Mutex::new(Bounded::default()));
        let err = Arc::new(Mutex::new(Bounded::default()));
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let child = match cmd.spawn() {
            Ok(mut c) => {
                if let Some(o) = c.stdout.take() {
                    reader(o, out.clone());
                }
                if let Some(e) = c.stderr.take() {
                    reader(e, err.clone());
                }
                Some(c)
            }
            Err(e) => {
                info.state = ProcState::FailedToStart(e.to_string());
                None
            }
        };
        self.procs.insert(
            id,
            Proc {
                info,
                child,
                out,
                err,
                deadline: Instant::now() + timeout,
            },
        );
        id
    }

    /// Reap finished processes and enforce timeouts.
    pub fn poll(&mut self) {
        for p in self.procs.values_mut() {
            if p.info.state != ProcState::Running {
                continue;
            }
            let Some(child) = p.child.as_mut() else {
                continue;
            };
            match child.try_wait() {
                Ok(Some(status)) => p.info.state = ProcState::Exited(status.code().unwrap_or(-1)),
                Ok(None) if Instant::now() >= p.deadline => {
                    kill_tree(child);
                    p.info.state = ProcState::TimedOut;
                }
                Ok(None) => {}
                Err(e) => p.info.state = ProcState::FailedToStart(e.to_string()),
            }
        }
    }

    pub fn cancel(&mut self, id: u64) -> bool {
        match self.procs.get_mut(&id) {
            Some(p) if p.info.state == ProcState::Running => {
                if let Some(c) = p.child.as_mut() {
                    kill_tree(c);
                }
                p.info.state = ProcState::Cancelled;
                true
            }
            _ => false,
        }
    }

    /// Cancel every running process of one project (e.g. on project close).
    pub fn cancel_project(&mut self, project_id: &str) -> usize {
        let ids: Vec<u64> = self
            .procs
            .values()
            .filter(|p| p.info.project_id == project_id && p.info.state == ProcState::Running)
            .map(|p| p.info.id)
            .collect();
        ids.iter().filter(|id| self.cancel(**id)).count()
    }

    pub fn info(&mut self, id: u64) -> Option<ProcInfo> {
        self.poll();
        self.procs.get(&id).map(|p| {
            let mut i = p.info.clone();
            let (o, e) = (p.out.lock().expect("lock"), p.err.lock().expect("lock"));
            i.stdout_tail = o.text();
            i.stderr_tail = e.text();
            i.truncated = o.truncated || e.truncated;
            i
        })
    }

    pub fn list(&mut self, project_id: Option<&str>) -> Vec<ProcInfo> {
        self.poll();
        let ids: Vec<u64> = self
            .procs
            .values()
            .filter(|p| project_id.is_none_or(|pid| p.info.project_id == pid))
            .map(|p| p.info.id)
            .collect();
        ids.into_iter().filter_map(|id| self.info(id)).collect()
    }

    /// Block until the process leaves Running (bounded by its own timeout), then give the
    /// capture threads up to `DRAIN_GRACE` to collect output still buffered in the pipes.
    /// Without the drain, a fast-exiting process could be reported with empty output.
    pub fn wait(&mut self, id: u64) -> Option<ProcInfo> {
        loop {
            self.poll();
            match self.procs.get(&id) {
                None => return None,
                Some(p) if p.info.state != ProcState::Running => {
                    let drained = Instant::now() + DRAIN_GRACE;
                    while !(p.out.lock().expect("lock").eof && p.err.lock().expect("lock").eof)
                        && Instant::now() < drained
                    {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    return self.info(id);
                }
                _ => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        let ids: Vec<u64> = self.procs.keys().copied().collect();
        for id in ids {
            self.cancel(id);
        }
    }
}
