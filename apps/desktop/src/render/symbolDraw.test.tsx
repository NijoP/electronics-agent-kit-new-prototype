// Slice B2: schematic symbols drawn from schema v3 graphics and pin geometry (ADR-0040).
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { fixture } from "../fixture.test-data";
import { SchematicView } from "../SchematicView";
import type { Graphic, Point, Snapshot } from "../types";
import { arcPoints, placeLocal, powerPortShape, powerStyleOf, symbolDrawing, type DrawableSymbol } from "./symbolDraw";

afterEach(cleanup);

const mm = (v: number) => Math.round(v * 1_000_000);
const pt = (x: number, y: number): Point => ({ x: mm(x), y: mm(y) });

const OPAMP_GRAPHICS: Graphic[] = [
  { kind: "polygon", points: [pt(-2.54, 2.54), pt(2.54, 0), pt(-2.54, -2.54)], fill: false },
  { kind: "arc", center: pt(0, 0), radius: mm(1), start_deg: 0, end_deg: 180 },
  { kind: "text", position: pt(-1.5, 1), text: "+", size: mm(1), rotation: "R0" },
];

/** An op-amp as the KiCad importer maps it: a triangle body, an arc, text and three pins. */
const OPAMP: DrawableSymbol = {
  body_half_w: mm(2.54),
  body_half_h: mm(2.54),
  graphics: OPAMP_GRAPHICS,
  pins: [
    { number: "3", name: "+", position: pt(-5.08, 1.27), length: mm(2.54), orientation: "right" },
    { number: "2", name: "-", position: pt(-5.08, -1.27), length: mm(2.54), orientation: "right" },
    { number: "1", name: "~", position: pt(5.08, 0), length: mm(2.54), orientation: "left" },
  ],
};

describe("symbol drawing geometry", () => {
  it("places library points exactly as the engine does: mirror about local Y, then rotate", () => {
    expect(placeLocal({ position: pt(10, 10), rotation: "R0" }, pt(1, 2))).toEqual(pt(11, 12));
    expect(placeLocal({ position: pt(0, 0), rotation: "R90" }, pt(1, 0))).toEqual(pt(0, 1));
    expect(placeLocal({ position: pt(0, 0), rotation: "R90", mirror: true }, pt(1, 0))).toEqual(pt(0, -1));
    expect(placeLocal({ position: pt(0, 0), rotation: "R270" }, pt(1, 0))).toEqual(pt(0, -1));
  });

  it("draws pins from their connection point towards the body, turned with the symbol", () => {
    const d = symbolDrawing(OPAMP, { position: pt(10, 10), rotation: "R90" });
    const p3 = d.pins.find((p) => p.number === "3")!;
    // R90 turns (-5.08, 1.27) into (-1.27, -5.08): sheet (8.73, 4.92), view y flipped.
    expect(p3.at).toEqual([8.73, -4.92]);
    expect(p3.inner).toEqual([8.73, -7.46]);
    // A vertical pin's number reads bottom to top; the unnamed output ("~") draws no name.
    const n3 = d.texts.find((t) => t.role === "pin-number" && t.text === "3")!;
    expect(n3.rotate).toBe(-90);
    expect(d.texts.filter((t) => t.role === "pin-name").map((t) => t.text)).toEqual(["+", "-"]);
  });

  it("puts pin names inside the body and numbers over the pin line", () => {
    const d = symbolDrawing(OPAMP, { position: pt(0, 0), rotation: "R0" });
    const name = d.texts.find((t) => t.role === "pin-name" && t.text === "+")!;
    expect(name.anchor).toBe("start");
    expect(name.x).toBeCloseTo(-2.54 + 0.5);
    const num = d.texts.find((t) => t.role === "pin-number" && t.text === "1")!;
    expect([num.x, num.anchor, num.rotate]).toEqual([3.81, "middle", 0]);
    expect(num.y).toBeLessThan(0); // above the line (view Y down)
  });

  it("hides pin texts the definition hides", () => {
    const quiet: DrawableSymbol = { ...OPAMP, pins: OPAMP.pins.map((p) => ({ ...p, name_visible: false, number_visible: false })) };
    expect(symbolDrawing(quiet, { position: pt(0, 0), rotation: "R0" }).texts.map((t) => t.role)).toEqual(["text"]);
  });

  it("maps every graphic kind and keeps text upright", () => {
    const d = symbolDrawing(OPAMP, { position: pt(0, 0), rotation: "R180" });
    expect(d.body.map((b) => [b.kind, b.fill])).toEqual([
      ["path", "body"],
      ["path", "none"],
    ]);
    expect((d.body[0] as { d: string }).d).toBe("M2.54 2.54L-2.54 0L2.54 -2.54Z");
    const text = d.texts.find((t) => t.role === "text")!;
    expect([text.x, text.y, text.rotate]).toEqual([1.5, 1, 0]);
  });

  it("samples arcs counter-clockwise, at most 5 degrees apart", () => {
    const pts = arcPoints(pt(0, 0), mm(1), 0, 90);
    expect(pts).toHaveLength(19);
    expect(pts[0].x).toBeCloseTo(mm(1));
    expect(pts[18].y).toBeCloseTo(mm(1));
    // Through 360: 350 -> 10 is a 20 degree sweep across +X.
    expect(arcPoints(pt(0, 0), mm(1), 350, 10)).toHaveLength(5);
  });

  it("draws a stroke width and dash style when the graphic sets one", () => {
    const def: DrawableSymbol = {
      pins: [],
      graphics: [{ kind: "rectangle", min: pt(-1, -1), max: pt(1, 1), fill: true, stroke: { width: mm(0.5), style: "dashed" } }],
    };
    expect(symbolDrawing(def, { position: pt(0, 0), rotation: "R0" }).body[0]).toEqual({ kind: "rect", x: -1, y: -1, w: 2, h: 2, fill: "solid", width: 0.5, dash: "2 1" });
  });

  it("a definition without v3 graphics still draws as its box with pin stubs (v2 fixtures)", () => {
    const placed = { half_w: mm(2), half_h: mm(1), pins: [{ number: "1", position: pt(46, 50) }] };
    const d = symbolDrawing({ pins: [{ number: "1", name: "~" }] }, { position: pt(50, 50), rotation: "R0" }, placed);
    expect(d.body).toEqual([{ kind: "rect", x: 48, y: -51, w: 4, h: 2, fill: "body" }]);
    expect(d.pins).toEqual([{ number: "1", at: [46, -50], inner: [48, -50] }]);
    expect(d.texts).toEqual([]);
  });
});

