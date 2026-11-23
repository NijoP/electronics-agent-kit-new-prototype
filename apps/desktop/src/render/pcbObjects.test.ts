// Schema v4 PCB objects in the desktop renderers (ADR-0042): arc flattening matches the engine's
// centre convention, the scene draws arc tracks as chords and adds regions, cut-outs, text and
// dimensions as non-pickable decoration.
import { describe, expect, it } from "vitest";
import type { CopperLayer, Point } from "../types";
import { toView } from "../viewport";
import { referenceBasic } from "./fixtures";
import { arcCenter, arcPoints, dimensionDrawing, dimensionLabel, outlinePath, trackPath } from "./pcbObjects";
import { buildPcbScene, type PcbSceneOptions } from "./pcbScene";

const mm = (x: number, y: number): Point => ({ x: Math.round(x * 1e6), y: Math.round(y * 1e6) });
const opts: PcbSceneOptions = {
  order: ["top", "bottom"] as CopperLayer[],
  hidden: new Set<string>(),
  colourOf: () => "#c0392b",
  activeLayer: "top",
  airwires: null,
};

describe("arc geometry", () => {
  it("puts the centre where the engine does (positive sweep = counter-clockwise)", () => {
    // 90° CCW from (10,10) to (20,10): centre (15,15), as the Gerber writer's test pins it.
    const c = arcCenter(mm(10, 10), mm(20, 10), 90);
    expect(c).not.toBeNull();
    expect(c![0]).toBeCloseTo(15e6, 0);
    expect(c![1]).toBeCloseTo(15e6, 0);
    expect(arcCenter(mm(0, 0), mm(0, 0), 90)).toBeNull();
    expect(arcCenter(mm(0, 0), mm(1, 0), 0)).toBeNull();
  });

  it("flattens within the chord error and keeps both ends", () => {
    const a = mm(0, 0);
    const b = mm(10, 0);
    const pts = arcPoints(a, b, 180, 20_000);
    expect(pts[0]).toEqual(a);
    expect(pts[pts.length - 1]).toEqual(b);
    expect(pts.length).toBeGreaterThan(10);
    // A half turn CCW from (0,0) to (10,0) passes below the chord (Y up), radius 5 mm.
    for (const p of pts) expect(Math.abs(Math.hypot(p.x - 5e6, p.y) - 5e6)).toBeLessThan(2);
    expect(Math.min(...pts.map((p) => p.y))).toBeLessThan(-4.9e6);
    expect(trackPath({ id: "t", layer: "top", width: 1, a, b, net: "" })).toEqual([a, b]);
  });

  it("flattens only the outline edges that carry an arc", () => {
    const sq = [mm(0, 0), mm(10, 0), mm(10, 10), mm(0, 10)];
    expect(outlinePath(sq, undefined)).toBe(sq);
    const path = outlinePath(sq, [{ edge: 1, sweep_deg: 90 }]);
    expect(path.length).toBeGreaterThan(5);
    expect(path.slice(0, 2)).toEqual(sq.slice(0, 2));
    expect(path.slice(-2)).toEqual(sq.slice(2));
  });

  it("lays out dimensions like the engine and labels them in their unit", () => {
    const d = {
      id: "d",
      layer: "documentation" as const,
      a: { x: 0, y: 0 },
      b: { x: 25_400_000, y: 0 },
      offset: -3_000_000,
      unit: "mm" as const,
      precision: 2,
      text_size: 0,
    };
    expect(dimensionLabel(d)).toBe("25.40 mm");
    expect(dimensionLabel({ ...d, unit: "mil", precision: 0 })).toBe("1000 mil");
    const g = dimensionDrawing(d);
    expect(g.lines).toHaveLength(7);
    expect(g.lines[0]).toEqual([
      { x: 0, y: -3_000_000 },
      { x: 25_400_000, y: -3_000_000 },
    ]);
    expect(g.textSize).toBe(1_000_000);
    expect(g.labelRotation).toBe(0);
  });
});

describe("scene with schema v4 objects", () => {
  it("draws an arc track as chords that hit on the arc, and adds the drawings unpickably", () => {
    const s = referenceBasic();
    const pcb = s.project.pcb;
    pcb.tracks["arc"] = { id: "arc", layer: "top", width: 250_000, a: mm(5, 5), b: mm(15, 5), net: "N", arc_deg: 180 };
    pcb.board.outline_arcs = [{ edge: 0, sweep_deg: -30 }];
    pcb.regions = { r: { id: "r", outline: [mm(1, 1), mm(3, 1), mm(3, 3)], forbid: ["tracks"] } };
    pcb.cutouts = { c: { id: "c", outline: [mm(4, 20), mm(6, 20), mm(6, 22)] } };
    pcb.texts = { t: { id: "t", layer: "top_silk", position: mm(20, 20), text: "REV A", size: 1_000_000 } };
    pcb.dimensions = { d: { id: "d", layer: "documentation", a: mm(0, 0), b: mm(10, 0), offset: 2_000_000, unit: "mm", precision: 1, text_size: 0 } };
    const scene = buildPcbScene(s, opts);
    const arc = scene.objects.find((o) => o.key === "track:arc")!;
    expect(arc.ops.length).toBeGreaterThan(10);
    expect(arc.ops.every((op) => op.t === "seg")).toBe(true);
    // The bottom of the half circle (10,0) is on the track; the chord midpoint (10,5) is not.
    expect(arc.hit(...toView(mm(10, 0)), 0.05)).toBe(true);
    expect(arc.hit(...toView(mm(10, 5)), 0.05)).toBe(false);
    for (const key of ["region:r", "cutout:c", "text:t", "dimension:d"]) {
      const o = scene.objects.find((x) => x.key === key);
      expect(o, key).toBeDefined();
      expect(o!.ref).toBeNull();
    }
    expect(scene.objects.find((o) => o.key === "text:t")!.layerKey).toBe("top:silk");
    expect(scene.objects.find((o) => o.key === "dimension:d")!.layerKey).toBe("outline");
    if (pcb.board.outline.length >= 2) {
      expect(scene.objects.find((o) => o.key === "outline")!.pathPts!.length).toBeGreaterThan(pcb.board.outline.length);
    }
  });
});
