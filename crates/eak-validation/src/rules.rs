//! The versioned engineering rule registry and domain-overlay composition (M4.4, ADR-0061).
//!
//! * The **registry** lists every rule the checks declare, built from the checks' own rule
//!   tables (so it cannot drift from them): id, version, check, evaluator, description, source,
//!   the state of that source, the gates that read the rule, the domain classes that require it,
//!   and a fingerprint of its definition.
//! * The **composition** reads the project's domain classification (`domain.*` requirement
//!   entries, ADR-0060) and adds the rules each active class requires. Composition only ever
//!   adds: a class that applies cannot switch a rule off, and a project configuration that
//!   disables a required rule, or shows it below error, is reported as a conflict, never applied
//!   silently. Concerns of an active class that no evaluator covers are listed, never passed.
//!
//! The overlay table below maps classes to rules and to uncovered concerns. It carries no
//! engineering values: the values stay in the rules and in the project's declared constraints.

use crate::gates::{self, unknown_unless_failed, Evidence, Gate, GateState};
use crate::report::RuleInfo;
use crate::{dfm, drc, erc, mech, sourcing};
use eak_core::requirements::{self as rq, DomainKind, Level, DOMAIN_CLASSES, DOMAIN_PREFIX};
use eak_core::{Project, ReqStatus, ReqValue, RuleSeverity};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// Version of the registry's structure and overlay table. Bump it when either changes meaning.
pub const REGISTRY_VERSION: u32 = 1;

/// Rules whose evaluated meaning changed since they were first registered, with their current
/// version. A rule not listed is at version 1. Change a rule's meaning, bump it here.
const RULE_VERSIONS: &[(&str, u32)] = &[];

/// What a rule's stated source is. There is deliberately no "verified" state: no rule table in
/// this build has been checked by a person against the text of the standard it cites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceState {
    /// EAK's own structural definition, or a limit the project declares. No outside claim.
    EakDefinition,
    /// Cites a standard, a datasheet formula, a fabricator or common practice; the values are
    /// not verified against that source.
    CitedNotVerified,
}

fn source_state(source: &str) -> SourceState {
    let own = source == "EAK"
        || source.starts_with("EAK (ADR-")
        || source.starts_with("Project constraints")
        || source.starts_with("Project enclosure declaration");
    if own {
        SourceState::EakDefinition
    } else {
        SourceState::CitedNotVerified
    }
}

/// One registered rule.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegisteredRule {
    pub id: String,
    pub version: u32,
    /// The report that carries it (`ERC`, `DRC`, `DFM`, `MECH`, `SRC`).
    pub check: String,
    /// The function that evaluates it.
    pub evaluator: String,
    pub description: String,
    pub source: String,
    pub source_state: SourceState,
    /// Gates whose state the rule decides.
    pub gates: Vec<String>,
    /// Domain classes that require it; empty means a base rule, run for every board.
    pub required_by: Vec<String>,
    /// SHA-256 (first 16 hex digits) of id, version, check, description and source: a changed
    /// definition is visible even where the version was not bumped.
    pub fingerprint: String,
}

/// What one domain class requires.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct OverlayRules {
    /// A [`DOMAIN_CLASSES`] id.
    pub class: &'static str,
    /// Rules that must stay enabled, at error, while the class applies.
    pub requires: &'static [&'static str],
    /// Concerns of the class that no evaluator in this build covers.
    pub not_covered: &'static [&'static str],
}

