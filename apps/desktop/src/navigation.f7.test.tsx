// Definition-of-done slice F7 (audit S11, S12, S13, the S24 sheet-picker drift), in the whole app
// against a mocked engine IPC on a two-sheet fixture (D1 and a second wire on Sheet 2): every
// navigation path opens the sheet the object is on (`sch:<id>`), selects it there and zooms to it.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { EngineeringMonitor } from "./EngineeringMonitor";
import { fixture } from "./fixture.test-data";
import { shellCmd } from "./shell.test-data";
import { schematicPick, schematicPlace, schematicTarget } from "./shell/designManagerData";
import { forgetViewports } from "./shell/viewState";
import type { CheckResult, Finding, Snapshot } from "./types";

const pt = (x: number, y: number) => ({ x: x * 1_000_000, y: y * 1_000_000 });

/** The fixture with a second sheet holding D1 (symbol and geometry) and a wire. */
function twoSheets(): Snapshot {
  const s = fixture(7);
  const sch = s.project.schematic;
  sch.sheets.sht_2 = { id: "sht_2", name: "Sheet 2", width: 297_000_000, height: 210_000_000 };
  sch.symbols.sym_d1.sheet_id = "sht_2";
  s.geometry.symbols.find((g) => g.id === "sym_d1")!.sheet_id = "sht_2";
  sch.wires.wir_2 = { id: "wir_2", sheet_id: "sht_2", a: pt(60, 60), b: pt(70, 60) };
  return s;
}

const erc = (findings: Finding[]): CheckResult => ({
  report: { check: "ERC", project_id: "prj_1", revision: 7, input_sha256: "ab", tool: "eak-validation", findings, outcome: "ERRORS", errors: findings.length, warnings: 0, not_implemented: [], limitations: [] },
  evidence_path: null,
});

