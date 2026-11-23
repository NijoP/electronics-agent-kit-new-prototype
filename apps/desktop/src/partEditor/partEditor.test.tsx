// Slice D2: symbol and footprint editors. The editors keep a draft and save it through exactly one
// typed command (UpsertComponentDefinition / AssignComponentFootprint); the model mirrors the
// engine's definition checks; the documents follow the project library.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { fixture } from "../fixture.test-data";
import { availableDocs, loadDocState, partDocId, partDocs, partOfDoc, saveDocState, visibleDocs } from "../shell/documents";
import type { Command, ComponentDef } from "../types";
import { addPad, addPin, deletePad, draftProblems, footprintScene, newPartDefinition, newPartIdProblem, normDef, parsePoints, renamePad, renamePin, sameSlice, setPadPins, snapToIr, symbolScene } from "./model";
import { PartEditor } from "./PartEditor";

afterEach(() => {
  cleanup();
  localStorage.clear();
});

const part = (): ComponentDef => newPartDefinition("D2_PART", "D2 test part", "U", "custom");

describe("part editor model", () => {
  it("a new part is a valid starting definition (no local problems, mapped pins)", () => {
    const d = part();
    expect(draftProblems(d)).toEqual([]);
    expect(d.pin_map).toEqual({ "1": ["1"], "2": ["2"] });
    expect(d.symbol.graphics?.[0].kind).toBe("rectangle");
  });

  it("checks the same rules the engine validates", () => {
    const d = part();
    expect(draftProblems({ ...d, symbol: { ...d.symbol, graphics: [] } })).toContain("Draw the symbol body: add at least one graphic.");
    const tht = { ...d, footprint: { ...d.footprint, pads: d.footprint.pads.map((p) => ({ ...p, kind: "tht" as const, drill: null })) } };
    expect(draftProblems(tht)).toContain("Through-hole pad 1 needs a drill.");
    const ring = { ...d, footprint: { ...d.footprint, pads: d.footprint.pads.map((p) => ({ ...p, kind: "tht" as const, drill: 2_000_000 })) } };
    expect(draftProblems(ring)).toContain("Pad 1: the drill leaves no annular ring.");
    expect(draftProblems(addPin(d))).toContain("Pin 3 is not mapped to a pad.");
    expect(draftProblems(deletePad(d, "2"))).toContain("Pin 2 is not mapped to a pad.");
  });

  it("renumbering carries the pin map; pad pins edit the inverse map", () => {
    const d = part();
    expect(renamePin(d, "1", "A").pin_map).toEqual({ A: ["1"], "2": ["2"] });
    expect(renamePad(d, "2", "B").pin_map).toEqual({ "1": ["1"], "2": ["B"] });
    expect(setPadPins(d, "2", ["1", "2"]).pin_map).toEqual({ "1": ["1", "2"], "2": ["2"] });
    expect(setPadPins(d, "1", []).pin_map).toEqual({ "2": ["2"] });
  });

  it("a new pin maps to the pad of the same number when the footprint has it", () => {
    const d = addPad(part());
    expect(d.footprint.pads.map((p) => p.number)).toEqual(["1", "2", "3"]);
    const e = addPin(d);
    expect(e.pin_map["3"]).toEqual(["3"]);
    expect(draftProblems(e)).toEqual([]);
  });

  it("normalised slices compare equal across optional-field differences", () => {
    const d = part();
    const sparse = { ...d, symbol: { ...d.symbol, pins: d.symbol.pins.map(({ number, name, electrical_type, position }) => ({ number, name, electrical_type, position })) } };
    const filled = { ...sparse, symbol: { ...sparse.symbol, pins: sparse.symbol.pins.map((p) => ({ ...p, length: 0, orientation: "right" as const, name_visible: true, number_visible: true })) } };
    expect(sameSlice("symbol", sparse, filled)).toBe(true);
    expect(sameSlice("footprint", d, normDef(d))).toBe(true);
    expect(sameSlice("symbol", d, renamePin(d, "1", "9"))).toBe(false);
  });

  it("scenes carry pickable pins, pads and graphics (A4 render objects)", () => {
    const d = part();
    const s = symbolScene(d.symbol);
    expect(s.objects.filter((o) => o.ref?.kind === "pin").map((o) => o.ref!.id)).toEqual(["1", "2"]);
    expect(s.objects.filter((o) => o.ref?.kind === "graphic")).toHaveLength(1);
    // pin 1 at x = -5.08 mm, pointing right with a 2.54 mm line
    const pin1 = s.objects.find((o) => o.key === "pin:1")!;
    expect(pin1.hit(-4, 0, 0.01)).toBe(true);
    expect(pin1.hit(-1, 0, 0.01)).toBe(false);
    const f = footprintScene(d.footprint);
    const pad2 = f.objects.find((o) => o.key === "pad:2")!;
    expect(pad2.hit(1.27, 0, 0)).toBe(true);
    expect(pad2.hit(0, 0, 0)).toBe(false);
    expect(f.objects.some((o) => o.key === "courtyard")).toBe(true);
  });

  it("parses points and snaps view coordinates to IR nanometres (Y up)", () => {
    expect(parsePoints("0 0; 1.27 -2.54")).toEqual([
      { x: 0, y: 0 },
      { x: 1_270_000, y: -2_540_000 },
    ]);
    expect(parsePoints("1 2 3")).toBeNull();
    expect(snapToIr(1.3, -2.5, 1.27)).toEqual({ x: 1_270_000, y: 2_540_000 });
  });

  it("refuses new part ids that are empty, malformed or already used", () => {
    expect(newPartIdProblem("", new Set())).toMatch(/Enter/);
    expect(newPartIdProblem("a b", new Set())).toMatch(/letters/);
    expect(newPartIdProblem("R_0603", new Set(["R_0603"]))).toMatch(/already exists/);
    expect(newPartIdProblem("MY_PART", new Set(["R_0603"]))).toBeNull();
  });
});

