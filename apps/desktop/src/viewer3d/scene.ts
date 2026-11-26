// 3D board scene. Pure data, no three.js: the viewer renders it and it is never electrical truth.
// Copper, zone fills, holes, outline, cut-outs, silkscreen and bodies come from the engine's board
// geometry (`board_geometry`, ADR-0052: the geometry DRC checks); until the geometry of the
// snapshot's revision arrives they are derived from the snapshot as before. Nothing is invented: a part without a body height in
// its library definition is listed in `missing`, not drawn with a guessed height.
import { layerColour, layerKey, layerName, stackLayers } from "../layers";
import type { BoardGeometry, ComponentDef, CopperItem, CopperLayer, ModelQuality, ModelRef, Point, Snapshot } from "../types";

const MM = 1e6;
const toMm = (nm: number) => nm / MM;
/** Copper thickness per ounce, in mm. */
const OZ_MM = 0.035;

export interface SceneLayer {
  layer: CopperLayer;
  key: string;
  name: string;
  colour: string;
  /** Slab centre height in mm; the board is centred on y = 0, top surface at +thickness/2. */
  y: number;
  /** Copper slab thickness in mm. */
  thickness: number;
}
export interface SceneTrack { layer: string; ax: number; ay: number; bx: number; by: number; width: number }
export interface ScenePad { layer: string; shape: "rect" | "circle" | "oval"; x: number; y: number; sx: number; sy: number }
export interface SceneVia { x: number; y: number; diameter: number; drill: number; yTop: number; yBottom: number }
export interface SceneZone { layer: string; outline: [number, number][] }
/** One zone's engine fill on a layer: [x0, y0, x1, y1] rectangles in mm (the DRC raster). */
export interface SceneFill { layer: string; zoneId: string; rects: [number, number, number, number][] }
/** One silkscreen legend stroke (the fabricated, mask-clipped legend), mm. */
export interface SceneSilk { side: "top" | "bottom"; ax: number; ay: number; bx: number; by: number; width: number }
export interface SceneBody { footprintId: string; refdes: string; side: "top" | "bottom"; x: number; y: number; sx: number; sy: number; height: number }
/** Every placed part and its library model reference (null = no model attached). */
export interface ScenePart { footprintId: string; refdes: string; componentId: string; side: "top" | "bottom"; x: number; y: number; rotationDeg: number; model: ModelRef | null }

/** Shown per part: a declared quality, UNCLASSIFIED (model attached, trust not declared) or MISSING (no usable model; any box is an approximation). */
export type ModelStatus = ModelQuality | "UNCLASSIFIED" | "MISSING";
export function modelStatus(model: ModelRef | null, loaded: boolean): ModelStatus {
  if (!model || !loaded) return "MISSING";
  return model.quality ?? "UNCLASSIFIED";
}

export interface SceneCourtyard { side: "top" | "bottom"; x: number; y: number; hw: number; hh: number }

export interface BoardScene {
  outline: [number, number][];
  thickness: number;
  layers: SceneLayer[];
  tracks: SceneTrack[];
  pads: ScenePad[];
  vias: SceneVia[];
  zones: SceneZone[];
  /** Engine zone fills (empty until the board geometry arrives). */
  fills: SceneFill[];
  /** Engine silkscreen legend (empty until the board geometry arrives). */
  silk: SceneSilk[];
  /** Board cut-outs (holes through the slab), mm. */
  cutouts: [number, number][][];
  /** "engine": drawn from the board geometry DRC checks; "snapshot": geometry not loaded yet. */
  source: "engine" | "snapshot";
  bodies: SceneBody[];
  parts: ScenePart[];
  courtyards: SceneCourtyard[];
  /** Reference designators whose library definition has no body height (not drawn). */
  missing: string[];
  /** Bounding box of everything (mm): [minX, minY, maxX, maxY]. */
  bounds: [number, number, number, number];
}

const rotDeg = (r: unknown) => ({ R90: 90, R180: 180, R270: 270 } as Record<string, number>)[String(r)] ?? 0;
const pt = (p: Point): [number, number] => [toMm(p.x), toMm(p.y)];

