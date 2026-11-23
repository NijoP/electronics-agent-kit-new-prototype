// PCB drawing tools for the schema v4 objects (slice C4, ADR-0042): keep-out / constraint
// regions, board cut-outs, PCB text and dimensions. Pure helpers in IR nanometres (Y up): they
// turn the clicked points into one typed engine command, hit-test the drawn objects for the
// Delete and Text tools, and re-index the outline's arc edges when a vertex is inserted or
// deleted. The engine validates every command; these only build them.
import { outlineFrom } from "../../editing";
import { arcCenter, dimensionDrawing } from "../../render/pcbObjects";
import type { Command, CopperLayer, DrawLayer, Id, Nm, OutlineArc, Point, RegionObject, Rotation, Snapshot } from "../../types";
import * as O from "./outline";

type Pcb = Snapshot["project"]["pcb"];

/** What a keep-out region keeps out (the tool's presets; the IR takes any subset). */
export const FORBID_PRESETS: [string, string, RegionObject[]][] = [
  ["all", "Tracks, vias, pads and pours", ["tracks", "vias", "components", "zones"]],
  ["copper", "Tracks and vias", ["tracks", "vias"]],
  ["zones", "Copper pours only", ["zones"]],
  ["none", "Nothing (track-width rule only)", []],
];
export const forbidOf = (preset: string): RegionObject[] => FORBID_PRESETS.find(([k]) => k === preset)?.[2] ?? FORBID_PRESETS[0][2];

export interface RegionOptions {
  /** One copper layer, or null for every copper layer. */
  layer: CopperLayer | null;
  forbid: RegionObject[];
  name: string;
  minWidth: Nm | null;
  maxWidth: Nm | null;
}

/** `AddRegion` from the clicked points (two opposite corners, or 3+ vertices), or why not. */
export function regionCommand(corners: Point[], o: RegionOptions): Command | string {
  const outline = outlineFrom(corners);
  if (!outline) return "A keep-out needs two opposite corners or at least three points.";
  const bad = O.validateOutline(outline);
  if (bad.length) return `Keep-out outline is not valid: ${bad[0]}.`;
  if (!o.forbid.length && o.minWidth === null && o.maxWidth === null)
    return "A region must keep something out or limit the track width: pick what it keeps out, or enter a width limit.";
  return {
    type: "AddRegion",
    name: o.name.trim() || null,
    outline,
    layers: o.layer ? [o.layer] : [],
    forbid: o.forbid,
    min_track_width: o.minWidth,
    max_track_width: o.maxWidth,
  };
}

/** `AddBoardCutout` from the clicked points, or why not (the engine checks it lies inside). */
export function cutoutCommand(corners: Point[], boardOutline: Point[]): Command | string {
  if (boardOutline.length < 3) return "Draw the board outline before a cut-out.";
  const outline = outlineFrom(corners);
  if (!outline) return "A cut-out needs two opposite corners or at least three points.";
  const bad = O.validateOutline(outline);
  if (bad.length) return `Cut-out outline is not valid: ${bad[0]}.`;
  return { type: "AddBoardCutout", outline };
}

export const DRAW_LAYERS: [DrawLayer, string][] = [
  ["top_silk", "Top silkscreen"],
  ["bottom_silk", "Bottom silkscreen"],
  ["documentation", "Documentation"],
];

/** `AddPcbText` at `at`, or why not. `sizeMm` empty = the engine's 1 mm. */
export function textCommand(at: Point, text: string, layer: DrawLayer, sizeMm: string): Command | string {
  const t = text.trim();
  if (!t) return "Type the text to place first.";
  const size = sizeMm.trim() === "" ? null : O.parseNum(sizeMm);
  if (size !== null && !(size > 0)) return "Text height must be a positive number of millimetres.";
  return { type: "AddPcbText", layer, position: at, text: t, size: size === null ? null : O.unitToNm(size, "mm"), stroke_width: null, rotation: null };
}

