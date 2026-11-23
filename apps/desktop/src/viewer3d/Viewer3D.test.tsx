// Slice C6 / E6: the 3D document's properties panel against a mocked engine. jsdom has no
// WebGL, so the canvas says so, but every property is still real: the stack-up table is the IR
// stack-up, board colour and finish are `SetManufacturing` commands, a part without a model is
// labelled as a placeholder, clicking a part selects it in the editors, and the selected part's
// model is assigned with one `UpsertComponentDefinition`.
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fixture } from "../fixture.test-data";
import type { Command, Snapshot } from "../types";
import { Viewer3D } from "./Viewer3D";

let snap: Snapshot;
beforeEach(() => {
  snap = fixture(7);
  snap.project.pcb.board.stackup = {
    board_thickness: 1_600_000,
    dielectric: "FR-4",
    layers: [
      { layer: "top", kind: "signal", copper_oz: 1, dielectric_after_um: 1510, er: 4.4 },
      { layer: "bottom", kind: "signal", copper_oz: 1, dielectric_after_um: 0, er: 4.4 },
    ],
  };
  snap.project.manufacturing = { fabricator_profile: "generic-2layer", surface_finish: "HASL", solder_mask_color: "Green", mask_expansion: 50_000 };
  // R1's definition is in the project library, without a model; D1's is not.
  (snap.project as { library?: unknown }).library = {
    R_0603: { id: "R_0603", name: "Resistor 0603", category: "passive", refdes_prefix: "R", default_value: "10k", symbol: { pins: [] }, footprint: { body_height: 500_000 }, model3d: { kind: "box", color: "#808080", external_ref: null, model_ref: null } },
  };
  mockIPC((cmd) => {
    if (cmd === "current_project") return snap;
    throw new Error(`unexpected ${cmd}`);
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
});

const panel = () => within(screen.getByRole("complementary", { name: "3D properties" }));

describe("3D document properties", () => {
  it("shows the IR stack-up, says WebGL is missing, and labels parts without a model", async () => {
    render(<Viewer3D projectId="prj_1" revision={7} active />);
    const table = await screen.findByRole("table", { name: "Stack-up layers" });
    const rows = within(table)
      .getAllByRole("row")
      .slice(1)
      .map((r) => within(r).getAllByRole("cell").map((c) => c.textContent));
    expect(rows).toEqual([
      ["F.Cu", "signal", "35", "1 oz"],
      ["Dielectric 1", "FR-4", "1510", "εr 4.4"],
      ["B.Cu", "signal", "35", "1 oz"],
    ]);
    expect(panel().getByText(/From the project stack-up: board 1.600 mm/)).toBeTruthy();
    expect(screen.getByText(/WebGL is not available/)).toBeTruthy();
    // ...and the camera controls, which cannot draw anything, are unavailable with that reason
    const top = within(screen.getByRole("toolbar", { name: "3D view" })).getByRole("button", { name: "Top" }) as HTMLButtonElement;
    expect(top.disabled).toBe(true);
    expect(top.title).toMatch(/^Unavailable: WebGL/);
    const models = within(screen.getByRole("region", { name: "3D model quality" }));
    expect(models.getAllByText(/no 3D model attached/)).toHaveLength(2);
    // nothing references a model file, so Reload is off and says why
    const reload = models.getByRole("button", { name: "Reload models" }) as HTMLButtonElement;
    expect(reload.disabled).toBe(true);
    expect(reload.title).toBe("No part references a 3D model file");
  });

  it("board colour and finish are SetManufacturing commands on the IR settings", async () => {
    const execute = vi.fn<(c: Command) => Promise<void>>(async () => undefined);
    render(<Viewer3D projectId="prj_1" revision={7} active execute={execute} />);
    const mask = (await screen.findByRole("combobox", { name: "Solder mask colour" })) as HTMLSelectElement;
    expect(mask.value).toBe("Green");
    fireEvent.change(mask, { target: { value: "Black" } });
    expect(execute).toHaveBeenLastCalledWith({ type: "SetManufacturing", config: { fabricator_profile: "generic-2layer", surface_finish: "HASL", solder_mask_color: "Black", mask_expansion: 50_000 } });
    fireEvent.change(screen.getByRole("combobox", { name: "Surface finish" }), { target: { value: "ENIG" } });
    expect(execute).toHaveBeenLastCalledWith({ type: "SetManufacturing", config: expect.objectContaining({ surface_finish: "ENIG", solder_mask_color: "Green" }) });
  });

  it("without an engine connection the board settings are unavailable with the reason", async () => {
    render(<Viewer3D projectId="prj_1" revision={7} active />);
    const mask = (await screen.findByRole("combobox", { name: "Solder mask colour" })) as HTMLSelectElement;
    expect(mask.disabled).toBe(true);
    expect(mask.title).toMatch(/^Unavailable:/);
  });

  it("clicking a part selects its symbol (cross-probe out); the selected part can be shown in the editors", async () => {
    const onSelect = vi.fn();
    const onProbe = vi.fn();
    const { rerender } = render(<Viewer3D projectId="prj_1" revision={7} active onSelect={onSelect} onProbe={onProbe} />);
    fireEvent.click(await screen.findByRole("button", { name: "R1" }));
    expect(onSelect).toHaveBeenLastCalledWith("sym_r1");
    rerender(<Viewer3D projectId="prj_1" revision={7} active onSelect={onSelect} onProbe={onProbe} selected="sym_r1" />);
    const sel = within(await screen.findByRole("region", { name: "Selected part" }));
    expect(screen.getByRole("button", { name: "R1" }).getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(sel.getByRole("button", { name: "Show in PCB" }));
    expect(onProbe).toHaveBeenLastCalledWith("pcb");
    fireEvent.click(sel.getByRole("button", { name: "Show in schematic" }));
    expect(onProbe).toHaveBeenLastCalledWith("schematic");
  });

  it("assigns a model to the selected part with one UpsertComponentDefinition, refusing an incomplete form", async () => {
    const execute = vi.fn<(c: Command) => Promise<void>>(async () => undefined);
    render(<Viewer3D projectId="prj_1" revision={7} active execute={execute} selected="sym_r1" />);
    const sel = within(await screen.findByRole("region", { name: "Selected part" }));
    const assign = sel.getByRole("button", { name: "Assign model" }) as HTMLButtonElement;
    expect(assign.disabled).toBe(true);
    expect(assign.title).toBe("Choose a model file.");
    fireEvent.change(sel.getByLabelText("Selected part 3D model path"), { target: { value: "/models/r0603.glb" } });
    expect(assign.title).toMatch(/provenance/);
    fireEvent.change(sel.getByLabelText("Selected part 3D model provenance"), { target: { value: "Vendor download" } });
    expect(assign.disabled).toBe(false);
    fireEvent.click(assign);
    expect(execute).toHaveBeenCalledTimes(1);
    const c = execute.mock.calls[0][0] as Extract<Command, { type: "UpsertComponentDefinition" }>;
    expect(c.type).toBe("UpsertComponentDefinition");
    expect(c.definition.id).toBe("R_0603");
    expect(c.definition.model3d?.model_ref).toMatchObject({ source: { kind: "file", path: "/models/r0603.glb" }, format: "glb", provenance: "Vendor download" });
  });

  it("a part whose definition is not in the project library cannot be assigned, and says why", async () => {
    render(<Viewer3D projectId="prj_1" revision={7} active execute={vi.fn()} selected="sym_d1" />);
    const sel = within(await screen.findByRole("region", { name: "Selected part" }));
    expect((sel.getByRole("button", { name: "Assign model" }) as HTMLButtonElement).title).toMatch(/^Unavailable: the part's definition is not in the project library/);
  });
});
