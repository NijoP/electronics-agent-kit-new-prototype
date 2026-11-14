//! Requirements, constraints, calculation records, risks and the dependency graph.
//!
//! Rules the types enforce:
//! * an unknown value stays `Value::Unknown` and its knowledge cannot be raised
//!   ([`Requirement::promote`]);
//! * an assumption is never promoted by an agent or by EAK itself: only a human, with
//!   evidence, can move a requirement to `Confirmed`;
//! * a safety or approved constraint cannot be weakened by a lower-precedence one
//!   ([`weakens`]); the conflict is reported instead;
//! * every constraint has provenance, every calculation names its inputs, formula and
//!   revision, and every risk keeps its three factors separate (no single score).

use eak_core::units::PhysicalQuantity;
pub use eak_core::units::{Tolerance, Unit};
pub use eak_validation::state::{Detectability, Likelihood, Severity};
pub use eak_validation::RiskRecord;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

// ------------------------------------------------------------------ requirements

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Electrical,
    Mechanical,
    Thermal,
    Environmental,
    Manufacturing,
    Safety,
    Commercial,
    Unclassified,
}

/// A requirement or constraint value, in the unit named next to it (`eak_core::units::Unit`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Value {
    Unknown,
    Number {
        value: f64,
    },
    Range {
        min: Option<f64>,
        nominal: Option<f64>,
        max: Option<f64>,
    },
    Text {
        text: String,
    },
}

