//! Copper and power-distribution engineering (integration directive §3F).
//!
//! Deterministic, unit-aware calculations over typed [`PhysicalQuantity`] inputs. Every input
//! that changes a conclusion (current, copper thickness, temperature rise, conductor
//! temperature) is a required argument: nothing here substitutes a default. Project analysis
//! reads only values declared in the EAK-IR and reports what it could not evaluate.
//!
//! Knowledge source: DESK `desk/power_distribution.py` and `desk/copper.py` (commit
//! 5ad8492c37a5dac919d97f4eccd8250780490e7a, MIT). Re-implemented, not translated line by
//! line; unlike DESK, no 1 oz / 10 °C fallback is applied when the profile is silent.
//!
//! Sources and applicability:
//! * Trace width: IPC-2221 conductor sizing `I = k·ΔT^0.44·A^0.725` (k = 0.048 external,
//!   0.024 internal), via [`crate::ipc`]; `VERIFIED_AGAINST_STANDARD = false`. The IPC-2221
//!   charts the formula fits are commonly cited for 0–35 A, widths up to 400 mil (10.16 mm),
//!   ΔT 10–100 °C and 0.5–3 oz copper; outside that range the result is flagged as
//!   extrapolated. IPC-2152 (which supersedes the 2221 charts) is not implemented.
//! * Resistance: `R = ρ(T)·L / (w·t)`, `ρ(T) = ρ20·(1 + α20·(T − 20 °C))`, with
//!   ρ20 = 1.7241e-8 Ω·m and α20 = 0.00393 /K for annealed copper (IEC 60028, 100 % IACS).
//!   Plated/electrodeposited PCB copper is typically a few percent more resistive, so results
//!   are a lower bound on resistance; the result says so.

use crate::copper;
use crate::ipc::{self, CalcError};
use eak_core::geom::{point_in_polygon, to_mm};
use eak_core::units::{Dimension, PhysicalQuantity, Unit};
use eak_core::{CopperLayer, Nm, Point, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Annealed-copper resistivity at 20 °C (IEC 60028, 100 % IACS), Ω·m.
pub const RHO_CU_20C_OHM_M: f64 = 1.7241e-8;
/// Temperature coefficient of resistance of annealed copper at 20 °C (IEC 60028), 1/K.
pub const ALPHA_CU_20C_PER_K: f64 = 0.00393;
/// Copper thickness per oz/ft², µm (same nominal as [`ipc::MIL_PER_OZ`] = 1.37 mil).
pub const COPPER_UM_PER_OZ: f64 = ipc::MIL_PER_OZ * 25.4;
/// 20 °C in kelvin: the reference temperature of [`RHO_CU_20C_OHM_M`].
pub const T20_K: f64 = 293.15;

const SOURCE_IPC2221: &str = "IPC-2221 conductor sizing I = k·ΔT^0.44·A^0.725 (k 0.048 external / 0.024 internal); transcribed from secondary sources via DESK desk/ipc.py; verified_against_standard = false; IPC-2152 not implemented";
const SOURCE_RHO: &str = "R = ρ(T)·L/(w·t), ρ20 = 1.7241e-8 Ω·m, α20 = 0.00393/K (IEC 60028 annealed copper, 100 % IACS)";

/// Where a conductor runs: drives the IPC-2221 constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerPosition {
    External,
    Internal,
}

/// Whether the inputs lie inside the range the formula's source data covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Applicability {
    pub within_source_range: bool,
    /// One line per input outside the source range (empty when within range).
    pub notes: Vec<String>,
}

fn invalid(msg: impl Into<String>) -> CalcError {
    CalcError::InvalidInput(msg.into())
}

/// SI magnitude of a required, strictly positive quantity of dimension `dim`.
fn positive(q: &PhysicalQuantity, dim: Dimension, name: &str) -> Result<f64, CalcError> {
    if q.dimension() != dim {
        return Err(invalid(format!(
            "{name} must be a {dim:?} quantity, got unit {:?}",
            q.unit
        )));
    }
    let v = q.si_magnitude();
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(invalid(format!("{name} must be a positive finite value")))
    }
}