describe("power ports", () => {
  it("chooses ground for ground-like names unless the port sets a style", () => {
    expect(["GND", "AGND", "pgnd", "VSS", "0V"].map((n) => powerStyleOf(n))).toEqual(Array(5).fill("ground"));
    expect(["VCC", "+3V3", "+5V", "VBUS"].map((n) => powerStyleOf(n))).toEqual(Array(4).fill("rail"));
    expect(powerStyleOf("GND", "chassis")).toBe("chassis");
  });

  it("draws a rail bar above the point and ground bars below it", () => {
    expect(powerPortShape("rail", 0, 0).d).toBe("M0 0L0 -1.27M-1.27 -1.27L1.27 -1.27");
    expect(powerPortShape("rail", 0, 0).text.y).toBeLessThan(0);
    expect(powerPortShape("ground", 0, 0).text.y).toBeGreaterThan(0);
  });
});

/** The fixture with R1's definition replaced by the imported op-amp drawing. */
function withImportedSymbol(): Snapshot {
  const s = fixture();
  s.project.library = {
    R_0603: { id: "R_0603", name: "OPAMP", category: "Imported", refdes_prefix: "U", default_value: "OPAMP", symbol: OPAMP },
  };
  const r1 = s.geometry.symbols.find((g) => g.id === "sym_r1")!;
  r1.pins = OPAMP.pins.map((p) => ({ number: p.number, name: p.name, position: { x: mm(50) + p.position!.x, y: mm(50) + p.position!.y } }));
  return s;
}

describe("schematic view (slice B2)", () => {
  it("renders an imported KiCad symbol with its body, pin names and numbers", () => {
    const { container } = render(<SchematicView snapshot={withImportedSymbol()} selected={null} onSelect={() => {}} />);
    const r1 = container.querySelector('[data-symbol-id="sym_r1"]')!;
    expect(r1.querySelectorAll("path.body")).toHaveLength(2);
    expect(r1.querySelector("path.body.fill-none")).toBeTruthy();
    expect([...r1.querySelectorAll("text.pin-name")].map((t) => t.textContent)).toEqual(["+", "-"]);
    expect([...r1.querySelectorAll("text.pin-number")].map((t) => t.textContent)).toEqual(["3", "2", "1"]);
    // Pin lines end on the engine's pin points, where wires attach.
    const line = r1.querySelector('[data-pin="3"] line.pin')!;
    expect([line.getAttribute("x2"), line.getAttribute("y2")]).toEqual(["44.92", "-51.27"]);
    // The whole body stays clickable even though the drawing is open.
    expect(r1.querySelector("rect.sym-hit")).toBeTruthy();
    // A symbol whose definition is not in the snapshot keeps the v2 box.
    expect(container.querySelector('[data-symbol-id="sym_d1"] rect.body')).toBeTruthy();
  });

  it("draws the GND power label as a ground symbol", () => {
    const { container } = render(<SchematicView snapshot={fixture()} selected={null} onSelect={() => {}} />);
    const gnd = container.querySelector('[data-label-id="lab_1"]')!;
    expect(gnd.getAttribute("data-power-style")).toBe("ground");
    expect(gnd.querySelector("path.power-port")).toBeTruthy();
    expect(gnd.textContent).toBe("GND");
  });
});
