//! Project lock states and takeover safety (ADR-0029). Written during the testing freeze; unrun.
use eak_runtime::store::{acquire_lock, inspect_lock, LockState};
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("eak-lock-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A process id above every platform's pid limit, so it is never running.
const DEAD_PID: u32 = 4_000_000_000;

#[test]
fn unlocked_then_open_elsewhere_for_a_live_holder() {
    let d = tmp("live");
    assert_eq!(inspect_lock(&d), LockState::Unlocked);
    let held = acquire_lock(&d, false).unwrap();
    assert_eq!(
        inspect_lock(&d),
        LockState::OpenElsewhere { this_app: true }
    );
    // A second open is refused, and takeover must not override a live holder.
    assert!(acquire_lock(&d, false).is_err());
    let err = acquire_lock(&d, true).unwrap_err();
    assert!(err.contains("already open"), "{err}");
    assert!(d.join(".lock").exists());
    drop(held);
    assert_eq!(inspect_lock(&d), LockState::Unlocked);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn dead_holder_is_interrupted_and_only_then_takeover_works() {
    let d = tmp("stale");
    std::fs::write(d.join(".lock"), DEAD_PID.to_string()).unwrap();
    assert_eq!(
        inspect_lock(&d),
        LockState::Interrupted {
            pending_operations: 0
        }
    );
    // Without an explicit takeover the lock is kept and the message has no process id.
    let err = acquire_lock(&d, false).unwrap_err();
    assert!(
        err.contains("interrupted") && !err.contains(&DEAD_PID.to_string()),
        "{err}"
    );
    assert!(d.join(".lock").exists());
    let lock = acquire_lock(&d, true).unwrap();
    assert_eq!(
        inspect_lock(&d),
        LockState::OpenElsewhere { this_app: true }
    );
    drop(lock);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn foreign_host_garbage_and_missing_folder_are_never_taken_over() {
    let d = tmp("foreign");
    std::fs::write(d.join(".lock"), format!("{DEAD_PID}\nsome-other-computer")).unwrap();
    assert!(matches!(inspect_lock(&d), LockState::Unverifiable { .. }));
    assert!(acquire_lock(&d, true).is_err());
    std::fs::write(d.join(".lock"), "not a pid").unwrap();
    assert!(matches!(inspect_lock(&d), LockState::Unverifiable { .. }));
    assert!(acquire_lock(&d, true).is_err());
    assert!(d.join(".lock").exists());
    let _ = std::fs::remove_dir_all(&d);
    assert!(matches!(inspect_lock(&d), LockState::Unavailable { .. }));
}
