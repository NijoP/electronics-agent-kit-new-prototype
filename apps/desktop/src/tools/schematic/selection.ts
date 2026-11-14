// Pure schematic tool reducers: selection, marquee, and the command lists for moving,
// rotating, deleting and pasting a selection. They only compute typed engine commands; the
// caller submits them as one `Batch` (ADR-0013), so a multi-object edit is one revision and
// one undo step. The engine snapshot stays the only design model.
import type { Command, Id, NetLabel, Point, Rotation, Snapshot, SymbolGeom, Wire } from "../../types";

export type Kind = "symbol" | "wire" | "label";
export interface Selection {
  symbols: Id[];
  wires: Id[];
  labels: Id[];
}
export const EMPTY: Selection = { symbols: [], wires: [], labels: [] };

const listOf = (k: Kind): keyof Selection => (k === "symbol" ? "symbols" : k === "wire" ? "wires" : "labels");

export const isEmpty = (s: Selection) => !s.symbols.length && !s.wires.length && !s.labels.length;
export const count = (s: Selection) => s.symbols.length + s.wires.length + s.labels.length;
export const has = (s: Selection, k: Kind, id: Id) => s[listOf(k)].includes(id);

/** Click selects one object; shift-click (additive) toggles it in the current selection. */
export function pick(s: Selection, k: Kind, id: Id, additive: boolean): Selection {
  if (!additive) return { ...EMPTY, [listOf(k)]: [id] };
  const l = s[listOf(k)];
  return { ...s, [listOf(k)]: l.includes(id) ? l.filter((x) => x !== id) : [...l, id] };
}

/** Union of two selections (shift-marquee adds to the selection). */
export function union(a: Selection, b: Selection): Selection {
  const u = (x: Id[], y: Id[]) => [...x, ...y.filter((i) => !x.includes(i))];
  return { symbols: u(a.symbols, b.symbols), wires: u(a.wires, b.wires), labels: u(a.labels, b.labels) };
}

/** Drop ids that no longer exist (after undo, delete, or a change by another actor). */
export function prune(s: Selection, snap: Snapshot): Selection {
  const { symbols, wires, labels } = snap.project.schematic;
  const next = {
    symbols: s.symbols.filter((i) => i in symbols),
    wires: s.wires.filter((i) => i in wires),
    labels: s.labels.filter((i) => i in labels),
  };
  return count(next) === count(s) ? s : next;
}

export interface Rect {
  min: Point;
  max: Point;
}
export const rectOf = (a: Point, b: Point): Rect => ({
  min: { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y) },
  max: { x: Math.max(a.x, b.x), y: Math.max(a.y, b.y) },
});
const inside = (r: Rect, p: Point) => p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y;

const sheetWires = (snap: Snapshot, sheet: Id) => Object.values(snap.project.schematic.wires).filter((w) => w.sheet_id === sheet);
const sheetLabels = (snap: Snapshot, sheet: Id) => Object.values(snap.project.schematic.labels).filter((l) => l.sheet_id === sheet);
const sheetSymbols = (snap: Snapshot, sheet: Id) => snap.geometry.symbols.filter((s) => s.sheet_id === sheet);

/** Objects entirely inside the marquee: symbol bodies and pins, both wire ends, label anchors. */
export function marquee(snap: Snapshot, sheet: Id, r: Rect): Selection {
  const symInside = (s: SymbolGeom) =>
    inside(r, { x: s.position.x - s.half_w, y: s.position.y - s.half_h }) &&
    inside(r, { x: s.position.x + s.half_w, y: s.position.y + s.half_h }) &&
    s.pins.every((p) => inside(r, p.position));
  return {
    symbols: sheetSymbols(snap, sheet).filter(symInside).map((s) => s.id),
    wires: sheetWires(snap, sheet).filter((w) => inside(r, w.a) && inside(r, w.b)).map((w) => w.id),
    labels: sheetLabels(snap, sheet).filter((l) => inside(r, l.position)).map((l) => l.id),
  };
}

