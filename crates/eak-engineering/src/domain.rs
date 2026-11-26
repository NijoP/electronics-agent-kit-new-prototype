//! Domain classification from the structured requirement record (M4.3, ADR-0060).
//!
//! [`classify`] is a pure, deterministic function of a [`RequirementSet`]: for every domain class
//! of [`DOMAIN_CLASSES`] (archetypes and overlays, started from the DESK taxonomy) it lists the
//! requirement values that speak for it and against it, each with the rule it meets, how strong
//! that rule is and how firm the value is. Nothing is guessed:
//! * a class with no evidence is not indicated, and the deciding inputs still UNKNOWN are named
//!   (it is not ruled out either);
//! * evidence for and against a class is a **conflict** for a person to resolve, never settled
//!   by a score;
//! * a class **applies** only with strong evidence (a structured value, or text in the field whose
//!   purpose is that property) from a firm value (CONFIRMED, or DERIVED from firm values);
//! * anything weaker (keywords in a description, a value still NEEDS_CONFIRMATION) is
//!   **NEEDS_CONFIRMATION**.
//!
//! [`plan`] turns a classification into the `domain.*` entries the engine's `ClassifyDomain`
//! records: DERIVED (with the evidence paths and this evaluator as a `calculation` source),
//! NEEDS_CONFIRMATION, or NEEDS_CONFIRMATION with the evidence against kept as a conflicting
//! value. A person's decision (CONFIRMED, or a value someone else recorded) is never replaced:
//! when the classifier disagrees, its view is kept beside it as a conflicting value.
//!
//! Thresholds are DESK's overlay thresholds (60 V, 3 A, 100 uA, 70 degC) or EAK's own (100 Mbps,
//! one year); they decide which rules to consider, they are not safety classifications.

use eak_core::requirements::{
    evaluate, item_field_def, DomainClassDef, DomainKind, Level, ReqEntry, ReqItemKind,
    ReqSource, ReqStatus, ReqUnit, ReqValue, RequirementSet, SourceKind, DOMAIN_CLASSES, MAX_TEXT,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// The evaluator's name; its entries carry it in their `calculation` source reference.
pub const EVALUATOR: &str = "eak-engineering domain classifier";
/// Bump when a rule changes, so stored classifications name the rules that made them.
pub const VERSION: u32 = 1;

/// The source reference of every entry this classifier records.
pub fn source_reference() -> String {
    format!("{EVALUATOR} v{VERSION} (ADR-0060)")
}

/// Whether an entry was recorded by this classifier (and is still its own: a person who
/// confirmed it made it theirs).
pub fn by_classifier(e: &ReqEntry) -> bool {
    matches!(e.status, ReqStatus::Derived | ReqStatus::NeedsConfirmation)
        && e.source.as_ref().is_some_and(ours)
}

fn ours(s: &ReqSource) -> bool {
    s.kind == SourceKind::Calculation && s.reference.starts_with(EVALUATOR)
}

// ------------------------------------------------------------------ rules

#[derive(Debug, Clone, Copy)]
enum At {
    /// A project field.
    Field(&'static str),
    /// A field of every rail or interface.
    Item(ReqItemKind, &'static str),
}

#[derive(Debug, Clone, Copy)]
enum Test {
    /// Text holding one of the words (whole words, any case), unless it holds one of the second.
    Words(&'static [&'static str], &'static [&'static str]),
    /// A quantity whose largest stated limit is at least this (in V, A, W, bps, s or degC).
    AtLeast(f64),
    /// A quantity whose smallest stated limit is at most this.
    AtMost(f64),
    /// A quantity whose stated maximum is below this.
    MaxBelow(f64),
    /// A flag with this value.
    Flag(bool),
    /// Any recorded value.
    Recorded,
    /// Recorded NOT_APPLICABLE.
    NotApplicable,
}

#[derive(Debug, Clone, Copy)]
struct Signal {
    at: At,
    test: Test,
    strong: bool,
    against: bool,
    rule: &'static str,
}

const fn strong(at: At, test: Test, rule: &'static str) -> Signal {
    Signal {
        at,
        test,
        strong: true,
        against: false,
        rule,
    }
}

const fn weak(at: At, test: Test, rule: &'static str) -> Signal {
    Signal {
        at,
        test,
        strong: false,
        against: false,
        rule,
    }
}

const fn against(at: At, test: Test, rule: &'static str) -> Signal {
    Signal {
        at,
        test,
        strong: true,
        against: true,
        rule,
    }
}

struct Rules {
    id: &'static str,
    signals: &'static [Signal],
    /// Inputs that would decide the class: named while still UNKNOWN and no evidence exists.
    deciding: &'static [&'static str],
}

use At::{Field as F, Item as I};
use ReqItemKind::{Interface as IFC, Rail as RAIL};
use Test::*;

const NONE: &[&str] = &[];
const DESC: At = F("function.description");