impl Value {
    pub fn is_known(&self) -> bool {
        !matches!(self, Value::Unknown)
    }
    /// (min, max) of a numeric value; `None` for unknown or text.
    pub fn bounds(&self) -> Option<(Option<f64>, Option<f64>)> {
        match self {
            Value::Number { value } => Some((Some(*value), Some(*value))),
            Value::Range { min, max, .. } => Some((*min, *max)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Must,
    Should,
    May,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// The structured design spec (`design_spec_set`).
    DesignSpec,
    /// A free-text requirement stored in EAK-IR (`Project::requirements`).
    IrRequirement,
    Document,
    Person,
    Assumption,
    Derived,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SourceRef {
    pub kind: SourceKind,
    /// Where exactly: spec field path, IR requirement id, document name, person.
    pub reference: String,
}

/// How much is known about a requirement value. Ordered: a later state is stronger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Knowledge {
    Unknown,
    Assumed,
    /// Stated by a source (spec, document) but not confirmed by a responsible human.
    Stated,
    Confirmed,
    Verified,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ApprovalState {
    NotRequired,
    Pending,
    Approved { by: String },
    Rejected { by: String, reason: String },
}

/// Who asks for a knowledge change.
#[derive(Debug, Clone, PartialEq)]
pub enum Promoter {
    Human {
        name: String,
    },
    Agent,
    /// A deterministic EAK check (ERC/DRC/eak-engineering evaluator) by name.
    Deterministic {
        check: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Requirement {
    pub id: String,
    pub description: String,
    pub category: Category,
    pub domains: Vec<String>,
    /// Machine key of the physical quantity, e.g. `input_voltage:VIN`, `ambient_temperature`.
    pub quantity: Option<String>,
    pub value: Value,
    pub unit: Option<Unit>,
    pub tolerance: Tolerance,
    pub priority: Priority,
    pub source: SourceRef,
    pub evidence: Vec<String>,
    pub knowledge: Knowledge,
    /// Profile node ids (product/board/block/component) this requirement applies to.
    pub affects: Vec<String>,
    /// Constraint ids compiled from this requirement.
    pub constraints: Vec<String>,
    /// How it will be validated (EAK check or evaluator name); `None` = no method yet.
    pub validation_method: Option<String>,
    pub approval: ApprovalState,
}

impl Requirement {
    /// Change the knowledge state under the promotion rules. Downgrades are always allowed.
    pub fn promote(
        &mut self,
        to: Knowledge,
        by: &Promoter,
        evidence: Option<&str>,
    ) -> Result<(), String> {
        if to <= self.knowledge {
            self.knowledge = to;
            return Ok(());
        }
        if !self.value.is_known() {
            return Err(format!(
                "{}: the value is unknown; it stays unknown until a value is supplied",
                self.id
            ));
        }
        let has_evidence = evidence.is_some_and(|e| !e.trim().is_empty());
        let ok = match (to, by) {
            (_, Promoter::Agent) => to == Knowledge::Assumed,
            (Knowledge::Assumed | Knowledge::Stated, _) => true,
            (Knowledge::Confirmed, Promoter::Human { .. }) => has_evidence,
            (Knowledge::Confirmed, Promoter::Deterministic { .. }) => false,
            (Knowledge::Verified, Promoter::Deterministic { .. }) => {
                self.knowledge >= Knowledge::Stated
            }
            (Knowledge::Verified, Promoter::Human { .. }) => has_evidence,
            (Knowledge::Unknown, _) => true,
        };
        if !ok {
            return Err(format!(
                "{}: {:?} -> {:?} by {:?} is not allowed (assumptions are confirmed only by a human with evidence)",
                self.id, self.knowledge, to, by
            ));
        }
        if let Some(e) = evidence {
            self.evidence.push(e.to_string());
        }
        if let (Knowledge::Confirmed, Promoter::Human { name }) = (to, by) {
            self.approval = ApprovalState::Approved { by: name.clone() };
        }
        self.knowledge = to;
        Ok(())
    }
}

// ------------------------------------------------------------------ constraints

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Hard,
    Soft,
    Derived,
    Conditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintCategory {
    Electrical,
    Mechanical,
    Thermal,
    Environmental,
    Manufacturing,
    Safety,
    Routing,
    Commercial,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Bound {
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// `None` for a dimensionless count or an unknown unit.
    pub unit: Option<Unit>,
}

fn si(v: Option<f64>, u: Option<Unit>) -> Option<f64> {
    v.map(|x| u.map_or(x, |u| PhysicalQuantity::new(x, u).si_magnitude()))
}

impl Bound {
    /// Bounds in SI base units (so mV and V compare).
    pub fn si(&self) -> (Option<f64>, Option<f64>) {
        (si(self.min, self.unit), si(self.max, self.unit))
    }
    /// Same physical dimension (both unitless counts also compare).
    pub fn comparable(&self, other: &Bound) -> bool {
        self.unit.map(Unit::dimension) == other.unit.map(Unit::dimension)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Provenance {
    /// Requirement id, spec field, DESK class or engineering formula the rule came from.
    pub source: String,
    /// The rule in one line.
    pub rule: String,
    /// Why it applies to this scope.
    pub applicability: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Constraint {
    pub id: String,
    pub description: String,
    pub kind: Kind,
    /// For `Kind::Conditional`: when it applies.
    pub condition: Option<String>,
    pub category: ConstraintCategory,
    pub safety: bool,
    /// Explicitly approved by a human (raises precedence).
    pub approved: bool,
    /// Profile node id; the constraint is inherited by every descendant.
    pub scope: String,
    pub quantity: String,
    pub bound: Bound,
    pub requirements: Vec<String>,
    pub derived_from: Vec<String>,
    pub provenance: Provenance,
}

impl Constraint {
    /// Precedence: safety and approved limits outrank everything; soft is lowest.
    pub fn precedence(&self) -> u8 {
        let base = match self.kind {
            Kind::Hard => 3,
            Kind::Derived => 2,
            Kind::Conditional => 1,
            Kind::Soft => 0,
        };
        let safety = if self.safety { 3 } else { 0 };
        let approved = if self.approved { 2 } else { 0 };
        base + safety + approved
    }
    pub fn protected(&self) -> bool {
        self.safety || self.approved
    }
}

/// `Some(reason)` when `new` would weaken the protected constraint `old` on the same
/// quantity: a looser or missing bound from a lower-precedence source.
pub fn weakens(old: &Constraint, new: &Constraint) -> Option<String> {
    if !old.protected() || old.quantity != new.quantity || new.precedence() >= old.precedence() {
        return None;
    }
    if !old.bound.comparable(&new.bound) {
        // Fail closed: a limit in another dimension cannot be shown not to weaken it.
        return Some(format!(
            "{} states {} in a unit not comparable with protected limit {}",
            new.id, new.quantity, old.id
        ));
    }
    let ((old_min, old_max), (new_min, new_max)) = (old.bound.si(), new.bound.si());
    let looser_min = match (old_min, new_min) {
        (Some(o), Some(n)) => n < o,
        (Some(_), None) => true,
        _ => false,
    };
    let looser_max = match (old_max, new_max) {
        (Some(o), Some(n)) => n > o,
        (Some(_), None) => true,
        _ => false,
    };
    (looser_min || looser_max).then(|| {
        format!(
            "{} would weaken protected limit {} on {}; a lower-precedence rule cannot relax a safety or approved limit",
            new.id, old.id, old.quantity
        )
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Conflict {
    pub id: String,
    pub constraints: [String; 2],
    pub quantity: String,
    pub explanation: String,
    /// Engineering options; EAK never picks one silently.
    pub tradeoffs: Vec<String>,
}

/// Contradictions between two constraints on the same quantity whose scopes overlap
/// (`within(a, b)`: scope a is b or inside b): disjoint bounds, or bounds in different units.
pub fn find_conflicts(cs: &[Constraint], within: impl Fn(&str, &str) -> bool) -> Vec<Conflict> {
    let mut out = vec![];
    for (i, a) in cs.iter().enumerate() {
        for b in &cs[i + 1..] {
            let related = within(&a.scope, &b.scope) || within(&b.scope, &a.scope);
            if a.quantity != b.quantity || !related {
                continue;
            }
            let explanation = if !a.bound.comparable(&b.bound) {
                format!(
                    "units differ ({:?} vs {:?}): the limits cannot be compared, so neither can be checked",
                    a.bound.unit, b.bound.unit
                )
            } else {
                let ((a_min, a_max), (b_min, b_max)) = (a.bound.si(), b.bound.si());
                let lo = [a_min, b_min]
                    .into_iter()
                    .flatten()
                    .fold(f64::NEG_INFINITY, f64::max);
                let hi = [a_max, b_max]
                    .into_iter()
                    .flatten()
                    .fold(f64::INFINITY, f64::min);
                if lo <= hi {
                    continue;
                }
                format!(
                    "no value satisfies both: needs >= {lo} and <= {hi} (SI base unit of {:?})",
                    a.bound.unit.map(Unit::dimension)
                )
            };
            let (strong, weak) = if a.precedence() >= b.precedence() {
                (a, b)
            } else {
                (b, a)
            };
            let mut tradeoffs = vec![format!(
                "change the design or the requirement behind {} ({:?}) to fit {}",
                weak.id, weak.kind, strong.id
            )];
            if strong.protected() {
                tradeoffs.push(format!(
                    "{} is a safety or approved limit: it may only change through human approval",
                    strong.id
                ));
            } else {
                tradeoffs.push(format!(
                    "relax {} after a human reviews its source ({})",
                    strong.id, strong.provenance.source
                ));
            }
            for c in [a, b] {
                if let Some(w) = &c.condition {
                    tradeoffs.push(format!("{} applies only when {w}", c.id));
                }
            }
            out.push(Conflict {
                id: format!("conflict:{}|{}", a.id, b.id),
                constraints: [a.id.clone(), b.id.clone()],
                quantity: a.quantity.clone(),
                explanation,
                tradeoffs,
            });
        }
    }
    out
}

// ------------------------------------------------------------------ calculations

#[derive(Debug, Clone, Serialize)]
pub struct CalcInput {
    pub name: String,
    pub value: f64,
    pub unit: Unit,
    /// Requirement id or design object the value came from.
    pub source: String,
    pub knowledge: Knowledge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    WithinLimit,
    OutsideLimit,
    NoLimit,
    NotComputed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalcState {
    /// Computed by eak-engineering from stated or confirmed inputs.
    Computed,
    /// Computed, but at least one input is only assumed.
    ComputedFromAssumptions,
    /// Inputs invalid or missing: no number is reported.
    NotComputed,
}

/// Directive §8: inputs, units, formula and source, assumptions, margins, result, limit
/// comparison, validation state and the revision it belongs to.
#[derive(Debug, Clone, Serialize)]
pub struct CalculationRecord {
    pub id: String,
    pub name: String,
    pub inputs: Vec<CalcInput>,
    pub formula: String,
    /// Function that computed it (`eak_engineering::calc::...`).
    pub method: String,
    pub formula_source: String,
    pub assumptions: Vec<String>,
    pub margin: Option<String>,
    pub result: Option<f64>,
    pub result_unit: Unit,
    pub error: Option<String>,
    pub limit: Option<Bound>,
    pub comparison: Comparison,
    pub state: CalcState,
    pub revision: u64,
}

// ------------------------------------------------------------------ risks

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    DesignUncertainty,
    MissingRequirement,
    IncompleteEvidence,
    ElectricalViolation,
    PhysicalViolation,
    Thermal,
    SignalIntegrity,
    Safety,
    Manufacturing,
    Lifecycle,
    Integration,
}

/// A multi-domain risk: the shared `eak_validation::RiskRecord` (ADR-0026; separate severity,
/// likelihood and detectability, no combined score) plus the class, title and domains.
#[derive(Debug, Clone, Serialize)]
pub struct Risk {
    #[serde(flatten)]
    pub record: RiskRecord,
    pub class: RiskClass,
    pub title: String,
    pub detail: String,
    pub domains: Vec<String>,
    /// How the axes were assigned (a fixed rule table, not a measurement).
    pub basis: String,
}

// ------------------------------------------------------------------ dependencies

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub why: String,
}

/// Directed "a change to `from` requires re-evaluating `to`" graph over prefixed ids
/// (`req:`, `con:`, `node:`, `calc:`, `check:`, `interaction:`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Dependencies {
    pub edges: BTreeSet<Edge>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImpactSet {
    pub changed: String,
    /// Every reachable id, grouped by prefix.
    pub affected: BTreeMap<String, Vec<String>>,
    /// The edges walked, so each entry is explained.
    pub path: Vec<Edge>,
}

impl Dependencies {
    pub fn add(&mut self, from: impl Into<String>, to: impl Into<String>, why: impl Into<String>) {
        let (from, to) = (from.into(), to.into());
        if from != to {
            self.edges.insert(Edge {
                from,
                to,
                why: why.into(),
            });
        }
    }

    pub fn knows(&self, id: &str) -> bool {
        self.edges.iter().any(|e| e.from == id || e.to == id)
    }

    /// Breadth-first impact set of a change to `start` (cycle-safe).
    pub fn impact(&self, start: &str) -> ImpactSet {
        let mut seen = BTreeSet::from([start.to_string()]);
        let mut queue = VecDeque::from([start.to_string()]);
        let mut path = vec![];
        while let Some(cur) = queue.pop_front() {
            for e in self.edges.iter().filter(|e| e.from == cur) {
                if seen.insert(e.to.clone()) {
                    path.push(e.clone());
                    queue.push_back(e.to.clone());
                }
            }
        }
        let mut affected: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for id in seen.into_iter().filter(|x| x != start) {
            let group = id.split(':').next().unwrap_or("other").to_string();
            affected.entry(group).or_default().push(id);
        }
        ImpactSet {
            changed: start.into(),
            affected,
            path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(value: Value) -> Requirement {
        Requirement {
            id: "R1".into(),
            description: "input voltage".into(),
            category: Category::Electrical,
            domains: vec!["power".into()],
            quantity: Some("input_voltage:VIN".into()),
            value,
            unit: Some(Unit::Volt),
            tolerance: Tolerance::None,
            priority: Priority::Must,
            source: SourceRef {
                kind: SourceKind::Assumption,
                reference: "agent".into(),
            },
            evidence: vec![],
            knowledge: Knowledge::Assumed,
            affects: vec![],
            constraints: vec![],
            validation_method: None,
            approval: ApprovalState::Pending,
        }
    }

    fn con(id: &str, kind: Kind, safety: bool, min: Option<f64>, max: Option<f64>) -> Constraint {
        Constraint {
            id: id.into(),
            description: String::new(),
            kind,
            condition: None,
            category: ConstraintCategory::Electrical,
            safety,
            approved: false,
            scope: "product".into(),
            quantity: "clearance:HV".into(),
            bound: Bound {
                min,
                max,
                unit: Some(Unit::Metre),
            },
            requirements: vec![],
            derived_from: vec![],
            provenance: Provenance {
                source: "test".into(),
                rule: "test".into(),
                applicability: "test".into(),
            },
        }
    }

    // Tests in this module are written, not run (code-reviewed only).
    #[test]
    fn assumptions_need_a_human_with_evidence() {
        let mut r = req(Value::Number { value: 12.0 });
        assert!(r
            .promote(Knowledge::Confirmed, &Promoter::Agent, Some("x"))
            .is_err());
        let det = Promoter::Deterministic {
            check: "erc".into(),
        };
        assert!(r.promote(Knowledge::Confirmed, &det, Some("x")).is_err());
        let human = Promoter::Human { name: "eng".into() };
        assert!(r.promote(Knowledge::Confirmed, &human, None).is_err());
        assert!(r
            .promote(Knowledge::Confirmed, &human, Some("datasheet p.3"))
            .is_ok());
        assert_eq!(r.approval, ApprovalState::Approved { by: "eng".into() });
        let mut u = req(Value::Unknown);
        u.knowledge = Knowledge::Unknown;
        assert!(u
            .promote(Knowledge::Assumed, &Promoter::Agent, None)
            .is_err());
    }

    #[test]
    fn safety_limit_cannot_be_weakened() {
        let old = con("c1", Kind::Hard, true, Some(0.003), None);
        let looser = con("c2", Kind::Soft, false, Some(0.002), None);
        assert!(weakens(&old, &looser).is_some());
        let tighter = con("c3", Kind::Soft, false, Some(0.004), None);
        assert!(weakens(&old, &tighter).is_none());
    }

    #[test]
    fn disjoint_bounds_conflict() {
        let a = con("a", Kind::Hard, false, Some(2.0), None);
        let b = con("b", Kind::Soft, false, None, Some(1.0));
        let c = find_conflicts(&[a, b], |x, y| x == y);
        assert_eq!(c.len(), 1);
        assert!(c[0].explanation.contains("no value satisfies both"));
    }

    #[test]
    fn impact_walks_transitively_and_survives_cycles() {
        let mut d = Dependencies::default();
        d.add("req:R1", "con:C1", "compiled");
        d.add("con:C1", "con:C2", "derived");
        d.add("con:C2", "req:R1", "cycle");
        let i = d.impact("req:R1");
        assert_eq!(i.affected["con"].len(), 2);
        assert_eq!(i.path.len(), 2);
    }
}
