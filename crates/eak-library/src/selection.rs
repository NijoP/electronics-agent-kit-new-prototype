//! Component selection: role-driven candidate search (M5.1), candidate comparison (M5.6) and
//! alternative / equivalence checks (M5.7). Library data only; nothing here changes a design.
//!
//! A [`FamilyRequirement`] states what one design role needs: the family searched for, the
//! search terms, and typed [`Criterion`]s, each on a stated specification, the category,
//! package or lifecycle text, the stated stock, or the unit price at the build quantity.
//!
//! Every criterion is evaluated from what the sources state, and only from that:
//! * a value no source states is [`Outcome::Unknown`], never assumed;
//! * a value only an AI proposed ([`SpecOrigin::AiNeedsReview`]) is `Unknown` until reviewed;
//! * values two sources state differently are `Unknown` with both values shown;
//! * a unit EAK cannot convert, or a unit of another quantity, is `Unknown`;
//! * stock and prices count only when the requirement sets how old an offer may be and the offer
//!   is younger than that; otherwise `Unknown`.
//!
//! A candidate is [`Verdict::Qualified`] only when every criterion passes. A requirement with no
//! criteria qualifies nothing. An equivalence check calls an alternative
//! [`EquivalenceVerdict::Equivalent`] only when package, pinout, pad map and every compared
//! electrical value are shown compatible; anything not shown is `Unknown`.

use crate::part::{
    OfferSnapshot, Part, PartKey, PartRecord, SourceId, Spec, SpecOrigin, SpecValue,
};
use crate::source::{resolve, PartQuery, PartSource, SourceOutcome};
use eak_core::ComponentDef;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// What a criterion is about, for grouping in a comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriterionKind {
    Electrical,
    Mechanical,
    Thermal,
    Supply,
    Cost,
}

/// The value a criterion reads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "subject", rename_all = "snake_case")]
pub enum Subject {
    /// A typed specification, by name (case-insensitive).
    Spec { name: String },
    /// The category text (a `one_of` value passes when the category contains it).
    Category,
    /// The package text (compared without case, spaces or separators).
    Package,
    /// The lifecycle text.
    Lifecycle,
    /// The largest stock stated by a fresh offer.
    Stock,
    /// The lowest unit price at the build quantity from a fresh offer; the rule's unit is the
    /// currency code.
    UnitPrice,
}

/// How a value is judged. Numeric rules carry their unit; EAK converts SI prefixes only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Rule {
    AtLeast {
        value: f64,
        #[serde(default)]
        unit: String,
    },
    AtMost {
        value: f64,
        #[serde(default)]
        unit: String,
    },
    OneOf {
        values: Vec<String>,
    },
}

/// One requirement on a candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    pub id: String,
    pub kind: CriterionKind,
    #[serde(flatten)]
    pub subject: Subject,
    pub rule: Rule,
    /// Where the criterion comes from (a requirement id, a calculation), for traceability.
    #[serde(default)]
    pub basis: Option<String>,
}

/// What one design role needs from a part family.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyRequirement {
    /// The design role, for example "U2: 3.3 V regulator from VBUS".
    pub role: String,
    /// The part family, for example "LDO regulator".
    pub family: String,
    /// Text sent to every ready source (an MPN, a family word). At least one.
    pub search_terms: Vec<String>,
    pub criteria: Vec<Criterion>,
    /// Build quantity, for stock and price.
    #[serde(default = "one")]
    pub quantity: u64,
    /// Oldest offer age, in seconds, that stock and price may come from. `None` makes every
    /// stock and price criterion `Unknown`.
    #[serde(default)]
    pub offer_max_age_s: Option<u64>,
    /// Requirement ids (EAK-IR requirements) this role serves, for traceability.
    #[serde(default)]
    pub requirement_ids: Vec<String>,
}

fn one() -> u64 {
    1
}

impl FamilyRequirement {
    /// Every problem with the requirement itself; empty when it can be evaluated.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.role.trim().is_empty() {
            out.push("role is empty".to_string());
        }
        if self.search_terms.iter().all(|t| t.trim().is_empty()) {
            out.push("at least one non-empty search term is needed".to_string());
        }
        if self.quantity == 0 {
            out.push("quantity must be at least 1".to_string());
        }
        let mut ids = BTreeSet::new();
        for c in &self.criteria {
            let id = c.id.trim();
            if id.is_empty() {
                out.push("a criterion has an empty id".to_string());
            } else if !ids.insert(id.to_string()) {
                out.push(format!("criterion id {id:?} is used twice"));
            }
            out.extend(
                criterion_problems(c)
                    .into_iter()
                    .map(|p| format!("{id}: {p}")),
            );
        }
        out
    }
}

/// Problems with one criterion; empty when it can be evaluated.
pub fn criterion_problems(c: &Criterion) -> Vec<String> {
    let mut out = Vec::new();
    let numeric = match &c.rule {
        Rule::AtLeast { value, unit } | Rule::AtMost { value, unit } => {
            if !value.is_finite() {
                out.push("the rule value is not a finite number".to_string());
            }
            Some(unit.as_str())
        }
        Rule::OneOf { values } => {
            if values.iter().all(|v| v.trim().is_empty()) {
                out.push("one_of needs at least one non-empty value".to_string());
            }
            None
        }
    };
    match (&c.subject, numeric) {
        (Subject::Spec { name }, numeric) => {
            if name.trim().is_empty() {
                out.push("the specification name is empty".to_string());
            }
            if let Some(unit) = numeric {
                if unit_scale(unit).is_none() {
                    out.push(format!("unit {unit:?} is not one EAK can convert"));
                }
            }
        }
        (Subject::Category | Subject::Package | Subject::Lifecycle, Some(_)) => {
            out.push("category, package and lifecycle take a one_of rule".to_string());
        }
        (Subject::Stock, _) if !matches!(c.rule, Rule::AtLeast { .. }) => {
            out.push("stock takes an at_least rule".to_string());
        }
        (Subject::UnitPrice, _) => match &c.rule {
            Rule::AtMost { unit, .. } if !unit.trim().is_empty() => {}
            _ => out.push("unit_price takes an at_most rule with a currency unit".to_string()),
        },
        _ => {}
    }
    out
}

/// Result of one criterion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
    Unknown,
}

/// One stated value a result rests on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub source: SourceId,
    /// The value as the source stated it, with its unit.
    pub value: String,
    /// Who stated a specification (absent for record fields and offers).
    #[serde(default)]
    pub origin: Option<SpecOrigin>,
    /// When the source's record or offer was retrieved, when known.
    #[serde(default)]
    pub retrieved_at_unix_s: Option<u64>,
}

/// One criterion evaluated on one candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CriterionResult {
    pub criterion_id: String,
    pub kind: CriterionKind,
    pub outcome: Outcome,
    /// Why, in words (always present, also for a pass).
    pub reason: String,
    pub evidence: Vec<Evidence>,
}

/// The overall state of a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Every criterion passed on stated evidence.
    Qualified,
    /// Nothing failed, but at least one criterion is unknown (or there were no criteria).
    Undetermined,
    /// At least one criterion failed.
    Rejected,
}

