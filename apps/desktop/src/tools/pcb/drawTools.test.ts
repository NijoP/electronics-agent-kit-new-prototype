// C4 drawing tools (slice F5): command building, geometric hit tests and arc re-indexing on a
// vertex insert / delete (S6: the outline's arc edges were silently made straight).
import { describe, expect, it } from "vitest";
import { arcCenter } from "../../render/pcbObjects";
import type { Command, Snapshot } from "../../types";
import * as D from "./drawTools";

const mm = (v: number) => v * 1_000_000;
const pt = (x: number, y: number) => ({ x: mm(x), y: mm(y) });
const square = [pt(0, 0), pt(40, 0), pt(40, 30), pt(0, 30)];
const opts = (o: Partial<D.RegionOptions> = {}): D.RegionOptions => ({ layer: null, forbid: D.forbidOf("all"), name: "", minWidth: null, maxWidth: null, ...o });
const steps = (c: Command) => (c.type === "Batch" ? c.commands : [c]);

describe("drawing tool commands", () => {
  it("a keep-out from two corners is a rectangle AddRegion; every copper layer by default", () => {
    expect(D.regionCommand([pt(2, 2), pt(6, 5)], opts())).toEqual({
      type: "AddRegion",
      name: null,
      outline: [pt(2, 2), pt(6, 2), pt(6, 5), pt(2, 5)],
      layers: [],
      forbid: ["tracks", "vias", "components", "zones"],
      min_track_width: null,
      max_track_width: null,
    });
    const c = D.regionCommand([pt(2, 2), pt(6, 5)], opts({ layer: "top", forbid: D.forbidOf("copper"), name: " RF " }));
    expect(c).toMatchObject({ layers: ["top"], forbid: ["tracks", "vias"], name: "RF" });
  });
  it("a region that keeps nothing out needs a width rule", () => {
    expect(D.regionCommand([pt(2, 2), pt(6, 5)], opts({ forbid: [] }))).toMatch(/keep something out/);
    expect(D.regionCommand([pt(2, 2), pt(6, 5)], opts({ forbid: [], maxWidth: mm(0.2) }))).toMatchObject({ forbid: [], max_track_width: mm(0.2) });
    expect(D.regionCommand([pt(2, 2)], opts())).toMatch(/two opposite corners/);
  });
  it("a cut-out needs a board outline and a valid polygon", () => {
    expect(D.cutoutCommand([pt(5, 5), pt(9, 9)], square)).toEqual({ type: "AddBoardCutout", outline: [pt(5, 5), pt(9, 5), pt(9, 9), pt(5, 9)] });
    expect(D.cutoutCommand([pt(5, 5), pt(9, 9)], [])).toMatch(/board outline/);
    expect(D.cutoutCommand([pt(5, 5), pt(9, 9), pt(5, 9), pt(9, 5)], square)).toMatch(/not valid/);
  });
  it("text needs text; the height is in mm", () => {
    expect(D.textCommand(pt(3, 4), "  ", "top_silk", "")).toMatch(/Type the text/);
    expect(D.textCommand(pt(3, 4), "REV B", "bottom_silk", "1.5")).toEqual({ type: "AddPcbText", layer: "bottom_silk", position: pt(3, 4), text: "REV B", size: mm(1.5), stroke_width: null, rotation: null });
    expect(D.textCommand(pt(3, 4), "X", "top_silk", "-1")).toMatch(/positive/);
    expect(D.nextTextRotation(undefined)).toBe("R90");
    expect(D.nextTextRotation("R270")).toBe("R0");
  });
  it("a dimension measures two different points", () => {
    expect(D.dimensionCommand(pt(0, 0), pt(40, 0), "documentation", "3")).toEqual({ type: "AddDimension", layer: "documentation", a: pt(0, 0), b: pt(40, 0), offset: mm(3), unit: null, precision: null, text_size: null });
    expect(D.dimensionCommand(pt(1, 1), pt(1, 1), "documentation", "")).toMatch(/two different points/);
  });
});

