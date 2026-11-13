import type { DomainReport } from "./types";

const ceiling = (label: DomainReport["classification"]["candidates"][0]["maturity"]) => ({
  label,
  claim:
    label === "DRAFT"
      ? "Draft: written but never exercised. Review prompts only, not requirements."
      : "Offline-tested in DESK fixtures. Not verified against the cited standards and not validated on hardware.",
  may_present_as_requirement: false,
  may_show_guidance: true,
});

export function domainReport(revision: number): DomainReport {
  return {
    revision,
    classification: {
      status: "CLASSIFIED",
      confidence: 0.82,
      primary: "esp32_dev_board",
      secondary: [],
      candidates: [
        {
          archetype_id: "esp32_dev_board",
          name: "ESP32 development board",
          domain: "embedded_mcu",
          domain_name: "Embedded systems and MCU boards",
          description: "ESP32 module with USB programming.",
          score: 21,
          reasons: ["keyword 'esp32' in project name (+6)", "protocols [\"ble\", \"usb\", \"wifi\"] (+6)"],
          penalties: [],
          maturity: "OFFLINE_TESTED",
          ceiling: ceiling("OFFLINE_TESTED"),
        },
        {
          archetype_id: "buck_converter",
          name: "Buck converter",
          domain: "dcdc_power",
          domain_name: "DC-DC power electronics",
          description: "Step-down converter.",
          score: 4,
          reasons: ["keyword 'buck' in requirement req_0 (+10)"],
          penalties: ["input 24 V far outside 4.5-12 V (-2)"],
          maturity: "DRAFT",
          ceiling: ceiling("DRAFT"),
        },
      ],
      overlays: ["rf_communication", "usb_interface"],
      required_component_families: [],
      missing_information: ["load current or power level"],
      conflicts: [],
      evidence: [],
      guidance: {
        archetype_id: "esp32_dev_board",
        ceiling: ceiling("OFFLINE_TESTED"),
        heading: "Review prompts (unverified)",
        critical_electrical_constraints: ["Strapping pins at the documented level during reset"],
        verification_requirements: [],
        required_component_roles: [],
        applicable_standards: [],
        unsupported_conditions: [],
        desk_rule_ids: [],
        maturity_evidence: "",
      },
      registry_classes: 100,
      not_implemented: ["DESK rule-set evaluation"],
      limitations: ["A class is a hypothesis about the design's domain, not a certification of the design."],
    },
  };
}
