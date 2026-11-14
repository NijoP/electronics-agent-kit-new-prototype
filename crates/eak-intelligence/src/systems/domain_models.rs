//! Typed per-domain engineering models (Track B): each discipline gets its own field set,
//! not one generic prompt. A field has a physical dimension (or is qualitative), is filled only
//! from a requirement with a known value and a unit of that dimension, and is otherwise
//! explicitly `Unknown`. Deterministic checks report one of six outcomes and never pass on
//! missing data; a check EAK has no evaluator for is `Unknown` with the reason.

use super::compile::EngineeringModel;
use super::model::{Constraint, Kind, Requirement, Unit, Value};
use eak_core::units::Dimension;
use serde::Serialize;

/// Outcomes are the shared check states of ADR-0026 (`eak_validation::CheckState`).
pub use eak_validation::CheckState;

/// Static field definition: `key` is the requirement quantity-key prefix that fills it.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct FieldDef {
    pub key: &'static str,
    pub label: &'static str,
    /// `None` = qualitative (text) field.
    pub dimension: Option<Dimension>,
}

const fn f(key: &'static str, label: &'static str, dimension: Option<Dimension>) -> FieldDef {
    FieldDef {
        key,
        label,
        dimension,
    }
}

use Dimension::{Current, Frequency, Power, Resistance, Temperature, Time, Voltage};

#[rustfmt::skip]
pub const POWER: &[FieldDef] = &[
    f("input_voltage", "input voltage range", Some(Voltage)),
    f("output_voltage", "output voltage", Some(Voltage)),
    f("output_current", "output current", Some(Current)),
    f("switching_frequency", "switching frequency", Some(Frequency)),
    f("topology", "topology", None),
    f("power_stage_loss", "power-stage losses", Some(Power)),
    f("gate_drive", "gate-drive requirements", None),
    f("magnetics", "magnetic component selection", None),
    f("current_path", "high-current paths", None),
    f("ambient_temperature", "thermal design ambient", Some(Temperature)),
    f("ripple", "input/output filtering (ripple)", Some(Voltage)),
    f("layout_loop", "layout loops (hot-loop definition)", None),
    f("protection", "protection", None),
    f("switch_node", "switching-node management", None),
];

#[rustfmt::skip]
pub const RF: &[FieldDef] = &[
    f("operating_frequency", "frequency bands", Some(Frequency)),
    f("impedance", "impedance requirement", Some(Resistance)),
    f("transmission_line", "transmission-line constraints", None),
    f("antenna", "antenna selection", None),
    f("matching_network", "matching network", None),
    f("rf_ground", "RF ground strategy", None),
    f("rf_isolation", "isolation", None),
    f("return_path", "return paths", None),
    f("coupling", "coupling", None),
    f("emi", "EMI limits", None),
    f("parasitics", "component parasitics", None),
    f("enclosure", "enclosure effects", None),
    f("rf_measurement", "RF measurements", None),
];

#[rustfmt::skip]
pub const ANALOG: &[FieldDef] = &[
    f("noise", "noise budget", Some(Voltage)),
    f("input_impedance", "input impedance", Some(Resistance)),
    f("output_load", "output loading", Some(Resistance)),
    f("reference", "reference design", None),
    f("bandwidth", "filtering bandwidth", Some(Frequency)),
    f("adc", "ADC/DAC requirements", None),
    f("analog_ground", "analog grounding", None),
    f("supply_noise", "power-supply noise", Some(Voltage)),
    f("signal_integrity", "signal integrity", None),
    f("accuracy", "measurement accuracy", None),
];

#[rustfmt::skip]
pub const DIGITAL_HS: &[FieldDef] = &[
    f("interface", "interface standards", None),
    f("clock", "clock requirements", Some(Frequency)),
    f("differential_impedance", "differential impedance", Some(Resistance)),
    f("length_match", "length matching", None),
    f("skew", "skew budget", Some(Time)),
    f("return_path", "return paths", None),
    f("crosstalk", "crosstalk", None),
    f("signal_integrity", "signal integrity", None),
    f("decoupling", "decoupling", None),
    f("routing_constraint", "routing constraints", None),
];