/** `AddDimension` from `a` to `b`, or why not. `offsetMm` empty = on the measured line. */
export function dimensionCommand(a: Point, b: Point, layer: DrawLayer, offsetMm: string): Command | string {
  if (a.x === b.x && a.y === b.y) return "A dimension needs two different points.";
  const off = offsetMm.trim() === "" ? 0 : O.parseNum(offsetMm);
  if (off === null) return "Dimension offset must be a number of millimetres.";
  return { type: "AddDimension", layer, a, b, offset: O.unitToNm(off, "mm"), unit: null, precision: null, text_size: null };
}

const ROT: Rotation[] = ["R0", "R90", "R180", "R270"];
const DEG: Record<Rotation, number> = { R0: 0, R90: 90, R180: 180, R270: 270 };
export const nextTextRotation = (r: Rotation | undefined): Rotation => ROT[(ROT.indexOf(r ?? "R0") + 1) % 4];

export type DrawingKind = "text" | "dimension" | "cutout" | "region";
export interface DrawingHit {
  kind: DrawingKind;
  id: Id;
}

function inPolygon(p: Point, poly: Point[]): boolean {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i];
    const b = poly[j];
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
}
function segDist(p: Point, a: Point, b: Point): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const l2 = dx * dx + dy * dy;
  const t = l2 === 0 ? 0 : Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}
const nearEdge = (p: Point, poly: Point[], tol: number) => poly.some((a, i) => segDist(p, a, poly[(i + 1) % poly.length]) <= tol);

/** Whether `p` is on the text's box: centred on its position, baseline at the position, the
 *  width the renderers use (0.8 × height per character), turned with the text. */
export function onText(p: Point, t: { position: Point; text: string; size: Nm; rotation?: Rotation }, tol: number): boolean {
  const a = (-DEG[t.rotation ?? "R0"] * Math.PI) / 180;
  const dx = p.x - t.position.x;
  const dy = p.y - t.position.y;
  const x = dx * Math.cos(a) - dy * Math.sin(a);
  const y = dx * Math.sin(a) + dy * Math.cos(a);
  const w = t.size * 0.8 * t.text.length;
  return Math.abs(x) <= w / 2 + tol && y >= -tol && y <= t.size + tol;
}

/** The topmost drawing object under `p` (text, then dimension, cut-out, region), or null.
 *  `shown` filters by draw layer (`top_silk`, …) or "outline" for cut-outs and regions. */
export function drawingAt(pcb: Pcb, p: Point, tol: Nm, shown: (layer: string) => boolean = () => true): DrawingHit | null {
  for (const t of Object.values(pcb.texts ?? {})) if (shown(t.layer) && onText(p, t, tol)) return { kind: "text", id: t.id };
  for (const d of Object.values(pcb.dimensions ?? {})) {
    if (!shown(d.layer)) continue;
    const g = dimensionDrawing(d);
    if (g.lines.some(([a, b]) => segDist(p, a, b) <= tol) || Math.hypot(p.x - g.labelAt.x, p.y - g.labelAt.y) <= g.textSize + tol) return { kind: "dimension", id: d.id };
  }
  if (shown("outline")) {
    for (const c of Object.values(pcb.cutouts ?? {})) if (inPolygon(p, c.outline) || nearEdge(p, c.outline, tol)) return { kind: "cutout", id: c.id };
    for (const r of Object.values(pcb.regions ?? {})) if (inPolygon(p, r.outline) || nearEdge(p, r.outline, tol)) return { kind: "region", id: r.id };
  }
  return null;
}

/** The engine command that deletes a hit drawing object. */
export function deleteDrawing(h: DrawingHit): Command {
  switch (h.kind) {
    case "text":
      return { type: "DeletePcbText", text_id: h.id };
    case "dimension":
      return { type: "DeleteDimension", dimension_id: h.id };
    case "cutout":
      return { type: "DeleteBoardCutout", cutout_id: h.id };
    case "region":
      return { type: "DeleteRegion", region_id: h.id };
  }
}

