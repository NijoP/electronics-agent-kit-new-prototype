// Geometry helpers for the scene renderer: boxes, precise hit tests and a uniform-grid spatial
// index used by both culling (draw only what is in a dirty rectangle) and picking.
import type { Box, Pt } from "./types";

export function boxOfPts(pts: Pt[], pad = 0): Box {
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const [x, y] of pts) {
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
  }
  return { minX: minX - pad, minY: minY - pad, maxX: maxX + pad, maxY: maxY + pad };
}

export const grow = (b: Box, d: number): Box => ({ minX: b.minX - d, minY: b.minY - d, maxX: b.maxX + d, maxY: b.maxY + d });

export const union = (a: Box | null, b: Box): Box =>
  a ? { minX: Math.min(a.minX, b.minX), minY: Math.min(a.minY, b.minY), maxX: Math.max(a.maxX, b.maxX), maxY: Math.max(a.maxY, b.maxY) } : { ...b };

export const intersects = (a: Box, b: Box): boolean => a.minX <= b.maxX && a.maxX >= b.minX && a.minY <= b.maxY && a.maxY >= b.minY;

export const inBox = (b: Box, x: number, y: number, tol = 0): boolean =>
  x >= b.minX - tol && x <= b.maxX + tol && y >= b.minY - tol && y <= b.maxY + tol;

/** Distance from (x, y) to segment a-b. */
export function segDist(x: number, y: number, a: Pt, b: Pt): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((x - a[0]) * dx + (y - a[1]) * dy) / len2));
  return Math.hypot(x - (a[0] + t * dx), y - (a[1] + t * dy));
}

/** Even-odd point-in-polygon test. */
export function inPoly(pts: Pt[], x: number, y: number): boolean {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const [xi, yi] = pts[i];
    const [xj, yj] = pts[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

/** Distance from (x, y) to the boundary of a closed polygon. */
export function polyEdgeDist(pts: Pt[], x: number, y: number): number {
  let d = Infinity;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) d = Math.min(d, segDist(x, y, pts[j], pts[i]));
  return d;
}

/** Four corners of an axis-aligned rectangle, clockwise from top-left. */
export const rectPoly = (b: Box): Pt[] => [
  [b.minX, b.minY],
  [b.maxX, b.minY],
  [b.maxX, b.maxY],
  [b.minX, b.maxY],
];

/**
 * Uniform grid over item boxes. `query` returns the indices of items whose box meets the query
 * box, each once, in ascending index order (so callers keep draw order).
 */
export class GridIndex {
  private readonly cells = new Map<number, number[]>();
  private readonly cell: number;
  private readonly boxes: Box[];
  /** Items spanning too many cells are kept apart and always tested. */
  private readonly big: number[] = [];
  private static readonly MAX_CELLS = 256;

  constructor(boxes: Box[], cell?: number) {
    this.boxes = boxes;
    this.cell = cell ?? GridIndex.cellFor(boxes);
    const c = this.cell;
    boxes.forEach((b, i) => {
      const x0 = Math.floor(b.minX / c);
      const x1 = Math.floor(b.maxX / c);
      const y0 = Math.floor(b.minY / c);
      const y1 = Math.floor(b.maxY / c);
      if ((x1 - x0 + 1) * (y1 - y0 + 1) > GridIndex.MAX_CELLS) {
        this.big.push(i);
        return;
      }
      for (let gx = x0; gx <= x1; gx++)
        for (let gy = y0; gy <= y1; gy++) {
          const k = GridIndex.key(gx, gy);
          const list = this.cells.get(k);
          if (list) list.push(i);
          else this.cells.set(k, [i]);
        }
    });
  }

  /** Cell edge: about four times the median item size, at least 0.5 mm. */
  private static cellFor(boxes: Box[]): number {
    if (!boxes.length) return 10;
    const sizes = boxes.map((b) => Math.max(b.maxX - b.minX, b.maxY - b.minY)).sort((a, b) => a - b);
    return Math.max(sizes[Math.floor(sizes.length / 2)] * 4, 0.5);
  }

  // Cell coordinates are offset into [0, 2^20): boards up to about ±500 m at 1 mm cells.
  private static key(gx: number, gy: number): number {
    return (gx + 0x80000) * 0x100000 + (gy + 0x80000);
  }

  query(q: Box): number[] {
    const c = this.cell;
    const x0 = Math.floor(q.minX / c);
    const x1 = Math.floor(q.maxX / c);
    const y0 = Math.floor(q.minY / c);
    const y1 = Math.floor(q.maxY / c);
    const out: number[] = [];
    if ((x1 - x0 + 1) * (y1 - y0 + 1) > this.cells.size) {
      // The query covers more cells than exist: a straight scan is cheaper.
      this.boxes.forEach((b, i) => {
        if (intersects(b, q)) out.push(i);
      });
      return out;
    }
    const seen = new Set<number>();
    const take = (i: number) => {
      if (!seen.has(i) && intersects(this.boxes[i], q)) {
        seen.add(i);
        out.push(i);
      }
    };
    for (let gx = x0; gx <= x1; gx++)
      for (let gy = y0; gy <= y1; gy++) {
        const list = this.cells.get(GridIndex.key(gx, gy));
        if (list) for (const i of list) take(i);
      }
    for (const i of this.big) take(i);
    return out.sort((a, b) => a - b);
  }
}
