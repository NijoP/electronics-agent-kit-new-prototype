//! Revision-bound verification evidence (Spec §9; Execution Instructions §9.1).
//! A report always lists what was NOT checked; "no errors" never means "design correct".

use eak_core::{Id, Point, Project};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    /// Canonical EAK-IR ids of affected objects (for cross-probing).
    pub objects: Vec<Id>,
    #[serde(default)]
    pub location: Option<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleInfo {
    pub id: String,
    pub description: String,
    /// Rule parameters actually used (human readable).
    pub parameters: String,
    /// Standard/source the rule derives from and its verification status.
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Outcome {
    /// Every implemented rule ran and produced no error. Does not prove correctness.
    NoErrorsInImplementedChecks,
    Errors,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub check: String,
    pub project_id: Id,
    pub revision: u64,
    /// SHA-256 of the canonical EAK-IR the check ran against.
    pub input_sha256: String,
    pub tool: String,
    pub rules: Vec<RuleInfo>,
    pub findings: Vec<Finding>,
    pub outcome: Outcome,
    pub errors: usize,
    pub warnings: usize,
    /// Checks a professional flow would include that this build does not perform.
    pub not_implemented: Vec<String>,
    pub limitations: Vec<String>,
}

pub fn project_hash(p: &Project) -> String {
    let mut h = Sha256::new();
    h.update(eak_core::to_json(p).as_bytes());
    format!("{:x}", h.finalize())
}

impl Report {
    pub fn new(
        check: &str,
        p: &Project,
        rules: Vec<RuleInfo>,
        mut findings: Vec<Finding>,
        not_implemented: Vec<String>,
        limitations: Vec<String>,
    ) -> Report {
        findings.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then(a.rule_id.cmp(&b.rule_id))
                .then(a.message.cmp(&b.message))
        });
        let errors = findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count();
        let warnings = findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count();
        Report {
            check: check.into(),
            project_id: p.id.clone(),
            revision: p.revision,
            input_sha256: project_hash(p),
            tool: format!("eak-validation {}", env!("CARGO_PKG_VERSION")),
            rules,
            findings,
            outcome: if errors == 0 {
                Outcome::NoErrorsInImplementedChecks
            } else {
                Outcome::Errors
            },
            errors,
            warnings,
            not_implemented,
            limitations,
        }
    }
}

pub(crate) fn rule(
    id: &str,
    description: &str,
    parameters: impl Into<String>,
    source: &str,
) -> RuleInfo {
    RuleInfo {
        id: id.into(),
        description: description.into(),
        parameters: parameters.into(),
        source: source.into(),
    }
}

pub(crate) fn finding(
    rule_id: &str,
    severity: Severity,
    message: impl Into<String>,
    objects: Vec<Id>,
    location: Option<Point>,
) -> Finding {
    Finding {
        rule_id: rule_id.into(),
        severity,
        message: message.into(),
        objects,
        location,
    }
}
