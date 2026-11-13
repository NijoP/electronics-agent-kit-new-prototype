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
