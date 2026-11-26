//! Finding a tool's binary and running it briefly (version, auth status, MCP registration)
//! with a timeout, so a hung tool never hangs the Settings page.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a status command (`--version`, `auth status`, `mcp get`) may take.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a registration change (`mcp add` / `mcp remove`) may take.
pub const CHANGE_TIMEOUT: Duration = Duration::from_secs(60);

/// Where tool binaries are looked for: the directories of `PATH`, then the user install folders
/// that a GUI app's `PATH` often lacks but a login shell has.
#[derive(Debug, Clone, Default)]
pub struct Locator {
    pub dirs: Vec<PathBuf>,
}

impl Locator {
    /// `PATH` plus the common per-user install folders on this OS.
    pub fn system() -> Locator {
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        if let Some(h) = home_dir() {
            for d in [
                ".local/bin",
                ".claude/local",
                ".npm-global/bin",
                ".bun/bin",
                ".opencode/bin",
                ".cargo/bin",
                ".deno/bin",
                ".volta/bin",
                "bin",
            ] {
                dirs.push(h.join(d));
            }
        }
        #[cfg(target_os = "macos")]
        {
            dirs.push(PathBuf::from("/opt/homebrew/bin"));
            dirs.push(PathBuf::from("/usr/local/bin"));
        }
        #[cfg(windows)]
        {
            if let Some(a) = std::env::var_os("APPDATA") {
                dirs.push(PathBuf::from(a).join("npm"));
            }
        }
        Locator { dirs }
    }

    /// Only these directories (tests use a temp folder of fake tools).
    pub fn only(dirs: Vec<PathBuf>) -> Locator {
        Locator { dirs }
    }

    /// First `program` in the search directories (with the Windows executable suffixes).
    pub fn find(&self, program: &str) -> Option<PathBuf> {
        let exts: &[&str] = if cfg!(windows) {
            &[".exe", ".cmd", ".bat", ""]
        } else {
            &[""]
        };
        self.dirs.iter().find_map(|d| {
            exts.iter()
                .map(|e| d.join(format!("{program}{e}")))
                .find(|p| p.is_file())
        })
    }
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

/// What a finished tool command printed.
#[derive(Debug, Clone, Default)]
pub struct Output {
    /// `None` when the process was ended by a signal.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
    /// stderr, else stdout, trimmed to a short paragraph for an error message.
    pub fn message(&self) -> String {
        let text = if self.stderr.trim().is_empty() {
            self.stdout.trim()
        } else {
            self.stderr.trim()
        };
        let s: String = text.chars().take(400).collect();
        if !s.is_empty() {
            return s;
        }
        match self.code {
            Some(c) => format!("exit code {c}"),
            None => "ended by a signal".into(),
        }
    }
}

/// Run `program args` with no stdin, capture its output and give up after `timeout`.
pub fn run(
    program: &Path,
    args: &[&str],
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<Output, String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not run {}: {e}", program.display()))?;
    let out = child.stdout.take();
    let err = child.stderr.take();
    let t_out = std::thread::spawn(move || read_all(out));
    let t_err = std::thread::spawn(move || read_all(err));
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(Output {
                    code: status.code(),
                    stdout: t_out.join().unwrap_or_default(),
                    stderr: t_err.join().unwrap_or_default(),
                })
            }
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                // The readers are not joined: a grandchild may still hold the pipes open.
                return Err(format!(
                    "{} did not answer within {} s",
                    program.display(),
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("could not wait for {}: {e}", program.display())),
        }
    }
}

fn read_all<R: Read>(r: Option<R>) -> String {
    let mut buf = Vec::new();
    if let Some(mut r) = r {
        let _ = r.read_to_end(&mut buf);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// The first dotted number in a `--version` line: `2.1.3 (Claude Code)` → `2.1.3`,
/// `codex-cli 0.48.0` → `0.48.0`, `v1.2.0` → `1.2.0`.
pub fn parse_version(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == ',' || c == '(' || c == ')')
        .map(|t| t.trim_start_matches('v').trim_end_matches('.'))
        .find(|t| {
            t.chars().next().is_some_and(|c| c.is_ascii_digit())
                && t.contains('.')
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '+')
        })
        .map(str::to_string)
}

/// EAK's MCP stdio server (`eak-mcp`): beside the running app first, then the search folders.
pub fn find_mcp_server(loc: &Locator) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "eak-mcp.exe"
    } else {
        "eak-mcp"
    };
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(name)))
        .filter(|p| p.is_file())
        .or_else(|| loc.find("eak-mcp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_from_the_tools_own_formats() {
        assert_eq!(
            parse_version("2.1.332 (Claude Code)\n").as_deref(),
            Some("2.1.332")
        );
        assert_eq!(
            parse_version("codex-cli 0.148.0").as_deref(),
            Some("0.148.0")
        );
        assert_eq!(parse_version("opencode v1.2.0").as_deref(), Some("1.2.0"));
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn a_missing_program_is_not_found() {
        let loc = Locator::only(vec![std::env::temp_dir().join("eak-agents-no-such-dir")]);
        assert!(loc.find("claude").is_none());
    }
}
