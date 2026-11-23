// Slice B4 place tools: one test per object the tools create (wire legs, bus, bus entry, power
// port, net port / off-page connector, graphics and text) and the fan-out Batch.
import { describe, expect, it } from "vitest";
import { fixture } from "../../fixture.test-data";
import type { Command, Point } from "../../types";
import {
  BUS_ENTRY_NM,
  FANOUT_NM,
  LABEL_SCOPE,
  busAt,
  busEntryWirePoint,
  constrainAxis,
  fanOutCommand,
  graphicFrom,
  nextWireMode,
  onSegment,
  powerPortCommand,
  unconnectedPins,
  wireLegs,
} from "./placeTools";
import { DEFAULT_POWER_PORTS, powerPortProblem } from "../../settings/powerPorts";

const mm = (v: number) => v * 1_000_000;
const pt = (x: number, y: number): Point => ({ x: mm(x), y: mm(y) });

describe("wire legs", () => {
  const a = pt(0, 0);
  const b = pt(10, 4);
  it("orthogonal: horizontal then vertical, Space flips to vertical first", () => {
    expect(wireLegs(a, b, "90")).toEqual([
      [a, pt(10, 0)],
      [pt(10, 0), b],
    ]);
    expect(wireLegs(a, b, "90", true)).toEqual([
      [a, pt(0, 4)],
      [pt(0, 4), b],
    ]);
  });
  it("45°: diagonal first then straight; flipped: straight first then diagonal", () => {
    expect(wireLegs(a, b, "45")).toEqual([
      [a, pt(4, 4)],
      [pt(4, 4), b],
    ]);
    expect(wireLegs(a, b, "45", true)).toEqual([
      [a, pt(6, 0)],
      [pt(6, 0), b],
    ]);
    // Exactly diagonal: one segment.
    expect(wireLegs(a, pt(-3, -3), "45")).toEqual([[a, pt(-3, -3)]]);
  });
  it("any angle is one segment; a straight leg is one segment in every mode; equal points none", () => {
    expect(wireLegs(a, b, "free")).toEqual([[a, b]]);
    for (const m of ["90", "45", "free"] as const) expect(wireLegs(a, pt(0, 7), m)).toEqual([[a, pt(0, 7)]]);
    expect(wireLegs(a, a, "90")).toEqual([]);
  });
  it("/ cycles the modes, Shift keeps the point on the dominant axis", () => {
    expect(nextWireMode("90")).toBe("45");
    expect(nextWireMode("45")).toBe("free");
    expect(nextWireMode("free")).toBe("90");
    expect(constrainAxis(a, pt(10, 4))).toEqual(pt(10, 0));
    expect(constrainAxis(a, pt(3, -9))).toEqual(pt(0, -9));
  });
  it("onSegment is exact on any angle", () => {
    expect(onSegment(pt(2, 2), a, pt(4, 4))).toBe(true);
    expect(onSegment(pt(2, 3), a, pt(4, 4))).toBe(false);
    expect(onSegment(pt(5, 5), a, pt(4, 4))).toBe(false);
  });
});

describe("bus and bus entry", () => {
  it("finds the bus under a point and puts the entry's wire end 0.1 in away on a diagonal; R turns it", () => {
    const s = fixture();
    s.project.schematic.buses = { bus_1: { id: "bus_1", sheet_id: "sht_1", name: "D[0..7]", points: [pt(10, 100), pt(60, 100), pt(60, 140)] } };
    expect(busAt(s, "sht_1", pt(30, 100))).toBe("bus_1");
    expect(busAt(s, "sht_1", pt(60, 120))).toBe("bus_1");
    expect(busAt(s, "sht_1", pt(30, 101))).toBeNull();
    expect(busAt(s, "other", pt(30, 100))).toBeNull();
    const at = pt(30, 100);
    expect(busEntryWirePoint(at, 0)).toEqual({ x: at.x + BUS_ENTRY_NM, y: at.y + BUS_ENTRY_NM });
    expect(busEntryWirePoint(at, 1)).toEqual({ x: at.x + BUS_ENTRY_NM, y: at.y - BUS_ENTRY_NM });
    expect(busEntryWirePoint(at, 2)).toEqual({ x: at.x - BUS_ENTRY_NM, y: at.y - BUS_ENTRY_NM });
    expect(busEntryWirePoint(at, 3)).toEqual({ x: at.x - BUS_ENTRY_NM, y: at.y + BUS_ENTRY_NM });
    expect(busEntryWirePoint(at, 4)).toEqual(busEntryWirePoint(at, 0));
  });
});

