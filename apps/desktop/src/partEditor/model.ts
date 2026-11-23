// Symbol and footprint editors (Phase 1 slice D2): the pure part of the editors.
//
// A draft is a copy of one project-library `ComponentDef` that the editor changes locally until
// the user saves it; saving sends the edited slice through a typed engine command
// (`UpsertComponentDefinition` for the symbol, `AssignComponentFootprint` for the footprint), and
// the engine validates it again (`ComponentDef::validate`). The checks here only give feedback
// before a save; they never replace the engine's.
//
// The drawing uses the A4 scene model (`render/types.ts`) and the B1 symbol primitives
// (`Graphic`, pin length/orientation/visibility).
import { boxOfPts, inPoly, segDist } from "../render/geometry";
import type { Box, DrawOp, Pt, RenderLayer, RenderObject, Scene } from "../render/types";
import type { ComponentDef, FootprintDef, Graphic, PadDef, Point, SymbolDef, SymbolPin } from "../types";

export const NM_PER_MM = 1_000_000;
export const toMm = (nm: number) => nm / NM_PER_MM;
/** Millimetres typed by the user to whole nanometres; null when not a finite number. */
export function fromMm(text: string): number | null {
  if (text.trim() === "") return null;
  const n = Number(text);
  return Number.isFinite(n) ? Math.round(n * NM_PER_MM) : null;
}
/** Shown value of a length in millimetres (no float noise). */
export const fmtMm = (nm: number) => String(Math.round(toMm(nm) * 1e6) / 1e6);

/** EAK-IR (nm, Y up) to view millimetres (Y down), as `viewport.ts` `toView`. */
const v = (p: Point): Pt => [toMm(p.x), -toMm(p.y)];

export type PartEditorKind = "symbol" | "footprint";
export const PIN_TYPES = ["passive", "input", "output", "bidirectional", "power_in", "power_out", "open_collector", "no_connect"] as const;
export const ORIENTATIONS = ["right", "left", "up", "down"] as const;
export const GRAPHIC_KINDS = ["rectangle", "line", "polyline", "polygon", "circle", "arc", "text"] as const;
export type GraphicKind = (typeof GRAPHIC_KINDS)[number];

// ---- Normalisation: fill the optional fields the engine always serialises, so a draft can be
// compared with the engine's definition field by field.
export const normPin = (p: SymbolPin): SymbolPin => ({
  number: p.number,
  name: p.name,
  electrical_type: p.electrical_type,
  position: { x: p.position.x, y: p.position.y },
  length: p.length ?? 0,
  orientation: p.orientation ?? "right",
  name_visible: p.name_visible ?? true,
  number_visible: p.number_visible ?? true,
});
const stroke = (g: { stroke?: { width: number; style: string } }) => ({ width: g.stroke?.width ?? 0, style: (g.stroke?.style ?? "solid") as "solid" });
export function normGraphic(g: Graphic): Graphic {
  switch (g.kind) {
    case "line":
      return { kind: "line", a: { ...g.a }, b: { ...g.b }, stroke: stroke(g) };
    case "polyline":
      return { kind: "polyline", points: g.points.map((p) => ({ ...p })), stroke: stroke(g) };
    case "polygon":
      return { kind: "polygon", points: g.points.map((p) => ({ ...p })), fill: g.fill ?? false, stroke: stroke(g) };
    case "arc":
      return { kind: "arc", center: { ...g.center }, radius: g.radius, start_deg: g.start_deg, end_deg: g.end_deg, stroke: stroke(g) };
    case "circle":
      return { kind: "circle", center: { ...g.center }, radius: g.radius, fill: g.fill ?? false, stroke: stroke(g) };
    case "rectangle":
      return { kind: "rectangle", min: { ...g.min }, max: { ...g.max }, fill: g.fill ?? false, stroke: stroke(g) };
    case "text":
      return { kind: "text", position: { ...g.position }, text: g.text, size: g.size, rotation: g.rotation ?? "R0" };
  }
}
export const normSymbol = (s: SymbolDef): SymbolDef => ({ pins: s.pins.map(normPin), body_half_w: s.body_half_w, body_half_h: s.body_half_h, graphics: (s.graphics ?? []).map(normGraphic) });
export const normPad = (p: PadDef): PadDef => ({ number: p.number, kind: p.kind, shape: p.shape, position: { ...p.position }, size_x: p.size_x, size_y: p.size_y, drill: p.kind === "tht" ? (p.drill ?? null) : null });
export const normFootprint = (f: FootprintDef): FootprintDef => ({ name: f.name, pads: f.pads.map(normPad), courtyard_half_w: f.courtyard_half_w, courtyard_half_h: f.courtyard_half_h, body_height: f.body_height ?? null });
const normMap = (m: Record<string, string[]>) =>
  Object.fromEntries(
    Object.entries(m)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([k, pads]) => [k, [...pads]]),
  );

