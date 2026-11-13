//! Process supervision: timeout, cancellation, bounded output, project isolation.

use eak_runtime::process::{ProcState, Supervisor};
use std::time::Duration;

fn sleeper(secs: u32) -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            "powershell".into(),
            vec![
                "-NoProfile".into(),
                "-Command".into(),
                format!("Start-Sleep -Seconds {secs}"),
            ],
        )
    } else {
        ("sleep".into(), vec![secs.to_string()])
    }
}

fn echo(text: &str) -> (String, Vec<String>) {
    if cfg!(windows) {
        ("cmd".into(), vec!["/C".into(), format!("echo {text}")])
    } else {
        ("sh".into(), vec!["-c".into(), format!("echo {text}")])
    }
}

#[test]
fn captures_output_and_exit_code() {
    let mut s = Supervisor::default();
    let (p, a) = echo("hello-eak");
    let id = s.start("prj_a", "echo", &p, &a, ".", Duration::from_secs(30));
    let info = s.wait(id).unwrap();
    assert_eq!(info.state, ProcState::Exited(0));
    assert!(info.stdout_tail.contains("hello-eak"));
}

#[test]
fn timeout_kills_process() {
    let mut s = Supervisor::default();
    let (p, a) = sleeper(30);
    let id = s.start("prj_a", "sleep", &p, &a, ".", Duration::from_millis(300));
    let info = s.wait(id).unwrap();
    assert_eq!(info.state, ProcState::TimedOut);
}

#[test]
fn cancelling_one_project_does_not_touch_another() {
    let mut s = Supervisor::default();
    let (p, a) = sleeper(30);
    let a1 = s.start("prj_a", "a", &p, &a, ".", Duration::from_secs(60));
    let b1 = s.start("prj_b", "b", &p, &a, ".", Duration::from_secs(60));
    assert_eq!(s.cancel_project("prj_a"), 1);
    assert_eq!(s.info(a1).unwrap().state, ProcState::Cancelled);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(s.info(b1).unwrap().state, ProcState::Running);
    assert!(s.cancel(b1));
}

#[test]
fn missing_program_is_reported_not_hidden() {
    let mut s = Supervisor::default();
    let id = s.start(
        "prj_a",
        "bad",
        "definitely-not-a-real-program-eak",
        &[],
        ".",
        Duration::from_secs(5),
    );
    assert!(matches!(
        s.info(id).unwrap().state,
        ProcState::FailedToStart(_)
    ));
}

/// A command whose own child (the grandchild of the supervisor) prints its pid and then
/// sleeps; the direct child waits on it. Cancelling must terminate both.
fn tree_spawner() -> (String, Vec<String>) {
    if cfg!(windows) {
        (
            "powershell".into(),
            vec![
                "-NoProfile".into(),
                "-Command".into(),
                "$p = Start-Process -FilePath powershell -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 60' -PassThru -WindowStyle Hidden; Write-Output \"GRANDCHILD=$($p.Id)\"; Wait-Process -Id $p.Id".into(),
            ],
        )
    } else {
        (
            "sh".into(),
            vec![
                "-c".into(),
                "sleep 60 & echo \"GRANDCHILD=$!\"; wait".into(),
            ],
        )
    }
}

/// Is `pid` still a live (non-zombie) process? Uses platform tools so the test does not
/// need its own FFI.
fn alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 0 }} else {{ exit 1 }}"),
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        // An orphaned grandchild is re-parented and reaped by init/launchd; until then
        // it is a zombie ("Z"), which no longer runs and counts as terminated.
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .expect("ps runs");
        let stat = String::from_utf8_lossy(&out.stdout).trim().to_string();
        !stat.is_empty() && !stat.starts_with('Z')
    }
}

#[test]
fn cancel_terminates_the_whole_process_tree() {
    let mut s = Supervisor::default();
    let (p, a) = tree_spawner();
    let id = s.start("prj_a", "tree", &p, &a, ".", Duration::from_secs(120));
    // Wait (bounded) for the grandchild to report its pid.
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let pid = loop {
        let info = s.info(id).unwrap();
        assert_eq!(info.state, ProcState::Running, "{info:?}");
        if let Some(pid) = info
            .stdout_tail
            .lines()
            .find_map(|l| l.trim().strip_prefix("GRANDCHILD="))
            .and_then(|v| v.trim().parse::<u32>().ok())
        {
            break pid;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "grandchild never started"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(
        alive(pid),
        "grandchild {pid} should be running before cancel"
    );
    assert!(s.cancel(id));
    assert_eq!(s.info(id).unwrap().state, ProcState::Cancelled);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while alive(pid) {
        assert!(
            std::time::Instant::now() < deadline,
            "grandchild {pid} survived cancellation of its process tree"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn fast_exit_output_is_not_lost() {
    // Regression: `wait` used to return as soon as the child exited, before the capture
    // threads had read the pipes, so short-lived output could be reported as empty.
    let mut s = Supervisor::default();
    for i in 0..20 {
        let (p, a) = echo(&format!("line-{i}"));
        let id = s.start("prj_a", "echo", &p, &a, ".", Duration::from_secs(30));
        let info = s.wait(id).unwrap();
        assert_eq!(info.state, ProcState::Exited(0));
        assert!(info.stdout_tail.contains(&format!("line-{i}")), "{info:?}");
    }
}