describe("drawing hit test (Delete and Text tools)", () => {
  const pcb = {
    board: { outline: square },
    footprints: {},
    tracks: {},
    vias: {},
    zones: {},
    regions: { rgn_1: { id: "rgn_1", outline: [pt(20, 10), pt(30, 10), pt(30, 20), pt(20, 20)], forbid: ["tracks" as const] } },
    cutouts: { cut_1: { id: "cut_1", outline: [pt(5, 5), pt(9, 5), pt(9, 9), pt(5, 9)] } },
    texts: { txt_1: { id: "txt_1", layer: "top_silk" as const, position: pt(25, 15), text: "REV A", size: mm(1) } },
    dimensions: { dim_1: { id: "dim_1", layer: "documentation" as const, a: pt(0, 30), b: pt(40, 30), offset: mm(3), unit: "mm" as const, precision: 2, text_size: 0 } },
  } as unknown as Snapshot["project"]["pcb"];
  const tol = mm(0.3);
  it("finds text above the region it sits in, then dimensions, cut-outs and regions", () => {
    expect(D.drawingAt(pcb, pt(25, 15.5), tol)).toEqual({ kind: "text", id: "txt_1" });
    expect(D.drawingAt(pcb, pt(22, 12), tol)).toEqual({ kind: "region", id: "rgn_1" });
    expect(D.drawingAt(pcb, pt(7, 7), tol)).toEqual({ kind: "cutout", id: "cut_1" });
    expect(D.drawingAt(pcb, pt(20, 33), tol)).toEqual({ kind: "dimension", id: "dim_1" });
    expect(D.drawingAt(pcb, pt(14, 4), tol)).toBeNull();
  });
  it("skips hidden layers", () => {
    expect(D.drawingAt(pcb, pt(25, 15.5), tol, (l) => l !== "top_silk")).toEqual({ kind: "region", id: "rgn_1" });
    expect(D.drawingAt(pcb, pt(7, 7), tol, (l) => l !== "outline")).toBeNull();
  });
  it("a turned text is hit along its turned box", () => {
    const t = { position: pt(0, 0), text: "ABCDE", size: mm(1), rotation: "R90" as const };
    expect(D.onText(pt(-0.5, 1.8), t, 0)).toBe(true);
    expect(D.onText(pt(1.8, 0.5), t, 0)).toBe(false);
  });
  it("maps each kind to its delete command", () => {
    expect(D.deleteDrawing({ kind: "text", id: "t" })).toEqual({ type: "DeletePcbText", text_id: "t" });
    expect(D.deleteDrawing({ kind: "dimension", id: "d" })).toEqual({ type: "DeleteDimension", dimension_id: "d" });
    expect(D.deleteDrawing({ kind: "cutout", id: "c" })).toEqual({ type: "DeleteBoardCutout", cutout_id: "c" });
    expect(D.deleteDrawing({ kind: "region", id: "r" })).toEqual({ type: "DeleteRegion", region_id: "r" });
  });
});

describe("outline vertex insert / delete keeps the arc edges (S6)", () => {
  it("without arcs it stays one SetBoardOutline", () => {
    const r = D.vertexEdit(square, [], "insert", 0) as D.VertexEdit;
    expect(r.command).toEqual({ type: "SetBoardOutline", points: [pt(0, 0), pt(20, 0), pt(40, 0), pt(40, 30), pt(0, 30)] });
  });
  it("inserting before an arc edge renumbers it in the same Batch, with no arc made straight", () => {
    const r = D.vertexEdit(square, [{ edge: 2, sweep_deg: 40 }], "insert", 0) as D.VertexEdit;
    expect(r.dropped).toEqual([]);
    expect(steps(r.command)).toEqual([
      { type: "SetOutlineArc", edge: 2, sweep_deg: null },
      { type: "SetBoardOutline", points: [pt(0, 0), pt(20, 0), pt(40, 0), pt(40, 30), pt(0, 30)] },
      { type: "SetOutlineArc", edge: 3, sweep_deg: 40 },
    ]);
  });
  it("inserting on an arc edge splits it at the arc's midpoint into two halves of the same circle", () => {
    const r = D.vertexEdit(square, [{ edge: 0, sweep_deg: 40 }], "insert", 0) as D.VertexEdit;
    const s = steps(r.command);
    const set = s.find((c) => c.type === "SetBoardOutline") as Extract<Command, { type: "SetBoardOutline" }>;
    const mid = set.points[1];
    const [cx, cy] = arcCenter(square[0], square[1], 40)!;
    const rad = Math.hypot(square[0].x - cx, square[0].y - cy);
    expect(Math.abs(Math.hypot(mid.x - cx, mid.y - cy) - rad)).toBeLessThan(2);
    expect(mid.x).toBe(mm(20));
    expect(s.filter((c) => c.type === "SetOutlineArc" && c.sweep_deg !== null)).toEqual([
      { type: "SetOutlineArc", edge: 0, sweep_deg: 20 },
      { type: "SetOutlineArc", edge: 1, sweep_deg: 20 },
    ]);
  });
  it("deleting a vertex renumbers later arcs and reports the arc on a joined edge", () => {
    const pent = [pt(0, 0), pt(20, -5), pt(40, 0), pt(40, 30), pt(0, 30)];
    const r = D.vertexEdit(pent, [{ edge: 0, sweep_deg: 10 }, { edge: 3, sweep_deg: -30 }], "delete", 1) as D.VertexEdit;
    expect(r.dropped).toEqual([0]);
    expect(steps(r.command).slice(-1)).toEqual([{ type: "SetOutlineArc", edge: 2, sweep_deg: -30 }]);
    // deleting vertex 0 joins the last edge and edge 0; edge 3 of 5 becomes edge 2 of 4
    const z = D.vertexEdit(pent, [{ edge: 4, sweep_deg: 10 }, { edge: 3, sweep_deg: 15 }], "delete", 0) as D.VertexEdit;
    expect(z.dropped).toEqual([4]);
    expect(steps(z.command).slice(-1)).toEqual([{ type: "SetOutlineArc", edge: 2, sweep_deg: 15 }]);
    expect(D.vertexEdit(square.slice(0, 3), [], "delete", 0)).toMatch(/at least 3/);
  });
  it("an arc field sends SetOutlineArc only when the sweep changes", () => {
    expect(D.arcCommand(1, "30", undefined)).toEqual({ type: "SetOutlineArc", edge: 1, sweep_deg: 30 });
    expect(D.arcCommand(1, "", 30)).toEqual({ type: "SetOutlineArc", edge: 1, sweep_deg: null });
    expect(D.arcCommand(1, "30", 30)).toBeNull();
    expect(D.arcCommand(1, "400", undefined)).toMatch(/between -360 and 360/);
  });
});