#[rustfmt::skip]
const MCU: &[&str] = &[
    "mcu", "microcontroller", "micro-controller", "firmware", "soc", "processor", "esp32",
    "stm32", "rp2040", "nrf52", "nrf53", "atmega", "attiny", "pic", "samd21", "cortex-m",
    "risc-v", "arduino",
];
#[rustfmt::skip]
const CONVERTER: &[&str] = &[
    "buck", "boost", "buck-boost", "sepic", "flyback", "forward converter", "llc", "dc-dc",
    "dc/dc", "dcdc", "ac-dc", "ac/dc", "converter", "power supply", "psu", "smps",
    "switch-mode", "switching regulator", "pfc", "inverter",
];
#[rustfmt::skip]
const BATTERY: &[&str] = &[
    "battery", "batteries", "li-ion", "lithium", "lipo", "li-po", "lifepo4", "nimh", "nicd",
    "lead-acid", "coin cell", "cr2032", "18650", "21700", "cell", "cells", "alkaline",
];
#[rustfmt::skip]
const BATTERY_TEXT: &[&str] = &[
    "battery", "battery-powered", "li-ion", "lipo", "charger", "charging", "bms", "fuel gauge",
];
#[rustfmt::skip]
const MOTOR: &[&str] = &[
    "motor", "motors", "bldc", "stepper", "servo", "h-bridge", "half-bridge", "esc", "foc",
    "actuator", "solenoid",
];
#[rustfmt::skip]
const MAINS: &[&str] = &[
    "mains", "vac", "v ac", "ac mains", "ac input", "ac line", "line voltage", "grid",
    "offline", "off-line", "230 v", "230v", "240 v", "240v", "100-240", "85-265", "wall socket",
];
#[rustfmt::skip]
const RADIO: &[&str] = &[
    "ble", "bluetooth", "wi-fi", "wifi", "wlan", "802.11", "802.15.4", "lora", "lorawan",
    "zigbee", "thread", "z-wave", "lte", "lte-m", "nb-iot", "gsm", "gprs", "4g", "5g",
    "cellular", "gnss", "gps", "nfc", "rfid", "uwb", "sub-ghz", "sigfox", "radio", "rf",
    "antenna", "wireless",
];
#[rustfmt::skip]
const ANALOG_IFC: &[&str] = &[
    "analog", "analogue", "4-20 ma", "4-20ma", "0-10 v", "0-10v", "thermocouple", "rtd",
    "pt100", "pt1000", "strain gauge", "load cell", "bridge", "adc", "sensor",
];
#[rustfmt::skip]
const ANALOG_TEXT: &[&str] = &[
    "sensor", "sensors", "sensing", "measure", "measures", "measurement", "adc", "analog",
    "analogue", "thermocouple", "rtd", "strain", "load cell", "amplifier", "op-amp", "opamp",
    "photodiode", "ecg", "eeg", "emg", "transducer",
];
#[rustfmt::skip]
const PRECISION: &[&str] = &[
    "ppm", "uv", "microvolt", "lsb", "enob", "adc", "16-bit", "18-bit", "24-bit",
];
#[rustfmt::skip]
const ISOLATION: &[&str] = &[
    "functional", "basic", "supplementary", "reinforced", "double", "isolated", "isolation",
    "galvanic", "kv", "kvrms",
];
#[rustfmt::skip]
const NO_ISOLATION: &[&str] = &[
    "none", "no isolation", "not required", "not needed", "non-isolated", "n/a",
];
#[rustfmt::skip]
const ISOLATION_TEXT: &[&str] = &[
    "isolated", "isolation", "galvanic", "optocoupler", "opto-isolated", "digital isolator",
];
const NOT_ISOLATED: &[&str] = &["non-isolated", "not isolated"];
const ISOLATED: &[&str] = &["isolated"];
#[rustfmt::skip]
const HIGH_SPEED: &[&str] = &[
    "usb 3", "usb3", "superspeed", "hdmi", "displayport", "pcie", "pci express", "sata",
    "mipi", "csi-2", "dsi", "lvds", "ddr", "ddr3", "ddr4", "lpddr4", "gigabit", "1000base-t",
    "10gbase", "sgmii", "rgmii", "serdes", "thunderbolt",
];
#[rustfmt::skip]
const MAYBE_HIGH_SPEED: &[&str] = &[
    "ethernet", "100base-tx", "usb 2.0", "usb2", "fpga", "high-speed",
];
#[rustfmt::skip]
const USB: &[&str] = &[
    "usb", "usb-c", "usb c", "type-c", "type c", "usb-pd", "usb pd", "micro-usb", "micro usb",
    "mini-usb", "micro-b", "vbus", "usb2", "usb3",
];
#[rustfmt::skip]
const CAN: &[&str] = &[
    "can", "can fd", "can-fd", "canopen", "j1939", "devicenet", "nmea 2000", "can bus",
    "canbus",
];
const CAN_TEXT: &[&str] = &["can bus", "canbus", "can fd", "canopen", "j1939"];
#[rustfmt::skip]
const INDUSTRIAL: &[&str] = &[
    "rs-485", "rs485", "rs-422", "rs422", "rs-232", "rs232", "modbus", "profibus", "profinet",
    "ethercat", "ethernet/ip", "io-link", "hart", "4-20 ma", "4-20ma", "m-bus", "mbus", "knx",
    "dali", "bacnet", "plc",
];
#[rustfmt::skip]
const LOW_POWER_TEXT: &[&str] = &[
    "low power", "low-power", "ultra-low", "deep sleep", "energy harvesting", "coin cell",
];
const SLEEP_MODES: &[&str] = &["sleep", "deep sleep", "standby", "hibernate"];
#[rustfmt::skip]
const WET: &[&str] = &[
    "ip54", "ip55", "ip65", "ip66", "ip67", "ip68", "ip69", "ip69k", "outdoor", "outdoors",
    "washdown", "submersible", "waterproof", "marine", "rain",
];
#[rustfmt::skip]
const AUTOMOTIVE: &[&str] = &[
    "automotive", "vehicle", "vehicles", "car", "cars", "truck", "trucks", "motorcycle",
    "aec-q100", "iso 26262", "obd", "obd-ii",
];
#[rustfmt::skip]
const AUTOMOTIVE_STANDARDS: &[&str] = &[
    "iso 7637", "iso 16750", "cispr 25", "aec-q100", "aec-q200", "iso 26262", "lv 124", "lv124",
    "e-mark", "ece r10",
];
#[rustfmt::skip]
const VEHICLE_SUPPLY: &[&str] = &[
    "vehicle", "car battery", "automotive", "obd", "obd-ii", "truck", "alternator",
];

/// One year in seconds (365.25 days).
const YEAR_S: f64 = 31_557_600.0;

