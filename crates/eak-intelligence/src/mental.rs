//! Engineering mental models recovered from DESK (`desk/mental/registry.py`,
//! `.agents/skills/schematic-engineering`, `.agents/skills/component-selection`; MIT, see
//! `third_party/licenses/DESK/`), re-expressed as typed EAK data instead of prompt text.
//!
//! The four kinds of knowledge stay separate:
//! * **facts**       – `DERATING` (numbers with their source);
//! * **constraints** – `HardConstraint`s, each naming the deterministic EAK evaluator that checks
//!   it, or `None` when nothing in EAK evaluates it yet (never reported as passing);
//! * **procedures**  – ordered `Step`s, each naming the harness tool that gates it;
//! * **guidance**    – trade-off priorities for the agent's own judgement.
//!
//! Nothing here evaluates a design; the harness design flow and the validators do.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    pub id: &'static str,
    pub title: &'static str,
    pub detail: &'static str,
    /// EAK tool / evaluator that checks this step; `None` = agent judgement only.
    pub checked_by: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HardConstraint {
    pub id: &'static str,
    pub statement: &'static str,
    /// Deterministic EAK evaluator; `None` = not implemented in EAK (never "passed").
    pub evaluated_by: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MentalModel {
    pub id: &'static str,
    pub domain: &'static str,
    pub version: &'static str,
    /// Where in DESK the model came from.
    pub source: &'static str,
    /// EAK module / worker area that owns acting on it.
    pub owner: &'static str,
    pub procedure: Vec<Step>,
    pub constraints: Vec<HardConstraint>,
    pub guidance: Vec<&'static str>,
}

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

fn step(
    id: &'static str,
    title: &'static str,
    detail: &'static str,
    checked_by: Option<&'static str>,
) -> Step {
    Step {
        id,
        title,
        detail,
        checked_by,
    }
}

/// DESK's 12-step schematic mental model, each step tied to the EAK gate that checks it.
pub fn schematic_procedure() -> Vec<Step> {
    vec![
        step("S1", "Understand the requirements", "Capture rails (V/I), conditions, safety, interfaces, protection and acceptance criteria as a typed spec.", Some("design_spec_set (structural check, domain gates)")),
        step("S2", "Divide the design into functional blocks", "Power stage, control, interfaces, protection; one plan per block.", None),
        step("S3", "Define the power architecture", "Voltage tree and power paths from the spec rails; worst-case input for duty/ripple.", Some("design_validate: buck_duty_cycle / buck_inductor_ripple_a")),
        step("S4", "Identify interfaces and signals", "Name every net the spec and interfaces refer to.", Some("design_validate: rail-net-missing")),
        step("S5", "Select components", "Library parts with MPN and datasheet; no unresolved MPN.", Some("design_validate: component-unresolved-mpn / component-no-datasheet")),
        step("S6", "Verify component datasheets", "Ratings with datasheet sources; derate per DERATING.", Some("design_validate: derating:*")),
        step("S7", "Verify every physical pin and pin function", "Read pins from get_component; never invent a pin map.", Some("design_plan: pin-not-on-component")),
        step("S8", "Define connectivity", "State the intended net of every pin in the plan's intent.", Some("design_plan: intent")),
        step("S9", "Construct block schematics", "Typed commands only (PlaceSymbol, LabelPin, ConnectPins, ...), applied as one batch per plan.", Some("design_apply")),
        step("S10", "Validate electrical connections", "ERC on the result.", Some("design_validate: erc")),
        step("S11", "Read the resulting netlist back", "Rebuild the netlist from EAK-IR, never from the drawing.", Some("design_validate: netlist read-back")),
        step("S12", "Compare the result with the intended design", "Every intent pin must be on its intended net.", Some("design_validate: intent-mismatch")),
    ]
}

fn hc(
    id: &'static str,
    statement: &'static str,
    evaluated_by: Option<&'static str>,
) -> HardConstraint {
    HardConstraint {
        id,
        statement,
        evaluated_by,
    }
}

/// DESK's six mental models (M1-M6) as typed EAK data.
pub fn models() -> Vec<MentalModel> {
    vec![
        MentalModel {
            id: "M1_SCHEMATIC",
            domain: "schematic",
            version: "1.0.0",
            source: "DESK desk/mental/registry.py M1; .agents/skills/schematic-engineering",
            owner: "eak-harness design flow (design_*)",
            procedure: schematic_procedure(),
            constraints: vec![
                hc(
                    "every_pin_accounted",
                    "every pin is connected or marked no-connect",
                    Some("eak-validation ERC"),
                ),
                hc(
                    "no_unresolved_mpn",
                    "every placed part has a manufacturer part number",
                    Some("design_validate: component-unresolved-mpn"),
                ),
                hc(
                    "formula_required_for_passives",
                    "passive values come from a recorded calculation",
                    Some("design_claim basis=calculation + criterion"),
                ),
                hc(
                    "intent_matches_netlist",
                    "the read-back netlist matches the stated intent",
                    Some("design_validate: intent-mismatch"),
                ),
                hc(
                    "default_off_reset_state",
                    "outputs default to a safe state at reset",
                    None,
                ),
            ],
            guidance: vec![
                "electrical_correctness",
                "failsafe_reset_behaviour",
                "bom_consolidation",
                "drawing_neatness",
            ],
        },
        MentalModel {
            id: "M2_PLACEMENT",
            domain: "placement",
            version: "1.0.0",
            source: "DESK desk/mental/registry.py M2",
            owner: "eak-engineering placer (placement worker)",
            procedure: vec![
                step(
                    "P1",
                    "Mechanical anchors",
                    "outline, mounting holes, keepouts",
                    Some("eak-validation MECH"),
                ),
                step(
                    "P2",
                    "Connectors and terminals",
                    "mating envelopes, screwdriver access",
                    None,
                ),
                step("P3", "Critical ICs and PMICs", "pin escape corridors", None),
                step(
                    "P4",
                    "Functional clusters",
                    "power stage, MCU, sensors, RF",
                    Some("AutoPlace"),
                ),
                step(
                    "P5",
                    "Topological passives",
                    "decoupling < 2 mm, ESD at connector, crystals",
                    None,
                ),
                step(
                    "P6",
                    "Routability gate",
                    "ratsnest crossings, escape congestion",
                    Some("get_connectivity"),
                ),
            ],
            constraints: vec![hc(
                "decap_max_distance_mm",
                "decoupling within 2 mm of its pin",
                None,
            )],
            guidance: vec![
                "mechanical_access",
                "thermal_isolation",
                "high_speed_loop_area",
                "routing_area_optimisation",
            ],
        },
        MentalModel {
            id: "M3_ROUTING",
            domain: "routing",
            version: "1.0.0",
            source: "DESK desk/mental/registry.py M3",
            owner: "eak-engineering router (routing worker)",
            procedure: vec![
                step(
                    "R1",
                    "GND preparation",
                    "plane access vias, escape stubs",
                    None,
                ),
                step("R2", "Connector escapes", "fine-pitch breakout", None),
                step(
                    "R3",
                    "Critical signals",
                    "diff pairs, RF, clocks, analog sense",
                    None,
                ),
                step("R4", "Control and sensors", "I2C, SPI, GPIO", None),
                step(
                    "R5",
                    "Power distribution",
                    "profile-driven width and thermal",
                    Some("design_validate: rail-trace-capacity"),
                ),
                step("R6", "Remaining nets", "everything else", Some("AutoRoute")),
            ],
            constraints: vec![
                hc(
                    "no_unintended_shorts",
                    "no copper shorts between nets",
                    Some("eak-validation DRC / get_connectivity"),
                ),
                hc(
                    "no_unconnected_pads",
                    "every connection routed",
                    Some("get_connectivity ratsnest"),
                ),
            ],
            guidance: vec!["signal_integrity", "current_capacity", "manufacturability"],
        },
        MentalModel {
            id: "M4_COPPER_GROUND",
            domain: "copper",
            version: "1.0.0",
            source: "DESK desk/mental/registry.py M4",
            owner: "eak-engineering copper (copper worker)",
            procedure: vec![
                step(
                    "C1",
                    "Reference plane allocation",
                    "from the actual stackup",
                    None,
                ),
                step(
                    "C2",
                    "High-current polygon zones",
                    "power rail flooding",
                    Some("AddZone"),
                ),
                step(
                    "C3",
                    "Ground flood and island removal",
                    "no floating copper",
                    None,
                ),
                step(
                    "C4",
                    "Thermal relief vs solid",
                    "solid for high current",
                    None,
                ),
                step("C5", "Stitching via arrays", "return-path continuity", None),
                step(
                    "C6",
                    "Copper to edge clearance",
                    "board edge keep-back",
                    Some("eak-validation DRC"),
                ),
            ],
            constraints: vec![hc(
                "no_floating_copper_islands",
                "no unconnected copper",
                None,
            )],
            guidance: vec![
                "unbroken_return_paths",
                "power_plane_low_impedance",
                "thermal_dissipation",
                "etching_balance",
            ],
        },
        MentalModel {
            id: "M5_ELECTRICAL_MFG",
            domain: "mfg",
            version: "1.0.0",
            source: "DESK desk/mental/registry.py M5",
            owner: "eak-engineering ipc + eak-validation DFM",
            procedure: vec![
                step(
                    "E1",
                    "Manufacturer capability limits",
                    "min trace, clearance, drill, annular ring",
                    Some("eak-validation DFM"),
                ),
                step(
                    "E2",
                    "Current and temperature rise",
                    "IPC-2221 in EAK (DESK cites IPC-2152)",
                    Some("eak-validation DRC net currents"),
                ),
                step(
                    "E3",
                    "Voltage clearance and creepage",
                    "IPC-2221 / IEC 60664-1",
                    Some("eak-engineering ipc::clearance_mm"),
                ),
                step(
                    "E4",
                    "Controlled impedance",
                    "dielectric height and er",
                    None,
                ),
                step(
                    "E5",
                    "Assembly and soldermask",
                    "mask and paste rules",
                    Some("eak-validation DFM"),
                ),
            ],
            constraints: vec![hc(
                "trace_width_gte_mfg_min",
                "tracks at least the fab minimum",
                Some("eak-validation DFM"),
            )],
            guidance: vec![
                "fabrication_yield",
                "electrical_safety_margin",
                "board_density",
                "fabrication_cost",
            ],
        },
        MentalModel {
            id: "M6_VERIFICATION_REVIEW",
            domain: "verification",
            version: "1.0.0",
            source: "DESK desk/mental/registry.py M6",
            owner: "eak-harness design_report + eak-validation",
            procedure: vec![
                step(
                    "V1",
                    "Hard acceptance gates",
                    "non-negotiable pass/fail",
                    Some("design_validate errors == 0"),
                ),
                step(
                    "V2",
                    "Weighted quality scoring",
                    "optimisation metric",
                    None,
                ),
                step(
                    "V3",
                    "Residual engineering risk audit",
                    "unverified and assumed labels",
                    Some("design_report label_counts"),
                ),
            ],
            constraints: vec![hc(
                "corroborated_evidence_before_ai_assertions",
                "claims are labelled by EAK, not by the agent",
                Some("design_report"),
            )],
            guidance: vec![
                "deterministic_hard_gates_before_weighted_score",
                "traceability_to_requirements",
                "corroborated_evidence_before_ai_assertions",
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    //! Written during the testing freeze: unrun.
    use super::*;

    #[test]
    fn derating_picks_most_specific() {
        assert_eq!(derating("C12", "voltage").unwrap().max_fraction, 0.5);
        assert_eq!(derating("U1", "voltage").unwrap().max_fraction, 0.8);
        assert_eq!(derating("L1", "current").unwrap().max_fraction, 0.75);
        assert!(derating("R1", "current").is_none());
    }

    #[test]
    fn procedure_has_twelve_steps_and_models_six() {
        assert_eq!(schematic_procedure().len(), 12);
        assert_eq!(models().len(), 6);
    }
}
