//! Deterministic, explainable domain classification (port of DESK `classifier.py`).
//!
//! No model is consulted and no API key is needed. Every score point carries a reason naming
//! the design signal it came from. The result is a *classification of the design as written*:
//! it never evaluates DESK rules and never states that a design is correct (see
//! [`NOT_IMPLEMENTED`] and [`LIMITATIONS`]); guidance is capped by the DESK maturity label.

use super::features::{self, kw_match, Features, Source};
use super::registry::{registry, Archetype, Ceiling, Maturity};
use eak_core::Project;
use serde::Serialize;
use serde_json::Value;

/// Checks EAK does not perform for domain classes. Reported with every classification so an
/// unknown is never read as a pass.
pub const NOT_IMPLEMENTED: &[&str] = &[
    "DESK rule-set evaluation (the per-class rule ids are listed, not executed)",
    "Verification that required component roles are present and correct in the design",
    "Verification of any DESK constraint against the cited standards",
];

pub const LIMITATIONS: &[&str] = &[
    "Classification is keyword and feature scoring over project name, requirements, component names and net labels; a sparse design yields a weak or no classification.",
    "A class is a hypothesis about the design's domain, not a certification of the design.",
    "Guidance is capped by the DESK maturity label of the class; most DESK classes are DRAFT.",
];

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub archetype_id: String,
    pub name: String,
    pub domain: String,
    pub domain_name: String,
    pub description: String,
    pub score: f64,
    pub reasons: Vec<String>,
    pub penalties: Vec<String>,
    pub maturity: Maturity,
    pub ceiling: Ceiling,
}

/// Guidance for a class, shown only as far as its maturity label allows.
#[derive(Debug, Clone, Serialize)]
pub struct Guidance {
    pub archetype_id: String,
    pub ceiling: Ceiling,
    /// Heading for the items: "Requirements to confirm" only when the label permits requirements,
    /// otherwise "Review prompts (unverified)".
    pub heading: &'static str,
    pub critical_electrical_constraints: Vec<String>,
    pub verification_requirements: Vec<String>,
    pub required_component_roles: Vec<String>,
    pub applicable_standards: Vec<String>,
    pub unsupported_conditions: Vec<String>,
    pub desk_rule_ids: Vec<String>,
    pub maturity_evidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    Classified,
    ReviewRequired,
    Unclassified,
}

#[derive(Debug, Clone, Serialize)]
pub struct DomainClassification {
    pub status: Status,
    pub confidence: f64,
    pub primary: Option<String>,
    pub secondary: Vec<String>,
    /// Top candidates, best first (at most 10).
    pub candidates: Vec<Candidate>,
    pub overlays: Vec<String>,
    pub required_component_families: Vec<String>,
    pub missing_information: Vec<String>,
    pub conflicts: Vec<String>,
    pub evidence: Vec<String>,
    pub features: Features,
    pub guidance: Option<Guidance>,
    pub registry_classes: usize,
    pub not_implemented: Vec<String>,
    pub limitations: Vec<String>,
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn pair(v: &Value) -> Option<(f64, f64)> {
    let a = v.as_array()?;
    Some((num(a.first()?)?, num(a.get(1)?)?))
}

fn strs(v: &Value) -> Vec<&str> {
    v.as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// First source whose text contains the keyword, for the reason text.
fn origin<'a>(srcs: &'a [Source], kw: &str) -> Option<&'a str> {
    srcs.iter()
        .find(|s| kw_match(&s.text, kw))
        .map(|s| s.origin.as_str())
}

