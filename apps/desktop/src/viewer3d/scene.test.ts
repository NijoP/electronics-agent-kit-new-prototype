// Scene builder for the 3D viewer.
import { describe, expect, it } from "vitest";
import { buildScene, EXPLODE_GAP_MM, explodeScene, layerHeights, modelStatus } from "./scene";
import type { BoardGeometry, CopperItem, Snapshot } from "../types";

const MM = 1_000_000;
const snap = (over: Partial<Snapshot["project"]["pcb"]> = {}, libH: number | null = 1_000_000): Snapshot =>
  ({
    project: {
      pcb: {
        board: { outline: [{ x: 0, y: 0 }, { x: 10 * MM, y: 0 }, { x: 10 * MM, y: 5 * MM }, { x: 0, y: 5 * MM }], stackup: { layers: [], board_thickness: 1_600_000, dielectric: "FR-4" } },
        footprints: { f1: { id: "f1", symbol_id: "s1", component_id: "c1", refdes: "R1", position: { x: 5 * MM, y: 2 * MM }, rotation: "R0", side: "top", locked: false } },
        tracks: { t1: { id: "t1", layer: "top", width: 200_000, a: { x: 0, y: 0 }, b: { x: MM, y: 0 }, net: "N" } },
        vias: {},
        zones: {},
        ...over,
      },
      library: { c1: { footprint: { body_height: libH } } },
    },
    geometry: { symbols: [], footprints: [{ id: "f1", symbol_id: "s1", refdes: "R1", side: "top", position: { x: 5 * MM, y: 2 * MM }, half_w: MM, half_h: 500_000, pads: [{ number: "1", kind: "smd", shape: "rect", position: { x: 4.5 * MM, y: 2 * MM }, size_x: 600_000, size_y: 600_000, drill: null, net: null }] }] },
  }) as unknown as Snapshot;

