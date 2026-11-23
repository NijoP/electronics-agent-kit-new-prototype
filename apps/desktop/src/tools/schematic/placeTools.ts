// Schematic place tools (Phase 1 slice B4): pure helpers that turn tool clicks into typed engine
// commands. Wire legs (orthogonal, 45° or free, with the bend flipped by Space), the Shift axis
// lock, bus entries, sheet graphics and text, and "fan out net labels / no-connects" for a part.
// Nothing here writes the project: every result is a `Command` the editor sends to the engine.
import type { Command, Graphic, Id, NetScope, Nm, Point, PowerStyle, Snapshot, SymbolGeom } from "../../types";

const same = (a: Point, b: Point) => a.x === b.x && a.y === b.y;

/** How a wire (or bus) leg goes from the last point to the pointer. */
export type WireMode = "90" | "45" | "free";
export const WIRE_MODES: readonly { id: WireMode; label: string }[] = [
  { id: "90", label: "orthogonal (90°)" },
  { id: "45", label: "45°" },
  { id: "free", label: "any angle" },
];
/** The mode after `m` (the "/" key cycles them). */
export const nextWireMode = (m: WireMode): WireMode => (m === "90" ? "45" : m === "45" ? "free" : "90");

/**
 * The segments from `a` to `b` in `mode`. Orthogonal: horizontal then vertical (vertical first
 * when `flip`). 45°: the diagonal part first, then the straight rest (straight first when `flip`).
 * Free: one segment. A leg that is already straight (or exactly diagonal in 45° mode) is one
 * segment. Equal points give no segment.
 */
export function wireLegs(a: Point, b: Point, mode: WireMode, flip = false): [Point, Point][] {
  if (same(a, b)) return [];
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  if (mode === "free" || dx === 0 || dy === 0) return [[a, b]];
  if (mode === "90") {
    const corner = flip ? { x: a.x, y: b.y } : { x: b.x, y: a.y };
    return [
      [a, corner],
      [corner, b],
    ];
  }
  if (Math.abs(dx) === Math.abs(dy)) return [[a, b]];
  const d = Math.min(Math.abs(dx), Math.abs(dy));
  const diag = { x: Math.sign(dx) * d, y: Math.sign(dy) * d };
  // Diagonal first: the corner is `a` moved along the diagonal; straight first: `b` moved back.
  const corner = flip ? { x: b.x - diag.x, y: b.y - diag.y } : { x: a.x + diag.x, y: a.y + diag.y };
  return [
    [a, corner],
    [corner, b],
  ];
}

/** Shift lock: keep `to` on the dominant axis through `from` (|dx| > |dy| locks Y, else X). */
export function constrainAxis(from: Point, to: Point): Point {
  return Math.abs(to.x - from.x) > Math.abs(to.y - from.y) ? { x: to.x, y: from.y } : { x: from.x, y: to.y };
}

/** Length of a standard bus entry leg on each axis: 0.1 in. */
export const BUS_ENTRY_NM: Nm = 2_540_000;
/** The four 45° directions a bus entry can take, turned by R: up-right, down-right, down-left, up-left. */
const ENTRY_DIRS: readonly Point[] = [
  { x: 1, y: 1 },
  { x: 1, y: -1 },
  { x: -1, y: -1 },
  { x: -1, y: 1 },
];
export const ENTRY_DIR_LABELS = ["up-right", "down-right", "down-left", "up-left"] as const;
/** Where the member wire attaches to a bus entry whose bus end is `at`. */
export const busEntryWirePoint = (at: Point, dir: number): Point => {
  const d = ENTRY_DIRS[((dir % 4) + 4) % 4];
  return { x: at.x + d.x * BUS_ENTRY_NM, y: at.y + d.y * BUS_ENTRY_NM };
};

/** True when `p` lies on the segment a-b (exact for grid points, any angle). */
export function onSegment(p: Point, a: Point, b: Point): boolean {
  const cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
  if (cross !== 0) return false;
  return p.x >= Math.min(a.x, b.x) && p.x <= Math.max(a.x, b.x) && p.y >= Math.min(a.y, b.y) && p.y <= Math.max(a.y, b.y);
}

/** The bus of `sheet` whose polyline passes through `p`, if any. */
export function busAt(snapshot: Snapshot, sheet: Id, p: Point): Id | null {
  for (const b of Object.values(snapshot.project.schematic.buses ?? {})) {
    if (b.sheet_id !== sheet) continue;
    for (let i = 1; i < b.points.length; i++) if (onSegment(p, b.points[i - 1], b.points[i])) return b.id;
  }
  return null;
}

/** Graphic and text tools. */
export type GraphicTool = "line" | "rect" | "circle" | "text";
export const GRAPHIC_TOOLS: readonly GraphicTool[] = ["line", "rect", "circle", "text"];

/**
 * The graphic a two-click tool draws from `a` to `b` (text: one click at `a`). `null` when the
 * shape would be degenerate (the engine refuses those) or the text is empty.
 */
