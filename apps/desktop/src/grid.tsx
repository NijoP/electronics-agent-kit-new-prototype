// Editor grids: zoom-responsive minor/major lines, an origin cross, measurement rulers and the
// settings bar (spacing, visibility, snap). The schematic and PCB editors each pass their own
// `GridKind` (different spacings, major division and persistence key), so the two grids share
// drawing code but are configured independently. All maths is in view millimetres (Y down).
import { useState } from "react";
import "./grid.css";
import type { ViewBox } from "./viewport";

export type GridKind = "schematic" | "pcb";
export interface GridConfig {
  /** Minor grid spacing in millimetres; also the snap step. */
  spacingMm: number;
  visible: boolean;
  snap: boolean;
}

interface KindSpec {
  options: { mm: number; label: string }[];
  /** Minor divisions per major line. */
  majorEvery: number;
  defaults: GridConfig;
}
export const GRID_KINDS: Record<GridKind, KindSpec> = {
  // Schematic: imperial pin pitch (100 mil = 2.54 mm); pins of built-in symbols sit on 1.27 mm.
  schematic: {
    options: [
      { mm: 0.635, label: "0.635 mm (25 mil)" },
      { mm: 1.27, label: "1.27 mm (50 mil)" },
      { mm: 2.54, label: "2.54 mm (100 mil)" },
    ],
    majorEvery: 4,
    defaults: { spacingMm: 1.27, visible: true, snap: true },
  },
  // PCB: metric placement grids plus the common imperial ones.
  pcb: {
    options: [
      { mm: 0.05, label: "0.05 mm" },
      { mm: 0.1, label: "0.1 mm" },
      { mm: 0.25, label: "0.25 mm" },
      { mm: 0.5, label: "0.5 mm" },
      { mm: 1, label: "1 mm" },
      { mm: 0.635, label: "0.635 mm (25 mil)" },
      { mm: 1.27, label: "1.27 mm (50 mil)" },
      { mm: 2.54, label: "2.54 mm (100 mil)" },
    ],
    majorEvery: 10,
    defaults: { spacingMm: 0.5, visible: true, snap: true },
  },
};

/** Most lines drawn across the view for one grid level before it is coarsened. */
export const MAX_LINES = 90;

/**
 * Line steps to draw at this zoom. The minor step starts at the configured spacing and is
 * multiplied by `majorEvery` until at most `MAX_LINES` minor lines cross the view, so zooming
 * out fades the fine grid into the coarse one instead of filling the screen.
 */
export function gridSteps(viewW: number, spacingMm: number, majorEvery: number): { minor: number; major: number } {
  let minor = spacingMm > 0 ? spacingMm : 1;
  for (let i = 0; i < 12 && viewW / minor > MAX_LINES; i++) minor *= majorEvery;
  return { minor, major: minor * majorEvery };
}

/** Multiples of `step` inside [lo, hi]. */
export function ticks(lo: number, hi: number, step: number): number[] {
  const out: number[] = [];
  const first = Math.ceil(lo / step - 1e-9);
  for (let i = first; i * step <= hi + 1e-9 && out.length < 1000; i++) out.push(Math.round(i * step * 1e6) / 1e6 + 0);
  return out;
}

const lines = (vb: ViewBox, xs: number[], ys: number[]) =>
  xs.map((x) => `M${x} ${vb.y}V${vb.y + vb.h}`).join("") + ys.map((y) => `M${vb.x} ${y}H${vb.x + vb.w}`).join("");

/** Minor and major grid lines plus the origin axes; drawn under the design. */
export function GridLayer({ vb, kind, cfg }: { vb: ViewBox; kind: GridKind; cfg: GridConfig }) {
  if (!cfg.visible) return null;
  const { minor, major } = gridSteps(vb.w, cfg.spacingMm, GRID_KINDS[kind].majorEvery);
  const isMajor = (v: number) => Math.abs(v / major - Math.round(v / major)) < 1e-6;
  const xs = ticks(vb.x, vb.x + vb.w, minor);
  const ys = ticks(vb.y, vb.y + vb.h, minor);
  return (
    <g className={`grid grid-${kind}`} data-grid-minor={minor} data-grid-major={major} aria-hidden="true">
      <path className="grid-minor" d={lines(vb, xs.filter((x) => !isMajor(x)), ys.filter((y) => !isMajor(y)))} />
      <path className="grid-major" d={lines(vb, xs.filter(isMajor), ys.filter(isMajor))} />
      <path className="grid-origin" d={lines(vb, [0], [0])} />
    </g>
  );
}

const fmt = (v: number) => (Math.abs(v) < 1e-9 ? "0" : String(Number(v.toFixed(3))));