describe("buildScene", () => {
  it("builds a 2-layer board with thickness, track, pad and body", () => {
    const sc = buildScene(snap());
    expect(sc.thickness).toBeCloseTo(1.6);
    expect(sc.layers.map((l) => l.key)).toEqual(["top", "bottom"]);
    expect(sc.tracks).toHaveLength(1);
    expect(sc.pads[0].layer).toBe("top");
    expect(sc.bodies[0]).toMatchObject({ refdes: "R1", height: 1 });
    expect(sc.bounds).toEqual([0, 0, 10, 5]);
  });
  it("lists a part without a body height instead of inventing one", () => {
    const sc = buildScene(snap({}, null));
    expect(sc.bodies).toHaveLength(0);
    expect(sc.missing).toEqual(["R1"]);
  });
  it("reports every part with its model reference and a MISSING status when none loaded", () => {
    const ref = { source: { kind: "file" as const, path: "/m/r.step" }, format: "step" as const, transform: { offset_mm: [0, 0, 0] as [number, number, number], rotation_deg: [0, 0, 0] as [number, number, number], scale: [1, 1, 1] as [number, number, number] }, licence: null, provenance: "p" };
    const s = snap();
    (s.project.library as Record<string, unknown>).c1 = { footprint: { body_height: 1_000_000 }, model3d: { kind: "box", color: "#000", external_ref: null, model_ref: ref } };
    const sc = buildScene(s);
    expect(sc.parts[0]).toMatchObject({ footprintId: "f1", componentId: "c1", rotationDeg: 0, model: ref });
    expect(modelStatus(sc.parts[0].model, false)).toBe("MISSING");
    expect(modelStatus(ref, true)).toBe("UNCLASSIFIED");
    expect(modelStatus({ ...ref, quality: "VENDOR_EXACT" }, true)).toBe("VENDOR_EXACT");
    expect(modelStatus(null, true)).toBe("MISSING");
  });
  it("E1: draws copper, fills, holes, silk, cut-outs and bodies from the engine board geometry of the same revision", () => {
    const s = snap();
    (s.project as { revision: number }).revision = 4;
    const item = (kind: CopperItem["kind"], id: string, layer: CopperItem["layer"], core: CopperItem["shape"]["core"], radius: number, sub = ""): CopperItem => ({ kind, object_id: id, sub, layer, net: "N", shape: { core, radius } });
    const P = (x: number, y: number) => ({ x: x * MM, y: y * MM });
    const g: BoardGeometry = {
      revision: 4,
      // An arc edge arrives flattened: more vertices than the four IR corners.
      outline: [P(0, 0), P(10, 0), P(10, 5), P(5, 6), P(0, 5)],
      cutouts: [[P(1, 1), P(2, 1), P(2, 2)]],
      thickness: 1_000_000,
      copper: [
        // An arc track arrives as chords: two segments of one track.
        item("track", "t1", "top", { segment: [P(0, 0), P(1, 0.5)] }, 100_000),
        item("track", "t1", "top", { segment: [P(1, 0.5), P(2, 0)] }, 100_000),
        item("pad", "f1", "top", { polygon: [P(4, 1.5), P(5, 1.5), P(5, 2.5), P(4, 2.5)] }, 0, "1"),
        item("pad", "f1", "bottom", { segment: [P(6, 2), P(6, 2)] }, 400_000, "2"),
        item("via", "v1", "top", { segment: [P(8, 3), P(8, 3)] }, 300_000),
        item("via", "v1", "bottom", { segment: [P(8, 3), P(8, 3)] }, 300_000),
      ],
      fill_views: [{ zone_id: "z1", layer: "bottom", net: "GND", islands: 1, removed_islands: 0, filled_area_mm2: 2, pitch: 100_000, rects: [[0, 0, 2 * MM, MM]] }],
      holes: [{ object_id: "v1", pad: "", label: "via v1", center: P(8, 3), drill: 300_000, net: "N" }],
      silk: { top: [{ a: P(1, 4), b: P(3, 4), width: 160_000 }], bottom: [] },
      outline_mm: [],
      thickness_mm: 1,
      bodies: [{ footprint_id: "f1", symbol_id: "s1", refdes: "R1", side: "top", x: 5, y: 2, z: 0.5, sx: 1.7, sy: 0.85, sz: 1, kind: "box", color: "#000" }],
      missing_models: [],
    };
    const sc = buildScene(s, undefined, g);
    expect(sc.source).toBe("engine");
    expect(sc.thickness).toBe(1);
    expect(sc.outline).toHaveLength(5);
    expect(sc.cutouts).toEqual([[[1, 1], [2, 1], [2, 2]]]);
    expect(sc.tracks.map((t) => [t.ax, t.bx, t.width])).toEqual([[0, 1, 0.2], [1, 2, 0.2]]);
    expect(sc.pads).toEqual([
      { layer: "top", shape: "rect", x: 4.5, y: 2, sx: 1, sy: 1 },
      { layer: "bottom", shape: "circle", x: 6, y: 2, sx: 0.8, sy: 0.8 },
    ]);
    expect(sc.vias).toHaveLength(1);
    expect(sc.vias[0]).toMatchObject({ x: 8, y: 3, diameter: 0.6, drill: 0.3 });
    expect(sc.vias[0].yTop).toBeGreaterThan(0.5);
    expect(sc.vias[0].yBottom).toBeLessThan(-0.5);
    expect(sc.fills).toEqual([{ layer: "bottom", zoneId: "z1", rects: [[0, 0, 2, 1]] }]);
    expect(sc.silk).toEqual([{ side: "top", ax: 1, ay: 4, bx: 3, by: 4, width: 0.16 }]);
    expect(sc.bodies).toEqual([{ footprintId: "f1", refdes: "R1", side: "top", x: 5, y: 2, sx: 1.7, sy: 0.85, height: 1 }]);
    // A geometry of another revision is never mixed with this snapshot.
    const stale = buildScene(s, undefined, { ...g, revision: 3 });
    expect(stale.source).toBe("snapshot");
    expect(stale.fills).toEqual([]);
    expect(stale.tracks).toHaveLength(1);
  });

  it("W2-K: inner layers sit at their stack-up depth (SetStackup gaps), scaled to the board", () => {
    // 0.2 mm prepreg, 1.2 mm core, 0.2 mm prepreg: In1 near the top, In2 near the bottom.
    const ls = layerHeights(["top", { inner: 1 }, { inner: 2 }, "bottom"], 1.6, [0, 0, 0, 0].map(() => 0.035), undefined, [0.2, 1.2, 0.2, 0]);
    expect(ls[1].y).toBeGreaterThan(0.5);
    expect(ls[2].y).toBeLessThan(-0.5);
    expect(ls[1].y + ls[2].y).toBeCloseTo(0, 6);
    expect(ls.map((l) => l.layer)).toEqual(["top", { inner: 1 }, { inner: 2 }, "bottom"]);
    // No gaps declared: even spread, as before.
    const even = layerHeights(["top", { inner: 1 }, { inner: 2 }, "bottom"], 1.6, [0.035, 0.035, 0.035, 0.035]);
    expect(even[1].y).toBeCloseTo(1.6 / 2 - 1.6 / 3, 6);
  });
  it("W2-K: explode pulls the copper layers apart symmetrically and stretches vias with them", () => {
    const sc = buildScene(snap({ vias: { v1: { id: "v1", position: { x: MM, y: MM }, drill: 300_000, diameter: 600_000, net: "N", from: "top", to: "bottom" } } }));
    expect(explodeScene(sc, 0)).toBe(sc);
    const ex = explodeScene(sc, 1);
    expect(ex.layers[0].y - sc.layers[0].y).toBeCloseTo(EXPLODE_GAP_MM / 2, 6);
    expect(ex.layers[1].y - sc.layers[1].y).toBeCloseTo(-EXPLODE_GAP_MM / 2, 6);
    expect(ex.vias[0].yTop - ex.vias[0].yBottom).toBeCloseTo(sc.vias[0].yTop - sc.vias[0].yBottom + EXPLODE_GAP_MM, 6);
    expect(ex.bounds).toEqual(sc.bounds);
    expect(explodeScene(sc, 5).layers[0].y).toBeCloseTo(ex.layers[0].y, 6);
  });

  it("places outer copper on the surfaces and inner copper inside", () => {
    const ls = layerHeights(["top", { inner: 1 }, { inner: 2 }, "bottom"], 1.6, [0.035, 0.035, 0.035, 0.035]);
    expect(ls[0].y).toBeGreaterThan(0.8);
    expect(ls[3].y).toBeLessThan(-0.8);
    expect(Math.abs(ls[1].y)).toBeLessThan(0.8);
  });
});
