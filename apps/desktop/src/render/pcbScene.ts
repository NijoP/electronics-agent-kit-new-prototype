// PCB scene builder: snapshot -> render-object table + layer draw-order stack. Pure, so the
// same table drives the Canvas2D backend, picking and the parity tests.
//
// Draw order, back to front (re-implemented from the EasyEDA viewer's PCB stack; EAK has no mask,
// paste or assembly layers yet, so those slots are empty):
//   bottom side: zone < copper (tracks, SMD pads) < net names < pad numbers < silk (courtyard, refdes)
//   < inner layers, deepest first: zone < copper < net names
//   < top side: zone < copper < net names < pad numbers < silk
//   < multi-layer (through-hole pads, vias) < multi-layer pad numbers
//   < board outline < ratsnest < drill holes on top.
// Layers on the far side of the active copper layer are drawn semi-transparent.
import { layerKey, viaLayers } from "../layers";
import type { CopperLayer, FillView, Point, SilkStroke, Snapshot } from "../types";
import { mm, toView } from "../viewport";
import { boxOfPts, grow, inBox, inPoly, polyEdgeDist, rectPoly, segDist, union } from "./geometry";
import type { Box, DrawOp, Pt, RenderLayer, RenderObject, Scene } from "./types";
import { dimensionDrawing, outlinePath, trackPath } from "./pcbObjects";

export interface PcbSceneOptions {
  /** Copper layers in stack order, top first. */
  order: CopperLayer[];
  /**
   * Hidden layer keys: copper layers (their zones, tracks and net names are left out, as in the SVG
   * view) and "outline" (the board edge).
   */
  hidden: Set<string>;
  colourOf: (l: CopperLayer) => string;
  /** Active copper layer key: sets which board side is drawn opaque. */
  activeLayer: string;
  /** Unrouted connections to draw as ratsnest, or null to hide them. */
  airwires: [string, Point, Point][] | null;
  /** Outline view (slice C1): tracks, pads, vias and zones drawn as outlines, not filled. */
  outlineView?: boolean;
  /**
   * Engine zone fills for this revision (slice E5): each zone is drawn as its outline plus the
   * copper the engine fill leaves (the DRC raster). Absent or null: the zone area is drawn
   * translucent, as before the fill is known.
   */
  fills?: FillView[] | null;
  /**
   * The engine silkscreen legend for this revision (ADR-0051: the legend DRC and the Gerber use).
   * When present it is drawn on each side's silk in place of the plain refdes label; absent or
   * null: the refdes label is drawn as before.
   */
  silk?: { top: SilkStroke[]; bottom: SilkStroke[] } | null;
}

/** Outline-view line width in view mm. */
export const OUTLINE_VIEW_MM = 0.05;

/** The outline-view form of a drawing op: fills become strokes, a track its rectangle outline. */
export function outlineOp(op: DrawOp): DrawOp {
  switch (op.t) {
    case "seg": {
      const [ax, ay] = op.a;
      const [bx, by] = op.b;
      const len = Math.hypot(bx - ax, by - ay) || 1;
      const h = op.width / 2;
      const ux = ((bx - ax) / len) * h;
      const uy = ((by - ay) / len) * h;
      const pts: Pt[] = [
        [ax - ux - uy, ay - uy + ux],
        [bx + ux - uy, by + uy + ux],
        [bx + ux + uy, by + uy - ux],
        [ax - ux + uy, ay - uy - ux],
      ];
      return { t: "poly", pts, stroke: op.colour, lineWidth: OUTLINE_VIEW_MM, alpha: op.alpha };
    }
    case "poly":
      return { ...op, fill: undefined, stroke: op.stroke ?? op.fill, lineWidth: OUTLINE_VIEW_MM };
    case "rect":
    case "circle":
      return op.fill ? { ...op, fill: undefined, stroke: op.fill, lineWidth: OUTLINE_VIEW_MM } : op;
    default:
      return op;
  }
}

/** Pad-number and net-name size in screen pixels. */
export const ANNOTATION_PX = 10;
/** Layers on the far side of the active layer are drawn at this opacity. */
export const FAR_SIDE_ALPHA = 0.55;

const side = (k: string): "top" | "bottom" | "inner" => (k === "top" || k === "bottom" ? k : "inner");