/**
 * Measurement rulers along the top and left edge of the view, labelled in millimetres in
 * design coordinates (Y up, so the left ruler shows -viewY). Drawn over the design.
 */
export function Rulers({ vb, kind, cfg }: { vb: ViewBox; kind: GridKind; cfg: GridConfig }) {
  const { minor, major } = gridSteps(vb.w, cfg.spacingMm, GRID_KINDS[kind].majorEvery);
  const band = Math.min(vb.w, vb.h) / 28;
  const font = band * 0.55;
  const isMajor = (v: number) => Math.abs(v / major - Math.round(v / major)) < 1e-6;
  const xs = ticks(vb.x + band, vb.x + vb.w, minor);
  const ys = ticks(vb.y + band, vb.y + vb.h, minor);
  return (
    <g className="rulers" aria-label="Rulers (mm)" style={{ fontSize: font }}>
      <rect className="ruler-band" x={vb.x} y={vb.y} width={vb.w} height={band} />
      <rect className="ruler-band" x={vb.x} y={vb.y} width={band} height={vb.h} />
      <path
        className="ruler-tick"
        d={
          xs.map((x) => `M${x} ${vb.y + band}v${-(isMajor(x) ? band * 0.6 : band * 0.25)}`).join("") +
          ys.map((y) => `M${vb.x + band} ${y}h${-(isMajor(y) ? band * 0.6 : band * 0.25)}`).join("")
        }
      />
      {xs.filter(isMajor).map((x) => (
        <text key={`x${x}`} className="ruler-label" x={x + font * 0.2} y={vb.y + font * 0.95}>
          {fmt(x)}
        </text>
      ))}
      {ys.filter(isMajor).map((y) => (
        <text key={`y${y}`} className="ruler-label" x={vb.x + font * 0.15} y={y - font * 0.2}>
          {fmt(-y)}
        </text>
      ))}
      <rect className="ruler-band corner" x={vb.x} y={vb.y} width={band} height={band} />
    </g>
  );
}

const storeKey = (kind: GridKind) => `eak.grid.${kind}`;

function load(kind: GridKind): GridConfig {
  const d = GRID_KINDS[kind].defaults;
  try {
    const raw = typeof localStorage === "undefined" ? null : localStorage.getItem(storeKey(kind));
    if (!raw) return d;
    const v = JSON.parse(raw) as Partial<GridConfig>;
    const spacing = GRID_KINDS[kind].options.some((o) => o.mm === v.spacingMm) ? (v.spacingMm as number) : d.spacingMm;
    return { spacingMm: spacing, visible: v.visible ?? d.visible, snap: v.snap ?? d.snap };
  } catch {
    return d;
  }
}

/** Grid settings of one editor kind, remembered across sessions (a view preference, not design data). */
export function useGridConfig(kind: GridKind): [GridConfig, (patch: Partial<GridConfig>) => void] {
  const [cfg, setCfg] = useState<GridConfig>(() => load(kind));
  const update = (patch: Partial<GridConfig>) =>
    setCfg((c) => {
      const next = { ...c, ...patch };
      try {
        localStorage.setItem(storeKey(kind), JSON.stringify(next));
      } catch {
        // Storage unavailable (private mode): the setting still applies for this session.
      }
      return next;
    });
  return [cfg, update];
}

/** Spacing / visibility / snap controls for one editor's grid. */
export function GridControls({ kind, cfg, onChange }: { kind: GridKind; cfg: GridConfig; onChange: (p: Partial<GridConfig>) => void }) {
  return (
    <span className="grid-controls" role="group" aria-label="Grid">
      <select aria-label="Grid spacing" value={cfg.spacingMm} onChange={(e) => onChange({ spacingMm: Number(e.target.value) })}>
        {GRID_KINDS[kind].options.map((o) => (
          <option key={o.mm} value={o.mm}>
            {o.label}
          </option>
        ))}
      </select>
      <label>
        <input type="checkbox" aria-label="Show grid" checked={cfg.visible} onChange={(e) => onChange({ visible: e.target.checked })} /> Grid
      </label>
      <label>
        <input type="checkbox" aria-label="Snap to grid" checked={cfg.snap} onChange={(e) => onChange({ snap: e.target.checked })} /> Snap
      </label>
    </span>
  );
}

/** Cursor position readout in design millimetres (Y up). */
export function Coords({ at }: { at: { x: number; y: number } | null }) {
  return (
    <span className="coords muted" aria-label="Cursor position">
      {at ? `X ${(at.x / 1e6).toFixed(3)}  Y ${(at.y / 1e6).toFixed(3)} mm` : "X —  Y — mm"}
    </span>
  );
}