#[rustfmt::skip]
const RULES: &[Rules] = &[
    Rules { id: "embedded_mcu", deciding: &["firmware.memory_min", "firmware.update_method", "function.description"], signals: &[
        strong(F("firmware.memory_min"), Recorded, "a program-memory requirement means a controller runs firmware"),
        weak(F("firmware.update_method"), Recorded, "a firmware update method is recorded"),
        weak(F("manufacturing.programming_method"), Recorded, "a firmware programming method is recorded"),
        weak(DESC, Words(MCU, NONE), "the function names a controller or firmware"),
        against(F("firmware.memory_min"), NotApplicable, "program memory is NOT_APPLICABLE: no controller"),
        against(F("firmware.update_method"), NotApplicable, "firmware updates are NOT_APPLICABLE: no firmware"),
        against(F("manufacturing.programming_method"), NotApplicable, "programming is NOT_APPLICABLE: no firmware"),
    ]},
    Rules { id: "dcdc_power", deciding: &["function.description"], signals: &[
        weak(DESC, Words(CONVERTER, NONE), "the function names a power converter"),
        weak(I(RAIL, "loads"), Words(CONVERTER, NONE), "a rail's loads name a converter"),
    ]},
    Rules { id: "battery_bms", deciding: &["supply.sources", "power_budget.battery_life"], signals: &[
        strong(F("supply.sources"), Words(BATTERY, NONE), "a power source is a battery"),
        strong(F("power_budget.battery_life"), Recorded, "a battery-life target is recorded"),
        weak(F("safety.battery_transport"), Recorded, "battery transport rules are recorded"),
        weak(DESC, Words(BATTERY_TEXT, NONE), "the function names a battery or charging"),
        against(F("power_budget.battery_life"), NotApplicable, "battery life is NOT_APPLICABLE: no battery"),
    ]},
    Rules { id: "motor_control", deciding: &["function.description"], signals: &[
        strong(I(RAIL, "loads"), Words(MOTOR, NONE), "a rail powers a motor or actuator"),
        weak(DESC, Words(MOTOR, NONE), "the function names a motor or actuator"),
    ]},
    Rules { id: "ac_mains_hv", deciding: &["supply.sources", "supply.input_voltage", "safety.max_working_voltage"], signals: &[
        strong(F("supply.sources"), Words(MAINS, NONE), "a power source is AC mains"),
        weak(F("supply.input_voltage"), AtLeast(60.0), "the input reaches 60 V or more (mains, or high-voltage DC)"),
        weak(DESC, Words(MAINS, NONE), "the function names mains"),
        against(F("supply.input_voltage"), MaxBelow(60.0), "the input stays below 60 V"),
        against(F("safety.max_working_voltage"), MaxBelow(60.0), "the highest working voltage is below 60 V"),
    ]},
    Rules { id: "rf_wireless", deciding: &["interfaces.list", "safety.radio_certification"], signals: &[
        strong(I(IFC, "protocol"), Words(RADIO, NONE), "an interface is a radio"),
        weak(F("safety.radio_certification"), Recorded, "a radio certification is recorded"),
        weak(DESC, Words(RADIO, NONE), "the function names a radio"),
        against(F("safety.radio_certification"), NotApplicable, "radio certification is NOT_APPLICABLE: the product does not transmit"),
    ]},
    Rules { id: "analog_sensors", deciding: &["function.description", "function.performance"], signals: &[
        strong(I(IFC, "protocol"), Words(ANALOG_IFC, NONE), "an interface carries an analog or sensor signal"),
        weak(DESC, Words(ANALOG_TEXT, NONE), "the function names sensing or analog measurement"),
        weak(F("function.performance"), Words(PRECISION, NONE), "the performance targets name analog precision"),
    ]},
    Rules { id: "high_voltage", deciding: &["safety.max_working_voltage", "supply.input_voltage"], signals: &[
        strong(F("safety.max_working_voltage"), AtLeast(60.0), "the highest working voltage is 60 V or more"),
        strong(F("supply.input_voltage"), AtLeast(60.0), "the input reaches 60 V or more"),
        strong(I(RAIL, "voltage"), AtLeast(60.0), "a rail reaches 60 V or more"),
        against(F("safety.max_working_voltage"), MaxBelow(60.0), "the highest working voltage is below 60 V"),
    ]},
    Rules { id: "galvanic_isolation", deciding: &["safety.isolation"], signals: &[
        strong(F("safety.isolation"), Words(ISOLATION, NO_ISOLATION), "the isolation requirement names an isolation class"),
        weak(DESC, Words(ISOLATION_TEXT, NOT_ISOLATED), "the function names isolation"),
        weak(I(IFC, "protocol"), Words(ISOLATED, NOT_ISOLATED), "an interface is isolated"),
        against(F("safety.isolation"), Words(NO_ISOLATION, NONE), "the isolation requirement says none is needed"),
    ]},
    Rules { id: "high_current", deciding: &["power_budget.peak_current", "supply.rails"], signals: &[
        strong(F("power_budget.peak_current"), AtLeast(3.0), "the peak input current is 3 A or more"),
        strong(I(RAIL, "current_peak"), AtLeast(3.0), "a rail's peak current is 3 A or more"),
        strong(I(RAIL, "current_continuous"), AtLeast(3.0), "a rail's continuous current is 3 A or more"),
    ]},
    Rules { id: "high_speed_digital", deciding: &["interfaces.list"], signals: &[
        strong(I(IFC, "data_rate"), AtLeast(100e6), "an interface runs at 100 Mbps or more"),
        strong(I(IFC, "protocol"), Words(HIGH_SPEED, NONE), "an interface is a high-speed protocol"),
        weak(I(IFC, "protocol"), Words(MAYBE_HIGH_SPEED, NONE), "an interface may run at 100 Mbps or more"),
        weak(DESC, Words(HIGH_SPEED, NONE), "the function names a high-speed interface"),
        weak(DESC, Words(MAYBE_HIGH_SPEED, NONE), "the function names an interface that may be high speed"),
    ]},
    Rules { id: "usb_interface", deciding: &["interfaces.list", "supply.sources"], signals: &[
        strong(I(IFC, "protocol"), Words(USB, NONE), "an interface is USB"),
        strong(I(IFC, "connector"), Words(USB, NONE), "a connector is a USB connector"),
        strong(F("supply.sources"), Words(USB, NONE), "a power source is USB"),
        weak(DESC, Words(USB, NONE), "the function names USB"),
    ]},
    Rules { id: "can_interface", deciding: &["interfaces.list"], signals: &[
        strong(I(IFC, "protocol"), Words(CAN, NONE), "an interface is CAN"),
        weak(DESC, Words(CAN_TEXT, NONE), "the function names a CAN bus"),
    ]},
    Rules { id: "industrial_interfaces", deciding: &["interfaces.list"], signals: &[
        strong(I(IFC, "protocol"), Words(INDUSTRIAL, NONE), "an interface is an industrial interface"),
        weak(DESC, Words(INDUSTRIAL, NONE), "the function names an industrial interface"),
    ]},
    Rules { id: "low_power", deciding: &["power_budget.sleep_current", "power_budget.battery_life"], signals: &[
        strong(F("power_budget.sleep_current"), AtMost(100e-6), "the sleep current is 100 uA or less"),
        strong(F("power_budget.battery_life"), AtLeast(YEAR_S), "the battery-life target is a year or more"),
        weak(DESC, Words(LOW_POWER_TEXT, NONE), "the function names low-power operation"),
        weak(F("function.operating_modes"), Words(SLEEP_MODES, NONE), "an operating mode is a sleep mode"),
    ]},
    Rules { id: "harsh_environment", deciding: &["environment.operating_temp", "environment.condensation", "environment.ingress"], signals: &[
        strong(F("environment.operating_temp"), AtLeast(70.0), "the ambient reaches 70 degC or more"),
        strong(F("environment.condensation"), Flag(true), "condensation can form"),
        strong(F("environment.ingress"), Words(WET, NONE), "the ingress requirement is wet or outdoor"),
        weak(F("environment.vibration"), Recorded, "a vibration or shock requirement is recorded"),
    ]},
    Rules { id: "automotive_environment", deciding: &["safety.sector", "supply.sources"], signals: &[
        strong(F("safety.sector"), Words(AUTOMOTIVE, NONE), "the sector is automotive"),
        strong(F("safety.standards"), Words(AUTOMOTIVE_STANDARDS, NONE), "an automotive standard applies"),
        strong(F("supply.sources"), Words(VEHICLE_SUPPLY, NONE), "the board is powered from a vehicle"),
        weak(DESC, Words(AUTOMOTIVE, NONE), "the function names a vehicle"),
    ]},
];

