//! Harness tools for unit-aware copper/power engineering (`eak_engineering::power`) and the
//! fail-closed engineering gates (`eak_validation::gates`). Every conclusion-changing input
//! is a required argument with its unit in the name; nothing is defaulted.

use eak_core::units::{PhysicalQuantity, Unit};
use eak_core::Project;
use eak_engineering::power::{self, ConductorInput, LayerPosition, TraceWidthInput};
use eak_validation::{dfm, gates, GateReport};
use serde_json::{json, Value};

fn num(args: &Value, key: &str) -> Result<f64, String> {
    args.get(key)
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("{key} is required (a finite number); it is never defaulted"))
}

fn q(v: f64, u: Unit) -> PhysicalQuantity {
    PhysicalQuantity::new(v, u)
}

fn layer(args: &Value) -> Result<LayerPosition, String> {
    serde_json::from_value(args.get("layer").cloned().unwrap_or(Value::Null))
        .map_err(|_| "layer is required: \"external\" or \"internal\"".to_string())
}

/// `calc_copper_trace_width`.
pub fn trace_width(args: &Value) -> Result<Value, String> {
    let input = TraceWidthInput {
        current: q(num(args, "current_a")?, Unit::Ampere),
        copper_thickness: q(num(args, "copper_thickness_um")?, Unit::Micrometre),
        temperature_rise: q(num(args, "temp_rise_k")?, Unit::Kelvin),
        layer: layer(args)?,
    };
    let r = power::trace_width(&input).map_err(|e| e.to_string())?;
    serde_json::to_value(r).map_err(|e| e.to_string())
}

/// `calc_copper_voltage_drop`.
pub fn voltage_drop(args: &Value) -> Result<Value, String> {
    let supply = match args.get("supply_voltage_v") {
        None | Some(Value::Null) => None,
        Some(_) => Some(q(num(args, "supply_voltage_v")?, Unit::Volt)),
    };
    let input = ConductorInput {
        current: q(num(args, "current_a")?, Unit::Ampere),
        length: q(num(args, "length_mm")?, Unit::Millimetre),
        width: q(num(args, "width_mm")?, Unit::Millimetre),
        copper_thickness: q(num(args, "copper_thickness_um")?, Unit::Micrometre),
        conductor_temperature: q(num(args, "conductor_temperature_c")?, Unit::DegreeCelsius),
        supply_voltage: supply,
    };
    let r = power::voltage_drop(&input).map_err(|e| e.to_string())?;
    serde_json::to_value(r).map_err(|e| e.to_string())
}

/// `power_analysis`: rail drops and return-path continuity of the current revision.
pub fn power_analysis(p: &Project) -> Result<Value, String> {
    let mut v = serde_json::to_value(power::analyse(p)).map_err(|e| e.to_string())?;
    v["note"] = json!(
        "estimates from declared constraints only; unknown entries were not evaluated and are never a pass"
    );
    Ok(v)
}

/// `run_gates`: evaluate the fail-closed gates for the current revision.
pub fn run_gates(p: &Project, args: &Value) -> Result<GateReport, String> {
    let fab = match args.get("fab_profile").and_then(Value::as_str) {
        Some("none") => None,
        Some("fabricator_standard") => Some(dfm::fabricator_standard()),
        _ => return Err("fab_profile is required: \"none\" or \"fabricator_standard\"".into()),
    };
    Ok(gates::evaluate(p, fab.as_ref()))
}

/// `desk_knowledge`: the DESK rule catalogue, references and component families with their
/// provenance and evidence state (ADR-0062). `rule_id` or `family_id` selects one record.
pub fn desk_knowledge(args: &Value) -> Result<Value, String> {
    use eak_intelligence::domain::knowledge::{knowledge, validate, view};
    let k = knowledge();
    let provenance = json!({
        "source": k.source,
        "licence": k.licence,
        "desk_commit": k.desk_commit,
        "loader_problems": validate(k),
        "note": "knowledge, not EAK limits: DESK parameters are never applied, every claim is capped by its DESK evidence state, and no reference is verified against its source text",
    });
    if let Some(id) = args.get("rule_id").and_then(Value::as_str) {
        let r = k.rule(id).ok_or_else(|| format!("no DESK rule {id}"))?;
        let v = serde_json::to_value(view(r)).map_err(|e| e.to_string())?;
        return Ok(json!({"provenance": provenance, "rule": v}));
    }
    if let Some(id) = args.get("family_id").and_then(Value::as_str) {
        let f = k
            .family(id)
            .ok_or_else(|| format!("no DESK component family {id}"))?;
        let v = serde_json::to_value(f).map_err(|e| e.to_string())?;
        return Ok(json!({"provenance": provenance, "family": v}));
    }
    let rules: Vec<Value> = k
        .rules
        .iter()
        .map(|r| {
            let v = view(r);
            json!({
                "rule_id": r.rule_id,
                "name": r.name,
                "status": r.status,
                "severity": r.severity,
                "scope": r.scope,
                "eak_rules": v.eak_rules,
                "eak_state": v.eak_state,
            })
        })
        .collect();
    let families: Vec<Value> = k
        .families
        .iter()
        .map(|f| json!({"family_id": f.family_id, "name": f.name, "group": f.group}))
        .collect();
    Ok(json!({
        "provenance": provenance,
        "rules": rules,
        "references": k.references,
        "families": families,
    }))
}
