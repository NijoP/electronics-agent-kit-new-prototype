//! Part-rating facts shared by validation (ERC-022..024) and the harness: the rating names EAK
//! libraries use and the derating fractions with their sources. Moved here from
//! `eak-intelligence::mental` so the verification engine does not depend on the agent crate.

use serde::Serialize;

/// A derating fact: use at most `max_fraction` of the rating.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Derating {
    /// Reference-designator prefix the fact applies to (`*` = any part).
    pub refdes_prefix: &'static str,
    /// `voltage` or `current`.
    pub quantity: &'static str,
    pub max_fraction: f64,
    pub source: &'static str,
}

/// Derating facts. Most specific first; `derating` picks the first match.
pub const DERATING: &[Derating] = &[
    Derating {
        refdes_prefix: "C",
        quantity: "voltage",
        max_fraction: 0.5,
        source: "DESK component-selection skill: 50 % voltage derating for ceramic capacitors (DC-bias loss); assumes MLCC",
    },
    Derating {
        refdes_prefix: "L",
        quantity: "current",
        max_fraction: 0.75,
        source: "DESK component-selection skill: 25 % current derating for power inductors",
    },
    Derating {
        refdes_prefix: "Q",
        quantity: "current",
        max_fraction: 0.75,
        source: "DESK component-selection skill: 25 % current derating for MOSFETs",
    },
    Derating {
        refdes_prefix: "D",
        quantity: "current",
        max_fraction: 0.75,
        source: "DESK component-selection skill: 25 % current derating for diodes",
    },
    Derating {
        refdes_prefix: "*",
        quantity: "voltage",
        max_fraction: 0.8,
        source: "DESK ipc.py default derating 0.8",
    },
];

/// The derating fact for a part and quantity, if any.
pub fn derating(refdes: &str, quantity: &str) -> Option<&'static Derating> {
    let prefix: String = refdes
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    DERATING
        .iter()
        .find(|d| d.quantity == quantity && (d.refdes_prefix == "*" || d.refdes_prefix == prefix))
}

/// Rating names EAK libraries use for a voltage limit.
pub const VOLTAGE_RATINGS: &[&str] = &[
    "voltage",
    "voltage_max",
    "vds_max",
    "vce_max",
    "reverse_voltage_max",
    "input_voltage_max",
    "supply_voltage_max",
];

/// Rating names EAK libraries use for a current limit of a series (rail-current) part.
pub const CURRENT_RATINGS: &[&str] = &[
    "saturation_current",
    "current",
    "current_max",
    "drain_current_max",
    "forward_current_max",
];

/// Rating names EAK libraries use for a power (dissipation) limit.
pub const POWER_RATINGS: &[&str] = &["power", "power_max"];

/// Rating names for the junction-to-ambient thermal resistance (unit `°C/W`).
pub const THETA_JA_RATINGS: &[&str] = &["theta_ja"];

/// Rating names for the maximum junction temperature (unit `°C`).
pub const TJ_MAX_RATINGS: &[&str] = &["tj_max"];