/// A part evaluated against a requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub key: PartKey,
    pub display_mpn: String,
    pub display_manufacturer: String,
    pub sources: Vec<SourceId>,
    pub package: Option<String>,
    pub results: Vec<CriterionResult>,
    pub verdict: Verdict,
    /// Fields whose stated values disagree between sources.
    pub conflicts: Vec<String>,
}

/// What the evaluation needs besides the parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalContext {
    pub quantity: u64,
    pub offer_max_age_s: Option<u64>,
    pub now_unix_s: u64,
}

impl EvalContext {
    pub fn of(req: &FamilyRequirement, now_unix_s: u64) -> EvalContext {
        EvalContext {
            quantity: req.quantity.max(1),
            offer_max_age_s: req.offer_max_age_s,
            now_unix_s,
        }
    }
}

/// Limitations every selection result carries.
pub const LIMITATIONS: &[&str] = &[
    "candidates come only from the sources asked; a part no ready source lists is not considered",
    "every value is as a source stated it; a value no source states is unknown, never assumed",
    "AI-proposed values count only after a person reviews them",
    "the direction of each criterion (at least / at most) is the requirement author's; EAK does not infer it",
    "units are converted by SI prefix only; other unit forms are unknown",
    "stock and price are a source's statement at its retrieval time, not a reservation",
];

fn limitations() -> Vec<String> {
    LIMITATIONS.iter().map(|s| s.to_string()).collect()
}

// ---------------------------------------------------------------------------------------------
// Units

/// `(base unit, factor to base)` for a unit EAK can convert: an SI base EAK knows, optionally
/// with one SI prefix. `""` is a dimensionless count.
pub fn unit_scale(unit: &str) -> Option<(&'static str, f64)> {
    fn base(u: &str) -> Option<&'static str> {
        Some(match u {
            "" => "",
            "V" => "V",
            "A" => "A",
            "W" => "W",
            "Ω" | "ohm" | "Ohm" | "ohms" => "ohm",
            "F" => "F",
            "H" => "H",
            "Hz" => "Hz",
            "s" => "s",
            "m" => "m",
            "g" => "g",
            "°C" | "degC" => "degC",
            "%" => "%",
            _ => return None,
        })
    }
    let u = unit.trim();
    if let Some(b) = base(u) {
        return Some((b, 1.0));
    }
    let mut chars = u.chars();
    let p = chars.next()?;
    let factor = match p {
        'p' => 1e-12,
        'n' => 1e-9,
        'u' | 'µ' | 'μ' => 1e-6,
        'm' => 1e-3,
        'k' => 1e3,
        'M' => 1e6,
        'G' => 1e9,
        _ => return None,
    };
    let b = base(chars.as_str())?;
    if matches!(b, "" | "degC" | "%") {
        return None;
    }
    Some((b, factor))
}

/// `value` in `from` expressed in `to`, when both are the same quantity.
pub fn convert(value: f64, from: &str, to: &str) -> Result<f64, String> {
    let (fb, ff) =
        unit_scale(from).ok_or_else(|| format!("unit {from:?} is not one EAK can convert"))?;
    let (tb, tf) =
        unit_scale(to).ok_or_else(|| format!("unit {to:?} is not one EAK can convert"))?;
    if fb != tb {
        return Err(format!(
            "stated in {from:?}, which is not the quantity of {to:?}"
        ));
    }
    Ok(value * ff / tf)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1e-30)
}

/// Compare text without case, spaces or separators ("SOT-23-5" == "sot23 5").
pub fn norm_text(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Evaluation

type Unknown = (String, Vec<Evidence>);
type Getter = fn(&PartRecord) -> Option<&String>;

fn result(
    c: &Criterion,
    outcome: Outcome,
    reason: String,
    evidence: Vec<Evidence>,
) -> CriterionResult {
    CriterionResult {
        criterion_id: c.id.clone(),
        kind: c.kind,
        outcome,
        reason,
        evidence,
    }
}

fn spec_evidence(s: &Spec, r: &PartRecord) -> Evidence {
    Evidence {
        source: r.provenance.source.clone(),
        value: s.display_value(),
        origin: Some(s.origin.clone()),
        retrieved_at_unix_s: r.provenance.retrieved_at_unix_s,
    }
}

/// Every specification named `name` (case-insensitive) across the part's records.
fn specs_named<'a>(part: &'a Part, name: &str) -> Vec<(&'a Spec, &'a PartRecord)> {
    let want = name.trim().to_lowercase();
    part.records
        .iter()
        .flat_map(|r| r.specs.iter().map(move |s| (s, r)))
        .filter(|(s, _)| s.name.trim().to_lowercase() == want)
        .collect()
}

/// The trusted numeric value of a specification in `unit`: one value every non-AI statement
/// agrees on. `Err` carries why it is unknown and the evidence seen.
fn trusted_number(part: &Part, name: &str, unit: &str) -> Result<(f64, Vec<Evidence>), Unknown> {
    let all = specs_named(part, name);
    let evidence: Vec<Evidence> = all.iter().map(|(s, r)| spec_evidence(s, r)).collect();
    let trusted: Vec<&Spec> = all
        .iter()
        .map(|(s, _)| *s)
        .filter(|s| !s.needs_review())
        .collect();
    if trusted.is_empty() {
        let why = if all.is_empty() {
            format!("no source states {name}")
        } else {
            format!("{name} is only AI-proposed and needs review")
        };
        return Err((why, evidence));
    }
    let mut values = Vec::new();
    for s in trusted {
        let (Some(v), SpecValue::Number { unit: from, .. }) = (s.as_f64(), &s.value) else {
            return Err((format!("{name} is stated as text, not a number"), evidence));
        };
        match convert(v, from, unit) {
            Ok(x) => values.push(x),
            Err(why) => return Err((format!("{name}: {why}"), evidence)),
        }
    }
    let first = values[0];
    if values.iter().any(|v| !close(*v, first)) {
        return Err((format!("sources disagree on {name}"), evidence));
    }
    Ok((first, evidence))
}

/// The one stated value of a record text field (all sources agree), or why it is unknown.
fn trusted_field(
    part: &Part,
    label: &str,
    get: Getter,
) -> Result<(String, Vec<Evidence>), Unknown> {
    let evidence: Vec<Evidence> = part
        .records
        .iter()
        .filter_map(|r| {
            get(r).map(|v| Evidence {
                source: r.provenance.source.clone(),
                value: v.clone(),
                origin: None,
                retrieved_at_unix_s: r.provenance.retrieved_at_unix_s,
            })
        })
        .collect();
    let Some(first) = evidence.first().map(|e| e.value.clone()) else {
        return Err((format!("no source states the {label}"), evidence));
    };
    if evidence
        .iter()
        .any(|e| norm_text(&e.value) != norm_text(&first))
    {
        return Err((format!("sources disagree on the {label}"), evidence));
    }
    Ok((first, evidence))
}

fn rec_category(r: &PartRecord) -> Option<&String> {
    r.category.as_ref()
}
fn rec_package(r: &PartRecord) -> Option<&String> {
    r.package.as_ref()
}
fn rec_lifecycle(r: &PartRecord) -> Option<&String> {
    r.lifecycle.as_ref()
}

