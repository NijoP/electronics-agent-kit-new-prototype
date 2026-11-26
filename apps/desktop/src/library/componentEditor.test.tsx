// @vitest-environment jsdom
// Personal-library component editor against a mocked Tauri IPC: the edited definition (name,
// pin-to-pad map) goes to `component_save`, a refused save is shown, and a saved part whose
// footprint rules are not all passing reads "Footprint unverified". The Rust side is tested
// headless in src-tauri/tests/library_verify_ipc.rs.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "../api";
import type { ComponentDef } from "../types";
import { ComponentEditor, definitionOf, formOf } from "./ComponentEditor";

const base: ComponentDef = {
  id: "R_0603",
  name: "Resistor 0603",
  category: "Resistor",
  refdes_prefix: "R",
  default_value: "10k",
  manufacturer: null,
  mpn: null,
  distributor: null,
  datasheet: null,
  symbol: {
    pins: [
      { number: "1", name: "~", electrical_type: "passive", position: { x: -5_000_000, y: 0 } },
      { number: "2", name: "~", electrical_type: "passive", position: { x: 5_000_000, y: 0 } },
    ],
    body_half_w: 2_000_000,
    body_half_h: 1_000_000,
  },
  footprint: {
    name: "R_0603",
    pads: [
      { number: "1", kind: "smd", shape: "rect", position: { x: -800_000, y: 0 }, size_x: 800_000, size_y: 900_000, drill: null },
      { number: "2", kind: "smd", shape: "rect", position: { x: 800_000, y: 0 }, size_x: 800_000, size_y: 900_000, drill: null },
    ],
    courtyard_half_w: 1_500_000,
    courtyard_half_h: 800_000,
    body_height: null,
  },
  pin_map: { "1": ["1"], "2": ["2"] },
  ratings: [],
  provenance: "built in",
};

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];
let refuse: boolean;

beforeEach(() => {
  calls = [];
  refuse = false;
  mockIPC((cmd, payload) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    if (cmd !== "component_save") throw new Error(`unmocked ${cmd}`);
    if (refuse) throw new Error("the pin map is wrong and was not saved: pad 1 joins pins 1 (~), 2 (VCC)");
    const def = args.def as ComponentDef;
    return {
      id: def.id,
      scope: "personal",
      replaced: false,
      pin_pad: { check: "pin_pad", definition_id: def.id, footprint: "R_0603", exposed_pads: [], rule_results: [], overall: "PASS", not_implemented: [], limitations: [] },
      footprint: {
        check: "footprint",
        definition_id: def.id,
        footprint: "R_0603",
        spec_source: null,
        tolerance_nm: 50_000,
        rule_results: [{ rule_id: "FP-002", state: "UNKNOWN", reason: "no land-pattern table" }],
        overall: "UNKNOWN",
        not_implemented: [],
        limitations: [],
      },
    };
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

describe("definitionOf", () => {
  it("builds the edited definition and refuses unusable ids", () => {
    const f = { ...formOf(base), id: "MY_R", name: " My R ", mpn: "RC0603", pads: { "1": "1, 3", "2": "" } };
    const r = definitionOf(base, f);
    expect("def" in r ? r.def : null).toMatchObject({ id: "MY_R", name: "My R", mpn: "RC0603", manufacturer: null, pin_map: { "1": ["1", "3"], "2": [] } });
    expect("problem" in definitionOf(base, { ...f, id: "../x" })).toBe(true);
    expect("problem" in definitionOf(base, { ...f, id: ".hidden" })).toBe(true);
    expect("problem" in definitionOf(base, { ...f, name: " " })).toBe(true);
  });
});

describe("ComponentEditor", () => {
  it("creates a new personal part from a definition and shows its footprint as unverified", async () => {
    render(<ComponentEditor def={base} />);
    fireEvent.change(screen.getByLabelText("Personal part id"), { target: { value: "MY_R" } });
    fireEvent.change(screen.getByLabelText("Personal part name"), { target: { value: "My resistor" } });
    fireEvent.change(screen.getByLabelText("Pads of pin 2"), { target: { value: "2" } });
    fireEvent.click(screen.getByRole("button", { name: "Save to personal library" }));
    const done = await screen.findByTestId("component-saved");
    expect(done.textContent).toContain("Created MY_R in the personal library");
    expect(screen.getByLabelText("Footprint status").textContent).toBe("Footprint unverified");
    const sent = calls.find((c) => c.cmd === "component_save")?.args.def as ComponentDef;
    expect(sent).toMatchObject({ id: "MY_R", name: "My resistor", pin_map: { "1": ["1"], "2": ["2"] } });
  });

  it("shows why the backend refused the save", async () => {
    refuse = true;
    render(<ComponentEditor def={base} />);
    fireEvent.click(screen.getByRole("button", { name: "Save to personal library" }));
    await waitFor(() => expect(screen.getByTestId("component-save-error").textContent).toContain("pin map is wrong"));
    expect(screen.queryByTestId("component-saved")).toBeNull();
  });

  it("disables Save with the reason while the id is unusable", () => {
    render(<ComponentEditor def={base} />);
    fireEvent.change(screen.getByLabelText("Personal part id"), { target: { value: "a/b" } });
    const save = screen.getByRole("button", { name: "Save to personal library" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    expect(save.title).toMatch(/letters, digits/);
  });
});