/** The point halfway along the arc from `a` to `b` (positive sweep counter-clockwise). */
export function arcMidpoint(a: Point, b: Point, sweepDeg: number): Point {
  const c = arcCenter(a, b, sweepDeg);
  if (!c) return { x: Math.round((a.x + b.x) / 2), y: Math.round((a.y + b.y) / 2) };
  const [cx, cy] = c;
  const r = Math.hypot(a.x - cx, a.y - cy);
  const t = Math.atan2(a.y - cy, a.x - cx) + (sweepDeg * Math.PI) / 360;
  return { x: Math.round(cx + r * Math.cos(t)), y: Math.round(cy + r * Math.sin(t)) };
}

export interface VertexEdit {
  /** The command to send: a bare `SetBoardOutline` when there are no arcs, else one Batch. */
  command: Command;
  /** Old edge numbers whose arc could not be kept (the two edges a deleted vertex joined). */
  dropped: number[];
}

/**
 * Insert a vertex on edge `i` or delete vertex `i`, keeping every arc edge on the edge it
 * belongs to. The engine makes all arcs straight when the vertex count changes, so the arcs
 * are cleared first (no "made straight" warning), the outline is set, and each arc is set again
 * on its new edge number: one Batch, one undo step. Inserting on an arc edge splits the arc at
 * its midpoint into two halves of the same circle; deleting a vertex next to an arc edge joins
 * two edges into one straight edge, and that arc is reported in `dropped`.
 */
export function vertexEdit(outline: Point[], arcs: OutlineArc[], op: "insert" | "delete", i: number): VertexEdit | string {
  const n = outline.length;
  if (op === "delete" && n <= 3) return "An outline needs at least 3 vertices.";
  let points: Point[];
  const next: OutlineArc[] = [];
  const dropped: number[] = [];
  if (op === "insert") {
    const own = arcs.find((a) => a.edge === i);
    points = O.insertVertex(outline, i, own ? arcMidpoint(outline[i], outline[(i + 1) % n], own.sweep_deg) : undefined);
    for (const a of arcs) {
      if (a.edge < i) next.push(a);
      else if (a.edge === i) next.push({ edge: i, sweep_deg: a.sweep_deg / 2 }, { edge: i + 1, sweep_deg: a.sweep_deg / 2 });
      else next.push({ edge: a.edge + 1, sweep_deg: a.sweep_deg });
    }
  } else {
    points = O.deleteVertex(outline, i);
    const joined = [(i - 1 + n) % n, i];
    for (const a of arcs) {
      if (joined.includes(a.edge)) dropped.push(a.edge);
      else next.push({ edge: a.edge < i ? a.edge : a.edge - 1, sweep_deg: a.sweep_deg });
    }
  }
  const bad = O.validateOutline(points);
  if (bad.length) return `Edit rejected: ${bad[0]}.`;
  if (!arcs.length) return { command: { type: "SetBoardOutline", points }, dropped };
  const commands: Command[] = [
    ...arcs.map<Command>((a) => ({ type: "SetOutlineArc", edge: a.edge, sweep_deg: null })),
    { type: "SetBoardOutline", points },
    ...next.sort((a, b) => a.edge - b.edge).map<Command>((a) => ({ type: "SetOutlineArc", edge: a.edge, sweep_deg: a.sweep_deg })),
  ];
  return { command: { type: "Batch", commands, label: `${op === "insert" ? "Insert" : "Delete"} outline vertex` }, dropped };
}

/** `SetOutlineArc` for edge `edge` from the typed sweep in degrees (empty or 0 = straight);
 *  null when it is already that shape; a string says why it cannot be sent. */
export function arcCommand(edge: number, text: string, current: number | undefined): Command | string | null {
  const t = text.trim();
  const sweep = t === "" ? 0 : O.parseNum(t);
  if (sweep === null || !(Math.abs(sweep) < 360)) return "Arc sweep must be a number of degrees between -360 and 360 (0 or empty = straight).";
  if ((current ?? 0) === sweep) return null;
  return { type: "SetOutlineArc", edge, sweep_deg: sweep === 0 ? null : sweep };
}