/** A definition with every optional field filled (the editor's draft starts from this). */
export const normDef = (d: ComponentDef): ComponentDef => ({ ...d, symbol: normSymbol(d.symbol), footprint: normFootprint(d.footprint), pin_map: normMap(d.pin_map) });

/** The fields an editor owns and saves; two equal slices draw and save identically. */
export function editedSlice(kind: PartEditorKind, d: ComponentDef): unknown {
  const n = normDef(d);
  return kind === "symbol"
    ? { name: n.name, category: n.category, refdes_prefix: n.refdes_prefix, default_value: n.default_value, symbol: n.symbol, pin_map: n.pin_map }
    : { footprint: n.footprint, pin_map: n.pin_map };
}
export const sameSlice = (kind: PartEditorKind, a: ComponentDef, b: ComponentDef) => JSON.stringify(editedSlice(kind, a)) === JSON.stringify(editedSlice(kind, b));

// ---- Edits (all return a new draft).

/** Renumber a pin, carrying its pin-map entry with it. */
export function renamePin(d: ComponentDef, from: string, to: string): ComponentDef {
  const pins = d.symbol.pins.map((p) => (p.number === from ? { ...p, number: to } : p));
  const pin_map: Record<string, string[]> = {};
  for (const [k, pads] of Object.entries(d.pin_map)) pin_map[k === from ? to : k] = pads;
  return { ...d, symbol: { ...d.symbol, pins }, pin_map };
}
/** Renumber a pad, carrying every pin-map reference to it. */
export function renamePad(d: ComponentDef, from: string, to: string): ComponentDef {
  const pads = d.footprint.pads.map((p) => (p.number === from ? { ...p, number: to } : p));
  const pin_map = Object.fromEntries(Object.entries(d.pin_map).map(([k, ps]) => [k, ps.map((p) => (p === from ? to : p))]));
  return { ...d, footprint: { ...d.footprint, pads }, pin_map };
}
/** Comma/space separated numbers ("1, 2") to a list. */
export const parseList = (text: string) =>
  text
    .split(/[\s,;]+/)
    .map((s) => s.trim())
    .filter(Boolean);
/** Set the pads a pin maps to (an empty list removes the pin's entry). */
export function setPinPads(d: ComponentDef, pin: string, pads: string[]): ComponentDef {
  const pin_map = { ...d.pin_map };
  if (pads.length) pin_map[pin] = pads;
  else delete pin_map[pin];
  return { ...d, pin_map };
}
/** Pins mapped to `pad`, in pin-map order. */
export const pinsOfPad = (d: ComponentDef, pad: string) =>
  Object.entries(d.pin_map)
    .filter(([, ps]) => ps.includes(pad))
    .map(([k]) => k);
/** Set the pins that map to `pad`: the pad is added to those pins' lists and removed from the others. */
export function setPadPins(d: ComponentDef, pad: string, pins: string[]): ComponentDef {
  const pin_map: Record<string, string[]> = {};
  const want = new Set(pins);
  for (const [k, ps] of Object.entries(d.pin_map)) {
    const rest = ps.filter((p) => p !== pad);
    const next = want.has(k) ? [...rest, pad] : rest;
    if (next.length) pin_map[k] = next;
  }
  for (const k of pins) if (!pin_map[k]) pin_map[k] = [pad];
  return { ...d, pin_map };
}

/** The next free number in a list of pin or pad numbers ("1", "2" -> "3"). */
export function nextNumber(numbers: string[]): string {
  const used = new Set(numbers);
  let n = numbers.reduce((m, s) => (/^\d+$/.test(s) ? Math.max(m, Number(s)) : m), 0) + 1;
  while (used.has(String(n))) n++;
  return String(n);
}

