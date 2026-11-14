//! The engineering lifecycle (directive §7) as a typed phase state machine.
//!
//! Ideas taken (not code) from the earlier prototype's `eak-phases`: a phase is entered only
//! when the previous one exited; exit criteria are evaluated, never asserted; a blocking
//! violation or an undischarged critical assumption holds every later phase; only a human
//! waives or approves. Each criterion reads real state: the compiled requirement/constraint
//! model, the EAK-IR, and the fail-closed gate report of `eak-validation::gates` (ADR-0021).
//! A criterion with no evidence is `Unknown`, and `Unknown` never lets a phase exit.

use super::compile::EngineeringModel;
use super::model::{CalcState, Knowledge, Priority};
use super::profile::Level;
use eak_core::Project;
use eak_validation::gates::{GateReport, GateState};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    ProductUnderstanding,
    RequirementsEngineering,
    EngineeringArchitecture,
    ComponentEngineering,
    SchematicEngineering,
    PreLayoutReview,
    PcbArchitecture,
    ComponentPlacement,
    PcbRouting,
    CopperAndPower,
    EngineeringVerification,
    ManufacturingEngineering,
    ReleaseReview,
}

pub const STAGES: [Stage; 13] = [
    Stage::ProductUnderstanding,
    Stage::RequirementsEngineering,
    Stage::EngineeringArchitecture,
    Stage::ComponentEngineering,
    Stage::SchematicEngineering,
    Stage::PreLayoutReview,
    Stage::PcbArchitecture,
    Stage::ComponentPlacement,
    Stage::PcbRouting,
    Stage::CopperAndPower,
    Stage::EngineeringVerification,
    Stage::ManufacturingEngineering,
    Stage::ReleaseReview,
];

/// Criterion states are the shared check states of ADR-0026; only `Pass` and `NotApplicable`
/// count towards an exit.
pub use eak_validation::CheckState;

