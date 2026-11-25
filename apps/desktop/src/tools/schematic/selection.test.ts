import { describe, expect, it } from "vitest";
import { fixture } from "../../fixture.test-data";
import type { Snapshot } from "../../types";
import {
  pickMode,
  EMPTY,
  asBatch,
  copy,
  deleteCommands,
  marquee,
  moveCommands,
  moveSymbolsTo,
  pasteCommands,
  pick,
  prune,
  rectOf,
  rotateCommands,
  union,
  alignCommands,
  arrayCommands,
  distributeCommands,
  filtered,
  similar,
  groupCommand,
  pickGrouped,
  ungroupCommands,
  withGroups,
} from "./selection";

const mm = (v: number) => v * 1_000_000;
const pt = (x: number, y: number) => ({ x: mm(x), y: mm(y) });

/** Fixture with the wire joining R1 pin 2 (54,50) to D1 pin 2 (76,50), GND label on D1 pin 1. */
function wired(): Snapshot {
  const s = fixture();
  s.project.schematic.wires.wir_1 = { id: "wir_1", sheet_id: "sht_1", a: pt(54, 50), b: pt(76, 50) };
  s.project.schematic.labels.lab_1.position = pt(84, 50);
  return s;
}

describe("selection", () => {
  it("click replaces, shift-click adds, ctrl-click toggles, union merges", () => {
    let s = pick(EMPTY, "symbol", "sym_r1", "replace");
    expect(s).toEqual({ symbols: ["sym_r1"], wires: [], labels: [] });
    s = pick(s, "wire", "wir_1", "add");
    expect(s.wires).toEqual(["wir_1"]);
    expect(pick(s, "symbol", "sym_r1", "add")).toBe(s); // Shift on a selected object keeps it
    s = pick(s, "symbol", "sym_r1", "toggle");
    expect(s.symbols).toEqual([]);
    s = pick(s, "symbol", "sym_r1", "toggle");
    expect(s.symbols).toEqual(["sym_r1"]);
    expect(pick(s, "label", "lab_1", "replace")).toEqual({ symbols: [], wires: [], labels: ["lab_1"] });
    expect(pickMode({ shiftKey: true, ctrlKey: false, metaKey: false })).toBe("add");
    expect(pickMode({ shiftKey: true, ctrlKey: true, metaKey: false })).toBe("toggle");
    expect(pickMode({ shiftKey: false, ctrlKey: false, metaKey: true })).toBe("toggle");
    expect(pickMode({ shiftKey: false, ctrlKey: false, metaKey: false })).toBe("replace");
    expect(union({ ...EMPTY, symbols: ["a"] }, { ...EMPTY, symbols: ["a", "b"] }).symbols).toEqual(["a", "b"]);
  });

  it("prune drops ids the engine no longer has", () => {
    const s = { symbols: ["sym_r1", "gone"], wires: ["wir_1"], labels: [] };
    expect(prune(s, fixture())).toEqual({ symbols: ["sym_r1"], wires: ["wir_1"], labels: [] });
    const kept = { symbols: ["sym_r1"], wires: [], labels: [] };
    expect(prune(kept, fixture())).toBe(kept);
  });

  it("marquee selects only objects entirely inside", () => {
    const snap = wired();
    // Covers R1 (body 48..52, pins 46..54) but only one end of the wire.
    expect(marquee(snap, "sht_1", rectOf(pt(45, 45), pt(60, 55)))).toEqual({ symbols: ["sym_r1"], wires: [], labels: [] });
    const all = marquee(snap, "sht_1", rectOf(pt(40, 40), pt(90, 60)));
    expect(all).toEqual({ symbols: ["sym_r1", "sym_d1"], wires: ["wir_1"], labels: ["lab_1"] });
    // Another sheet sees nothing.
    expect(marquee(snap, "sht_2", rectOf(pt(0, 0), pt(300, 300)))).toEqual(EMPTY);
  });
});

