// Small engine-shaped snapshot used by the UI tests (shape checked on the Rust side by
// tests/ipc.rs `geometry_is_derived_from_eak_ir_with_cross_probe_ids`).
import type { CheckResult, Snapshot } from "./types";

const mm = (v: number) => v * 1_000_000;
const pt = (x: number, y: number) => ({ x: mm(x), y: mm(y) });

export function fixture(revision = 7): Snapshot {
  return {
    project: {
      schema_version: 1,
      id: "prj_1",
      name: "Fixture board",
      revision,
      schematic: {
        sheets: { sht_1: { id: "sht_1", name: "Sheet 1", width: mm(297), height: mm(210) } },
        symbols: {
          sym_r1: { id: "sym_r1", sheet_id: "sht_1", component_id: "R_0603", refdes: "R1", value: "330", position: pt(50, 50), rotation: "R0" },
          sym_d1: { id: "sym_d1", sheet_id: "sht_1", component_id: "LED_0603", refdes: "D1", value: "Green", position: pt(80, 50), rotation: "R0" },
        },
        wires: { wir_1: { id: "wir_1", sheet_id: "sht_1", a: pt(55, 50), b: pt(75, 50) } },
        labels: { lab_1: { id: "lab_1", sheet_id: "sht_1", name: "GND", position: pt(85, 50), kind: "power" } },
        no_connects: {},
      },
      pcb: {
        board: { outline: [pt(0, 0), pt(30, 0), pt(30, 20), pt(0, 20)] },
        footprints: {
          fp_r1: { id: "fp_r1", symbol_id: "sym_r1", component_id: "R_0603", refdes: "R1", position: pt(10, 10), rotation: "R0", side: "top", locked: false },
          fp_d1: { id: "fp_d1", symbol_id: "sym_d1", component_id: "LED_0603", refdes: "D1", position: pt(20, 10), rotation: "R0", side: "top", locked: false },
        },
        tracks: { trk_1: { id: "trk_1", layer: "top", width: mm(0.25), a: pt(11, 10), b: pt(19, 10), net: "N1" } },
        vias: { via_1: { id: "via_1", position: pt(15, 15), drill: mm(0.3), diameter: mm(0.6), net: "GND" } },
        zones: { zon_1: { id: "zon_1", layer: "bottom", net: "GND", outline: [pt(1, 1), pt(29, 1), pt(29, 19), pt(1, 19)], clearance: mm(0.3) } },
      },
    },
    dir: "/projects/fixture.eak",
    can_undo: true,
    can_redo: false,
    blocked: null,
    recovery: { reconciled: [], torn_temp_file_removed: false },
    geometry: {
      symbols: [
        { id: "sym_r1", sheet_id: "sht_1", refdes: "R1", value: "330", position: pt(50, 50), half_w: mm(2), half_h: mm(1), pins: [{ number: "1", name: "~", position: pt(46, 50) }, { number: "2", name: "~", position: pt(54, 50) }] },
        { id: "sym_d1", sheet_id: "sht_1", refdes: "D1", value: "Green", position: pt(80, 50), half_w: mm(2), half_h: mm(1), pins: [{ number: "1", name: "K", position: pt(84, 50) }, { number: "2", name: "A", position: pt(76, 50) }] },
      ],
      footprints: [
        { id: "fp_r1", symbol_id: "sym_r1", refdes: "R1", side: "top", position: pt(10, 10), half_w: mm(1.5), half_h: mm(0.8), pads: [{ number: "1", kind: "smd", shape: "rect", position: pt(9.2, 10), size_x: mm(0.8), size_y: mm(0.9), drill: null, net: "N1" }] },
        { id: "fp_d1", symbol_id: "sym_d1", refdes: "D1", side: "top", position: pt(20, 10), half_w: mm(1.5), half_h: mm(0.8), pads: [{ number: "1", kind: "tht", shape: "circle", position: pt(20, 10), size_x: mm(1.6), size_y: mm(1.6), drill: mm(0.8), net: "GND" }] },
      ],
    },
  };
}

export function ercResult(revision = 7): CheckResult {
  return {
    report: {
      check: "ERC",
      project_id: "prj_1",
      revision,
      input_sha256: "ab",
      tool: "eak-validation 0.1.0",
      findings: [
        { rule_id: "ERC-001", severity: "error", message: "D1 pin 1 (K) is unconnected", objects: ["fp_d1"], location: null },
        { rule_id: "ERC-009", severity: "warning", message: "net-level note", objects: ["net_x"], location: null },
      ],
      outcome: "ERRORS",
      errors: 1,
      warnings: 1,
      not_implemented: ["Simulation-based checks"],
      limitations: ["Pin types come from library data"],
    },
    evidence_path: "/projects/fixture.eak/evidence/erc-r000007.json",
  };
}