fn numeric_judgement(v: f64, rule: &Rule, shown: &str) -> (Outcome, String) {
    match rule {
        Rule::AtLeast { value, unit } => {
            if v >= *value || close(v, *value) {
                (
                    Outcome::Pass,
                    format!("{shown} meets at least {value} {unit}"),
                )
            } else {
                (
                    Outcome::Fail,
                    format!("{shown} is below the minimum {value} {unit}"),
                )
            }
        }
        Rule::AtMost { value, unit } => {
            if v <= *value || close(v, *value) {
                (
                    Outcome::Pass,
                    format!("{shown} meets at most {value} {unit}"),
                )
            } else {
                (
                    Outcome::Fail,
                    format!("{shown} is above the maximum {value} {unit}"),
                )
            }
        }
        Rule::OneOf { .. } => (Outcome::Unknown, "a one_of rule needs text".to_string()),
    }
}

fn one_of(values: &[String], got: &str, contains: bool) -> bool {
    let g = norm_text(got);
    values
        .iter()
        .map(|v| norm_text(v))
        .filter(|v| !v.is_empty())
        .any(|v| if contains { g.contains(&v) } else { g == v })
}

fn offer_evidence(o: &OfferSnapshot) -> Evidence {
    let stock = o
        .stock
        .map_or("stock not stated".to_string(), |s| format!("stock {s}"));
    let pn = o.supplier_pn.as_deref().unwrap_or("-");
    Evidence {
        source: o.source.clone(),
        value: format!("{pn}: {stock}, {} price breaks", o.breaks.len()),
        origin: None,
        retrieved_at_unix_s: Some(o.retrieved_at_unix_s),
    }
}

/// Whether an offer is young enough: retrieved no later than now and at most `max_age_s` ago.
pub fn offer_is_fresh(o: &OfferSnapshot, max_age_s: u64, now_unix_s: u64) -> bool {
    o.retrieved_at_unix_s <= now_unix_s && now_unix_s - o.retrieved_at_unix_s <= max_age_s
}

/// Offers young enough to use, or why none is.
fn fresh_offers<'a>(part: &'a Part, ctx: &EvalContext) -> Result<Vec<&'a OfferSnapshot>, Unknown> {
    let offers = part.offers();
    let evidence: Vec<Evidence> = offers.iter().copied().map(offer_evidence).collect();
    if offers.is_empty() {
        return Err(("no source states an offer".to_string(), evidence));
    }
    let Some(max_age) = ctx.offer_max_age_s else {
        return Err((
            "the requirement sets no offer_max_age_s, so no stock or price is used".to_string(),
            evidence,
        ));
    };
    let fresh: Vec<&OfferSnapshot> = offers
        .into_iter()
        .filter(|o| offer_is_fresh(o, max_age, ctx.now_unix_s))
        .collect();
    if fresh.is_empty() {
        return Err((
            format!("every offer is older than {max_age} s (or dated in the future)"),
            evidence,
        ));
    }
    Ok(fresh)
}

/// Unit price of an offer at `quantity`, in `currency`, from its stated breaks: the break with
/// the largest minimum quantity not above `quantity`. `None` when no such break is stated.
pub fn unit_price_at(o: &OfferSnapshot, quantity: u64, currency: &str) -> Option<f64> {
    o.breaks
        .iter()
        .filter(|b| b.currency.trim().eq_ignore_ascii_case(currency.trim()))
        .filter(|b| b.min_quantity <= quantity)
        .max_by_key(|b| b.min_quantity)
        .and_then(|b| b.unit_price.trim().parse::<f64>().ok())
        .filter(|p| p.is_finite() && *p >= 0.0)
}

fn best_price(fresh: &[&OfferSnapshot], quantity: u64, currency: &str) -> Option<f64> {
    fresh
        .iter()
        .filter_map(|o| unit_price_at(o, quantity, currency))
        .min_by(f64::total_cmp)
}

/// Evaluate one criterion on one part.
pub fn evaluate(part: &Part, c: &Criterion, ctx: &EvalContext) -> CriterionResult {
    let problems = criterion_problems(c);
    if !problems.is_empty() {
        let why = format!("criterion not usable: {}", problems.join("; "));
        return result(c, Outcome::Unknown, why, vec![]);
    }
    match (&c.subject, &c.rule) {
        (Subject::Spec { name }, Rule::AtLeast { unit, .. } | Rule::AtMost { unit, .. }) => {
            match trusted_number(part, name, unit) {
                Ok((v, ev)) => {
                    let (o, why) = numeric_judgement(v, &c.rule, &format!("{name} = {v} {unit}"));
                    result(c, o, why, ev)
                }
                Err((why, ev)) => result(c, Outcome::Unknown, why, ev),
            }
        }
        (Subject::Spec { name }, Rule::OneOf { values }) => {
            let all = specs_named(part, name);
            let ev: Vec<Evidence> = all.iter().map(|(s, r)| spec_evidence(s, r)).collect();
            let trusted: Vec<String> = all
                .iter()
                .filter(|(s, _)| !s.needs_review())
                .map(|(s, _)| s.display_value())
                .collect();
            let (outcome, why) = match trusted.first() {
                None if all.is_empty() => (Outcome::Unknown, format!("no source states {name}")),
                None => (
                    Outcome::Unknown,
                    format!("{name} is only AI-proposed and needs review"),
                ),
                Some(first) if trusted.iter().any(|t| norm_text(t) != norm_text(first)) => {
                    (Outcome::Unknown, format!("sources disagree on {name}"))
                }
                Some(first) if one_of(values, first, false) => (
                    Outcome::Pass,
                    format!("{name} = {first} is one of {values:?}"),
                ),
                Some(first) => (
                    Outcome::Fail,
                    format!("{name} = {first} is not one of {values:?}"),
                ),
            };
            result(c, outcome, why, ev)
        }
        (Subject::Category | Subject::Package | Subject::Lifecycle, Rule::OneOf { values }) => {
            let (label, get, contains): (&str, Getter, bool) = match c.subject {
                Subject::Category => ("category", rec_category as Getter, true),
                Subject::Package => ("package", rec_package as Getter, false),
                _ => ("lifecycle", rec_lifecycle as Getter, false),
            };
            match trusted_field(part, label, get) {
                Ok((v, ev)) if one_of(values, &v, contains) => result(
                    c,
                    Outcome::Pass,
                    format!("{label} {v:?} matches {values:?}"),
                    ev,
                ),
                Ok((v, ev)) => result(
                    c,
                    Outcome::Fail,
                    format!("{label} {v:?} does not match {values:?}"),
                    ev,
                ),
                Err((why, ev)) => result(c, Outcome::Unknown, why, ev),
            }
        }
        (Subject::Stock, Rule::AtLeast { .. }) => match fresh_offers(part, ctx) {
            Err((why, ev)) => result(c, Outcome::Unknown, why, ev),
            Ok(fresh) => {
                let ev: Vec<Evidence> = fresh.iter().copied().map(offer_evidence).collect();
                match fresh.iter().filter_map(|o| o.stock).max() {
                    None => result(
                        c,
                        Outcome::Unknown,
                        "no fresh offer states its stock".to_string(),
                        ev,
                    ),
                    Some(s) => {
                        let (o, why) =
                            numeric_judgement(s as f64, &c.rule, &format!("stated stock {s}"));
                        result(c, o, why, ev)
                    }
                }
            }
        },
        (Subject::UnitPrice, Rule::AtMost { unit, .. }) => match fresh_offers(part, ctx) {
            Err((why, ev)) => result(c, Outcome::Unknown, why, ev),
            Ok(fresh) => {
                let ev: Vec<Evidence> = fresh.iter().copied().map(offer_evidence).collect();
                match best_price(&fresh, ctx.quantity, unit) {
                    None => result(
                        c,
                        Outcome::Unknown,
                        format!(
                            "no fresh offer states a {unit} price break at quantity {}",
                            ctx.quantity
                        ),
                        ev,
                    ),
                    Some(p) => {
                        let shown = format!("unit price {p} {unit} at quantity {}", ctx.quantity);
                        let (o, why) = numeric_judgement(p, &c.rule, &shown);
                        result(c, o, why, ev)
                    }
                }
            }
        },
        _ => result(
            c,
            Outcome::Unknown,
            "criterion not usable".to_string(),
            vec![],
        ),
    }
}

