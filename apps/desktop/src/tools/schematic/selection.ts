// Pure schematic tool reducers: selection, marquee, and the command lists for moving,
// rotating, deleting and pasting a selection. They only compute typed engine commands; the
// caller submits them as one `Batch` (ADR-0013), so a multi-object edit is one revision and
// one undo step. The engine snapshot stays the only design model.
import type { Command, FlipAxis, Id, NetLabel, NetScope, Point, PowerStyle, Rotation, SchematicGroup, Snapshot, SymbolGeom, Wire } from "../../types";

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

/** How a click combines with the current selection: plain replaces, Shift adds, Ctrl/Cmd toggles. */
export type PickMode = "replace" | "add" | "toggle";
export const pickMode = (e: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }): PickMode =>
  e.ctrlKey || e.metaKey ? "toggle" : e.shiftKey ? "add" : "replace";

/** Click selects exactly one object; Shift adds it to the selection; Ctrl/Cmd toggles it. */
export function pick(s: Selection, k: Kind, id: Id, mode: PickMode): Selection {
  if (mode === "replace") return { ...EMPTY, [listOf(k)]: [id] };
  const l = s[listOf(k)];
  if (l.includes(id)) return mode === "add" ? s : { ...s, [listOf(k)]: l.filter((x) => x !== id) };
  return { ...s, [listOf(k)]: [...l, id] };
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

/**
 * Move symbols to new origins, each by its own delta (Inspector X/Y on several parts), dragging
 * attached wire ends and pin labels like `moveCommands`; for one symbol it is `moveCommands`
 * with the delta to its target. Wire edits come first, the moves last.
 */
export function moveSymbolsTo(snap: Snapshot, targets: { id: Id; position: Point }[]): Command[] {
  const { symbols } = snap.project.schematic;
  const moves = targets.flatMap((t) => {
    const s = symbols[t.id];
    return s && !same(s.position, t.position) ? [{ id: t.id, sheet: s.sheet_id, delta: { x: t.position.x - s.position.x, y: t.position.y - s.position.y } }] : [];
  });
  const out: Command[] = [];
  for (const sheet of [...new Set(moves.map((m) => m.sheet))]) {
    const moved = new Map<string, Point>();
    for (const m of moves.filter((m) => m.sheet === sheet)) {
      const g = sheetSymbols(snap, sheet).find((g) => g.id === m.id);
      for (const p of g?.pins ?? []) moved.set(key(p.position), add(p.position, m.delta));
    }
    out.push(...rubberBand(sheetWires(snap, sheet), moved, new Set(), null));
    for (const l of sheetLabels(snap, sheet)) {
      const to = moved.get(key(l.position));
      if (to && !same(to, l.position)) out.push({ type: "MoveNetLabel", label_id: l.id, position: to });
    }
  }
  for (const m of moves) out.push({ type: "MoveSymbol", symbol_id: m.id, position: add(symbols[m.id].position, m.delta) });
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

/** Mirror point `p` about the vertical (horizontal flip) or horizontal axis through `about`. */
const mirror = (p: Point, about: Point, axis: FlipAxis): Point =>
  axis === "horizontal" ? { x: 2 * about.x - p.x, y: p.y } : { x: p.x, y: 2 * about.y - p.y };

/**
 * Flip each selected symbol about its own origin (`FlipSymbol`, ADR-0016), dragging attached
 * wire ends and pin labels to the mirrored pins. Wire edits come first, the flips last.
 */
export function flipCommands(snap: Snapshot, sheet: Id, sel: Selection, axis: FlipAxis): Command[] {
  const moved = new Map<string, Point>();
  const out: Command[] = [];
  for (const g of sheetSymbols(snap, sheet).filter((g) => sel.symbols.includes(g.id))) {
    for (const p of g.pins) moved.set(key(p.position), mirror(p.position, g.position, axis));
    out.push({ type: "FlipSymbol", symbol_id: g.id, axis });
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
  /** Labels keep their scope and power style, so a paste on another sheet joins the same nets by label only. */
  labels: { name: string; kind: "net" | "power"; position: Point; scope?: NetScope; power_style?: PowerStyle }[];
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
      return l ? [{ name: l.name, kind: l.kind, position: l.position, ...(l.scope ? { scope: l.scope } : {}), ...(l.power_style ? { power_style: l.power_style } : {}) }] : [];
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
      (l): Command => ({
        type: "AddNetLabel",
        sheet_id: sheet,
        name: l.name,
        kind: l.kind,
        position: add(l.position, delta),
        ...(l.scope ? { scope: l.scope } : {}),
        ...(l.power_style ? { power_style: l.power_style } : {}),
      }),
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

// ---- Slice B3: arrange (align, distribute), array, find similar, selection filter ----------

/**
 * Move several symbols, each by its own delta, in one command list: attached wire ends and pin
 * labels follow (rubber-banding), wire edits first, the symbol moves last.
 */
export function moveEachCommands(snap: Snapshot, sheet: Id, deltas: Map<Id, Point>): Command[] {
  const { symbols } = snap.project.schematic;
  const moved = new Map<string, Point>();
  for (const g of sheetSymbols(snap, sheet)) {
    const d = deltas.get(g.id);
    if (d && (d.x !== 0 || d.y !== 0)) for (const p of g.pins) moved.set(key(p.position), add(p.position, d));
  }
  const out = rubberBand(sheetWires(snap, sheet), moved, new Set(), null);
  for (const l of sheetLabels(snap, sheet)) {
    const to = moved.get(key(l.position));
    if (to && !same(to, l.position)) out.push({ type: "MoveNetLabel", label_id: l.id, position: to });
  }
  for (const [id, d] of deltas) {
    const s = symbols[id];
    if (s && (d.x !== 0 || d.y !== 0)) out.push({ type: "MoveSymbol", symbol_id: id, position: add(s.position, d) });
  }
  return out;
}

/** Edge or centre line the selected parts are aligned to (EAK-IR is Y up: "top" is the largest y). */
export type Align = "left" | "right" | "top" | "bottom" | "center-x" | "center-y";

const selectedGeoms = (snap: Snapshot, sheet: Id, sel: Selection) => sheetSymbols(snap, sheet).filter((g) => sel.symbols.includes(g.id));

/**
 * Align the selected parts' bodies to the outermost edge (or the common centre line) of the
 * selection. Needs two or more parts; wires and labels on their pins follow.
 */
export function alignCommands(snap: Snapshot, sheet: Id, sel: Selection, how: Align): Command[] {
  const gs = selectedGeoms(snap, sheet, sel);
  if (gs.length < 2) return [];
  const left = Math.min(...gs.map((g) => g.position.x - g.half_w));
  const right = Math.max(...gs.map((g) => g.position.x + g.half_w));
  const bottom = Math.min(...gs.map((g) => g.position.y - g.half_h));
  const top = Math.max(...gs.map((g) => g.position.y + g.half_h));
  const deltas = new Map<Id, Point>();
  for (const g of gs) {
    const { x, y } = g.position;
    const d =
      how === "left" ? { x: left - (x - g.half_w), y: 0 }
      : how === "right" ? { x: right - (x + g.half_w), y: 0 }
      : how === "bottom" ? { x: 0, y: bottom - (y - g.half_h) }
      : how === "top" ? { x: 0, y: top - (y + g.half_h) }
      : how === "center-x" ? { x: Math.round((left + right) / 2) - x, y: 0 }
      : { x: 0, y: Math.round((bottom + top) / 2) - y };
    deltas.set(g.id, d);
  }
  return moveEachCommands(snap, sheet, deltas);
}

/**
 * Space the selected parts evenly along an axis: the two outermost stay, the others move so
 * their origins are equally spaced between them (whole nanometres). Needs three or more parts.
 */
export function distributeCommands(snap: Snapshot, sheet: Id, sel: Selection, axis: "x" | "y"): Command[] {
  const gs = selectedGeoms(snap, sheet, sel).sort((a, b) => a.position[axis] - b.position[axis] || a.id.localeCompare(b.id));
  if (gs.length < 3) return [];
  const first = gs[0].position[axis];
  const span = gs[gs.length - 1].position[axis] - first;
  const deltas = new Map<Id, Point>();
  gs.forEach((g, i) => {
    const d = Math.round(first + (span * i) / (gs.length - 1)) - g.position[axis];
    deltas.set(g.id, axis === "x" ? { x: d, y: 0 } : { x: 0, y: d });
  });
  return moveEachCommands(snap, sheet, deltas);
}

/** Largest array the editor sends in one Batch. */
export const MAX_ARRAY_COPIES = 400;

/**
 * Paste `cols` x `rows` copies of a clip on a grid of `step` (the original is the cell at 0,0
 * and is not copied again). Each copy gets fresh ids and designators from the engine.
 */
export function arrayCommands(clip: Clip, sheet: Id, cols: number, rows: number, step: Point): Command[] {
  const out: Command[] = [];
  for (let r = 0; r < rows; r++)
    for (let c = 0; c < cols; c++) if (r !== 0 || c !== 0) out.push(...pasteCommands(clip, sheet, { x: c * step.x, y: r * step.y }));
  return out;
}

/**
 * Objects like `id` on the sheet (EasyEDA's "Find similar objects", in EAK terms): parts of the
 * same library component, net labels of the same name and kind, or every wire.
 */
export function similar(snap: Snapshot, sheet: Id, kind: Kind, id: Id): Selection {
  const { symbols, labels } = snap.project.schematic;
  if (kind === "symbol") {
    const c = symbols[id]?.component_id;
    return { ...EMPTY, symbols: Object.values(symbols).filter((s) => s.sheet_id === sheet && s.component_id === c).map((s) => s.id) };
  }
  if (kind === "label") {
    const l = labels[id];
    return { ...EMPTY, labels: l ? sheetLabels(snap, sheet).filter((x) => x.name === l.name && x.kind === l.kind).map((x) => x.id) : [] };
  }
  return { ...EMPTY, wires: sheetWires(snap, sheet).map((w) => w.id) };
}

/** Which object kinds a click, a box or Select all may pick (the selection filter). */
export type Filter = Record<Kind, boolean>;
export const ALL_KINDS: Filter = { symbol: true, wire: true, label: true };

/** Drop the kinds the filter excludes. */
export function filtered(s: Selection, f: Filter): Selection {
  if (f.symbol && f.wire && f.label) return s;
  return { symbols: f.symbol ? s.symbols : [], wires: f.wire ? s.wires : [], labels: f.label ? s.labels : [] };
}

// ---- Groups (ADR-0045) ------------------------------------------------------------------------
// A group is engine state (`Group` / `Ungroup` commands). The editor treats it as one object: a
// click or a box that touches a member selects every member, so move, copy and delete act on the
// whole group through the same commands as any multi-selection.

const groupsOf = (snap: Snapshot): SchematicGroup[] => Object.values(snap.project.schematic.groups ?? {});

/** The group `id` belongs to, if any. */
export const groupOf = (snap: Snapshot, id: Id): SchematicGroup | undefined => groupsOf(snap).find((g) => g.members.includes(id));

const kindOf = (snap: Snapshot, id: Id): Kind | null => {
  const { symbols, wires, labels } = snap.project.schematic;
  return id in symbols ? "symbol" : id in wires ? "wire" : id in labels ? "label" : null;
};
const ids = (s: Selection): Id[] => [...s.symbols, ...s.wires, ...s.labels];

/** The existing objects among `members`, as a selection. */
export function selectionOf(snap: Snapshot, members: readonly Id[]): Selection {
  const out: Selection = { symbols: [], wires: [], labels: [] };
  for (const m of members) {
    const k = kindOf(snap, m);
    if (k) out[listOf(k)].push(m);
  }
  return out;
}

/** `s` widened to every member of each group it touches. */
export function withGroups(snap: Snapshot, s: Selection): Selection {
  const chosen = new Set(ids(s));
  const touched = groupsOf(snap).filter((g) => g.members.some((m) => chosen.has(m)));
  return touched.reduce((acc, g) => union(acc, selectionOf(snap, g.members)), s);
}

/** A click on a grouped object acts on its whole group: plain and Shift select it, Ctrl/Cmd toggles it. */
export function pickGrouped(snap: Snapshot, s: Selection, k: Kind, id: Id, mode: PickMode): Selection {
  const g = groupOf(snap, id);
  if (!g) return pick(s, k, id, mode);
  const members = selectionOf(snap, g.members);
  if (mode === "replace") return members;
  if (mode === "toggle" && has(s, k, id)) {
    const drop = new Set(ids(members));
    return { symbols: s.symbols.filter((i) => !drop.has(i)), wires: s.wires.filter((i) => !drop.has(i)), labels: s.labels.filter((i) => !drop.has(i)) };
  }
  return union(s, members);
}

/** The groups the selection touches. */
export const groupsIn = (snap: Snapshot, s: Selection): SchematicGroup[] => {
  const chosen = new Set(ids(s));
  return groupsOf(snap).filter((g) => g.members.some((m) => chosen.has(m)));
};

/** `Group` for the selection, or why it cannot be grouped. */
export function groupCommand(snap: Snapshot, s: Selection): Command | string {
  const members = ids(s);
  if (members.length < 2) return "Select two or more objects first";
  const g = groupsIn(snap, s)[0];
  if (g) return `${g.name} is already a group: ungroup it first`;
  return { type: "Group", members, name: null };
}

/** `Ungroup` for every group the selection touches (one `Batch` when there are several). */
export const ungroupCommands = (snap: Snapshot, s: Selection): Command[] => groupsIn(snap, s).map((g): Command => ({ type: "Ungroup", group_id: g.id }));
