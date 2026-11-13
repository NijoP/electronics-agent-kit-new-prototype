//! # eak-validation — the Verification Engine
//!
//! Independent of the agent: AI output is checked here, never self-certified.
//! Every report is bound to a project revision and input hash and lists the checks
//! that are not implemented (unknown is never reported as pass).

pub mod drc;
pub mod erc;
pub mod mech;
pub mod report;
pub mod sourcing;

pub use report::{Finding, Outcome, Report, Severity};
