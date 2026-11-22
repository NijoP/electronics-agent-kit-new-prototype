// Scene renderer (slice A4): render-object table, layer draw-order stack, picking, and the
// Canvas2D backend's per-layer offscreen cache with dirty rectangles, against a recording 2D
// context (jsdom has none). Real-browser pixels and frame rates are measured by the render QA
// page in CI (`e2e/render-qa.mjs`), not here.
import { describe, expect, it } from "vitest";
import type { CopperLayer } from "../types";
import { Canvas2DRenderer, diffBoxes, MAX_DIRTY_RECTS } from "./canvas2d";
import { benchmarkBoard, BENCH_FOOTPRINTS, BENCH_SEGMENTS, referenceBasic, referenceFourLayer } from "./fixtures";
import { GridIndex } from "./geometry";
import { buildPcbScene, FAR_SIDE_ALPHA, pcbLayers, type PcbSceneOptions } from "./pcbScene";
import { PickIndex } from "./pick";
import { stubContext, stubSurfaces, type Call } from "./stubContext.test-util";
import type { PickOptions, RenderObject, Scene, ViewTransform } from "./types";

const FOUR: CopperLayer[] = ["top", { inner: 1 }, { inner: 2 }, "bottom"];
const opts = (order: CopperLayer[], o: Partial<PcbSceneOptions> = {}): PcbSceneOptions => ({
  order,
  hidden: new Set(),
  colourOf: (l) => (typeof l === "string" ? (l === "top" ? "#c0392b" : "#2f6fb5") : "#3fa66b"),
  activeLayer: "top",
  airwires: null,
  ...o,
});
const basic = (o: Partial<PcbSceneOptions> = {}) => buildPcbScene(referenceBasic(), opts(["top", "bottom"], o));
const count = (s: Scene, prefix: string) => s.objects.filter((o) => o.key.startsWith(prefix)).length;

describe("layer draw-order stack", () => {
  it("runs bottom side < inner layers (deepest first) < top side < multi-layer < outline < ratsnest < drill", () => {
    const keys = pcbLayers(FOUR, "top").map((l) => l.key);
    expect(keys).toEqual([
      "bottom:zone",
      "bottom:copper",
      "bottom:netNames",
      "bottom:padNumbers",
      "bottom:silk",
      "in2:zone",
      "in2:copper",
      "in2:netNames",
      "in1:zone",
      "in1:copper",
      "in1:netNames",
      "top:zone",
      "top:copper",
      "top:netNames",
      "top:padNumbers",
      "top:silk",
      "multi",
      "multi:padNumbers",
      "outline",
      "ratsnest",
      "drill",
    ]);
  });

  it("draws the far side semi-transparent and marks pad numbers / net names as annotation layers", () => {
    const fromTop = new Map(pcbLayers(FOUR, "top").map((l) => [l.key, l]));
    expect(fromTop.get("bottom:copper")!.alpha).toBe(FAR_SIDE_ALPHA);
    expect(fromTop.get("top:copper")!.alpha).toBe(1);
    expect(fromTop.get("in1:copper")!.alpha).toBe(0.7);
    expect(fromTop.get("multi")!.alpha).toBe(1);
    const fromBottom = new Map(pcbLayers(FOUR, "bottom").map((l) => [l.key, l]));
    expect(fromBottom.get("top:copper")!.alpha).toBe(FAR_SIDE_ALPHA);
    expect(fromBottom.get("bottom:copper")!.alpha).toBe(1);
    expect([...fromTop.values()].filter((l) => l.annotation).map((l) => l.key)).toEqual([
      "bottom:netNames",
      "bottom:padNumbers",
      "in2:netNames",
      "in1:netNames",
      "top:netNames",
      "top:padNumbers",
      "multi:padNumbers",
    ]);
  });
});