#[rustfmt::skip]
pub const EMBEDDED: &[FieldDef] = &[
    f("mcu", "MCU architecture", None),
    f("pin_mux", "pin multiplexing", None),
    f("peripheral", "peripheral allocation", None),
    f("boot_reset", "boot and reset", None),
    f("debug", "debug interface", None),
    f("programming", "programming access", None),
    f("power_state", "power states", None),
    f("clock", "clocking", Some(Frequency)),
    f("firmware_interface", "firmware interfaces", None),
    f("protection", "EMC and protection", None),
];

/// Field set of a domain; `None` when the domain has no typed model yet.
pub fn fields(domain: &str) -> Option<&'static [FieldDef]> {
    Some(match domain {
        "power" => POWER,
        "rf" => RF,
        "analog" => ANALOG,
        "digital_hs" => DIGITAL_HS,
        "embedded" => EMBEDDED,
        _ => return None,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum FieldValue {
    Known {
        requirement: String,
        value: Value,
        unit: Option<Unit>,
    },
    /// A requirement exists but its unit is not of the field's dimension: never used.
    UnitMismatch {
        requirement: String,
        unit: Option<Unit>,
    },
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct Field {
    #[serde(flatten)]
    pub def: FieldDef,
    pub value: FieldValue,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub id: String,
    pub outcome: CheckState,
    pub detail: String,
    /// Requirement, constraint or rule ids the outcome rests on.
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DomainModel {
    pub domain: String,
    pub fields: Vec<Field>,
    pub checks: Vec<CheckResult>,
}

fn resolve(def: &FieldDef, reqs: &[Requirement]) -> FieldValue {
    let prefix = format!("{}:", def.key);
    let mut mismatch = None;
    for r in reqs.iter().filter(|r| {
        r.value.is_known()
            && r.quantity
                .as_deref()
                .is_some_and(|q| q == def.key || q.starts_with(&prefix))
    }) {
        let ok = match def.dimension {
            None => true,
            Some(d) => r.unit.is_some_and(|u| u.dimension() == d),
        };
        if ok {
            return FieldValue::Known {
                requirement: r.id.clone(),
                value: r.value.clone(),
                unit: r.unit,
            };
        }
        if mismatch.is_none() {
            mismatch = Some(FieldValue::UnitMismatch {
                requirement: r.id.clone(),
                unit: r.unit,
            });
        }
    }
    mismatch.unwrap_or(FieldValue::Unknown)
}

fn check(id: &str, outcome: CheckState, detail: String, evidence: Vec<String>) -> CheckResult {
    CheckResult {
        id: id.into(),
        outcome,
        detail,
        evidence,
    }
}

fn known<'a>(fields: &'a [Field], key: &str) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|x| x.def.key == key)
        .map(|x| &x.value)
        .filter(|v| matches!(v, FieldValue::Known { .. }))
}

fn interaction_warning(m: &EngineeringModel, rule: &str, id: &str) -> CheckResult {
    match m.interactions.iter().find(|i| i.rule.id == rule) {
        Some(i) => check(
            id,
            CheckState::Warning,
            format!(
                "{} Re-evaluate: {}",
                i.rule.mechanism,
                i.rule.re_evaluate.join("; ")
            ),
            vec![format!("interaction.{rule}")],
        ),
        None => check(
            id,
            CheckState::NotApplicable,
            "the interacting domain is not present".into(),
            vec![],
        ),
    }
}

fn power_checks(fields: &[Field], m: &EngineeringModel, out: &mut Vec<CheckResult>) {
    // A buck needs Vout < Vin(min); decided only when the topology says buck.
    let topology = match known(fields, "topology") {
        Some(FieldValue::Known {
            value: Value::Text { text },
            ..
        }) => Some(text.to_lowercase()),
        _ => None,
    };
    let duty: Vec<_> = m
        .calculations
        .iter()
        .filter(|c| c.method.ends_with("buck_duty_cycle"))
        .collect();
    let ids = |v: &[&super::model::CalculationRecord]| v.iter().map(|c| c.id.clone()).collect();
    out.push(match topology.as_deref() {
        Some(t) if t.contains("buck") => {
            let failed: Vec<_> = duty
                .iter()
                .copied()
                .filter(|c| c.result.is_none())
                .collect();
            if duty.is_empty() {
                check(
                    "buck_ratio",
                    CheckState::Unknown,
                    "no input/output voltage pair to evaluate".into(),
                    vec![],
                )
            } else if failed.is_empty() {
                check(
                    "buck_ratio",
                    CheckState::Pass,
                    "every output is below the minimum input".into(),
                    ids(&duty),
                )
            } else {
                check(
                    "buck_ratio",
                    CheckState::Fail,
                    "an output is not below the minimum input; a buck cannot regulate it".into(),
                    ids(&failed),
                )
            }
        }
        Some(_) => check(
            "buck_ratio",
            CheckState::NotApplicable,
            "topology is not a buck".into(),
            vec![],
        ),
        None => check(
            "buck_ratio",
            CheckState::Unknown,
            "topology unknown".into(),
            vec![],
        ),
    });
    let is_current = |c: &Constraint| {
        ["output_current:", "input_current:", "rail_current:"]
            .iter()
            .any(|k| c.quantity.starts_with(k))
    };
    let currents: Vec<&Constraint> = m.constraints.iter().filter(|c| is_current(c)).collect();
    let underived: Vec<String> = currents
        .iter()
        .filter(|c| {
            !m.constraints
                .iter()
                .any(|d| d.kind == Kind::Derived && d.derived_from.contains(&c.id))
        })
        .map(|c| c.id.clone())
        .collect();
    out.push(if currents.is_empty() {
        check(
            "current_paths_sized",
            CheckState::Unknown,
            "no rail current stated".into(),
            vec![],
        )
    } else if underived.is_empty() {
        check(
            "current_paths_sized",
            CheckState::Pass,
            "every stated rail current has a derived trace-capacity constraint (evaluated on the layout by net_trace_capacity_a)".into(),
            currents.iter().map(|c| c.id.clone()).collect(),
        )
    } else {
        check(
            "current_paths_sized",
            CheckState::Fail,
            "a rail current has no derivable trace-capacity constraint (missing maximum or not a current unit)".into(),
            underived,
        )
    });
    out.push(check(
        "loss_and_thermal",
        CheckState::Unknown,
        "switching and conduction loss is not implemented in eak-engineering".into(),
        vec![],
    ));
}

fn domain_checks(domain: &str, fields: &[Field], m: &EngineeringModel) -> Vec<CheckResult> {
    let mut out = vec![];
    let unknown: Vec<String> = fields
        .iter()
        .filter(|x| !matches!(x.value, FieldValue::Known { .. }))
        .map(|x| x.def.key.to_string())
        .collect();
    out.push(check(
        "fields_known",
        if unknown.is_empty() {
            CheckState::Pass
        } else {
            CheckState::Unknown
        },
        format!("{} of {} fields unknown", unknown.len(), fields.len()),
        unknown,
    ));
    let mismatched: Vec<String> = fields
        .iter()
        .filter_map(|x| match &x.value {
            FieldValue::UnitMismatch { requirement, .. } => Some(requirement.clone()),
            _ => None,
        })
        .collect();
    if !mismatched.is_empty() {
        out.push(check(
            "field_units",
            CheckState::Fail,
            "a requirement's unit does not match the field's physical dimension".into(),
            mismatched,
        ));
    }
    let not_impl = |id: &str, what: &str| {
        check(
            id,
            CheckState::Unknown,
            format!("{what} not implemented"),
            vec![],
        )
    };
    match domain {
        "power" => power_checks(fields, m, &mut out),
        "rf" => {
            out.push(interaction_warning(
                m,
                "power_noise_into_rf",
                "switching_noise_coexistence",
            ));
            out.push(interaction_warning(
                m,
                "ground_split_breaks_return_paths",
                "ground_split_return_paths",
            ));
            out.push(not_impl(
                "impedance_and_keepout",
                "controlled-impedance and antenna-keepout checks are",
            ));
        }
        "analog" => {
            out.push(interaction_warning(
                m,
                "heat_near_precision_analog",
                "thermal_drift_exposure",
            ));
            out.push(interaction_warning(
                m,
                "motor_transients_into_sensing",
                "transient_coupling",
            ));
            out.push(not_impl("noise_budget", "noise and error budgets are"));
        }
        "digital_hs" => {
            out.push(interaction_warning(
                m,
                "connector_placement_limits_high_speed",
                "connector_constraints",
            ));
            out.push(not_impl(
                "length_skew_impedance",
                "length, skew and impedance checks are",
            ));
        }
        "embedded" => {
            let debug = known(fields, "debug").is_some()
                || m.requirements.iter().any(|r| {
                    r.quantity.as_deref().is_some_and(|q| {
                        q.starts_with("interface:") && (q.contains("swd") || q.contains("jtag"))
                    })
                });
            out.push(if debug {
                check(
                    "debug_access",
                    CheckState::Pass,
                    "a debug interface is declared".into(),
                    vec![],
                )
            } else {
                check(
                    "debug_access",
                    CheckState::Unknown,
                    "no debug interface declared".into(),
                    vec![],
                )
            });
        }
        _ => {}
    }
    if super::domains::domain(domain).is_some_and(|d| d.high_risk) {
        out.push(check(
            "human_review",
            CheckState::RequiresHumanApproval,
            "high-risk domain: a human expert must approve".into(),
            vec![],
        ));
    }
    out
}

/// Typed models of every active domain that has a field set.
pub fn build(m: &EngineeringModel) -> Vec<DomainModel> {
    m.domains
        .iter()
        .filter_map(|d| {
            let defs = fields(&d.id)?;
            let fields: Vec<Field> = defs
                .iter()
                .map(|def| Field {
                    def: *def,
                    value: resolve(def, &m.requirements),
                })
                .collect();
            let checks = domain_checks(&d.id, &fields, m);
            Some(DomainModel {
                domain: d.id.clone(),
                fields,
                checks,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::systems::compile::{compile, BlockHint, CompileInput};
    use crate::systems::profile::Level;

    // Written, not run (code-reviewed only).
    #[test]
    fn unknown_never_passes_and_rf_warns_next_to_power() {
        let p = eak_core::Project::new("t");
        let block = |id: &str, d: &str| BlockHint {
            id: id.into(),
            name: id.into(),
            level: Level::FunctionalBlock,
            parent: None,
            domain: Some(d.into()),
            reason: "test".into(),
            objects: vec![],
        };
        let input = CompileInput {
            blocks: vec![block("b1", "power"), block("b2", "rf")],
            ..Default::default()
        };
        let m = compile(&p, input);
        let power = m
            .domain_models
            .iter()
            .find(|x| x.domain == "power")
            .unwrap();
        assert!(power
            .fields
            .iter()
            .all(|f| matches!(f.value, FieldValue::Unknown)));
        assert!(power.checks.iter().all(|c| c.outcome != CheckState::Pass));
        let rf = m.domain_models.iter().find(|x| x.domain == "rf").unwrap();
        let w = rf
            .checks
            .iter()
            .find(|c| c.id == "switching_noise_coexistence")
            .unwrap();
        assert_eq!(w.outcome, CheckState::Warning);
    }

    #[test]
    fn field_sets_have_unique_keys() {
        for set in [POWER, RF, ANALOG, DIGITAL_HS, EMBEDDED] {
            let mut keys = std::collections::BTreeSet::new();
            assert!(set.iter().all(|d| keys.insert(d.key)));
        }
    }
}