/** The layer stack for a board with these copper layers (top first). */
export function pcbLayers(order: CopperLayer[], activeLayer: string): RenderLayer[] {
  const keys: { key: string; copper: string | null; annotation?: boolean }[] = [];
  const sideLayers = (k: string, withSilk: boolean) => {
    keys.push({ key: `${k}:zone`, copper: k }, { key: `${k}:copper`, copper: k }, { key: `${k}:netNames`, copper: k, annotation: true });
    if (withSilk) keys.push({ key: `${k}:padNumbers`, copper: k, annotation: true }, { key: `${k}:silk`, copper: k });
  };
  sideLayers("bottom", true);
  const inner = order.map(layerKey).filter((k) => side(k) === "inner");
  for (const k of [...inner].reverse()) sideLayers(k, false);
  sideLayers("top", true);
  keys.push(
    { key: "multi", copper: null },
    { key: "multi:padNumbers", copper: null, annotation: true },
    { key: "outline", copper: null },
    { key: "ratsnest", copper: null },
    { key: "drill", copper: null },
  );
  const activeSide = side(activeLayer);
  return keys.map((k, i) => {
    let alpha = 1;
    if (k.copper !== null) {
      const s = side(k.copper);
      if (s === "inner") alpha = k.copper === activeLayer ? 1 : 0.7;
      else if (activeSide !== "inner" && s !== activeSide) alpha = FAR_SIDE_ALPHA;
    }
    return { key: k.key, order: i, alpha, annotation: k.annotation };
  });
}

/** A fill rectangle `[x0, y0, x1, y1]` (board nm) as its four view corners. */
export function rectPts([x0, y0, x1, y1]: [number, number, number, number]): Pt[] {
  return [toView({ x: x0, y: y0 }), toView({ x: x1, y: y0 }), toView({ x: x1, y: y1 }), toView({ x: x0, y: y1 })];
}

/** A fill as one SVG path (one subpath per rectangle), in view mm. */
export function fillPath(f: FillView): string {
  return f.rects
    .map((r) => {
      const [[ax, ay], , [cx, cy]] = rectPts(r);
      return `M${ax} ${ay}H${cx}V${cy}H${ax}Z`;
    })
    .join("");
}

/** Content signature of a fill (rectangle count plus a running hash of the corners). */
function fillSig(f: FillView): string {
  let h = 0;
  for (const r of f.rects) for (const v of r) h = (h * 31 + v / f.pitch) | 0;
  return `${f.rects.length}:${h}`;
}

/** Text that reads left to right: track angle folded into (-90°, 90°]. */
function readableAngle(a: Pt, b: Pt): number {
  let ang = Math.atan2(b[1] - a[1], b[0] - a[0]);
  if (ang > Math.PI / 2) ang -= Math.PI;
  if (ang <= -Math.PI / 2) ang += Math.PI;
  return ang;
}