fn rise_k(q: &PhysicalQuantity) -> Result<f64, CalcError> {
    let v = q
        .temperature_difference_k()
        .map_err(|e| invalid(format!("temperature rise: {e}")))?;
    if v > 0.0 {
        Ok(v)
    } else {
        Err(invalid("temperature rise must be > 0"))
    }
}

/// Copper thickness of a copper weight (oz/ft²), nominal.
pub fn copper_thickness_from_oz(oz: f64) -> Result<PhysicalQuantity, CalcError> {
    if oz.is_finite() && oz > 0.0 {
        Ok(PhysicalQuantity::new(
            oz * COPPER_UM_PER_OZ,
            Unit::Micrometre,
        ))
    } else {
        Err(invalid("copper weight must be a positive number of oz/ft²"))
    }
}

/// Inputs to [`trace_width`]; all required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceWidthInput {
    pub current: PhysicalQuantity,
    pub copper_thickness: PhysicalQuantity,
    pub temperature_rise: PhysicalQuantity,
    pub layer: LayerPosition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraceWidthResult {
    pub width: PhysicalQuantity,
    pub cross_section_mil2: f64,
    pub input: TraceWidthInput,
    pub source: String,
    pub verified_against_standard: bool,
    pub applicability: Applicability,
}

/// Minimum conductor width for a continuous current (IPC-2221).
pub fn trace_width(input: &TraceWidthInput) -> Result<TraceWidthResult, CalcError> {
    let amps = positive(&input.current, Dimension::Current, "current")?;
    let t_um = positive(
        &input.copper_thickness,
        Dimension::Length,
        "copper thickness",
    )? * 1e6;
    let rise = rise_k(&input.temperature_rise)?;
    let internal = input.layer == LayerPosition::Internal;
    let oz = t_um / COPPER_UM_PER_OZ;
    let area = ipc::trace_area_mil2(amps, internal, rise)?;
    let width_mil = ipc::trace_width_mil(amps, internal, oz, rise)?;
    let mut notes = vec![];
    if amps > 35.0 {
        notes.push(format!("current {amps} A is above the 35 A chart range"));
    }
    if width_mil > 400.0 {
        notes.push(format!(
            "width {width_mil:.0} mil is above the 400 mil chart range"
        ));
    }
    if !(10.0..=100.0).contains(&rise) {
        notes.push(format!(
            "temperature rise {rise} K is outside the 10–100 K chart range"
        ));
    }
    if !(0.5..=3.0).contains(&oz) {
        notes.push(format!(
            "copper {oz:.2} oz/ft² ({t_um:.1} µm) is outside the 0.5–3 oz chart range"
        ));
    }
    Ok(TraceWidthResult {
        width: PhysicalQuantity::new(width_mil / ipc::MIL_PER_MM, Unit::Millimetre),
        cross_section_mil2: area,
        input: input.clone(),
        source: SOURCE_IPC2221.into(),
        verified_against_standard: ipc::VERIFIED_AGAINST_STANDARD,
        applicability: Applicability {
            within_source_range: notes.is_empty(),
            notes,
        },
    })
}

/// Inputs to [`voltage_drop`]; all required except the supply voltage, which only adds the
/// percentage drop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConductorInput {
    pub current: PhysicalQuantity,
    pub length: PhysicalQuantity,
    pub width: PhysicalQuantity,
    pub copper_thickness: PhysicalQuantity,
    /// Absolute conductor temperature (e.g. ambient plus the allowed rise).
    pub conductor_temperature: PhysicalQuantity,
    #[serde(default)]
    pub supply_voltage: Option<PhysicalQuantity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoltageDropResult {
    pub resistance: PhysicalQuantity,
    pub drop: PhysicalQuantity,
    pub dissipation: PhysicalQuantity,
    /// Percent of the supply voltage, when one was given.
    pub drop_percent: Option<f64>,
    pub input: ConductorInput,
    pub source: String,
    pub assumptions: Vec<String>,
}