fn score(
    a: &Archetype,
    f: &Features,
    joined: &str,
    srcs: &[Source],
) -> (f64, Vec<String>, Vec<String>) {
    let (mut s, mut reasons, mut pen) = (0.0, Vec::new(), Vec::new());
    for (kw, w) in &a.classification.keywords {
        if kw_match(joined, &kw.to_lowercase()) {
            s += w;
            let from = origin(srcs, &kw.to_lowercase()).unwrap_or("design text");
            reasons.push(format!("keyword '{kw}' in {from} (+{w})"));
        }
    }
    let spec = &a.classification.features;
    let g = |k: &str| f.get(k);
    if let (Some(range), Some(v)) = (
        spec.get("input_voltage_v").and_then(pair),
        g("input_voltage_v").and_then(num),
    ) {
        let (lo, hi) = range;
        if lo <= v && v <= hi {
            s += 3.0;
            reasons.push(format!("input {v} V within {lo}-{hi} V (+3)"));
        } else if v > 2.0 * hi || v < lo / 2.0 {
            s -= 2.0;
            pen.push(format!("input {v} V far outside {lo}-{hi} V (-2)"));
        }
    }
    if let Some(want) = spec.get("protocols_any") {
        let have = g("protocols").map(strs).unwrap_or_default();
        let mut hits: Vec<&str> = strs(want)
            .into_iter()
            .filter(|p| have.contains(p))
            .collect();
        hits.sort_unstable();
        if !hits.is_empty() {
            let pts = f64::min(6.0, 3.0 * hits.len() as f64);
            s += pts;
            reasons.push(format!("protocols {hits:?} (+{pts})"));
        }
    }
    if let Some(want) = spec.get("power_source") {
        let have = g("power_source").map(strs).unwrap_or_default();
        let mut hits: Vec<&str> = strs(want)
            .into_iter()
            .filter(|p| have.contains(p))
            .collect();
        hits.sort_unstable();
        if !hits.is_empty() {
            s += 3.0;
            reasons.push(format!("power source {hits:?} (+3)"));
        }
    }
    if let (Some((lo, hi)), Some(fz)) = (
        spec.get("rf_frequency_hz").and_then(pair),
        g("rf_frequency_hz").and_then(num),
    ) {
        if lo <= fz && fz <= hi {
            s += 4.0;
            reasons.push(format!("RF {} MHz within band (+4)", fz / 1e6));
        } else {
            s -= 3.0;
            pen.push(format!(
                "RF {} MHz outside {}-{} MHz (-3)",
                fz / 1e6,
                lo / 1e6,
                hi / 1e6
            ));
        }
    }
    if let Some(want) = spec.get("motor") {
        let have = g("motor").map(strs).unwrap_or_default();
        let hits: Vec<&str> = strs(want)
            .into_iter()
            .filter(|m| have.contains(m))
            .collect();
        if !hits.is_empty() {
            s += 4.0;
            reasons.push(format!("motor {hits:?} (+4)"));
        } else if !have.is_empty() {
            s -= 3.0;
            pen.push(format!("motor type {have:?} not {:?} (-3)", strs(want)));
        }
    }
    if let (Some(want), Some(have)) = (spec.get("converter"), g("converter")) {
        let have = strs(have);
        let hits: Vec<&str> = strs(want)
            .into_iter()
            .filter(|c| have.contains(c))
            .collect();
        if !hits.is_empty() {
            s += 4.0;
            reasons.push(format!("converter {hits:?} (+4)"));
        }
    }
    if spec.get("mains") == Some(&Value::Bool(true)) {
        let have = g("power_source").map(strs).unwrap_or_default();
        if g("mains") == Some(&Value::Bool(true)) {
            s += 4.0;
            reasons.push("mains input (+4)".into());
        } else if !have.is_empty() && !have.contains(&"mains") {
            s -= 2.0;
            pen.push("no mains source mentioned (-2)".into());
        }
    }
    if let (Some(want), Some(have)) = (
        spec.get("isolation").and_then(Value::as_bool),
        g("isolation").and_then(Value::as_bool),
    ) {
        if want == have {
            s += 2.0;
            reasons.push("isolation requirement matches (+2)".into());
        } else {
            s -= 3.0;
            pen.push("isolation requirement contradicts (-3)".into());
        }
    }
    if let (Some((lo, hi)), Some(c)) = (spec.get("cells").and_then(pair), g("cells").and_then(num))
    {
        if lo <= c && c <= hi {
            s += 2.0;
            reasons.push(format!("{c} cells (+2)"));
        }
    }
    if let (Some((lo, hi)), Some(w)) = (
        spec.get("power_w").and_then(pair),
        g("power_w").and_then(num),
    ) {
        if lo <= w && w <= hi {
            s += 2.0;
            reasons.push(format!("{w} W within range (+2)"));
        }
    }
    (s, reasons, pen)
}

/// DESK overlay `applies_when` evaluator: any-of conditions over the feature map.
fn overlay_applies(applies_when: &Value, f: &Features) -> bool {
    let conds = applies_when.get("any").and_then(Value::as_array);
    conds.is_some_and(|cs| {
        cs.iter().any(|c| {
            let (Some(field), Some(op)) = (
                c.get("field").and_then(Value::as_str),
                c.get("op").and_then(Value::as_str),
            ) else {
                return false;
            };
            let want = c.get("value").unwrap_or(&Value::Null);
            let have = f.get(field);
            let present = have.is_some_and(|h| {
                !h.is_null()
                    && h != &Value::Bool(false)
                    && h.as_array().is_none_or(|a| !a.is_empty())
            });
            match op {
                "exists" => present == want.as_bool().unwrap_or(true),
                "==" => have == Some(want),
                ">=" | "<=" | ">" | "<" => match (have.and_then(num), num(want)) {
                    (Some(h), Some(w)) => match op {
                        ">=" => h >= w,
                        "<=" => h <= w,
                        ">" => h > w,
                        _ => h < w,
                    },
                    _ => false,
                },
                "between" => match (have.and_then(num), pair(want)) {
                    (Some(h), Some((lo, hi))) => lo <= h && h <= hi,
                    _ => false,
                },
                "in" => have.is_some_and(|h| {
                    let w = strs(want);
                    h.as_str().is_some_and(|x| w.contains(&x))
                }),
                "intersects" => have.is_some_and(|h| {
                    let w = strs(want);
                    strs(h).iter().any(|x| w.contains(x))
                }),
                _ => false,
            }
        })
    })
}

