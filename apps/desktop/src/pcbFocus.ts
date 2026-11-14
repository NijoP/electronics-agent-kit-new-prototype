// Finding navigation geometry: the board-space box around the objects a finding names (plus
// its reported location), in canvas view coordinates, so the PCB editor can zoom to it.
import type { Id, Point, Snapshot } from "./types";
import { boundsOf, mm, toView, type Bounds } from "./viewport";

export interface Focus {
  /** Changes on every navigation so re-selecting the same finding re-zooms. */
  n: number;
  objects: Id[];
  location: Point | null;
}

const MIN_SPAN_MM = 8;

export function focusBounds(s: Snapshot, objects: Id[], location: Point | null): Bounds | null {
  const { pcb } = s.project;
  const pts: [number, number][] = [];
  for (const id of objects) {
    const t = pcb.tracks[id];
    if (t) pts.push(toView(t.a), toView(t.b));
    const v = pcb.vias[id];
    if (v) {
      const [x, y] = toView(v.position);
      const r = mm(v.diameter) / 2;
      pts.push([x - r, y - r], [x + r, y + r]);
    }
    const z = pcb.zones[id];
    if (z) pts.push(...z.outline.map(toView));
    for (const f of s.geometry.footprints) {
      if (f.id !== id && f.symbol_id !== id) continue;
      const [x, y] = toView(f.position);
      pts.push([x - mm(f.half_w), y - mm(f.half_h)], [x + mm(f.half_w), y + mm(f.half_h)]);
    }
  }
  if (location) pts.push(toView(location));
  const b = boundsOf(pts);
  if (!b) return null;
  // Never zoom closer than a few millimetres: a single via or point still needs context.
  const padX = Math.max(0, (MIN_SPAN_MM - (b.maxX - b.minX)) / 2);
  const padY = Math.max(0, (MIN_SPAN_MM - (b.maxY - b.minY)) / 2);
  return { minX: b.minX - padX, minY: b.minY - padY, maxX: b.maxX + padX, maxY: b.maxY + padY };
}
