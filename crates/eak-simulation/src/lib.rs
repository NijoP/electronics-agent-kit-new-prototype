//! # eak-simulation — optional SPICE simulation through an external ngspice
//!
//! Exapted capability: EAK does not reimplement a circuit solver. It exports a SPICE netlist
//! from the canonical EAK-IR schematic connectivity (`eak_engineering::netlist`), runs the
//! user's installed `ngspice` in batch mode as a subprocess with a timeout, and parses the
//! ASCII rawfile into typed results. ngspice is never required by any other EAK feature; when
//! it is absent every entry point reports [`SimError::NotInstalled`] with an install hint.
//!
//! Simulation is read-only: it never mutates the project, so it does not go through
//! `Engine::execute`. Results are an engineering aid computed by ngspice from generic models;
//! they are not verification evidence and never certify a design.

pub mod export;
pub mod raw;
pub mod run;

pub use export::{
    describe, export_netlist, Describe, Element, ElementKind, Export, ExtraSource, Skipped,
    SourceKind,
};
pub use run::{probe, run, Analysis, NgspiceStatus, SimRequest, SimResult};

#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("ngspice is not installed or not on PATH. Simulation is optional; install ngspice (Debian/Ubuntu: `sudo apt install ngspice`, macOS: `brew install ngspice`, Windows: https://ngspice.sourceforge.io/download.html) or set EAK_NGSPICE to the executable")]
    NotInstalled,
    #[error("invalid simulation request: {0}")]
    InvalidRequest(String),
    #[error("nothing to simulate: {0}")]
    EmptyCircuit(String),
    #[error("ngspice timed out after {0} s and was killed")]
    Timeout(u64),
    #[error("ngspice failed ({status}): {log}")]
    NgspiceFailed { status: String, log: String },
    #[error("could not read ngspice output: {0}")]
    BadOutput(String),
    #[error("I/O error: {0}")]
    Io(String),
}