fn verdict_of(results: &[CriterionResult]) -> Verdict {
    if results.iter().any(|r| r.outcome == Outcome::Fail) {
        Verdict::Rejected
    } else if results.is_empty() || results.iter().any(|r| r.outcome == Outcome::Unknown) {
        Verdict::Undetermined
    } else {
        Verdict::Qualified
    }
}

/// Evaluate every criterion on one part.
pub fn assess(part: &Part, criteria: &[Criterion], ctx: &EvalContext) -> Candidate {
    let results: Vec<CriterionResult> = criteria.iter().map(|c| evaluate(part, c, ctx)).collect();
    let package = trusted_field(part, "package", rec_package)
        .ok()
        .map(|(v, _)| v);
    Candidate {
        key: part.key.clone(),
        display_mpn: part.display_mpn().to_string(),
        display_manufacturer: part.display_manufacturer().to_string(),
        sources: part.sources(),
        package,
        verdict: verdict_of(&results),
        results,
        conflicts: part.conflicts().into_iter().map(|f| f.field).collect(),
    }
}

// ---------------------------------------------------------------------------------------------
// M5.1 candidate search

/// The answer of a role-driven search.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateSearch {
    pub role: String,
    pub family: String,
    pub requirement_ids: Vec<String>,
    /// Problems with the requirement; when not empty nothing was searched.
    pub requirement_problems: Vec<String>,
    /// Qualified first, then undetermined, then rejected; by key inside each group.
    pub candidates: Vec<Candidate>,
    /// Sources skipped, failed, or with listings they could not key.
    pub sources: Vec<SourceOutcome>,
    pub limitations: Vec<String>,
}

/// Search every ready source with each term, merge what comes back by part key, and evaluate
/// every part against the requirement.
pub fn search_candidates(
    sources: &[&dyn PartSource],
    req: &FamilyRequirement,
    now_unix_s: u64,
) -> CandidateSearch {
    let mut out = CandidateSearch {
        role: req.role.clone(),
        family: req.family.clone(),
        requirement_ids: req.requirement_ids.clone(),
        requirement_problems: req.problems(),
        candidates: Vec::new(),
        sources: Vec::new(),
        limitations: limitations(),
    };
    if !out.requirement_problems.is_empty() {
        return out;
    }
    let mut seen: BTreeSet<(SourceId, PartKey)> = BTreeSet::new();
    let mut records: Vec<PartRecord> = Vec::new();
    for term in req
        .search_terms
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
    {
        let r = resolve(sources, &PartQuery::new(term));
        let found = r
            .parts
            .into_iter()
            .flat_map(|p| p.part.records)
            .chain(r.suggestions);
        for rec in found {
            if seen.insert((rec.provenance.source.clone(), rec.key.clone())) {
                records.push(rec);
            }
        }
        for s in r.sources {
            if !out.sources.contains(&s) {
                out.sources.push(s);
            }
        }
    }
    let ctx = EvalContext::of(req, now_unix_s);
    out.candidates = Part::merge(records)
        .iter()
        .map(|p| assess(p, &req.criteria, &ctx))
        .collect();
    out.candidates
        .sort_by(|a, b| a.verdict.cmp(&b.verdict).then_with(|| a.key.cmp(&b.key)));
    out
}

// ---------------------------------------------------------------------------------------------
// M5.6 comparison

/// Pass / fail / unknown counts of one criterion kind on one candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct KindTally {
    pub pass: usize,
    pub fail: usize,
    pub unknown: usize,
}

/// One candidate in a comparison.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ComparedCandidate {
    pub candidate: Candidate,
    /// Tally per criterion kind (kinds with no criterion are absent).
    pub by_kind: BTreeMap<CriterionKind, KindTally>,
    /// Lowest fresh unit price at the build quantity, in the first unit-price criterion's
    /// currency, when one is stated.
    pub unit_price: Option<f64>,
    /// 1-based rank among qualified candidates; `None` when not qualified.
    pub rank: Option<usize>,
}

/// A side-by-side comparison of chosen parts against one requirement.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comparison {
    pub role: String,
    pub requirement_problems: Vec<String>,
    pub criteria: Vec<Criterion>,
    pub candidates: Vec<ComparedCandidate>,
    /// How the ranking was made, in words.
    pub ranking_basis: String,
    pub limitations: Vec<String>,
}

fn part_price(part: &Part, criteria: &[Criterion], ctx: &EvalContext) -> Option<f64> {
    let currency = criteria.iter().find_map(|c| match (&c.subject, &c.rule) {
        (Subject::UnitPrice, Rule::AtMost { unit, .. }) => Some(unit.as_str()),
        _ => None,
    })?;
    let fresh = fresh_offers(part, ctx).ok()?;
    best_price(&fresh, ctx.quantity, currency)
}

/// Compare parts against a requirement. Only qualified parts are ranked: by lowest stated unit
/// price at the build quantity when every qualified part has one, otherwise by key (and the
/// basis says so). Nothing undetermined or rejected is ranked.
pub fn compare(parts: &[Part], req: &FamilyRequirement, now_unix_s: u64) -> Comparison {
    let ctx = EvalContext::of(req, now_unix_s);
    let problems = req.problems();
    let mut rows: Vec<ComparedCandidate> = parts
        .iter()
        .map(|p| {
            let candidate = assess(p, &req.criteria, &ctx);
            let mut by_kind: BTreeMap<CriterionKind, KindTally> = BTreeMap::new();
            for r in &candidate.results {
                let t = by_kind.entry(r.kind).or_default();
                match r.outcome {
                    Outcome::Pass => t.pass += 1,
                    Outcome::Fail => t.fail += 1,
                    Outcome::Unknown => t.unknown += 1,
                }
            }
            ComparedCandidate {
                unit_price: part_price(p, &req.criteria, &ctx),
                candidate,
                by_kind,
                rank: None,
            }
        })
        .collect();
    let mut order: Vec<usize> = (0..rows.len())
        .filter(|i| problems.is_empty() && rows[*i].candidate.verdict == Verdict::Qualified)
        .collect();
    let all_priced = !order.is_empty() && order.iter().all(|i| rows[*i].unit_price.is_some());
    order.sort_by(|a, b| {
        let by_key = rows[*a].candidate.key.cmp(&rows[*b].candidate.key);
        if all_priced {
            let pa = rows[*a].unit_price.unwrap_or(f64::MAX);
            let pb = rows[*b].unit_price.unwrap_or(f64::MAX);
            pa.total_cmp(&pb).then(by_key)
        } else {
            by_key
        }
    });
    for (n, i) in order.iter().enumerate() {
        rows[*i].rank = Some(n + 1);
    }
    let ranking_basis = if !problems.is_empty() {
        "not ranked: the requirement has problems".to_string()
    } else if order.is_empty() {
        "not ranked: no candidate is qualified on stated evidence".to_string()
    } else if all_priced {
        format!(
            "qualified candidates by lowest fresh unit price at quantity {}",
            ctx.quantity
        )
    } else {
        "qualified candidates by part key: not every qualified candidate has a fresh unit price, so cost is not compared".to_string()
    };
    Comparison {
        role: req.role.clone(),
        requirement_problems: problems,
        criteria: req.criteria.clone(),
        candidates: rows,
        ranking_basis,
        limitations: limitations(),
    }
}

