//! The requirement compiler: design data + structured requirements -> engineering model.
//!
//! Inputs are the EAK-IR project (DESK classification, free-text IR requirements) and the
//! structured requirements the caller derived from the stored design spec (the harness owns
//! that adapter, so there is one spec schema). Nothing is stored: the model is recomputed for
//! the revision it reports, so it can never drift from the design.

use super::discipline;
use super::domain_models::{self, CheckState, DomainModel};
use super::domains::{self, Coverage};
use super::interaction::{self, InteractionRule, RULES};
use super::model::*;
use super::profile::{Level, ProductModel, ProfileNode};
use crate::domain::{classify_project, classify_text, DomainClassification, Status};
use eak_core::units::{Dimension, PhysicalQuantity};
use eak_core::Project;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const PRODUCT: &str = "product";
pub const BOARD: &str = "board:main";

/// A profile node the caller knows about (a spec rail, an interface, a referenced part).
#[derive(Debug, Clone)]
pub struct BlockHint {
    pub id: String,
    pub name: String,
    pub level: Level,
    /// Defaults to the main board.
    pub parent: Option<String>,
    /// `None`: no engineering-domain mapping (reported as missing information).
    pub domain: Option<String>,
    pub reason: String,
    pub objects: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CompileInput {
    pub requirements: Vec<Requirement>,
    pub blocks: Vec<BlockHint>,
    /// Where the structured requirements came from, `None` when no spec is stored.
    pub spec_source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassificationBrief {
    pub status: Status,
    pub confidence: f64,
    pub primary: Option<String>,
    pub secondary: Vec<String>,
    pub overlays: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveDomain {
    pub id: String,
    pub name: String,
    pub high_risk: bool,
    pub coverage: Coverage,
    /// Nodes tagged with the domain directly.
    pub nodes: Vec<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingInfo {
    pub id: String,
    pub domain: Option<String>,
    pub what: String,
    pub why: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveInteraction {
    pub rule: &'static InteractionRule,
    /// Participants of the rule present in this product.
    pub present: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineeringModel {
    pub revision: u64,
    pub classification: ClassificationBrief,
    pub product: ProductModel,
    pub domains: Vec<ActiveDomain>,
    pub requirements: Vec<Requirement>,
    pub constraints: Vec<Constraint>,
    pub conflicts: Vec<Conflict>,
    pub precedence_violations: Vec<String>,
    pub calculations: Vec<CalculationRecord>,
    pub interactions: Vec<ActiveInteraction>,
    /// Typed per-domain engineering models with their deterministic checks.
    pub domain_models: Vec<DomainModel>,
    pub risks: Vec<Risk>,
    pub missing_information: Vec<MissingInfo>,
    pub dependencies: Dependencies,
    pub not_implemented: Vec<&'static str>,
    pub limitations: Vec<&'static str>,
}

pub const NOT_IMPLEMENTED: &[&str] = &[
    "Multi-board products: EAK-IR holds one board per project, so the model has one board node",
    "PCB-region profiles: no region is derived from the layout yet (the level exists in the model)",
    "Requirement extraction from documents, spreadsheets or diagrams (only the structured spec and IR requirement text are read)",
    "Coupling magnitudes for cross-domain interactions (rules name what to re-evaluate; they compute nothing)",
    "Creepage/clearance values from standards (no standard tables are embedded)",
];

pub const LIMITATIONS: &[&str] = &[
    "The model is recomputed from the design and the stored spec at the reported revision; it is not stored and has no history of its own.",
    "Risk severity, likelihood and detectability come from a fixed rule table (see `basis`), not from measurement; they are not combined into one score.",
    "Domain classification is the DESK keyword/feature classifier plus the spec's interfaces and rails; a sparse design yields few domains.",
    "Mental models are DRAFT: written from general practice, not verified against sources or hardware.",
];

fn eng_domains_of_desk(desk_domain: &str) -> Vec<&'static str> {
    domains::from_desk_domain(desk_domain)
        .map(|d| d.id)
        .collect()
}

fn product_tags(model: &mut ProductModel, c: &DomainClassification) {
    let ids: Vec<&String> = c.primary.iter().chain(c.secondary.iter()).collect();
    for cand in c
        .candidates
        .iter()
        .filter(|x| ids.contains(&&x.archetype_id))
    {
        let why = format!(
            "DESK class {} ({:?}, score {:.1}){}",
            cand.name,
            c.status,
            cand.score,
            cand.reasons
                .first()
                .map(|r| format!(": {r}"))
                .unwrap_or_default()
        );
        for d in eng_domains_of_desk(&cand.domain) {
            model.tag(PRODUCT, d, why.clone());
        }
    }
    for o in &c.overlays {
        for d in domains::from_desk_overlay(o) {
            model.tag(PRODUCT, d.id, format!("DESK overlay {o}"));
        }
    }
}

fn ir_requirements(p: &Project) -> Vec<Requirement> {
    p.requirements
        .iter()
        .map(|r| {
            let c = classify_text(&r.text);
            let domains = c
                .candidates
                .first()
                .filter(|_| c.primary.is_some())
                .map(|cand| eng_domains_of_desk(&cand.domain))
                .unwrap_or_default()
                .into_iter()
                .map(String::from)
                .collect();
            Requirement {
                id: format!("ir.{}", r.id),
                description: r.text.clone(),
                category: Category::Unclassified,
                domains,
                quantity: None,
                value: Value::Text {
                    text: r.text.clone(),
                },
                unit: None,
                tolerance: Tolerance::None,
                priority: Priority::Should,
                source: SourceRef {
                    kind: SourceKind::IrRequirement,
                    reference: r.id.clone(),
                },
                evidence: vec![],
                knowledge: Knowledge::Stated,
                affects: vec![PRODUCT.into()],
                constraints: vec![],
                validation_method: None,
                approval: ApprovalState::Pending,
            }
        })
        .collect()
}

fn constraint_category(c: Category) -> ConstraintCategory {
    match c {
        Category::Electrical | Category::Unclassified => ConstraintCategory::Electrical,
        Category::Mechanical => ConstraintCategory::Mechanical,
        Category::Thermal => ConstraintCategory::Thermal,
        Category::Environmental => ConstraintCategory::Environmental,
        Category::Manufacturing => ConstraintCategory::Manufacturing,
        Category::Safety => ConstraintCategory::Safety,
        Category::Commercial => ConstraintCategory::Commercial,
    }
}

const CURRENT_KEYS: &[&str] = &[
    "output_current:",
    "input_current:",
    "rail_current:",
    "phase_current:",
    "charge_current:",
];

/// Compile requirement constraints, their derived constraints, and conditional safety
/// constraints of high-risk domains. Fills `Requirement::constraints`.
fn compile_constraints(reqs: &mut [Requirement], present: &[&str]) -> Vec<Constraint> {
    let mut out = vec![];
    for r in reqs.iter_mut() {
        let (Some(q), Some((min, max))) = (r.quantity.clone(), r.value.bounds()) else {
            continue;
        };
        let scope = r.affects.first().cloned().unwrap_or_else(|| PRODUCT.into());
        let unit = r.unit;
        let base = Constraint {
            id: format!("C.{}", r.id),
            description: r.description.clone(),
            kind: if r.priority == Priority::Must {
                Kind::Hard
            } else {
                Kind::Soft
            },
            condition: None,
            category: constraint_category(r.category),
            safety: r.category == Category::Safety,
            approved: matches!(r.approval, ApprovalState::Approved { .. }),
            scope: scope.clone(),
            quantity: q.clone(),
            bound: Bound { min, max, unit },
            requirements: vec![r.id.clone()],
            derived_from: vec![],
            provenance: Provenance {
                source: format!("requirement {} ({:?})", r.id, r.knowledge),
                rule: r.description.clone(),
                applicability: format!("applies to {scope} and every node inside it"),
            },
        };
        r.constraints.push(base.id.clone());
        let net = CURRENT_KEYS.iter().find_map(|k| q.strip_prefix(*k));
        let imax = max
            .zip(unit)
            .filter(|(_, u)| u.dimension() == Dimension::Current)
            .map(|(m, u)| PhysicalQuantity::new(m, u).si_magnitude());
        if let (Some(net), Some(imax)) = (net, imax) {
            let id = format!("C.{}.trace_capacity", r.id);
            r.constraints.push(id.clone());
            out.push(Constraint {
                id,
                description: format!("every track of net {net} carries {imax} A"),
                kind: Kind::Derived,
                condition: None,
                category: ConstraintCategory::Routing,
                safety: false,
                approved: false,
                scope: scope.clone(),
                quantity: format!("trace_capacity_a:{net}"),
                bound: Bound {
                    min: Some(imax),
                    max: None,
                    unit: Some(Unit::Ampere),
                },
                requirements: vec![r.id.clone()],
                derived_from: vec![base.id.clone()],
                provenance: Provenance {
                    source:
                        "eak_engineering::ipc (IPC-2221 conductor sizing as implemented in EAK)"
                            .into(),
                    rule: "a track's current capacity must cover the worst-case current of its net"
                        .into(),
                    applicability: format!("{q} has a worst-case value"),
                },
            });
        }
        out.push(base);
    }
    if present.contains(&"mains_hv") {
        out.push(Constraint {
            id: "C.mains_hv.creepage".into(),
            description: "creepage and clearance across the isolation barrier".into(),
            kind: Kind::Conditional,
            condition: Some("an isolation barrier exists on the board".into()),
            category: ConstraintCategory::Safety,
            safety: true,
            approved: false,
            scope: BOARD.into(),
            quantity: "creepage:isolation_barrier".into(),
            bound: Bound {
                min: None,
                max: None,
                unit: Some(Unit::Metre),
            },
            requirements: vec![],
            derived_from: vec![],
            provenance: Provenance {
                source: "engineering domain mains_hv".into(),
                rule: "an isolation barrier needs a creepage and clearance limit from the applicable standard".into(),
                applicability: "the product is classified mains/high voltage".into(),
            },
        });
    }
    out
}

fn precedence_violations(cs: &[Constraint], model: &ProductModel) -> Vec<String> {
    let mut out = vec![];
    for a in cs {
        for b in cs {
            let related =
                model.is_within(&a.scope, &b.scope) || model.is_within(&b.scope, &a.scope);
            if a.id != b.id && related {
                if let Some(v) = weakens(a, b) {
                    out.push(v);
                }
            }
        }
    }
    out
}

/// A voltage requirement value in volts; `None` when the unit is missing or not a voltage.
fn volts(r: &Requirement, v: f64) -> Option<f64> {
    r.unit
        .filter(|u| u.dimension() == Dimension::Voltage)
        .map(|u| PhysicalQuantity::new(v, u).si_magnitude())
}

fn buck_calcs(reqs: &[Requirement], revision: u64) -> Vec<CalculationRecord> {
    let by = |k: &str| {
        reqs.iter()
            .filter(|r| r.quantity.as_deref().is_some_and(|q| q.starts_with(k)))
            .collect::<Vec<_>>()
    };
    let mut out = vec![];
    for vin in by("input_voltage:") {
        let Value::Range {
            min: Some(vin_min),
            max: vin_max,
            ..
        } = vin.value
        else {
            continue;
        };
        let (Some(vin_min), vin_max) = (volts(vin, vin_min), vin_max.and_then(|m| volts(vin, m)))
        else {
            continue;
        };
        for vout in by("output_voltage:") {
            let vo = match vout.value {
                Value::Range {
                    nominal: Some(n), ..
                }
                | Value::Number { value: n } => n,
                _ => continue,
            };
            let Some(vo) = volts(vout, vo) else { continue };
            let inputs = vec![
                CalcInput {
                    name: "Vin(min)".into(),
                    value: vin_min,
                    unit: Unit::Volt,
                    source: vin.id.clone(),
                    knowledge: vin.knowledge,
                },
                CalcInput {
                    name: "Vout(nom)".into(),
                    value: vo,
                    unit: Unit::Volt,
                    source: vout.id.clone(),
                    knowledge: vout.knowledge,
                },
            ];
            let assumed = inputs.iter().any(|i| i.knowledge < Knowledge::Stated);
            let (result, error) = match eak_engineering::calc::buck_duty_cycle(vin_min, vo) {
                Ok(d) => (Some(d), None),
                Err(e) => (None, Some(e.to_string())),
            };
            let mut assumptions: Vec<String> =
                vec!["ideal continuous conduction, losses ignored".into()];
            if let Some(m) = vin_max {
                assumptions.push(format!(
                    "worst case is Vin(min); Vin(max) = {m} V gives the minimum duty"
                ));
            }
            let state = match (result.is_some(), assumed) {
                (false, _) => CalcState::NotComputed,
                (true, true) => CalcState::ComputedFromAssumptions,
                (true, false) => CalcState::Computed,
            };
            out.push(CalculationRecord {
                id: format!("calc.buck_duty.{}.{}", vin.id, vout.id),
                name: "buck duty cycle at minimum input".into(),
                inputs,
                formula: "D = Vout / Vin".into(),
                method: "eak_engineering::calc::buck_duty_cycle".into(),
                formula_source: "ideal buck volt-second balance (textbook relation)".into(),
                assumptions,
                margin: None,
                result,
                result_unit: Unit::Unitless,
                error,
                limit: None,
                comparison: if result.is_some() {
                    Comparison::NoLimit
                } else {
                    Comparison::NotComputed
                },
                state,
                revision,
            });
        }
    }
    out
}

/// The EAK check that evaluates a constraint quantity, if any.
fn check_for(quantity: &str) -> Option<&'static str> {
    if quantity.starts_with("trace_capacity_a:") {
        Some("net_trace_capacity_a")
    } else if quantity.contains("voltage:") {
        Some("erc")
    } else {
        None
    }
}

fn missing(
    present: &[&str],
    reqs: &[Requirement],
    input: &CompileInput,
    c: &DomainClassification,
) -> Vec<MissingInfo> {
    let mut out = vec![];
    if input.spec_source.is_none() {
        out.push(MissingInfo {
            id: "missing.spec".into(),
            domain: None,
            what: "no structured design spec (design_spec_set)".into(),
            why: "only free-text IR requirements are known; no value, unit or limit can be checked"
                .into(),
        });
    }
    for &d in present {
        let Some(m) = discipline::model(d) else {
            continue;
        };
        for ri in &m.required_inputs {
            let have = reqs.iter().any(|r| {
                r.value.is_known()
                    && r.quantity.as_deref().is_some_and(|q| {
                        ri.quantity_prefixes
                            .iter()
                            .any(|p| q.starts_with(p.as_str()))
                    })
            });
            if !have {
                out.push(MissingInfo {
                    id: format!("missing.{d}.{}", ri.key),
                    domain: Some(d.to_string()),
                    what: ri.key.clone(),
                    why: ri.why.clone(),
                });
            }
        }
    }
    for r in reqs {
        if !r.value.is_known() {
            out.push(MissingInfo {
                id: format!("missing.value.{}", r.id),
                domain: r.domains.first().cloned(),
                what: format!("value of {}", r.id),
                why: "stated without a value; it stays unknown".into(),
            });
        } else if r.priority == Priority::Must && r.validation_method.is_none() {
            out.push(MissingInfo {
                id: format!("missing.validation.{}", r.id),
                domain: r.domains.first().cloned(),
                what: format!("validation method for {}", r.id),
                why: "a mandatory requirement nothing checks".into(),
            });
        }
    }
    for b in input.blocks.iter().filter(|b| b.domain.is_none()) {
        out.push(MissingInfo {
            id: format!("missing.domain.{}", b.id),
            domain: None,
            what: format!("engineering domain of {}", b.name),
            why: format!("{}: no domain mapping", b.reason),
        });
    }
    for (i, m) in c.missing_information.iter().enumerate() {
        out.push(MissingInfo {
            id: format!("missing.classifier.{i}"),
            domain: None,
            what: m.clone(),
            why: "reported by the DESK classifier".into(),
        });
    }
    out
}

const RISK_BASIS: &str = "fixed EAK rule table (systems::compile::risks); not measured";

fn risk(
    id: String,
    class: RiskClass,
    title: String,
    detail: String,
    (severity, likelihood, detectability): (u8, u8, u8),
    impact: &str,
    sources: Vec<String>,
    doms: Vec<String>,
) -> Risk {
    let high = doms
        .iter()
        .any(|d| domains::domain(d).is_some_and(|x| x.high_risk));
    // Rule-table factors (1..5) onto the shared ADR-0026 axes.
    let severity = match severity {
        5.. => Severity::Critical,
        4 => Severity::High,
        3 => Severity::Medium,
        _ => Severity::Low,
    };
    let likelihood = match likelihood {
        4.. => Likelihood::Likely,
        3 => Likelihood::Possible,
        _ => Likelihood::Rare,
    };
    let detectability = if detectability <= 2 {
        Detectability::Detected
    } else {
        Detectability::NotChecked
    };
    let subsystem = if doms.is_empty() {
        "product".to_string()
    } else {
        doms.join(", ")
    };
    Risk {
        record: RiskRecord {
            id,
            subsystem,
            severity,
            likelihood,
            detectability,
            consequence: impact.into(),
            evidence: sources,
            mitigation: None,
            approval_required: high || class == RiskClass::Safety,
        },
        class,
        title,
        detail,
        domains: doms,
        basis: RISK_BASIS.into(),
    }
}

fn risks(m: &EngineeringModel) -> Vec<Risk> {
    let mut out = vec![];
    let con = |id: &str| m.constraints.iter().find(|c| c.id == id);
    let req_domains = |ids: &[String]| -> Vec<String> {
        let set: BTreeSet<String> = m
            .requirements
            .iter()
            .filter(|r| ids.contains(&r.id))
            .flat_map(|r| r.domains.clone())
            .collect();
        set.into_iter().collect()
    };
    for c in &m.conflicts {
        let safety = c
            .constraints
            .iter()
            .any(|i| con(i).is_some_and(|x| x.safety));
        let reqs: Vec<String> = c
            .constraints
            .iter()
            .filter_map(|i| con(i))
            .flat_map(|x| x.requirements.clone())
            .collect();
        out.push(risk(
            format!("risk.{}", c.id),
            if safety {
                RiskClass::Safety
            } else {
                RiskClass::ElectricalViolation
            },
            format!("conflicting limits on {}", c.quantity),
            c.explanation.clone(),
            (if safety { 5 } else { 4 }, 5, 1),
            "no design can satisfy every stated requirement until the conflict is resolved",
            vec![c.id.clone()],
            req_domains(&reqs),
        ));
    }
    for (i, v) in m.precedence_violations.iter().enumerate() {
        out.push(risk(
            format!("risk.precedence.{i}"),
            RiskClass::Safety,
            "a protected limit would be weakened".into(),
            v.clone(),
            (5, 3, 2),
            "a safety or approved limit could be relaxed silently",
            vec![],
            vec![],
        ));
    }
    for mi in &m.missing_information {
        let Some(d) = mi.domain.clone() else { continue };
        let high = domains::domain(&d).is_some_and(|x| x.high_risk);
        out.push(risk(
            format!("risk.{}", mi.id),
            RiskClass::MissingRequirement,
            format!("{d}: {} unknown", mi.what),
            mi.why.clone(),
            (if high { 4 } else { 3 }, 3, 4),
            "the design decisions that depend on it rest on guesses",
            vec![mi.id.clone()],
            vec![d],
        ));
    }
    for r in m
        .requirements
        .iter()
        .filter(|r| r.knowledge == Knowledge::Assumed && r.priority == Priority::Must)
    {
        out.push(risk(
            format!("risk.assumed.{}", r.id),
            RiskClass::DesignUncertainty,
            format!("mandatory requirement {} is only assumed", r.id),
            r.description.clone(),
            (3, 3, 3),
            "everything derived from it may change once a human confirms the real value",
            vec![r.id.clone()],
            r.domains.clone(),
        ));
    }
    for d in m.domains.iter().filter(|d| d.high_risk) {
        out.push(risk(
            format!("risk.high_risk_domain.{}", d.id),
            RiskClass::Safety,
            format!("{} is a high-risk application", d.name),
            "stronger evidence and a human expert's approval are required; EAK never certifies its own design".into(),
            (5, 2, 4),
            "harm to people or property if a fault escapes review",
            d.nodes.iter().map(|n| format!("node.{n}")).collect(),
            vec![d.id.clone()],
        ));
    }
    for i in m
        .interactions
        .iter()
        .filter(|i| !i.rule.not_implemented.is_empty())
    {
        out.push(risk(
            format!("risk.interaction.{}", i.rule.id),
            RiskClass::Integration,
            format!("cross-domain interaction {}", i.rule.id),
            format!(
                "{} EAK cannot check: {}",
                i.rule.mechanism,
                i.rule.not_implemented.join("; ")
            ),
            (3, 3, 4),
            "locally valid subsystems can still produce an invalid product",
            vec![format!("interaction.{}", i.rule.id)],
            i.present
                .iter()
                .filter(|p| !p.starts_with("category:"))
                .cloned()
                .collect(),
        ));
    }
    for (d, k) in m
        .domain_models
        .iter()
        .flat_map(|d| d.checks.iter().map(move |k| (d, k)))
        .filter(|(_, k)| k.outcome == CheckState::Fail)
    {
        out.push(risk(
            format!("risk.check.{}.{}", d.domain, k.id),
            RiskClass::ElectricalViolation,
            format!("{} check {} failed", d.domain, k.id),
            k.detail.clone(),
            (4, 4, 1),
            "a deterministic domain check failed",
            k.evidence.clone(),
            vec![d.domain.clone()],
        ));
    }
    for c in m
        .calculations
        .iter()
        .filter(|c| c.state == CalcState::NotComputed)
    {
        out.push(risk(
            format!("risk.{}", c.id),
            RiskClass::IncompleteEvidence,
            format!("{} not computed", c.name),
            c.error.clone().unwrap_or_default(),
            (2, 3, 2),
            "the calculation cannot support any claim",
            c.inputs.iter().map(|i| i.source.clone()).collect(),
            vec![],
        ));
    }
    out.sort_by(|a, b| {
        b.record
            .severity
            .cmp(&a.record.severity)
            .then(a.record.id.cmp(&b.record.id))
    });
    out
}

fn dependencies(m: &EngineeringModel) -> Dependencies {
    let mut d = Dependencies::default();
    for n in &m.product.nodes {
        if let Some(p) = &n.parent {
            d.add(
                format!("node:{p}"),
                format!("node:{}", n.id),
                "contained in",
            );
        }
    }
    for r in &m.requirements {
        for a in &r.affects {
            d.add(format!("req:{}", r.id), format!("node:{a}"), "applies to");
        }
        for c in &r.constraints {
            d.add(format!("req:{}", r.id), format!("con:{c}"), "compiled into");
        }
        if let Some(v) = &r.validation_method {
            d.add(
                format!("req:{}", r.id),
                format!("check:{v}"),
                "validated by",
            );
        }
    }
    for c in &m.constraints {
        for f in &c.derived_from {
            d.add(format!("con:{f}"), format!("con:{}", c.id), "derived from");
        }
        if let Some(chk) = check_for(&c.quantity) {
            d.add(
                format!("con:{}", c.id),
                format!("check:{chk}"),
                "checked by",
            );
        }
    }
    for k in &m.calculations {
        for i in &k.inputs {
            d.add(
                format!("req:{}", i.source),
                format!("calc:{}", k.id),
                "input of",
            );
        }
    }
    let tagged = |p: &str| -> Vec<String> {
        if p.starts_with("category:") {
            return vec![PRODUCT.into()];
        }
        m.product
            .nodes
            .iter()
            .filter(|n| n.domains.iter().any(|t| t.domain == p))
            .map(|n| n.id.clone())
            .collect()
    };
    for i in &m.interactions {
        let iid = format!("interaction:{}", i.rule.id);
        for r in &m.requirements {
            let Some(q) = &r.quantity else { continue };
            let from_match = i.rule.from.iter().any(|f| {
                r.domains.iter().any(|x| x == f)
                    || f.strip_prefix("category:")
                        .is_some_and(|c| format!("{:?}", r.category).eq_ignore_ascii_case(c))
            });
            if from_match && interaction::triggered_by(i.rule, q) {
                d.add(format!("req:{}", r.id), iid.clone(), i.rule.mechanism);
            }
        }
        for &t in i.rule.to {
            for n in tagged(t) {
                d.add(iid.clone(), format!("node:{n}"), format!("re-evaluate {t}"));
            }
        }
        for &e in i.rule.re_evaluate {
            d.add(iid.clone(), format!("review:{e}"), "re-evaluate");
        }
    }
    d
}

/// Compile the engineering model of the project at its current revision.
pub fn compile(p: &Project, input: CompileInput) -> EngineeringModel {
    let c = classify_project(p);
    let mut product = ProductModel::default();
    product.add(ProfileNode {
        id: PRODUCT.into(),
        level: Level::Product,
        name: p.name.clone(),
        parent: None,
        domains: vec![],
        objects: vec![],
    });
    product.add(ProfileNode {
        id: BOARD.into(),
        level: Level::Board,
        name: "main board".into(),
        parent: Some(PRODUCT.into()),
        domains: vec![],
        objects: vec![],
    });
    product_tags(&mut product, &c);
    for b in &input.blocks {
        product.add(ProfileNode {
            id: b.id.clone(),
            level: b.level,
            name: b.name.clone(),
            parent: Some(b.parent.clone().unwrap_or_else(|| BOARD.into())),
            domains: vec![],
            objects: b.objects.clone(),
        });
        if let Some(d) = &b.domain {
            product.tag(&b.id, d, b.reason.clone());
            product.tag(PRODUCT, d, format!("contains {}", b.name));
        }
    }
    let mut requirements = input.requirements.clone();
    requirements.extend(ir_requirements(p));
    for r in requirements
        .iter()
        .filter(|r| r.source.kind == SourceKind::IrRequirement)
    {
        for d in &r.domains {
            product.tag(PRODUCT, d, format!("requirement {}", r.id));
        }
    }

    let mut domain_map: BTreeMap<String, ActiveDomain> = BTreeMap::new();
    for n in &product.nodes {
        for t in &n.domains {
            let Some(info) = domains::domain(&t.domain) else {
                continue;
            };
            let e = domain_map
                .entry(t.domain.clone())
                .or_insert_with(|| ActiveDomain {
                    id: info.id.into(),
                    name: info.name.into(),
                    high_risk: info.high_risk,
                    coverage: info.coverage,
                    nodes: vec![],
                    reasons: vec![],
                });
            e.nodes.push(n.id.clone());
            for r in &t.reasons {
                if !e.reasons.contains(r) {
                    e.reasons.push(r.clone());
                }
            }
        }
    }
    // Registry order, so the UI is stable.
    let active: Vec<ActiveDomain> = domains::DOMAINS
        .iter()
        .filter_map(|d| domain_map.remove(d.id))
        .collect();
    let present: Vec<&str> = active.iter().map(|d| d.id.as_str()).collect();

    let constraints = compile_constraints(&mut requirements, &present);
    let conflicts = find_conflicts(&constraints, |a, b| product.is_within(a, b));
    let precedence_violations = precedence_violations(&constraints, &product);
    let calculations = if present.contains(&"power") {
        buck_calcs(&requirements, p.revision)
    } else {
        vec![]
    };
    let interactions: Vec<ActiveInteraction> = RULES
        .iter()
        .filter(|r| interaction::active(r, &present))
        .map(|r| ActiveInteraction {
            rule: r,
            present: r
                .from
                .iter()
                .chain(r.to)
                .filter(|x| interaction::ALWAYS.contains(*x) || present.contains(*x))
                .map(|x| x.to_string())
                .collect(),
        })
        .collect();
    let missing_information = missing(&present, &requirements, &input, &c);

    let mut m = EngineeringModel {
        revision: p.revision,
        classification: ClassificationBrief {
            status: c.status,
            confidence: c.confidence,
            primary: c.primary.clone(),
            secondary: c.secondary.clone(),
            overlays: c.overlays.clone(),
        },
        product,
        domains: active,
        requirements,
        constraints,
        conflicts,
        precedence_violations,
        calculations,
        interactions,
        domain_models: vec![],
        risks: vec![],
        missing_information,
        dependencies: Dependencies::default(),
        not_implemented: NOT_IMPLEMENTED.to_vec(),
        limitations: LIMITATIONS.to_vec(),
    };
    m.domain_models = domain_models::build(&m);
    m.risks = risks(&m);
    m.dependencies = dependencies(&m);
    m
}

impl EngineeringModel {
    /// Impact set of a change to a requirement, constraint or node id (prefixed or bare).
    pub fn impact(&self, id: &str) -> Result<ImpactSet, String> {
        let candidates = [
            id.to_string(),
            format!("req:{id}"),
            format!("con:{id}"),
            format!("node:{id}"),
        ];
        candidates
            .iter()
            .find(|c| self.dependencies.knows(c))
            .map(|c| self.dependencies.impact(c))
            .ok_or_else(|| {
                format!("{id} is not a requirement, constraint or node id of this model")
            })
    }

    /// Constraints that apply to a node: its own and every ancestor's (`Some(scope)` when
    /// inherited).
    pub fn applicable(&self, node: &str) -> Vec<(&Constraint, Option<String>)> {
        self.constraints
            .iter()
            .filter(|c| self.product.is_within(node, &c.scope))
            .map(|c| (c, (c.scope != node).then(|| c.scope.clone())))
            .collect()
    }
}

// ------------------------------------------------------------------ summary (§11)

#[derive(Debug, Clone, Serialize)]
pub struct NodeBrief {
    pub id: String,
    pub level: Level,
    pub name: String,
    pub parent: Option<String>,
    pub domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequirementCoverage {
    pub total: usize,
    pub by_knowledge: BTreeMap<String, usize>,
    pub with_validation_method: usize,
    pub unknown_values: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConstraintCounts {
    pub hard: usize,
    pub soft: usize,
    pub derived: usize,
    pub conditional: usize,
    pub safety: usize,
}

/// The intelligence summary for the UI (directive §11). Only computed values; dashboard
/// items EAK computes through other tools are named in `elsewhere`, never repeated or invented.
#[derive(Debug, Clone, Serialize)]
pub struct IntelligenceSummary {
    pub revision: u64,
    pub classification: ClassificationBrief,
    pub domains: Vec<ActiveDomain>,
    pub architecture: Vec<NodeBrief>,
    pub requirement_coverage: RequirementCoverage,
    pub missing_information: Vec<MissingInfo>,
    pub assumptions: Vec<String>,
    pub constraints: ConstraintCounts,
    pub conflicts: Vec<Conflict>,
    /// Per domain: (check id, outcome).
    pub domain_checks: BTreeMap<String, Vec<(String, CheckState)>>,
    pub risks: Vec<Risk>,
    pub human_approval_required: bool,
    /// (dashboard item, the tool that computes it).
    pub elsewhere: Vec<(&'static str, &'static str)>,
    pub not_implemented: Vec<&'static str>,
    pub limitations: Vec<&'static str>,
}

impl EngineeringModel {
    pub fn summary(&self) -> IntelligenceSummary {
        let mut by_knowledge = BTreeMap::new();
        for r in &self.requirements {
            let k = serde_json::to_value(r.knowledge)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            *by_knowledge.entry(k).or_insert(0) += 1;
        }
        let count = |k: Kind| self.constraints.iter().filter(|c| c.kind == k).count();
        IntelligenceSummary {
            revision: self.revision,
            classification: self.classification.clone(),
            domains: self.domains.clone(),
            architecture: self
                .product
                .nodes
                .iter()
                .map(|n| NodeBrief {
                    id: n.id.clone(),
                    level: n.level,
                    name: n.name.clone(),
                    parent: n.parent.clone(),
                    domains: n.domains.iter().map(|t| t.domain.clone()).collect(),
                })
                .collect(),
            requirement_coverage: RequirementCoverage {
                total: self.requirements.len(),
                by_knowledge,
                with_validation_method: self
                    .requirements
                    .iter()
                    .filter(|r| r.validation_method.is_some())
                    .count(),
                unknown_values: self
                    .requirements
                    .iter()
                    .filter(|r| !r.value.is_known())
                    .count(),
            },
            missing_information: self.missing_information.clone(),
            assumptions: self
                .requirements
                .iter()
                .filter(|r| r.knowledge == Knowledge::Assumed)
                .map(|r| format!("{}: {}", r.id, r.description))
                .collect(),
            constraints: ConstraintCounts {
                hard: count(Kind::Hard),
                soft: count(Kind::Soft),
                derived: count(Kind::Derived),
                conditional: count(Kind::Conditional),
                safety: self.constraints.iter().filter(|c| c.safety).count(),
            },
            conflicts: self.conflicts.clone(),
            domain_checks: self
                .domain_models
                .iter()
                .map(|d| {
                    let v: Vec<(String, CheckState)> =
                        d.checks.iter().map(|k| (k.id.clone(), k.outcome)).collect();
                    (d.domain.clone(), v)
                })
                .collect(),
            risks: self.risks.clone(),
            human_approval_required: self.risks.iter().any(|r| r.record.approval_required),
            elsewhere: vec![
                ("placement quality", "analyze_placement"),
                ("routing completeness", "get_connectivity"),
                ("validation results", "run_check / design_validate"),
                ("schematic completeness", "run_check erc"),
                (
                    "manufacturing package",
                    "export_manufacturing (never 'manufacturing-ready')",
                ),
            ],
            not_implemented: self.not_implemented.clone(),
            limitations: self.limitations.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_req(id: &str, q: &str, value: Value, unit: Unit, affects: &str) -> Requirement {
        Requirement {
            id: id.into(),
            description: id.into(),
            category: Category::Electrical,
            domains: vec!["power".into()],
            quantity: Some(q.into()),
            value,
            unit: Some(unit),
            tolerance: Tolerance::None,
            priority: Priority::Must,
            source: SourceRef {
                kind: SourceKind::DesignSpec,
                reference: id.into(),
            },
            evidence: vec![],
            knowledge: Knowledge::Stated,
            affects: vec![affects.into()],
            constraints: vec![],
            validation_method: Some("erc".into()),
            approval: ApprovalState::Pending,
        }
    }

    fn block(id: &str, dom: Option<&str>) -> BlockHint {
        BlockHint {
            id: id.into(),
            name: id.into(),
            level: Level::FunctionalBlock,
            parent: None,
            domain: dom.map(Into::into),
            reason: "test".into(),
            objects: vec![],
        }
    }

    fn range(min: Option<f64>, nominal: Option<f64>, max: Option<f64>) -> Value {
        Value::Range { min, nominal, max }
    }

    // Tests in this module are written, not run (code-reviewed only).
    #[test]
    fn power_plus_rf_product_compiles_with_conflict_impact_and_risks() {
        let p = eak_core::Project::new("bench unit");
        let input = CompileInput {
            requirements: vec![
                spec_req(
                    "vin",
                    "input_voltage:VIN",
                    range(Some(9.0), Some(12.0), Some(15.0)),
                    Unit::Volt,
                    "block:VIN",
                ),
                spec_req(
                    "vout",
                    "output_voltage:3V3",
                    range(Some(3.2), Some(3.3), Some(3.4)),
                    Unit::Volt,
                    "block:3V3",
                ),
                spec_req(
                    "iout",
                    "output_current:3V3",
                    range(None, None, Some(2.0)),
                    Unit::Ampere,
                    "block:3V3",
                ),
                // An acceptance limit contradicting the derived trace-capacity constraint.
                spec_req(
                    "cap",
                    "trace_capacity_a:3V3",
                    range(None, None, Some(1.0)),
                    Unit::Ampere,
                    "block:3V3",
                ),
            ],
            blocks: vec![
                block("block:VIN", Some("power")),
                block("block:3V3", Some("power")),
                block("block:radio", Some("rf")),
            ],
            spec_source: Some("design-flow/spec.json".into()),
        };
        let m = compile(&p, input);
        let ids: Vec<&str> = m.domains.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"power") && ids.contains(&"rf"));
        assert!(m
            .constraints
            .iter()
            .any(|c| c.kind == Kind::Derived && c.quantity == "trace_capacity_a:3V3"));
        assert_eq!(m.conflicts.len(), 1, "derived min 2 A vs stated max 1 A");
        let calc = &m.calculations[0];
        assert!((calc.result.unwrap() - 3.3 / 9.0).abs() < 1e-9);
        assert_eq!(calc.state, CalcState::Computed);
        assert!(m
            .interactions
            .iter()
            .any(|i| i.rule.id == "power_noise_into_rf"));
        assert!(m
            .missing_information
            .iter()
            .any(|x| x.id == "missing.rf.operating_frequency"));
        let imp = m.impact("iout").unwrap();
        assert!(imp.affected["con"]
            .iter()
            .any(|c| c.ends_with("trace_capacity")));
        assert!(imp.affected["check"].contains(&"check:net_trace_capacity_a".to_string()));
        assert!(imp.affected["node"].contains(&"node:block:radio".to_string()));
        assert!(m
            .risks
            .iter()
            .any(|r| r.class == RiskClass::ElectricalViolation));
        assert_eq!(m.summary().constraints.derived, 1);
        assert!(m.impact("nope").is_err());
    }

    #[test]
    fn high_risk_domain_requires_human_approval() {
        let p = eak_core::Project::new("pack");
        let input = CompileInput {
            blocks: vec![block("block:pack", Some("battery"))],
            ..Default::default()
        };
        let m = compile(&p, input);
        assert!(m
            .risks
            .iter()
            .any(|r| r.record.approval_required && r.class == RiskClass::Safety));
        assert!(m.missing_information.iter().any(|x| x.id == "missing.spec"));
        assert!(m.summary().human_approval_required);
    }
}
