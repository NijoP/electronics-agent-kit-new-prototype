// Pure helpers for the interactive editors: grid snapping, pin/pad capture and track
// segmentation. They only compute command arguments; every change still goes through
// the engine (`api.execute`), and the UI keeps no model of its own.
import type { Nm, Point, Rotation } from "./types";

export const mmToNm = (v: number): Nm => Math.round(v * 1_000_000);

export const samePoint = (a: Point, b: Point) => a.x === b.x && a.y === b.y;

const snapAxis = (v: Nm, grid: Nm) => Math.round(v / grid) * grid + 0;

export interface Snap {
  point: Point;
  /** Index into the `targets` array when the point was captured by one, else null. */
  hit: number | null;
}

/** Capture the nearest target within `radius`; otherwise snap to the grid. */
export function snapPoint(p: Point, grid: Nm, targets: Point[], radius: Nm): Snap {
  let best: number | null = null;
  let bestD = radius * radius;
  targets.forEach((t, i) => {
    const d = (t.x - p.x) ** 2 + (t.y - p.y) ** 2;
    if (d <= bestD) {
      bestD = d;
      best = i;
    }
  });
  if (best !== null) return { point: targets[best], hit: best };
  return { point: { x: snapAxis(p.x, grid), y: snapAxis(p.y, grid) }, hit: null };
}

/**
 * Split a connection into segments the engine accepts: one segment when it is axis-aligned
 * (or 45° when allowed), otherwise a horizontal then a vertical leg.
 */
export function segments(a: Point, b: Point, allow45: boolean): [Point, Point][] {
  if (samePoint(a, b)) return [];
  const dx = Math.abs(b.x - a.x);
  const dy = Math.abs(b.y - a.y);
  if (dx === 0 || dy === 0 || (allow45 && dx === dy)) return [[a, b]];
  const corner = { x: b.x, y: a.y };
  return [
    [a, corner],
    [corner, b],
  ];
}

const ORDER: Rotation[] = ["R0", "R90", "R180", "R270"];
export const nextRotation = (r: Rotation): Rotation => ORDER[(ORDER.indexOf(r) + 1) % 4];

/** Board outline from the points clicked: two points are opposite corners of a rectangle. */
export function outlineFrom(points: Point[]): Point[] | null {
  if (points.length === 2) {
    const [a, b] = points;
    if (a.x === b.x || a.y === b.y) return null;
    return [a, { x: b.x, y: a.y }, b, { x: a.x, y: b.y }];
  }
  return points.length >= 3 ? points : null;
}