export function graphicFrom(tool: GraphicTool, a: Point, b: Point, text = "", size: Nm = 1_800_000): Graphic | null {
  const stroke = { width: 0, style: "solid" as const };
  switch (tool) {
    case "line":
      return same(a, b) ? null : { kind: "line", a, b, stroke };
    case "rect": {
      if (a.x === b.x || a.y === b.y) return null;
      return { kind: "rectangle", min: { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y) }, max: { x: Math.max(a.x, b.x), y: Math.max(a.y, b.y) }, fill: false, stroke };
    }
    case "circle": {
      const r = Math.round(Math.hypot(b.x - a.x, b.y - a.y));
      return r > 0 ? { kind: "circle", center: a, radius: r, fill: false, stroke } : null;
    }
    case "text": {
      const t = text.trim();
      return t ? { kind: "text", position: a, text: t, size, rotation: "R0" } : null;
    }
  }
}

/** Net label variants of the label split button: how far each reaches (ADR-0040). */
export type LabelTool = "label" | "netport" | "offpage";
export const LABEL_SCOPE: Record<LabelTool, NetScope> = { label: "global", netport: "local", offpage: "off_page" };

/** One entry of the user's power-port list (Settings › Power ports). */
export interface PowerPreset {
  name: string;
  style: PowerStyle;
}

/** The AddNetLabel a power-port click sends. */
export const powerPortCommand = (sheet: Id, p: PowerPreset, at: Point): Command => ({
  type: "AddNetLabel",
  sheet_id: sheet,
  name: p.name.trim(),
  position: at,
  kind: "power",
  power_style: p.style,
});

/** Fan-out stub length: 0.1 in, one standard pin pitch. */
export const FANOUT_NM: Nm = 2_540_000;

/** Pins of a placed part that nothing touches: no wire, label, no-connect or other pin there. */
export function unconnectedPins(snapshot: Snapshot, g: SymbolGeom): SymbolGeom["pins"] {
  const sch = snapshot.project.schematic;
  const wires = Object.values(sch.wires).filter((w) => w.sheet_id === g.sheet_id);
  const labels = Object.values(sch.labels).filter((l) => l.sheet_id === g.sheet_id);
  const entries = Object.values(sch.bus_entries ?? {}).filter((e) => e.sheet_id === g.sheet_id);
  const others = snapshot.geometry.symbols.filter((s) => s.sheet_id === g.sheet_id && s.id !== g.id).flatMap((s) => s.pins.map((p) => p.position));
  const marked = new Set(Object.values(sch.no_connects).filter((n) => n.symbol_id === g.id).map((n) => n.pin));
  return g.pins.filter(
    (p) =>
      !marked.has(p.number) &&
      !wires.some((w) => onSegment(p.position, w.a, w.b)) &&
      !labels.some((l) => same(l.position, p.position)) &&
      !entries.some((e) => same(e.wire_point, p.position)) &&
      !others.some((o) => same(o, p.position)),
  );
}

/** Outward direction of a pin from its part's body centre (one axis, unit length). */
function outward(g: SymbolGeom, at: Point): Point {
  const dx = at.x - g.position.x;
  const dy = at.y - g.position.y;
  // Compare against the body's half-extents so pins of a tall part go sideways.
  const fx = g.half_w > 0 ? Math.abs(dx) / g.half_w : Math.abs(dx);
  const fy = g.half_h > 0 ? Math.abs(dy) / g.half_h : Math.abs(dy);
  if (fx >= fy) return { x: dx < 0 ? -1 : 1, y: 0 };
  return { x: 0, y: dy < 0 ? -1 : 1 };
}

/** Net name a fanned-out label gets: the pin name, or `<refdes>_<pin number>` when it has none. */
export function fanoutName(g: SymbolGeom, pin: { number: string; name: string }): string {
  const n = pin.name.trim().replace(/\s+/g, "_");
  return !n || n === "~" || n === pin.number ? `${g.refdes}_${pin.number}` : n;
}

/**
 * "Fan out net labels / no-connects" (EasyEDA's part context menu): one Batch that gives every
 * unconnected pin of the part a short wire stub and a net label named after the pin, or a
 * no-connect marker. Returns the reason instead when there is nothing to do. Pins named NC get no
 * label: one shared "NC" net would join them.
 */
export function fanOutCommand(snapshot: Snapshot, symbolId: Id, kind: "labels" | "noconnects"): Command | string {
  const g = snapshot.geometry.symbols.find((s) => s.id === symbolId);
  if (!g) return "Select a placed part first.";
  let pins = unconnectedPins(snapshot, g);
  if (kind === "labels") pins = pins.filter((p) => p.name.trim().toUpperCase() !== "NC");
  if (!pins.length) return `Every pin of ${g.refdes} is already connected${kind === "labels" ? " or named NC" : ""}.`;
  const commands: Command[] =
    kind === "noconnects"
      ? pins.map((p) => ({ type: "SetNoConnect" as const, symbol_id: g.id, pin: p.number }))
      : pins.flatMap((p) => {
          const d = outward(g, p.position);
          const end = { x: p.position.x + d.x * FANOUT_NM, y: p.position.y + d.y * FANOUT_NM };
          return [
            { type: "AddWire" as const, sheet_id: g.sheet_id, a: p.position, b: end },
            { type: "AddNetLabel" as const, sheet_id: g.sheet_id, name: fanoutName(g, p), position: end, kind: "net" as const },
          ];
        });
  return { type: "Batch", commands, label: `Fan out ${kind === "labels" ? "net labels" : "no-connects"} on ${g.refdes}` };
}
