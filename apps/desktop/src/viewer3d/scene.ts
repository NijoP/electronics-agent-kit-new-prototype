// 3D board scene derived from the engine snapshot (EAK-IR). Pure data, no three.js: the viewer
// renders it and it is never electrical truth. Nothing is invented: a part without a body height in
// its library definition is listed in `missing`, not drawn with a guessed height.
import { layerColour, layerKey, layerName, stackLayers } from "../layers";
import type { ComponentDef, CopperLayer, ModelQuality, ModelRef, Point, Snapshot } from "../types";

const MM = 1e6;
const toMm = (nm: number) => nm / MM;
/** Copper thickness per ounce, in mm. */
const OZ_MM = 0.035;

export interface SceneLayer {
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
 * Centre heights per copper layer: outer layers sit on the surfaces, inner ones are spread evenly.
 * `colourOf` is the PCB layer manager's colour (user override or default), so 3D and PCB agree.
 */
export function layerHeights(layers: CopperLayer[], thickness: number, copper: number[], colourOf: (l: CopperLayer) => string = layerColour): SceneLayer[] {
  const n = layers.length;
  return layers.map((l, i) => {
    const ct = Math.max(copper[i] ?? OZ_MM, 0.02);
    let y: number;
    if (i === 0) y = thickness / 2 + ct / 2;
    else if (i === n - 1) y = -thickness / 2 - ct / 2;
    else y = thickness / 2 - (thickness * i) / (n - 1);
    return { key: layerKey(l), name: layerName(l), colour: colourOf(l), y, thickness: ct };
  });
}

export function buildScene(s: Snapshot, colourOf: (l: CopperLayer) => string = layerColour): BoardScene {
  const p = s.project;
  const stackup = p.pcb.board.stackup;
  const order = stackLayers(stackup).map((l) => l.layer);
  const thickness = stackup && stackup.board_thickness > 0 ? toMm(stackup.board_thickness) : 1.6;
  const copper = order.map((l) => {
    const sl = stackup?.layers.find((x) => layerKey(x.layer) === layerKey(l));
    return (sl?.copper_oz ?? 1) * OZ_MM;
  });
  const layers = layerHeights(order, thickness, copper, colourOf);
  const byKey = new Map(layers.map((l) => [l.key, l]));
  const top = layers[0];
  const bottom = layers[layers.length - 1];
  const sideLayer = (side: "top" | "bottom") => (side === "top" ? top : bottom);

  const tracks: SceneTrack[] = Object.values(p.pcb.tracks)
    .filter((t) => byKey.has(layerKey(t.layer)))
    .map((t) => {
      const [ax, ay] = pt(t.a);
      const [bx, by] = pt(t.b);
      return { layer: layerKey(t.layer), ax, ay, bx, by, width: toMm(t.width) };
    });

  const pads: ScenePad[] = [];
  const bodies: SceneBody[] = [];
  const courtyards: SceneCourtyard[] = [];
  const missing: string[] = [];
  const parts: ScenePart[] = [];
  for (const fp of s.geometry.footprints) {
    const [cx, cy] = pt(fp.position);
    courtyards.push({ side: fp.side, x: cx, y: cy, hw: toMm(fp.half_w), hh: toMm(fp.half_h) });
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
    const h = def?.footprint?.body_height;
    if (h == null || h <= 0) {
      missing.push(fp.refdes);
      continue;
    }
    // Body footprint approximated as 85 % of the courtyard (same rule as eak-engineering scene3d).
    bodies.push({ footprintId: fp.id, refdes: fp.refdes, side: fp.side, x: cx, y: cy, sx: toMm(fp.half_w) * 2 * 0.85, sy: toMm(fp.half_h) * 2 * 0.85, height: toMm(h) });
  }

  const vias: SceneVia[] = Object.values(p.pcb.vias).map((v) => {
    const [x, y] = pt(v.position);
    const a = byKey.get(layerKey(v.from ?? "top")) ?? top;
    const b = byKey.get(layerKey(v.to ?? "bottom")) ?? bottom;
    const hi = Math.max(a.y + a.thickness / 2, b.y + b.thickness / 2);
    const lo = Math.min(a.y - a.thickness / 2, b.y - b.thickness / 2);
    return { x, y, diameter: toMm(v.diameter), drill: toMm(v.drill), yTop: hi, yBottom: lo };
  });

  // Zone fills (engine-computed, drawn in the PCB editor) are not drawn in 3D: outlines only.
  const zones: SceneZone[] = Object.values(p.pcb.zones)
    .filter((z) => byKey.has(layerKey(z.layer)))
    .map((z) => ({ layer: layerKey(z.layer), outline: z.outline.map(pt) }));

  const outline = p.pcb.board.outline.map(pt);
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
  return { outline, thickness, layers, tracks, pads, vias, zones, bodies, parts, courtyards, missing, bounds };
}
