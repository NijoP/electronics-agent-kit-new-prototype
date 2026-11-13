// Pan/zoom maths for the read-only canvases. View coordinates are millimetres with Y
// pointing down (EAK-IR is Y-up nanometres; see `toView`).
import type { Point } from "./types";

export interface ViewBox {
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface Bounds {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}
export interface Size {
  width: number;
  height: number;
}

const NM_PER_MM = 1_000_000;
export const mm = (nm: number) => nm / NM_PER_MM;
/** EAK-IR point (nm, Y up) to view coordinates (mm, Y down). */
export const toView = (p: Point): [number, number] => [mm(p.x), -mm(p.y)];

export function boundsOf(points: [number, number][]): Bounds | null {
  if (points.length === 0) return null;
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  return {
    minX: Math.min(...xs),
    minY: Math.min(...ys),
    maxX: Math.max(...xs),
    maxY: Math.max(...ys),
  };
}

export function fit(b: Bounds, margin = 0.05): ViewBox {
  const w = Math.max(b.maxX - b.minX, 1);
  const h = Math.max(b.maxY - b.minY, 1);
  return { x: b.minX - w * margin, y: b.minY - h * margin, w: w * (1 + 2 * margin), h: h * (1 + 2 * margin) };
}

/** Map a client pixel offset to view coordinates under `preserveAspectRatio="xMidYMid meet"`. */
export function clientToView(vb: ViewBox, size: Size, px: number, py: number): [number, number] {
  const s = Math.min(size.width / vb.w, size.height / vb.h);
  const offX = (size.width - vb.w * s) / 2;
  const offY = (size.height - vb.h * s) / 2;
  return [vb.x + (px - offX) / s, vb.y + (py - offY) / s];
}

/** Zoom by `factor` (<1 zooms in) keeping view point (cx, cy) fixed on screen. */
export function zoomAt(vb: ViewBox, factor: number, cx: number, cy: number): ViewBox {
  const f = Math.min(Math.max(factor, 0.05), 20);
  return { x: cx - (cx - vb.x) * f, y: cy - (cy - vb.y) * f, w: vb.w * f, h: vb.h * f };
}

/** Pan by a pointer movement of (dx, dy) client pixels. */
export function pan(vb: ViewBox, size: Size, dx: number, dy: number): ViewBox {
  const s = Math.min(size.width / vb.w, size.height / vb.h);
  return { ...vb, x: vb.x - dx / s, y: vb.y - dy / s };
}

/** View millimetres (Y down) to an EAK-IR point in whole nanometres (Y up). */
export const fromView = (x: number, y: number): Point => ({
  x: Math.round(x * NM_PER_MM),
  y: Math.round(-y * NM_PER_MM) + 0,
});
