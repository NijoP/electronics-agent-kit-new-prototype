//! Simulation IPC: read-only, optional ngspice, and never a design mutation.

use eak_core::Project;
use eak_desktop::*;
use eak_runtime::index::Index;
use eak_runtime::{fixtures, Engine};
use eak_simulation::{Analysis, SimRequest};
use std::path::PathBuf;

fn open_session(name: &str) -> (Session, PathBuf) {
    let d = std::env::temp_dir().join(format!("eak-desktop-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let s = Session::new(Index::in_memory());
    create_project(&s, &d.display().to_string(), name).unwrap();
    (s, d)
}

#[test]
fn simulation_status_needs_an_open_project_and_reports_nets() {
    let s = Session::new(Index::in_memory());
    assert!(simulation_status(&s).is_err());
    let (s, d) = open_session("simstatus");
    let st = simulation_status(&s).unwrap();
    assert_eq!(st.revision, current_project(&s).unwrap().project.revision);
    assert!(st.circuit.elements.is_empty());
    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn run_simulation_on_an_empty_design_is_a_clear_error_and_changes_nothing() {
    let (s, d) = open_session("simrun");
    let before = current_project(&s).unwrap().project.revision;
    let err = run_simulation(&s, &SimRequest { analysis: Analysis::Op, sources: vec![], timeout_secs: None }).unwrap_err();
    // Either ngspice is absent (clear install hint) or the circuit is empty; both are errors.
    assert!(err.contains("not installed") || err.contains("nothing to simulate"), "{err}");
    assert_eq!(current_project(&s).unwrap().project.revision, before);
    close_project(&s).unwrap();
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn fixture_schematic_describes_simulatable_parts_and_skips_the_rest() {
    let mut e = Engine::in_memory(Project::new("fx"));
    fixtures::build_schematic(&mut e).unwrap();
    let d = eak_simulation::describe(e.project());
    assert!(!d.nets.is_empty());
    assert!(d.elements.iter().any(|x| x.refdes.starts_with('R')));
    assert!(d.skipped.iter().any(|x| x.refdes.starts_with('U') || x.refdes.starts_with('J')), "{:?}", d.skipped);
}
