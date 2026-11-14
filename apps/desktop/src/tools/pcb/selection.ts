// Pure PCB footprint-selection reducers: marquee, and the typed engine commands that move,
// rotate, flip, align, distribute and lock a selection. The caller submits a list as one
// `Batch` (ADR-0013), so a multi-footprint edit is one revision and one undo step. Locked
// footprints are never moved (the engine refuses `MoveFootprint` on them); the reducers skip
// them and `movable` reports how many were skipped.
import type { Command, FootprintGeom, Id, Point, Snapshot } from "../../types";
import { nextRotation } from "../../editing";

export type PickMode = "replace" | "add" | "toggle";
export const pickMode = (e: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean }): PickMode =>
  e.ctrlKey || e.metaKey ? "toggle" : e.shiftKey ? "add" : "replace";

/** Click selects exactly one footprint; Shift adds it; Ctrl/Cmd toggles it. */
export function pick(sel: Id[], id: Id, mode: PickMode): Id[] {
  if (mode === "replace") return [id];
  if (sel.includes(id)) return mode === "add" ? sel : sel.filter((x) => x !== id);
  return [...sel, id];
}

/** Drop footprint ids the engine no longer has. */
export const prune = (sel: Id[], snap: Snapshot): Id[] => {
  const next = sel.filter((id) => id in snap.project.pcb.footprints);
  return next.length === sel.length ? sel : next;
};

export interface Rect {
  min: Point;
  max: Point;
}
export const rectOf = (a: Point, b: Point): Rect => ({
  min: { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y) },
  max: { x: Math.max(a.x, b.x), y: Math.max(a.y, b.y) },
});

/** Courtyard box of a footprint (geometry extents are already turned by its rotation). */
export const boxOf = (f: FootprintGeom): Rect => ({
  min: { x: f.position.x - f.half_w, y: f.position.y - f.half_h },
  max: { x: f.position.x + f.half_w, y: f.position.y + f.half_h },
});

/** Footprints whose courtyard lies entirely inside the marquee. */
export function marquee(snap: Snapshot, r: Rect): Id[] {
  return snap.geometry.footprints
    .filter((f) => {
      const b = boxOf(f);
      return b.min.x >= r.min.x && b.max.x <= r.max.x && b.min.y >= r.min.y && b.max.y <= r.max.y;
    })
    .map((f) => f.id);
}

const isLocked = (snap: Snapshot, id: Id) => !!snap.project.pcb.footprints[id]?.locked;
/** Selected footprints that may move, and how many locked ones were left out. */
export function movable(snap: Snapshot, sel: Id[]): { ids: Id[]; locked: number } {
  const ids = sel.filter((id) => id in snap.project.pcb.footprints && !isLocked(snap, id));
  return { ids, locked: sel.filter((id) => isLocked(snap, id)).length };
}

const move = (snap: Snapshot, id: Id, position: Point): Command | null => {
  const f = snap.project.pcb.footprints[id];
  if (!f || (f.position.x === position.x && f.position.y === position.y)) return null;
  return { type: "MoveFootprint", footprint_id: id, position, rotation: null, side: null };
};
const some = (c: (Command | null)[]): Command[] => c.filter((x): x is Command => x !== null);

/** Move every unlocked selected footprint by `delta`. */
export function moveCommands(snap: Snapshot, sel: Id[], delta: Point): Command[] {
  if (delta.x === 0 && delta.y === 0) return [];
  return some(
    movable(snap, sel).ids.map((id) => {
      const p = snap.project.pcb.footprints[id].position;
      return move(snap, id, { x: p.x + delta.x, y: p.y + delta.y });
    }),
  );
}

/** Rotate each unlocked selected footprint a quarter turn in place. */
export function rotateCommands(snap: Snapshot, sel: Id[]): Command[] {
  return movable(snap, sel).ids.map((id) => {
    const f = snap.project.pcb.footprints[id];
    return { type: "MoveFootprint", footprint_id: id, position: f.position, rotation: nextRotation(f.rotation), side: null };
  });
}

/** Move each unlocked selected footprint to the other board side, in place. */
export function flipCommands(snap: Snapshot, sel: Id[]): Command[] {
  return movable(snap, sel).ids.map((id) => {
    const f = snap.project.pcb.footprints[id];
    return { type: "MoveFootprint", footprint_id: id, position: f.position, rotation: null, side: f.side === "top" ? "bottom" : "top" };
  });
}

export type Align = "left" | "right" | "top" | "bottom" | "hcenter" | "vcenter";

/**
 * Align the selection's courtyards to the selection's extreme edge, or put their centres on
 * one line ("hcenter": common horizontal centre line, "vcenter": common vertical one).
 * Locked footprints are not moved but still define the reference, so "align to a locked
 * connector" works. Design Y points up, so "top" is the largest Y.
 */
export function alignCommands(snap: Snapshot, sel: Id[], how: Align): Command[] {
  const geo = snap.geometry.footprints.filter((f) => sel.includes(f.id));
  if (geo.length < 2) return [];
  const boxes = geo.map(boxOf);
  const minX = Math.min(...boxes.map((b) => b.min.x));
  const maxX = Math.max(...boxes.map((b) => b.max.x));
  const minY = Math.min(...boxes.map((b) => b.min.y));
  const maxY = Math.max(...boxes.map((b) => b.max.y));
  const ok = new Set(movable(snap, sel).ids);
  return some(
    geo.map((f) => {
      if (!ok.has(f.id)) return null;
      const to = { ...snap.project.pcb.footprints[f.id].position };
      if (how === "left") to.x = minX + f.half_w;
      if (how === "right") to.x = maxX - f.half_w;
      if (how === "bottom") to.y = minY + f.half_h;
      if (how === "top") to.y = maxY - f.half_h;
      if (how === "hcenter") to.y = Math.round((minY + maxY) / 2);
      if (how === "vcenter") to.x = Math.round((minX + maxX) / 2);
      return move(snap, f.id, to);
    }),
  );
}

/**
 * Distribute centres evenly along an axis between the two outermost footprints (which stay
 * put). Needs three or more footprints; locked ones keep their place in the order but are not
 * moved.
 */
export function distributeCommands(snap: Snapshot, sel: Id[], axis: "x" | "y"): Command[] {
  const geo = snap.geometry.footprints.filter((f) => sel.includes(f.id)).sort((a, b) => a.position[axis] - b.position[axis]);
  if (geo.length < 3) return [];
  const first = geo[0].position[axis];
  const gap = (geo[geo.length - 1].position[axis] - first) / (geo.length - 1);
  const ok = new Set(movable(snap, sel).ids);
  return some(
    geo.slice(1, -1).map((f, i) => {
      if (!ok.has(f.id)) return null;
      const p = snap.project.pcb.footprints[f.id].position;
      return move(snap, f.id, { ...p, [axis]: Math.round(first + gap * (i + 1)) });
    }),
  );
}

/** Lock or unlock every selected footprint whose state differs. */
export function lockCommands(snap: Snapshot, sel: Id[], locked: boolean): Command[] {
  return sel
    .filter((id) => id in snap.project.pcb.footprints && isLocked(snap, id) !== locked)
    .map((id): Command => ({ type: "SetFootprintLocked", footprint_id: id, locked }));
}
