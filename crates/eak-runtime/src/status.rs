//! Read-only status of a project folder for Project Home (Spec §13.1).
//!
//! Everything here is read from files that already exist: the project file (revision, content
//! counts, modification time) and the revision-bound evidence reports the engine saved
//! (`evidence/<check>-rNNNNNN.json`). Nothing is computed that is not stored, and nothing is
//! invented: a check with no evidence for the current revision is *pending*, never "passed".

use crate::store;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Checks Project Home expects evidence for before a revision counts as validated.
pub const EXPECTED_CHECKS: [&str; 4] = ["erc", "drc", "mech", "dfm"];

/// What the saved evidence says about one check for the project's current revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckEvidence {
    pub check: String,
    pub errors: u64,
    pub warnings: u64,
    /// Overall state recorded in the report (`UNKNOWN` for reports older than ADR-0026).
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectStatus {
    /// Modification time of the project file, ms since the Unix epoch.
    pub last_modified_ms: Option<u64>,
    pub revision: u64,
    /// Engineering stage derived from what the project contains (not a stored workflow phase).
    pub phase: String,
    /// Checks with no evidence for the current revision (never run, or the design changed since).
    pub pending_validation: Vec<String>,
    /// Evidence for the current revision only; stale evidence is not counted.
    pub evidence: Vec<CheckEvidence>,
    /// Errors in current evidence; `None` when no check has current evidence.
    pub unresolved_errors: Option<u64>,
}

/// Stage named after the furthest design content present.
pub fn phase_of(p: &eak_core::Project) -> &'static str {
    if !p.pcb.tracks.is_empty() {
        "Routing"
    } else if !p.pcb.footprints.is_empty() {
        "PCB placement"
    } else if !p.schematic.symbols.is_empty() {
        "Schematic capture"
    } else {
        "Empty project"
    }
}

fn read_evidence(dir: &Path, check: &str, revision: u64) -> Option<CheckEvidence> {
    let path = dir
        .join("evidence")
        .join(format!("{check}-r{revision:06}.json"));
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    // Evidence bound to another revision under this file name is not accepted.
    if v.get("revision")?.as_u64()? != revision {
        return None;
    }
    Some(CheckEvidence {
        check: check.into(),
        errors: v.get("errors")?.as_u64()?,
        warnings: v.get("warnings").and_then(|w| w.as_u64()).unwrap_or(0),
        state: v
            .get("overall")
            .and_then(|s| s.as_str())
            .unwrap_or("UNKNOWN")
            .into(),
    })
}

/// Status of the project in `dir`; `None` when it cannot be read (not an EAK project folder).
pub fn read(dir: &Path) -> Option<ProjectStatus> {
    let p = store::read_project(dir).ok()?;
    let last_modified_ms = std::fs::metadata(dir.join(store::PROJECT_FILE))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    let mut evidence = vec![];
    let mut pending = vec![];
    for check in EXPECTED_CHECKS {
        match read_evidence(dir, check, p.revision) {
            Some(e) => evidence.push(e),
            None => pending.push(check.to_uppercase()),
        }
    }
    let unresolved_errors = if evidence.is_empty() {
        None
    } else {
        Some(evidence.iter().map(|e| e.errors).sum())
    };
    Some(ProjectStatus {
        last_modified_ms,
        revision: p.revision,
        phase: phase_of(&p).into(),
        pending_validation: pending,
        evidence,
        unresolved_errors,
    })
}

#[cfg(test)]
mod tests {
    // Written during the testing freeze; never compiled or run.
    use super::*;

    #[test]
    fn fresh_project_has_every_check_pending_and_no_error_count() {
        let dir = std::env::temp_dir().join(format!("eak-status-{}", store::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        store::write_project(&dir, &eak_core::Project::new("s")).unwrap();
        let s = read(&dir).unwrap();
        assert_eq!(s.phase, "Empty project");
        assert_eq!(s.pending_validation.len(), EXPECTED_CHECKS.len());
        assert_eq!(s.unresolved_errors, None);
        assert!(s.last_modified_ms.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn evidence_for_another_revision_is_ignored() {
        let dir = std::env::temp_dir().join(format!("eak-status-stale-{}", store::now_ms()));
        std::fs::create_dir_all(dir.join("evidence")).unwrap();
        let p = eak_core::Project::new("s");
        store::write_project(&dir, &p).unwrap();
        let name = format!("erc-r{:06}.json", p.revision);
        std::fs::write(
            dir.join("evidence").join(name),
            format!(r#"{{"revision":{},"errors":3}}"#, p.revision + 1),
        )
        .unwrap();
        assert!(read(&dir).unwrap().evidence.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