describe("move", () => {
  it("drags attached wire ends and pin labels, wire edits before the symbol move", () => {
    const snap = wired();
    const cmds = moveCommands(snap, "sht_1", { ...EMPTY, symbols: ["sym_d1"] }, pt(0, 10));
    expect(cmds).toEqual([
      { type: "MoveWire", wire_id: "wir_1", a: pt(54, 50), b: pt(76, 60) },
      { type: "MoveNetLabel", label_id: "lab_1", position: pt(84, 60) },
      { type: "MoveSymbol", symbol_id: "sym_d1", position: pt(80, 60) },
    ]);
  });

  it("moves a selected wire whole and stretches wires sharing its ends", () => {
    const snap = wired();
    snap.project.schematic.wires.wir_2 = { id: "wir_2", sheet_id: "sht_1", a: pt(76, 50), b: pt(76, 70) };
    snap.project.schematic.wires.wir_3 = { id: "wir_3", sheet_id: "sht_1", a: pt(0, 0), b: pt(5, 0) };
    const cmds = moveCommands(snap, "sht_1", { ...EMPTY, wires: ["wir_1"] }, pt(-2, 0));
    expect(cmds).toEqual([
      { type: "MoveWire", wire_id: "wir_1", a: pt(52, 50), b: pt(74, 50) },
      { type: "MoveWire", wire_id: "wir_2", a: pt(74, 50), b: pt(76, 70) },
    ]);
  });

  it("deletes a wire that would collapse to a point", () => {
    const snap = wired();
    snap.project.schematic.wires.wir_1 = { id: "wir_1", sheet_id: "sht_1", a: pt(54, 50), b: pt(54, 60) };
    const cmds = moveCommands(snap, "sht_1", { ...EMPTY, symbols: ["sym_r1"] }, pt(0, 10));
    expect(cmds[0]).toEqual({ type: "DeleteWire", wire_id: "wir_1" });
  });

  it("zero delta or empty selection is no command", () => {
    expect(moveCommands(wired(), "sht_1", { ...EMPTY, symbols: ["sym_r1"] }, pt(0, 0))).toEqual([]);
    expect(moveCommands(wired(), "sht_1", EMPTY, pt(1, 0))).toEqual([]);
  });

  it("moving everything moves each object exactly once", () => {
    const snap = wired();
    const sel = marquee(snap, "sht_1", rectOf(pt(40, 40), pt(90, 60)));
    const cmds = moveCommands(snap, "sht_1", sel, pt(10, 0));
    expect(cmds.map((c) => c.type).sort()).toEqual(["MoveNetLabel", "MoveSymbol", "MoveSymbol", "MoveWire"]);
    expect(cmds).toContainEqual({ type: "MoveWire", wire_id: "wir_1", a: pt(64, 50), b: pt(86, 50) });
  });

  it("moves symbols to new origins, each by its own delta, dragging shared wires once (Inspector X/Y, F3)", () => {
    const snap = wired();
    // R1 50 -> 60 (+10), D1 80 -> 60 (-20): the wire between their pins 2 moves both ends in one MoveWire.
    expect(moveSymbolsTo(snap, [{ id: "sym_r1", position: pt(60, 50) }, { id: "sym_d1", position: pt(60, 50) }])).toEqual([
      { type: "MoveWire", wire_id: "wir_1", a: pt(64, 50), b: pt(56, 50) },
      { type: "MoveNetLabel", label_id: "lab_1", position: pt(64, 50) },
      { type: "MoveSymbol", symbol_id: "sym_r1", position: pt(60, 50) },
      { type: "MoveSymbol", symbol_id: "sym_d1", position: pt(60, 50) },
    ]);
    // one symbol is exactly moveCommands with the delta to its target
    expect(moveSymbolsTo(snap, [{ id: "sym_r1", position: pt(60, 50) }])).toEqual(moveCommands(snap, "sht_1", { ...EMPTY, symbols: ["sym_r1"] }, pt(10, 0)));
    // an unchanged position or an unknown id is no command
    expect(moveSymbolsTo(snap, [{ id: "sym_r1", position: pt(50, 50) }, { id: "sym_x", position: pt(1, 1) }])).toEqual([]);
  });
});