const key = (p: Point) => `${p.x},${p.y}`;
const add = (p: Point, d: Point): Point => ({ x: p.x + d.x, y: p.y + d.y });
const same = (a: Point, b: Point) => a.x === b.x && a.y === b.y;

/**
 * Rubber-band wires: given where some connection points go (`moved`, old position -> new),
 * move every wire end sitting on one of them. Wires whose both ends land on the same point are
 * deleted (the engine refuses zero-length wires). Wire edits come first in the returned list so
 * a later `MoveSymbol` sees its pins already released.
 */
function rubberBand(wires: Wire[], moved: Map<string, Point>, whole: Set<Id>, delta: Point | null): Command[] {
  const out: Command[] = [];
  for (const w of wires) {
    const a = whole.has(w.id) && delta ? add(w.a, delta) : (moved.get(key(w.a)) ?? w.a);
    const b = whole.has(w.id) && delta ? add(w.b, delta) : (moved.get(key(w.b)) ?? w.b);
    if (same(a, w.a) && same(b, w.b)) continue;
    out.push(same(a, b) ? { type: "DeleteWire", wire_id: w.id } : { type: "MoveWire", wire_id: w.id, a, b });
  }
  return out;
}

/** Commands that move the selection by `delta`, dragging attached wire ends and pin labels. */
export function moveCommands(snap: Snapshot, sheet: Id, sel: Selection, delta: Point): Command[] {
  if ((delta.x === 0 && delta.y === 0) || isEmpty(sel)) return [];
  const { symbols, labels } = snap.project.schematic;
  const moved = new Map<string, Point>();
  for (const g of sheetSymbols(snap, sheet).filter((g) => sel.symbols.includes(g.id)))
    for (const p of g.pins) moved.set(key(p.position), add(p.position, delta));
  const wires = sheetWires(snap, sheet);
  for (const w of wires.filter((w) => sel.wires.includes(w.id))) {
    moved.set(key(w.a), add(w.a, delta));
    moved.set(key(w.b), add(w.b, delta));
  }
  for (const id of sel.labels) {
    const l = labels[id];
    if (l) moved.set(key(l.position), add(l.position, delta));
  }
  const out = rubberBand(wires, moved, new Set(sel.wires), delta);
  // Labels: selected ones, plus labels sitting on a moved pin or wire end.
  for (const l of sheetLabels(snap, sheet)) {
    const to = sel.labels.includes(l.id) ? add(l.position, delta) : moved.get(key(l.position));
    if (to && !same(to, l.position)) out.push({ type: "MoveNetLabel", label_id: l.id, position: to });
  }
  for (const id of sel.symbols) {
    const s = symbols[id];
    if (s) out.push({ type: "MoveSymbol", symbol_id: id, position: add(s.position, delta) });
  }
  return out;
}

const ORDER: Rotation[] = ["R0", "R90", "R180", "R270"];
/** A quarter turn counter-clockwise about `about` (EAK-IR's R90). */
const quarter = (p: Point, about: Point): Point => ({ x: about.x - (p.y - about.y), y: about.y + (p.x - about.x) });

/** Rotate each selected symbol a quarter turn about its origin, dragging attached wire ends. */
export function rotateCommands(snap: Snapshot, sheet: Id, sel: Selection): Command[] {
  const { symbols } = snap.project.schematic;
  const moved = new Map<string, Point>();
  const out: Command[] = [];
  for (const g of sheetSymbols(snap, sheet).filter((g) => sel.symbols.includes(g.id))) {
    for (const p of g.pins) moved.set(key(p.position), quarter(p.position, g.position));
    const s = symbols[g.id];
    if (s) out.push({ type: "RotateSymbol", symbol_id: s.id, rotation: ORDER[(ORDER.indexOf(s.rotation) + 1) % 4] });
  }
  if (!out.length) return [];
  const wires = rubberBand(sheetWires(snap, sheet), moved, new Set(), null);
  const labels: Command[] = sheetLabels(snap, sheet).flatMap((l: NetLabel) => {
    const to = moved.get(key(l.position));
    return to && !same(to, l.position) ? [{ type: "MoveNetLabel" as const, label_id: l.id, position: to }] : [];
  });
  return [...wires, ...labels, ...out];
}

