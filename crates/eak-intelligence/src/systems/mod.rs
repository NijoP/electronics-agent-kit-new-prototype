//! Multi-domain engineering intelligence (ADR-0027): which engineering disciplines a product
//! combines, its architecture (product -> subsystem -> board -> block -> component -> region),
//! its requirements and the explainable constraint graph compiled from them, calculation
//! records, cross-domain interactions, risks and the domain mental models.
//!
//! * `domains`     – engineering domain registry, mapped onto the DESK classes.
//! * `profile`     – product architecture nodes with inherited domain tags.
//! * `model`       – requirements, constraints, calculations, risks, dependency graph.
//! * `interaction` – cross-domain interaction rules.
//! * `domain_models` – typed per-domain field sets (units, explicit Unknown) and checks with
//!   the six shared check states (ADR-0026).
//! * `discipline`  – per-discipline mental models (power, RF, ...) as data; the process
//!   models recovered from DESK (schematic, placement, routing, ...) stay in `crate::mental`.
//! * `compile`     – the requirement compiler and the UI intelligence summary.
//! * `lifecycle`   – the 13-stage engineering lifecycle as a phase state machine.
//!
//! Deterministic and read-only: no model is consulted, nothing is stored, and the design is
//! never edited. Domain checks here compare stated requirements and derived constraints only;
//! design verification stays in `eak-validation` and numbers in `eak-engineering`.

pub mod compile;
pub mod discipline;
pub mod domain_models;
pub mod domains;
pub mod interaction;
pub mod lifecycle;
pub mod model;
pub mod profile;

pub use compile::{
    compile, BlockHint, CompileInput, EngineeringModel, IntelligenceSummary, BOARD, PRODUCT,
};
pub use profile::Level;