/**
 * Centre heights per copper layer: outer layers sit on the surfaces; inner ones at their depth in the
 * stack-up (copper plus the dielectric gap after each layer, as `SetStackup` sets them, scaled to the
 * board thickness), spread evenly when no gaps are given. `colourOf` is the PCB layer manager's
 * colour (user override or default), so 3D and PCB agree.
 */
export function layerHeights(layers: CopperLayer[], thickness: number, copper: number[], colourOf: (l: CopperLayer) => string = layerColour, gaps: number[] = []): SceneLayer[] {
  const n = layers.length;
  const ct = layers.map((_, i) => Math.max(copper[i] ?? OZ_MM, 0.02));
  const stack = ct.reduce((a, b) => a + b, 0) + gaps.slice(0, n - 1).reduce((a, b) => a + Math.max(b, 0), 0);
  // Without declared dielectric gaps (older stack-ups) the inner layers are spread evenly.
  const useGaps = gaps.length >= n - 1 && gaps.slice(0, n - 1).some((g) => g > 0) && stack > 0;
  let depth = 0;
  return layers.map((l, i) => {
    let y: number;
    if (i === 0) y = thickness / 2 + ct[i] / 2;
    else if (i === n - 1) y = -thickness / 2 - ct[i] / 2;
    else if (useGaps) y = thickness / 2 - ((depth + ct[i] / 2) * thickness) / stack;
    else y = thickness / 2 - (thickness * i) / (n - 1);
    depth += ct[i] + Math.max(gaps[i] ?? 0, 0);
    return { layer: l, key: layerKey(l), name: layerName(l), colour: colourOf(l), y, thickness: ct[i] };
  });
}

/** Gap (mm) between neighbouring copper layers at full explode. */
export const EXPLODE_GAP_MM = 4;

/**
 * The scene with its copper layers pulled apart symmetrically about the board centre: `factor` 0 is
 * the real stack-up, 1 separates neighbouring layers by `EXPLODE_GAP_MM`. Vias stretch with the
 * layers they join; parts, silkscreen and courtyards follow the outer layers. View only.
 */
export function explodeScene(sc: BoardScene, factor: number): BoardScene {
  const f = Math.min(Math.max(factor, 0), 1);
  if (f === 0 || sc.layers.length < 2) return sc;
  const mid = (sc.layers.length - 1) / 2;
  const shift = sc.layers.map((_, i) => f * EXPLODE_GAP_MM * (mid - i));
  const near = (y: number) => {
    let best = 0;
    sc.layers.forEach((l, i) => {
      if (Math.abs(l.y - y) < Math.abs(sc.layers[best].y - y)) best = i;
    });
    return shift[best];
  };
  return {
    ...sc,
    layers: sc.layers.map((l, i) => ({ ...l, y: l.y + shift[i] })),
    vias: sc.vias.map((v) => ({ ...v, yTop: v.yTop + near(v.yTop), yBottom: v.yBottom + near(v.yBottom) })),
  };
}

/** A pad's copper item as a scene pad: polygon = rectangle, disc = circle, stadium = oval. */
function padOf(i: CopperItem, layer: string): ScenePad {
  const r = toMm(i.shape.radius);
  if ("polygon" in i.shape.core) {
    const xs = i.shape.core.polygon.map((q) => toMm(q.x));
    const ys = i.shape.core.polygon.map((q) => toMm(q.y));
    const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
    return { layer, shape: "rect", x: (x0 + x1) / 2, y: (y0 + y1) / 2, sx: x1 - x0 + 2 * r, sy: y1 - y0 + 2 * r };
  }
  const [a, b] = i.shape.core.segment;
  const [ax, ay, bx, by] = [toMm(a.x), toMm(a.y), toMm(b.x), toMm(b.y)];
  const round = ax === bx && ay === by;
  return { layer, shape: round ? "circle" : "oval", x: (ax + bx) / 2, y: (ay + by) / 2, sx: Math.abs(bx - ax) + 2 * r, sy: Math.abs(by - ay) + 2 * r };
}

/**
 * The 3D scene of a snapshot. `g` is the engine board geometry; it is used when it belongs to the
 * snapshot's revision (a stale one is ignored, never mixed with a newer snapshot).
 */
