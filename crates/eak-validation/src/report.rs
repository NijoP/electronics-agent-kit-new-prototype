//! Revision-bound verification evidence (Spec §9; Execution Instructions §9.1).
//! A report always lists what was NOT checked; "no errors" never means "design correct".

use eak_core::{Id, Point, Project};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A measured value with its engineering unit (`mm`, `V`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quantity {
    pub value: f64,
    pub unit: String,
}

/// Direction of a limit: `Min` means the measured value must be at least the limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bound {
    Min,
    Max,
}

/// The limit a measured value was checked against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Limit {
    pub bound: Bound,
    pub value: f64,
    pub unit: String,
}

/// One actionable rule violation. Fields added after the first release are
/// `#[serde(default)]`, so reports written before them still deserialise.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable id `<rule_id>-<12 hex>` derived from the rule, the sorted object ids and a
    /// semantic discriminator; empty only in reports written before ids existed.
    #[serde(default)]
    pub id: String,
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    /// Canonical EAK-IR ids of affected objects (for cross-probing).
    pub objects: Vec<Id>,
    /// In the coordinate space of the report's editor: schematic for ERC, board for DRC/MECH.
    #[serde(default)]
    pub location: Option<Point>,
    /// Measured value, only for quantitative rules.
    #[serde(default)]
    pub measured: Option<Quantity>,
    /// Limit the measured value was checked against, only for quantitative rules.
    #[serde(default)]
    pub required: Option<Limit>,
    /// Suggested correction in engineering language; never a guarantee of correctness.
    #[serde(default)]
    pub suggested_fix: Option<String>,
}

/// Deterministic finding id: rule id plus the first 12 hex digits of SHA-256 over the rule id,
/// the sorted, de-duplicated object ids and a discriminator `key`.
pub fn finding_id(rule_id: &str, objects: &[Id], key: &str) -> String {
    let mut objs: Vec<&str> = objects.iter().map(String::as_str).collect();
    objs.sort_unstable();
    objs.dedup();
    let mut h = Sha256::new();
    h.update(rule_id.as_bytes());
    for o in objs {
        h.update([0x1f]);
        h.update(o.as_bytes());
    }
    h.update([0x1e]);
    h.update(key.as_bytes());
    let hex = format!("{:x}", h.finalize());
    format!("{rule_id}-{}", &hex[..12])
}

/// Millimetres rounded to 1 nm, the resolution of canonical geometry.
fn round_mm(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

impl Finding {
    /// Sets the discriminator used for the id when one rule can fire several times on the
    /// same objects (a pin number, `drill`/`ring`, a track end, a net name).
    pub(crate) fn key(mut self, key: &str) -> Self {
        self.id = finding_id(&self.rule_id, &self.objects, key);
        self
    }
    pub(crate) fn measured(mut self, value: f64, unit: &str) -> Self {
        self.measured = Some(Quantity {
            value,
            unit: unit.into(),
        });
        self
    }
    pub(crate) fn required(mut self, bound: Bound, value: f64, unit: &str) -> Self {
        self.required = Some(Limit {
            bound,
            value,
            unit: unit.into(),
        });
        self
    }
    /// Measured and minimum required length, both in millimetres (rounded to 1 nm).
    pub(crate) fn mm_min(self, measured_mm: f64, required_mm: f64) -> Self {
        self.measured(round_mm(measured_mm), "mm")
            .required(Bound::Min, round_mm(required_mm), "mm")
    }
    pub(crate) fn fix(mut self, fix: impl Into<String>) -> Self {
        self.suggested_fix = Some(fix.into());
        self
    }
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
                .then(a.id.cmp(&b.id))
        });
        // Ids must be unique within a report; a residual collision gets a deterministic suffix.
        let mut seen: BTreeMap<String, usize> = BTreeMap::new();
        for f in &mut findings {
            let n = seen.entry(f.id.clone()).or_insert(0);
            *n += 1;
            if *n > 1 {
                f.id = format!("{}-{}", f.id, n);
            }
        }
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
        id: finding_id(rule_id, &objects, ""),
        rule_id: rule_id.into(),
        severity,
        message: message.into(),
        objects,
        location,
        measured: None,
        required: None,
        suggested_fix: None,
    }
}
