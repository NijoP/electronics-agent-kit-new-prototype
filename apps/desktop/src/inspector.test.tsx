// Phase 1 A5: the context properties inspector, per mode (jsdom).
//  * document: sheet size and title block (unavailable until IR v3), the board and stack-up;
//  * selection: per object kind, common fields for several, each edit ONE engine command (a
//    Batch for several objects, so one revision and one undo step);
//  * tool options: the active tool's option schema moves out of the editor's strip into the
//    inspector while it is shown, and stays in the strip when it is not.
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EditCtx } from "./EditContext";
import { fixture } from "./fixture.test-data";
import { PcbView } from "./PcbView";
import { SchematicView } from "./SchematicView";
import { Inspector } from "./shell/Inspector";
import { NO_SELECTION, documentModel, resetInspectorStore, selectionModel, type PropField } from "./shell/inspectorModel";
import { DocIdContext } from "./shell/viewState";
import type { Command, LibraryEntry, Snapshot } from "./types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: async () => null }));

const library: LibraryEntry[] = [
  { id: "R_0603", name: "Resistor 0603", category: "Resistor", refdes_prefix: "R", default_value: "10k", pins: [["1", "~"], ["2", "~"]] },
  { id: "LED_0603", name: "LED 0603", category: "LED", refdes_prefix: "D", default_value: "Green", pins: [["1", "K"], ["2", "A"]] },
];
let sent: (Command | Command[])[];
let notes: string[];
const edit = (): EditCtx => ({
  execute: async (c) => {
    sent.push(c);
  },
  busy: false,
  library,
  notify: (m) => {
    notes.push(m);
  },
});

beforeEach(() => {
  sent = [];
  notes = [];
  resetInspectorStore();
  Element.prototype.getBoundingClientRect = () => ({ x: 0, y: 0, left: 0, top: 0, right: 1000, bottom: 1000, width: 1000, height: 1000, toJSON: () => ({}) }) as DOMRect;
});
afterEach(cleanup);

const field = (fields: PropField[], id: string) => fields.find((f) => f.id === id)!;
const commit = (f: PropField, v: string) => (f.edit ? f.edit.commit(v) : undefined);

/** An editor in its document slot next to the inspector, as the workspace lays them out. */
function Both({ pcb = false, selected = null, snap = fixture() }: { pcb?: boolean; selected?: string | null; snap?: Snapshot }) {
  const ctx = edit();
  return (
    <>
      <DocIdContext.Provider value="doc">
        {pcb ? <PcbView snapshot={snap} selected={selected} onSelect={() => {}} edit={ctx} /> : <SchematicView snapshot={snap} selected={selected} onSelect={() => {}} edit={ctx} />}
      </DocIdContext.Provider>
      <Inspector snapshot={snap} docId="doc" pcb={pcb} selected={selected} edit={ctx} />
    </>
  );
}
const inspector = () => screen.getByRole("region", { name: "Properties" });
const mode = () => inspector().getAttribute("data-mode");
const tool = (name: string) => fireEvent.click(screen.getByRole("button", { name }));

describe("document mode", () => {
  it("schematic: sheet size; title block and sheet-size editing unavailable until IR v3", () => {
    const m = documentModel(fixture(), false, "sht_1");
    expect(m.title).toBe("Sheet Sheet 1");
    expect(field(m.fields, "size").value).toBe("297 × 210 mm");
    expect(field(m.fields, "title-block").unavailable).toMatch(/^Unavailable: .*B1/);
    render(<Both />);
    expect(mode()).toBe("document");
    expect(within(inspector()).getByText("297 × 210 mm")).toBeTruthy();
    // an unavailable field is text with its reason, never a control
    const tb = inspector().querySelector('[data-field="title-block"]')!;
    expect(tb.querySelector("input, select, button")).toBeNull();
    expect(tb.textContent).toMatch(/Unavailable/);
  });

  it("PCB: stack-up summary, and the copper-layer count is one SetLayerCount", () => {
    render(<Both pcb />);
    expect(mode()).toBe("document");
    expect(within(inspector()).getByText("F.Cu · B.Cu")).toBeTruthy();
    fireEvent.change(within(inspector()).getByLabelText("Copper layer count"), { target: { value: "4" } });
    expect(sent).toEqual([{ type: "SetLayerCount", count: 4, board_thickness: null }]);
  });
});