describe("render-object table", () => {
  it("carries bbox, hit, layerKey, pathPts, selPoly and pickLayers for the pickable objects", () => {
    const s = buildPcbScene(referenceFourLayer(), opts(FOUR));
    const by = new Map(s.objects.map((o) => [o.key, o]));
    const track = by.get("track:t3")!;
    expect(track.layerKey).toBe("in2:copper");
    expect(track.pathPts).toHaveLength(2);
    expect(track.pickLayers).toEqual(["in2"]);
    const tht = by.get("pad:fp_3:1")!;
    expect(tht.layerKey).toBe("multi");
    expect(tht.pickLayers).toEqual(["top", "in1", "in2", "bottom"]);
    expect(by.get("via:v1")!.pickLayers).toEqual(["top", "in1", "in2"]);
    expect(by.get("footprint:fp_2")!.layerKey).toBe("bottom:silk");
    expect(by.get("footprint:fp_2")!.selPoly).toHaveLength(4);
    expect(by.get("zone:z1")!.pathPts).toHaveLength(4);
    for (const o of s.objects) expect(o.bbox.minX).toBeLessThanOrEqual(o.bbox.maxX);
  });

  it("leaves hidden layers out, as the SVG view does, and keeps a via while one of its layers shows", () => {
    const s = basic({ hidden: new Set(["bottom"]) });
    expect(count(s, "zone:")).toBe(0);
    expect(count(s, "via:")).toBe(1);
    expect(count(s, "track:")).toBe(1);
  });

  it("builds the benchmark board: 2,000 footprints and 20,000 segments", () => {
    const s = buildPcbScene(benchmarkBoard(), opts(FOUR));
    expect(count(s, "footprint:")).toBe(BENCH_FOOTPRINTS);
    expect(count(s, "track:")).toBe(BENCH_SEGMENTS);
  });
});

describe("picking", () => {
  const pk = new PickIndex(basic());
  const at = (x: number, y: number, o: PickOptions = {}) => pk.pick(x, -y, { tol: 0.1, ...o })?.ref ?? null;

  it("returns what the topmost SVG element would be: via > footprint > track > zone", () => {
    expect(at(10, 10)).toEqual({ kind: "footprint", id: "fp_0" });
    expect(at(15, 10)).toEqual({ kind: "track", id: "trk_1" });
    expect(at(15, 15)).toEqual({ kind: "via", id: "via_1" });
    expect(at(5, 5)).toEqual({ kind: "zone", id: "zon_1" });
    expect(at(40, 40)).toBeNull();
    expect(at(10.9, 10, { kinds: ["track"] })).toEqual({ kind: "track", id: "trk_1" });
  });

  it("active-layer-exclusive picking: a miss is a miss, through-hole objects pick on every layer", () => {
    expect(at(5, 5, { activeLayer: "top" })).toBeNull();
    expect(at(15, 10, { activeLayer: "bottom" })).toEqual({ kind: "zone", id: "zon_1" });
    const four = new PickIndex(buildPcbScene(referenceFourLayer(), opts(FOUR)));
    const p4 = (x: number, y: number, activeLayer: string) => four.pick(x, -y, { tol: 0.05, activeLayer })?.ref ?? null;
    expect(p4(17.46, 26.19, "in2")).toEqual({ kind: "footprint", id: "fp_3" });
    expect(p4(26, 22, "bottom")).toBeNull();
    expect(p4(26, 22, "in1")).toEqual({ kind: "via", id: "v1" });
  });

  it("grid index returns each overlapping item once, in draw order", () => {
    const boxes = [
      { minX: 0, minY: 0, maxX: 1, maxY: 1 },
      { minX: 5, minY: 5, maxX: 6, maxY: 6 },
      { minX: -100, minY: -100, maxX: 100, maxY: 100 },
      { minX: 0.5, minY: 0.5, maxX: 5.5, maxY: 5.5 },
    ];
    const g = new GridIndex(boxes, 1);
    expect(g.query({ minX: 0.6, minY: 0.6, maxX: 0.7, maxY: 0.7 })).toEqual([0, 2, 3]);
    expect(g.query({ minX: 200, minY: 200, maxX: 201, maxY: 201 })).toEqual([]);
  });
});

