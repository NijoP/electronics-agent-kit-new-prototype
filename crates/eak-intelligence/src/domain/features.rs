//! Deterministic feature extraction from an EAK-IR design (port of DESK
//! `extract_requirements`, without a regex dependency). Signals come from the project name,
//! requirement texts, placed component definitions (name, category, value, MPN) and net label
//! names. Nothing is invented: an absent signal is simply absent.

use eak_core::Project;
use serde_json::{json, Map, Value};

/// One piece of design text and where it came from (used to explain a match).
#[derive(Debug, Clone)]
pub struct Source {
    pub origin: String,
    pub text: String,
}

pub type Features = Map<String, Value>;

const PROTOCOLS: &[(&str, &[&str])] = &[
    ("usb", &["usb", "usb-c", "type-c", "usb c"]),
    ("can", &["can bus", "canbus", "can-fd", "canh", "canl"]),
    ("rs485", &["rs485", "rs-485", "modbus"]),
    ("ethernet", &["ethernet", "rj45", "rmii", "rgmii"]),
    ("gigabit", &["gigabit", "1000base"]),
    ("wifi", &["wifi", "wi-fi", "802.11"]),
    ("ble", &["ble", "bluetooth"]),
    ("lora", &["lora", "lorawan"]),
    (
        "lte",
        &["lte", "nb-iot", "nbiot", "cat-m", "cellular", "4g"],
    ),
    ("gnss", &["gnss", "gps", "glonass", "galileo"]),
    ("hdmi", &["hdmi"]),
    ("mipi", &["mipi", "csi-2", "csi2"]),
    ("pcie", &["pcie", "pci-e", "pci express", "nvme", "m.2"]),
    ("ddr", &["ddr", "ddr2", "ddr3", "ddr4", "lpddr", "sdram"]),
    ("sata", &["sata"]),
    ("i2c", &["i2c", "i²c"]),
    ("spi", &["spi", "qspi"]),
    ("uart", &["uart", "serial"]),
    ("zigbee", &["zigbee", "thread", "matter"]),
    ("uwb", &["uwb", "ultra-wideband", "ultrawideband"]),
    (
        "subghz",
        &["sub-ghz", "subghz", "433 mhz", "433mhz", "315 mhz"],
    ),
    ("poe", &["poe", "power over ethernet"]),
    ("lin", &["lin bus"]),
    ("i2s", &["i2s", "pdm"]),
];

const POWER_SOURCE: &[(&str, &[&str])] = &[
    (
        "battery",
        &[
            "battery",
            "batteries",
            "li-ion",
            "lithium",
            "li-po",
            "lipo",
            "lifepo4",
            "cell",
            "cells",
            "coin cell",
            "cr2032",
        ],
    ),
    ("usb", &["usb", "usb-c", "vbus"]),
    (
        "mains",
        &[
            "mains",
            "ac input",
            "110 vac",
            "115 vac",
            "120 vac",
            "230 vac",
            "240 vac",
            "110vac",
            "230vac",
            "ac-dc",
            "acdc",
            "offline",
            "line voltage",
            "wall power",
        ],
    ),
    ("solar", &["solar", "pv", "photovoltaic", "mppt"]),
    (
        "vehicle",
        &["vehicle", "automotive", "car", "truck", "obd", "load dump"],
    ),
    ("poe", &["poe", "power over ethernet"]),
    ("dc_input", &["dc input", "dc barrel", "barrel jack"]),
];

const MOTOR: &[(&str, &[&str])] = &[
    (
        "bldc",
        &[
            "bldc",
            "brushless",
            "pmsm",
            "foc",
            "esc",
            "three-phase motor",
            "3-phase motor",
        ],
    ),
    ("brushed", &["brushed", "h-bridge", "h bridge", "dc motor"]),
    (
        "stepper",
        &["stepper", "step/dir", "microstep", "microstepping"],
    ),
    ("servo", &["servo"]),
    ("ac", &["ac motor", "induction motor", "vfd"]),
    ("solenoid", &["solenoid", "valve"]),
    ("fan", &["fan", "pump"]),
];

const CONVERTER: &[(&str, &[&str])] = &[
    ("buck", &["buck", "step-down", "step down"]),
    ("boost", &["boost", "step-up", "step up"]),
    ("buck_boost", &["buck-boost", "buck boost"]),
    ("sepic", &["sepic"]),
    ("flyback", &["flyback"]),
    ("forward", &["forward converter", "active clamp"]),
    ("push_pull", &["push-pull", "push pull"]),
    ("half_bridge", &["half-bridge", "half bridge", "llc"]),
    (
        "full_bridge",
        &["full-bridge", "full bridge", "phase-shift"],
    ),
    ("dab", &["dual active bridge", "dual-active-bridge", "dab"]),
    ("pfc", &["pfc", "power factor"]),
];