// ---------------------------------------------------------------------------------------------
// M5.7 equivalence

/// State of one equivalence aspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Compatible,
    Incompatible,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EquivalenceCheck {
    /// `identity`, `package`, `pinout`, `pad_map`, `spec:<name>`, `criterion:<id>` or
    /// `electrical_evidence`.
    pub aspect: String,
    pub state: CheckState,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquivalenceVerdict {
    /// Every aspect shown compatible on stated evidence.
    Equivalent,
    /// At least one aspect shown incompatible.
    NotEquivalent,
    /// Nothing incompatible shown, but at least one aspect is not established.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Equivalence {
    pub original: PartKey,
    pub alternative: PartKey,
    pub verdict: EquivalenceVerdict,
    pub checks: Vec<EquivalenceCheck>,
    pub limitations: Vec<String>,
}

fn check(aspect: &str, state: CheckState, detail: String) -> EquivalenceCheck {
    EquivalenceCheck {
        aspect: aspect.to_string(),
        state,
        detail,
    }
}

fn state_of(o: Outcome) -> CheckState {
    match o {
        Outcome::Pass => CheckState::Compatible,
        Outcome::Fail => CheckState::Incompatible,
        Outcome::Unknown => CheckState::Unknown,
    }
}

type PinTable = BTreeMap<String, (String, String)>;

fn pin_table(d: &ComponentDef) -> PinTable {
    d.symbol
        .pins
        .iter()
        .map(|p| {
            let ty = serde_json::to_value(p.electrical_type)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default();
            (
                p.number.trim().to_string(),
                (p.name.trim().to_lowercase(), ty),
            )
        })
        .collect()
}

fn pinout_checks(a: Option<&ComponentDef>, b: Option<&ComponentDef>) -> Vec<EquivalenceCheck> {
    let (Some(a), Some(b)) = (a, b) else {
        let why = "a component definition for both parts is needed to compare pins";
        return vec![
            check("pinout", CheckState::Unknown, why.to_string()),
            check("pad_map", CheckState::Unknown, why.to_string()),
        ];
    };
    let (pa, pb) = (pin_table(a), pin_table(b));
    let show = |m: &PinTable, n: &str| {
        m.get(n)
            .map_or("absent".to_string(), |(name, ty)| format!("{name} ({ty})"))
    };
    let numbers: BTreeSet<&String> = pa.keys().chain(pb.keys()).collect();
    let diffs: Vec<String> = numbers
        .into_iter()
        .filter(|n| pa.get(*n) != pb.get(*n))
        .map(|n| {
            format!(
                "pin {n}: {} vs {}",
                show(&pa, n.as_str()),
                show(&pb, n.as_str())
            )
        })
        .collect();
    let pinout = if diffs.is_empty() {
        check(
            "pinout",
            CheckState::Compatible,
            format!(
                "{} pins match by number, name and electrical type",
                pa.len()
            ),
        )
    } else {
        let shown: Vec<String> = diffs.into_iter().take(10).collect();
        check("pinout", CheckState::Incompatible, shown.join("; "))
    };
    let pads = |d: &ComponentDef| -> BTreeSet<String> {
        d.footprint
            .pads
            .iter()
            .map(|p| p.number.trim().to_string())
            .collect()
    };
    let pad_map = if a.pin_map == b.pin_map && pads(a) == pads(b) {
        check(
            "pad_map",
            CheckState::Compatible,
            "the same pin-to-pad map and pad numbers".to_string(),
        )
    } else {
        check(
            "pad_map",
            CheckState::Incompatible,
            "the pin-to-pad map or the pad numbers differ".to_string(),
        )
    };
    vec![pinout, pad_map]
}

fn package_check(original: &Part, alternative: &Part) -> EquivalenceCheck {
    match (
        trusted_field(original, "package", rec_package),
        trusted_field(alternative, "package", rec_package),
    ) {
        (Ok((a, _)), Ok((b, _))) if norm_text(&a) == norm_text(&b) => {
            check("package", CheckState::Compatible, format!("both {a}"))
        }
        (Ok((a, _)), Ok((b, _))) => {
            check("package", CheckState::Incompatible, format!("{a} vs {b}"))
        }
        (Err((why, _)), _) => check("package", CheckState::Unknown, format!("original: {why}")),
        (_, Err((why, _))) => check(
            "package",
            CheckState::Unknown,
            format!("alternative: {why}"),
        ),
    }
}

/// The single trusted text of a specification, when every non-AI statement agrees.
fn trusted_text(p: &Part, name: &str) -> Option<String> {
    let vals: BTreeSet<String> = specs_named(p, name)
        .into_iter()
        .filter(|(s, _)| !s.needs_review())
        .map(|(s, _)| norm_text(&s.display_value()))
        .collect();
    if vals.len() == 1 {
        vals.into_iter().next()
    } else {
        None
    }
}

fn spec_check(
    original: &Part,
    alternative: &Part,
    name: &str,
    covering: Option<&Criterion>,
    ctx: &EvalContext,
) -> (CheckState, String) {
    let unit = specs_named(original, name)
        .into_iter()
        .find(|(s, _)| !s.needs_review())
        .and_then(|(s, _)| match &s.value {
            SpecValue::Number { unit, .. } => Some(unit.clone()),
            SpecValue::Text { .. } => None,
        });
    let Some(unit) = unit else {
        return match (
            trusted_text(original, name),
            trusted_text(alternative, name),
        ) {
            (Some(a), Some(b)) if a == b => {
                (CheckState::Compatible, "the same stated text".to_string())
            }
            (Some(_), Some(_)) => (
                CheckState::Unknown,
                "the stated text differs; needs engineering review".to_string(),
            ),
            _ => (
                CheckState::Unknown,
                "not stated once and consistently by both parts".to_string(),
            ),
        };
    };
    match (
        trusted_number(original, name, &unit),
        trusted_number(alternative, name, &unit),
    ) {
        (Ok((a, _)), Ok((b, _))) if close(a, b) => {
            (CheckState::Compatible, format!("both {a} {unit}"))
        }
        (Ok((a, _)), Ok((b, _))) => match covering {
            Some(c) => {
                let r = evaluate(alternative, c, ctx);
                (
                    state_of(r.outcome),
                    format!("{a} vs {b} {unit}; criterion {}: {}", c.id, r.reason),
                )
            }
            None => (
                CheckState::Unknown,
                format!("{a} vs {b} {unit}; no criterion says which way is acceptable, so it needs engineering review"),
            ),
        },
        (Err((why, _)), _) => (CheckState::Unknown, format!("original: {why}")),
        (_, Err((why, _))) => (CheckState::Unknown, format!("alternative: {why}")),
    }
}

/// Whether `alternative` can replace `original`. `criteria` (optional, usually the role's) decide
/// whether a differing specification is acceptable; without a criterion a difference stays
/// `Unknown`, because EAK does not guess which direction is safe.
pub fn check_equivalence(
    original: &Part,
    alternative: &Part,
    original_def: Option<&ComponentDef>,
    alternative_def: Option<&ComponentDef>,
    criteria: &[Criterion],
    ctx: &EvalContext,
) -> Equivalence {
    let mut checks = Vec::new();
    let same_part = original.key == alternative.key;
    if same_part {
        checks.push(check(
            "identity",
            CheckState::Compatible,
            "the same manufacturer and MPN".to_string(),
        ));
    }
    checks.push(package_check(original, alternative));
    checks.extend(pinout_checks(original_def, alternative_def));

    let mut electrical = 0usize;
    let covered: BTreeMap<String, &Criterion> = criteria
        .iter()
        .filter_map(|c| match &c.subject {
            Subject::Spec { name } => Some((name.trim().to_lowercase(), c)),
            _ => None,
        })
        .collect();
    let names: BTreeSet<String> = original
        .records
        .iter()
        .flat_map(|r| &r.specs)
        .filter(|s| !s.needs_review())
        .map(|s| s.name.trim().to_string())
        .collect();
    for name in &names {
        let covering = covered.get(&name.to_lowercase()).copied();
        let (state, detail) = spec_check(original, alternative, name, covering, ctx);
        if state == CheckState::Compatible {
            electrical += 1;
        }
        checks.push(check(&format!("spec:{name}"), state, detail));
    }
    for c in criteria {
        let r = evaluate(alternative, c, ctx);
        let state = state_of(r.outcome);
        if state == CheckState::Compatible && c.kind == CriterionKind::Electrical {
            electrical += 1;
        }
        checks.push(check(&format!("criterion:{}", c.id), state, r.reason));
    }
    if electrical == 0 && !same_part {
        checks.push(check(
            "electrical_evidence",
            CheckState::Unknown,
            "no stated electrical value or electrical criterion was shown compatible".to_string(),
        ));
    }
    let verdict = if checks.iter().any(|c| c.state == CheckState::Incompatible) {
        EquivalenceVerdict::NotEquivalent
    } else if checks.iter().any(|c| c.state == CheckState::Unknown) {
        EquivalenceVerdict::Unknown
    } else {
        EquivalenceVerdict::Equivalent
    };
    let mut limitations = limitations();
    limitations.push(
        "specifications are compared by name; a value stated under another name is not matched"
            .to_string(),
    );
    limitations.push("land-pattern geometry is compared only through the definitions' pad numbers; run footprint verification for geometry".to_string());
    Equivalence {
        original: original.key.clone(),
        alternative: alternative.key.clone(),
        verdict,
        checks,
        limitations,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::part::{PriceBreak, Provenance};
    use crate::source::{Capabilities, SourceAnswer, SourceError, SourceKind, SourceStatus};

    const NOW: u64 = 1_800_000_000;

    fn prov(src: &str) -> Provenance {
        Provenance {
            source: SourceId::new(src),
            reference: "fixture".into(),
            retrieved_at_unix_s: Some(NOW - 10),
            raw_sha256: None,
        }
    }

    fn origin(src: &str) -> SpecOrigin {
        SpecOrigin::Source(SourceId::new(src))
    }

    fn ldo(mpn: &str, src: &str, vin_max: &str, iout_ma: &str, package: &str) -> PartRecord {
        let mut r = PartRecord::new("Maker", mpn, prov(src)).unwrap();
        r.category = Some("Power / LDO regulator".into());
        r.package = Some(package.into());
        r.description = Some("LDO regulator".into());
        r.specs
            .push(Spec::number("Vin max", vin_max, "V", origin(src)).unwrap());
        r.specs
            .push(Spec::number("Iout max", iout_ma, "mA", origin(src)).unwrap());
        r
    }

    fn crit(id: &str, kind: CriterionKind, subject: Subject, rule: Rule) -> Criterion {
        Criterion {
            id: id.into(),
            kind,
            subject,
            rule,
            basis: None,
        }
    }

    fn spec(name: &str) -> Subject {
        Subject::Spec { name: name.into() }
    }

    fn at_least(value: f64, unit: &str) -> Rule {
        Rule::AtLeast {
            value,
            unit: unit.into(),
        }
    }

    fn req(criteria: Vec<Criterion>) -> FamilyRequirement {
        FamilyRequirement {
            role: "U2 3V3 regulator".into(),
            family: "LDO regulator".into(),
            search_terms: vec!["LDO".into()],
            criteria,
            quantity: 100,
            offer_max_age_s: Some(3600),
            requirement_ids: vec!["REQ-PWR-3V3".into()],
        }
    }

    fn electrical() -> Vec<Criterion> {
        vec![
            crit(
                "vin",
                CriterionKind::Electrical,
                spec("Vin max"),
                at_least(5.5, "V"),
            ),
            crit(
                "iout",
                CriterionKind::Electrical,
                spec("Iout max"),
                at_least(0.3, "A"),
            ),
            crit(
                "pkg",
                CriterionKind::Mechanical,
                Subject::Package,
                Rule::OneOf {
                    values: vec!["SOT-23-5".into()],
                },
            ),
        ]
    }

    struct Fixture(Vec<PartRecord>, SourceStatus);

    impl PartSource for Fixture {
        fn id(&self) -> SourceId {
            SourceId::new("fixture")
        }
        fn kind(&self) -> SourceKind {
            SourceKind::Local
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::default()
        }
        fn status(&self) -> SourceStatus {
            self.1.clone()
        }
        fn terms(&self) -> String {
            "test fixture".into()
        }
        fn search(&self, q: &PartQuery) -> Result<SourceAnswer, SourceError> {
            let t = q.text.to_lowercase();
            Ok(SourceAnswer {
                records: self
                    .0
                    .iter()
                    .filter(|r| {
                        r.description
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&t)
                    })
                    .cloned()
                    .collect(),
                unkeyed: vec![],
            })
        }
        fn fetch(&self, key: &PartKey) -> Result<Option<PartRecord>, SourceError> {
            Ok(self.0.iter().find(|r| &r.key == key).cloned())
        }
    }

    #[test]
    fn units_convert_by_si_prefix_only() {
        assert_eq!(unit_scale("mA"), Some(("A", 1e-3)));
        assert_eq!(unit_scale("kΩ"), Some(("ohm", 1e3)));
        assert_eq!(unit_scale("m"), Some(("m", 1.0)));
        assert_eq!(unit_scale("mm"), Some(("m", 1e-3)));
        assert_eq!(unit_scale("°C"), Some(("degC", 1.0)));
        assert_eq!(unit_scale("m°C"), None);
        assert_eq!(unit_scale("V/us"), None);
        assert!(close(convert(300.0, "mA", "A").unwrap(), 0.3));
        assert!(convert(1.0, "V", "A").is_err());
    }

    #[test]
    fn role_search_qualifies_only_on_stated_evidence() {
        let good = ldo("LDO-A", "fixture", "6", "300", "SOT-23-5");
        let weak = ldo("LDO-B", "fixture", "6", "150", "SOT-23-5");
        let mut bare = PartRecord::new("Maker", "LDO-C", prov("fixture")).unwrap();
        bare.description = Some("LDO regulator".into());
        let src = Fixture(vec![good, weak, bare], SourceStatus::Ready);
        let s = search_candidates(&[&src], &req(electrical()), NOW);
        assert!(s.requirement_problems.is_empty());
        let v: Vec<(&str, Verdict)> = s
            .candidates
            .iter()
            .map(|c| (c.display_mpn.as_str(), c.verdict))
            .collect();
        assert_eq!(
            v,
            vec![
                ("LDO-A", Verdict::Qualified),
                ("LDO-C", Verdict::Undetermined),
                ("LDO-B", Verdict::Rejected)
            ]
        );
        assert!(
            s.candidates[1]
                .results
                .iter()
                .all(|r| r.outcome == Outcome::Unknown),
            "nothing stated, nothing assumed"
        );
        assert_eq!(s.requirement_ids, vec!["REQ-PWR-3V3".to_string()]);
    }

    #[test]
    fn a_source_that_is_not_ready_is_reported_and_not_asked() {
        let src = Fixture(
            vec![ldo("LDO-A", "fixture", "6", "300", "SOT-23-5")],
            SourceStatus::NeedsSetup("key".into()),
        );
        let s = search_candidates(&[&src], &req(electrical()), NOW);
        assert!(s.candidates.is_empty());
        assert_eq!(s.sources.len(), 1);
    }

    #[test]
    fn a_bad_requirement_searches_nothing() {
        let mut r = req(vec![crit(
            "x",
            CriterionKind::Mechanical,
            Subject::Package,
            at_least(1.0, ""),
        )]);
        r.search_terms = vec![" ".into()];
        let src = Fixture(
            vec![ldo("LDO-A", "fixture", "6", "300", "SOT-23-5")],
            SourceStatus::Ready,
        );
        let s = search_candidates(&[&src], &r, NOW);
        assert_eq!(
            s.requirement_problems.len(),
            2,
            "{:?}",
            s.requirement_problems
        );
        assert!(s.candidates.is_empty());
    }

    #[test]
    fn no_criteria_qualifies_nothing() {
        let part = Part::merge(vec![ldo("LDO-A", "s1", "6", "300", "SOT-23-5")]).remove(0);
        let c = assess(&part, &[], &EvalContext::of(&req(vec![]), NOW));
        assert_eq!(c.verdict, Verdict::Undetermined);
    }

    #[test]
    fn ai_values_disagreements_and_foreign_units_are_unknown() {
        let mut a = PartRecord::new("Maker", "P1", prov("s1")).unwrap();
        a.specs
            .push(Spec::number("Vin max", "6", "V", SpecOrigin::AiNeedsReview).unwrap());
        a.specs
            .push(Spec::number("Iout max", "300", "mA", origin("s1")).unwrap());
        a.specs
            .push(Spec::number("Tj max", "125", "°C", origin("s1")).unwrap());
        let mut b = PartRecord::new("Maker", "P1", prov("s2")).unwrap();
        b.specs
            .push(Spec::number("Iout max", "0.25", "A", origin("s2")).unwrap());
        b.specs
            .push(Spec::number("Tj max", "125", "K", origin("s2")).unwrap());
        let part = Part::merge(vec![a, b]).remove(0);
        let ctx = EvalContext::of(&req(vec![]), NOW);
        let vin = evaluate(&part, &electrical()[0], &ctx);
        assert_eq!(vin.outcome, Outcome::Unknown);
        assert!(vin.reason.contains("AI-proposed"), "{}", vin.reason);
        let iout = evaluate(&part, &electrical()[1], &ctx);
        assert_eq!(iout.outcome, Outcome::Unknown);
        assert!(iout.reason.contains("disagree"), "{}", iout.reason);
        assert_eq!(iout.evidence.len(), 2);
        let tj = crit(
            "tj",
            CriterionKind::Thermal,
            spec("Tj max"),
            at_least(125.0, "°C"),
        );
        assert_eq!(evaluate(&part, &tj, &ctx).outcome, Outcome::Unknown);
    }

    #[test]
    fn converted_values_at_the_limit_pass() {
        let part = Part::merge(vec![ldo("P", "s1", "5.5", "300", "SOT-23-5")]).remove(0);
        let ctx = EvalContext::of(&req(vec![]), NOW);
        let c = crit(
            "i",
            CriterionKind::Electrical,
            spec("iout MAX"),
            Rule::AtMost {
                value: 0.3,
                unit: "A".into(),
            },
        );
        assert_eq!(evaluate(&part, &c, &ctx).outcome, Outcome::Pass);
        assert_eq!(
            evaluate(&part, &electrical()[0], &ctx).outcome,
            Outcome::Pass
        );
    }

    fn offer(src: &str, stock: Option<u64>, age: u64, breaks: &[(u64, &str)]) -> OfferSnapshot {
        OfferSnapshot {
            source: SourceId::new(src),
            supplier_pn: Some("C123".into()),
            stock,
            breaks: breaks
                .iter()
                .map(|(q, p)| PriceBreak {
                    min_quantity: *q,
                    unit_price: p.to_string(),
                    currency: "USD".into(),
                })
                .collect(),
            retrieved_at_unix_s: NOW - age,
            terms_note: "fixture".into(),
        }
    }

    fn supply() -> Vec<Criterion> {
        vec![
            crit(
                "stock",
                CriterionKind::Supply,
                Subject::Stock,
                at_least(100.0, ""),
            ),
            crit(
                "price",
                CriterionKind::Cost,
                Subject::UnitPrice,
                Rule::AtMost {
                    value: 0.20,
                    unit: "USD".into(),
                },
            ),
        ]
    }

    #[test]
    fn stock_and_price_need_a_freshness_limit_and_a_fresh_offer() {
        let mut r = ldo("P", "s1", "6", "300", "SOT-23-5");
        r.offers.push(offer(
            "s1",
            Some(5000),
            60,
            &[(1, "0.30"), (100, "0.15"), (1000, "0.10")],
        ));
        let part = Part::merge(vec![r]).remove(0);
        let mut q = req(supply());
        let ctx = EvalContext::of(&q, NOW);
        let res: Vec<Outcome> = supply()
            .iter()
            .map(|c| evaluate(&part, c, &ctx).outcome)
            .collect();
        assert_eq!(res, vec![Outcome::Pass, Outcome::Pass]);
        let price = evaluate(&part, &supply()[1], &ctx);
        assert!(
            price.reason.starts_with("unit price 0.15 USD"),
            "{}",
            price.reason
        );

        q.offer_max_age_s = None;
        let ctx = EvalContext::of(&q, NOW);
        assert!(supply()
            .iter()
            .all(|c| evaluate(&part, c, &ctx).outcome == Outcome::Unknown));

        q.offer_max_age_s = Some(30);
        let ctx = EvalContext::of(&q, NOW);
        let stale = evaluate(&part, &supply()[0], &ctx);
        assert_eq!(stale.outcome, Outcome::Unknown);
        assert!(stale.reason.contains("older"), "{}", stale.reason);

        let bare = Part::merge(vec![ldo("Q", "s1", "6", "300", "SOT-23-5")]).remove(0);
        let ctx = EvalContext::of(&req(supply()), NOW);
        assert_eq!(
            evaluate(&bare, &supply()[0], &ctx).outcome,
            Outcome::Unknown,
            "no offer is never in stock"
        );
    }

    #[test]
    fn comparison_ranks_only_qualified_parts_by_price() {
        let mut a = ldo("A", "s1", "6", "300", "SOT-23-5");
        a.offers.push(offer("s1", Some(500), 60, &[(1, "0.18")]));
        let mut b = ldo("B", "s1", "6", "300", "SOT-23-5");
        b.offers.push(offer("s1", Some(500), 60, &[(1, "0.12")]));
        let c = ldo("C", "s1", "6", "300", "SOT-23-5");
        let parts = Part::merge(vec![a, b, c]);
        let mut criteria = electrical();
        criteria.extend(supply());
        let cmp = compare(&parts, &req(criteria), NOW);
        let ranks: Vec<(&str, Option<usize>)> = cmp
            .candidates
            .iter()
            .map(|c| (c.candidate.display_mpn.as_str(), c.rank))
            .collect();
        assert_eq!(ranks, vec![("A", Some(2)), ("B", Some(1)), ("C", None)]);
        assert!(
            cmp.ranking_basis.contains("unit price"),
            "{}",
            cmp.ranking_basis
        );
        let cost = cmp.candidates[2].by_kind[&CriterionKind::Cost];
        assert_eq!(
            cost,
            KindTally {
                pass: 0,
                fail: 0,
                unknown: 1
            }
        );
        assert_eq!(cmp.candidates[0].unit_price, Some(0.18));
    }

    fn def(pins: &[(&str, &str)]) -> ComponentDef {
        let mut d = eak_core::builtin::components()
            .into_iter()
            .find(|c| c.symbol.pins.len() >= pins.len())
            .expect("a built-in component");
        d.symbol.pins.truncate(pins.len());
        for (p, (num, name)) in d.symbol.pins.iter_mut().zip(pins) {
            p.number = (*num).to_string();
            p.name = (*name).to_string();
        }
        d
    }

    #[test]
    fn equivalence_needs_every_aspect_shown() {
        let orig = Part::merge(vec![ldo("A", "s1", "6", "300", "SOT-23-5")]).remove(0);
        let same = Part::merge(vec![ldo("B", "s2", "6", "300", "SOT23-5")]).remove(0);
        let ctx = EvalContext::of(&req(vec![]), NOW);
        let d = def(&[("1", "VIN"), ("2", "GND")]);

        let no_defs = check_equivalence(&orig, &same, None, None, &[], &ctx);
        assert_eq!(
            no_defs.verdict,
            EquivalenceVerdict::Unknown,
            "pins not shown"
        );

        let ok = check_equivalence(&orig, &same, Some(&d), Some(&d), &[], &ctx);
        assert_eq!(
            ok.verdict,
            EquivalenceVerdict::Equivalent,
            "{:?}",
            ok.checks
        );

        let swapped = def(&[("1", "GND"), ("2", "VIN")]);
        let bad = check_equivalence(&orig, &same, Some(&d), Some(&swapped), &[], &ctx);
        assert_eq!(bad.verdict, EquivalenceVerdict::NotEquivalent);

        let other_pkg = Part::merge(vec![ldo("B", "s2", "6", "300", "SOT-89")]).remove(0);
        let pkg = check_equivalence(&orig, &other_pkg, Some(&d), Some(&d), &[], &ctx);
        assert_eq!(pkg.verdict, EquivalenceVerdict::NotEquivalent);
    }

    #[test]
    fn a_differing_value_is_unknown_unless_a_criterion_decides() {
        let orig = Part::merge(vec![ldo("A", "s1", "6", "300", "SOT-23-5")]).remove(0);
        let alt = Part::merge(vec![ldo("B", "s2", "6", "500", "SOT-23-5")]).remove(0);
        let ctx = EvalContext::of(&req(vec![]), NOW);
        let d = def(&[("1", "VIN"), ("2", "GND")]);
        let open = check_equivalence(&orig, &alt, Some(&d), Some(&d), &[], &ctx);
        assert_eq!(open.verdict, EquivalenceVerdict::Unknown);
        let iout = open
            .checks
            .iter()
            .find(|c| c.aspect == "spec:Iout max")
            .unwrap();
        assert!(
            iout.detail.contains("engineering review"),
            "{}",
            iout.detail
        );

        let iout_rule = electrical()[1..2].to_vec();
        let decided = check_equivalence(&orig, &alt, Some(&d), Some(&d), &iout_rule, &ctx);
        assert_eq!(
            decided.verdict,
            EquivalenceVerdict::Equivalent,
            "{:?}",
            decided.checks
        );

        let weaker = Part::merge(vec![ldo("C", "s2", "6", "200", "SOT-23-5")]).remove(0);
        let no = check_equivalence(&orig, &weaker, Some(&d), Some(&d), &iout_rule, &ctx);
        assert_eq!(no.verdict, EquivalenceVerdict::NotEquivalent);
    }

    #[test]
    fn equivalence_without_electrical_evidence_is_unknown() {
        let mut a = PartRecord::new("Maker", "A", prov("s1")).unwrap();
        a.package = Some("0603".into());
        let mut b = PartRecord::new("Maker", "B", prov("s1")).unwrap();
        b.package = Some("0603".into());
        let parts = Part::merge(vec![a, b]);
        let d = def(&[("1", "~"), ("2", "~")]);
        let ctx = EvalContext::of(&req(vec![]), NOW);
        let e = check_equivalence(&parts[0], &parts[1], Some(&d), Some(&d), &[], &ctx);
        assert_eq!(e.verdict, EquivalenceVerdict::Unknown);
        assert!(e.checks.iter().any(|c| c.aspect == "electrical_evidence"));
    }

    #[test]
    fn requirement_json_round_trips() {
        let mut criteria = electrical();
        criteria.extend(supply());
        let r = req(criteria);
        let text = serde_json::to_string(&r).unwrap();
        assert!(text.contains(r#""subject":"spec""#), "{text}");
        assert!(text.contains(r#""name":"Vin max""#), "{text}");
        let back: FamilyRequirement = serde_json::from_str(&text).unwrap();
        assert_eq!(back, r);
        let minimal: FamilyRequirement = serde_json::from_str(
            r#"{"role":"R1","family":"resistor","search_terms":["10k"],"criteria":[{"id":"p","kind":"mechanical","subject":"package","rule":{"op":"one_of","values":["0603"]}}]}"#,
        )
        .unwrap();
        assert_eq!(minimal.quantity, 1);
        assert_eq!(minimal.offer_max_age_s, None);
    }
}