/// The overlay table: one entry per domain class, in [`DOMAIN_CLASSES`] order.
#[rustfmt::skip]
pub const OVERLAY_RULES: &[OverlayRules] = &[
    OverlayRules { class: "embedded_mcu",
        requires: &["ERC-001", "ERC-003", "ERC-010"],
        not_covered: &["reset, clock and programming/debug access of the controller"] },
    OverlayRules { class: "dcdc_power",
        requires: &["DRC-008", "DRC-028", "ERC-022", "ERC-023", "ERC-025"],
        not_covered: &["switching-loop area and hot-loop layout", "inductor saturation and ripple current against the converter design"] },
    OverlayRules { class: "battery_bms",
        requires: &["ERC-022", "ERC-023"],
        not_covered: &["cell protection (over-charge, over-discharge, short circuit)", "charging profile and thermal cut-off", "fusing of the battery path"] },
    OverlayRules { class: "motor_control",
        requires: &["DRC-008", "DRC-027", "ERC-022", "ERC-023", "ERC-025"],
        not_covered: &["flyback and back-EMF paths", "current-sense placement and Kelvin routing"] },
    OverlayRules { class: "ac_mains_hv",
        requires: &["DRC-002", "DRC-005", "DRC-018", "ERC-011", "ERC-022"],
        not_covered: &["creepage along surfaces (only through-air clearance is checked)", "safety-standard spacing tables for mains (DRC-002 uses an unverified IPC-2221 table)", "input fusing and protective earth"] },
    OverlayRules { class: "rf_wireless",
        requires: &["DRC-021"],
        not_covered: &["antenna keep-out region declared from the module or antenna datasheet", "RF matching network and feed-line impedance", "electromagnetic analysis of the antenna region"] },
    OverlayRules { class: "analog_sensors",
        requires: &["ERC-010"],
        not_covered: &["analog and digital ground partitioning", "guarding and leakage of high-impedance nodes", "reference and ADC input filtering"] },
    OverlayRules { class: "high_voltage",
        requires: &["DRC-002", "DRC-018", "ERC-011", "ERC-022"],
        not_covered: &["creepage along surfaces (only through-air clearance is checked)", "insulation coordination against the working voltage and pollution degree"] },
    OverlayRules { class: "galvanic_isolation",
        requires: &["DRC-002", "DRC-021"],
        not_covered: &["isolation-barrier spacing and slots under the isolator", "isolator and transformer rating against the working and test voltage"] },
    OverlayRules { class: "high_current",
        requires: &["DRC-008", "DRC-014", "DRC-027", "DRC-028", "ERC-023"],
        not_covered: &["connector and terminal current rating", "copper heating of zones (DRC-008 sizes tracks only)"] },
    OverlayRules { class: "high_speed_digital",
        requires: &["DRC-024", "DRC-025", "DRC-026"],
        not_covered: &["length matching of single-ended buses", "reference-plane continuity under every high-speed net", "stack-up impedance confirmed by the fabricator"] },
    OverlayRules { class: "usb_interface",
        requires: &["DRC-024", "DRC-025", "DRC-026"],
        not_covered: &["ESD protection at the connector", "VBUS over-current and over-voltage protection"] },
    OverlayRules { class: "can_interface",
        requires: &[],
        not_covered: &["bus termination", "common-mode and ESD protection of the bus lines"] },
    OverlayRules { class: "industrial_interfaces",
        requires: &[],
        not_covered: &["surge and burst protection of field wiring", "termination and isolation of the interface"] },
    OverlayRules { class: "low_power",
        requires: &[],
        not_covered: &["sleep-current budget of every part on the always-on rail", "leakage of pull-ups and dividers"] },
    OverlayRules { class: "harsh_environment",
        requires: &["ERC-025"],
        not_covered: &["temperature derating of every part at the declared ambient", "conformal coating and creepage under pollution"] },
    OverlayRules { class: "automotive_environment",
        requires: &["ERC-022"],
        not_covered: &["load dump and reverse battery", "AEC-Q grade of the parts", "automotive EMC"] },
];

/// What the overlay table says about `class`.
pub fn overlay(class: &str) -> Option<&'static OverlayRules> {
    OVERLAY_RULES.iter().find(|o| o.class == class)
}

/// The registry: every rule the checks declare.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuleRegistry {
    pub version: u32,
    pub tool: String,
    pub rules: Vec<RegisteredRule>,
    pub overlays: &'static [OverlayRules],
    pub limitations: Vec<&'static str>,
}

impl RuleRegistry {
    pub fn rule(&self, id: &str) -> Option<&RegisteredRule> {
        self.rules.iter().find(|r| r.id == id)
    }
}

fn fingerprint(id: &str, version: u32, check: &str, description: &str, source: &str) -> String {
    let version = version.to_string();
    let mut h = Sha256::new();
    for part in [id, version.as_str(), check, description, source] {
        h.update(part.as_bytes());
        h.update([0u8]);
    }
    let hex = format!("{:x}", h.finalize());
    hex[..16].to_string()
}

