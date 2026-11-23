// Net highlight (slice B3): the engine netlist names the net; the sheet's drawing gives its members.
import { describe, expect, it } from "vitest";
import { fixture } from "../../fixture.test-data";
import type { Snapshot } from "../../types";
import { netAt, netMembers, sheetGroups, type NetRow } from "./nets";

const mm = (v: number) => v * 1_000_000;
const pt = (x: number, y: number) => ({ x: mm(x), y: mm(y) });

/** Wire R1.2 (54,50) to D1.2 (76,50); GND label on D1.1 (84,50); a stub wire T-joined at (60,50). */
function wired(): Snapshot {
  const s = fixture();
  s.project.schematic.wires.wir_1 = { id: "wir_1", sheet_id: "sht_1", a: pt(54, 50), b: pt(76, 50) };
  s.project.schematic.wires.wir_2 = { id: "wir_2", sheet_id: "sht_1", a: pt(60, 50), b: pt(60, 40) };
  s.project.schematic.labels.lab_1.position = pt(84, 50);
  return s;
}
const ROWS: NetRow[] = [
  { name: "N1", pins: ["R1.2", "D1.2"], label_names: [] },
  { name: "GND", pins: ["D1.1"], label_names: ["GND"] },
];

describe("net highlight", () => {
  it("groups wires joined end to end or on a wire's interior (T-join) with the pins and labels on them", () => {
    const g = sheetGroups(wired(), "sht_1");
    const wire = g.find((x) => x.wires.includes("wir_1"))!;
    expect(wire.wires.sort()).toEqual(["wir_1", "wir_2"]);
    expect(wire.pins.sort()).toEqual(["D1.2", "R1.2"]);
    const gnd = g.find((x) => x.labels.includes("lab_1"))!;
    expect(gnd.pinKeys).toEqual(["sym_d1/1"]);
    expect(gnd.labelNames).toEqual(["GND"]);
  });

  it("names the net from the engine netlist and highlights every drawn member of it", () => {
    const s = wired();
    const g = sheetGroups(s, "sht_1");
    const h = netAt(g, ROWS, "wir_2")!;
    expect(h.name).toBe("N1");
    const m = netMembers(g, h);
    expect([...m.wires].sort()).toEqual(["wir_1", "wir_2"]);
    expect([...m.pinKeys].sort()).toEqual(["sym_d1/2", "sym_r1/2"]);
    expect(m.labels.size).toBe(0);
    const gnd = netMembers(g, netAt(g, ROWS, "lab_1")!);
    expect([...gnd.labels]).toEqual(["lab_1"]);
    expect([...gnd.pinKeys]).toEqual(["sym_d1/1"]);
    expect(gnd.wires.size).toBe(0);
  });

  it("without the engine netlist, highlights what is drawn connected and names it by its label (or not at all)", () => {
    const g = sheetGroups(wired(), "sht_1");
    expect(netAt(g, null, "wir_1")!.name).toBeNull();
    expect(netAt(g, null, "lab_1")!.name).toBe("GND");
    expect([...netMembers(g, netAt(g, null, "wir_1")!).wires].sort()).toEqual(["wir_1", "wir_2"]);
    expect(netAt(g, null, "nope")).toBeNull();
  });

  it("same-named labels in separate groups light up together (labels join nets)", () => {
    const s = wired();
    s.project.schematic.labels.lab_2 = { id: "lab_2", sheet_id: "sht_1", name: "GND", position: pt(100, 100), kind: "power" };
    const g = sheetGroups(s, "sht_1");
    expect([...netMembers(g, netAt(g, null, "lab_1")!).labels].sort()).toEqual(["lab_1", "lab_2"]);
  });
});