const SYMBOL_PITCH = 2_540_000;
/** A new pin one pitch below the lowest left-side pin, mapped to the pad of the same number when it exists. */
export function addPin(d: ComponentDef): ComponentDef {
  const number = nextNumber(d.symbol.pins.map((p) => p.number));
  const left = d.symbol.pins.filter((p) => (p.orientation ?? "right") === "right");
  const x = left[0]?.position.x ?? -d.symbol.body_half_w - SYMBOL_PITCH;
  const y = left.length ? Math.min(...left.map((p) => p.position.y)) - SYMBOL_PITCH : 0;
  const pin: SymbolPin = { number, name: `P${number}`, electrical_type: "passive", position: { x, y }, length: SYMBOL_PITCH, orientation: "right", name_visible: true, number_visible: true };
  const next = { ...d, symbol: { ...d.symbol, pins: [...d.symbol.pins, pin] } };
  return d.footprint.pads.some((p) => p.number === number) ? setPinPads(next, number, [number]) : next;
}
export function deletePin(d: ComponentDef, number: string): ComponentDef {
  const pin_map = { ...d.pin_map };
  delete pin_map[number];
  return { ...d, symbol: { ...d.symbol, pins: d.symbol.pins.filter((p) => p.number !== number) }, pin_map };
}

const PAD_PITCH = 2_540_000;
/** A new pad one pitch right of the right-most pad, like the last pad. */
export function addPad(d: ComponentDef): ComponentDef {
  const pads = d.footprint.pads;
  const number = nextNumber(pads.map((p) => p.number));
  const last = pads[pads.length - 1];
  const x = pads.length ? Math.max(...pads.map((p) => p.position.x)) + PAD_PITCH : 0;
  const pad: PadDef = {
    number,
    kind: last?.kind ?? "smd",
    shape: last?.shape ?? "rect",
    position: { x, y: last?.position.y ?? 0 },
    size_x: last?.size_x ?? 1_200_000,
    size_y: last?.size_y ?? 1_200_000,
    drill: last?.kind === "tht" ? (last.drill ?? 700_000) : null,
  };
  return { ...d, footprint: { ...d.footprint, pads: [...pads, pad] } };
}
export function deletePad(d: ComponentDef, number: string): ComponentDef {
  const pin_map = Object.fromEntries(
    Object.entries(d.pin_map)
      .map(([k, ps]) => [k, ps.filter((p) => p !== number)] as const)
      .filter(([, ps]) => ps.length),
  );
  return { ...d, footprint: { ...d.footprint, pads: d.footprint.pads.filter((p) => p.number !== number) }, pin_map };
}

/** A new graphic of `kind` sized to the symbol body. */
export function newGraphic(kind: GraphicKind, s: SymbolDef): Graphic {
  const w = Math.max(s.body_half_w, 1_270_000);
  const h = Math.max(s.body_half_h, 1_270_000);
  const st = { width: 0, style: "solid" as const };
  switch (kind) {
    case "rectangle":
      return { kind, min: { x: -w, y: -h }, max: { x: w, y: h }, fill: false, stroke: st };
    case "line":
      return { kind, a: { x: -w, y: 0 }, b: { x: w, y: 0 }, stroke: st };
    case "polyline":
      return { kind, points: [{ x: -w, y: -h }, { x: 0, y: h }, { x: w, y: -h }], stroke: st };
    case "polygon":
      return { kind, points: [{ x: -w, y: -h }, { x: w, y: 0 }, { x: -w, y: h }], fill: false, stroke: st };
    case "circle":
      return { kind, center: { x: 0, y: 0 }, radius: Math.min(w, h), fill: false, stroke: st };
    case "arc":
      return { kind, center: { x: 0, y: 0 }, radius: Math.min(w, h), start_deg: 0, end_deg: 180, stroke: st };
    case "text":
      return { kind, position: { x: 0, y: 0 }, text: "Text", size: 1_270_000, rotation: "R0" };
  }
}