const PERIPHERAL_DOMAINS: &[&str] = &["interfaces_comms"];
const CONTROLLER_HINTS: &[&str] = &[
    "mcu",
    "microcontroller",
    "controller board",
    "stm32",
    "esp32",
    "rp2040",
    "nrf52",
    "pic16",
    "pic18",
    "atmega",
    "samd",
    "controller",
];

const CHECKLIST: &[(&[&str], &str)] = &[
    (&["input_voltage_v"], "supply voltage or input range"),
    (&["current_a", "power_w"], "load current or power level"),
    (
        &["power_source"],
        "power source (battery, USB, mains, DC input, vehicle, PoE)",
    ),
    (&["protocols"], "communication interfaces"),
    (
        &["max_ambient_c", "environment"],
        "operating environment and temperature",
    ),
];

fn guidance(a: &Archetype) -> Guidance {
    let ceiling = a.maturity.status.ceiling();
    let show = ceiling.may_show_guidance;
    let take = |v: &Vec<String>| if show { v.clone() } else { Vec::new() };
    Guidance {
        archetype_id: a.archetype_id.clone(),
        heading: if ceiling.may_present_as_requirement {
            "Requirements to confirm"
        } else {
            "Review prompts (unverified)"
        },
        critical_electrical_constraints: take(&a.critical_electrical_constraints),
        verification_requirements: take(&a.verification_requirements),
        required_component_roles: take(&a.required_component_roles),
        applicable_standards: take(&a.applicable_standards),
        unsupported_conditions: take(&a.unsupported_conditions),
        desk_rule_ids: take(&a.maturity.rule_ids),
        maturity_evidence: a.maturity.evidence.clone(),
        ceiling,
    }
}