/** Delete everything selected. */
export function deleteCommands(sel: Selection): Command[] {
  return [
    ...sel.labels.map((id): Command => ({ type: "DeleteNetLabel", label_id: id })),
    ...sel.wires.map((id): Command => ({ type: "DeleteWire", wire_id: id })),
    ...sel.symbols.map((id): Command => ({ type: "DeleteSymbol", symbol_id: id })),
  ];
}

/** Clipboard: what the selection is, by value, so it can be pasted after the originals change. */
export interface Clip {
  symbols: { component_id: string; position: Point; rotation: Rotation; value: string }[];
  wires: { a: Point; b: Point }[];
  labels: { name: string; kind: "net" | "power"; position: Point }[];
}

export function copy(snap: Snapshot, sel: Selection): Clip | null {
  const { symbols, wires, labels } = snap.project.schematic;
  const clip: Clip = {
    symbols: sel.symbols.flatMap((id) => {
      const s = symbols[id];
      return s ? [{ component_id: s.component_id, position: s.position, rotation: s.rotation, value: s.value }] : [];
    }),
    wires: sel.wires.flatMap((id) => (wires[id] ? [{ a: wires[id].a, b: wires[id].b }] : [])),
    labels: sel.labels.flatMap((id) => {
      const l = labels[id];
      return l ? [{ name: l.name, kind: l.kind, position: l.position }] : [];
    }),
  };
  return clip.symbols.length + clip.wires.length + clip.labels.length ? clip : null;
}

/** Paste a clip onto `sheet`, offset by `delta`. New symbols get fresh reference designators. */
export function pasteCommands(clip: Clip, sheet: Id, delta: Point): Command[] {
  return [
    ...clip.symbols.map(
      (s): Command => ({
        type: "PlaceSymbol",
        sheet_id: sheet,
        component_id: s.component_id,
        position: add(s.position, delta),
        rotation: s.rotation,
        refdes: null,
        value: s.value,
      }),
    ),
    ...clip.wires.map((w): Command => ({ type: "AddWire", sheet_id: sheet, a: add(w.a, delta), b: add(w.b, delta) })),
    ...clip.labels.map(
      (l): Command => ({ type: "AddNetLabel", sheet_id: sheet, name: l.name, kind: l.kind, position: add(l.position, delta) }),
    ),
  ];
}

/** One engine command for a list: the single command itself, or an atomic `Batch`. */
export function asBatch(commands: Command[], label: string): Command | null {
  if (!commands.length) return null;
  return commands.length === 1 ? commands[0] : { type: "Batch", commands, label };
}

/**
 * Where a paste lands: the clip's top-left anchor moves to the cursor (snapped to the grid);
 * without a cursor position it is offset by four grid steps so it never covers the original.
 */
export function pasteOffset(clip: Clip, at: Point | null, grid: number): Point {
  const pts = [...clip.symbols.map((s) => s.position), ...clip.wires.flatMap((w) => [w.a, w.b]), ...clip.labels.map((l) => l.position)];
  if (!at || !pts.length) return { x: 4 * grid, y: 4 * grid };
  const anchor = { x: Math.min(...pts.map((p) => p.x)), y: Math.min(...pts.map((p) => p.y)) };
  const d = { x: Math.round((at.x - anchor.x) / grid) * grid, y: Math.round((at.y - anchor.y) / grid) * grid };
  return d.x === 0 && d.y === 0 ? { x: 4 * grid, y: 4 * grid } : d;
}