/// Whole-word match; a keyword ending in a digit (`stm32`, `esp32`) may continue with a part
/// suffix (`stm32f4`), as in DESK's `_kw_match`.
pub fn kw_match(text: &str, kw: &str) -> bool {
    let kw = kw.to_lowercase();
    if kw.is_empty() {
        return false;
    }
    let open_tail = kw.chars().last().is_some_and(|c| c.is_ascii_digit());
    let mut from = 0;
    while let Some(i) = text[from..].find(&kw) {
        let start = from + i;
        let end = start + kw.len();
        let before_ok = text[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after_ok = open_tail
            || text[end..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        from = start + kw.chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn any(text: &str, kws: &[&str]) -> bool {
    kws.iter().any(|k| kw_match(text, k))
}

/// All numbers immediately followed by `unit` (case-insensitive, whole token), e.g. `12 V`,
/// `3.3V`; a range `4.5-5.5 V` or `3 to 5 V` yields both ends. Values are scaled by `scale`.
fn values(text: &str, unit: &str, scale: f64) -> Vec<f64> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if !(b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_alphanumeric())) {
            i += 1;
            continue;
        }
        let s = i;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
        let Ok(n) = text[s..i].trim_end_matches('.').parse::<f64>() else {
            continue;
        };
        let mut j = i;
        while j < b.len() && b[j] == b' ' {
            j += 1;
        }
        if text[j..].starts_with(unit) {
            let end = j + unit.len();
            if text[end..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric())
            {
                out.push(n * scale);
                // A range "a-b unit" / "a to b unit": the lower end precedes this number.
                let head = text[..s].trim_end();
                let head = head
                    .strip_suffix('-')
                    .or_else(|| head.strip_suffix('–'))
                    .or_else(|| head.strip_suffix("to"))
                    .map(str::trim_end);
                if let Some(h) = head {
                    let st = h
                        .rfind(|c: char| !(c.is_ascii_digit() || c == '.'))
                        .map_or(0, |p| p + 1);
                    if let Ok(lo) = h[st..].parse::<f64>() {
                        out.push(lo * scale);
                    }
                }
            }
        }
    }
    out
}

fn max(v: &[f64]) -> Option<f64> {
    v.iter()
        .copied()
        .fold(None, |m, x| Some(m.map_or(x, |m: f64| m.max(x))))
}

/// Collect every text signal of a design with its origin.
pub fn sources(p: &Project) -> Vec<Source> {
    let mut out = vec![Source {
        origin: "project name".into(),
        text: p.name.to_lowercase(),
    }];
    for r in &p.requirements {
        out.push(Source {
            origin: format!("requirement {}", r.id),
            text: r.text.to_lowercase(),
        });
    }
    for s in p.schematic.symbols.values() {
        let mut t = format!("{} {} {}", s.component_id, s.value, s.refdes);
        if let Some(def) = p.library.get(&s.component_id) {
            t.push_str(&format!(" {} {}", def.name, def.category));
            for x in [&def.mpn, &def.manufacturer].into_iter().flatten() {
                t.push(' ');
                t.push_str(x);
            }
        }
        out.push(Source {
            origin: format!("component {}", s.refdes),
            text: t.to_lowercase(),
        });
    }
    for l in p.schematic.labels.values() {
        out.push(Source {
            origin: format!("net label {}", l.name),
            text: l.name.to_lowercase().replace('_', " "),
        });
    }
    out
}

fn names(table: &[(&str, &[&str])], text: &str) -> Vec<String> {
    table
        .iter()
        .filter(|(_, k)| any(text, k))
        .map(|(n, _)| (*n).to_string())
        .collect()
}

/// Extract normalised features from the joined text of all sources.
pub fn extract(text: &str) -> Features {
    let t = text.to_lowercase();
    let mut f = Features::new();

    let kv: Vec<f64> = values(&t, "kv", 1000.0);
    let iso_ctx = ["isolat", "barrier", "hipot", "withstand", "dielectric"]
        .iter()
        .any(|k| t.contains(k));
    if iso_ctx {
        if let Some(v) = max(&kv
            .iter()
            .copied()
            .filter(|v| *v >= 500.0)
            .collect::<Vec<_>>())
        {
            f.insert("isolation_voltage_v".into(), json!(v));
        }
    }
    let mut volts = values(&t, "v", 1.0);
    volts.extend(values(&t, "vac", 1.0));
    volts.extend(values(&t, "vdc", 1.0));
    volts.extend(kv.iter().filter(|v| !(iso_ctx && **v >= 500.0)));
    volts.retain(|v| *v > 0.0 && *v < 100_000.0);
    if let Some(m) = max(&volts) {
        let mut u = volts.clone();
        u.sort_by(f64::total_cmp);
        u.dedup();
        f.insert("voltages_v".into(), json!(u));
        f.insert("input_voltage_v".into(), json!(m));
    }
    let mut amps = values(&t, "a", 1.0);
    amps.extend(values(&t, "ma", 1e-3));
    amps.extend(values(&t, "ua", 1e-6));
    amps.retain(|a| *a > 0.0 && *a < 10_000.0);
    if let Some(m) = max(&amps) {
        f.insert("current_a".into(), json!(m));
    }
    let mut watts = values(&t, "w", 1.0);
    watts.extend(values(&t, "kw", 1e3));
    watts.extend(values(&t, "mw", 1e-3));
    if let Some(m) = max(&watts) {
        f.insert("power_w".into(), json!(m));
    }
    let mut rf = values(&t, "mhz", 1e6);
    rf.extend(values(&t, "ghz", 1e9));
    if let Some(m) = max(&rf) {
        f.insert("rf_frequency_hz".into(), json!(m));
    }
    let sw: Vec<f64> = values(&t, "khz", 1e3)
        .into_iter()
        .chain(values(&t, "hz", 1.0))
        .filter(|x| (1e4..5e6).contains(x))
        .collect();
    if let Some(m) = max(&sw) {
        f.insert("switching_frequency_hz".into(), json!(m));
    }

    f.insert("protocols".into(), json!(names(PROTOCOLS, &t)));
    let mut power = names(POWER_SOURCE, &t);
    let mains_v = f
        .get("input_voltage_v")
        .and_then(Value::as_f64)
        .is_some_and(|v| v >= 85.0)
        && kw_match(&t, "ac");
    let mains = power.iter().any(|s| s == "mains") || mains_v;
    if mains && !power.iter().any(|s| s == "mains") {
        power.push("mains".into());
        power.sort();
    }
    f.insert("power_source".into(), json!(power));
    let motors = names(MOTOR, &t);
    if !motors.is_empty() {
        f.insert("motor".into(), json!(motors));
    }
    let mut conv = names(CONVERTER, &t);
    if conv.iter().any(|c| c == "buck_boost") {
        conv.retain(|c| c != "buck" && c != "boost");
    }
    if !conv.is_empty() {
        f.insert("converter".into(), json!(conv));
    }
    if mains {
        f.insert("mains".into(), json!(true));
    }
    if [
        "non-isolated",
        "nonisolated",
        "transformerless",
        "capacitive dropper",
    ]
    .iter()
    .any(|k| t.contains(k))
    {
        f.insert("isolation".into(), json!(false));
    } else if ["isolat", "galvanic", "reinforced", "selv"]
        .iter()
        .any(|k| t.contains(k))
    {
        f.insert("isolation".into(), json!(true));
    }
    if let Some(n) = cells(&t) {
        f.insert("cells".into(), json!(n));
    }
    if ["low-power", "low power", "sleep", "battery life"]
        .iter()
        .any(|k| t.contains(k))
    {
        f.insert("battery_life_target".into(), json!(true));
    }
    f
}

/// `4s`, `3-cell`, `2 cell`, or a single-cell phrase.
fn cells(t: &str) -> Option<u32> {
    let w: Vec<&str> = t
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .collect();
    for (i, tok) in w.iter().enumerate() {
        if let Some(n) = tok.strip_suffix('s').and_then(|d| d.parse::<u32>().ok()) {
            if (1..=99).contains(&n) && (tok.len() <= 3) {
                return Some(n);
            }
        }
        if let Some(n) = tok
            .strip_suffix("-cell")
            .and_then(|d| d.parse::<u32>().ok())
        {
            return Some(n);
        }
        if w.get(i + 1) == Some(&"cell") || w.get(i + 1) == Some(&"cells") {
            if let Ok(n) = tok.parse::<u32>() {
                if (1..=99).contains(&n) {
                    return Some(n);
                }
            }
        }
    }
    (kw_match(t, "single-cell") || kw_match(t, "single cell") || kw_match(t, "one cell"))
        .then_some(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_matching_is_whole_word_with_part_suffix() {
        assert!(kw_match("stm32f407 board", "stm32"));
        assert!(kw_match("an esp32-s3 module", "esp32"));
        assert!(!kw_match("scan bus", "can bus x"));
        assert!(!kw_match("obscure", "scu"));
        assert!(kw_match("usb-c port", "usb"));
    }

    #[test]
    fn values_parse_units_and_ranges() {
        assert_eq!(max(&values("input 12 v and 3.3v", "v", 1.0)), Some(12.0));
        let r = values("supply 4.5-5.5 v", "v", 1.0);
        assert!(r.contains(&4.5) && r.contains(&5.5));
        assert_eq!(values("500 ma", "ma", 1e-3), vec![0.5]);
        assert!(values("5 amps", "a", 1.0).is_empty());
    }

    #[test]
    fn extracts_structured_features() {
        let f = extract("12-24 v input buck converter, 3 a, usb-c, 4s li-ion battery");
        assert_eq!(f["converter"], json!(["buck"]));
        assert_eq!(f["input_voltage_v"], json!(24.0));
        assert_eq!(f["cells"], json!(4));
        assert!(f["protocols"].as_array().unwrap().contains(&json!("usb")));
    }
}