describe("rotate, delete, copy/paste", () => {
  it("rotates a quarter turn and drags the wire end to the rotated pin", () => {
    const snap = wired();
    const cmds = rotateCommands(snap, "sht_1", { ...EMPTY, symbols: ["sym_r1"] });
    // R1 at (50,50): pin 2 at (54,50) turns to (50,54).
    expect(cmds).toEqual([
      { type: "MoveWire", wire_id: "wir_1", a: pt(50, 54), b: pt(76, 50) },
      { type: "RotateSymbol", symbol_id: "sym_r1", rotation: "R90" },
    ]);
    expect(rotateCommands(snap, "sht_1", { ...EMPTY, wires: ["wir_1"] })).toEqual([]);
  });

  it("deletes labels, wires, then symbols", () => {
    expect(deleteCommands({ symbols: ["s"], wires: ["w"], labels: ["l"] })).toEqual([
      { type: "DeleteNetLabel", label_id: "l" },
      { type: "DeleteWire", wire_id: "w" },
      { type: "DeleteSymbol", symbol_id: "s" },
    ]);
  });

  it("copies by value and pastes with an offset and fresh designators", () => {
    const snap = wired();
    const clip = copy(snap, { symbols: ["sym_r1"], wires: ["wir_1"], labels: ["lab_1"] })!;
    expect(copy(snap, EMPTY)).toBeNull();
    expect(pasteCommands(clip, "sht_1", pt(5, 5))).toEqual([
      { type: "PlaceSymbol", sheet_id: "sht_1", component_id: "R_0603", position: pt(55, 55), rotation: "R0", refdes: null, value: "330" },
      { type: "AddWire", sheet_id: "sht_1", a: pt(59, 55), b: pt(81, 55) },
      { type: "AddNetLabel", sheet_id: "sht_1", name: "GND", kind: "power", position: pt(89, 55) },
    ]);
  });

  it("wraps several commands in one Batch, passes one through, and none is nothing", () => {
    const one = { type: "DeleteWire" as const, wire_id: "w" };
    expect(asBatch([], "x")).toBeNull();
    expect(asBatch([one], "x")).toBe(one);
    expect(asBatch([one, one], "Delete 2 objects")).toEqual({ type: "Batch", commands: [one, one], label: "Delete 2 objects" });
  });
});

describe("slice B3: arrange, array, similar, filter, paste on another sheet", () => {
  /** wired() plus a third part U1 at (110,60) (no pins in its geometry). */
  function three(): Snapshot {
    const s = wired();
    s.project.schematic.symbols.sym_u1 = { id: "sym_u1", sheet_id: "sht_1", component_id: "R_0603", refdes: "R2", value: "1k", position: pt(110, 60), rotation: "R0" };
    s.geometry.symbols.push({ id: "sym_u1", sheet_id: "sht_1", refdes: "R2", value: "1k", position: pt(110, 60), half_w: mm(2), half_h: mm(1), pins: [] });
    return s;
  }

  it("align left moves each part by its own delta in one command list, dragging attached wire ends and labels", () => {
    const s = wired();
    const sel = { ...EMPTY, symbols: ["sym_r1", "sym_d1"] };
    // R1 body left edge 48 mm; D1 (80,50) moves 30 mm left with its pins: wire end 76 -> 46, GND label 84 -> 54.
    expect(alignCommands(s, "sht_1", sel, "left")).toEqual([
      { type: "MoveWire", wire_id: "wir_1", a: pt(54, 50), b: pt(46, 50) },
      { type: "MoveNetLabel", label_id: "lab_1", position: pt(54, 50) },
      { type: "MoveSymbol", symbol_id: "sym_d1", position: pt(50, 50) },
    ]);
    // already aligned on top edges: nothing to send; one part alone cannot be aligned
    expect(alignCommands(s, "sht_1", sel, "top")).toEqual([]);
    expect(alignCommands(s, "sht_1", { ...EMPTY, symbols: ["sym_r1"] }, "left")).toEqual([]);
  });

  it("distributes three or more parts evenly between the outermost two", () => {
    const s = three();
    const sel = { ...EMPTY, symbols: ["sym_r1", "sym_d1", "sym_u1"] };
    // x: 50, 80, 110 is already even; y: 50, 50, 60 -> the middle one (by y order, D1 or R1) moves to 55
    expect(distributeCommands(s, "sht_1", sel, "x")).toEqual([]);
    const y = distributeCommands(s, "sht_1", sel, "y");
    expect(y.filter((c) => c.type === "MoveSymbol")).toEqual([{ type: "MoveSymbol", symbol_id: "sym_r1", position: pt(50, 55) }]);
    expect(distributeCommands(s, "sht_1", { ...EMPTY, symbols: ["sym_r1", "sym_d1"] }, "x")).toEqual([]);
  });

  it("an array pastes cols x rows - 1 copies on the step, the original cell skipped", () => {
    const clip = copy(wired(), { ...EMPTY, symbols: ["sym_r1"] })!;
    const cmds = arrayCommands(clip, "sht_1", 3, 2, pt(10, -5));
    expect(cmds).toHaveLength(5);
    expect(cmds.map((c) => (c.type === "PlaceSymbol" ? c.position : null))).toEqual([pt(60, 50), pt(70, 50), pt(50, 45), pt(60, 45), pt(70, 45)]);
    expect(arrayCommands(clip, "sht_1", 1, 1, pt(10, 10))).toEqual([]);
  });

  it("find similar selects parts of the same component, labels of the same name, or every wire; the filter drops kinds", () => {
    const s = three();
    expect(similar(s, "sht_1", "symbol", "sym_r1").symbols.sort()).toEqual(["sym_r1", "sym_u1"]);
    expect(similar(s, "sht_1", "label", "lab_1").labels).toEqual(["lab_1"]);
    expect(similar(s, "sht_1", "wire", "wir_1").wires).toEqual(["wir_1"]);
    const all = { symbols: ["sym_r1"], wires: ["wir_1"], labels: ["lab_1"] };
    expect(filtered(all, { symbol: true, wire: false, label: true })).toEqual({ symbols: ["sym_r1"], wires: [], labels: ["lab_1"] });
    expect(filtered(all, { symbol: true, wire: true, label: true })).toBe(all);
  });

  it("pasting on another sheet keeps nets by label only: new objects on that sheet, labels with their name, scope and style", () => {
    const s = wired();
    s.project.schematic.labels.lab_1 = { ...s.project.schematic.labels.lab_1, scope: "global", power_style: "ground" };
    const clip = copy(s, { symbols: ["sym_d1"], wires: ["wir_1"], labels: ["lab_1"] })!;
    const cmds = pasteCommands(clip, "sht_2", pt(0, 0));
    expect(cmds.every((c) => "sheet_id" in c && c.sheet_id === "sht_2")).toBe(true);
    // no command refers to an object of the source sheet: nothing is connected by id, only by label name
    expect(cmds.map((c) => c.type)).toEqual(["PlaceSymbol", "AddWire", "AddNetLabel"]);
    expect(cmds[2]).toEqual({ type: "AddNetLabel", sheet_id: "sht_2", name: "GND", kind: "power", position: pt(84, 50), scope: "global", power_style: "ground" });
    expect(cmds[0]).toMatchObject({ refdes: null });
  });
});