/// Resistance (Ω) of a uniform rectangular copper conductor at an absolute temperature;
/// lengths in metres, temperature in kelvin.
pub fn copper_resistance_ohm(
    length_m: f64,
    width_m: f64,
    thickness_m: f64,
    temperature_k: f64,
) -> Result<f64, CalcError> {
    for (n, v) in [
        ("length", length_m),
        ("width", width_m),
        ("thickness", thickness_m),
        ("temperature", temperature_k),
    ] {
        if !(v.is_finite() && v > 0.0) {
            return Err(invalid(format!("{n} must be positive and finite")));
        }
    }
    let rho = RHO_CU_20C_OHM_M * (1.0 + ALPHA_CU_20C_PER_K * (temperature_k - T20_K));
    if rho <= 0.0 {
        return Err(CalcError::OutOfRange(
            "temperature far below the linear resistivity model's range".into(),
        ));
    }
    Ok(rho * length_m / (width_m * thickness_m))
}

/// DC voltage drop along one conductor.
pub fn voltage_drop(input: &ConductorInput) -> Result<VoltageDropResult, CalcError> {
    let amps = positive(&input.current, Dimension::Current, "current")?;
    let len = positive(&input.length, Dimension::Length, "length")?;
    let w = positive(&input.width, Dimension::Length, "width")?;
    let t = positive(
        &input.copper_thickness,
        Dimension::Length,
        "copper thickness",
    )?;
    let temp = positive(
        &input.conductor_temperature,
        Dimension::Temperature,
        "conductor temperature",
    )?;
    let r = copper_resistance_ohm(len, w, t, temp)?;
    let drop = amps * r;
    let pct = match &input.supply_voltage {
        Some(v) => Some(drop / positive(v, Dimension::Voltage, "supply voltage")? * 100.0),
        None => None,
    };
    Ok(VoltageDropResult {
        resistance: PhysicalQuantity::new(r, Unit::Ohm),
        drop: PhysicalQuantity::new(drop, Unit::Volt),
        dissipation: PhysicalQuantity::new(amps * drop, Unit::Watt),
        drop_percent: pct,
        input: input.clone(),
        source: SOURCE_RHO.into(),
        assumptions: vec![
            "DC, uniform rectangular cross-section; no skin effect".into(),
            "annealed-copper resistivity: plated PCB copper is usually a few percent more resistive, so this is a lower bound on R and drop".into(),
        ],
    })
}

/// Evaluation status of one project-level estimate. `Unknown` is never a pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", content = "reason", rename_all = "snake_case")]
pub enum Status {
    Estimated,
    Unknown(String),
}

/// Track-only DC drop estimate for one net with a declared current.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RailDrop {
    pub net: String,
    pub current_a: f64,
    pub voltage_v: Option<f64>,
    pub tracks: usize,
    pub track_length_mm: f64,
    /// Sum of all track segment resistances of the net at 20 °C.
    pub series_resistance_20c_ohm: Option<f64>,
    /// `current × series resistance`: an upper bound (every segment carries the full current)
    /// at 20 °C conductor temperature; add 0.393 %/K for a warmer conductor.
    pub drop_upper_bound_v: Option<f64>,
    pub drop_percent: Option<f64>,
    pub status: Status,
}

/// A stretch of a signal track over a gap in its adjacent reference plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnPathGap {
    pub track_id: String,
    pub net: String,
    pub layer: CopperLayer,
    pub reference_layer: CopperLayer,
    pub reference_net: String,
    pub gap_length_mm: f64,
    pub at: Point,
}

