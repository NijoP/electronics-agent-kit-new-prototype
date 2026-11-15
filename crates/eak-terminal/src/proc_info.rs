//! Facts about a foreground process, read from the OS. Linux reads `/proc`; Windows reads a
//! process snapshot (names, parents) and the process command line; other systems return
//! `None`/`false`, and status falls back to process liveness plus labelled heuristics.

/// Short process name (`/proc/<pid>/comm`).
#[cfg(target_os = "linux")]
pub fn name(pid: u32) -> Option<String> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(s.trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(windows)]
pub fn name(pid: u32) -> Option<String> {
    win::snapshot()
        .into_iter()
        .find(|p| p.pid == pid)
        .map(|p| p.exe)
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn name(_pid: u32) -> Option<String> {
    None
}

/// True when the kernel reports the process asleep in a terminal read with its standard
/// input on a terminal — i.e. it is waiting for the user to type.
#[cfg(target_os = "linux")]
pub fn reading_terminal(pid: u32) -> bool {
    let wchan = std::fs::read_to_string(format!("/proc/{pid}/wchan")).unwrap_or_default();
    let in_tty_read = matches!(wchan.trim(), "n_tty_read" | "wait_woken");
    let stdin_is_tty = std::fs::read_link(format!("/proc/{pid}/fd/0"))
        .map(|p| {
            let p = p.to_string_lossy();
            p.starts_with("/dev/pts/") || p.starts_with("/dev/tty")
        })
        .unwrap_or(false);
    in_tty_read && stdin_is_tty
}

#[cfg(not(target_os = "linux"))]
pub fn reading_terminal(_pid: u32) -> bool {
    false
}

/// Runtimes that run an agent script (`node /path/to/pi`), whose own name says nothing.
// Used by the Linux and Windows readers; elsewhere only the tests call it, so the lint is allowed.
#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
const RUNTIMES: &[&str] = &["node", "bun", "deno", "python", "python3", "env"];

/// The known agent this process is, judged from its command line: the program name, or
/// the script a runtime such as node runs.
#[cfg(target_os = "linux")]
pub fn agent(pid: u32, known: &[String]) -> Option<String> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let argv: Vec<String> = raw
        .split(|b| *b == 0)
        .filter(|a| !a.is_empty())
        .map(|a| String::from_utf8_lossy(a).into_owned())
        .collect();
    agent_from_argv(&argv, known)
}

/// The known agent this process is, or the first one among its descendants (breadth first,
/// a few levels): on Windows a shell often starts an agent through a wrapper such as
/// `claude.cmd` -> `cmd.exe` -> `node.exe cli.js`.
#[cfg(windows)]
pub fn agent(pid: u32, known: &[String]) -> Option<String> {
    let procs = win::snapshot();
    let mut level = vec![pid];
    for _ in 0..4 {
        for &p in &level {
            let Some(exe) = procs.iter().find(|e| e.pid == p).map(|e| e.exe.as_str()) else {
                continue;
            };
            if let Some(label) = crate::agents::canonical(exe) {
                return Some(label.to_string());
            }
            let base = exe.strip_suffix(".exe").unwrap_or(exe).to_ascii_lowercase();
            if known.contains(&base) {
                return Some(base);
            }
            if RUNTIMES.contains(&base.as_str()) || base == "cmd" {
                if let Some(argv) = win::command_line(p) {
                    if let Some(a) =
                        agent_from_argv(&argv, known).or_else(|| agent_from_package(&argv))
                    {
                        return Some(a);
                    }
                }
            }
        }
        level = procs
            .iter()
            .filter(|e| level.contains(&e.parent) && !win::is_console_host(&e.exe))
            .map(|e| e.pid)
            .collect();
        if level.is_empty() {
            break;
        }
    }
    None
}

/// The program a shell is running in front of it: its newest child that is not a console
/// host. `None` when the shell has no child (it is at its prompt).
#[cfg(windows)]
pub fn foreground_child(shell: u32) -> Option<u32> {
    win::snapshot()
        .into_iter()
        .filter(|p| p.parent == shell && p.pid != shell && !win::is_console_host(&p.exe))
        .map(|p| p.pid)
        .next_back()
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn agent(_pid: u32, _known: &[String]) -> Option<String> {
    None
}

/// An agent installed as an npm package and started as `node <package dir>/cli.js`: the
/// package directory (`claude-code`, `codex`, ...) names it when the script name does not.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn agent_from_package(argv: &[String]) -> Option<String> {
    argv.iter().skip(1).take(3).find_map(|a| {
        a.split(['/', '\\'])
            .filter(|seg| !seg.is_empty() && *seg != "node_modules" && !seg.starts_with('@'))
            .find_map(crate::agents::canonical)
            .map(str::to_string)
    })
}

#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
pub fn agent_from_argv(argv: &[String], known: &[String]) -> Option<String> {
    let base = |s: &str| {
        let b = s.rsplit(['/', '\\']).next().unwrap_or(s);
        b.strip_suffix(".exe").unwrap_or(b).to_string()
    };
    for (i, a) in argv.iter().enumerate().take(3) {
        if i > 0 && a.starts_with('-') {
            continue;
        }
        if let Some(label) = crate::agents::canonical(a) {
            return Some(label.to_string());
        }
        let b = base(a);
        if known.contains(&b) {
            return Some(b);
        }
        if !RUNTIMES.contains(&b.as_str()) && i == 0 {
            return None;
        }
    }
    None
}