describe("groups (ADR-0045)", () => {
  const grouped = (): Snapshot => {
    const s = wired();
    s.project.schematic.groups = { grp_1: { id: "grp_1", sheet_id: "sht_1", name: "Group 1", members: ["lab_1", "sym_r1", "wir_1"] } };
    return s;
  };

  it("a click on a member selects the whole group; Ctrl/Cmd toggles the whole group off", () => {
    const s = grouped();
    const whole = { symbols: ["sym_r1"], wires: ["wir_1"], labels: ["lab_1"] };
    expect(pickGrouped(s, EMPTY, "wire", "wir_1", "replace")).toEqual(whole);
    const plus = pickGrouped(s, { ...EMPTY, symbols: ["sym_d1"] }, "symbol", "sym_r1", "add");
    expect(plus).toEqual({ symbols: ["sym_d1", "sym_r1"], wires: ["wir_1"], labels: ["lab_1"] });
    expect(pickGrouped(s, plus, "label", "lab_1", "toggle")).toEqual({ symbols: ["sym_d1"], wires: [], labels: [] });
    // an ungrouped object picks as before
    expect(pickGrouped(s, EMPTY, "symbol", "sym_d1", "replace")).toEqual(pick(EMPTY, "symbol", "sym_d1", "replace"));
  });

  it("a box that touches a member widens to the group", () => {
    const s = grouped();
    expect(withGroups(s, { ...EMPTY, labels: ["lab_1"] })).toEqual({ symbols: ["sym_r1"], wires: ["wir_1"], labels: ["lab_1"] });
    const free = { ...EMPTY, symbols: ["sym_d1"] };
    expect(withGroups(s, free)).toBe(free);
  });

  it("Group needs two or more ungrouped objects; Ungroup names every group the selection touches", () => {
    const s = wired();
    expect(groupCommand(s, { ...EMPTY, symbols: ["sym_r1"] })).toBe("Select two or more objects first");
    expect(groupCommand(s, { ...EMPTY, symbols: ["sym_r1", "sym_d1"] })).toEqual({ type: "Group", members: ["sym_r1", "sym_d1"], name: null });
    expect(ungroupCommands(s, { ...EMPTY, symbols: ["sym_r1"] })).toEqual([]);
    const g = grouped();
    expect(groupCommand(g, { ...EMPTY, symbols: ["sym_r1", "sym_d1"] })).toBe("Group 1 is already a group: ungroup it first");
    expect(ungroupCommands(g, { ...EMPTY, wires: ["wir_1"] })).toEqual([{ type: "Ungroup", group_id: "grp_1" }]);
  });
});
