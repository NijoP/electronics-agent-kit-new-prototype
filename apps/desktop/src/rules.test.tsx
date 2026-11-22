// Design rules / net class dialog: each Apply is one typed engine command; bad input sends nothing.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { fixture } from "./fixture.test-data";
import { netNames } from "./shell/panels";
import { netDeclaration, RulesDialog } from "./shell/RulesDialog";
import type { Command, Snapshot } from "./types";

function withRules(): Snapshot {
  const s = fixture();
  s.project.constraints = {
    min_track_width: 150_000, min_clearance: 150_000, min_drill: 300_000, min_annular_ring: 130_000, board_edge_clearance: 300_000,
    default_class: { name: "Default", track_width: 250_000, clearance: 200_000, via_drill: 300_000, via_diameter: 600_000, nets: [] },
    net_classes: [{ name: "Power", track_width: 500_000, clearance: 250_000, via_drill: 400_000, via_diameter: 800_000, nets: ["GND"] }],
    net_currents_a: { GND: 2 }, net_voltages_v: {}, temp_rise_c: 10, max_component_height: null,
  };
  return s;
}
let sent: (Command | Command[])[];
const setup = () => {
  sent = [];
  render(<RulesDialog snapshot={withRules()} busy={false} apply={async (c) => void sent.push(c)} onClose={() => {}} />);
};
afterEach(cleanup);
const type = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });

describe("design rules dialog", () => {
  it("shows the engine's current values and applies board rules as one SetDesignRules (mm to nm)", () => {
    setup();
    expect((screen.getByLabelText("Min clearance (mm)") as HTMLInputElement).value).toBe("0.15");
    type("Min clearance (mm)", "0.2");
    type("Max component height (mm, optional)", "12");
    fireEvent.click(screen.getByRole("button", { name: "Apply board rules" }));
    expect(sent).toHaveLength(1);
    expect((sent[0] as Command[])[0]).toMatchObject({ type: "SetDesignRules", min_clearance: 200_000, min_track_width: 150_000, default_track_width: 250_000, temp_rise_c: 10, max_component_height: 12_000_000 });
  });
  it("rejects non-numeric input without sending anything", () => {
    setup();
    type("Min drill (mm)", "abc");
    fireEvent.click(screen.getByRole("button", { name: "Apply board rules" }));
    expect(screen.getByRole("alert").textContent).toContain("min drill");
    expect(sent).toEqual([]);
  });
  it("saves a net class as UpsertNetClass, loading an existing class for editing", () => {
    setup();
    fireEvent.click(screen.getByRole("button", { name: /Power/ }));
    expect((screen.getByLabelText("Class track width (mm)") as HTMLInputElement).value).toBe("0.5");
    type("Class track width (mm)", "0.8");
    type("Nets (comma separated)", "GND, VIN");
    fireEvent.click(screen.getByRole("button", { name: "Save net class" }));
    expect((sent[0] as Command[])[0]).toEqual({ type: "UpsertNetClass", class: { name: "Power", track_width: 800_000, clearance: 250_000, via_drill: 400_000, via_diameter: 800_000, nets: ["GND", "VIN"] } });
  });
  it("declares and clears net current and voltage, sending only what changed", () => {
    setup();
    fireEvent.change(screen.getByLabelText("Declared net"), { target: { value: "GND" } });
    expect((screen.getByLabelText("Net current (A)") as HTMLInputElement).value).toBe("2");
    type("Net voltage (V)", "12");
    fireEvent.click(screen.getByRole("button", { name: "Apply net declaration" }));
    // The stored 2 A is untouched, so only the voltage is sent.
    expect(sent[0]).toEqual([{ type: "SetNetVoltage", net: "GND", volts: 12 }]);
    type("Net current (A)", "");
    fireEvent.click(screen.getByRole("button", { name: "Apply net declaration" }));
    expect((sent[1] as Command[])[0]).toMatchObject({ type: "Batch" });
    expect(((sent[1] as Command[])[0] as Extract<Command, { type: "Batch" }>).commands).toEqual([
      { type: "SetNetCurrent", net: "GND", amps: null },
      { type: "SetNetVoltage", net: "GND", volts: 12 },
    ]);
  });

  // Regression (inventory F4): the first net was preselected with empty fields, so Apply sent
  // amps/volts null and silently cleared its stored declaration.
  it("prefills the preselected net and never clears an untouched declaration", () => {
    sent = [];
    const s = withRules();
    const first = netNames(s)[0];
    s.project.constraints!.net_currents_a = { [first]: 1.5 };
    s.project.constraints!.net_voltages_v = { [first]: 5 };
    render(<RulesDialog snapshot={s} busy={false} apply={async (c) => void sent.push(c)} onClose={() => {}} />);
    expect((screen.getByLabelText("Declared net") as HTMLSelectElement).value).toBe(first);
    expect((screen.getByLabelText("Net current (A)") as HTMLInputElement).value).toBe("1.5");
    expect((screen.getByLabelText("Net voltage (V)") as HTMLInputElement).value).toBe("5");
    fireEvent.click(screen.getByRole("button", { name: "Apply net declaration" }));
    expect(sent).toEqual([]);
    expect(screen.getByRole("alert").textContent).toContain("Nothing to apply");
  });

  it("sends current and voltage together as ONE Batch (one revision, one undo)", () => {
    const d = netDeclaration(withRules().project.constraints, "VIN", "0.5", "12");
    expect(d).toEqual({
      command: {
        type: "Batch",
        label: "Declare net VIN",
        commands: [
          { type: "SetNetCurrent", net: "VIN", amps: 0.5 },
          { type: "SetNetVoltage", net: "VIN", volts: 12 },
        ],
      },
    });
    expect(netDeclaration(undefined, "VIN", "x", "")).toHaveProperty("error");
  });
});