fn build() -> RuleRegistry {
    // The checks declare their rules in every report, so an empty project yields the tables.
    let p = Project::new("rule-registry");
    let fab = dfm::fabricator_standard();
    let reports = [
        ("eak_validation::erc::run", erc::run(&p)),
        ("eak_validation::drc::run", drc::run(&p)),
        ("eak_validation::dfm::run", dfm::run(&p, &fab)),
        ("eak_validation::mech::run", mech::run(&p)),
        ("eak_validation::sourcing::run", sourcing::run(&p, &[])),
    ];
    let mut declared: Vec<(&str, String, RuleInfo)> = vec![];
    for (eval, r) in reports {
        for ri in r.rules {
            declared.push((eval, r.check.clone(), ri));
        }
    }
    let mut by_rule: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for g in gates::evaluate(&p, Some(&fab)).gates {
        for e in &g.evidence {
            for id in &e.rule_ids {
                by_rule.entry(id.clone()).or_default().insert(g.id.clone());
            }
        }
    }
    let rules = declared
        .into_iter()
        .map(|(eval, check, ri)| {
            let version = RULE_VERSIONS
                .iter()
                .find(|(id, _)| *id == ri.id)
                .map_or(1, |(_, v)| *v);
            let required_by = OVERLAY_RULES
                .iter()
                .filter(|o| o.requires.contains(&ri.id.as_str()))
                .map(|o| o.class.to_string())
                .collect();
            RegisteredRule {
                fingerprint: fingerprint(&ri.id, version, &check, &ri.description, &ri.source),
                version,
                gates: by_rule
                    .get(&ri.id)
                    .map(|s| s.iter().cloned().collect())
                    .unwrap_or_default(),
                required_by,
                source_state: source_state(&ri.source),
                evaluator: eval.to_string(),
                check,
                id: ri.id,
                description: ri.description,
                source: ri.source,
            }
        })
        .collect();
    RuleRegistry {
        version: REGISTRY_VERSION,
        tool: format!("eak-validation {}", env!("CARGO_PKG_VERSION")),
        rules,
        overlays: OVERLAY_RULES,
        limitations: vec![
            "the overlay table is EAK's own mapping of domain classes to rules, not a standard",
            "rule versions are kept by hand; the fingerprint shows a changed definition that was not bumped",
            "no rule's source has been verified against the text of the standard it cites",
        ],
    }
}

/// The registry of this build (computed once).
pub fn registry() -> &'static RuleRegistry {
    static R: OnceLock<RuleRegistry> = OnceLock::new();
    R.get_or_init(build)
}

// ------------------------------------------------------------------ composition

/// A domain class the composition applies.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActiveClass {
    pub id: String,
    /// `archetype` or `overlay`.
    pub kind: String,
    /// How far the classification can be relied on (a value in conflict is `missing`).
    pub level: Level,
    /// Why it is active.
    pub basis: String,
}

/// One rule an active class requires, with the project configuration it meets.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComposedRule {
    pub rule_id: String,
    pub required_by: Vec<String>,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity_override: Option<RuleSeverity>,
    /// Set when the project configuration weakens the rule: it is reported, never applied
    /// silently.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UncoveredConcern {
    pub class: String,
    pub concern: String,
}

/// The rules the project's domain classification composes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Composition {
    pub registry_version: u32,
    /// Whether any domain class is recorded at all.
    pub classified: bool,
    pub active: Vec<ActiveClass>,
    /// Recorded classes that are not active (`false`, with no `true` candidate).
    pub not_applying: Vec<String>,
    /// Recorded classes that are not firm yet (provisional, or in conflict).
    pub not_firm: Vec<String>,
    pub rules: Vec<ComposedRule>,
    pub not_covered: Vec<UncoveredConcern>,
    pub limitations: Vec<&'static str>,
}

impl Composition {
    /// Required rules whose project configuration weakens them.
    pub fn conflicts(&self) -> impl Iterator<Item = &ComposedRule> {
        self.rules.iter().filter(|r| r.conflict.is_some())
    }
}

fn is_true(v: Option<&ReqValue>) -> bool {
    matches!(v, Some(ReqValue::Flag { value: true }))
}