describe("graphics and text", () => {
  it("line, rectangle (normalised corners), circle and text; degenerate shapes and empty text give nothing", () => {
    expect(graphicFrom("line", pt(0, 0), pt(5, 5))).toMatchObject({ kind: "line", a: pt(0, 0), b: pt(5, 5) });
    expect(graphicFrom("rect", pt(5, 0), pt(0, 3))).toMatchObject({ kind: "rectangle", min: pt(0, 0), max: pt(5, 3), fill: false });
    expect(graphicFrom("circle", pt(1, 1), pt(4, 5))).toMatchObject({ kind: "circle", center: pt(1, 1), radius: mm(5) });
    expect(graphicFrom("text", pt(2, 2), pt(2, 2), "  Note  ")).toMatchObject({ kind: "text", position: pt(2, 2), text: "Note" });
    expect(graphicFrom("line", pt(1, 1), pt(1, 1))).toBeNull();
    expect(graphicFrom("rect", pt(1, 1), pt(1, 9))).toBeNull();
    expect(graphicFrom("circle", pt(1, 1), pt(1, 1))).toBeNull();
    expect(graphicFrom("text", pt(1, 1), pt(1, 1), "   ")).toBeNull();
  });
});

describe("net labels, ports and power ports", () => {
  it("net label is global, net port is sheet-local, off-page connector is off_page", () => {
    expect(LABEL_SCOPE).toEqual({ label: "global", netport: "local", offpage: "off_page" });
  });
  it("a power port from the list is a power AddNetLabel with its style", () => {
    expect(powerPortCommand("sht_1", { name: " AGND ", style: "ground" }, pt(3, 4))).toEqual({
      type: "AddNetLabel",
      sheet_id: "sht_1",
      name: "AGND",
      position: pt(3, 4),
      kind: "power",
      power_style: "ground",
    });
  });
  it("the default list has ground, a rail, analog ground and chassis; bad names are reported", () => {
    const names = DEFAULT_POWER_PORTS.map((p) => `${p.name}:${p.style}`);
    expect(names).toEqual(expect.arrayContaining(["GND:ground", "+3V3:rail", "AGND:ground", "CHASSIS:chassis"]));
    expect(DEFAULT_POWER_PORTS.every((p) => powerPortProblem(p) === null)).toBe(true);
    expect(powerPortProblem({ name: " ", style: "rail" })).toMatch(/net name/);
    expect(powerPortProblem({ name: "V CC", style: "rail" })).toMatch(/spaces/);
  });
});

describe("fan out net labels / no-connects", () => {
  const batch = (c: Command | string) => {
    if (typeof c === "string" || c.type !== "Batch") throw new Error(`not a Batch: ${JSON.stringify(c)}`);
    return c;
  };
  it("gives each unconnected pin a stub pointing away from the body and a label named after the pin", () => {
    const c = batch(fanOutCommand(fixture(), "sym_d1", "labels"));
    expect(c.label).toBe("Fan out net labels on D1");
    expect(c.commands).toEqual([
      { type: "AddWire", sheet_id: "sht_1", a: pt(84, 50), b: { x: mm(84) + FANOUT_NM, y: mm(50) } },
      { type: "AddNetLabel", sheet_id: "sht_1", name: "K", position: { x: mm(84) + FANOUT_NM, y: mm(50) }, kind: "net" },
      { type: "AddWire", sheet_id: "sht_1", a: pt(76, 50), b: { x: mm(76) - FANOUT_NM, y: mm(50) } },
      { type: "AddNetLabel", sheet_id: "sht_1", name: "A", position: { x: mm(76) - FANOUT_NM, y: mm(50) }, kind: "net" },
    ]);
  });
  it("names unnamed pins <refdes>_<number> and skips pins a wire, label or no-connect already reaches", () => {
    const s = fixture();
    s.project.schematic.wires.wir_1.a = pt(54, 50); // the wire now ends on R1 pin 2
    const c = batch(fanOutCommand(s, "sym_r1", "labels"));
    expect(c.commands.map((x) => (x.type === "AddNetLabel" ? x.name : x.type))).toEqual(["AddWire", "R1_1"]);
    s.project.schematic.no_connects = { nc_1: { id: "nc_1", symbol_id: "sym_r1", pin: "1" } };
    expect(fanOutCommand(s, "sym_r1", "labels")).toMatch(/Every pin of R1 is already connected/);
    s.project.schematic.labels.lab_1.position = pt(84, 50); // GND label on D1 pin K
    expect(unconnectedPins(s, s.geometry.symbols[1]).map((p) => p.number)).toEqual(["2"]);
  });
  it("no-connects: one SetNoConnect per unconnected pin, in one Batch", () => {
    const c = batch(fanOutCommand(fixture(), "sym_r1", "noconnects"));
    expect(c.label).toBe("Fan out no-connects on R1");
    expect(c.commands).toEqual([
      { type: "SetNoConnect", symbol_id: "sym_r1", pin: "1" },
      { type: "SetNoConnect", symbol_id: "sym_r1", pin: "2" },
    ]);
    expect(fanOutCommand(fixture(), "nope", "noconnects")).toMatch(/Select a placed part/);
  });
});