beforeEach(() => {
  localStorage.clear();
  forgetViewports();
  mockIPC((cmd) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    switch (cmd) {
      case "recent_projects":
        return [{ path: "/p.eak", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
      case "open_project":
      case "current_project":
        return twoSheets();
      case "events_since":
      case "library_components":
      case "model_providers":
        return [];
      case "netlist":
        return [{ name: "GND", pins: ["D1.1"], label_names: ["GND"], has_power_port: true }];
      case "run_check":
        return erc([{ rule_id: "ERC-001", severity: "error", message: "D1 pin 1 is unconnected", objects: ["sym_d1"], location: null }]);
      default: {
        const h = shellCmd(cmd, null);
        if (h.handled) return h.value;
        throw new Error(`unmocked ${cmd}`);
      }
    }
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
  localStorage.clear();
});

async function open() {
  render(<App />);
  fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
  await screen.findAllByRole("img", { name: "Schematic canvas" });
  await waitFor(() => expect(sheetTab("Sheet 1")).toBe("true"));
}
const tabs = () => within(screen.getByRole("tablist", { name: "Documents" }));
const sheetTab = (name: string) => tabs().queryByRole("tab", { name: new RegExp(name) })?.getAttribute("aria-selected") ?? null;
const selected = (attr: string, id: string) => document.querySelector(`[${attr}="${id}"].selected`) !== null;
const quickOpen = async (option: RegExp) => {
  fireEvent.keyDown(window, { key: "p", ctrlKey: true });
  const dlg = await screen.findByRole("dialog", { name: "Quick open" });
  fireEvent.click(within(dlg).getAllByRole("option", { name: option })[0]);
};

describe("F7 navigation opens the sheet the object is on", () => {
  it("S12: an ERC finding on sheet 2 opens sheet 2 and selects the symbol there", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Run ERC" }));
    fireEvent.click(await screen.findByRole("button", { name: /D1 pin 1 is unconnected/ }));
    await waitFor(() => expect(sheetTab("Sheet 2")).toBe("true"));
    await waitFor(() => expect(selected("data-symbol-id", "sym_d1")).toBe(true));
  });

  it("S11: Quick open › Symbol opens the symbol's sheet and selects it", async () => {
    await open();
    await quickOpen(/^D1 Green/);
    await waitFor(() => expect(sheetTab("Sheet 2")).toBe("true"));
    await waitFor(() => expect(selected("data-symbol-id", "sym_d1")).toBe(true));
    expect(document.querySelector("footer.status")!.textContent).toContain("D1: selected and zoomed in the schematic");
  });

  it("S11: Quick open › Net goes to the net with the engine netlist instead of only printing a message", async () => {
    await open();
    await quickOpen(/^GND/);
    // GND's pin D1.1 is on sheet 2 (the netlist's pins come before the labels)
    await waitFor(() => expect(sheetTab("Sheet 2")).toBe("true"));
    expect(document.querySelector("footer.status")!.textContent).toContain("Net GND: 1 pin(s) in the engine netlist; zoomed in the schematic");
  });

  it("S13: Design manager › Objects › a wire opens its sheet and selects the wire", async () => {
    await open();
    fireEvent.click(screen.getByRole("tab", { name: "Objects" }));
    fireEvent.click(within(screen.getByRole("tabpanel", { name: "Objects" })).getByRole("button", { name: /Wire 2/ }));
    await waitFor(() => expect(sheetTab("Sheet 2")).toBe("true"));
    await waitFor(() => expect(selected("data-wire-id", "wir_2")).toBe(true));
  });

  it("S13: Design manager › Objects › a net label selects the label", async () => {
    await open();
    fireEvent.click(screen.getByRole("tab", { name: "Objects" }));
    fireEvent.click(within(screen.getByRole("tabpanel", { name: "Objects" })).getByRole("button", { name: /GND power port/ }));
    await waitFor(() => expect(selected("data-label-id", "lab_1")).toBe(true));
  });

  it("S24: the header sheet picker opens the sch:<id> document instead of switching the view under the tab", async () => {
    await open();
    const picker = document.querySelector<HTMLSelectElement>('section[aria-label="Schematic view"] select[aria-label="Sheet"]')!;
    fireEvent.change(picker, { target: { value: "sht_2" } });
    await waitFor(() => expect(sheetTab("Sheet 2")).toBe("true"));
    expect(sheetTab("Sheet 1")).toBe("false");
  });
});

describe("F7 object references give a position or say why not", () => {
  const snap = () => {
    const s = twoSheets();
    s.project.schematic.no_connects = { nc_1: { id: "nc_1", symbol_id: "sym_d1", pin: "2" }, nc_9: { id: "nc_9", symbol_id: "sym_r1", pin: "9" } };
    s.project.schematic.buses = { bus_1: { id: "bus_1", sheet_id: "sht_2", name: "D[0..1]", points: [pt(10, 10), pt(30, 10)] } };
    s.project.schematic.bus_entries = { be_1: { id: "be_1", sheet_id: "sht_2", bus_id: "bus_1", bus_point: pt(20, 10), wire_point: pt(22, 12) } };
    return s;
  };

  it("no-connects, buses and bus entries have a sheet and a box; a no-connect selects its symbol", () => {
    const s = snap();
    for (const id of ["nc_1", "bus_1", "be_1", "wir_2", "sym_d1", "fp_d1"]) expect(schematicPlace(s, schematicTarget(s, id)!, null)?.sheetId).toBe("sht_2");
    expect(schematicPick(s, schematicTarget(s, "nc_1")!)).toEqual({ kind: "symbol", id: "sym_d1" });
    expect(schematicPick(s, schematicTarget(s, "wir_2")!)).toEqual({ kind: "wire", id: "wir_2" });
    expect(schematicPick(s, schematicTarget(s, "bus_1")!)).toBeNull();
    // a no-connect on a pin the symbol does not draw has no position
    expect(schematicPlace(s, schematicTarget(s, "nc_9")!, null)).toBeNull();
    expect(schematicTarget(s, "not_an_id")).toBeNull();
  });

  it("ERC refs: placeable ones are enabled, one with no position is disabled with the reason; a bus-only finding is clickable", () => {
    const s = snap();
    const results = {
      erc: erc([
        { rule_id: "ERC-010", severity: "error", message: "pins without a connection", objects: ["nc_1", "nc_9", "be_1"], location: null },
        { rule_id: "ERC-020", severity: "error", message: "bus has no entries named", objects: ["bus_1"], location: null },
      ]),
    };
    render(<EngineeringMonitor snapshot={s} results={results} onProbe={() => {}} onNavigate={() => {}} onObject={() => {}} />);
    const refs = screen.getAllByRole("group", { name: "Objects" });
    const [nc1, nc9, be1] = within(refs[0]).getAllByRole("button") as HTMLButtonElement[];
    expect(nc1.disabled).toBe(false);
    expect(be1.disabled).toBe(false);
    expect(nc9.disabled).toBe(true);
    expect(nc9.title).toBe("No-connect has no position in the schematic to zoom to");
    expect((screen.getByRole("button", { name: /bus has no entries named/ }) as HTMLButtonElement).disabled).toBe(false);
  });
});