describe("Canvas2D backend: per-layer offscreen cache and dirty rectangles", () => {
  const view = (o: Partial<ViewTransform> = {}): ViewTransform => ({ x0: 0, y0: -20, scale: 20, width: 600, height: 400, dpr: 1, ...o });
  const setup = (scene = basic()) => {
    const log: Call[] = [];
    const target = stubContext(600, 400);
    const r = new Canvas2DRenderer(target.ctx, { createSurface: stubSurfaces(log).create, resolveColour: () => "#123456" });
    r.setScene(scene);
    return { r, target };
  };
  // Non-empty cached layers of the basic board: bottom:zone, top:copper, top:silk, multi, outline, drill.
  const CACHED = 6;

  it("draws every layer once, then composites from cache while the view is unchanged", () => {
    const { r } = setup();
    const first = r.render(view());
    expect(first.fullLayers).toBe(CACHED);
    expect(first.directLayers).toBe(2); // top:netNames and top:padNumbers are drawn live
    expect(r.render(view())).toMatchObject({ fullLayers: 0, dirtyRects: 0, cachedLayers: CACHED });
  });

  it("pans by shifting the cached pixels and redrawing only the exposed strips", () => {
    const { r } = setup();
    r.render(view());
    expect(r.render(view({ x0: -0.25 }))).toMatchObject({ fullLayers: 0, dirtyRects: CACHED }); // 5 px right
    // 10 px left and 10 px down: two strips per layer.
    expect(r.render(view({ x0: 0.25, y0: -20.5 }))).toMatchObject({ fullLayers: 0, dirtyRects: 2 * CACHED });
  });

  it("redraws everything on zoom, and on a sub-pixel pan it cannot reuse", () => {
    const { r } = setup();
    r.render(view());
    expect(r.render(view({ scale: 25 })).fullLayers).toBe(CACHED);
    expect(r.render(view({ scale: 25, x0: 0.013 })).fullLayers).toBe(CACHED);
  });

  it("an edit redraws only the changed object's rectangle on its own layer", () => {
    const { r } = setup();
    r.render(view());
    const snap = referenceBasic();
    snap.project.pcb.tracks.trk_1.b = { x: 19_200_000, y: 12_000_000 };
    r.setScene(buildPcbScene(snap, opts(["top", "bottom"])));
    expect(r.render(view())).toMatchObject({ fullLayers: 0, dirtyRects: 1, cachedLayers: CACHED - 1 });
  });

  it("diffs layers by signature and gives up past the dirty-rectangle limit", () => {
    const a = basic().objects;
    expect(diffBoxes(a, a)).toEqual([]);
    const many: RenderObject[] = Array.from({ length: MAX_DIRTY_RECTS + 1 }, (_, i) => ({ ...a[0], key: `k${i}` }));
    expect(diffBoxes([], many)).toBeNull();
  });

  it("strokes all segments of one colour and width as one path", () => {
    const seg = (i: number): RenderObject => ({
      key: `s${i}`,
      ref: null,
      layerKey: "L",
      bbox: { minX: 0, minY: i, maxX: 10, maxY: i + 0.2 },
      hit: () => false,
      ops: [{ t: "seg", a: [0, i], b: [10, i], width: 0.2, colour: "#c0392b" }],
      sig: String(i),
    });
    const target = stubContext(600, 400);
    const r = new Canvas2DRenderer(target.ctx, { createSurface: null, resolveColour: () => "#000" });
    r.setScene({ layers: [{ key: "L", order: 0, alpha: 1 }], objects: [seg(1), seg(2), seg(3)], bounds: null });
    r.render(view({ y0: 0 }));
    expect(target.calls.filter((c) => c.fn === "stroke")).toHaveLength(1);
    expect(target.calls.filter((c) => c.fn === "moveTo")).toHaveLength(3);
  });

  it("pad numbers are constant-pixel: drawn where they fit the pad, hidden when zoomed out", () => {
    const texts = (scale: number) => {
      const target = stubContext(600, 400);
      const r = new Canvas2DRenderer(target.ctx, { createSurface: null, resolveColour: () => "#000" });
      r.setScene(basic());
      r.render(view({ scale }));
      return target.calls.filter((c) => c.fn === "fillText").map((c) => String(c.args[0]));
    };
    // 20 px/mm: a 0.8 x 0.9 mm pad is 16 x 18 px, room for a 10 px "1"; the 0.25 mm track (5 px)
    // has no room for its net name; the refdes (0.9 mm = 18 px) is shown.
    expect(texts(20).sort()).toEqual(["1", "1", "2", "2", "R1", "R2"]);
    // 5 px/mm: no pad number fits and the refdes is under 5 px.
    expect(texts(5)).toEqual([]);
  });

  it("draws the overlay (selection outline) over the composited layers", () => {
    const { r, target } = setup();
    r.setOverlay({
      items: [
        {
          pts: [
            [8, -11],
            [12, -11],
            [12, -9],
            [8, -9],
          ],
          closed: true,
          colour: "--accent",
          px: 2,
        },
      ],
    });
    r.render(view());
    const fns = target.calls.map((c) => c.fn);
    expect(fns.lastIndexOf("stroke")).toBeGreaterThan(fns.lastIndexOf("drawImage"));
    expect(fns).toContain("setLineDash");
  });
});
