// Design rules / net class dialog: each Apply is one typed engine command; bad input sends nothing.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ercResult, fixture } from "./fixture.test-data";
import { matrixCellRule, ruleCommands } from "./shell/ruleTables";
import { AnnotateDialog, annotateCommand } from "./shell/AnnotateDialog";
import { netNames } from "./shell/panels";
import { netDeclaration, RulesDialog } from "./shell/RulesDialog";
import type { Command, Report, Snapshot } from "./types";

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
    type("Max component height (mm, empty = no limit)", "12");
    fireEvent.click(screen.getByRole("button", { name: "Apply board rules" }));
    expect(sent).toHaveLength(1);
    expect((sent[0] as Command[])[0]).toMatchObject({ type: "SetDesignRules", min_clearance: 200_000, min_track_width: 150_000, default_track_width: 250_000, temp_rise_c: 10, max_component_height: 12_000_000 });
  });
  it("S19 regression: emptying a stored max component height sends an explicit clear (ADR-0050)", () => {
    const s = withRules();
    s.project.constraints!.max_component_height = 12_000_000;
    sent = [];
    render(<RulesDialog snapshot={s} busy={false} apply={async (c) => void sent.push(c)} onClose={() => {}} />);
    const label = "Max component height (mm, empty = no limit)";
    expect((screen.getByLabelText(label) as HTMLInputElement).value).toBe("12");
    type(label, "");
    fireEvent.click(screen.getByRole("button", { name: "Apply board rules" }));
    expect((sent[0] as Command[])[0]).toMatchObject({ type: "SetDesignRules", max_component_height: null, clear_max_component_height: true });
  });
  it("an empty max component height with no stored limit is not a clear", () => {
    setup();
    type("Min clearance (mm)", "0.2");
    fireEvent.click(screen.getByRole("button", { name: "Apply board rules" }));
    const c = (sent[0] as Command[])[0] as Record<string, unknown>;
    expect(c.max_component_height).toBeNull();
    expect(c.clear_max_component_height).toBeUndefined();
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

// Phase 1 B5/C5 (ADR-0046): rule tree, clearance matrix, net-to-net clearance and the ERC / DRC
// rule tables. Each Apply is one typed command (a Batch when several rows change).
describe("rule tables", () => {
  const erc: Report = {
    ...ercResult(7).report,
    rules: [
      { id: "ERC-001", description: "Unconnected pin", parameters: "", source: "EAK" },
      { id: "ERC-009", description: "Net note", parameters: "", source: "EAK" },
    ],
    rule_results: [
      { rule_id: "ERC-001", state: "FAIL", reason: "" },
      { rule_id: "ERC-009", state: "WARNING", reason: "" },
    ],
  };
  const open = (reports?: { erc?: Report; drc?: Report }, onRun?: (c: "erc" | "drc") => void) => {
    sent = [];
    render(<RulesDialog snapshot={withRules()} busy={false} apply={async (c) => void sent.push(c)} onClose={() => {}} reports={reports} onRun={onRun} />);
  };

  it("lists the rule tree groups", () => {
    open();
    const tree = within(screen.getByRole("navigation", { name: "Rule tree" }));
    for (const g of ["Board rules", "Net classes", "Net-to-net clearance", "Clearance matrix", "ERC rules", "DRC rules"]) expect(tree.getByRole("button", { name: g })).toBeTruthy();
  });

  it("applies changed clearance-matrix cells as SetClearanceCell (mm to nm, unordered pair)", () => {
    open();
    type("Track to Via clearance (mm)", "0.25");
    fireEvent.click(screen.getByRole("button", { name: "Apply clearance matrix" }));
    expect(sent).toEqual([[{ type: "SetClearanceCell", a: "track", b: "via", clearance: 250_000 }]]);
  });

  it("refuses a non-positive matrix value without sending anything", () => {
    open();
    type("Track to Track clearance (mm)", "-1");
    fireEvent.click(screen.getByRole("button", { name: "Apply clearance matrix" }));
    expect(sent).toHaveLength(0);
    expect(screen.getByRole("alert").textContent).toContain("positive");
  });

  it("matrix cells no DRC rule checks are marked; checked ones name their rule", () => {
    expect(matrixCellRule("via", "track")).toBe("DRC-002");
    expect(matrixCellRule("hole", "th_pad")).toBe("DRC-018");
    expect(matrixCellRule("silk", "outline")).toBe("DRC-020");
    expect(matrixCellRule("zone", "track")).toBeNull();
    open();
    expect(screen.getByLabelText("Track to Zone clearance (mm)").getAttribute("title")).toContain("UNKNOWN");
  });

  it("applies a net-to-net clearance", () => {
    open();
    const nets = netNames(withRules());
    fireEvent.change(screen.getByLabelText("First net"), { target: { value: nets[0] } });
    fireEvent.change(screen.getByLabelText("Second net"), { target: { value: nets[1] } });
    type("Net pair clearance (mm)", "0.5");
    fireEvent.click(screen.getByRole("button", { name: "Apply net pair rule" }));
    expect(sent).toEqual([[{ type: "SetNetPairClearance", net_a: nets[0], net_b: nets[1], clearance: 500_000 }]]);
  });

  it("the ERC rule table disables a rule and overrides another as one Batch", () => {
    open({ erc });
    const table = within(screen.getByRole("table", { name: "ERC rule table" }));
    fireEvent.click(table.getByLabelText("Enable ERC-001"));
    expect((table.getByLabelText("Severity of ERC-001") as HTMLSelectElement).disabled).toBe(true);
    fireEvent.change(table.getByLabelText("Severity of ERC-009"), { target: { value: "info" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply ERC rule configuration" }));
    expect(sent).toEqual([
      [
        {
          type: "Batch",
          label: "ERC rule configuration",
          commands: [
            { type: "SetRuleConfig", rule_id: "ERC-001", config: { enabled: false, severity: null } },
            { type: "SetRuleConfig", rule_id: "ERC-009", config: { enabled: true, severity: "info" } },
          ],
        },
      ],
    ]);
  });

  it("restoring a rule to enabled at its default severity clears its configuration", () => {
    expect(ruleCommands({ "ERC-001": { enabled: false } }, { "ERC-001": { enabled: true, severity: "" } })).toEqual([{ type: "SetRuleConfig", rule_id: "ERC-001", config: null }]);
    expect(ruleCommands({ "ERC-001": { enabled: false } }, { "ERC-001": { enabled: false, severity: "" } })).toEqual([]);
  });

  it("without a report the DRC rule table offers to run DRC", () => {
    const onRun = vi.fn();
    open({}, onRun);
    fireEvent.click(within(screen.getByRole("region", { name: "DRC rules" })).getByRole("button", { name: "Run DRC" }));
    expect(onRun).toHaveBeenCalledWith("drc");
  });
});

// Annotate options (Phase 1 B5, ADR-0046): order, scope and reset as one typed Annotate command.
describe("Annotate options dialog", () => {
  const open = (selected?: string) => {
    const got: Command[] = [];
    const onClose = vi.fn();
    render(<AnnotateDialog busy={false} selected={selected} apply={async (c) => void got.push(c)} onClose={onClose} />);
    return { got, onClose };
  };

  it("sends the default Annotate (unannotated, across) and closes", async () => {
    const { got, onClose } = open();
    fireEvent.click(screen.getByRole("button", { name: "Annotate" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(got).toEqual([{ type: "Annotate", order: "across", scope: "unannotated", symbols: [], reset: false }]);
  });

  it("all symbols, down", async () => {
    const { got } = open();
    fireEvent.change(screen.getByLabelText("Annotate scope"), { target: { value: "all" } });
    fireEvent.change(screen.getByLabelText("Annotate order"), { target: { value: "down" } });
    fireEvent.click(screen.getByRole("button", { name: "Annotate" }));
    await waitFor(() => expect(got).toEqual([{ type: "Annotate", order: "down", scope: "all", symbols: [], reset: false }]));
  });

  it("resets only the selected symbol", async () => {
    const { got } = open("sym_1");
    fireEvent.change(screen.getByLabelText("Annotate scope"), { target: { value: "selection" } });
    fireEvent.click(screen.getByLabelText("Reset designators"));
    fireEvent.click(screen.getByRole("button", { name: "Reset designators" }));
    await waitFor(() => expect(got).toEqual([{ type: "Annotate", order: "across", scope: "selection", symbols: ["sym_1"], reset: true }]));
  });

  it("the selection scope without a selected symbol sends nothing", () => {
    expect(annotateCommand("across", "selection", false)).toBeNull();
    open();
    expect((screen.getByRole("option", { name: "Selected symbol" }) as HTMLOptionElement).disabled).toBe(true);
  });

  it("stays open with a reason when the engine refuses the command or is busy (S18)", async () => {
    const onClose = vi.fn();
    render(<AnnotateDialog busy={false} apply={async () => false} onClose={onClose} />);
    fireEvent.click(screen.getByRole("button", { name: "Annotate" }));
    expect((await screen.findByRole("alert")).textContent).toMatch(/Not applied/);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("a busy Annotate button says why it is disabled", () => {
    render(<AnnotateDialog busy={true} apply={async () => true} onClose={vi.fn()} />);
    const b = screen.getByRole("button", { name: "Annotate" });
    expect((b as HTMLButtonElement).disabled).toBe(true);
    expect(b.getAttribute("title")).toMatch(/Another command is running/);
  });
});
