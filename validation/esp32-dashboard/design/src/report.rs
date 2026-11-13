//! Engineering calculations (electrical, thermal, interface) computed with EAK's own
//! `eak-engineering` functions from the values in the design and the pin map.
//!
//! Every input carries its source. Inputs that were not read from a datasheet during this
//! project are marked `verified: false` and the result inherits that status.

use dashboard_core::pins;
use eak_engineering::{calc, ipc};
use serde_json::{json, Value};

pub struct Input {
    pub name: &'static str,
    pub value: f64,
    pub unit: &'static str,
    pub source: &'static str,
    pub verified: bool,
}

const ESP: &str = "ESP32-C3-WROOM-02 datasheet v1.7";
const SHT: &str = "Sensirion SHT4x datasheet v7.3 (June 2026)";

pub fn inputs() -> Vec<Input> {
    vec![
        Input { name: "vbus_nominal", value: 5.0, unit: "V", source: "USB 2.0 nominal", verified: true },
        Input { name: "vbus_min", value: 4.75, unit: "V", source: "USB 2.0 VBUS at device (not re-read in this project)", verified: false },
        Input { name: "vbus_max", value: 5.25, unit: "V", source: "USB 2.0 VBUS (not re-read in this project)", verified: false },
        Input { name: "v3v3", value: 3.3, unit: "V", source: "AMS1117-3.3 nominal output", verified: false },
        Input { name: "module_vdd_min", value: 3.0, unit: "V", source: "ESP32-C3-WROOM-02 datasheet v1.7 Table 6-2", verified: true },
        Input { name: "module_vdd_max", value: 3.6, unit: "V", source: "ESP32-C3-WROOM-02 datasheet v1.7 Table 6-2", verified: true },
        Input { name: "module_peak_current", value: 0.345, unit: "A", source: "ESP32-C3-WROOM-02 datasheet v1.7 Table 6-4 (802.11b TX 20.5 dBm, 100% duty)", verified: true },
        Input { name: "module_run_current", value: 0.028, unit: "A", source: "ESP32-C3-WROOM-02 datasheet v1.7 Table 6-5 (160 MHz CPU running, peripherals clocked, Wi-Fi gated)", verified: true },
        Input { name: "supply_capability_required", value: 0.5, unit: "A", source: "ESP32-C3-WROOM-02 datasheet v1.7 Table 6-2 (IVDD)", verified: true },
        Input { name: "ldo_dropout_max", value: 1.3, unit: "V", source: "AMS1117 datasheet (could not be fetched; EAK built-in library value)", verified: false },
        Input { name: "ldo_iq", value: 0.010, unit: "A", source: "AMS1117 datasheet (could not be fetched; conservative assumption)", verified: false },
        Input { name: "ldo_theta_ja", value: 90.0, unit: "C/W", source: "SOT-223 on small copper area (assumption; AMS1117 datasheet could not be fetched)", verified: false },
        Input { name: "ldo_tj_max", value: 125.0, unit: "C", source: "AMS1117 datasheet (could not be fetched; assumption)", verified: false },
        Input { name: "ambient_max", value: 40.0, unit: "C", source: "Project requirement (indoor dashboard)", verified: true },
        Input { name: "sht40_vdd_max", value: 3.6, unit: "V", source: "Sensirion SHT4x datasheet v7.3 section 3", verified: true },
        Input { name: "sht40_vih_frac", value: 0.7, unit: "xVDD", source: "Sensirion SHT4x datasheet v7.3 Table 4", verified: true },
        Input { name: "esp_vih_frac", value: 0.75, unit: "xVDD", source: "ESP32-C3-WROOM-02 datasheet v1.7 Table 6-3 (extracted text misaligned)", verified: false },
        Input { name: "esp_vol_frac", value: 0.1, unit: "xVDD", source: "ESP32-C3-WROOM-02 datasheet v1.7 Table 6-3 (extracted text misaligned)", verified: false },
        Input { name: "esp_ioh", value: 0.040, unit: "A", source: "ESP32-C3-WROOM-02 datasheet v1.7 Table 6-3 (extracted text misaligned)", verified: false },
        Input { name: "led_vf", value: 2.0, unit: "V", source: "Generic green 0603 LED (no MPN chosen; assumption)", verified: false },
        Input { name: "i2c_bus_capacitance", value: 100e-12, unit: "F", source: "Assumption: board + two 10 cm cables + 2 devices", verified: false },
        Input { name: "i2c_tr_max_standard", value: 1000e-9, unit: "s", source: "NXP UM10204 I2C specification, standard mode (not re-read in this project)", verified: false },
    ]
}

