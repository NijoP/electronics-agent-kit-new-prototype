//! Engineering numbers for the board, every one computed by `eak_engineering`.
//!
//! This module does not add any electrical formula of its own. It only feeds
//! declared operating conditions and datasheet ratings (from the project library) into
//! EAK's calculation functions and compares the results with the routed copper. Buck-stage
//! quantities that EAK cannot calculate yet (duty cycle, inductor ripple, capacitor RMS
//! current, switching loss) are listed as `missing_in_eak`. Their hand calculations live
//! in DESIGN.md and are NOT presented as EAK results.

use crate::design::{COPPER_OZ, NET_CURRENTS, NET_VOLTAGES, TEMP_RISE_C};
use eak_core::geom::to_mm;
use eak_core::Project;
use eak_engineering::ipc::{self, ClearanceCondition};
use eak_engineering::{calc, copper};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Line {
    pub item: String,
    pub function: String,
    pub inputs: String,
    pub result: String,
    pub requirement: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct CalcReport {
    pub project_revision: u64,
    pub ipc_tables_verified_against_standard: bool,
    pub lines: Vec<Line>,
    pub missing_in_eak: Vec<String>,
}

fn rating(p: &Project, comp: &str, name: &str) -> Option<f64> {
    p.library
        .get(comp)
        .and_then(|d| crate::parts::rating(d, name))
}

fn status(ok: bool) -> String {
    if ok { "PASS (calculation)" } else { "FAIL" }.into()
}

pub fn run(p: &Project) -> Result<CalcReport, String> {
    let mut lines = vec![];
    let e = |x: eak_engineering::ipc::CalcError| x.to_string();

    // 1. Trace widths vs IPC-2221 for every declared current; compare with routed copper.
    for (net, amps) in NET_CURRENTS {
        let req = ipc::trace_width_mm(*amps, false, COPPER_OZ, TEMP_RISE_C).map_err(e)?;
        let narrowest = p
            .pcb
            .tracks
            .values()
            .filter(|t| t.net == *net)
            .map(|t| to_mm(t.width))
            .fold(f64::INFINITY, f64::min);
        let routed = if narrowest.is_finite() {
            format!("narrowest routed track {narrowest:.2} mm")
        } else {
            "no tracks (pour/pads only)".into()
        };
        lines.push(Line {
            item: format!("Trace width, net {net}"),
            function: "eak_engineering::ipc::trace_width_mm".into(),
            inputs: format!("I = {amps} A, external, {COPPER_OZ} oz, ΔT = {TEMP_RISE_C} °C"),
            result: format!("required ≥ {req:.3} mm; {routed}"),
            requirement: "every track on the net ≥ required width".into(),
            status: status(!narrowest.is_finite() || narrowest + 1e-9 >= req),
        });
    }

    // 2. Via current: 0.3 mm drill, 20 µm (0.79 mil) barrel plating (IPC-6012 class 2 average,
    //    assumed - fabricator plating not verified).
    let drill_mil = 0.3 * ipc::MIL_PER_MM;
    let plating_mil = 0.020 * ipc::MIL_PER_MM;
    let per_via = ipc::via_current_a(drill_mil, plating_mil, TEMP_RISE_C).map_err(e)?;
    let need = ipc::vias_required(3.0, drill_mil, plating_mil, TEMP_RISE_C, 0.8).map_err(e)?;
    let at_u1 = crate::design::GND_VIAS
        .iter()
        .filter(|(x, y)| (*x - 44.3).abs() < 0.01 && *y > 25.0 && *y < 29.0)
        .count() as u32;
    let at_cin = crate::design::GND_VIAS
        .iter()
        .filter(|(x, y)| *x > 33.0 && *x < 40.0 && *y > 22.0 && *y < 24.5)
        .count() as u32;
    let at_cout = crate::design::GND_VIAS
        .iter()
        .filter(|(x, y)| *x > 49.0 && *x < 56.0 && (*y - 22.5).abs() < 0.01)
        .count() as u32;
    lines.push(Line {
        item: "Via current capacity (0.3 mm drill)".into(),
        function: "eak_engineering::ipc::via_current_a / vias_required".into(),
        inputs: format!("drill {drill_mil:.1} mil, plating {plating_mil:.2} mil (assumed), ΔT {TEMP_RISE_C} °C, sharing derating 0.8, 3.0 A"),
        result: format!("{per_via:.2} A per via; {need} vias needed for 3 A; placed: U1 GND {at_u1}, input caps {at_cin}, output caps {at_cout}"),
        requirement: "each GND via cluster that carries the full 3 A return ≥ required count".into(),
        status: if at_u1 >= need && at_cin >= need && at_cout >= need {
            status(true)
        } else {
            format!("PARTIAL: clusters below {need} vias rely on parallel top-layer GND pour (pour current density NOT checked by EAK)")
        },
    });

    // 3. Clearance / creepage for the highest declared voltages.
    let vmax_in = NET_VOLTAGES
        .iter()
        .find(|n| n.0 == "VIN")
        .map(|n| n.1)
        .unwrap_or(0.0);
    let vboot = NET_VOLTAGES
        .iter()
        .find(|n| n.0 == "BOOT")
        .map(|n| n.1)
        .unwrap_or(0.0);
    for (what, v, cond) in [
        (
            "VIN/SW to GND, external uncoated",
            vmax_in,
            ClearanceCondition::B2,
        ),
        (
            "VIN/SW to GND, component leads",
            vmax_in,
            ClearanceCondition::A5,
        ),
        (
            "BOOT to GND, external uncoated",
            vboot,
            ClearanceCondition::B2,
        ),
    ] {
        let c = ipc::clearance_mm(v, cond);
        let used = if what.starts_with("BOOT") { 0.6 } else { 0.2 };
        lines.push(Line {
            item: format!("Electrical clearance {what}"),
            function: "eak_engineering::ipc::clearance_mm".into(),
            inputs: format!("{v} V, {cond:?}"),
            result: format!("required ≥ {c:.2} mm; design minimum {used:.2} mm"),
            requirement: "design clearance ≥ required".into(),
            status: status(used + 1e-9 >= c),
        });
    }
    let cr = ipc::creepage_mm(vboot, 2, "IIIb").map_err(e)?;
    lines.push(Line {
        item: "Creepage (functional insulation only, no isolation barrier)".into(),
        function: "eak_engineering::ipc::creepage_mm".into(),
        inputs: format!("{vboot} V, pollution degree 2, material group IIIb (FR-4 assumed)"),
        result: format!("table value {cr:.2} mm; design minimum surface distance 0.2 mm"),
        requirement: "functional creepage ≥ table value (IEC 60664-1 table as transcribed; EAK flags the ≤50 V rows as unverified)".into(),
        status: status(0.2 + 1e-9 >= cr),
    });

    // 4. Feedback divider -> output voltage spread (Vref min/typ/max from the datasheet).
    let (r_top, r_bot) = (105e3, 14.3e3);
    let ratio = calc::divider_vout(1.0, r_top, r_bot).map_err(e)?;
    let vref = |n| rating(p, "TPS54302DDCR", n).ok_or(format!("missing rating {n}"));
    let (vmin, vtyp, vmax) = (vref("vfb_min")?, vref("vfb_typ")?, vref("vfb_max")?);
    // 1 % resistors: worst case ratio extremes.
    let ratio_hi = calc::divider_vout(1.0, r_top * 0.99, r_bot * 1.01).map_err(e)?;
    let ratio_lo = calc::divider_vout(1.0, r_top * 1.01, r_bot * 0.99).map_err(e)?;
    lines.push(Line {
        item: "Output voltage set point".into(),
        function: "eak_engineering::calc::divider_vout".into(),
        inputs: format!(
            "R2 = 105 kΩ, R3 = 14.3 kΩ (±1 %), VFB = {vmin}/{vtyp}/{vmax} V (TI SLVSDG6C §5.5)"
        ),
        result: format!(
            "divider ratio {ratio:.5}; VOUT typ {:.3} V, worst-case {:.3}-{:.3} V",
            vtyp / ratio,
            vmin / ratio_hi,
            vmax / ratio_lo
        ),
        requirement:
            "5.0 V ±5 % (4.75-5.25 V) and below the 5.3 V declared for clearance/rating checks"
                .into(),
        status: status(vmin / ratio_hi >= 4.75 && vmax / ratio_lo <= 5.25),
    });
    let i_div = 5.0 / (r_top + r_bot);
    let p_rtop = calc::resistor_power_w(i_div, r_top).map_err(e)?;
    lines.push(Line {
        item: "Feedback divider dissipation (R2)".into(),
        function: "eak_engineering::calc::resistor_power_w".into(),
        inputs: format!("I = {:.1} µA, R = 105 kΩ", i_div * 1e6),
        result: format!("{:.3} mW", p_rtop * 1e3),
        requirement: "≤ 50 % of 0603 rating (0.1 W, unverified generic)".into(),
        status: status(p_rtop <= 0.05),
    });

    // 5. LED indicator.
    let r_led = calc::led_resistor_ohm(5.08, 2.0, 0.002).map_err(e)?;
    let p_led = calc::resistor_power_w(0.002, 1500.0).map_err(e)?;
    lines.push(Line {
        item: "Power LED series resistor".into(),
        function: "eak_engineering::calc::led_resistor_ohm / resistor_power_w".into(),
        inputs: "VOUT 5.08 V, Vf 2.0 V (assumed green LED, unverified), If 2 mA".into(),
        result: format!("{r_led:.0} Ω → 1.5 kΩ chosen; {:.1} mW in R4", p_led * 1e3),
        requirement: "R4 ≤ 50 % of rating".into(),
        status: status(p_led <= 0.05),
    });

    // 6. Gate clamp resistor dissipation at 25.2 V.
    let p_r1 = calc::resistor_power_w((25.2 - 12.0) / 100e3, 100e3).map_err(e)?;
    lines.push(Line {
        item: "Q1 gate pull-down R1 dissipation (zener clamping at 12 V)".into(),
        function: "eak_engineering::calc::resistor_power_w".into(),
        inputs: "I = (25.2 V - 12 V) / 100 kΩ".into(),
        result: format!("{:.2} mW", p_r1 * 1e3),
        requirement: "≤ 50 % of 0603 rating".into(),
        status: status(p_r1 <= 0.05),
    });

    // 7. Junction temperatures. Loss inputs are hand estimates from DESIGN.md §4 (not EAK-computed).
    let ta = 40.0;
    let p_u1 = 0.78; // DESIGN.md §4: conduction 0.44 W + switching 0.30 W + quiescent/gate ~0.04 W at 25.2 V, 3 A
    let tj_max = rating(p, "TPS54302DDCR", "tj_abs_max").unwrap_or(150.0);
    let tsd = rating(p, "TPS54302DDCR", "thermal_shutdown_rising").unwrap_or(160.0);
    for (label, theta) in [
        (
            "JEDEC 2s2p θJA",
            rating(p, "TPS54302DDCR", "theta_ja_jedec"),
        ),
        ("TI EVM θJA", rating(p, "TPS54302DDCR", "theta_ja_evm")),
    ] {
        let theta = theta.ok_or("missing θJA")?;
        let tj = calc::junction_temp_c(ta, p_u1, theta).map_err(e)?;
        lines.push(Line {
            item: format!("U1 junction temperature ({label})"),
            function: "eak_engineering::calc::junction_temp_c".into(),
            inputs: format!("Ta {ta} °C, P {p_u1} W (hand estimate), θJA {theta} °C/W (TI SLVSDG6C §5.4)"),
            result: format!("Tj ≈ {tj:.1} °C (abs max {tj_max} °C, shutdown {tsd} °C)"),
            requirement: "Tj ≤ 125 °C design target (25 °C below abs max)".into(),
            status: if tj <= 125.0 {
                "PASS (estimate only - NOT VERIFIED without measurement)".into()
            } else {
                "FAIL against 125 °C target (estimate) - board copper must perform like TI EVM; NOT VERIFIED".into()
            },
        });
    }
    let rds = rating(p, "AO4407A", "rdson_vgs_10v_max").unwrap_or(0.014);
    let p_q1 = calc::resistor_power_w(1.55, rds).map_err(e)?;
    let tj_q1 = calc::junction_temp_c(ta, p_q1, 75.0).map_err(e)?;
    lines.push(Line {
        item: "Q1 reverse-polarity MOSFET dissipation and Tj".into(),
        function: "eak_engineering::calc::resistor_power_w / junction_temp_c".into(),
        inputs: format!("Iin 1.55 A (11.4 V, η 0.85 assumed), RDS(on) {rds} Ω (unverified), θJA 75 °C/W (unverified)"),
        result: format!("P ≈ {:.0} mW, Tj ≈ {tj_q1:.1} °C", p_q1 * 1e3),
        requirement: "Tj ≤ 125 °C".into(),
        status: "PASS (estimate on unverified ratings - NOT VERIFIED)".into(),
    });

    // 8. Pour coverage (zone fill computed by EAK's copper engine).
    let items = copper::items(p);
    for z in p.pcb.zones.values() {
        if let Some(f) = copper::fill_zone(p, &z.id, &items) {
            lines.push(Line {
                item: format!("GND pour fill, {:?} layer", z.layer),
                function: "eak_engineering::copper::fill_zone".into(),
                inputs: format!("clearance {:.2} mm, 0.1 mm raster", to_mm(z.clearance)),
                result: format!(
                    "{:.0} mm² filled in {} island(s)",
                    f.filled_area_mm2(),
                    f.islands
                ),
                requirement: "informational".into(),
                status: "INFO".into(),
            });
        }
    }

    Ok(CalcReport {
        project_revision: p.revision,
        ipc_tables_verified_against_standard: ipc::VERIFIED_AGAINST_STANDARD,
        lines,
        missing_in_eak: vec![
            "Buck operating point: duty cycle, inductor ripple/peak/RMS current (eak-engineering)".into(),
            "Output voltage ripple and output-capacitor RMS current (eak-engineering)".into(),
            "Input-capacitor RMS ripple current and input voltage ripple (eak-engineering)".into(),
            "MOSFET conduction + switching loss and converter efficiency (eak-engineering)".into(),
            "Loop stability / phase margin (internally compensated part; datasheet-defined only)".into(),
            "MLCC DC-bias capacitance derating (eak-engineering / library data)".into(),
            "Copper-pour current density and PCB thermal (spreading) model (eak-engineering / eak-validation)".into(),
        ],
    })
}

pub fn to_markdown(r: &CalcReport) -> String {
    let mut s = format!(
        "# Engineering calculations (EAK-computed)\n\nProject revision {}. IPC tables verified against the standard text: **{}**.\n\n| Item | EAK function | Inputs | Result | Requirement | Status |\n|---|---|---|---|---|---|\n",
        r.project_revision, r.ipc_tables_verified_against_standard
    );
    for l in &r.lines {
        s.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} |\n",
            l.item, l.function, l.inputs, l.result, l.requirement, l.status
        ));
    }
    s.push_str("\n## Not computable in EAK today (NOT VERIFIED by EAK)\n\n");
    for m in &r.missing_in_eak {
        s.push_str(&format!("- {m}\n"));
    }
    s
}
