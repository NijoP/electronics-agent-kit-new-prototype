//! # eak-core — EAK-IR
//!
//! The canonical, versioned engineering representation of an EAK project
//! (Spec §2.1 "One canonical EAK-IR"; Execution Instructions §7.1).
//!
//! * `geom`     – exact integer geometry (nanometres).
//! * `library`  – native component / symbol / footprint definitions.
//! * `graphic`  – drawing primitives for symbol bodies and sheets (schema v3, ADR-0040).
//! * `libmeta`  – library identity, scope, maturity, catalogue and committed-definition references (ADR-0035).
//! * `sexpr`    – bounded S-expression reader for library interchange files (import groundwork).
//! * `ir`       – project, schematic, PCB, constraints, manufacturing config.
//! * `integrity`– structural validation of a whole project (dangling references etc.).
//! * `migrate`  – schema version handling; unknown/newer schemas fail safely.
//! * `builtin`  – the built-in, provenance-tagged starter component library.
//! * `units`    – typed physical quantities (from the earlier EAK prototype; not IR fields).
//! * `datasheet` – datasheet-aware values, supplier refs, fact labels (not IR fields; ADR-0024).

pub mod builtin;
pub mod datasheet;
pub mod geom;
pub mod graphic;
pub mod integrity;
pub mod ir;
pub mod kicad_import;
pub mod libmeta;
pub mod library;
pub mod migrate;
pub mod pinmap;
pub mod sexpr;
pub mod units;

pub use geom::{Nm, Point, Rect, Rotation};
pub use graphic::{Graphic, LineStyle, Stroke};
pub use ir::*;
pub use library::*;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("project file is not valid EAK-IR JSON: {0}")]
    Parse(String),
    #[error("unsupported schema version {found} (this build supports up to {supported})")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("project integrity check failed: {0:?}")]
    Integrity(Vec<String>),
}

/// Serialise a project to canonical pretty JSON (BTreeMaps give a stable key order,
/// so identical designs produce byte-identical files).
pub fn to_json(p: &Project) -> String {
    serde_json::to_string_pretty(p).expect("EAK-IR is always serialisable")
}

/// Parse, migrate and integrity-check a project document.
pub fn from_json(s: &str) -> Result<Project, CoreError> {
    let raw: serde_json::Value =
        serde_json::from_str(s).map_err(|e| CoreError::Parse(e.to_string()))?;
    let migrated = migrate::migrate(raw)?;
    let p: Project =
        serde_json::from_value(migrated).map_err(|e| CoreError::Parse(e.to_string()))?;
    let problems = integrity::check(&p);
    if !problems.is_empty() {
        return Err(CoreError::Integrity(problems));
    }
    Ok(p)
}
