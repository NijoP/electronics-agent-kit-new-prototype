// Unrun (no builds or tests during the testing freeze): scene builder for the 3D viewer.
import { describe, expect, it } from "vitest";
import { buildScene, layerHeights } from "./scene";
import type { Snapshot } from "../types";

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
  it("places outer copper on the surfaces and inner copper inside", () => {
    const ls = layerHeights(["top", { inner: 1 }, { inner: 2 }, "bottom"], 1.6, [0.035, 0.035, 0.035, 0.035]);
    expect(ls[0].y).toBeGreaterThan(0.8);
    expect(ls[3].y).toBeLessThan(-0.8);
    expect(Math.abs(ls[1].y)).toBeLessThan(0.8);
  });
});