describe("selection mode", () => {
  it("one symbol: a value edit is a single command, and the strip has no second copy", () => {
    render(<Both selected="sym_r1" />);
    expect(mode()).toBe("selection");
    const box = within(inspector());
    expect(box.getByText("Component R1")).toBeTruthy();
    const v = box.getByLabelText("Value");
    fireEvent.change(v, { target: { value: "4k7" } });
    fireEvent.blur(v);
    expect(sent).toEqual([{ type: "SetSymbolValue", symbol_id: "sym_r1", value: "4k7" }]);
    expect(screen.getAllByLabelText("Value")).toHaveLength(1);
  });

  it("several symbols: common fields, mixed values, one Batch per edit", () => {
    const m = selectionModel(fixture(), { ...NO_SELECTION, symbols: ["sym_r1", "sym_d1"] })!;
    expect(m.title).toBe("2 components");
    expect(field(m.fields, "value").mixed).toBe(true);
    expect(field(m.fields, "rotation").value).toBe("R0");
    expect(field(m.fields, "refdes").unavailable).toMatch(/^Unavailable/);
    expect(commit(field(m.fields, "rotation"), "R90")).toEqual({
      type: "Batch",
      label: "Rotate 2 symbols",
      commands: [
        { type: "RotateSymbol", symbol_id: "sym_r1", rotation: "R90" },
        { type: "RotateSymbol", symbol_id: "sym_d1", rotation: "R90" },
      ],
    });
    expect(commit(field(m.fields, "x"), "12.5")).toMatchObject({
      type: "Batch",
      commands: [
        { type: "MoveSymbol", symbol_id: "sym_r1", position: { x: 12_500_000, y: 50_000_000 } },
        { type: "MoveSymbol", symbol_id: "sym_d1", position: { x: 12_500_000, y: 50_000_000 } },
      ],
    });
    expect(commit(field(m.fields, "x"), "abc")).toMatch(/not a number/);
    // an unchanged value sends nothing
    expect(commit(field(m.fields, "rotation"), "R0")).toBeNull();
  });

  it("footprints in the PCB: Ctrl+A selects all and a rotation is one Batch; locked parts are refused", () => {
    render(<Both pcb />);
    fireEvent.keyDown(window, { key: "a", ctrlKey: true });
    expect(mode()).toBe("selection");
    const box = within(inspector());
    expect(box.getByText("2 footprints")).toBeTruthy();
    fireEvent.change(box.getByLabelText("Rotation"), { target: { value: "R180" } });
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({
      type: "Batch",
      commands: [
        { type: "MoveFootprint", footprint_id: "fp_r1", rotation: "R180" },
        { type: "MoveFootprint", footprint_id: "fp_d1", rotation: "R180" },
      ],
    });
    const s = fixture();
    s.project.pcb.footprints.fp_r1.locked = true;
    const m = selectionModel(s, { ...NO_SELECTION, footprints: ["fp_r1"] })!;
    expect(commit(field(m.fields, "side"), "bottom")).toBe("1 footprint locked; unlock to move.");
    expect(commit(field(m.fields, "locked"), "no")).toEqual({ type: "SetFootprintLocked", footprint_id: "fp_r1", locked: false });
  });

  it("labels, tracks, wires and mixed kinds", () => {
    const s = fixture();
    const l = selectionModel(s, { ...NO_SELECTION, labels: ["lab_1"] })!;
    // no rename command exists: delete and place again, in one Batch (one undo step)
    expect(commit(field(l.fields, "name"), "VCC")).toEqual({
      type: "Batch",
      label: "Rename 1 label",
      commands: [
        { type: "DeleteNetLabel", label_id: "lab_1" },
        { type: "AddNetLabel", sheet_id: "sht_1", name: "VCC", position: { x: 85_000_000, y: 50_000_000 }, kind: "power" },
      ],
    });
    const t = selectionModel(s, { ...NO_SELECTION, tracks: ["trk_1"] })!;
    expect(field(t.fields, "width").value).toBe("0.250");
    expect(commit(field(t.fields, "width"), "0.4")).toMatchObject({ type: "Batch" });
    expect(commit(field(t.fields, "width"), "-1")).toMatch(/not a positive width/);
    const w = selectionModel(s, { ...NO_SELECTION, wires: ["wir_1"] })!;
    expect(field(w.fields, "style").unavailable).toMatch(/^Unavailable/);
    expect(selectionModel(s, { ...NO_SELECTION, wires: ["wir_1"], labels: ["lab_1"] })!.title).toBe("2 objects");
    expect(selectionModel(s, NO_SELECTION)).toBeNull();
  });
});

