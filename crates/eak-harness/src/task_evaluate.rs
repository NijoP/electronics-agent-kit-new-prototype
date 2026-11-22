//! Independent evaluation of a design (ADR-0037).
//!
//! The evaluator judges the committed design from EAK-IR and deterministic validation only. Its
//! single input is the `Project`; it cannot read what an agent said it did. Every check is `Pass`,
//! `Fail` or `NotChecked`: anything that could not be evaluated (for example board checks before a
//! board exists) is reported as not checked and prevents a passing verdict, and the checks' own
//! `not_implemented` and `limitations` lists are carried through. A passing verdict means only that
//! the checks EAK implements found no error at this revision. It is never a manufacturing-readiness
//! claim.

use eak_core::{pinmap, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pass,
    Fail,
    NotChecked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalCheck {
    pub id: String,
    pub status: Status,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Every check ran and found no error.
    Pass,
    /// At least one check found an error.
    Fail,
    /// No error found, but at least one check could not be evaluated.
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evaluation {
    pub revision: u64,
    pub verdict: Verdict,
    pub checks: Vec<EvalCheck>,
    pub not_implemented: Vec<String>,
    pub limitations: Vec<String>,
    pub statement: String,
}

fn check(id: &str, status: Status, detail: impl Into<String>) -> EvalCheck {
    EvalCheck {
        id: id.to_string(),
        status,
        detail: detail.into(),
    }
}

fn listed(items: &[String]) -> String {
    const MAX: usize = 5;
    let mut s = items
        .iter()
        .take(MAX)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > MAX {
        s.push_str(&format!(" and {} more", items.len() - MAX));
    }
    s
}

/// Evaluate the design as committed. Pure with respect to the project: nothing is changed.
pub fn evaluate(p: &Project) -> Evaluation {
    let mut checks = Vec::new();
    let mut not_impl: BTreeSet<String> = BTreeSet::new();
    let mut limits: BTreeSet<String> = BTreeSet::new();

    let symbols: Vec<_> = p.schematic.symbols.values().collect();
    if symbols.is_empty() {
        checks.push(check(
            "schematic_present",
            Status::Fail,
            "the schematic has no symbols",
        ));
    } else {
        checks.push(check(
            "schematic_present",
            Status::Pass,
            format!("{} symbol(s)", symbols.len()),
        ));
    }

    let unresolved: Vec<String> = symbols
        .iter()
        .filter(|s| !p.library.contains_key(&s.component_id))
        .map(|s| format!("{} ({})", s.refdes, s.component_id))
        .collect();
    checks.push(if unresolved.is_empty() {
        check(
            "library_resolved",
            Status::Pass,
            "every symbol has a library definition",
        )
    } else {
        check(
            "library_resolved",
            Status::Fail,
            format!("no definition for: {}", listed(&unresolved)),
        )
    });

    let bad_map: Vec<String> = symbols
        .iter()
        .filter_map(|s| p.library.get(&s.component_id).map(|d| (s, d)))
        .filter(|(_, d)| pinmap::check_component(d).blocking())
        .map(|(s, _)| s.refdes.clone())
        .collect();
    checks.push(if bad_map.is_empty() {
        check(
            "footprint_mapping",
            Status::Pass,
            "every pin/pad mapping is usable",
        )
    } else {
        check(
            "footprint_mapping",
            Status::Fail,
            format!("unusable pin/pad mapping: {}", listed(&bad_map)),
        )
    });

    let erc = eak_validation::erc::run(p);
    not_impl.extend(erc.not_implemented.iter().cloned());
    limits.extend(erc.limitations.iter().cloned());
    checks.push(if erc.errors == 0 {
        check(
            "erc",
            Status::Pass,
            format!(
                "no ERC errors in the implemented checks ({} warning(s))",
                erc.warnings
            ),
        )
    } else {
        check("erc", Status::Fail, format!("{} ERC error(s)", erc.errors))
    });

    if p.pcb.footprints.is_empty() {
        for id in ["pcb_synchronised", "routing_complete", "drc", "dfm"] {
            checks.push(check(id, Status::NotChecked, "there is no board yet"));
        }
    } else {
        let missing: Vec<String> = symbols
            .iter()
            .filter(|s| !p.pcb.footprints.values().any(|f| f.symbol_id == s.id))
            .map(|s| s.refdes.clone())
            .collect();
        checks.push(if missing.is_empty() {
            check(
                "pcb_synchronised",
                Status::Pass,
                "every symbol has a footprint",
            )
        } else {
            check(
                "pcb_synchronised",
                Status::Fail,
                format!("no footprint for: {}", listed(&missing)),
            )
        });

        let c = eak_engineering::copper::analyse(p);
        checks.push(if c.ratsnest.is_empty() && c.shorts.is_empty() {
            check(
                "routing_complete",
                Status::Pass,
                "no unrouted connections and no shorts",
            )
        } else {
            check(
                "routing_complete",
                Status::Fail,
                format!(
                    "{} unrouted connection(s), {} short(s)",
                    c.ratsnest.len(),
                    c.shorts.len()
                ),
            )
        });

        let drc = eak_validation::drc::run(p);
        not_impl.extend(drc.not_implemented.iter().cloned());
        limits.extend(drc.limitations.iter().cloned());
        checks.push(if drc.errors == 0 {
            check(
                "drc",
                Status::Pass,
                format!(
                    "no DRC errors in the implemented checks ({} warning(s))",
                    drc.warnings
                ),
            )
        } else {
            check("drc", Status::Fail, format!("{} DRC error(s)", drc.errors))
        });

        let dfm = eak_validation::dfm::run(p, &eak_validation::dfm::fabricator_standard());
        not_impl.extend(dfm.not_implemented.iter().cloned());
        limits.extend(dfm.limitations.iter().cloned());
        checks.push(if dfm.errors == 0 {
            check(
                "dfm",
                Status::Pass,
                format!(
                    "no DFM errors against the standard process profile ({} warning(s))",
                    dfm.warnings
                ),
            )
        } else {
            check("dfm", Status::Fail, format!("{} DFM error(s)", dfm.errors))
        });
    }

    let verdict = if checks.iter().any(|c| c.status == Status::Fail) {
        Verdict::Fail
    } else if checks.iter().any(|c| c.status == Status::NotChecked) {
        Verdict::Incomplete
    } else {
        Verdict::Pass
    };
    Evaluation {
        revision: p.revision,
        verdict,
        checks,
        not_implemented: not_impl.into_iter().collect(),
        limitations: limits.into_iter().collect(),
        statement: format!(
            "Evaluation of the recorded design at revision {}: only the listed checks were run, \
             anything under not_implemented or limitations was not evaluated, and this is not a \
             manufacturing-readiness claim.",
            p.revision
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_runtime::{fixtures, Engine};

    fn status(e: &Evaluation, id: &str) -> Status {
        e.checks
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.status)
            .unwrap()
    }

    #[test]
    fn an_empty_project_fails_and_board_checks_are_not_checked() {
        let e = evaluate(&Project::new("empty"));
        assert_eq!(e.verdict, Verdict::Fail);
        assert_eq!(status(&e, "schematic_present"), Status::Fail);
        for id in ["pcb_synchronised", "routing_complete", "drc", "dfm"] {
            assert_eq!(status(&e, id), Status::NotChecked, "{id}");
        }
        assert!(e.statement.contains("not a manufacturing-readiness claim"));
    }

    #[test]
    fn a_schematic_without_a_board_can_never_pass() {
        let mut eng = Engine::in_memory(Project::new("s"));
        fixtures::build_schematic(&mut eng).unwrap();
        let e = evaluate(eng.project());
        assert_eq!(status(&e, "schematic_present"), Status::Pass);
        assert_eq!(status(&e, "library_resolved"), Status::Pass);
        assert_eq!(status(&e, "routing_complete"), Status::NotChecked);
        assert_ne!(e.verdict, Verdict::Pass, "no board means incomplete");
        assert_eq!(e.revision, eng.project().revision);
    }

    #[test]
    fn evaluation_does_not_change_the_design_and_is_repeatable() {
        let mut eng = Engine::in_memory(Project::new("s"));
        fixtures::build_schematic(&mut eng).unwrap();
        let before = eng.project().revision;
        let a = evaluate(eng.project());
        let b = evaluate(eng.project());
        assert_eq!(a, b);
        assert_eq!(eng.project().revision, before);
    }

    #[test]
    fn a_missing_library_definition_is_a_failure_not_a_pass() {
        let mut eng = Engine::in_memory(Project::new("s"));
        fixtures::build_schematic(&mut eng).unwrap();
        let mut p = eng.project().clone();
        let any = p
            .schematic
            .symbols
            .values()
            .next()
            .unwrap()
            .component_id
            .clone();
        p.library.remove(&any);
        let e = evaluate(&p);
        assert_eq!(status(&e, "library_resolved"), Status::Fail);
        assert_eq!(e.verdict, Verdict::Fail);
    }
}
