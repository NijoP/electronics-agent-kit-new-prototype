//! Engineering domain registry: the EAK-native disciplines a product can combine.
//!
//! This sits *above* the DESK class registry (`crate::domain::registry`): a DESK archetype
//! belongs to one DESK domain id, and each DESK domain id maps to one or more engineering
//! domains here. The table is data, open for extension: a new discipline is a new row, and a
//! domain without a mental model is still classifiable (its `coverage` says so).

use serde::Serialize;

/// How much EAK knows about a domain. Never read as validation coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    /// A structured mental model exists (`data/discipline_models.json`).
    MentalModel,
    /// Classifiable and inheritable, but no mental model yet.
    RegistryOnly,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineeringDomain {
    pub id: &'static str,
    pub name: &'static str,
    /// DESK domain ids (`desk_domains.json`) that imply this domain.
    pub desk_domains: &'static [&'static str],
    /// DESK overlay ids that imply this domain.
    pub desk_overlays: &'static [&'static str],
    /// High-risk application: risks raised in this domain need human expert approval
    /// (directive §12: high voltage, mains, batteries, motor drives, medical, safety-critical).
    pub high_risk: bool,
    pub coverage: Coverage,
}

const fn d(
    id: &'static str,
    name: &'static str,
    desk_domains: &'static [&'static str],
    desk_overlays: &'static [&'static str],
    high_risk: bool,
    coverage: Coverage,
) -> EngineeringDomain {
    EngineeringDomain {
        id,
        name,
        desk_domains,
        desk_overlays,
        high_risk,
        coverage,
    }
}

use Coverage::{MentalModel as MM, RegistryOnly as RO};

/// Every engineering domain EAK can name. Order is stable (UI and tests rely on it).
#[rustfmt::skip]
pub const DOMAINS: &[EngineeringDomain] = &[
    d("power", "Power electronics", &["dcdc_power"], &["switching_power", "high_current"], false, MM),
    d("rf", "RF and microwave", &["rf_wireless"], &["rf_communication"], false, MM),
    d("analog", "Analog and mixed-signal", &["analog_sensors"], &["precision_analog"], false, MM),
    d("digital_hs", "Digital and high-speed", &["high_speed_digital", "interfaces_comms"], &["high_speed_digital", "usb_interface"], false, MM),
    d("embedded", "Embedded systems and control", &["embedded_mcu", "test_debug"], &["low_power"], false, RO),
    d("motor_drive", "Motor drives and electromechanical", &["motor_control"], &["motor_drive"], true, MM),
    d("battery", "Battery and energy storage", &["battery_bms"], &["battery_management"], true, MM),
    d("mains_hv", "Mains and high voltage", &["ac_mains_hv"], &["mains_safety_isolation", "galvanic_isolation"], true, RO),
    d("industrial", "Industrial automation", &["industrial_control"], &["industrial_interfaces", "can_interface"], false, RO),
    d("automotive", "Automotive electronics", &["automotive"], &["automotive_environment"], true, RO),
    d("audio_video", "Audio and video electronics", &["audio_video"], &[], false, RO),
    d("lighting", "Lighting and display electronics", &["led_display"], &[], false, RO),
    d("thermal", "Thermal management (cross-cutting)", &[], &["thermal_management"], false, RO),
    d("emc", "EMC and EMI (cross-cutting)", &[], &["emi_emc"], false, RO),
    d("harsh_env", "Harsh-environment and aerospace", &[], &["harsh_environment"], true, RO),
    d("medical", "Medical and assistive electronics", &[], &[], true, RO),
    d("instrumentation", "Instrumentation and measurement", &[], &[], false, RO),
    d("wearable_iot", "Wearable and IoT", &[], &[], false, RO),
    d("security_access", "Security and access control", &[], &[], false, RO),
];

pub fn domain(id: &str) -> Option<&'static EngineeringDomain> {
    DOMAINS.iter().find(|x| x.id == id)
}

/// Engineering domains implied by a DESK domain id.
pub fn from_desk_domain(desk: &str) -> impl Iterator<Item = &'static EngineeringDomain> + '_ {
    DOMAINS
        .iter()
        .filter(move |x| x.desk_domains.contains(&desk))
}

/// Engineering domains implied by a DESK overlay id.
pub fn from_desk_overlay(overlay: &str) -> impl Iterator<Item = &'static EngineeringDomain> + '_ {
    DOMAINS
        .iter()
        .filter(move |x| x.desk_overlays.contains(&overlay))
}

/// Interface keyword table. Single words match whole tokens (so "can" does not match "scan");
/// entries with a space or hyphen match as substrings.
#[rustfmt::skip]
const INTERFACE_DOMAINS: &[(&str, &[&str])] = &[
    ("rf", &["lora", "lorawan", "wifi", "wi-fi", "ble", "bluetooth", "lte", "cellular", "gnss", "gps", "zigbee", "uwb", "subghz", "sub-ghz", "antenna", "rf"]),
    ("digital_hs", &["usb", "ethernet", "pcie", "ddr", "ddr3", "ddr4", "lpddr", "mipi", "hdmi", "sata", "lvds", "serdes", "rgmii", "rmii"]),
    ("industrial", &["can", "canbus", "can-fd", "rs485", "rs-485", "modbus", "lin bus"]),
    ("embedded", &["i2c", "spi", "qspi", "uart", "swd", "jtag", "gpio"]),
    ("motor_drive", &["motor", "phase", "hall", "encoder"]),
    ("battery", &["battery", "cell", "bms"]),
    ("analog", &["adc", "dac", "sensor", "analog", "thermocouple", "rtd", "bridge"]),
    ("mains_hv", &["mains", "ac input", "230v", "120v"]),
];

/// Domain implied by an interface kind or protocol name (spec `interfaces[].kind`).
/// `None` when EAK has no mapping: the caller reports it as missing information.
pub fn from_interface(kind: &str) -> Option<&'static str> {
    let k = kind.to_lowercase();
    let tokens: Vec<&str> = k
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    let hit = |w: &str| {
        if w.contains(' ') || w.contains('-') {
            k.contains(w)
        } else {
            tokens.contains(&w)
        }
    };
    INTERFACE_DOMAINS
        .iter()
        .find(|(_, words)| words.iter().any(|w| hit(w)))
        .map(|(id, _)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::registry;

    // Written, not run (code-reviewed only).
    #[test]
    fn ids_unique_and_desk_ids_exist() {
        let mut seen = std::collections::BTreeSet::new();
        let r = registry();
        for x in DOMAINS {
            assert!(seen.insert(x.id), "duplicate {}", x.id);
            for dd in x.desk_domains {
                assert!(
                    r.domain(dd).is_some(),
                    "{} names unknown DESK domain {dd}",
                    x.id
                );
            }
            for o in x.desk_overlays {
                assert!(
                    r.overlay(o).is_some(),
                    "{} names unknown DESK overlay {o}",
                    x.id
                );
            }
        }
        for dd in &r.domains {
            assert!(
                from_desk_domain(&dd.domain_id).next().is_some(),
                "{} unmapped",
                dd.domain_id
            );
        }
    }

    #[test]
    fn interfaces_map_by_token() {
        assert_eq!(from_interface("LoRa"), Some("rf"));
        assert_eq!(from_interface("USB 2.0"), Some("digital_hs"));
        assert_eq!(from_interface("I2C"), Some("embedded"));
        assert_eq!(from_interface("CAN"), Some("industrial"));
        assert_eq!(from_interface("scan chain interface"), None);
    }
}