/** Points as "x y; x y" in millimetres. */
export const fmtPoints = (pts: Point[]) => pts.map((p) => `${fmtMm(p.x)} ${fmtMm(p.y)}`).join("; ");
export function parsePoints(text: string): Point[] | null {
  const out: Point[] = [];
  for (const part of text
    .split(";")
    .map((s) => s.trim())
    .filter(Boolean)) {
    const xy = part.split(/[\s,]+/);
    if (xy.length !== 2) return null;
    const x = fromMm(xy[0]);
    const y = fromMm(xy[1]);
    if (x === null || y === null) return null;
    out.push({ x, y });
  }
  return out;
}

// ---- New part.

/** The starting point of a part created in the editors: a two-pin box symbol on a two-pad SMD footprint. */
export function newPartDefinition(id: string, name: string, refdesPrefix: string, category: string): ComponentDef {
  const pin = (n: string, x: number, orientation: "right" | "left"): SymbolPin => ({ number: n, name: n, electrical_type: "passive", position: { x, y: 0 }, length: SYMBOL_PITCH, orientation, name_visible: false, number_visible: true });
  const pad = (n: string, x: number): PadDef => ({ number: n, kind: "smd", shape: "rect", position: { x, y: 0 }, size_x: 1_200_000, size_y: 1_400_000, drill: null });
  return {
    id,
    name,
    category,
    refdes_prefix: refdesPrefix,
    default_value: "",
    manufacturer: null,
    mpn: null,
    distributor: null,
    datasheet: null,
    symbol: {
      pins: [pin("1", -5_080_000, "right"), pin("2", 5_080_000, "left")],
      body_half_w: 2_540_000,
      body_half_h: 1_270_000,
      graphics: [{ kind: "rectangle", min: { x: -2_540_000, y: -1_270_000 }, max: { x: 2_540_000, y: 1_270_000 }, fill: false, stroke: { width: 0, style: "solid" } }],
    },
    footprint: { name: `${id}_FP`, pads: [pad("1", -1_270_000), pad("2", 1_270_000)], courtyard_half_w: 2_500_000, courtyard_half_h: 1_200_000, body_height: null },
    pin_map: { "1": ["1"], "2": ["2"] },
    ratings: [],
    model3d: { kind: "box", color: "#808080", external_ref: null },
    provenance: "Created in the EAK symbol and footprint editors; unverified",
  };
}

/** Why `id` cannot name a new part (empty, bad characters, or taken), or null. */
export function newPartIdProblem(id: string, taken: Set<string>): string | null {
  const t = id.trim();
  if (!t) return "Enter a part id.";
  if (!/^[A-Za-z0-9_.+-]+$/.test(t)) return "Use letters, digits and _ . + - only in the part id.";
  if (taken.has(t)) return `A part with id ${t} already exists; choose another id.`;
  return null;
}

// ---- Local checks (feedback before Save; the engine re-validates).

export function graphicProblems(g: Graphic): string[] {
  const e: string[] = [];
  if (g.kind !== "text" && (g.stroke?.width ?? 0) < 0) e.push("line width must not be negative");
  switch (g.kind) {
    case "line":
      if (g.a.x === g.b.x && g.a.y === g.b.y) e.push("line has zero length");
      break;
    case "polyline":
      if (g.points.length < 2) e.push("polyline needs at least 2 points");
      else if (g.points.some((p, i) => i > 0 && p.x === g.points[i - 1].x && p.y === g.points[i - 1].y)) e.push("polyline has a zero-length segment");
      break;
    case "polygon":
      if (g.points.length < 3) e.push("polygon needs at least 3 points");
      break;
    case "arc":
      if (g.radius <= 0) e.push("arc radius must be positive");
      if (g.start_deg === g.end_deg) e.push("arc has zero sweep");
      break;
    case "circle":
      if (g.radius <= 0) e.push("circle radius must be positive");
      break;
    case "rectangle":
      if (g.min.x >= g.max.x || g.min.y >= g.max.y) e.push("rectangle min must be below and left of max");
      break;
    case "text":
      if (!g.text.trim()) e.push("text is empty");
      if (g.size <= 0) e.push("text size must be positive");
      break;
  }
  return e;
}

