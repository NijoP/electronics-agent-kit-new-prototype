//! Domain mental models (directive §6), embedded as data (`data/discipline_models.json`).
//!
//! Knowledge, procedure, deterministic calculations, constraint keys, verification rules and
//! AI guidance are separate fields so a planner can take the procedure without the guidance
//! and a validator can read the verification list without either. Every model is DRAFT.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

const DATA: &str = include_str!("../../data/discipline_models.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredInput {
    pub key: String,
    /// A requirement whose quantity key starts with one of these, and whose value is known,
    /// satisfies the input.
    pub quantity_prefixes: Vec<String>,
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalcRef {
    pub name: String,
    /// eak-engineering function, `None` when not implemented.
    pub method: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRef {
    pub method: String,
    /// EAK check or evaluator name, `None` when EAK cannot perform it.
    pub eak_check: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardRef {
    pub id: String,
    pub scope: String,
    pub r#use: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentalModel {
    pub domain: String,
    pub maturity: String,
    /// approach, decomposition, component_selection, schematic, placement, routing,
    /// power_grounding, thermal, mechanical, manufacturing, failure_modes.
    pub knowledge: BTreeMap<String, Vec<String>>,
    pub required_inputs: Vec<RequiredInput>,
    pub procedure: Vec<String>,
    pub calculations: Vec<CalcRef>,
    pub constraints: Vec<String>,
    pub verification: Vec<VerificationRef>,
    pub standards: Vec<StandardRef>,
    pub ai_guidance: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MentalModels {
    pub schema_version: String,
    pub note: String,
    /// EAK target layer (1-5, see `layer_names`) of every model section. Layer 1 entries
    /// (calculations, verification) are references to Rust code, never rules evaluated here;
    /// layer 5 (ai_guidance) is never evidence.
    pub layers: BTreeMap<String, u8>,
    pub layer_names: BTreeMap<String, String>,
    pub models: Vec<MentalModel>,
}

/// Knowledge sections every model carries (with required_inputs, calculations, standards and
/// verification these cover directive §6 items 1-15).
pub const KNOWLEDGE_SECTIONS: &[&str] = &[
    "approach",
    "decomposition",
    "component_selection",
    "schematic",
    "placement",
    "routing",
    "power_grounding",
    "thermal",
    "mechanical",
    "manufacturing",
    "failure_modes",
];

/// The embedded models. The data ships in the binary and is checked by the tests, so a parse
/// failure is a build defect.
pub fn models() -> &'static MentalModels {
    static M: OnceLock<MentalModels> = OnceLock::new();
    M.get_or_init(|| serde_json::from_str(DATA).expect("embedded discipline_models.json is valid"))
}

pub fn model(domain: &str) -> Option<&'static MentalModel> {
    models().models.iter().find(|m| m.domain == domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::domains::{domain, Coverage, DOMAINS};

    // Written, not run (code-reviewed only).
    #[test]
    fn every_model_is_complete_and_registered() {
        for m in &models().models {
            let d = domain(&m.domain).expect("model for a registered domain");
            assert_eq!(d.coverage, Coverage::MentalModel, "{}", m.domain);
            for s in KNOWLEDGE_SECTIONS {
                assert!(
                    m.knowledge.get(*s).is_some_and(|v| !v.is_empty()),
                    "{} lacks {s}",
                    m.domain
                );
            }
            assert!(!m.required_inputs.is_empty() && !m.procedure.is_empty());
            assert!(m
                .calculations
                .iter()
                .all(|c| c.method.is_some() == (c.status == "implemented")));
        }
        let l = &models().layers;
        for s in [
            "knowledge",
            "standards",
            "constraints",
            "required_inputs",
            "procedure",
            "calculations",
            "verification",
            "ai_guidance",
        ] {
            let n = l.get(s).copied().unwrap_or(0);
            assert!((1..=5).contains(&n), "{s} has no EAK layer");
            assert!(models().layer_names.contains_key(&n.to_string()));
        }
        assert_eq!(
            l["ai_guidance"], 5,
            "guidance is an optional agent workflow, never evidence"
        );
        for d in DOMAINS {
            if d.coverage == Coverage::MentalModel {
                assert!(model(d.id).is_some(), "{} claims a mental model", d.id);
            }
        }
    }
}
