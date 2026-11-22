// Picking from the render-object table: coarse grid lookup on `bbox`, then the object's precise
// `hit`. It replaces DOM hit-testing (`closest("[data-footprint-id]")`) when the PCB is drawn on
// a canvas, and returns what the SVG view's topmost element would have been:
//   vias > footprints (and their pads) > tracks > zones,
// the stacking order of the SVG markup, ties broken by draw order (later wins).
import { GridIndex } from "./geometry";
import type { PickHit, PickKind, PickOptions, RenderObject, Scene } from "./types";

const PRIORITY: Record<PickKind, number> = { zone: 1, track: 2, footprint: 3, via: 4 };

export class PickIndex {
  private readonly items: RenderObject[];
  private readonly rank: number[];
  private readonly grid: GridIndex;

  constructor(scene: Scene) {
    const order = new Map(scene.layers.map((l) => [l.key, l.order]));
    const indexed = scene.objects
      .map((o, i) => ({ o, i }))
      .filter(({ o }) => o.ref !== null)
      .map(({ o, i }) => ({ o, r: (order.get(o.layerKey) ?? 0) * 1e7 + i }));
    this.items = indexed.map((x) => x.o);
    this.rank = indexed.map((x) => x.r);
    this.grid = new GridIndex(this.items.map((o) => o.bbox));
  }

  pick(x: number, y: number, opts: PickOptions = {}): PickHit | null {
    const tol = opts.tol ?? 0;
    let best: { hit: PickHit; p: number; r: number } | null = null;
    for (const i of this.grid.query({ minX: x - tol, minY: y - tol, maxX: x + tol, maxY: y + tol })) {
      const o = this.items[i];
      const ref = o.ref;
      if (!ref) continue;
      if (opts.kinds && !opts.kinds.includes(ref.kind)) continue;
      if (opts.activeLayer !== undefined && o.pickLayers && !o.pickLayers.includes(opts.activeLayer)) continue;
      if (!o.hit(x, y, tol)) continue;
      const p = PRIORITY[ref.kind];
      const r = this.rank[i];
      if (!best || p > best.p || (p === best.p && r > best.r)) best = { hit: { ref, object: o }, p, r };
    }
    return best?.hit ?? null;
  }
}

/** Map a DOM pick attribute (as `PcbView` asks for it) to a pick kind. */
export const ATTR_KIND: Record<string, PickKind> = {
  "data-footprint-id": "footprint",
  "data-track-id": "track",
  "data-via-id": "via",
  "data-zone-id": "zone",
};