/** Problems that make the engine refuse the definition (mirrors `ComponentDef::validate`). */
export function draftProblems(d: ComponentDef): string[] {
  const out: string[] = [];
  const s = d.symbol;
  if (s.pins.length === 0) out.push("The symbol has no pins.");
  if ((s.graphics ?? []).length === 0) out.push("Draw the symbol body: add at least one graphic.");
  (s.graphics ?? []).forEach((g, i) => graphicProblems(g).forEach((p) => out.push(`Graphic ${i + 1} (${g.kind}): ${p}.`)));
  const pinSeen = new Set<string>();
  for (const p of s.pins) {
    if (!p.number.trim()) out.push("A pin has no number.");
    if (pinSeen.has(p.number)) out.push(`Pin ${p.number} is listed twice.`);
    pinSeen.add(p.number);
    if ((p.length ?? 0) < 0) out.push(`Pin ${p.number} has a negative length.`);
    const pads = d.pin_map[p.number];
    if (!pads && p.electrical_type !== "no_connect") out.push(`Pin ${p.number} is not mapped to a pad.`);
    for (const pad of pads ?? []) if (!d.footprint.pads.some((x) => x.number === pad)) out.push(`Pin ${p.number} maps to pad ${pad}, which the footprint does not have.`);
  }
  for (const k of Object.keys(d.pin_map)) if (!pinSeen.has(k)) out.push(`The pin map names pin ${k}, which the symbol does not have.`);
  const padSeen = new Set<string>();
  for (const p of d.footprint.pads) {
    if (!p.number.trim()) out.push("A pad has no number.");
    if (padSeen.has(p.number)) out.push(`Pad ${p.number} is listed twice.`);
    padSeen.add(p.number);
    if (p.size_x <= 0 || p.size_y <= 0) out.push(`Pad ${p.number} needs a positive size.`);
    if (p.kind === "tht" && !p.drill) out.push(`Through-hole pad ${p.number} needs a drill.`);
    if (p.kind === "tht" && p.drill && p.drill >= Math.min(p.size_x, p.size_y)) out.push(`Pad ${p.number}: the drill leaves no annular ring.`);
  }
  if (d.footprint.courtyard_half_w <= 0 || d.footprint.courtyard_half_h <= 0) out.push("The courtyard needs a positive size.");
  if (!d.name.trim()) out.push("The part needs a name.");
  if (!d.refdes_prefix.trim()) out.push("The part needs a reference prefix.");
  return out;
}

// ---- Scenes (A4 render-object tables).

export const EDITOR_LAYERS: RenderLayer[] = [
  { key: "courtyard", order: 0, alpha: 1 },
  { key: "body", order: 1, alpha: 1 },
  { key: "pins", order: 2, alpha: 1 },
  { key: "labels", order: 3, alpha: 1, annotation: true },
];
/** The default symbol line width in mm (what a stroke width of 0 means). */
const LINE = 0.254;
const strokeMm = (g: Graphic) => (g.kind !== "text" && g.stroke && g.stroke.width > 0 ? toMm(g.stroke.width) : LINE);

function arcPts(c: Pt, r: number, a0: number, a1: number): Pt[] {
  const sweep = a1 - a0 || 360;
  const n = Math.max(8, Math.ceil(Math.abs(sweep) / 10));
  // IR angles run counter-clockwise with Y up; view Y points down.
  return Array.from({ length: n + 1 }, (_, i) => {
    const a = ((a0 + (sweep * i) / n) * Math.PI) / 180;
    return [c[0] + r * Math.cos(a), c[1] - r * Math.sin(a)] as Pt;
  });
}