// ------------------------------------------------------------------ result

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    /// A structured value, or text in the field whose purpose is that property.
    Strong,
    /// Keywords in a description, or a value whose presence only hints at the class.
    Weak,
}

/// One requirement value that speaks for or against a class.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evidence {
    pub path: String,
    pub label: String,
    /// What the record holds there.
    pub observed: String,
    pub status: ReqStatus,
    /// How firm the value is (from `requirements::evaluate`; a value in conflict is missing).
    pub level: Level,
    pub strength: Strength,
    /// The rule it meets.
    pub rule: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    /// Strong evidence from a firm value and none against: recorded DERIVED.
    Applies,
    /// Only weak or not-yet-firm evidence: recorded NEEDS_CONFIRMATION for a person.
    NeedsConfirmation,
    /// Evidence for and against: recorded as a conflict for a person to resolve.
    Conflict,
}

/// A class with evidence for it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClassResult {
    pub id: &'static str,
    pub key: &'static str,
    pub kind: DomainKind,
    pub label: &'static str,
    /// The DESK domain or overlay it starts from.
    pub desk: &'static str,
    pub verdict: Verdict,
    pub reason: &'static str,
    pub supports: Vec<Evidence>,
    pub contradicts: Vec<Evidence>,
}

/// A class with no evidence for it: not indicated, and not ruled out while `unknown_inputs`
/// are still UNKNOWN.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotIndicated {
    pub id: &'static str,
    pub key: &'static str,
    pub kind: DomainKind,
    pub label: &'static str,
    pub unknown_inputs: Vec<&'static str>,
    pub contradicts: Vec<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Classification {
    /// This evaluator and its version (the `calculation` source of every entry it records).
    pub evaluator: String,
    pub classes: Vec<ClassResult>,
    pub not_indicated: Vec<NotIndicated>,
    pub not_implemented: Vec<&'static str>,
    pub limitations: Vec<&'static str>,
}

pub const NOT_IMPLEMENTED: &[&str] = &[
    "a primary and secondary archetype ranking: every class with evidence is listed, none is ranked above another",
    "DESK's 100 sub-archetypes and the archetypes audio_video, led_display, industrial_control, test_debug and interfaces_comms; the DESK text classifier (eak-intelligence classify_domain) still scores those from free text",
    "the overlays switching_power, precision_analog, thermal_management and emi_emc, and separate rf_communication, battery_management and motor_drive overlays (folded into the archetypes rf_wireless, battery_bms and motor_control)",
    "rules per class (the rule registry and composition of M4.4); a classification only says which rule families must be considered",
    "domain-required fields: a classification does not yet refuse NOT_APPLICABLE for fields its rules need",
    "AI-assisted interpretation: an agent may propose a class as NEEDS_CONFIRMATION, and the classifier keeps a disagreeing view as a conflict",
];