/// Compose the rules of `p`'s domain classification. A class is active when its recorded value
/// is `true`, or when any conflicting candidate is `true`: until a person resolves the conflict,
/// the stricter reading applies.
pub fn compose(p: &Project) -> Composition {
    let report = rq::evaluate(&p.requirement_set);
    let mut active = vec![];
    let mut not_applying = vec![];
    let mut not_firm = vec![];
    for f in &report.domain {
        let Some(id) = f.path.strip_prefix(DOMAIN_PREFIX) else {
            continue;
        };
        let Some(def) = DOMAIN_CLASSES.iter().find(|c| c.id == id) else {
            continue;
        };
        if f.level != Level::Firm {
            not_firm.push(id.to_string());
        }
        let basis = if is_true(f.value.as_ref()) {
            let status = f.status.unwrap_or(ReqStatus::Unknown);
            format!("recorded true ({status:?})")
        } else if f.conflicts.iter().any(|c| is_true(c.value.as_ref())) {
            "a conflicting value says it applies: the stricter reading is kept until a person resolves it".to_string()
        } else {
            not_applying.push(id.to_string());
            continue;
        };
        active.push(ActiveClass {
            id: id.to_string(),
            kind: match def.kind {
                DomainKind::Archetype => "archetype".to_string(),
                DomainKind::Overlay => "overlay".to_string(),
            },
            level: f.level,
            basis,
        });
    }
    let mut required: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    let mut not_covered = vec![];
    for a in &active {
        let Some(o) = overlay(&a.id) else { continue };
        for r in o.requires {
            required.entry(*r).or_default().push(a.id.clone());
        }
        for c in o.not_covered {
            not_covered.push(UncoveredConcern {
                class: a.id.clone(),
                concern: (*c).to_string(),
            });
        }
    }
    let cfg = &p.constraints.rules;
    let rules = required
        .into_iter()
        .map(|(id, by)| {
            let c = cfg.get(id);
            let enabled = c.is_none_or(|c| c.enabled);
            let severity_override = c.and_then(|c| c.severity);
            let classes = by.join(", ");
            let conflict = if !enabled {
                Some(format!(
                    "{id} is disabled in the project's rule configuration, but {classes} require(s) it"
                ))
            } else {
                severity_override
                    .filter(|s| *s != RuleSeverity::Error)
                    .map(|s| {
                        format!(
                            "{id} is shown at {s:?} in the project's rule configuration, but {classes} require(s) it at error"
                        )
                    })
            };
            ComposedRule {
                rule_id: id.to_string(),
                required_by: by,
                enabled,
                severity_override,
                conflict,
            }
        })
        .collect();
    Composition {
        registry_version: REGISTRY_VERSION,
        classified: !report.domain.is_empty(),
        active,
        not_applying,
        not_firm,
        rules,
        not_covered,
        limitations: vec![
            "only recorded domain classes are composed: a class nobody recorded adds nothing",
            "composition checks that required rules run at error; the rules' own limits are the project's declared constraints",
        ],
    }
}

/// Id of the required composition gate in [`gates::evaluate`].
pub const GATE_ID: &str = "domain_rules";
/// Id of the advisory gate listing the concerns no evaluator covers.
pub const CONCERNS_GATE_ID: &str = "domain_concerns";

fn active_list(c: &Composition) -> String {
    if c.active.is_empty() {
        return "none".to_string();
    }
    let ids: Vec<&str> = c.active.iter().map(|a| a.id.as_str()).collect();
    ids.join(", ")
}