/// Classify design text signals (the shared core; [`classify_project`] feeds it from EAK-IR).
pub fn classify_sources(srcs: &[Source]) -> DomainClassification {
    let reg = registry();
    let joined = srcs
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" \n ");
    let feats = features::extract(&joined);

    let mut cands: Vec<Candidate> = reg
        .archetypes
        .iter()
        .map(|a| {
            let (score, reasons, penalties) = score(a, &feats, &joined, srcs);
            Candidate {
                archetype_id: a.archetype_id.clone(),
                name: a.name.clone(),
                domain: a.domain.clone(),
                domain_name: reg
                    .domain(&a.domain)
                    .map_or_else(String::new, |d| d.name.clone()),
                description: a.description.clone(),
                score,
                reasons,
                penalties,
                maturity: a.maturity.status,
                ceiling: a.maturity.status.ceiling(),
            }
        })
        .collect();
    cands.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then(a.archetype_id.cmp(&b.archetype_id))
    });

    let mut evidence = Vec::new();
    let mut conflicts = Vec::new();
    let mut demoted: Option<String> = None;
    let controller = CONTROLLER_HINTS.iter().any(|k| kw_match(&joined, k));
    if cands
        .first()
        .is_some_and(|c| PERIPHERAL_DOMAINS.contains(&c.domain.as_str()))
        && controller
    {
        let top_score = cands[0].score;
        if let Some(i) = (1..cands.len()).find(|&i| {
            !PERIPHERAL_DOMAINS.contains(&cands[i].domain.as_str())
                && cands[i].score >= f64::max(6.0, 0.4 * top_score)
        }) {
            evidence.push(format!(
                "{} is an interface feature; {} describes the board (controller named)",
                cands[0].archetype_id, cands[i].archetype_id
            ));
            demoted = Some(cands[0].archetype_id.clone());
            let c = cands.remove(i);
            cands.insert(0, c);
        }
    }

    let top = cands.first().cloned();
    let second = cands
        .iter()
        .skip(1)
        .filter(|c| Some(&c.archetype_id) != demoted.as_ref())
        .map(|c| c.score)
        .fold(0.0_f64, f64::max);
    let (mut status, mut confidence, mut primary) = (Status::Unclassified, 0.0, None);
    if let Some(t) = &top {
        if t.score >= 8.0 && t.score - second >= 3.0 {
            status = Status::Classified;
            primary = Some(t.archetype_id.clone());
            confidence = (0.5 + 0.04 * (t.score - second) + 0.01 * t.score).min(0.95);
        } else if t.score >= 4.0 {
            status = Status::ReviewRequired;
            primary = Some(t.archetype_id.clone());
            confidence = (0.2 + 0.03 * t.score).min(0.6);
            if t.score - second < 3.0 {
                if let Some(c2) = cands.get(1) {
                    conflicts.push(format!(
                        "close candidates: {} ({}) vs {} ({second})",
                        t.archetype_id, t.score, c2.archetype_id
                    ));
                }
            }
        }
        for r in t.reasons.iter().take(8) {
            evidence.push(format!("{}: {r}", t.archetype_id));
        }
    }
    confidence = (confidence * 100.0).round() / 100.0;
    let prim = primary.as_deref().and_then(|id| reg.archetype(id));
    let secondary: Vec<String> = match (&primary, &top) {
        (Some(p), Some(t)) => cands
            .iter()
            .skip(1)
            .take(3)
            .filter(|c| c.score >= f64::max(4.0, 0.4 * t.score) && &c.archetype_id != p)
            .map(|c| c.archetype_id.clone())
            .collect(),
        _ => Vec::new(),
    };

    let mut overlays: Vec<String> = prim.map(|a| a.typical_overlays.clone()).unwrap_or_default();
    for o in &reg.overlays {
        if overlay_applies(&o.applies_when, &feats) && !overlays.contains(&o.overlay_id) {
            overlays.push(o.overlay_id.clone());
            evidence.push(format!("overlay {} from features", o.overlay_id));
        }
    }
    let mains_cands = cands
        .iter()
        .take(5)
        .any(|c| c.domain == "ac_mains_hv" && c.score >= 4.0);
    let prim_mains = prim.is_some_and(|a| a.domain == "ac_mains_hv");
    if (mains_cands || feats.get("mains") == Some(&Value::Bool(true))) && !prim_mains {
        conflicts.push("possible mains/high-voltage involvement; a mains archetype scored or mains was detected while the primary is not a mains archetype".into());
        if status == Status::Classified {
            status = Status::ReviewRequired;
            confidence = confidence.min(0.6);
        }
    } else if prim_mains
        && (controller
            || ["sensor", "node", "iot"]
                .iter()
                .any(|k| kw_match(&joined, k)))
    {
        conflicts.push("mains class selected but the text also describes a controller or sensor product: confirm whether this is the mains supply section or a mains-powered device (both need the mains_safety_isolation overlay)".into());
        if status == Status::Classified {
            status = Status::ReviewRequired;
            confidence = confidence.min(0.6);
        }
    }
    if feats.contains_key("mains") && !overlays.iter().any(|o| o == "mains_safety_isolation") {
        overlays.push("mains_safety_isolation".into());
    }
    if let (Some(a), true) = (prim, feats.contains_key("motor")) {
        if a.domain != "motor_control" && a.domain != "ac_mains_hv" {
            conflicts.push("motor mentioned but the primary class is not a motor-control class; motor_drive overlay applied".into());
            if !overlays.iter().any(|o| o == "motor_drive") {
                overlays.push("motor_drive".into());
            }
        }
    }

    let mut families: Vec<String> = prim
        .map(|a| a.required_component_roles.clone())
        .unwrap_or_default();
    for oid in &overlays {
        if let Some(o) = reg.overlay(oid) {
            for fid in &o.adds_component_families {
                if !families.contains(fid) {
                    families.push(fid.clone());
                }
            }
        }
    }
    let mut missing: Vec<String> = CHECKLIST
        .iter()
        .filter(|(keys, _)| {
            !keys.iter().any(|k| {
                feats.get(*k).is_some_and(|v| {
                    !v.is_null()
                        && v != &Value::Bool(false)
                        && v.as_array().is_none_or(|a| !a.is_empty())
                })
            })
        })
        .map(|(_, label)| (*label).to_string())
        .collect();
    if let Some(a) = prim {
        if a.domain == "ac_mains_hv" || overlays.iter().any(|o| o == "mains_safety_isolation") {
            missing.push("voltage_rails: every mains/HV net with its working voltage".into());
        }
    }

    DomainClassification {
        status,
        confidence,
        primary,
        secondary,
        candidates: cands.into_iter().take(10).collect(),
        overlays,
        required_component_families: families,
        missing_information: missing,
        conflicts,
        evidence,
        features: feats,
        guidance: prim.map(guidance),
        registry_classes: reg.archetypes.len(),
        not_implemented: NOT_IMPLEMENTED.iter().map(|s| (*s).into()).collect(),
        limitations: LIMITATIONS.iter().map(|s| (*s).into()).collect(),
    }
}

/// Classify the design currently held in EAK-IR.
pub fn classify_project(p: &Project) -> DomainClassification {
    classify_sources(&features::sources(p))
}

/// Classify free text (a client sentence, a requirements document).
pub fn classify_text(text: &str) -> DomainClassification {
    classify_sources(&[Source {
        origin: "text".into(),
        text: text.to_lowercase(),
    }])
}