pub const LIMITATIONS: &[&str] = &[
    "keyword evidence matches whole words in English text only; a class named in other words is not found, and is shown as not indicated with its deciding inputs",
    "thresholds (60 V, 3 A, 100 Mbps, 100 uA, one year, 70 degC) select rule families; they are not safety classifications",
    "power conversion has only indirect evidence in the requirement record, so the classifier never records it DERIVED; a person confirms it",
    "a classification is only as right as the requirement values it reads",
];

/// Classify the record. Pure and deterministic.
pub fn classify(set: &RequirementSet) -> Classification {
    let report = evaluate(set);
    let levels: BTreeMap<&str, Level> = report
        .fields
        .iter()
        .map(|f| (f.path.as_str(), f.level))
        .collect();
    let mut classes = vec![];
    let mut not_indicated = vec![];
    for rules in RULES {
        let Some(def) = DOMAIN_CLASSES.iter().find(|c| c.id == rules.id) else {
            continue;
        };
        let mut supports = vec![];
        let mut contradicts = vec![];
        for s in rules.signals {
            for ev in matches(set, &levels, s) {
                if s.against {
                    contradicts.push(ev);
                } else {
                    supports.push(ev);
                }
            }
        }
        if supports.is_empty() {
            let unknown_inputs = rules
                .deciding
                .iter()
                .copied()
                .filter(|p| !levels.get(p).is_some_and(|l| *l != Level::Missing))
                .collect();
            not_indicated.push(NotIndicated {
                id: def.id,
                key: def.field.key,
                kind: def.kind,
                label: def.field.label,
                unknown_inputs,
                contradicts,
            });
            continue;
        }
        let firm_strong = supports
            .iter()
            .any(|e| e.strength == Strength::Strong && e.level == Level::Firm);
        let any_strong = supports.iter().any(|e| e.strength == Strength::Strong);
        let (verdict, reason) = if !contradicts.is_empty() {
            (
                Verdict::Conflict,
                "evidence for and against it: a person resolves the conflict",
            )
        } else if firm_strong {
            (
                Verdict::Applies,
                "strong evidence from a firm value, and none against it",
            )
        } else if any_strong {
            (
                Verdict::NeedsConfirmation,
                "the strong evidence is not firm yet (NEEDS_CONFIRMATION, in conflict, or derived from values that are not firm)",
            )
        } else {
            (
                Verdict::NeedsConfirmation,
                "only indirect evidence (keywords in descriptions, or a value that only hints at it)",
            )
        };
        classes.push(ClassResult {
            id: def.id,
            key: def.field.key,
            kind: def.kind,
            label: def.field.label,
            desk: def.desk,
            verdict,
            reason,
            supports,
            contradicts,
        });
    }
    Classification {
        evaluator: source_reference(),
        classes,
        not_indicated,
        not_implemented: NOT_IMPLEMENTED.to_vec(),
        limitations: LIMITATIONS.to_vec(),
    }
}

/// Evidence one signal finds in the record.
fn matches(set: &RequirementSet, levels: &BTreeMap<&str, Level>, s: &Signal) -> Vec<Evidence> {
    let mut places: Vec<(String, String, &ReqEntry)> = vec![];
    match s.at {
        At::Field(k) => {
            if let (Some(e), Ok(d)) = (set.fields.get(k), set.def_of(k)) {
                places.push((k.to_string(), d.label.to_string(), e));
            }
        }
        At::Item(kind, f) => {
            let Some(d) = item_field_def(kind, f) else {
                return vec![];
            };
            for item in set.items_of(kind) {
                if let Some(e) = item.fields.get(f) {
                    let label = format!("{} {}: {}", kind.label(), item.name, d.label);
                    places.push((format!("{}/{f}", item.id), label, e));
                }
            }
        }
    }
    let strength = if s.strong {
        Strength::Strong
    } else {
        Strength::Weak
    };
    let mut out = vec![];
    for (path, label, e) in places {
        if !meets(s.test, e) {
            continue;
        }
        let level = levels.get(path.as_str()).copied();
        out.push(Evidence {
            level: level.unwrap_or(Level::Missing),
            observed: observed(e),
            status: e.status,
            strength,
            rule: s.rule,
            path,
            label,
        });
    }
    out
}

fn meets(test: Test, e: &ReqEntry) -> bool {
    match (test, &e.value) {
        (NotApplicable, _) => e.status == ReqStatus::NotApplicable,
        (Recorded, v) => v.is_some(),
        (Flag(want), Some(ReqValue::Flag { value })) => *value == want,
        (Words(any, unless), Some(ReqValue::Text { text })) => {
            !unless.iter().any(|w| has_word(text, w)) && any.iter().any(|w| has_word(text, w))
        }
        (AtLeast(t), Some(v)) => limits(v).into_iter().any(|x| x >= t),
        (AtMost(t), Some(v)) => limits(v).into_iter().any(|x| x <= t),
        (MaxBelow(t), Some(v)) => stated_max(v).is_some_and(|x| x < t),
        _ => false,
    }
}

/// Every stated limit of a quantity, in base units.
fn limits(v: &ReqValue) -> Vec<f64> {
    match v {
        ReqValue::Number { value, unit, .. } => vec![value * scale(*unit)],
        ReqValue::Range {
            min,
            nom,
            max,
            unit,
        } => [*min, *nom, *max]
            .into_iter()
            .flatten()
            .map(|x| x * scale(*unit))
            .collect(),
        _ => vec![],
    }
}

/// The stated maximum (a range's max, or a single figure), in base units.
fn stated_max(v: &ReqValue) -> Option<f64> {
    match v {
        ReqValue::Number { value, unit, .. } => Some(value * scale(*unit)),
        ReqValue::Range { max, unit, .. } => max.map(|x| x * scale(*unit)),
        _ => None,
    }
}