function editor(kind: "symbol" | "footprint", def = part(), placedOnPcb = 0) {
  const execute = vi.fn(async (_c: Command | Command[]) => undefined);
  const onDirty = vi.fn();
  const r = render(<PartEditor kind={kind} def={def} disabled={null} placedOnPcb={placedOnPcb} execute={execute} onDirty={onDirty} />);
  return { execute, onDirty, ...r };
}
const save = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;

describe("symbol editor", () => {
  it("edits the pin table and saves once with UpsertComponentDefinition", () => {
    const { execute, onDirty } = editor("symbol");
    expect(save().disabled).toBe(true);
    const name = screen.getByLabelText("Pin 1 name");
    fireEvent.change(name, { target: { value: "IN" } });
    fireEvent.blur(name);
    fireEvent.change(screen.getByLabelText("Pin 2 type"), { target: { value: "output" } });
    expect(onDirty).toHaveBeenLastCalledWith(true);
    expect(screen.getByRole("status").textContent).toMatch(/Unsaved changes/);
    fireEvent.click(save());
    expect(execute).toHaveBeenCalledTimes(1);
    const c = execute.mock.calls[0][0] as Extract<Command, { type: "UpsertComponentDefinition" }>;
    expect(c.type).toBe("UpsertComponentDefinition");
    expect(c.definition.id).toBe("D2_PART");
    expect(c.definition.symbol.pins.map((p) => [p.number, p.name, p.electrical_type])).toEqual([
      ["1", "IN", "passive"],
      ["2", "2", "output"],
    ]);
  });

  it("does not offer Save while the draft breaks a rule, and says which", () => {
    editor("symbol");
    fireEvent.click(screen.getByRole("button", { name: "Add pin" }));
    expect(screen.getByRole("group", { name: "Problems" }).textContent).toContain("Pin 3 is not mapped to a pad.");
    expect(save().disabled).toBe(true);
    expect(save().getAttribute("title")).toMatch(/^Fix first: Pin 3 is not mapped/);
    const pads = screen.getByLabelText("Pin 3 pads");
    fireEvent.change(pads, { target: { value: "2" } });
    fireEvent.blur(pads);
    expect(save().disabled).toBe(false);
  });

  it("adds a B1 graphic and keeps millimetre edits in nanometres", () => {
    const { execute } = editor("symbol");
    fireEvent.change(screen.getByLabelText("New graphic kind"), { target: { value: "circle" } });
    fireEvent.click(screen.getByRole("button", { name: "Add graphic" }));
    const r = screen.getByLabelText("Graphic 2 radius");
    fireEvent.change(r, { target: { value: "0.5" } });
    fireEvent.keyDown(r, { key: "Enter" });
    fireEvent.click(save());
    const c = execute.mock.calls[0][0] as Extract<Command, { type: "UpsertComponentDefinition" }>;
    expect(c.definition.symbol.graphics?.[1]).toMatchObject({ kind: "circle", radius: 500_000 });
  });

  it("regression: a footprint saved by the other editor reaches the symbol draft (a new pin maps to the new pad)", () => {
    const d = part();
    const { rerender, execute, onDirty } = editor("symbol", d);
    // The footprint editor saved pad 3; the symbol editor's own slice did not change.
    rerender(<PartEditor kind="symbol" def={addPad(d)} disabled={null} placedOnPcb={0} execute={execute} onDirty={onDirty} />);
    fireEvent.click(screen.getByRole("button", { name: "Add pin" }));
    expect((screen.getByLabelText("Pin 3 pads") as HTMLInputElement).value).toBe("3");
    expect(screen.queryByRole("group", { name: "Problems" })).toBeNull();
    expect(save().disabled).toBe(false);
  });

  it("follows the engine when it saves, and Revert drops a draft", () => {
    const d = part();
    const { onDirty, rerender, execute } = editor("symbol", d);
    const name = screen.getByLabelText("Pin 1 name");
    fireEvent.change(name, { target: { value: "X" } });
    fireEvent.blur(name);
    expect(onDirty).toHaveBeenLastCalledWith(true);
    // The engine accepted it: the definition comes back with the edit.
    const saved = { ...d, symbol: { ...d.symbol, pins: d.symbol.pins.map((p) => (p.number === "1" ? { ...p, name: "X" } : p)) } };
    rerender(<PartEditor kind="symbol" def={saved} disabled={null} placedOnPcb={0} execute={execute} onDirty={onDirty} />);
    expect(onDirty).toHaveBeenLastCalledWith(false);
    const n2 = screen.getByLabelText("Pin 2 name");
    fireEvent.change(n2, { target: { value: "Y" } });
    fireEvent.blur(n2);
    fireEvent.click(screen.getByRole("button", { name: "Revert" }));
    expect((screen.getByLabelText("Pin 2 name") as HTMLInputElement).value).toBe("2");
    expect(onDirty).toHaveBeenLastCalledWith(false);
  });
});