function graphicObject(g: Graphic, i: number): RenderObject {
  const w = strokeMm(g);
  const line = "--c-sym-line";
  const fill = "--c-sym-fill";
  let ops: DrawOp[] = [];
  let pts: Pt[] = [];
  let closed = false;
  switch (g.kind) {
    case "line":
      pts = [v(g.a), v(g.b)];
      ops = [{ t: "seg", a: pts[0], b: pts[1], width: w, colour: line }];
      break;
    case "polyline":
      pts = g.points.map(v);
      ops = pts.slice(1).map((p, k) => ({ t: "seg", a: pts[k], b: p, width: w, colour: line }));
      break;
    case "polygon":
      pts = g.points.map(v);
      closed = true;
      ops = [{ t: "poly", pts, fill: g.fill ? fill : undefined, stroke: line, lineWidth: w }];
      break;
    case "rectangle": {
      const a = v(g.min);
      const b = v(g.max);
      pts = [a, [b[0], a[1]], b, [a[0], b[1]]];
      closed = true;
      ops = [{ t: "rect", x: Math.min(a[0], b[0]), y: Math.min(a[1], b[1]), w: Math.abs(b[0] - a[0]), h: Math.abs(b[1] - a[1]), radius: 0, fill: g.fill ? fill : undefined, stroke: line, lineWidth: w }];
      break;
    }
    case "circle": {
      const c = v(g.center);
      const r = toMm(g.radius);
      pts = arcPts(c, r, 0, 360);
      ops = [{ t: "circle", cx: c[0], cy: c[1], r, fill: g.fill ? fill : undefined, stroke: line, lineWidth: w }];
      break;
    }
    case "arc":
      pts = arcPts(v(g.center), toMm(g.radius), g.start_deg, g.end_deg);
      ops = pts.slice(1).map((p, k) => ({ t: "seg", a: pts[k], b: p, width: w, colour: line }));
      break;
    case "text": {
      const p = v(g.position);
      const s = toMm(g.size);
      pts = [
        [p[0], p[1] - s],
        [p[0] + s * 0.6 * Math.max(1, g.text.length), p[1]],
      ];
      ops = [{ t: "text", x: p[0], y: p[1], text: g.text, size: s, colour: "--c-text", minPx: 3 }];
      break;
    }
  }
  const segs = closed && pts.length ? [...pts, pts[0]] : pts;
  const filled = closed && "fill" in g && g.fill === true;
  const hit =
    g.kind === "text"
      ? (x: number, y: number, tol: number) => x >= pts[0][0] - tol && x <= pts[1][0] + tol && y >= pts[0][1] - tol && y <= pts[1][1] + tol
      : (x: number, y: number, tol: number) => (filled && inPoly(pts, x, y)) || segs.slice(1).some((p, k) => segDist(x, y, segs[k], p) <= w / 2 + tol);
  return { key: `g:${i}`, ref: { kind: "graphic", id: String(i) }, layerKey: "body", bbox: boxOfPts(pts.length ? pts : [[0, 0]], w), hit, pathPts: segs, ops, sig: JSON.stringify(g) };
}

const DIR: Record<string, Pt> = { right: [1, 0], left: [-1, 0], up: [0, -1], down: [0, 1] };
/** Pin line from the connection point towards the body (view mm). */
export function pinLine(p: SymbolPin): [Pt, Pt] {
  const a = v(p.position);
  const d = DIR[p.orientation ?? "right"];
  const l = toMm(p.length ?? 0);
  return [a, [a[0] + d[0] * l, a[1] + d[1] * l]];
}

export function symbolScene(s: SymbolDef): Scene {
  const objects: RenderObject[] = (s.graphics ?? []).map(graphicObject);
  for (const p of s.pins) {
    const [a, b] = pinLine(p);
    const r = 0.3;
    const ops: DrawOp[] = [
      { t: "seg", a, b, width: LINE, colour: "--c-wire" },
      { t: "circle", cx: a[0], cy: a[1], r, stroke: "--c-wire", lineWidth: 0.1 },
    ];
    objects.push({ key: `pin:${p.number}`, ref: { kind: "pin", id: p.number }, layerKey: "pins", bbox: boxOfPts([a, b], r), hit: (x, y, tol) => segDist(x, y, a, b) <= r + tol, pathPts: [a, b], ops, sig: JSON.stringify(p) });
    const labels: DrawOp[] = [];
    if (p.number_visible ?? true) labels.push({ t: "label", x: (a[0] + b[0]) / 2, y: (a[1] + b[1]) / 2 - 0.6, text: p.number, px: 10, colour: "--c-text", fitW: 100, fitH: 100, angle: 0 });
    if ((p.name_visible ?? true) && p.name) {
      const d = DIR[p.orientation ?? "right"];
      labels.push({ t: "label", x: b[0] + d[0] * 1.5, y: b[1] + d[1] * 1.5, text: p.name, px: 10, colour: "--c-label", fitW: 100, fitH: 100, angle: 0 });
    }
    if (labels.length) objects.push({ key: `pinlabel:${p.number}`, ref: null, layerKey: "labels", bbox: boxOfPts([a, b], 1.5), hit: () => false, ops: labels, sig: `${JSON.stringify(p)}|l` });
  }
  return { layers: EDITOR_LAYERS, objects, bounds: boundsOf(objects) };
}