/// A reference-plane zone whose fill is split into several islands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaneSplit {
    pub zone_id: String,
    pub net: String,
    pub layer: CopperLayer,
    pub islands: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReturnPathReport {
    pub status: Status,
    pub reference_nets: Vec<String>,
    /// Shortest uncovered run along a track that is reported (smaller voids, e.g. via
    /// anti-pads, are ignored).
    pub min_gap_mm: f64,
    pub gaps: Vec<ReturnPathGap>,
    pub plane_splits: Vec<PlaneSplit>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PowerAnalysis {
    pub revision: u64,
    pub rails: Vec<RailDrop>,
    /// Nets with a declared current but no track or zone: nothing carries that current yet.
    pub unrouted_declared_currents: BTreeMap<String, f64>,
    pub return_path: ReturnPathReport,
    pub source: String,
}

/// Reference (ground) nets by name: `GND*`, `*GND`, `VSS`, `0V` (case-insensitive).
/// The IR has no net-role field yet, so this is a naming convention and is reported as such.
pub fn is_reference_net(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    n.starts_with("GND") || n.ends_with("GND") || n == "VSS" || n == "0V"
}

/// Track-only drop per net with a declared current.
pub fn rail_drops(p: &Project) -> Vec<RailDrop> {
    let c = &p.constraints;
    let st = &p.pcb.board.stackup;
    let mut out = vec![];
    for (net, &amps) in &c.net_currents_a {
        let voltage_v = c.net_voltages_v.get(net).copied();
        let tracks: Vec<_> = p.pcb.tracks.values().filter(|t| &t.net == net).collect();
        let length_mm: f64 = tracks.iter().map(|t| t.a.dist(t.b) / 1e6).sum();
        let mut rail = RailDrop {
            net: net.clone(),
            current_a: amps,
            voltage_v,
            tracks: tracks.len(),
            track_length_mm: length_mm,
            series_resistance_20c_ohm: None,
            drop_upper_bound_v: None,
            drop_percent: None,
            status: Status::Estimated,
        };
        if !(amps.is_finite() && amps > 0.0) {
            rail.status = Status::Unknown("declared current is not a positive number".into());
        } else if tracks.is_empty() {
            rail.status = Status::Unknown("no tracks on this net".into());
        } else if p.pcb.zones.values().any(|z| &z.net == net) {
            rail.status = Status::Unknown(
                "net has a copper zone; conduction through zones is not modelled".into(),
            );
        } else {
            let mut r = 0.0;
            let mut bad = None;
            for t in &tracks {
                let len_m = t.a.dist(t.b) * 1e-9;
                if len_m <= 0.0 {
                    continue;
                }
                let r_t = st
                    .copper_oz(t.layer)
                    .ok_or_else(|| invalid(format!("layer {} not in stackup", t.layer.name())))
                    .and_then(copper_thickness_from_oz)
                    .and_then(|th| {
                        copper_resistance_ohm(
                            len_m,
                            t.width as f64 * 1e-9,
                            th.si_magnitude(),
                            T20_K,
                        )
                    });
                match r_t {
                    Ok(v) => r += v,
                    Err(e) => {
                        bad = Some(format!("track {}: {e}", t.id));
                        break;
                    }
                }
            }
            if let Some(why) = bad {
                rail.status = Status::Unknown(why);
            } else {
                rail.series_resistance_20c_ohm = Some(r);
                rail.drop_upper_bound_v = Some(amps * r);
                rail.drop_percent = voltage_v
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .map(|v| amps * r / v * 100.0);
            }
        }
        out.push(rail);
    }
    out
}

/// Return-path continuity: signal tracks whose adjacent reference plane has a gap under them,
/// and reference zones split into islands. `min_gap` is the shortest reported gap.
pub fn return_path(p: &Project, min_gap: Nm) -> ReturnPathReport {
    let st = &p.pcb.board.stackup;
    let items = copper::items(p);
    let fills: Vec<copper::ZoneFill> = p
        .pcb
        .zones
        .iter()
        .filter(|(_, z)| is_reference_net(&z.net))
        .filter_map(|(id, _)| copper::fill_zone(p, id, &items))
        .collect();
    let mut reference_nets: Vec<String> = fills.iter().map(|f| f.net.clone()).collect();
    reference_nets.sort();
    reference_nets.dedup();
    let limitations = vec![
        "reference nets are found by name (GND*, *GND, VSS, 0V); the IR has no net-role field".into(),
        "only the adjacent copper layer is taken as the reference; power planes as references are not evaluated".into(),
        "zone fill is a 0.1 mm raster; gaps are measured by sampling the track centreline".into(),
        "high-frequency return current spreading, stitching-via pitch and plane-edge effects are not evaluated".into(),
    ];
    if fills.is_empty() {
        return ReturnPathReport {
            status: Status::Unknown(
                "no reference (ground) zone on the board: return-path continuity not evaluated"
                    .into(),
            ),
            reference_nets,
            min_gap_mm: to_mm(min_gap),
            gaps: vec![],
            plane_splits: vec![],
            limitations,
        };
    }
    let plane_splits = fills
        .iter()
        .filter(|f| f.islands > 1)
        .map(|f| PlaneSplit {
            zone_id: f.zone_id.clone(),
            net: f.net.clone(),
            layer: f.layer,
            islands: f.islands,
        })
        .collect();
    let step: i64 = copper::ZONE_FILL_PITCH / 2;
    let mut gaps = vec![];
    for t in p.pcb.tracks.values() {
        if is_reference_net(&t.net) {
            continue;
        }
        let Some(ti) = st.index_of(t.layer) else {
            continue;
        };
        for f in &fills {
            let Some(fi) = st.index_of(f.layer) else {
                continue;
            };
            if ti.abs_diff(fi) != 1 {
                continue;
            }
            let zone_outline: &[Point] = p
                .pcb
                .zones
                .get(&f.zone_id)
                .map(|z| z.outline.as_slice())
                .unwrap_or(&[]);
            let len = t.a.dist(t.b).round() as i64;
            let n = (len / step).max(1);
            let mut any_ref = false;
            let mut run: i64 = 0;
            let mut run_start = t.a;
            let mut worst: Option<(i64, Point)> = None;
            for k in 0..=n {
                let pt = Point::new(
                    t.a.x + (t.b.x - t.a.x) * k / n,
                    t.a.y + (t.b.y - t.a.y) * k / n,
                );
                // Only stretches inside the plane's own outline count: a track leaving the
                // plane area entirely is a different (unreferenced-routing) question.
                let inside = point_in_polygon(pt, zone_outline);
                if inside && f.cell_at(pt) != 0 {
                    any_ref = true;
                    run = 0;
                } else if inside {
                    if run == 0 {
                        run_start = pt;
                    }
                    run += 1;
                    let gap = run * len / n;
                    if worst.is_none_or(|(g, _)| gap > g) {
                        worst = Some((gap, run_start));
                    }
                } else {
                    run = 0;
                }
            }
            if let (true, Some((gap, at))) = (any_ref, worst) {
                if gap >= min_gap {
                    gaps.push(ReturnPathGap {
                        track_id: t.id.clone(),
                        net: t.net.clone(),
                        layer: t.layer,
                        reference_layer: f.layer,
                        reference_net: f.net.clone(),
                        gap_length_mm: to_mm(gap),
                        at,
                    });
                }
            }
        }
    }
    ReturnPathReport {
        status: Status::Estimated,
        reference_nets,
        min_gap_mm: to_mm(min_gap),
        gaps,
        plane_splits,
        limitations,
    }
}

/// Default shortest reported return-path gap: 2 mm (larger than a typical via anti-pad).
/// A reporting threshold, not an engineering limit; callers may pass their own.
pub const RETURN_GAP_MIN: Nm = 2_000_000;

/// Rail drops and return-path continuity for the current revision.
pub fn analyse(p: &Project) -> PowerAnalysis {
    PowerAnalysis {
        revision: p.revision,
        rails: rail_drops(p),
        unrouted_declared_currents: p
            .constraints
            .net_currents_a
            .iter()
            .filter(|(n, _)| {
                !p.pcb.tracks.values().any(|t| &t.net == *n)
                    && !p.pcb.zones.values().any(|z| &z.net == *n)
            })
            .map(|(n, a)| (n.clone(), *a))
            .collect(),
        return_path: return_path(p, RETURN_GAP_MIN),
        source: SOURCE_RHO.into(),
    }
}

#[cfg(test)]
mod tests {
    // Written during the testing freeze; never compiled or run.
    use super::*;

    fn q(v: f64, u: Unit) -> PhysicalQuantity {
        PhysicalQuantity::new(v, u)
    }

    #[test]
    fn trace_width_matches_ipc_module() {
        let r = trace_width(&TraceWidthInput {
            current: q(1.0, Unit::Ampere),
            copper_thickness: copper_thickness_from_oz(1.0).unwrap(),
            temperature_rise: q(10.0, Unit::DegreeCelsius),
            layer: LayerPosition::External,
        })
        .unwrap();
        let want = ipc::trace_width_mm(1.0, false, 1.0, 10.0).unwrap();
        assert!((r.width.magnitude - want).abs() < 1e-9);
        assert!(r.applicability.within_source_range);
        assert!(!r.verified_against_standard);
    }

    #[test]
    fn trace_width_rejects_wrong_dimension_and_flags_extrapolation() {
        let bad = TraceWidthInput {
            current: q(1.0, Unit::Volt),
            copper_thickness: q(35.0, Unit::Micrometre),
            temperature_rise: q(10.0, Unit::Kelvin),
            layer: LayerPosition::External,
        };
        assert!(trace_width(&bad).is_err());
        let hot = TraceWidthInput {
            current: q(40.0, Unit::Ampere),
            ..bad
        };
        let r = trace_width(&hot).unwrap();
        assert!(!r.applicability.within_source_range);
    }

    #[test]
    fn voltage_drop_of_known_conductor() {
        // 100 mm × 1 mm × 35 µm at 20 °C: R = 1.7241e-8 · 0.1 / (1e-3 · 35e-6) ≈ 49.26 mΩ.
        let r = voltage_drop(&ConductorInput {
            current: q(2.0, Unit::Ampere),
            length: q(100.0, Unit::Millimetre),
            width: q(1.0, Unit::Millimetre),
            copper_thickness: q(35.0, Unit::Micrometre),
            conductor_temperature: q(20.0, Unit::DegreeCelsius),
            supply_voltage: Some(q(5.0, Unit::Volt)),
        })
        .unwrap();
        assert!((r.resistance.magnitude - 0.049_26).abs() < 1e-4);
        assert!((r.drop.magnitude - 0.098_52).abs() < 2e-4);
        assert!((r.drop_percent.unwrap() - 1.97).abs() < 0.01);
    }

    #[test]
    fn resistance_rises_with_temperature() {
        let cold = copper_resistance_ohm(0.1, 1e-3, 35e-6, T20_K).unwrap();
        let hot = copper_resistance_ohm(0.1, 1e-3, 35e-6, T20_K + 10.0).unwrap();
        assert!((hot / cold - 1.0393).abs() < 1e-9);
    }

    #[test]
    fn empty_board_has_unknown_return_path() {
        let p = Project::new("t");
        let r = return_path(&p, RETURN_GAP_MIN);
        assert!(matches!(r.status, Status::Unknown(_)));
    }

    #[test]
    fn reference_net_names() {
        assert!(is_reference_net("GND"));
        assert!(is_reference_net("agnd"));
        assert!(is_reference_net("GND_PWR"));
        assert!(!is_reference_net("VCC"));
    }
}
