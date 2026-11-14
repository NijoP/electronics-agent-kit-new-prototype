import { describe, expect, it } from "vitest";
import { fixture } from "../../fixture.test-data";
import type { Snapshot } from "../../types";
import {
  EMPTY,
  asBatch,
  copy,
  deleteCommands,
  marquee,
  moveCommands,
  pasteCommands,
  pick,
  prune,
  rectOf,
  rotateCommands,
  union,
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
  it("click replaces, shift-click toggles, union merges", () => {
    let s = pick(EMPTY, "symbol", "sym_r1", false);
    expect(s).toEqual({ symbols: ["sym_r1"], wires: [], labels: [] });
    s = pick(s, "wire", "wir_1", true);
    expect(s.wires).toEqual(["wir_1"]);
    s = pick(s, "symbol", "sym_r1", true);
    expect(s.symbols).toEqual([]);
    expect(pick(s, "label", "lab_1", false)).toEqual({ symbols: [], wires: [], labels: ["lab_1"] });
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
