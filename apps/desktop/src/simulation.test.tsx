// Simulation panel: engineering-notation parsing, the clear "ngspice missing" state, running
// an analysis through IPC, and the node-voltage table / transient plot.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { fmtEng, parseEng, SimulationPanel } from "./SimulationPanel";
import type { SimResult, SimulationStatus } from "./types";

afterEach(() => {
  cleanup();
  clearMocks();
});

const status = (available: boolean): SimulationStatus => ({
  ngspice: { available, path: available ? "/usr/bin/ngspice" : null, version: available ? "42" : null, message: available ? null : "ngspice is not installed or not on PATH." },
  revision: 3,
  nets: ["GND", "OUT", "VIN"],
  elements: [{ refdes: "R1", kind: "resistor", value: "1k", line: "R1 n_vin n_out 1e3" }],
  skipped: [{ refdes: "J1", reason: "no SPICE model for this part" }],
});

const result: SimResult = {
  analysis: { kind: "tran", stop: 0.005, step: 1e-5 },
  ngspice_version: "42",
  nodes: [{ net: "OUT", volts: 4.97 }, { net: "VIN", volts: 5 }],
  currents: [{ element: "V1", amps: -0.001 }],
  tran: { time: [0, 0.0025, 0.005], traces: [{ net: "OUT", volts: [0, 4.1, 4.97] }], points_total: 3, decimated: false },
  warnings: [],
  limitations: ["Computed by an external ngspice."],
  netlist: "R1 n_vin n_out 1e3",
  elapsed_ms: 12,
};

describe("engineering notation", () => {
  it("parses SPICE-style values and rejects junk", () => {
    expect(parseEng("10k")).toBe(10000);
    expect(parseEng("4.7u")).toBeCloseTo(4.7e-6);
    expect(parseEng("5m")).toBeCloseTo(5e-3);
    expect(parseEng("1meg")).toBe(1e6);
    expect(parseEng("1e-3")).toBeCloseTo(1e-3);
    expect(parseEng("abc")).toBeNull();
    expect(fmtEng(0.0025, "V")).toBe("2.5 mV");
    expect(fmtEng(4.97, "V")).toBe("4.97 V");
  });
});

describe("SimulationPanel", () => {
  it("shows a clear error and disables Run when ngspice is not installed", async () => {
    mockIPC((cmd) => (cmd === "simulation_status" ? status(false) : undefined));
    render(<SimulationPanel revision={3} />);
    expect((await screen.findByRole("alert")).textContent).toContain("ngspice is not installed");
    expect((screen.getByRole("button", { name: "Run simulation" }) as HTMLButtonElement).disabled).toBe(true);
  });

  // S17: the disabled Run and "+ Add" buttons say why.
  it("gives every disabled button a reason (no ngspice, no nets)", async () => {
    mockIPC((cmd) => (cmd === "simulation_status" ? { ...status(false), nets: [] } : undefined));
    render(<SimulationPanel revision={3} />);
    await screen.findByRole("alert");
    const run = screen.getByRole("button", { name: "Run simulation" }) as HTMLButtonElement;
    expect(run.title).toMatch(/ngspice is not installed/);
    const add = screen.getByRole("button", { name: "+ Add" }) as HTMLButtonElement;
    expect(add.disabled).toBe(true);
    expect(add.title).toMatch(/no net/);
  });

  it("runs a transient analysis and shows node voltages and a plot", async () => {
    let request: unknown;
    mockIPC((cmd, payload) => {
      if (cmd === "simulation_status") return status(true);
      if (cmd === "run_simulation") {
        request = (payload as { request: unknown }).request;
        return result;
      }
    });
    render(<SimulationPanel revision={3} />);
    await screen.findByText(/ngspice 42/);
    expect(screen.getByText(/J1 \(no SPICE model/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "+ Add" }));
    fireEvent.change(screen.getByLabelText("Analysis"), { target: { value: "tran" } });
    fireEvent.click(screen.getByRole("button", { name: "Run simulation" }));
    await waitFor(() => expect(screen.getByRole("table", { name: "Node voltages" })).toBeTruthy());
    expect(request).toMatchObject({
      analysis: { kind: "tran", stop: 0.005, step: 1e-5 },
      sources: [{ name: "V1", kind: "voltage", positive_net: "OUT", negative_net: "GND", spec: "5" }],
    });
    expect(screen.getAllByText("4.97 V").length).toBeGreaterThan(0);
    expect(screen.getByRole("img", { name: "Transient plot" })).toBeTruthy();
    expect(screen.getByText(/Not verified: Computed by an external ngspice/)).toBeTruthy();
  });

  it("rejects an invalid transient before calling the engine", async () => {
    mockIPC((cmd) => (cmd === "simulation_status" ? status(true) : undefined));
    render(<SimulationPanel revision={3} />);
    await screen.findByText(/ngspice 42/);
    fireEvent.change(screen.getByLabelText("Analysis"), { target: { value: "tran" } });
    fireEvent.change(screen.getByLabelText("Step time"), { target: { value: "oops" } });
    fireEvent.click(screen.getByRole("button", { name: "Run simulation" }));
    expect((await screen.findByRole("alert")).textContent).toContain("0 < step");
  });
});
