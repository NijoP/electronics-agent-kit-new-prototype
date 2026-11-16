//! Minimal project-scoped process supervisor (EAK-native code; design concepts credited in third_party/licenses/).
//!
//! Each process belongs to one project. It has a wall-clock timeout, bounded stdout/stderr
//! capture, cancellation that terminates the whole process tree, and an explicit final
//! state. Cancelling or crashing a process in one project never touches another project.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
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

struct Bounded {
    buf: VecDeque<u8>,
    truncated: bool,
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
    /// One flag per output stream, true once that stream has been fully read.
    readers: Vec<Arc<AtomicBool>>,
    deadline: Instant,
}

#[derive(Default)]
pub struct Supervisor {
    procs: BTreeMap<u64, Proc>,
    next: u64,
}

/// Drain `r` into `sink` on a thread. The returned flag turns true once the stream has ended, so
/// `wait` can let the last output arrive before it reports the process (a finished process can
/// still have unread bytes in its pipe).
fn reader<R: Read + Send + 'static>(mut r: R, sink: Arc<Mutex<Bounded>>) -> Arc<AtomicBool> {
    let done = Arc::new(AtomicBool::new(false));
    let flag = done.clone();
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        loop {
            match r.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink.lock().expect("capture lock").push(&chunk[..n]),
            }
        }
        flag.store(true, Ordering::SeqCst);
    });
    done
}

/// The `kill` argument that signals exactly one process group, or `None` when `pgid` could
/// address anything broader (0 = own group, 1 = init, negatives = other group semantics).
#[cfg_attr(not(unix), allow(dead_code))]
fn group_kill_target(pgid: i64) -> Option<String> {
    (pgid > 1).then(|| format!("-{pgid}"))
}

/// Terminate a process and all of its children.
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        // The child was started as its own process-group leader. `--` stops option parsing so
        // the negative id is always read as a process group, and the guard guarantees the
        // target is a real group (> 1): an id of 0, 1 or below would signal far more than
        // this child's own tree (every process of the user, on some procps versions).
        if let Some(target) = group_kill_target(i64::from(child.id())) {
            let _ = Command::new("kill")
                .args(["-s", "TERM", "--", &target])
                .status();
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
        let out = Arc::new(Mutex::new(Bounded {
            buf: VecDeque::new(),
            truncated: false,
        }));
        let err = Arc::new(Mutex::new(Bounded {
            buf: VecDeque::new(),
            truncated: false,
        }));
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
        let mut readers = Vec::new();
        let child = match cmd.spawn() {
            Ok(mut c) => {
                if let Some(o) = c.stdout.take() {
                    readers.push(reader(o, out.clone()));
                }
                if let Some(e) = c.stderr.take() {
                    readers.push(reader(e, err.clone()));
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
                readers,
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

    /// Block until the process leaves Running (bounded by its own timeout).
    pub fn wait(&mut self, id: u64) -> Option<ProcInfo> {
        loop {
            self.poll();
            match self.procs.get(&id) {
                None => return None,
                Some(p) if p.info.state != ProcState::Running => {
                    // Let the readers drain what the exited process wrote (bounded: a grandchild
                    // may keep the pipe open).
                    let grace = Instant::now() + Duration::from_secs(1);
                    while Instant::now() < grace
                        && !p.readers.iter().all(|d| d.load(Ordering::SeqCst))
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

#[cfg(test)]
mod tests {
    use super::group_kill_target;

    #[test]
    fn kill_guard_rejects_ids_that_could_signal_more_than_one_group() {
        for id in [i64::MIN, -100, -1, 0, 1] {
            assert_eq!(group_kill_target(id), None, "id {id} must be rejected");
        }
        assert_eq!(group_kill_target(2).as_deref(), Some("-2"));
        assert_eq!(group_kill_target(4242).as_deref(), Some("-4242"));
    }
}