fn get(inp: &[Input], n: &str) -> f64 {
    inp.iter().find(|i| i.name == n).expect(n).value
}

fn verified(inp: &[Input], names: &[&str]) -> bool {
    names
        .iter()
        .all(|n| inp.iter().find(|i| i.name == *n).expect(n).verified)
}

/// One calculation result with pass/fail against its limit.
fn item(name: &str, value: f64, unit: &str, limit: &str, pass: bool, inputs_verified: bool, note: &str) -> Value {
    json!({
        "check": name,
        "value": (value * 1e6).round() / 1e6,
        "unit": unit,
        "limit": limit,
        "result": if !inputs_verified { if pass { "PASS_WITH_UNVERIFIED_INPUTS" } else { "FAIL" } } else if pass { "PASS" } else { "FAIL" },
        "inputs_verified": inputs_verified,
        "note": note,
    })
}

pub fn compute() -> Value {
    let inp = inputs();
    let g = |n| get(&inp, n);
    let mut items = vec![];

    // --- Power / thermal: LDO dissipation, worst case = radio peak at 100 % duty.
    let i_other = 0.002 + 0.0013 + 0.001; // I2C pull-ups/LED/divider, generous
    for (case, i_mod) in [("usb_telemetry_firmware", g("module_run_current")), ("worst_case_wifi_tx_continuous", g("module_peak_current"))] {
        let i = i_mod + i_other;
        let p = calc::linear_reg_dissipation_w(g("vbus_max"), g("v3v3"), i, g("ldo_iq")).unwrap();
        let tj = calc::junction_temp_c(g("ambient_max"), p, g("ldo_theta_ja")).unwrap();
        items.push(item(
            &format!("ldo_dissipation_{case}"),
            p,
            "W",
            "informational",
            true,
            verified(&inp, &["ldo_iq"]),
            &format!("I_load = {:.0} mA at VBUS 5.25 V", i * 1000.0),
        ));
        items.push(item(
            &format!("ldo_junction_temp_{case}"),
            tj,
            "C",
            &format!("<= {} C", g("ldo_tj_max")),
            tj <= g("ldo_tj_max"),
            verified(&inp, &["ldo_theta_ja", "ldo_tj_max", "ldo_iq"]),
            "Tj = Ta(40 C) + P * thetaJA (eak_engineering::calc::junction_temp_c)",
        ));
    }
    // Dropout headroom at minimum USB VBUS.
    let headroom = g("vbus_min") - g("v3v3");
    items.push(item(
        "ldo_dropout_headroom_at_vbus_min",
        headroom,
        "V",
        &format!(">= {} V (AMS1117 max dropout)", g("ldo_dropout_max")),
        headroom >= g("ldo_dropout_max"),
        verified(&inp, &["vbus_min", "ldo_dropout_max"]),
        "4.75 V - 3.3 V = 1.45 V. Margin is small; a low-dropout regulator (e.g. 3.3 V, <= 0.4 V dropout) is recommended for a production revision.",
    ));
    items.push(item(
        "rail_3v3_within_module_range",
        g("v3v3"),
        "V",
        "3.0 .. 3.6 V",
        g("v3v3") >= g("module_vdd_min") && g("v3v3") <= g("module_vdd_max"),
        verified(&inp, &["v3v3", "module_vdd_min", "module_vdd_max"]),
        "AMS1117-3.3 output tolerance not verified (datasheet unavailable)",
    ));

    // --- Track sizing (IPC-2221 via eak-engineering) for the declared 0.5 A power nets.
    let w_req = ipc::trace_width_mm(0.5, false, 1.0, 10.0).unwrap();
    items.push(item(
        "power_track_width_required_0.5A_10C",
        w_req,
        "mm",
        "<= 0.40 mm (Power net class width)",
        w_req <= 0.40,
        true,
        "IPC-2221 external layer, 1 oz, dT 10 C (eak_engineering::ipc::trace_width_mm; formula constants not verified against the standard text)",
    ));
    let i_cap = ipc::trace_current_a(0.40 / 0.0254, false, 1.0, 10.0);
    items.push(item(
        "power_track_capacity_0.40mm",
        i_cap,
        "A",
        ">= 0.5 A",
        i_cap >= 0.5,
        true,
        "IPC-2221 external, 1 oz, dT 10 C",
    ));

    // --- EN RC delay (datasheet recommends R = 10 k, C = 1 uF).
    let tau = calc::rc_time_constant_s(10_000.0, 1e-6).unwrap();
    items.push(item("en_rc_time_constant", tau, "s", "= 10 ms (datasheet recommendation R=10k, C=1uF)", (tau - 0.01).abs() < 1e-9, true, "ESP32-C3-WROOM-02 datasheet v1.7 section 8 note"));

    // --- VBUS sense divider vs ADC range.
    let v_adc = calc::divider_vout(g("vbus_max"), pins::VBUS_DIV_TOP_OHM as f64, pins::VBUS_DIV_BOTTOM_OHM as f64).unwrap();
    items.push(item("vbus_sense_adc_voltage_at_vbus_max", v_adc, "V", "< 2.5 V (ADC 11 dB range used by firmware) and < VDD", v_adc < 2.5, verified(&inp, &["vbus_max"]), "eak_engineering::calc::divider_vout"));
    let i_div = g("vbus_max") / (pins::VBUS_DIV_TOP_OHM + pins::VBUS_DIV_BOTTOM_OHM) as f64;
    items.push(item("vbus_divider_current", i_div, "A", "< 100 uA", i_div < 100e-6, true, ""));

    // --- LED.
    let i_led = (g("v3v3") - g("led_vf")) / pins::LED_SERIES_OHM as f64;
    let r_for_2ma = calc::led_resistor_ohm(g("v3v3"), g("led_vf"), 0.002).unwrap();
    items.push(item("status_led_current", i_led, "A", "<= GPIO IOH (40 mA) and <= 20 mA LED rating", i_led <= g("esp_ioh") && i_led <= 0.020, verified(&inp, &["led_vf", "esp_ioh"]), &format!("R7 = {} ohm; resistor for 2 mA would be {:.0} ohm", pins::LED_SERIES_OHM, r_for_2ma)));
    let p_r7 = calc::resistor_power_w(i_led, pins::LED_SERIES_OHM as f64).unwrap();
    items.push(item("r7_power", p_r7, "W", "<= 0.1 W (0603, unverified)", p_r7 <= 0.1, false, ""));

    // --- I2C: pull-up rise time and sink current, logic thresholds.
    let tr = 0.8473 * calc::rc_time_constant_s(pins::I2C_PULLUP_OHM as f64, g("i2c_bus_capacitance")).unwrap();
    items.push(item("i2c_rise_time", tr, "s", "<= 1000 ns (standard mode, 100 kHz)", tr <= g("i2c_tr_max_standard"), verified(&inp, &["i2c_bus_capacitance", "i2c_tr_max_standard"]), "tr = 0.8473 * Rp * Cb"));
    let i_sink = g("v3v3") / pins::I2C_PULLUP_OHM as f64;
    items.push(item("i2c_pullup_sink_current", i_sink, "A", "<= 3 mA (standard-mode VOL spec)", i_sink <= 0.003, false, "UM10204 3 mA sink limit (not re-read)"));
    let sht_vih = g("sht40_vih_frac") * g("v3v3");
    items.push(item("i2c_logic_high_vs_sht40_vih", g("v3v3"), "V", &format!(">= {:.2} V (0.7*VDD)", sht_vih), g("v3v3") >= sht_vih, true, "Pull-ups to the same +3V3 rail as the sensor VDD"));
    items.push(item("sht40_supply_within_range", g("v3v3"), "V", "1.08 .. 3.6 V", g("v3v3") <= g("sht40_vdd_max") && g("v3v3") >= 1.08, true, SHT));
    items.push(item("usb_data_levels", 3.3, "V", "ESP32-C3 USB Serial/JTAG PHY on GPIO18/19 connected directly (no series resistors)", true, true, ESP));

    json!({
        "title": "Engineering calculations — ESP32-C3 Environment Dashboard",
        "tool": "eak-engineering calc/ipc functions",
        "inputs": inp.iter().map(|i| json!({"name": i.name, "value": i.value, "unit": i.unit, "source": i.source, "verified": i.verified})).collect::<Vec<_>>(),
        "results": items,
        "not_verified": [
            "AMS1117 dropout, quiescent current, thermal resistance and Tj max: datasheet could not be retrieved in this project; values are assumptions.",
            "Thermal results are first-order Tj = Ta + P*thetaJA estimates; no thermal simulation was run (EAK has none).",
            "No measurement on physical hardware.",
        ],
    })
}
