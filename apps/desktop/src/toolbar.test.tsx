// Main toolbar and per-tab viewport persistence.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { shellCmd } from "./shell.test-data";
import { fixture } from "./fixture.test-data";
import { forgetViewports, loadViewport, sanitizeViewBox, saveViewport } from "./shell/viewState";
import type { CommandResponse } from "./types";

describe("viewport store", () => {
  it("rejects malformed viewports and round-trips a good one", () => {
    expect(sanitizeViewBox({ x: 1, y: 2, w: 0, h: 3 })).toBeNull();
    expect(sanitizeViewBox({ x: "a" })).toBeNull();
    forgetViewports();
    localStorage.clear();
    saveViewport("sch:x", { x: 1, y: 2, w: 3, h: 4 });
    expect(loadViewport("sch:x")).toEqual({ x: 1, y: 2, w: 3, h: 4 });
    forgetViewports();
    localStorage.setItem("eak.viewport.sch:y", "garbage");
    expect(loadViewport("sch:y")).toBeNull();
  });
});

let revision: number;
let executed: unknown[];
let calls: string[];
const resp = (): CommandResponse => ({ op_id: "o", command: "x", class: "reversible_local", accepted: true, revision_before: revision, revision_after: revision + 1, created: [], affected: [], warnings: [], error: null, validation_note: "" });
beforeEach(() => {
  localStorage.clear();
  forgetViewports();
  revision = 7;
  executed = [];
  calls = [];
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push(cmd);
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    switch (cmd) {
      case "recent_projects":
        return [{ path: "/p.eak", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
      case "open_project":
      case "current_project":
        return fixture(revision);
      case "events_since":
        return [];
      case "execute_command":
        executed.push(args.command);
        revision += 1;
        return resp();
      case "simulation_status":
        return { ngspice: { available: false, path: null, version: null, message: "ngspice is not installed or not on PATH." }, revision, nets: [], elements: [], skipped: [] };
      case "manufacturing_preview":
        throw new Error("no preview in this test");
      case "library_components":
      case "model_providers":
        return [];
      default:
        { const h = shellCmd(cmd, null); if (h.handled) return h.value; { const h = shellCmd(cmd, null); if (h.handled) return h.value; throw new Error(`unmocked ${cmd}`); } }
    }
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
  vi.restoreAllMocks();
});

async function open() {
  render(<App />);
  fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
  await screen.findByRole("img", { name: "Schematic canvas" });
}
const tb = () => within(screen.getByRole("toolbar", { name: "Main toolbar" }));
const viewBox = () => screen.getByRole("img", { name: "Schematic canvas" }).getAttribute("viewBox");

describe("main toolbar", () => {
  it("slice E1: no permanently disabled placeholder is left on the toolbar; state-dependent controls say why", async () => {
    await open();
    for (const name of ["Save as", "Junction", "Grid"]) expect(tb().queryByRole("button", { name }), name).toBeNull();
    for (const b of tb().getAllByRole("button") as HTMLButtonElement[]) if (b.disabled) expect(b.title, b.getAttribute("aria-label") ?? "").not.toMatch(/not implemented/i);
    // selection-dependent controls explain themselves too
    expect((tb().getByRole("button", { name: "Rotate selection" }) as HTMLButtonElement).title).toContain("Select a part first");
  });

  it("slice A2: the grid placeholder is gone; the View group holds the active editor's grid, unit and snap combos", async () => {
    await open();
    expect(tb().queryByRole("button", { name: "Grid" })).toBeNull();
    const view = within(tb().getByRole("group", { name: "View" }));
    expect(view.getByRole("group", { name: "Schematic grid" })).toBeTruthy();
    fireEvent.change(view.getByLabelText("Unit"), { target: { value: "mil" } });
    expect((view.getByLabelText("Grid spacing") as HTMLSelectElement).value).toBe("1.27");
    expect([...(view.getByLabelText("Grid spacing") as HTMLSelectElement).options].map((o) => o.text)).toEqual(["10 mil", "25 mil", "50 mil", "100 mil"]);
    expect(JSON.parse(localStorage.getItem("eak.grid.schematic")!)).toMatchObject({ unit: "mil", spacingMm: 1.27 });
    expect(screen.getByRole("status").textContent).toContain("Grid 50 mil");
    // the PCB keeps its own grid
    fireEvent.click(screen.getByRole("button", { name: "PCB" }));
    await waitFor(() => expect(within(tb().getByRole("group", { name: "View" })).getByRole("group", { name: "PCB grid" })).toBeTruthy());
    expect((within(tb().getByRole("group", { name: "View" })).getByLabelText("Unit") as HTMLSelectElement).value).toBe("mm");
    // every toolbar button carries an EAK icon
    for (const b of tb().getAllByRole("button").filter((x) => x.classList.contains("tb-btn"))) expect(b.querySelector("svg[data-icon]"), b.getAttribute("aria-label") ?? "").not.toBeNull();
  });

  it("slice A2: View > Toolbar labels shows text beside the icons and is remembered", async () => {
    await open();
    expect(tb().getByRole("button", { name: "Save" }).textContent).toBe("");
    fireEvent.click(within(screen.getByRole("navigation", { name: "Application menu" })).getByRole("button", { name: "View" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Toolbar labels" }));
    await waitFor(() => expect(tb().getByRole("button", { name: "Save" }).textContent).toBe("Save"));
    expect(localStorage.getItem("eak.toolbar.labels")).toBe("1");
  });

  it("every enabled toolbar button does something: engine command, IPC call or UI state", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: "PCB" })); // Auto-route is a PCB-view button
    fireEvent.click(tb().getByRole("button", { name: "Auto-route" }));
    await waitFor(() => expect(executed).toEqual([{ type: "AutoRoute", nets: null }]));
    fireEvent.click(screen.getByRole("button", { name: "Schematic" }));
    calls.length = 0;
    fireEvent.click(tb().getByRole("button", { name: "Save" }));
    await waitFor(() => expect(calls).toContain("current_project"));
    expect(screen.getByRole("status").textContent).toContain("already written");
    const before = viewBox();
    fireEvent.click(tb().getByRole("button", { name: "Zoom in" }));
    await waitFor(() => expect(viewBox()).not.toBe(before));
    fireEvent.click(tb().getByRole("button", { name: "Fit to view" }));
    await waitFor(() => expect(viewBox()).toBe(before));
  });

  it("Move tool arms the active editor's move tool; Rotate and Delete act on the selection through typed commands", async () => {
    await open();
    const sch = within(screen.getByRole("region", { name: "Schematic view" }));
    // The editor registers its tool handler in an effect; clicking before that arms nothing.
    // Clicking is idempotent, so retry it until the editor reports the tool as armed.
    await waitFor(() => {
      fireEvent.click(tb().getByRole("button", { name: "Move tool" }));
      expect(sch.getByRole("button", { name: "Move" }).getAttribute("aria-pressed")).toBe("true");
    });
    // The part list is the design manager's Components tab (Phase 1 A3).
    fireEvent.click(screen.getByRole("tab", { name: "Components" }));
    fireEvent.click(screen.getByRole("button", { name: /R1 330/ }));
    fireEvent.click(tb().getByRole("button", { name: "Rotate selection" }));
    await waitFor(() => expect(executed).toEqual([{ type: "RotateSymbol", symbol_id: "sym_r1", rotation: "R90" }]));
    // Delete's state comes from the editor's published operations, one render after the engine is idle.
    await waitFor(() => expect((tb().getByRole("button", { name: "Delete selection" }) as HTMLButtonElement).disabled).toBe(false));
    vi.spyOn(window, "confirm").mockReturnValueOnce(false).mockReturnValueOnce(true);
    fireEvent.click(tb().getByRole("button", { name: "Delete selection" }));
    expect(executed).toHaveLength(1);
    fireEvent.click(tb().getByRole("button", { name: "Delete selection" }));
    await waitFor(() => expect(executed[1]).toEqual({ type: "DeleteSymbol", symbol_id: "sym_r1" }));
  });
});

describe("per-tab viewport", () => {
  it("a closed tab reopens at its last zoom, not at fit", async () => {
    await open();
    const fitted = viewBox();
    fireEvent.click(tb().getByRole("button", { name: "Zoom in" }));
    await waitFor(() => expect(viewBox()).not.toBe(fitted));
    const zoomed = viewBox();
    fireEvent.click(screen.getByRole("button", { name: "Close Sheet 1" }));
    await waitFor(() => expect(screen.queryByRole("img", { name: "Schematic canvas" })).toBeNull());
    fireEvent.click(within(screen.getByRole("navigation", { name: "Application menu" })).getByRole("button", { name: "Project" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Open sheet Sheet 1" }));
    await screen.findByRole("img", { name: "Schematic canvas" });
    expect(viewBox()).toBe(zoomed);
  });
});

describe("keyboard scope (evaluator C4)", () => {
  it("C4: in split view a keypress acts only in the active editor; clicking the other editor hands it the keyboard", async () => {
    await open();
    const sch = within(screen.getByRole("region", { name: "Schematic view" }));
    const pcb = within(screen.getByRole("region", { name: "PCB view" }));
    const pressed = (v: typeof sch, name: string) => v.getByRole("button", { name }).getAttribute("aria-pressed");
    await waitFor(() => {
      fireEvent.keyDown(window, { key: "3" }); // schematic: Wire, PCB: Move
      expect(pressed(sch, "Wire")).toBe("true");
    });
    expect(pressed(pcb, "Move")).toBe("false");
    fireEvent.pointerDown(screen.getByRole("region", { name: "PCB view" }));
    await waitFor(() => {
      fireEvent.keyDown(window, { key: "6" }); // PCB: Delete, schematic: Move
      expect(pressed(pcb, "Delete")).toBe("true");
    });
    expect(pressed(sch, "Move")).toBe("false");
    expect(pressed(sch, "Wire")).toBe("true"); // untouched
  });
});

describe("design rules entry point", () => {
  it("the toolbar Design rules button opens the dialog", async () => {
    await open();
    fireEvent.click(tb().getByRole("button", { name: "Design rules" }));
    expect(await screen.findByRole("dialog", { name: "Design rules" })).toBeTruthy();
  });
});

// @ts-expect-error node types are not installed for the frontend project
const { readFileSync } = await import("node:fs");
const css: string = readFileSync("src/styles.css", "utf8");
describe("desktop layout fixes (E2E defects F1-F3)", () => {
  const rule = (sel: string) => css.split("\n").find((l: string) => l.startsWith(`${sel} {`)) ?? "";

  it("Simulate opens the Simulation panel, same as the Simulation menu", async () => {
    await open();
    const b = tb().getByRole("button", { name: "Simulate" }) as HTMLButtonElement;
    expect(b.disabled).toBe(false);
    fireEvent.click(b);
    expect((await screen.findByRole("tab", { name: "Simulation" })).getAttribute("aria-selected")).toBe("true");
    await screen.findByRole("button", { name: "Run simulation" });
  });

  it("split view: rows are bounded so the PCB view cannot paint over the bottom panel", () => {
    for (const sel of [".main", ".views"]) expect(rule(sel)).toContain("grid-template-rows: minmax(0, 1fr)");
    expect(rule(".views")).toContain("overflow: hidden");
    expect(rule(".view")).toContain("overflow: hidden");
    expect(rule(".view")).toContain("minmax(0, 1fr)");
    expect(rule(".center")).toContain("overflow: hidden");
  });

  it("Manufacturing panel content scrolls inside the side panel", async () => {
    await open();
    fireEvent.click(within(screen.getByRole("navigation", { name: "Activity bar" })).getByRole("button", { name: "Manufacturing" }));
    const mfg = await screen.findByRole("region", { name: "Manufacturing prep" });
    expect(mfg.closest(".side-scroll")).not.toBeNull();
    expect(rule(".side-scroll")).toMatch(/overflow: auto/);
    expect(rule(".side-scroll")).toContain("min-height: 0");
  });
});