#[derive(Debug, Clone, Serialize)]
pub struct Criterion {
    pub id: String,
    pub description: String,
    pub state: CheckState,
    /// What the state rests on (counts, ids, gate id), or why it is unknown.
    pub evidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseState {
    /// Every exit criterion met (or not applicable).
    ExitMet,
    /// At least one criterion not met.
    Blocked,
    /// Nothing failed, but at least one criterion does not certify (unknown, warning,
    /// requires human approval).
    Unverified,
}

#[derive(Debug, Clone, Serialize)]
pub struct Phase {
    pub stage: Stage,
    /// Entry criterion: every earlier phase is `ExitMet`.
    pub entered: bool,
    pub state: PhaseState,
    pub exit_criteria: Vec<Criterion>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Lifecycle {
    pub revision: u64,
    /// First phase that has not exited; `None` only when all thirteen did.
    pub current: Option<Stage>,
    pub phases: Vec<Phase>,
    /// Whether a gate report was supplied and computed for this exact revision and input.
    pub gates_current: bool,
    pub limitations: Vec<&'static str>,
}

pub const LIMITATIONS: &[&str] = &[
    "Release review never exits automatically: human approval is recorded by `eak design approve` and is not evaluated here.",
    "Gate criteria are Unknown unless `run_gates` ran at this exact revision.",
    "Stage applicability is not yet narrowed per domain; every stage is evaluated for every product.",
];

fn c(id: &str, description: &str, state: CheckState, evidence: String) -> Criterion {
    Criterion {
        id: id.into(),
        description: description.into(),
        state,
        evidence,
    }
}

fn met(ok: bool) -> CheckState {
    if ok {
        CheckState::Pass
    } else {
        CheckState::Fail
    }
}

fn gate(gates: Option<&GateReport>, id: &str, description: &str) -> Criterion {
    let cid = format!("gate.{id}");
    let Some(r) = gates else {
        return c(
            &cid,
            description,
            CheckState::Unknown,
            "no current gate report (run_gates at this revision)".into(),
        );
    };
    match r.gates.iter().find(|g| g.id == id) {
        None => c(
            &cid,
            description,
            CheckState::Unknown,
            format!("gate {id} is not in the report"),
        ),
        Some(g) => c(
            &cid,
            description,
            match g.state {
                GateState::Pass => CheckState::Pass,
                GateState::Fail => CheckState::Fail,
                GateState::Unknown | GateState::NotRun => CheckState::Unknown,
            },
            format!("gate {id}: {:?}; {}", g.state, g.reasons.join("; ")),
        ),
    }
}

fn criteria(
    stage: Stage,
    m: &EngineeringModel,
    p: &Project,
    g: Option<&GateReport>,
) -> Vec<Criterion> {
    use CheckState::{NotApplicable, Unknown};
    match stage {
        Stage::ProductUnderstanding => vec![
            c(
                "classified",
                "product domains identified",
                met(!m.domains.is_empty()),
                format!("{} engineering domains", m.domains.len()),
            ),
            c(
                "spec_present",
                "structured requirement spec stored",
                met(!m.missing_information.iter().any(|x| x.id == "missing.spec")),
                "design_spec_set".into(),
            ),
        ],
        Stage::RequirementsEngineering => {
            let missing = m.missing_information.len();
            let unknown = m
                .requirements
                .iter()
                .filter(|r| !r.value.is_known())
                .count();
            let must = || {
                m.requirements
                    .iter()
                    .filter(|r| r.priority == Priority::Must)
            };
            let unchecked = must().filter(|r| r.validation_method.is_none()).count();
            let assumed = must().filter(|r| r.knowledge <= Knowledge::Assumed).count();
            vec![
                c(
                    "no_missing_information",
                    "no required input is missing",
                    met(missing == 0),
                    format!("{missing} missing items"),
                ),
                c(
                    "no_unknown_values",
                    "every requirement has a value",
                    met(unknown == 0),
                    format!("{unknown} unknown"),
                ),
                c(
                    "must_have_validation",
                    "every mandatory requirement names a validation method",
                    met(unchecked == 0),
                    format!("{unchecked} without a method"),
                ),
                c(
                    "critical_assumptions_discharged",
                    "no mandatory requirement rests on an assumption",
                    met(assumed == 0),
                    format!("{assumed} assumed (only a human confirms)"),
                ),
            ]
        }
        Stage::EngineeringArchitecture => {
            let blocks: Vec<_> = m
                .product
                .nodes
                .iter()
                .filter(|n| n.level == Level::FunctionalBlock)
                .collect();
            let untagged = blocks.iter().filter(|n| n.domains.is_empty()).count();
            vec![
                c(
                    "blocks_defined",
                    "functional blocks identified",
                    met(!blocks.is_empty()),
                    format!("{} blocks", blocks.len()),
                ),
                c(
                    "blocks_classified",
                    "every block has an engineering domain",
                    met(untagged == 0),
                    format!("{untagged} unclassified"),
                ),
            ]
        }
        Stage::ComponentEngineering => {
            let syms = &p.schematic.symbols;
            let no_mpn = syms
                .values()
                .filter(|s| {
                    !p.library
                        .get(&s.component_id)
                        .and_then(|d| d.mpn.as_deref())
                        .is_some_and(|m| !m.trim().is_empty())
                })
                .count();
            vec![
                c(
                    "parts_present",
                    "components placed in the schematic",
                    met(!syms.is_empty()),
                    format!("{} symbols", syms.len()),
                ),
                c(
                    "parts_resolved",
                    "every part has a manufacturer part number",
                    met(!syms.is_empty() && no_mpn == 0),
                    format!("{no_mpn} without MPN"),
                ),
            ]
        }
        Stage::SchematicEngineering => vec![
            gate(
                g,
                "net_connectivity",
                "schematic and board connectivity agree",
            ),
            c(
                "no_conflicts",
                "no conflicting requirement constraints",
                met(m.conflicts.is_empty()),
                format!("{} conflicts", m.conflicts.len()),
            ),
        ],
        Stage::PreLayoutReview => {
            let not_computed = m
                .calculations
                .iter()
                .filter(|k| k.state == CalcState::NotComputed)
                .count();
            vec![
                c(
                    "no_precedence_violations",
                    "no protected limit weakened",
                    met(m.precedence_violations.is_empty()),
                    format!("{} violations", m.precedence_violations.len()),
                ),
                c(
                    "calculations_computed",
                    "every engineering calculation computed",
                    met(not_computed == 0),
                    format!("{not_computed} not computed"),
                ),
                c(
                    "interactions_reviewed",
                    "cross-domain interactions reviewed by a human",
                    if m.interactions.is_empty() {
                        NotApplicable
                    } else {
                        Unknown
                    },
                    format!(
                        "{} active interaction rules; review is not recorded by EAK",
                        m.interactions.len()
                    ),
                ),
            ]
        }
        Stage::PcbArchitecture => vec![
            gate(g, "board_outline", "board outline closed"),
            gate(g, "stackup", "stackup defined"),
        ],
        Stage::ComponentPlacement => {
            let (f, s) = (p.pcb.footprints.len(), p.schematic.symbols.len());
            vec![c(
                "footprints_placed",
                "every schematic part has a footprint on the board",
                met(f > 0 && f >= s),
                format!("{f} footprints for {s} symbols"),
            )]
        }
        Stage::PcbRouting => vec![
            gate(
                g,
                "required_pads_connected",
                "every required connection routed",
            ),
            gate(g, "no_shorts", "no shorts"),
        ],
        Stage::CopperAndPower => vec![
            gate(g, "copper_continuity", "copper continuity"),
            gate(g, "electrical_constraints", "electrical constraints"),
            gate(g, "return_path_continuity", "return paths continuous"),
        ],
        Stage::EngineeringVerification => vec![
            gate(g, "clearances", "clearances"),
            gate(g, "annular_ring", "annular rings"),
        ],
        Stage::ManufacturingEngineering => vec![
            gate(
                g,
                "fabricator_capability",
                "within the fabricator's capability",
            ),
            c(
                "required_gates_pass",
                "every required gate passes",
                g.map_or(Unknown, |r| met(r.required_passed)),
                g.map_or_else(
                    || "no current gate report".into(),
                    |r| r.blocking.join("; "),
                ),
            ),
        ],
        Stage::ReleaseReview => vec![c(
            "human_approval",
            "a responsible human approved this revision",
            Unknown,
            format!(
                "{} risks need human approval; approvals are not evaluated here",
                m.risks
                    .iter()
                    .filter(|r| r.record.approval_required)
                    .count()
            ),
        )],
    }
}

/// Evaluate the lifecycle. `gates` is used only if computed for this exact revision and input.
pub fn evaluate(m: &EngineeringModel, p: &Project, gates: Option<&GateReport>) -> Lifecycle {
    let gates_current = gates.is_some_and(|r| r.is_current_for(p));
    let g = gates.filter(|_| gates_current);
    let mut phases: Vec<Phase> = vec![];
    let mut entered = true;
    for stage in STAGES {
        let exit_criteria = criteria(stage, m, p, g);
        let state = if exit_criteria.iter().any(|x| x.state == CheckState::Fail) {
            PhaseState::Blocked
        } else if exit_criteria.iter().any(|x| !x.state.certifies()) {
            PhaseState::Unverified
        } else {
            PhaseState::ExitMet
        };
        phases.push(Phase {
            stage,
            entered,
            state,
            exit_criteria,
        });
        entered = entered && state == PhaseState::ExitMet;
    }
    Lifecycle {
        revision: p.revision,
        current: phases
            .iter()
            .find(|x| x.state != PhaseState::ExitMet)
            .map(|x| x.stage),
        phases,
        gates_current,
        limitations: LIMITATIONS.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::compile::{compile, CompileInput};

    // Written, not run (code-reviewed only).
    #[test]
    fn empty_project_stops_at_product_understanding_and_never_releases() {
        let p = Project::new("empty");
        let m = compile(&p, CompileInput::default());
        let l = evaluate(&m, &p, None);
        assert_eq!(l.phases.len(), 13);
        assert_eq!(l.current, Some(Stage::ProductUnderstanding));
        assert!(!l.gates_current);
        assert!(!l.phases[1].entered);
        let release = l.phases.last().unwrap();
        assert_ne!(release.state, PhaseState::ExitMet);
        // Without a gate report every gate criterion is unknown, never met.
        assert!(l.phases[6]
            .exit_criteria
            .iter()
            .all(|c| c.state == CheckState::Unknown));
    }
}
