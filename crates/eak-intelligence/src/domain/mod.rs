//! DESK domain intelligence in Rust: the full DESK domain-class registry, a deterministic
//! classifier over the EAK-IR design, and maturity ceilings on every claim.

pub mod classify;
pub mod features;
pub mod registry;

pub use classify::{
    classify_project, classify_text, Candidate, DomainClassification, Guidance, Status,
    LIMITATIONS, NOT_IMPLEMENTED,
};
pub use registry::{registry, Archetype, Ceiling, Maturity, Registry};