describe("footprint editor", () => {
  it("edits pads (shape, size, drill) and the courtyard, saved with AssignComponentFootprint", () => {
    const { execute } = editor("footprint");
    fireEvent.click(screen.getByRole("button", { name: "Add pad" }));
    fireEvent.change(screen.getByLabelText("Pad 3 kind"), { target: { value: "tht" } });
    fireEvent.change(screen.getByLabelText("Pad 3 shape"), { target: { value: "circle" } });
    const sx = screen.getByLabelText("Pad 3 size X");
    fireEvent.change(sx, { target: { value: "1.6" } });
    fireEvent.blur(sx);
    const drill = screen.getByLabelText("Pad 3 drill");
    fireEvent.change(drill, { target: { value: "0.8" } });
    fireEvent.blur(drill);
    const cw = screen.getByLabelText("Courtyard half width");
    fireEvent.change(cw, { target: { value: "3.5" } });
    fireEvent.blur(cw);
    fireEvent.click(save());
    expect(execute).toHaveBeenCalledTimes(1);
    const c = execute.mock.calls[0][0] as Extract<Command, { type: "AssignComponentFootprint" }>;
    expect(c.type).toBe("AssignComponentFootprint");
    expect(c.component_id).toBe("D2_PART");
    expect(c.footprint.courtyard_half_w).toBe(3_500_000);
    expect(c.footprint.pads[2]).toMatchObject({ number: "3", kind: "tht", shape: "circle", size_x: 1_600_000, drill: 800_000 });
    expect(c.pin_map).toEqual({ "1": ["1"], "2": ["2"] });
  });

  it("states that mask/paste expansion is not available instead of showing a control", () => {
    editor("footprint");
    expect(screen.getByText(/Solder-mask and paste expansion per pad: not available yet/)).toBeTruthy();
    expect(screen.queryByLabelText(/mask/i)).toBeNull();
  });

  it("refuses to save a footprint change while the part is placed on the PCB, with the reason", () => {
    const { execute } = editor("footprint", part(), 2);
    fireEvent.click(screen.getByRole("button", { name: "Add pad" }));
    expect(save().disabled).toBe(true);
    expect(save().getAttribute("title")).toMatch(/placed on the PCB \(2 footprint/);
    fireEvent.click(save());
    expect(execute).not.toHaveBeenCalled();
  });
});

describe("part editor documents", () => {
  const snap = () => {
    const s = fixture(3);
    s.project.library = { D2_PART: part() as unknown as NonNullable<typeof s.project.library>[string] };
    return s;
  };

  it("a part document exists only while its part is in the project library", () => {
    const s = snap();
    const ids = [partDocId("symbol", "D2_PART"), partDocId("footprint", "D2_PART"), partDocId("symbol", "GONE")];
    expect(partDocs(ids, s).map((d) => [d.kind, d.ref, d.title])).toEqual([
      ["symbol", "D2_PART", "Symbol: D2_PART"],
      ["footprint", "D2_PART", "Footprint: D2_PART"],
    ]);
    expect(partOfDoc("part-fp:X")).toEqual({ kind: "footprint", componentId: "X" });
    expect(partOfDoc("sch:sht_1")).toBeNull();
  });

  it("restores open part editors and shows one alone, even in split view", () => {
    const s = snap();
    const id = partDocId("footprint", "D2_PART");
    saveDocState(s.project.id, { open: ["sch:sht_1", id, partDocId("symbol", "GONE")], active: id });
    const st = loadDocState(s);
    expect(st).toEqual({ open: ["sch:sht_1", id], active: id });
    const docs = [...availableDocs(s), ...partDocs(st.open, s)];
    expect(visibleDocs(st, docs, true).map((d) => d.id)).toEqual([id]);
  });
});