/// Windows process snapshot and command-line reader.
#[cfg(windows)]
mod win {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    use windows_sys::Wdk::System::Threading::{
        NtQueryInformationProcess, ProcessCommandLineInformation,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE, UNICODE_STRING};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    #[derive(Clone)]
    pub struct Proc {
        pub pid: u32,
        pub parent: u32,
        pub exe: String,
    }

    /// Console hosts ConPTY attaches to console programs; never the program the user runs.
    pub fn is_console_host(exe: &str) -> bool {
        exe.eq_ignore_ascii_case("conhost.exe") || exe.eq_ignore_ascii_case("OpenConsole.exe")
    }

    /// Every process (pid, parent pid, exe name). Status is computed for each session on every
    /// poll, so one snapshot is shared for a short while instead of taken per call.
    pub fn snapshot() -> Vec<Proc> {
        static CACHE: Mutex<Option<(Instant, Vec<Proc>)>> = Mutex::new(None);
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, procs)) = cache.as_ref() {
            if at.elapsed() < Duration::from_millis(400) {
                return procs.clone();
            }
        }
        let procs = take_snapshot();
        *cache = Some((Instant::now(), procs.clone()));
        procs
    }

    fn take_snapshot() -> Vec<Proc> {
        let mut out = Vec::new();
        // SAFETY: the snapshot handle is checked and closed; the entry is a plain struct whose
        // dwSize is set as the API requires.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE || snap.is_null() {
                return out;
            }
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snap, &mut e) != 0 {
                loop {
                    let n = e
                        .szExeFile
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(e.szExeFile.len());
                    out.push(Proc {
                        pid: e.th32ProcessID,
                        parent: e.th32ParentProcessID,
                        exe: String::from_utf16_lossy(&e.szExeFile[..n]),
                    });
                    if Process32NextW(snap, &mut e) == 0 {
                        break;
                    }
                }
            }
            CloseHandle(snap);
        }
        out
    }

    /// The process command line split into arguments, or `None` when it cannot be read
    /// (for example a process of another user).
    pub fn command_line(pid: u32) -> Option<Vec<String>> {
        // SAFETY: the process handle is checked and closed; the buffer is 8-byte aligned and
        // sized from the length the first call reports; the UNICODE_STRING it holds points
        // inside that buffer and is read only while the buffer lives.
        let text = unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut len = 0u32;
            NtQueryInformationProcess(
                h,
                ProcessCommandLineInformation,
                std::ptr::null_mut(),
                0,
                &mut len,
            );
            let min = std::mem::size_of::<UNICODE_STRING>() / 8 + 1;
            let mut buf = vec![0u64; (len as usize).div_ceil(8).max(min)];
            let status = NtQueryInformationProcess(
                h,
                ProcessCommandLineInformation,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
                &mut len,
            );
            CloseHandle(h);
            if status < 0 {
                return None;
            }
            let us = &*(buf.as_ptr() as *const UNICODE_STRING);
            if us.Buffer.is_null() || us.Length == 0 {
                return None;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(
                us.Buffer,
                us.Length as usize / 2,
            ))
        };
        Some(super::split_command_line(&text))
    }
}

/// Splits a Windows command line into arguments: whitespace separates, double quotes group.
#[cfg_attr(not(windows), allow(dead_code))]
fn split_command_line(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut quoted, mut any) = (false, false);
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{agent_from_argv, agent_from_package, split_command_line};

    #[test]
    fn windows_command_lines_and_npm_agents() {
        let argv = split_command_line(
            r#""C:\Program Files\nodejs\node.exe"  "C:\Users\u\npm\node_modules\@anthropic-ai\claude-code\cli.js" --resume"#,
        );
        assert_eq!(argv.len(), 3);
        assert_eq!(argv[0], r"C:\Program Files\nodejs\node.exe");
        assert_eq!(agent_from_argv(&argv, &[]), None);
        assert_eq!(agent_from_package(&argv).as_deref(), Some("claude"));
        let codex = split_command_line(r"node C:\npm\node_modules\@openai\codex\bin\codex.js");
        assert_eq!(agent_from_argv(&codex, &[]).as_deref(), Some("codex"));
        assert_eq!(
            agent_from_package(&split_command_line("node server.js")),
            None
        );
    }

    #[test]
    fn agents_are_found_directly_or_behind_a_runtime() {
        let known: Vec<String> = ["pi", "opencode"].iter().map(|s| s.to_string()).collect();
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            agent_from_argv(&v(&["/home/u/.opencode/bin/opencode"]), &known).as_deref(),
            Some("opencode")
        );
        assert_eq!(
            agent_from_argv(&v(&["node", "/home/u/.local/bin/pi"]), &known).as_deref(),
            Some("pi")
        );
        assert_eq!(
            agent_from_argv(&v(&["node", "--no-warnings", "/x/pi"]), &known).as_deref(),
            Some("pi")
        );
        assert_eq!(agent_from_argv(&v(&["vim", "pi"]), &known), None);
        assert_eq!(agent_from_argv(&v(&["node", "server.js"]), &known), None);
    }
}