/// Factor to the base unit (V, A, W, m, Hz, bps, s; others are already base or not compared).
fn scale(u: ReqUnit) -> f64 {
    use ReqUnit::*;
    match u {
        Millivolt | Milliampere | Milliwatt | Millimetre | Millisecond => 1e-3,
        Microampere | Micrometre => 1e-6,
        Kilovolt | Kilohertz | KilobitPerSecond => 1e3,
        Megahertz | MegabitPerSecond => 1e6,
        Gigahertz => 1e9,
        Hour => 3_600.0,
        Day => 86_400.0,
        Year => YEAR_S,
        _ => 1.0,
    }
}

/// Whether `text` holds `word` as a whole word (any case; the word may hold spaces or dashes).
fn has_word(text: &str, word: &str) -> bool {
    let t = text.to_lowercase();
    let w = word.to_lowercase();
    let edge = |c: Option<char>| !matches!(c, Some(c) if c.is_alphanumeric());
    let mut from = 0;
    while let Some(i) = t[from..].find(&w) {
        let start = from + i;
        let end = start + w.len();
        if edge(t[..start].chars().next_back()) && edge(t[end..].chars().next()) {
            return true;
        }
        from = start + t[start..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn observed(e: &ReqEntry) -> String {
    match &e.value {
        None => e.status.as_str().to_string(),
        Some(ReqValue::Text { text }) => {
            let short: String = text.chars().take(80).collect();
            if short.len() < text.len() {
                format!("\"{short}...\"")
            } else {
                format!("\"{short}\"")
            }
        }
        Some(ReqValue::Flag { value: true }) => "yes".to_string(),
        Some(ReqValue::Flag { value: false }) => "no".to_string(),
        Some(ReqValue::Number { value, unit, .. }) => format!("{value} {}", unit.symbol()),
        Some(ReqValue::Range {
            min,
            nom,
            max,
            unit,
        }) => {
            let part = |x: &Option<f64>| x.map_or_else(|| "-".to_string(), |v| v.to_string());
            format!(
                "{} / {} / {} {}",
                part(min),
                part(nom),
                part(max),
                unit.symbol()
            )
        }
    }
}

// ------------------------------------------------------------------ recording

fn evidence_text(heading: &str, ev: &[Evidence]) -> String {
    let lines: Vec<String> = ev
        .iter()
        .map(|e| {
            let strength = match e.strength {
                Strength::Strong => "strong",
                Strength::Weak => "weak",
            };
            format!(
                "{} = {} ({}, {strength}): {}",
                e.path,
                e.observed,
                e.status.as_str(),
                e.rule
            )
        })
        .collect();
    format!("{heading}: {}", lines.join("; "))
}

fn bounded(text: String) -> String {
    if text.chars().count() <= MAX_TEXT {
        return text;
    }
    let mut cut: String = text.chars().take(MAX_TEXT - 3).collect();
    cut.push_str("...");
    cut
}

/// The entry `ClassifyDomain` records for one class result.
fn entry_for(r: &ClassResult, def: &DomainClassDef, at_ms: u64, revision: u64) -> ReqEntry {
    let blank = |status: ReqStatus, value: bool, note: String| ReqEntry {
        status,
        value: Some(ReqValue::Flag { value }),
        source: Some(ReqSource {
            kind: SourceKind::Calculation,
            reference: source_reference(),
        }),
        derived_from: vec![],
        owner: None,
        risk_if_wrong: (status == ReqStatus::NeedsConfirmation)
            .then(|| def.risk_if_wrong.to_string()),
        note: Some(bounded(note)),
        updated_at_ms: at_ms,
        updated_revision: revision,
        conflicts: vec![],
    };
    let for_it = evidence_text("Evidence for", &r.supports);
    match r.verdict {
        Verdict::Applies => {
            let inputs: BTreeSet<String> = r.supports.iter().map(|e| e.path.clone()).collect();
            ReqEntry {
                derived_from: inputs.into_iter().collect(),
                ..blank(ReqStatus::Derived, true, for_it)
            }
        }
        Verdict::NeedsConfirmation => blank(
            ReqStatus::NeedsConfirmation,
            true,
            format!("{}. {for_it}", r.reason),
        ),
        Verdict::Conflict => {
            let against_it = evidence_text("Evidence against", &r.contradicts);
            ReqEntry {
                conflicts: vec![blank(ReqStatus::NeedsConfirmation, false, against_it)],
                ..blank(
                    ReqStatus::NeedsConfirmation,
                    true,
                    format!("{}. {for_it}", r.reason),
                )
            }
        }
    }
}

/// The `domain.*` entries `ClassifyDomain` records for a classification: each changed key with
/// its new entry (`None` = remove the classifier's entry). Empty when the stored classification
/// already matches the evidence. Entries recorded by anyone else are never replaced: when the
/// classifier disagrees with one, its view is kept beside it as a conflicting value (and dropped
/// again once it agrees).
pub fn plan(
    set: &RequirementSet,
    c: &Classification,
    at_ms: u64,
    revision: u64,
) -> Vec<(&'static str, Option<ReqEntry>)> {
    let mut out = vec![];
    for def in DOMAIN_CLASSES {
        let key = def.field.key;
        let wanted = c
            .classes
            .iter()
            .find(|r| r.id == def.id)
            .map(|r| entry_for(r, def, at_ms, revision));
        match set.fields.get(key) {
            None => {
                if let Some(w) = wanted {
                    out.push((key, Some(w)));
                }
            }
            Some(old) if by_classifier(old) => match wanted {
                None => out.push((key, None)),
                Some(w) if w.same_content(old) => {}
                Some(w) => out.push((key, Some(w))),
            },
            Some(old) => {
                // Someone else's entry: keep it, with the classifier's view beside it while
                // they disagree (an unchanged view keeps its earlier stamp).
                let earlier = old.conflicts.iter().find(|x| by_classifier(x));
                let mut kept = old.clone();
                kept.conflicts.retain(|x| !by_classifier(x));
                if let Some(mut w) = wanted {
                    let disagrees = old.value.is_some() && w.value != old.value;
                    let on_record = kept.conflicts.iter().any(|x| x.value == w.value);
                    if disagrees && !on_record {
                        w.conflicts.clear();
                        match earlier {
                            Some(e) if e.same_content(&w) => kept.conflicts.push(e.clone()),
                            _ => kept.conflicts.push(w),
                        }
                    }
                }
                if !kept.same_content(old) {
                    out.push((key, Some(kept)));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use eak_core::requirements::{domain_class, field_def, Recorded};

    fn source(kind: SourceKind) -> Option<ReqSource> {
        Some(ReqSource {
            kind,
            reference: "kick-off call 2026-11-20".into(),
        })
    }

    fn entry(status: ReqStatus, value: ReqValue) -> ReqEntry {
        ReqEntry {
            status,
            value: Some(value),
            source: source(SourceKind::Client),
            derived_from: vec![],
            owner: Some("Client PM".into()),
            risk_if_wrong: (status == ReqStatus::NeedsConfirmation)
                .then(|| "the architecture changes".to_string()),
            note: None,
            updated_at_ms: 1,
            updated_revision: 1,
            conflicts: vec![],
        }
    }

    fn text(s: &str) -> ReqValue {
        ReqValue::Text { text: s.into() }
    }

    fn number(value: f64, unit: ReqUnit) -> ReqValue {
        ReqValue::Number {
            value,
            unit,
            tolerance_pct: None,
        }
    }

    fn not_applicable() -> ReqEntry {
        ReqEntry {
            status: ReqStatus::NotApplicable,
            value: None,
            source: None,
            derived_from: vec![],
            owner: None,
            risk_if_wrong: None,
            note: Some("the product does not transmit".into()),
            updated_at_ms: 1,
            updated_revision: 1,
            conflicts: vec![],
        }
    }

    fn class<'a>(c: &'a Classification, id: &str) -> Option<&'a ClassResult> {
        c.classes.iter().find(|r| r.id == id)
    }

    /// Record a plan the way the engine's ClassifyDomain does; returns how many keys changed.
    fn record(set: &mut RequirementSet, revision: u64) -> usize {
        let c = classify(set);
        let p = plan(set, &c, 10, revision);
        let n = p.len();
        for (key, e) in p {
            match e {
                Some(e) => set.set(key, e).unwrap(),
                None => {
                    set.fields.remove(key);
                }
            }
        }
        assert!(set.problems().is_empty(), "{:?}", set.problems());
        n
    }

    #[test]
    fn every_rule_names_a_catalogue_class_and_field() {
        let mut seen = BTreeSet::new();
        for r in RULES {
            assert!(domain_class(r.id).is_some(), "{}", r.id);
            assert!(seen.insert(r.id), "duplicate rules for {}", r.id);
            for s in r.signals {
                let known = match s.at {
                    At::Field(k) => field_def(k).is_some(),
                    At::Item(kind, f) => item_field_def(kind, f).is_some(),
                };
                assert!(known, "{}: {:?}", r.id, s.at);
            }
            for k in r.deciding {
                assert!(field_def(k).is_some(), "{k}");
            }
        }
        assert_eq!(seen.len(), DOMAIN_CLASSES.len(), "every class has rules");
    }

    #[test]
    fn keywords_match_whole_words_only() {
        assert!(has_word("ESP32-C3 with BLE", "ble"));
        assert!(has_word("USB-C power", "usb"));
        assert!(has_word("230 V AC mains", "230 v"));
        assert!(has_word("Isolation: none", "none"));
        assert!(!has_word("a cable to the enclosure", "ble"));
        assert!(!has_word("scan the board", "can"));
    }

    /// Negative test: with nothing recorded, nothing is classified and nothing is ruled out.
    #[test]
    fn an_empty_record_classifies_nothing_and_names_what_would_decide() {
        let c = classify(&RequirementSet::default());
        assert!(c.classes.is_empty());
        assert_eq!(c.not_indicated.len(), DOMAIN_CLASSES.len());
        let hv = c
            .not_indicated
            .iter()
            .find(|n| n.id == "high_voltage")
            .unwrap();
        assert!(hv.unknown_inputs.contains(&"safety.max_working_voltage"));
        assert!(!c.not_implemented.is_empty() && !c.limitations.is_empty());
        assert!(plan(&RequirementSet::default(), &c, 1, 1).is_empty());
    }

    /// Negative test: weak evidence (keywords in the description) or a value still
    /// NEEDS_CONFIRMATION is never enough for DERIVED.
    #[test]
    fn insufficient_evidence_needs_confirmation() {
        let mut s = RequirementSet::default();
        s.set(
            "function.description",
            entry(ReqStatus::Confirmed, text("ESP32 sensor node with Bluetooth")),
        )
        .unwrap();
        let memory = number(512.0, ReqUnit::Kibibyte);
        s.set(
            "firmware.memory_min",
            entry(ReqStatus::NeedsConfirmation, memory.clone()),
        )
        .unwrap();
        let c = classify(&s);
        for id in ["embedded_mcu", "rf_wireless", "analog_sensors"] {
            let r = class(&c, id).expect(id);
            assert_eq!(r.verdict, Verdict::NeedsConfirmation, "{id}");
        }
        let mcu = class(&c, "embedded_mcu").unwrap();
        assert!(mcu
            .supports
            .iter()
            .any(|e| e.strength == Strength::Strong && e.level == Level::Provisional));
        // Recorded, they are NEEDS_CONFIRMATION with the class's risk and the evidence.
        assert!(record(&mut s, 2) >= 3);
        let e = s.entry("domain.embedded_mcu").unwrap();
        assert_eq!(e.status, ReqStatus::NeedsConfirmation);
        assert!(e.risk_if_wrong.is_some() && e.owner.is_none());
        assert!(e.note.as_deref().unwrap().contains("firmware.memory_min"));
        // Once the strong value is confirmed, the class is DERIVED from it.
        s.set("firmware.memory_min", entry(ReqStatus::Confirmed, memory))
            .unwrap();
        assert!(record(&mut s, 3) >= 1);
        let e = s.entry("domain.embedded_mcu").unwrap();
        assert_eq!(e.status, ReqStatus::Derived);
        assert!(e.derived_from.contains(&"firmware.memory_min".to_string()));
        // Nothing changed since: classifying again records nothing.
        assert_eq!(record(&mut s, 4), 0);
    }

    /// Negative test: evidence for and against a class is a conflict, never resolved by score.
    #[test]
    fn contradictory_evidence_is_a_conflict() {
        let mut s = RequirementSet::default();
        let mains = ReqValue::Range {
            min: Some(207.0),
            nom: Some(230.0),
            max: Some(253.0),
            unit: ReqUnit::Volt,
        };
        s.set("supply.input_voltage", entry(ReqStatus::Confirmed, mains))
            .unwrap();
        s.set(
            "safety.max_working_voltage",
            entry(ReqStatus::Confirmed, number(48.0, ReqUnit::Volt)),
        )
        .unwrap();
        let radio = s
            .upsert_item(None, ReqItemKind::Interface, "Radio", None)
            .unwrap()
            .unwrap();
        s.set(
            &format!("{radio}/protocol"),
            entry(ReqStatus::Confirmed, text("BLE 5")),
        )
        .unwrap();
        s.set("safety.radio_certification", not_applicable())
            .unwrap();
        let c = classify(&s);
        for id in ["high_voltage", "rf_wireless", "ac_mains_hv"] {
            let r = class(&c, id).expect(id);
            assert_eq!(r.verdict, Verdict::Conflict, "{id}");
            assert!(!r.supports.is_empty() && !r.contradicts.is_empty(), "{id}");
        }
        record(&mut s, 2);
        let e = s.entry("domain.high_voltage").unwrap();
        assert_eq!(e.value, Some(ReqValue::Flag { value: true }));
        assert_eq!(e.conflicts.len(), 1);
        assert_eq!(e.conflicts[0].value, Some(ReqValue::Flag { value: false }));
        let against = e.conflicts[0].note.as_deref().unwrap();
        assert!(against.contains("safety.max_working_voltage"), "{against}");
    }

    #[test]
    fn firm_structured_evidence_is_derived_and_goes_stale() {
        let mut s = RequirementSet::default();
        let rail = s
            .upsert_item(None, ReqItemKind::Rail, "VMOT", None)
            .unwrap()
            .unwrap();
        let peak = format!("{rail}/current_peak");
        s.set(
            &peak,
            entry(ReqStatus::Confirmed, number(4500.0, ReqUnit::Milliampere)),
        )
        .unwrap();
        let c = classify(&s);
        assert_eq!(class(&c, "high_current").unwrap().verdict, Verdict::Applies);
        record(&mut s, 2);
        let e = s.entry("domain.high_current").unwrap().clone();
        assert_eq!(e.status, ReqStatus::Derived);
        assert_eq!(e.derived_from, vec![peak.clone()]);
        assert!(e.source.as_ref().unwrap().reference.starts_with(EVALUATOR));
        // The input changes after the classification: the class is stale until classified again.
        let later = ReqEntry {
            updated_revision: 3,
            ..entry(ReqStatus::Confirmed, number(1.0, ReqUnit::Ampere))
        };
        assert_eq!(s.record(&peak, later, true), Ok(Recorded::Set));
        let r = evaluate(&s);
        let f = r
            .domain
            .iter()
            .find(|f| f.path == "domain.high_current")
            .unwrap();
        assert_eq!(f.level, Level::Provisional);
        // Classified again, the class has no evidence left and the classifier's entry goes.
        record(&mut s, 4);
        assert!(s.entry("domain.high_current").is_none());
    }

    #[test]
    fn a_person_decision_is_never_replaced_only_disputed() {
        let mut s = RequirementSet::default();
        s.set(
            "supply.sources",
            entry(ReqStatus::Confirmed, text("USB-C 5 V")),
        )
        .unwrap();
        // A person rejected USB for this board.
        let rejected = ReqEntry {
            source: source(SourceKind::Engineer),
            ..entry(ReqStatus::Confirmed, ReqValue::Flag { value: false })
        };
        s.set("domain.usb_interface", rejected.clone()).unwrap();
        record(&mut s, 2);
        let e = s.entry("domain.usb_interface").unwrap();
        assert_eq!(e.status, ReqStatus::Confirmed);
        assert_eq!(e.value, rejected.value, "the person's value stands");
        assert_eq!(e.conflicts.len(), 1, "the classifier's view is a conflict");
        assert!(by_classifier(&e.conflicts[0]));
        // Unchanged evidence: nothing more to record.
        assert_eq!(record(&mut s, 3), 0);
        // Once the evidence is gone, the classifier's conflicting value goes too.
        s.clear("supply.sources").unwrap();
        record(&mut s, 4);
        let e = s.entry("domain.usb_interface").unwrap();
        assert!(e.conflicts.is_empty());
        assert_eq!(e.value, rejected.value);
    }

    #[test]
    fn classification_is_deterministic() {
        let mut s = RequirementSet::default();
        s.set(
            "function.description",
            entry(
                ReqStatus::NeedsConfirmation,
                text("Mains-powered BLDC fan controller with Wi-Fi"),
            ),
        )
        .unwrap();
        assert_eq!(classify(&s), classify(&s.clone()));
        assert_eq!(
            plan(&s, &classify(&s), 5, 5),
            plan(&s, &classify(&s), 5, 5)
        );
    }
}
