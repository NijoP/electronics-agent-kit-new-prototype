//! Explicit check states, per-rule results and the engineering risk record (Spec §10, §10.1).
//!
//! A rule never passes by default: a rule that could not be evaluated (missing input, missing
//! data) is `Unknown`, and a result only speaks for the revision and input hash it ran against.

use serde::{Deserialize, Serialize};

/// Outcome of one check. `Unknown` is the default so a result deserialised from an older
/// report, or built without evidence, can never read as a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CheckState {
    Pass,
    Fail,
    Warning,
    #[default]
    Unknown,
    NotApplicable,
    RequiresHumanApproval,
}

impl CheckState {
    /// Seriousness for combining results: the worst state wins. `NotApplicable` is neutral.
    fn rank(self) -> u8 {
        match self {
            CheckState::NotApplicable => 0,
            CheckState::Pass => 1,
            CheckState::Warning => 2,
            CheckState::RequiresHumanApproval => 3,
            CheckState::Unknown => 4,
            CheckState::Fail => 5,
        }
    }

    /// The worse of two states; `Fail` > `Unknown` > `RequiresHumanApproval` > `Warning` > `Pass`.
    pub fn worst(self, other: CheckState) -> CheckState {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }

    /// Only `Pass` (and a rule that does not apply) certifies; everything else does not.
    pub fn certifies(self) -> bool {
        matches!(self, CheckState::Pass | CheckState::NotApplicable)
    }
}

/// Result of one rule in one report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleResult {
    pub rule_id: String,
    pub state: CheckState,
    /// Why the rule is not `Pass` (missing input, nothing to check, approval needed).
    #[serde(default)]
    pub reason: String,
}

/// Whether a stored result may speak for the project as it is now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Currency {
    Current,
    /// Ran against another revision or other canonical content: shown as stale, never certifies.
    Stale {
        result_revision: u64,
        project_revision: u64,
    },
}

/// Impact if the risk materialises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Likelihood {
    Rare,
    Possible,
    Likely,
}

/// How likely the problem is to be caught before it reaches hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Detectability {
    /// A deterministic check found it.
    Detected,
    /// The check could not run (unknown), so nothing would catch it.
    NotChecked,
}

/// One engineering risk. Axes are kept separate on purpose: there is no combined score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiskRecord {
    pub id: String,
    pub subsystem: String,
    pub severity: Severity,
    pub likelihood: Likelihood,
    pub detectability: Detectability,
    pub consequence: String,
    /// Rule ids and finding ids the record rests on.
    pub evidence: Vec<String>,
    pub mitigation: Option<String>,
    /// High-consequence risks need a recorded human decision before release.
    pub approval_required: bool,
}

/// Subsystem named by a rule id prefix (`DRC-008` → `PCB layout`).
pub fn subsystem_of(rule_id: &str) -> &'static str {
    match rule_id.split('-').next().unwrap_or("") {
        "ERC" => "Schematic electrical",
        "DRC" => "PCB layout",
        "MECH" => "Mechanical fit",
        "DFM" => "Fabrication",
        "SRC" => "Sourcing",
        _ => "Other",
    }
}

#[cfg(test)]
mod tests {
    // Written during the testing freeze; never compiled or run.
    use super::*;

    #[test]
    fn worst_state_wins_and_fail_is_never_softened() {
        assert_eq!(CheckState::Pass.worst(CheckState::Unknown), CheckState::Unknown);
        assert_eq!(CheckState::Fail.worst(CheckState::Unknown), CheckState::Fail);
        assert_eq!(CheckState::NotApplicable.worst(CheckState::Pass), CheckState::Pass);
    }

    #[test]
    fn default_and_certification_are_fail_closed() {
        assert_eq!(CheckState::default(), CheckState::Unknown);
        assert!(!CheckState::Unknown.certifies());
        assert!(!CheckState::RequiresHumanApproval.certifies());
        assert!(CheckState::Pass.certifies());
    }
}
