//! Fail-closed engineering gates (integration directive §3G).
//!
//! A gate is `PASS` only when executed evidence for this exact revision shows no violation.
//! A deterministic violation is `FAIL`; missing inputs or evidence are `UNKNOWN`; a gate whose
//! check was not executed is `NOT_RUN`. Only `PASS` counts as passing, and a stored gate report
//! for an older revision or input hash is stale ([`GateReport::is_current_for`]). AI
//! confidence has no input here: gates read the EAK-IR and the deterministic checks only.
//!
//! Knowledge source: the hard-gate lifecycle of DESK `desk/quality.py` (commit
//! 5ad8492c37a5dac919d97f4eccd8250780490e7a, MIT), re-implemented over EAK's own DRC, DFM and
//! copper engines. DESK's weighted quality score is deliberately not carried over: a score
//! can never override a gate, and EAK reports the gates alone.

use crate::dfm::{self, FabProfile};
use crate::drc;
use crate::report::{project_hash, Finding, Report, Severity};
use eak_core::{PinType, Project};
use eak_engineering::{copper, netlist, power};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GateState {
    Pass,
    Fail,
    Unknown,
    NotRun,
}

/// What a gate's state rests on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    /// `DRC`, `DFM`, `STACKUP`, `NETLIST`, `POWER` …
    pub source: String,
    pub rule_ids: Vec<String>,
    /// Ids of the findings that decided the gate (empty when it passed).
    pub finding_ids: Vec<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gate {
    pub id: String,
    pub title: String,
    pub state: GateState,
    /// Required gates block release labels when not `PASS`; advisory gates are reported only.
    pub required: bool,
    pub reasons: Vec<String>,
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GateReport {
    pub project_id: String,
    pub revision: u64,
    pub input_sha256: String,
    pub tool: String,
    /// Fabrication profile the capability gate used, if any.
    pub fab_profile: Option<String>,
    pub gates: Vec<Gate>,
    /// True only when every required gate is `PASS`.
    pub required_passed: bool,
    /// One line per required gate that is not `PASS`.
    pub blocking: Vec<String>,
    pub statement: String,
}

impl GateReport {
    /// A gate report only speaks for the revision and canonical input it was computed from.
    pub fn is_current_for(&self, p: &Project) -> bool {
        self.project_id == p.id
            && self.revision == p.revision
            && self.input_sha256 == project_hash(p)
    }
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}

/// Gate over the findings `hits` selected from report `r` (`FAIL` when any).
fn gate_from(
    id: &str,
    title: &str,
    r: &Report,
    rule_ids: Vec<String>,
    hits: Vec<&Finding>,
    required: bool,
) -> Gate {
    let mut reasons: Vec<String> = hits
        .iter()
        .take(20)
        .map(|f| format!("{}: {}", f.rule_id, f.message))
        .collect();
    if hits.len() > 20 {
        reasons.push(format!("… and {} more", hits.len() - 20));
    }
    Gate {
        id: id.into(),
        title: title.into(),
        state: if hits.is_empty() {
            GateState::Pass
        } else {
            GateState::Fail
        },
        required,
        reasons,
        evidence: vec![Evidence {
            source: r.check.clone(),
            rule_ids,
            finding_ids: hits.iter().map(|f| f.id.clone()).collect(),
            detail: format!(
                "{} report, revision {}, input {}",
                r.check,
                r.revision,
                short(&r.input_sha256)
            ),
        }],
    }
}

/// Gate over report findings: `FAIL` when any finding of `rules` has at least the given severity.
fn from_findings(
    id: &str,
    title: &str,
    r: &Report,
    rules: &[(&str, Severity)],
    required: bool,
) -> Gate {
    let hits: Vec<&Finding> = r
        .findings
        .iter()
        .filter(|f| {
            rules
                .iter()
                .any(|(rule, min)| f.rule_id == *rule && f.severity >= *min)
        })
        .collect();
    let rule_ids = rules.iter().map(|(rule, _)| rule.to_string()).collect();
    gate_from(id, title, r, rule_ids, hits, required)
}

/// `UNKNOWN` unless the gate already failed: a deterministic violation outranks missing data.
fn unknown_unless_failed(g: &mut Gate, reason: impl Into<String>) {
    if g.state != GateState::Fail {
        g.state = GateState::Unknown;
    }
    g.reasons.push(reason.into());
}

/// Non-reference nets that carry power by the schematic (a power-port label or a power pin).
fn power_nets(p: &Project) -> Vec<String> {
    netlist::build(p)
        .nets
        .values()
        .filter(|n| !power::is_reference_net(&n.name))
        .filter(|n| {
            n.has_power_port
                || n.pins
                    .iter()
                    .any(|r| matches!(r.electrical_type, PinType::PowerIn | PinType::PowerOut))
        })
        .map(|n| n.name.clone())
        .collect()
}

const FAB_TITLE: &str = "Board fits the fabricator's published capabilities";

fn fabricator_gate(p: &Project, fab: Option<&FabProfile>) -> Gate {
    let Some(fab) = fab else {
        return Gate {
            id: "fabricator_capability".into(),
            title: FAB_TITLE.into(),
            state: GateState::Unknown,
            required: true,
            reasons: vec!["no fabrication profile selected".into()],
            evidence: vec![],
        };
    };
    let r = dfm::run(p, fab);
    let hits: Vec<&Finding> = r
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .collect();
    let rule_ids = r.rules.iter().map(|x| x.id.clone()).collect();
    let mut g = gate_from("fabricator_capability", FAB_TITLE, &r, rule_ids, hits, true);
    if let Some(e) = g.evidence.first_mut() {
        e.detail = format!(
            "{}; profile {} ({}), source {}",
            e.detail, fab.id, fab.name, fab.source_url
        );
    }
    if !fab.verified {
        unknown_unless_failed(
            &mut g,
            format!("profile {} is an unverified transcription of the fabricator's published capabilities", fab.id),
        );
    }
    g
}

/// A gate any of whose rules the project disables is `NOT_RUN`, never `PASS` (ADR-0045, AD-8);
/// a gate with an overridden rule says that it used the default severity.
fn apply_rule_config(p: &Project, gates: &mut [Gate]) {
    let cfg = &p.constraints.rules;
    for g in gates {
        let ids: Vec<String> = g
            .evidence
            .iter()
            .flat_map(|e| e.rule_ids.iter().cloned())
            .collect();
        let disabled: Vec<&str> = ids
            .iter()
            .filter(|id| cfg.get(*id).is_some_and(|c| !c.enabled))
            .map(String::as_str)
            .collect();
        let overridden: Vec<&str> = ids
            .iter()
            .filter(|id| {
                cfg.get(*id)
                    .is_some_and(|c| c.enabled && c.severity.is_some())
            })
            .map(String::as_str)
            .collect();
        if !disabled.is_empty() {
            g.state = GateState::NotRun;
            g.reasons.insert(
                0,
                format!(
                    "{} disabled in the project's rule configuration: gate not run",
                    disabled.join(", ")
                ),
            );
        }
        if !overridden.is_empty() {
            g.reasons.push(format!(
                "severity of {} overridden in the project; this gate used the default severity",
                overridden.join(", ")
            ));
        }
    }
}

/// Evaluate every gate for the current revision. `fab` is the fabricator capability profile
/// the board is meant for; without one the capability gate is `UNKNOWN`.
pub fn evaluate(p: &Project, fab: Option<&FabProfile>) -> GateReport {
    use Severity::{Error, Warning};
    // Gates run every rule at its default severity (ADR-0045): a severity override in the
    // project changes how findings are shown, never whether a gate passes.
    let at_defaults = {
        let mut d = p.clone();
        d.constraints.rules.clear();
        d
    };
    let drc = drc::run(&at_defaults);
    let has_copper = !copper::items(p).is_empty();
    let has_footprints = !p.pcb.footprints.is_empty();
    let mut gates = vec![];

    let mut g = from_findings(
        "no_shorts",
        "No short circuits between nets",
        &drc,
        &[("DRC-003", Error)],
        true,
    );
    if !has_copper {
        unknown_unless_failed(&mut g, "no copper on the board: nothing to evaluate");
    }
    gates.push(g);

    let mut g = from_findings(
        "required_pads_connected",
        "Every required pad connected (no unrouted connections, no missing footprints)",
        &drc,
        &[("DRC-004", Error), ("DRC-011", Error)],
        true,
    );
    if !has_footprints {
        unknown_unless_failed(
            &mut g,
            "no footprints placed: pad connectivity not evaluated",
        );
    }
    gates.push(g);

    let mut g = from_findings(
        "net_connectivity",
        "Board connectivity matches the schematic; no dangling copper",
        &drc,
        &[("DRC-010", Error), ("DRC-012", Warning)],
        true,
    );
    if !has_footprints {
        unknown_unless_failed(
            &mut g,
            "no footprints placed: schematic/PCB consistency not evaluated",
        );
    }
    gates.push(g);

    let mut g = from_findings(
        "clearances",
        "Track, pad and via clearances (net class, declared voltages, holes, board edge)",
        &drc,
        &[("DRC-002", Error), ("DRC-018", Error), ("DRC-005", Error)],
        true,
    );
    if !has_copper {
        unknown_unless_failed(&mut g, "no copper on the board: nothing to evaluate");
    }
    // Net-class clearance applies to every net; the voltage part only to declared nets.
    g.reasons.extend(drc::undeclared_nets_not_evaluated(p));
    gates.push(g);

    gates.push(from_findings(
        "annular_ring",
        "Drill sizes, annular rings and hole spacing",
        &drc,
        &[("DRC-006", Error), ("DRC-019", Error)],
        true,
    ));

    gates.push(from_findings(
        "board_outline",
        "Closed, simple board outline",
        &drc,
        &[("DRC-009", Error)],
        true,
    ));

    let mut g = from_findings(
        "stackup",
        "Valid layer stackup and layer assignments",
        &drc,
        &[("DRC-016", Warning), ("DRC-017", Warning)],
        true,
    );
    let st_errors = p.pcb.board.stackup.validate();
    if !st_errors.is_empty() {
        g.state = GateState::Fail;
        g.reasons
            .extend(st_errors.iter().map(|e| format!("stackup: {e}")));
    }
    g.evidence.push(Evidence {
        source: "STACKUP".into(),
        rule_ids: vec!["Stackup::validate".into()],
        finding_ids: vec![],
        detail: format!("{} validation error(s)", st_errors.len()),
    });
    gates.push(g);

    gates.push(from_findings(
        "copper_continuity",
        "Copper zones: no isolated islands, thermal-relief spokes connect",
        &drc,
        &[("DRC-013", Warning), ("DRC-014", Warning)],
        true,
    ));

    gates.push(fabricator_gate(&at_defaults, fab));

    // Electrical constraints: current capacity where declared; power nets without a declared
    // current or voltage are unknown, never passed.
    let mut g = from_findings(
        "electrical_constraints",
        "Declared electrical constraints met (track and relief current capacity)",
        &drc,
        &[("DRC-008", Error), ("DRC-014", Error)],
        true,
    );
    let c = &p.constraints;
    let pn = power_nets(p);
    let no_current: Vec<&str> = pn
        .iter()
        .filter(|n| !c.net_currents_a.contains_key(*n))
        .map(String::as_str)
        .collect();
    let no_voltage: Vec<&str> = pn
        .iter()
        .filter(|n| !c.net_voltages_v.contains_key(*n))
        .map(String::as_str)
        .collect();
    if !no_current.is_empty() {
        unknown_unless_failed(
            &mut g,
            format!(
                "power nets without a declared current: {}",
                no_current.join(", ")
            ),
        );
    }
    if !no_voltage.is_empty() {
        unknown_unless_failed(
            &mut g,
            format!(
                "power nets without a declared voltage: {}",
                no_voltage.join(", ")
            ),
        );
    }
    let analysis = power::analyse(p);
    if !analysis.unrouted_declared_currents.is_empty() {
        let nets: Vec<&str> = analysis
            .unrouted_declared_currents
            .keys()
            .map(String::as_str)
            .collect();
        unknown_unless_failed(
            &mut g,
            format!("declared currents with no copper yet: {}", nets.join(", ")),
        );
    }
    g.evidence.push(Evidence {
        source: "NETLIST".into(),
        rule_ids: vec!["power nets: power-port label or power-in/out pin".into()],
        finding_ids: vec![],
        detail: format!("{} power net(s): {}", pn.len(), pn.join(", ")),
    });
    gates.push(g);

    // Return-path continuity: advisory (a signal-integrity concern, not a fabrication fault),
    // reported with the same fail-closed states.
    let rp = &analysis.return_path;
    let mut g = Gate {
        id: "return_path_continuity".into(),
        title:
            "Signal tracks have continuous adjacent reference copper; ground planes are not split"
                .into(),
        state: GateState::Pass,
        required: false,
        reasons: vec![],
        evidence: vec![Evidence {
            source: "POWER".into(),
            rule_ids: vec!["eak_engineering::power::return_path".into()],
            finding_ids: vec![],
            detail: format!(
                "reference nets {:?}; reported gaps ≥ {} mm",
                rp.reference_nets, rp.min_gap_mm
            ),
        }],
    };
    for s in &rp.plane_splits {
        g.state = GateState::Fail;
        g.reasons.push(format!(
            "zone {} ({} on {}) is split into {} islands",
            s.zone_id,
            s.net,
            s.layer.name(),
            s.islands
        ));
    }
    for gap in &rp.gaps {
        g.state = GateState::Fail;
        g.reasons.push(format!(
            "track {} ({}) crosses a {:.2} mm gap in {} on {}",
            gap.track_id,
            gap.net,
            gap.gap_length_mm,
            gap.reference_net,
            gap.reference_layer.name()
        ));
    }
    if let power::Status::Unknown(why) = &rp.status {
        unknown_unless_failed(&mut g, why.clone());
    }
    g.reasons
        .extend(rp.limitations.iter().map(|l| format!("limitation: {l}")));
    gates.push(g);
    apply_rule_config(p, &mut gates);

    let blocking: Vec<String> = gates
        .iter()
        .filter(|g| g.required && g.state != GateState::Pass)
        .map(|g| {
            format!(
                "{} is {:?}: {}",
                g.id,
                g.state,
                g.reasons.first().cloned().unwrap_or_default()
            )
        })
        .collect();
    GateReport {
        project_id: p.id.clone(),
        revision: p.revision,
        input_sha256: project_hash(p),
        tool: format!("eak-validation {} gates", env!("CARGO_PKG_VERSION")),
        fab_profile: fab.map(|f| f.id.clone()),
        required_passed: blocking.is_empty(),
        blocking,
        gates,
        statement: "Gates are necessary, not sufficient: all required gates passing means the implemented checks found no violation in this revision. It is not a manufacturing-ready claim; independent engineering review and fabricator DFM remain required.".into(),
    }
}

#[cfg(test)]
mod tests {
    // Written during the testing freeze; never compiled or run.
    use super::*;

    fn state(r: &GateReport, id: &str) -> GateState {
        r.gates.iter().find(|g| g.id == id).unwrap().state
    }

    #[test]
    fn empty_project_never_passes() {
        let p = Project::new("empty");
        let r = evaluate(&p, None);
        assert!(!r.required_passed);
        assert_eq!(state(&r, "no_shorts"), GateState::Unknown);
        assert_eq!(state(&r, "required_pads_connected"), GateState::Unknown);
        assert_eq!(state(&r, "fabricator_capability"), GateState::Unknown);
        assert_eq!(state(&r, "board_outline"), GateState::Fail);
        assert_eq!(state(&r, "return_path_continuity"), GateState::Unknown);
        assert!(r.is_current_for(&p));
    }

    #[test]
    fn report_goes_stale_on_new_revision() {
        let mut p = Project::new("x");
        let r = evaluate(&p, None);
        p.revision += 1;
        assert!(!r.is_current_for(&p));
    }

    #[test]
    fn unverified_fab_profile_is_never_pass() {
        let p = Project::new("x");
        let fab = dfm::fabricator_standard();
        let r = evaluate(&p, Some(&fab));
        if !fab.verified {
            assert_ne!(state(&r, "fabricator_capability"), GateState::Pass);
        }
    }
}