describe("tool options mode", () => {
  it("schematic Label tool: net name and kind live in the inspector and drive the tool", () => {
    render(<Both />);
    tool("Label");
    expect(mode()).toBe("tool");
    expect(within(inspector()).getByText("Label tool")).toBeTruthy();
    expect(screen.getAllByLabelText("Net name")).toHaveLength(1); // not in the strip as well
    fireEvent.change(within(inspector()).getByLabelText("Net name"), { target: { value: "VBUS" } });
    expect((within(inspector()).getByLabelText("Net name") as HTMLInputElement).value).toBe("VBUS");
    tool("Select");
    expect(mode()).toBe("document");
  });

  it("schematic Place tool offers the part and the placement rotation", () => {
    render(<Both />);
    tool("Place");
    fireEvent.change(within(inspector()).getByLabelText("Place rotation"), { target: { value: "R90" } });
    expect((within(inspector()).getByLabelText("Place rotation") as HTMLSelectElement).value).toBe("R90");
    expect(screen.getByText(/R rotate \(R90\)/)).toBeTruthy();
  });

  it("while routing the inspector shows the routing options; with no tool, the selection", () => {
    render(<Both pcb selected="sym_r1" />);
    expect(mode()).toBe("selection");
    tool("Route");
    expect(mode()).toBe("tool");
    const box = within(inspector());
    expect(box.getByLabelText("Track width")).toBeTruthy();
    expect(box.getByLabelText("Layer")).toBeTruthy();
    fireEvent.change(box.getByLabelText("Bend mode"), { target: { value: "orthogonal90" } });
    expect((within(inspector()).getByLabelText("Bend mode") as HTMLSelectElement).value).toBe("orthogonal90");
    // the strip keeps the route actions, not the options
    const view = within(screen.getByRole("region", { name: "PCB view" }));
    expect(view.queryByLabelText("Track width")).toBeNull();
    expect(view.getByRole("button", { name: /Finish route/ })).toBeTruthy();
    tool("Zone");
    expect(within(inspector()).getByLabelText("Zone clearance")).toBeTruthy();
    expect(inspector().querySelector('[data-field="Zone fill"]')!.textContent).toMatch(/Unavailable/);
    tool("Select");
    expect(mode()).toBe("selection");
    expect(sent).toEqual([]);
  });

  // ADR-0044 (schema v5): the routing options are saved with the board and come back with it.
  it("a changed routing option is saved with SetRoutingOptions after a pause, once", async () => {
    vi.useFakeTimers();
    try {
      render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
      tool("Route");
      fireEvent.change(screen.getByLabelText("Obstacle mode"), { target: { value: "push" } });
      fireEvent.change(screen.getByLabelText("Bend mode"), { target: { value: "orthogonal90" } });
      expect(sent).toEqual([]);
      await vi.advanceTimersByTimeAsync(700);
      expect(sent).toEqual([{ type: "SetRoutingOptions", options: { bend: "orthogonal90", conflict: "push", width: null, via: null, remove_loops: true } }]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("the route tool starts from the board's saved options and follows them when they change", () => {
    const s = fixture();
    s.project.pcb.routing = { bend: "orthogonal90", conflict: "block", width: 400_000, via: { drill: 300_000, diameter: 650_000 }, remove_loops: false };
    const { rerender } = render(<PcbView snapshot={s} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Route");
    expect((screen.getByLabelText("Bend mode") as HTMLSelectElement).value).toBe("orthogonal90");
    expect((screen.getByLabelText("Obstacle mode") as HTMLSelectElement).value).toBe("block");
    expect((screen.getByLabelText("Track width") as HTMLInputElement).value).toBe("0.4");
    expect((screen.getByLabelText("Via preset") as HTMLSelectElement).value).toBe("300000/650000");
    expect((screen.getByLabelText("Remove loops") as HTMLSelectElement).value).toBe("off");
    // An undo (or another window) changes the saved options: the inspector shows them.
    const t = fixture();
    t.project.pcb.routing = { bend: "diagonal45", conflict: "push", width: null, via: null, remove_loops: true };
    rerender(<PcbView snapshot={t} selected={null} onSelect={() => {}} edit={edit()} />);
    expect((screen.getByLabelText("Obstacle mode") as HTMLSelectElement).value).toBe("push");
    expect((screen.getByLabelText("Track width") as HTMLInputElement).value).toBe("");
    expect(sent).toEqual([]);
  });

  it("without an inspector the options stay in the editor's strip", () => {
    render(<PcbView snapshot={fixture()} selected={null} onSelect={() => {}} edit={edit()} />);
    tool("Route");
    expect(screen.getByLabelText("Track width")).toBeTruthy();
    expect(screen.getByLabelText("Bend mode")).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Properties" })).toBeNull();
  });

  it("notifies instead of sending when an edit cannot be applied", () => {
    const s = fixture();
    s.project.pcb.footprints.fp_r1.locked = true;
    s.project.pcb.footprints.fp_d1.locked = true;
    render(<Both pcb snap={s} />);
    fireEvent.keyDown(window, { key: "a", ctrlKey: true });
    fireEvent.change(within(inspector()).getByLabelText("Layer (side)"), { target: { value: "bottom" } });
    expect(sent).toEqual([]);
    expect(notes).toEqual(["2 footprints locked; unlock to move."]);
  });
});