/** Outline of a pad (view mm), for selection and hit testing. */
export function padOutline(p: PadDef): Pt[] {
  const [cx, cy] = v(p.position);
  const hw = toMm(p.size_x) / 2;
  const hh = toMm(p.size_y) / 2;
  return [
    [cx - hw, cy - hh],
    [cx + hw, cy - hh],
    [cx + hw, cy + hh],
    [cx - hw, cy + hh],
  ];
}

export function footprintScene(f: FootprintDef): Scene {
  const objects: RenderObject[] = [];
  const cw = toMm(f.courtyard_half_w);
  const ch = toMm(f.courtyard_half_h);
  const court: Pt[] = [
    [-cw, -ch],
    [cw, -ch],
    [cw, ch],
    [-cw, ch],
  ];
  objects.push({ key: "courtyard", ref: null, layerKey: "courtyard", bbox: boxOfPts(court), hit: () => false, ops: [{ t: "poly", pts: court, stroke: "--c-courtyard", lineWidth: 0.05 }], sig: `${cw}|${ch}` });
  for (const p of f.pads) {
    const [cx, cy] = v(p.position);
    const w = toMm(p.size_x);
    const h = toMm(p.size_y);
    const outline = padOutline(p);
    const ops: DrawOp[] =
      p.shape === "circle"
        ? [{ t: "circle", cx, cy, r: Math.min(w, h) / 2, fill: "--c-pad" }]
        : [{ t: "rect", x: cx - w / 2, y: cy - h / 2, w, h, radius: p.shape === "oval" ? Math.min(w, h) / 2 : 0, fill: "--c-pad" }];
    if (p.kind === "tht" && p.drill) ops.push({ t: "circle", cx, cy, r: toMm(p.drill) / 2, fill: "--c-board" });
    objects.push({ key: `pad:${p.number}`, ref: { kind: "pad", id: p.number }, layerKey: "body", bbox: boxOfPts(outline), hit: (x, y, tol) => Math.abs(x - cx) <= w / 2 + tol && Math.abs(y - cy) <= h / 2 + tol, selPoly: outline, ops, sig: JSON.stringify(p) });
    objects.push({ key: `padnum:${p.number}`, ref: null, layerKey: "labels", bbox: boxOfPts(outline), hit: () => false, ops: [{ t: "label", x: cx, y: cy, text: p.number, px: 10, colour: "--c-text", fitW: w, fitH: h, angle: 0 }], sig: `${p.number}|${cx}|${cy}|${w}|${h}` });
  }
  return { layers: EDITOR_LAYERS, objects, bounds: boundsOf(objects) };
}

function boundsOf(objects: RenderObject[]): Box | null {
  let b: Box | null = null;
  for (const o of objects)
    b = b ? { minX: Math.min(b.minX, o.bbox.minX), minY: Math.min(b.minY, o.bbox.minY), maxX: Math.max(b.maxX, o.bbox.maxX), maxY: Math.max(b.maxY, o.bbox.maxY) } : { ...o.bbox };
  return b;
}

/** Move a pin or pad of the draft to `to` (nm). */
export function moveItem(d: ComponentDef, kind: "pin" | "pad", id: string, to: Point): ComponentDef {
  return kind === "pin"
    ? { ...d, symbol: { ...d.symbol, pins: d.symbol.pins.map((p) => (p.number === id ? { ...p, position: to } : p)) } }
    : { ...d, footprint: { ...d.footprint, pads: d.footprint.pads.map((p) => (p.number === id ? { ...p, position: to } : p)) } };
}
/** Snap a view-mm point to `grid` mm and convert to IR nm (Y up). */
export function snapToIr(x: number, y: number, grid: number): Point {
  const s = (n: number) => Math.round(n / grid) * grid;
  return { x: Math.round(s(x) * NM_PER_MM), y: Math.round(-s(y) * NM_PER_MM) + 0 };
}
