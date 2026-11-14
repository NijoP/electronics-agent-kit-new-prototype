// Pure board-outline geometry for the PCB Board outline tool: shapes from exact numeric
// entry, vertex edits, units and closure validation. Everything produces a `Point[]` for the
// existing `SetBoardOutline` command; the engine stays the authority (it rejects outlines that
// are not simple polygons). EAK-IR stores an outline as a closed polygon of straight edges, so
// circles, rounded corners and arcs are emitted as chord polylines (no IR/command change).
import type { Nm, Point } from "../../types";

export type Unit = "mm" | "mil";
const NM_PER_UNIT: Record<Unit, number> = { mm: 1_000_000, mil: 25_400 };

export const unitToNm = (v: number, u: Unit): Nm => Math.round(v * NM_PER_UNIT[u]);
export const nmToUnit = (nm: Nm, u: Unit): number => nm / NM_PER_UNIT[u];
/** Display a length in the chosen unit (3 decimals in mm, 1 in mil). */
export const fmtLen = (nm: Nm, u: Unit): string => nmToUnit(nm, u).toFixed(u === "mm" ? 3 : 1);
/** Parse a typed number (accepts a decimal comma); null when blank or not finite. */
export function parseNum(s: string): number | null {
  const t = s.trim().replace(",", ".");
  if (t === "") return null;
  const n = Number(t);
  return Number.isFinite(n) ? n : null;
}

const rnd = (n: number) => Math.round(n) + 0;

/** Axis-aligned rectangle with its lower-left corner at `origin`. Null when a side is not > 0. */
export function rectangle(origin: Point, width: Nm, height: Nm): Point[] | null {
  if (!(width > 0) || !(height > 0)) return null;
  const { x, y } = origin;
  return [
    { x, y },
    { x: x + width, y },
    { x: x + width, y: y + height },
    { x, y: y + height },
  ];
}

/** Points on an arc, counter-clockwise from `startDeg` to `endDeg`, `steps` chords. */
export function arcPoints(center: Point, radius: Nm, startDeg: number, endDeg: number, steps: number): Point[] {
  const out: Point[] = [];
  for (let i = 0; i <= steps; i++) {
    const a = ((startDeg + ((endDeg - startDeg) * i) / steps) * Math.PI) / 180;
    out.push({ x: rnd(center.x + radius * Math.cos(a)), y: rnd(center.y + radius * Math.sin(a)) });
  }
  return out;
}

/** Circle as a regular `segments`-gon inscribed in the radius (default 64 chords). */
export function circle(center: Point, radius: Nm, segments = 64): Point[] | null {
  if (!(radius > 0) || segments < 8) return null;
  return arcPoints(center, radius, 0, 360, segments).slice(0, -1);
}

/** Rectangle with quarter-circle corners of `corner` radius (0 = square corners). */
export function roundedRectangle(origin: Point, width: Nm, height: Nm, corner: Nm, stepsPerCorner = 8): Point[] | null {
  if (!(width > 0) || !(height > 0)) return null;
  if (!(corner > 0)) return rectangle(origin, width, height);
  if (corner * 2 > Math.min(width, height)) return null;
  const { x, y } = origin;
  const c = (cx: number, cy: number, a0: number) => arcPoints({ x: cx, y: cy }, corner, a0, a0 + 90, stepsPerCorner);
  return [
    ...c(x + width - corner, y + corner, -90),
    ...c(x + width - corner, y + height - corner, 0),
    ...c(x + corner, y + height - corner, 90),
    ...c(x + corner, y + corner, 180),
  ];
}

export function moveVertex(pts: Point[], i: number, to: Point): Point[] {
  return pts.map((p, k) => (k === i ? to : p));
}
export function deleteVertex(pts: Point[], i: number): Point[] {
  return pts.filter((_, k) => k !== i);
}
/** Insert a vertex on the edge after vertex `i` (at its midpoint unless `at` is given). */
export function insertVertex(pts: Point[], i: number, at?: Point): Point[] {
  const a = pts[i];
  const b = pts[(i + 1) % pts.length];
  const p = at ?? { x: rnd((a.x + b.x) / 2), y: rnd((a.y + b.y) / 2) };
  return [...pts.slice(0, i + 1), p, ...pts.slice(i + 1)];
}

export interface Extent {
  minX: Nm;
  minY: Nm;
  maxX: Nm;
  maxY: Nm;
  width: Nm;
  height: Nm;
}
export function extent(pts: Point[]): Extent | null {
  if (!pts.length) return null;
  const xs = pts.map((p) => p.x);
  const ys = pts.map((p) => p.y);
  const [minX, maxX, minY, maxY] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
  return { minX, minY, maxX, maxY, width: maxX - minX, height: maxY - minY };
}

/** Signed shoelace area in nm² (floating point, coordinates centred on the first vertex). */
export function signedArea(pts: Point[]): number {
  if (pts.length < 3) return 0;
  const o = pts[0];
  let s = 0;
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i];
    const b = pts[(i + 1) % pts.length];
    s += (a.x - o.x) * (b.y - o.y) - (b.x - o.x) * (a.y - o.y);
  }
  return s / 2;
}
export const perimeter = (pts: Point[]): number =>
  pts.reduce((s, a, i) => {
    const b = pts[(i + 1) % pts.length];
    return s + Math.hypot(b.x - a.x, b.y - a.y);
  }, 0);

const orient = (a: Point, b: Point, c: Point) => Math.sign((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x));
const onSeg = (a: Point, b: Point, p: Point) =>
  Math.min(a.x, b.x) <= p.x && p.x <= Math.max(a.x, b.x) && Math.min(a.y, b.y) <= p.y && p.y <= Math.max(a.y, b.y);
function segmentsCross(a: Point, b: Point, c: Point, d: Point): boolean {
  const [o1, o2, o3, o4] = [orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b)];
  if (o1 !== o2 && o3 !== o4) return true;
  return (o1 === 0 && onSeg(a, b, c)) || (o2 === 0 && onSeg(a, b, d)) || (o3 === 0 && onSeg(c, d, a)) || (o4 === 0 && onSeg(c, d, b));
}

/**
 * Closure/validity check. The outline is an implicitly closed polygon (the last vertex joins the
 * first), so "closed" here means: at least 3 distinct vertices, non-zero area and no crossing or
 * touching edges (other than neighbours meeting at their shared vertex). Empty list = valid.
 */
export function validateOutline(pts: Point[]): string[] {
  const issues: string[] = [];
  const n = pts.length;
  if (n < 3) return [`needs at least 3 vertices (has ${n})`];
  pts.forEach((p, i) => {
    const q = pts[(i + 1) % n];
    if (p.x === q.x && p.y === q.y) issues.push(`vertices ${i + 1} and ${((i + 1) % n) + 1} coincide`);
  });
  if (signedArea(pts) === 0) issues.push("outline has zero area");
  for (let i = 0; i < n; i++) {
    for (let j = i + 1; j < n; j++) {
      if (j === i + 1 || (i === 0 && j === n - 1)) continue; // neighbours share a vertex
      if (segmentsCross(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n])) {
        issues.push(`edges ${i + 1} and ${j + 1} cross`);
      }
    }
  }
  return issues;
}

/** Index of the vertex within `radius` of `p`, if any. */
export function nearestVertex(pts: Point[], p: Point, radius: Nm): number | null {
  let best: number | null = null;
  let bestD = radius * radius;
  pts.forEach((v, i) => {
    const d = (v.x - p.x) ** 2 + (v.y - p.y) ** 2;
    if (d <= bestD) {
      bestD = d;
      best = i;
    }
  });
  return best;
}