export function buildPcbScene(s: Snapshot, o: PcbSceneOptions): Scene {
  const { pcb } = s.project;
  const layers = pcbLayers(o.order, o.activeLayer);
  const objects: RenderObject[] = [];
  const shown = (l: CopperLayer) => !o.hidden.has(layerKey(l));
  const allCopper = o.order.map(layerKey);

  // Zones (pours sit under the copper of their layer).
  const fillOf = o.fills ? new Map(o.fills.map((f) => [f.zone_id, f])) : null;
  for (const z of Object.values(pcb.zones)) {
    if (!shown(z.layer)) continue;
    const pts = z.outline.map(toView);
    const c = o.colourOf(z.layer);
    const lk = layerKey(z.layer);
    if (fillOf) {
      // The outline, then the engine's fill. Outline view draws the outline alone.
      const fill = fillOf.get(z.id);
      const ops: DrawOp[] = [{ t: "poly", pts, stroke: c, lineWidth: 0.08, alpha: 0.9 }];
      if (fill && !o.outlineView) for (const r of fill.rects) ops.push({ t: "poly", pts: rectPts(r), fill: c, alpha: 0.6 });
      objects.push({
        key: `zone:${z.id}`,
        ref: { kind: "zone", id: z.id },
        layerKey: `${lk}:zone`,
        bbox: boxOfPts(pts),
        hit: (x, y, tol) => inPoly(pts, x, y) || polyEdgeDist(pts, x, y) <= tol,
        pathPts: pts,
        pickLayers: [lk],
        ops,
        sig: `${pts.join(";")}|${c}|fill:${fill ? fillSig(fill) : "none"}`,
      });
      continue;
    }
    objects.push({
      key: `zone:${z.id}`,
      ref: { kind: "zone", id: z.id },
      layerKey: `${lk}:zone`,
      bbox: boxOfPts(pts),
      hit: (x, y, tol) => inPoly(pts, x, y) || polyEdgeDist(pts, x, y) <= tol,
      pathPts: pts,
      pickLayers: [lk],
      ops: [{ t: "poly", pts, fill: c, stroke: c, lineWidth: 0.05, alpha: 0.25 }],
      sig: `${pts.join(";")}|${c}`,
    });
  }

  // Tracks and their net names.
  for (const t of Object.values(pcb.tracks)) {
    if (!shown(t.layer)) continue;
    const a = toView(t.a);
    const b = toView(t.b);
    const w = mm(t.width);
    const c = o.colourOf(t.layer);
    const lk = layerKey(t.layer);
    // An arc track (schema v4) is drawn and picked as its chords.
    const path = trackPath(t).map(toView);
    const segs = path.slice(1).map((q, i): [Pt, Pt] => [path[i], q]);
    objects.push({
      key: `track:${t.id}`,
      ref: { kind: "track", id: t.id },
      layerKey: `${lk}:copper`,
      bbox: boxOfPts(path, w / 2),
      hit: (x, y, tol) => segs.some(([p, q]) => segDist(x, y, p, q) <= w / 2 + tol),
      pathPts: path,
      pickLayers: [lk],
      ops: segs.map(([p, q]): DrawOp => ({ t: "seg", a: p, b: q, width: w, colour: c })),
      sig: `${path.join(";")}|${w}|${c}`,
    });
    if (t.net) {
      const len = Math.hypot(b[0] - a[0], b[1] - a[1]);
      const mid: Pt = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
      objects.push({
        key: `netname:${t.id}`,
        ref: null,
        layerKey: `${lk}:netNames`,
        bbox: boxOfPts([a, b], w / 2),
        hit: () => false,
        ops: [{ t: "label", x: mid[0], y: mid[1], text: t.net, px: ANNOTATION_PX, colour: "--c-text", fitW: len, fitH: w, angle: readableAngle(a, b) }],
        sig: `${mid}|${t.net}|${len}|${w}`,
      });
    }
  }

  // Footprints: courtyard + refdes on the side's silk, pads on copper (SMD) or multi-layer (THT).
  // A pad picks its footprint, as clicking a pad inside the SVG footprint group does.
  for (const f of s.geometry.footprints) {
    const [x, y] = toView(f.position);
    const hw = mm(f.half_w);
    const hh = mm(f.half_h);
    const court: Box = { minX: x - hw, minY: y - hh, maxX: x + hw, maxY: y + hh };
    const fside = f.side === "bottom" ? "bottom" : "top";
    const padBoxes: Box[] = [];
    for (const p of f.pads) {
      const [px, py] = toView(p.position);
      const w = mm(p.size_x);
      const h = mm(p.size_y);
      const r = p.shape === "rect" ? 0 : Math.min(w, h) / 2;
      const pb: Box = { minX: px - w / 2, minY: py - h / 2, maxX: px + w / 2, maxY: py + h / 2 };
      padBoxes.push(pb);
      const tht = p.kind === "tht";
      const fill = !tht && fside === "bottom" ? "--c-pad-bottom" : "--c-pad";
      const op: DrawOp = { t: "rect", x: pb.minX, y: pb.minY, w, h, radius: r, fill };
      objects.push({
        key: `pad:${f.id}:${p.number}`,
        ref: { kind: "footprint", id: f.id },
        layerKey: tht ? "multi" : `${fside}:copper`,
        bbox: pb,
        hit: (hx, hy, tol) => inBox(pb, hx, hy, tol),
        selPoly: rectPoly(pb),
        pickLayers: tht ? allCopper : [fside],
        owner: f.id,
        ops: [op],
        sig: `${pb.minX},${pb.minY},${w},${h},${r},${fill}`,
      });
      if (p.drill !== null) {
        const dr = mm(p.drill) / 2;
        objects.push({
          key: `paddrill:${f.id}:${p.number}`,
          ref: null,
          layerKey: "drill",
          bbox: { minX: px - dr, minY: py - dr, maxX: px + dr, maxY: py + dr },
          hit: () => false,
          owner: f.id,
          ops: [{ t: "circle", cx: px, cy: py, r: dr, fill: "--bg-canvas" }],
          sig: `${px},${py},${dr}`,
        });
      }
      objects.push({
        key: `padnum:${f.id}:${p.number}`,
        ref: null,
        layerKey: tht ? "multi:padNumbers" : `${fside}:padNumbers`,
        bbox: pb,
        hit: () => false,
        owner: f.id,
        ops: [{ t: "label", x: px, y: py, text: p.number, px: ANNOTATION_PX, colour: "--bg-canvas", fitW: w, fitH: h, angle: 0 }],
        sig: `${px},${py},${w},${h},${p.number}`,
      });
    }
    objects.push({
      key: `footprint:${f.id}`,
      ref: { kind: "footprint", id: f.id },
      layerKey: `${fside}:silk`,
      bbox: padBoxes.reduce<Box>((acc, b) => union(acc, b), { ...court, minY: court.minY - 1.4 }),
      hit: (hx, hy, tol) => inBox(court, hx, hy, tol) || padBoxes.some((b) => inBox(b, hx, hy, tol)),
      selPoly: rectPoly(grow(court, 0.3)),
      owner: f.id,
      ops: [
        { t: "rect", x: court.minX, y: court.minY, w: 2 * hw, h: 2 * hh, radius: 0, stroke: "--c-courtyard", lineWidth: 0.05 },
        ...(o.silk ? [] : [{ t: "text", x, y: court.minY - 0.5, text: f.refdes, size: 0.9, colour: "--c-text", minPx: 5 } as DrawOp]),
      ],
      sig: `${x},${y},${hw},${hh},${f.refdes}${o.silk ? ",legend" : ""}`,
    });
  }

  // The engine silkscreen legend (outlines, pin-1 marks, stroke-font refdes, mask-clipped): one
  // decoration object per side on that side's silk.
  for (const side of ["top", "bottom"] as const) {
    const strokes = o.silk?.[side] ?? [];
    if (!strokes.length) continue;
    const ops: DrawOp[] = strokes.map((st) => ({ t: "seg", a: toView(st.a), b: toView(st.b), width: mm(st.width), colour: "--c-text" }));
    const pts = strokes.flatMap((st) => [toView(st.a), toView(st.b)]);
    let h = 0;
    for (const st of strokes) h = (h * 31 + st.a.x + 7 * st.a.y + 13 * st.b.x + 17 * st.b.y + st.width) | 0;
    objects.push({ key: `silk:${side}`, ref: null, layerKey: `${side}:silk`, bbox: boxOfPts(pts, mm(strokes[0].width)), hit: () => false, ops, sig: `${strokes.length}:${h}` });
  }

  // Vias: barrel on the multi-layer, hole on the drill layer; pickable on every layer they span.
  for (const v of Object.values(pcb.vias)) {
    const span = viaLayers(o.order, v.from ?? "top", v.to ?? "bottom");
    if (!span.some(shown)) continue;
    const [x, y] = toView(v.position);
    const r = mm(v.diameter) / 2;
    const dr = mm(v.drill) / 2;
    const bb: Box = { minX: x - r, minY: y - r, maxX: x + r, maxY: y + r };
    objects.push({
      key: `via:${v.id}`,
      ref: { kind: "via", id: v.id },
      layerKey: "multi",
      bbox: bb,
      hit: (hx, hy, tol) => Math.hypot(hx - x, hy - y) <= r + tol,
      selPoly: rectPoly(bb),
      pickLayers: span.map(layerKey),
      ops: [{ t: "circle", cx: x, cy: y, r, fill: "--c-via" }],
      sig: `${x},${y},${r}`,
    });
    objects.push({
      key: `viadrill:${v.id}`,
      ref: null,
      layerKey: "drill",
      bbox: { minX: x - dr, minY: y - dr, maxX: x + dr, maxY: y + dr },
      hit: () => false,
      ops: [{ t: "circle", cx: x, cy: y, r: dr, fill: "--bg-canvas" }],
      sig: `${x},${y},${dr}`,
    });
  }

  // Schema v4 drawing objects (ADR-0042): decoration only, not pickable. Silk text sits on its
  // side's silk; documentation text, dimensions, regions and cut-outs sit with the outline.
  const drawLayer = (l: string) => (l === "top_silk" ? "top:silk" : l === "bottom_silk" ? "bottom:silk" : "outline");
  const drawColour = (l: string) => (l === "documentation" ? "--c-courtyard" : "--c-text");
  for (const r of Object.values(pcb.regions ?? {})) {
    const pts = r.outline.map(toView);
    const keepout = (r.forbid ?? []).length > 0;
    objects.push({
      key: `region:${r.id}`,
      ref: null,
      layerKey: "outline",
      bbox: boxOfPts(pts, 0.1),
      hit: () => false,
      pathPts: pts,
      ops: [{ t: "poly", pts, fill: "--c-violation", stroke: "--c-violation", lineWidth: 0.1, alpha: keepout ? 0.12 : 0.05 }],
      sig: `${pts.join(";")}|${keepout}`,
    });
  }
  for (const cut of Object.values(pcb.cutouts ?? {})) {
    const pts = cut.outline.map(toView);
    objects.push({
      key: `cutout:${cut.id}`,
      ref: null,
      layerKey: "outline",
      bbox: boxOfPts(pts, 0.1),
      hit: () => false,
      pathPts: pts,
      ops: [{ t: "poly", pts, fill: "--bg-canvas", stroke: "--accent", lineWidth: 0.15 }],
      sig: pts.join(";"),
    });
  }
  for (const tx of Object.values(pcb.texts ?? {})) {
    const [x, y] = toView(tx.position);
    const size = mm(tx.size);
    const w = size * 0.8 * tx.text.length;
    objects.push({
      key: `text:${tx.id}`,
      ref: null,
      layerKey: drawLayer(tx.layer),
      bbox: { minX: x - w / 2, minY: y - size, maxX: x + w / 2, maxY: y },
      hit: () => false,
      ops: [{ t: "text", x, y, text: tx.text, size, colour: drawColour(tx.layer), minPx: 4 }],
      sig: `${x},${y},${size},${tx.text},${tx.layer}`,
    });
  }
  for (const d of Object.values(pcb.dimensions ?? {})) {
    const g = dimensionDrawing(d);
    const colour = drawColour(d.layer);
    const ops: DrawOp[] = g.lines.map(([p, q]) => ({ t: "seg", a: toView(p), b: toView(q), width: 0.15, colour }));
    const [lx, ly] = toView(g.labelAt);
    ops.push({ t: "text", x: lx, y: ly, text: g.label, size: mm(g.textSize), colour, minPx: 4 });
    const pts = g.lines.flat().map(toView);
    objects.push({
      key: `dimension:${d.id}`,
      ref: null,
      layerKey: drawLayer(d.layer),
      bbox: boxOfPts([...pts, [lx, ly]], mm(g.textSize)),
      hit: () => false,
      ops,
      sig: `${pts.join(";")}|${g.label}|${d.layer}`,
    });
  }

  // Board edge (the board fill stays in the SVG underlay, under the grid); arc edges flattened.
  const outline = outlinePath(pcb.board.outline, pcb.board.outline_arcs).map(toView);
  if (outline.length && !o.hidden.has("outline")) {
    objects.push({
      key: "outline",
      ref: null,
      layerKey: "outline",
      bbox: boxOfPts(outline, 0.1),
      hit: () => false,
      pathPts: outline,
      ops: [{ t: "poly", pts: outline, stroke: "--accent", lineWidth: 0.15 }],
      sig: outline.join(";"),
    });
  }

  // Ratsnest.
  (o.airwires ?? []).forEach(([n, p, q], i) => {
    const a = toView(p);
    const b = toView(q);
    objects.push({
      key: `airwire:${i}:${n}`,
      ref: null,
      layerKey: "ratsnest",
      bbox: boxOfPts([a, b], 0.1),
      hit: () => false,
      ops: [{ t: "seg", a, b, width: 0.12, colour: "--c-airwire", alpha: 0.7 }],
      sig: `${a}|${b}`,
    });
  });

  // Outline view: copper objects keep their geometry and picking, drawn as outlines. Holes are
  // left out (their background fill would otherwise hide the pad outline under them).
  const drawn = o.outlineView
    ? objects
        .filter((ob) => ob.layerKey !== "drill")
        .map((ob) => (ob.ref ? { ...ob, ops: ob.ops.map(outlineOp), sig: `outline|${ob.sig}` } : ob))
    : objects;
  let bounds: Box | null = null;
  for (const ob of drawn) bounds = union(bounds, ob.bbox);
  return { layers, objects: drawn, bounds };
}