export function buildScene(s: Snapshot, colourOf: (l: CopperLayer) => string = layerColour, g: BoardGeometry | null = null): BoardScene {
  const p = s.project;
  const geo = g && g.revision === p.revision ? g : null;
  const stackup = p.pcb.board.stackup;
  const order = stackLayers(stackup).map((l) => l.layer);
  const thickness = geo ? geo.thickness_mm : stackup && stackup.board_thickness > 0 ? toMm(stackup.board_thickness) : 1.6;
  const copper = order.map((l) => {
    const sl = stackup?.layers.find((x) => layerKey(x.layer) === layerKey(l));
    return (sl?.copper_oz ?? 1) * OZ_MM;
  });
  const gaps = order.map((l) => (stackup?.layers.find((x) => layerKey(x.layer) === layerKey(l))?.dielectric_after_um ?? 0) / 1000);
  const layers = layerHeights(order, thickness, copper, colourOf, stackup?.layers.length ? gaps : []);
  const byKey = new Map(layers.map((l) => [l.key, l]));
  const top = layers[0];
  const bottom = layers[layers.length - 1];
  const sideLayer = (side: "top" | "bottom") => (side === "top" ? top : bottom);

  // Engine copper: every track chord (arc tracks arrive flattened), pad and via barrel per layer.
  const items = (geo?.copper ?? []).filter((i) => byKey.has(layerKey(i.layer)));
  const tracks: SceneTrack[] = geo
    ? items.flatMap((i) => {
        if (i.kind !== "track" || !("segment" in i.shape.core)) return [];
        const [[ax, ay], [bx, by]] = i.shape.core.segment.map(pt);
        return [{ layer: layerKey(i.layer), ax, ay, bx, by, width: toMm(2 * i.shape.radius) }];
      })
    : Object.values(p.pcb.tracks)
        .filter((t) => byKey.has(layerKey(t.layer)))
        .map((t) => {
          const [ax, ay] = pt(t.a);
          const [bx, by] = pt(t.b);
          return { layer: layerKey(t.layer), ax, ay, bx, by, width: toMm(t.width) };
        });

  const pads: ScenePad[] = geo ? items.filter((i) => i.kind === "pad").map((i) => padOf(i, layerKey(i.layer))) : [];
  const bodies: SceneBody[] = [];
  const courtyards: SceneCourtyard[] = [];
  const missing: string[] = geo ? [...geo.missing_models] : [];
  const parts: ScenePart[] = [];
  for (const fp of s.geometry.footprints) {
    const [cx, cy] = pt(fp.position);
    courtyards.push({ side: fp.side, x: cx, y: cy, hw: toMm(fp.half_w), hh: toMm(fp.half_h) });
    if (!geo)
      for (const pad of fp.pads) {
        const [x, y] = pt(pad.position);
        const base = { shape: pad.shape, x, y, sx: toMm(pad.size_x), sy: toMm(pad.size_y) };
        // A through-hole pad is copper on both outer layers; an SMD pad only on its footprint's side.
        if (pad.kind === "tht") {
          pads.push({ ...base, layer: top.key });
          pads.push({ ...base, layer: bottom.key });
        } else pads.push({ ...base, layer: sideLayer(fp.side).key });
      }
    const inst = p.pcb.footprints[fp.id];
    const def = inst ? (p.library?.[inst.component_id] as unknown as ComponentDef | undefined) : undefined;
    const model = (def as unknown as { model3d?: { model_ref?: ModelRef | null } } | undefined)?.model3d?.model_ref ?? null;
    parts.push({ footprintId: fp.id, refdes: fp.refdes, componentId: inst?.component_id ?? "", side: fp.side, x: cx, y: cy, rotationDeg: rotDeg(inst?.rotation), model });
    if (geo) continue;
    const h = def?.footprint?.body_height;
    if (h == null || h <= 0) {
      missing.push(fp.refdes);
      continue;
    }
    // Body footprint approximated as 85 % of the courtyard (same rule as eak-engineering scene3d).
    bodies.push({ footprintId: fp.id, refdes: fp.refdes, side: fp.side, x: cx, y: cy, sx: toMm(fp.half_w) * 2 * 0.85, sy: toMm(fp.half_h) * 2 * 0.85, height: toMm(h) });
  }
  // Engine bodies (scene3d::build): a part whose library has no body height is in `missing`.
  for (const b of geo?.bodies ?? []) bodies.push({ footprintId: b.footprint_id, refdes: b.refdes, side: b.side, x: b.x, y: b.y, sx: b.sx, sy: b.sy, height: b.sz });

  const span = (keys: string[], x: number, y: number, diameter: number, drill: number): SceneVia => {
    const ls = keys.map((k) => byKey.get(k)).filter((l): l is SceneLayer => !!l);
    const hi = Math.max(...ls.map((l) => l.y + l.thickness / 2));
    const lo = Math.min(...ls.map((l) => l.y - l.thickness / 2));
    return { x, y, diameter, drill, yTop: hi, yBottom: lo };
  };
  let vias: SceneVia[];
  if (geo) {
    const barrels = new Map<string, { at: Point; d: number; keys: string[] }>();
    for (const i of items.filter((it) => it.kind === "via" && "segment" in it.shape.core)) {
      const at = (i.shape.core as { segment: [Point, Point] }).segment[0];
      const b = barrels.get(i.object_id) ?? { at, d: 2 * i.shape.radius, keys: [] };
      b.keys.push(layerKey(i.layer));
      barrels.set(i.object_id, b);
    }
    const drillOf = new Map(geo.holes.filter((h) => h.pad === "").map((h) => [h.object_id, h.drill]));
    vias = [...barrels].map(([id, b]) => span(b.keys, toMm(b.at.x), toMm(b.at.y), toMm(b.d), toMm(drillOf.get(id) ?? 0)));
  } else
    vias = Object.values(p.pcb.vias).map((v) => {
      const [x, y] = pt(v.position);
      const a = byKey.get(layerKey(v.from ?? "top")) ?? top;
      const b = byKey.get(layerKey(v.to ?? "bottom")) ?? bottom;
      return span([a.key, b.key], x, y, toMm(v.diameter), toMm(v.drill));
    });

  // Zone outlines from the IR; their copper is the engine fill (the cells DRC checks).
  const zones: SceneZone[] = Object.values(p.pcb.zones)
    .filter((z) => byKey.has(layerKey(z.layer)))
    .map((z) => ({ layer: layerKey(z.layer), outline: z.outline.map(pt) }));
  const fills: SceneFill[] = (geo?.fill_views ?? [])
    .filter((f) => byKey.has(layerKey(f.layer)))
    .map((f) => ({ layer: layerKey(f.layer), zoneId: f.zone_id, rects: f.rects.map(([x0, y0, x1, y1]) => [toMm(x0), toMm(y0), toMm(x1), toMm(y1)] as [number, number, number, number]) }));
  const silk: SceneSilk[] = geo
    ? (["top", "bottom"] as const).flatMap((side) =>
        geo.silk[side].map((st) => {
          const [ax, ay] = pt(st.a);
          const [bx, by] = pt(st.b);
          return { side, ax, ay, bx, by, width: toMm(st.width) };
        }),
      )
    : [];

  const outline = (geo ? geo.outline : p.pcb.board.outline).map(pt);
  const cutouts = (geo?.cutouts ?? []).map((c) => c.map(pt));
  const xs: number[] = [];
  const ys: number[] = [];
  const add = (x: number, y: number) => {
    xs.push(x);
    ys.push(y);
  };
  outline.forEach(([x, y]) => add(x, y));
  if (outline.length < 3) {
    // No usable outline: frame the footprints and tracks instead (nothing is drawn for the slab).
    courtyards.forEach((c) => {
      add(c.x - c.hw, c.y - c.hh);
      add(c.x + c.hw, c.y + c.hh);
    });
    tracks.forEach((t) => {
      add(t.ax, t.ay);
      add(t.bx, t.by);
    });
  }
  const bounds: [number, number, number, number] = xs.length ? [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)] : [0, 0, 10, 10];
  return { outline, thickness, layers, tracks, pads, vias, zones, fills, silk, cutouts, source: geo ? "engine" : "snapshot", bodies, parts, courtyards, missing, bounds };
}