/// The two composition gates.
///
/// * `domain_rules` (required): `FAIL` when the project configuration weakens a rule an active
///   class requires; `UNKNOWN` when no archetype is recorded as applying, or a recorded class is
///   not firm; `PASS` otherwise.
/// * `domain_concerns` (advisory): `UNKNOWN` while an active class has concerns no evaluator
///   covers; EAK records no review of them yet, so they are listed, never passed.
pub(crate) fn composition_gates(p: &Project) -> [Gate; 2] {
    let c = compose(p);
    let detail = format!(
        "rule registry v{}; active classes: {}",
        c.registry_version,
        active_list(&c)
    );
    let mut g = Gate {
        id: GATE_ID.into(),
        title: "Rules the board's domain classes require run unweakened".into(),
        state: GateState::Pass,
        required: true,
        reasons: vec![],
        evidence: vec![Evidence {
            source: "DOMAIN".into(),
            rule_ids: c.rules.iter().map(|r| r.rule_id.clone()).collect(),
            finding_ids: vec![],
            detail: detail.clone(),
        }],
    };
    for r in c.conflicts() {
        g.state = GateState::Fail;
        g.reasons.push(r.conflict.clone().unwrap_or_default());
    }
    if !c.active.iter().any(|a| a.kind == "archetype") {
        unknown_unless_failed(
            &mut g,
            "no archetype is recorded as applying (domain.*): run ClassifyDomain and confirm what the board is; overlay rules are composed only from recorded classes",
        );
    }
    for id in &c.not_firm {
        unknown_unless_failed(
            &mut g,
            format!("domain.{id} is not firm: a person confirms it or resolves its conflict"),
        );
    }
    let mut concerns = Gate {
        id: CONCERNS_GATE_ID.into(),
        title: "Concerns of the board's domain classes that no evaluator covers".into(),
        state: GateState::Pass,
        required: false,
        reasons: vec![],
        evidence: vec![Evidence {
            source: "DOMAIN".into(),
            rule_ids: vec![],
            finding_ids: vec![],
            detail,
        }],
    };
    if c.active.is_empty() {
        unknown_unless_failed(&mut concerns, "no domain class is recorded as applying");
    }
    for u in &c.not_covered {
        unknown_unless_failed(
            &mut concerns,
            format!("{}: {} has no evaluator in this build", u.class, u.concern),
        );
    }
    [g, concerns]
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::{known_rule_id, RULE_FAMILIES};

    #[test]
    fn the_registry_lists_every_declared_rule_once() {
        let r = registry();
        let ids: BTreeSet<&str> = r.rules.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids.len(), r.rules.len(), "duplicate rule ids");
        let known: usize = RULE_FAMILIES.iter().map(|(_, n)| *n as usize).sum();
        assert_eq!(r.rules.len(), known);
        for x in &r.rules {
            assert!(known_rule_id(&x.id), "{}", x.id);
            assert!(x.version >= 1);
            assert_eq!(x.fingerprint.len(), 16);
            assert!(x.evaluator.starts_with("eak_validation::"));
            assert!(
                !x.description.is_empty() && !x.source.is_empty(),
                "{}",
                x.id
            );
        }
        for (id, v) in RULE_VERSIONS {
            assert!(ids.contains(id), "versioned rule {id} is not declared");
            assert!(*v > 1, "{id}: a listed version is a bump");
        }
    }

    #[test]
    fn the_overlay_table_covers_every_domain_class_with_real_rules() {
        let classes: Vec<&str> = DOMAIN_CLASSES.iter().map(|c| c.id).collect();
        let table: Vec<&str> = OVERLAY_RULES.iter().map(|o| o.class).collect();
        assert_eq!(classes, table, "one entry per class, in catalogue order");
        for o in OVERLAY_RULES {
            for r in o.requires {
                assert!(known_rule_id(r), "{}: {r}", o.class);
            }
            assert!(
                !o.requires.is_empty() || !o.not_covered.is_empty(),
                "{} adds nothing",
                o.class
            );
        }
        let r = registry();
        assert_eq!(
            r.rule("DRC-002").unwrap().required_by,
            ["ac_mains_hv", "high_voltage", "galvanic_isolation"]
        );
    }

    #[test]
    fn registry_links_rules_to_their_gates_and_marks_sources_honestly() {
        let r = registry();
        let shorts = r.rule("DRC-003").unwrap();
        assert!(shorts.gates.iter().any(|g| g == "no_shorts"), "{shorts:?}");
        assert_eq!(shorts.source_state, SourceState::EakDefinition);
        // IPC-2221 based current capacity is cited, never presented as verified.
        let current = r.rule("DRC-008").unwrap();
        assert_eq!(current.source_state, SourceState::CitedNotVerified);
        // Same build, same registry: the fingerprints are deterministic.
        assert_eq!(&build(), r);
    }
}
