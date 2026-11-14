// End-to-end flow through the real UI against a small stateful fake backend:
// new project -> schematic -> Update PCB from schematic -> PCB -> ERC/DRC -> export dialog ->
// save / close / reopen. One test per step; each asserts the typed backend call it makes.
import { cleanup, configure, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { fixture } from "./fixture.test-data";
import { forgetViewports } from "./shell/viewState";
import type { CommandResponse, ExportResult, Snapshot } from "./types";

configure({ asyncUtilTimeout: 6000 }); // the shared laptop is slow; these are flow tests, not latency tests
let calls: { cmd: string; args: Record<string, unknown> }[];
let proj: Snapshot | null;
let failOpen = false;
const LIB = [{ id: "R_0603", name: "Resistor 0603", category: "passive", refdes_prefix: "R", default_value: "10k", pins: [["1", "1"], ["2", "2"]] }];

function blank(name: string, withOutline: boolean): Snapshot {
  const s = fixture(0);
  s.project.id = `prj_${name}`;
  s.project.name = name;
  s.project.schematic.symbols = {};
  s.project.schematic.wires = {};
  s.project.schematic.labels = {};
  s.project.pcb.footprints = {};
  s.project.pcb.tracks = {};
  s.project.pcb.vias = {};
  s.project.pcb.zones = {};
  if (!withOutline) s.project.pcb.board.outline = [];
  s.geometry = { symbols: [], footprints: [] };
  s.can_undo = false;
  return s;
}
const resp = (command: string, before: number): CommandResponse => ({ op_id: "o", command, class: "reversible_local", accepted: true, revision_before: before, revision_after: before + 1, created: [], affected: [], warnings: [], error: null, validation_note: "" });
const exportResult = (rev: number): ExportResult => ({
  out_dir: "/p/output/rev-" + rev,
  manifest: { revision: rev, status: "DRAFT", statement: "DRAFT package", blocking_reasons: ["DRC has errors"], files: [{ name: "board.gtl", kind: "gerber", bytes: 10, sha256: "ab" }], not_verified: ["fit"] },
  reports: [],
});

beforeEach(() => {
  localStorage.clear();
  forgetViewports();
  calls = [];
  proj = null;
  failOpen = false;
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    switch (cmd) {
      case "recent_projects":
        return proj ? [{ path: "/p/demo.eak", project_id: proj.project.id, name: proj.project.name, revision: proj.project.revision, last_opened_ms: 1 }] : [];
      case "create_project":
        proj = blank(String(args.name), false);
        return proj;
      case "open_project":
        if (failOpen) {
          proj = null;
          throw new Error("project is locked by another process");
        }
        return proj ?? blank("Reopened", true);
      case "current_project":
        if (!proj) throw new Error("no project is open");
        return proj;
      case "close_project":
        return null;
      case "events_since":
      case "model_providers":
        return [];
      case "library_components":
        return LIB;
      case "library_search":
        return LIB.map((c) => ({
          id: c.id,
          source: "builtin",
          in_project: false,
          unverified_ratings: 0,
          def: { ...c, manufacturer: null, mpn: null, lcsc: null, datasheet: null, symbol: { pins: [], body_half_w: 1_000_000, body_half_h: 1_000_000 }, footprint: { name: "R_0603", pads: [], courtyard_half_w: 1_000_000, courtyard_half_h: 1_000_000, body_height: null }, pin_map: {}, ratings: [], provenance: "test" },
        }));
      case "library_footprint_candidates":
        return [];
      case "connectivity":
        return { revision: proj?.project.revision ?? 0, net_groups: {}, shorts: [], ratsnest: [] };
      case "execute_command": {
        const c = args.command as { type: string; component_id?: string };
        const p = proj!;
        const before = p.project.revision;
        p.project.revision += 1;
        p.can_undo = true;
        if (c.type === "PlaceSymbol") {
          const n = Object.keys(p.project.schematic.symbols).length + 1;
          const id = `sym_${n}`;
          p.project.schematic.symbols[id] = { id, sheet_id: "sht_1", component_id: c.component_id!, refdes: `R${n}`, value: "10k", position: { x: n * 10_000_000, y: 50_000_000 }, rotation: "R0" };
          p.geometry.symbols.push({ id, sheet_id: "sht_1", refdes: `R${n}`, value: "10k", position: { x: n * 10_000_000, y: 50_000_000 }, half_w: 2_000_000, half_h: 1_000_000, pins: [] });
        }
        if (c.type === "SyncSchematicToPcb")
          for (const sy of Object.values(p.project.schematic.symbols)) {
            const fid = `fp_${sy.id}`;
            p.project.pcb.footprints[fid] = { id: fid, symbol_id: sy.id, component_id: sy.component_id, refdes: sy.refdes, position: { x: 50_000_000, y: 0 }, rotation: "R0", side: "top", locked: false };
            p.geometry.footprints.push({ id: fid, symbol_id: sy.id, refdes: sy.refdes, side: "top", position: { x: 50_000_000, y: 0 }, half_w: 1_500_000, half_h: 800_000, pads: [] });
          }
        return resp(c.type, before);
      }
      case "run_check":
        return { report: { check: String(args.check).toUpperCase(), project_id: proj!.project.id, revision: proj!.project.revision, input_sha256: "x", tool: "t", findings: [], outcome: "NO_ERRORS_IN_IMPLEMENTED_CHECKS", errors: 0, warnings: 0, not_implemented: ["bus checks"], limitations: [] }, evidence_path: null };
      case "export_manufacturing":
        return exportResult(proj!.project.revision);
      default:
        throw new Error(`unmocked ${cmd}`);
    }
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
});

const toolbar = () => within(screen.getByRole("toolbar", { name: "Main toolbar" }));
const menubar = () => within(screen.getByRole("navigation", { name: "Application menu" }));
const ran = (cmd: string) => calls.filter((c) => c.cmd === cmd);
const execCalls = () => calls.filter((c) => c.cmd === "execute_command" || c.cmd === "execute_assistant_command");
const executed = () => execCalls().map((c) => (c.args.command as { type: string }).type);

async function createFromHome(name = "Flow") {
  render(<App />);
  fireEvent.change(await screen.findByLabelText(/New project folder/), { target: { value: "/p/demo.eak" } });
  fireEvent.change(screen.getByLabelText("Name"), { target: { value: name } });
  fireEvent.click(screen.getByRole("button", { name: "Create" }));
  await screen.findByRole("img", { name: "Schematic canvas" });
}
async function placeResistor() {
  // the assistant resolves part names against the loaded library: wait until it is there
  fireEvent.click(screen.getByRole("button", { name: "Library / Parts" }));
  await screen.findByText("Resistor 0603");
  const box = screen.getByRole("textbox", { name: "Ask the assistant" });
  fireEvent.change(box, { target: { value: "add resistor" } });
  fireEvent.keyDown(box, { key: "Enter" });
  fireEvent.click(await screen.findByRole("button", { name: "Accept" }));
  await waitFor(() => expect(executed()).toContain("PlaceSymbol"));
}

describe("end-to-end flow", () => {
  it("1. new project dialog replaces the open project and lands in its schematic tab", async () => {
    await createFromHome("First");
    fireEvent.click(toolbar().getByRole("button", { name: "New project" }));
    const dlg = await screen.findByRole("dialog", { name: "New project" });
    fireEvent.change(within(dlg).getByLabelText(/Project folder/), { target: { value: "/p/second.eak" } });
    fireEvent.change(within(dlg).getByLabelText("Name"), { target: { value: "Second" } });
    fireEvent.click(within(dlg).getByRole("button", { name: "Create" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "New project" })).toBeNull());
    expect(ran("create_project").at(-1)!.args).toEqual({ path: "/p/second.eak", name: "Second" });
    await screen.findByRole("img", { name: "Schematic canvas" });
    expect(toolbar().getByText("Second")).toBeTruthy();
  });

  it("2. a part is placed in the schematic through a typed command", async () => {
    await createFromHome();
    await placeResistor();
    expect(execCalls()[0].args.command).toMatchObject({ type: "PlaceSymbol", component_id: "R_0603" });
    await waitFor(() => expect(screen.getAllByText(/R1/).length).toBeGreaterThan(0));
  });

  it("3. Update PCB from schematic runs SyncSchematicToPcb and shows the PCB with the footprints", async () => {
    await createFromHome();
    await placeResistor();
    fireEvent.click(screen.getByRole("button", { name: "Close PCB" }));
    await waitFor(() => expect(screen.queryByRole("img", { name: "PCB canvas" })).toBeNull());
    ran("connectivity").length = 0;
    calls.length = 0;
    fireEvent.click(toolbar().getByRole("button", { name: "Update PCB" }));
    await waitFor(() => expect(executed()).toEqual(["SyncSchematicToPcb"]));
    await screen.findByRole("img", { name: "PCB canvas" }); // tab reopened and shown
    await waitFor(() => expect(document.querySelector("[data-footprint-id]")).toBeTruthy());
    expect(screen.getByRole("status").textContent).toContain("Footprints are staged");
    await waitFor(() => expect(ran("connectivity").length).toBeGreaterThan(0)); // airwires re-read for the new revision
  });

  it("4. ERC and DRC run from the toolbar and show in the Findings tab", async () => {
    await createFromHome();
    fireEvent.click(toolbar().getByRole("button", { name: "Run ERC" }));
    await waitFor(() => expect(ran("run_check")).toHaveLength(1));
    await waitFor(() => expect((toolbar().getByRole("button", { name: "Run DRC" }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(toolbar().getByRole("button", { name: "Run DRC" }));
    await waitFor(() => expect(ran("run_check").map((c) => c.args.check)).toEqual(["erc", "drc"]));
    await screen.findByText(/DRC · revision/);
    expect(screen.getAllByText(/No errors in implemented checks/).length).toBeGreaterThan(0);
  });

  it("5a. the export dialog refuses to run until the board has an outline, and says so", async () => {
    await createFromHome();
    fireEvent.click(toolbar().getByRole("button", { name: "Export manufacturing files" }));
    const dlg = await screen.findByRole("dialog", { name: "Export manufacturing files" });
    expect(within(dlg).getByRole("alert").textContent).toContain("no outline");
    expect((within(dlg).getByRole("button", { name: "Export" }) as HTMLButtonElement).disabled).toBe(true);
    expect(ran("export_manufacturing")).toHaveLength(0);
  });

  it("5b. with an outline the dialog exports and shows the DRAFT manifest, never 'ready'", async () => {
    proj = blank("Outlined", true);
    render(<App />);
    fireEvent.click(await screen.findByText("Outlined"));
    await screen.findByRole("img", { name: "Schematic canvas" });
    fireEvent.click(toolbar().getByRole("button", { name: "Export manufacturing files" }));
    const dlg = await screen.findByRole("dialog", { name: "Export manufacturing files" });
    fireEvent.click(within(dlg).getByRole("button", { name: "Export" }));
    await within(dlg).findByText(/DRAFT — not for fabrication/);
    expect(within(dlg).getByText(/board\.gtl/)).toBeTruthy();
    expect(dlg.textContent).not.toMatch(/is manufacturing-ready|ready for fabrication|release candidate/i);
    expect(ran("export_manufacturing")).toHaveLength(1);
  });

  it("6. File menu: Save asks the engine, Close returns Home, the project reopens from Recent", async () => {
    await createFromHome("Persist");
    await placeResistor();
    calls.length = 0;
    fireEvent.click(menubar().getByRole("button", { name: "File" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /^Save(?! as)/ }));
    await waitFor(() => expect(ran("current_project")).toHaveLength(1));
    fireEvent.click(menubar().getByRole("button", { name: "File" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Close project" }));
    await screen.findByRole("heading", { name: "Recent projects" });
    fireEvent.click(await screen.findByText("Persist"));
    await screen.findByRole("img", { name: "Schematic canvas" });
    expect(ran("open_project").at(-1)!.args).toEqual({ path: "/p/demo.eak", take_over_lock: false });
    expect(Object.keys(proj!.project.schematic.symbols)).toHaveLength(1); // unchanged after reopen
  });

  it("7. a failed replace that drops the old project returns to Project Home, not a stale workspace", async () => {
    await createFromHome("Doomed");
    failOpen = true;
    fireEvent.click(toolbar().getByRole("button", { name: "Open project" }));
    const dlg = await screen.findByRole("dialog", { name: "Open project" });
    fireEvent.change(within(dlg).getByLabelText(/Project folder/), { target: { value: "/p/locked.eak" } });
    fireEvent.click(within(dlg).getByRole("button", { name: "Open" }));
    await screen.findByRole("heading", { name: "Recent projects" });
  });
});
