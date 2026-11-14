//! Facts about a foreground process, read from the OS. Linux reads `/proc`; other systems
//! return `None`/`false`, and status falls back to process liveness plus labelled heuristics.

/// Short process name (`/proc/<pid>/comm`).
#[cfg(target_os = "linux")]
pub fn name(pid: u32) -> Option<String> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(s.trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(not(target_os = "linux"))]
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

#[cfg(not(target_os = "linux"))]
pub fn agent(_pid: u32, _known: &[String]) -> Option<String> {
    None
}

pub fn agent_from_argv(argv: &[String], known: &[String]) -> Option<String> {
    let base = |s: &str| {
        let b = s.rsplit(['/', '\\']).next().unwrap_or(s);
        b.strip_suffix(".exe").unwrap_or(b).to_string()
    };
    for (i, a) in argv.iter().enumerate().take(3) {
        if i > 0 && a.starts_with('-') {
            continue;
        }
        let b = base(a);
        if known.iter().any(|k| *k == b) {
            return Some(b);
        }
        if !RUNTIMES.contains(&b.as_str()) && i == 0 {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::agent_from_argv;

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
