// Schema v4 PCB object geometry for the desktop renderers (ADR-0042): arc tracks and arc
// outline edges flattened to chords, and the dimension drawing. Mirrors `eak-core::pcb_objects`
// (same centre formula and dimension layout); the engine's values stay the source of truth, this
// only draws them. Pure functions in IR nanometres (Y up).
import type { OutlineArc, PcbDimension, Point, Track } from "../types";

/** Chord error for drawing (the engine flattens at 10 µm for checks and output). */
export const DRAW_ARC_ERR_NM = 20_000;
const MAX_SEGMENTS = 256;
/** Default dimension text height when the IR leaves it at 0. */
const DIMENSION_TEXT_NM = 1_000_000;

export const validSweep = (s: number | undefined | null): s is number =>
  typeof s === "number" && Number.isFinite(s) && s !== 0 && Math.abs(s) < 360;

/** Centre of the arc from `a` to `b` sweeping `sweepDeg` (positive counter-clockwise). */
export function arcCenter(a: Point, b: Point, sweepDeg: number): [number, number] | null {
  if (!validSweep(sweepDeg) || (a.x === b.x && a.y === b.y)) return null;
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const c = Math.hypot(dx, dy);
  const h = c / 2 / Math.tan((sweepDeg * Math.PI) / 360);
  return [(a.x + b.x) / 2 + (-dy / c) * h, (a.y + b.y) / 2 + (dx / c) * h];
}

/** The arc as a polyline from `a` to `b` (both included), chord error at most `maxErr` nm. */
export function arcPoints(a: Point, b: Point, sweepDeg: number, maxErr = DRAW_ARC_ERR_NM): Point[] {
  const c = arcCenter(a, b, sweepDeg);
  if (!c) return [a, b];
  const [cx, cy] = c;
  const r = Math.hypot(a.x - cx, a.y - cy);
  const theta = (sweepDeg * Math.PI) / 180;
  const step = maxErr > 0 && maxErr < r ? 2 * Math.acos(1 - maxErr / r) : Math.abs(theta);
  const n = Math.min(MAX_SEGMENTS, Math.max(1, Math.ceil(Math.abs(theta) / step)));
  const a0 = Math.atan2(a.y - cy, a.x - cx);
  const out: Point[] = [a];
  for (let k = 1; k < n; k++) {
    const t = a0 + (theta * k) / n;
    out.push({ x: Math.round(cx + r * Math.cos(t)), y: Math.round(cy + r * Math.sin(t)) });
  }
  out.push(b);
  return out;
}

/** Centre line of a track: its two ends, or its flattened arc. */
export const trackPath = (t: Track): Point[] => (validSweep(t.arc_deg) ? arcPoints(t.a, t.b, t.arc_deg) : [t.a, t.b]);

/** Board outline with arc edges flattened (closed implicitly, like the IR outline). */
export function outlinePath(outline: Point[], arcs: OutlineArc[] | undefined): Point[] {
  if (!arcs?.length || outline.length < 2) return outline;
  const out: Point[] = [];
  outline.forEach((a, i) => {
    const arc = arcs.find((e) => e.edge === i);
    if (!arc) {
      out.push(a);
      return;
    }
    const pts = arcPoints(a, outline[(i + 1) % outline.length], arc.sweep_deg);
    out.push(...pts.slice(0, Math.max(1, pts.length - 1)));
  });
  return out;
}

const UNIT: Record<PcbDimension["unit"], [number, string]> = { mm: [1e6, "mm"], mil: [25_400, "mil"], inch: [25_400_000, "in"] };

/** Dimension label, e.g. "25.40 mm" (same rounding as the engine). */
export function dimensionLabel(d: PcbDimension): string {
  const [per, suffix] = UNIT[d.unit] ?? UNIT.mm;
  return `${(Math.hypot(d.b.x - d.a.x, d.b.y - d.a.y) / per).toFixed(Math.min(6, d.precision))} ${suffix}`;
}

export interface DimensionDrawing {
  lines: [Point, Point][];
  labelAt: Point;
  /** Label reads along X (0) or along Y (90). */
  labelRotation: 0 | 90;
  label: string;
  textSize: number;
}

/** Dimension line, extension lines and arrow heads, as `PcbDimension::drawing` lays them out. */
export function dimensionDrawing(d: PcbDimension): DimensionDrawing {
  const len = Math.max(1, Math.hypot(d.b.x - d.a.x, d.b.y - d.a.y));
  const ux = (d.b.x - d.a.x) / len;
  const uy = (d.b.y - d.a.y) / len;
  const nx = -uy;
  const ny = ux;
  const at = (p: Point, along: number, across: number): Point => ({
    x: Math.round(p.x + ux * along + nx * across),
    y: Math.round(p.y + uy * along + ny * across),
  });
  const off = d.offset;
  const a2 = at(d.a, 0, off);
  const b2 = at(d.b, 0, off);
  const lines: [Point, Point][] = [[a2, b2]];
  if (off !== 0) {
    const past = 500_000 * Math.sign(off);
    lines.push([d.a, at(d.a, 0, off + past)], [d.b, at(d.b, 0, off + past)]);
  }
  const arrow = Math.min(len / 4, 1_000_000);
  const wing = arrow * 0.35;
  const tips: [Point, number][] = [
    [a2, 1],
    [b2, -1],
  ];
  for (const [tip, dir] of tips) lines.push([tip, at(tip, dir * arrow, wing)], [tip, at(tip, dir * arrow, -wing)]);
  const mid = { x: Math.trunc((a2.x + b2.x) / 2), y: Math.trunc((a2.y + b2.y) / 2) };
  return {
    lines,
    labelAt: at(mid, 0, (off < 0 ? -1 : 1) * 400_000),
    labelRotation: Math.abs(d.b.x - d.a.x) >= Math.abs(d.b.y - d.a.y) ? 0 : 90,
    label: dimensionLabel(d),
    textSize: d.text_size > 0 ? d.text_size : DIMENSION_TEXT_NM,
  };
}
