// Editor grids: zoom-responsive minor/major lines, an origin cross, measurement rulers, the grid
// settings (spacing, unit, visibility, snap) and the cursor readout. The schematic and PCB editors
// each have their own `GridKind` (different presets, major division and persistence key), so the
// two grids share drawing code but are configured independently. The settings of a kind live in
// one small store, so the main toolbar's grid combos and that editor's snapping always read the
// same value. All maths is in view millimetres (Y down); the unit chooses the presets offered and
// how lengths are shown.
import { useSyncExternalStore } from "react";
import { Icon } from "./design/icons/icons";
import "./grid.css";
import type { ViewBox } from "./viewport";

export type GridKind = "schematic" | "pcb";
export type Unit = "mm" | "mil" | "inch";
export const UNITS: readonly Unit[] = ["mm", "mil", "inch"];
export interface GridConfig {
  /** Display unit: the grid presets offered, and the unit of the rulers and cursor readout. */
  unit: Unit;
  /** Minor grid spacing in millimetres (always one of the unit's presets); also the snap step. */
  spacingMm: number;
  visible: boolean;
  snap: boolean;
}

/** Millimetres per unit. */
export const MM_PER: Record<Unit, number> = { mm: 1, mil: 0.0254, inch: 25.4 };
const round6 = (v: number) => Math.round(v * 1e6) / 1e6 + 0;
/** A length in `unit` as millimetres (rounded to the nanometre). */
export const toMm = (v: number, unit: Unit) => round6(v * MM_PER[unit]);
/** Millimetres as a length in `unit`. */
export const fromMm = (mm: number, unit: Unit) => round6(mm / MM_PER[unit]);
export const unitLabel = (u: Unit) => (u === "inch" ? "in" : u);

interface KindSpec {
  /** Grid presets per unit, in that unit, finest first. */
  presets: Record<Unit, number[]>;
  /** Minor divisions per major line. */
  majorEvery: number;
  defaults: GridConfig;
}
export const GRID_KINDS: Record<GridKind, KindSpec> = {
  // Schematic: imperial pin pitch (100 mil = 2.54 mm); pins of built-in symbols sit on 1.27 mm.
  schematic: {
    presets: { mm: [0.5, 1, 1.27, 2.54], mil: [10, 25, 50, 100], inch: [0.025, 0.05, 0.1] },
    majorEvery: 4,
    defaults: { unit: "mm", spacingMm: 1.27, visible: true, snap: true },
  },
  // PCB: metric placement grids, and the common imperial ones in mil or inch.
  pcb: {
    presets: { mm: [0.05, 0.1, 0.25, 0.5, 1, 2.54], mil: [1, 5, 10, 25, 50, 100], inch: [0.001, 0.005, 0.01, 0.025, 0.05, 0.1] },
    majorEvery: 10,
    defaults: { unit: "mm", spacingMm: 0.5, visible: true, snap: true },
  },
};

/** The unit's grid presets of an editor kind, in millimetres. */
export const presetsMm = (kind: GridKind, unit: Unit): number[] => GRID_KINDS[kind].presets[unit].map((v) => toMm(v, unit));
const presetOf = (kind: GridKind, unit: Unit, mm: number) => presetsMm(kind, unit).find((p) => Math.abs(p - mm) < 1e-9);

/** Text for a length in a unit: "1.27 mm", "50 mil", "0.05 in". */
export const formatLength = (mm: number, unit: Unit) => `${fromMm(mm, unit)} ${unitLabel(unit)}`;

/**
 * Switching unit keeps the grid on that unit's presets: the new spacing is the preset nearest to
 * the old one (the finer on a tie), so 1.27 mm becomes exactly 50 mil and 0.5 mm becomes 25 mil.
 */
export function withUnit(kind: GridKind, cfg: GridConfig, unit: Unit): GridConfig {
  const d = (p: number) => Math.abs(p - cfg.spacingMm);
  const near = presetsMm(kind, unit).reduce((best, p) => (d(p) < d(best) - 1e-12 ? p : best));
  return { ...cfg, unit, spacingMm: near };
}

/** The next finer (-1) or coarser (+1) preset of the current unit, or null when there is none. */
export function stepPreset(kind: GridKind, cfg: GridConfig, dir: -1 | 1): number | null {
  const all = presetsMm(kind, cfg.unit);
  const i = all.findIndex((p) => Math.abs(p - cfg.spacingMm) < 1e-9);
  const j = i + dir;
  return i < 0 || j < 0 || j >= all.length ? null : all[j];
}

/** Snap step in nanometres: the grid spacing, or 1 nm (free placement) when snap is off. */
export const snapStepNm = (cfg: GridConfig): number => (cfg.snap ? Math.round(cfg.spacingMm * 1_000_000) : 1);

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
 * Measurement rulers along the top and left edge of the view, labelled in the grid's unit in
 * design coordinates (Y up, so the left ruler shows -viewY). Drawn over the design.
 */
export function Rulers({ vb, kind, cfg }: { vb: ViewBox; kind: GridKind; cfg: GridConfig }) {
  const { minor, major } = gridSteps(vb.w, cfg.spacingMm, GRID_KINDS[kind].majorEvery);
  const band = Math.min(vb.w, vb.h) / 28;
  const font = band * 0.55;
  const isMajor = (v: number) => Math.abs(v / major - Math.round(v / major)) < 1e-6;
  const xs = ticks(vb.x + band, vb.x + vb.w, minor);
  const ys = ticks(vb.y + band, vb.y + vb.h, minor);
  const label = (mm: number) => fmt(fromMm(mm, cfg.unit));
  return (
    <g className="rulers" aria-label={`Rulers (${unitLabel(cfg.unit)})`} style={{ fontSize: font }}>
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
          {label(x)}
        </text>
      ))}
      {ys.filter(isMajor).map((y) => (
        <text key={`y${y}`} className="ruler-label" x={vb.x + font * 0.15} y={y - font * 0.2}>
          {label(-y)}
        </text>
      ))}
      <rect className="ruler-band corner" x={vb.x} y={vb.y} width={band} height={band} />
    </g>
  );
}

// --- Settings store: one per editor kind, persisted (a view preference, not design data). ---

const storeKey = (kind: GridKind) => `eak.grid.${kind}`;
const memory: Partial<Record<GridKind, string>> = {};
let storageOk = typeof localStorage !== "undefined";
function readRaw(kind: GridKind): string | null {
  if (storageOk)
    try {
      return localStorage.getItem(storeKey(kind));
    } catch {
      storageOk = false;
    }
  return memory[kind] ?? null;
}
function writeRaw(kind: GridKind, raw: string) {
  memory[kind] = raw;
  if (storageOk)
    try {
      localStorage.setItem(storeKey(kind), raw);
    } catch {
      // Storage unavailable (private mode): the setting still applies for this session.
      storageOk = false;
    }
}

/** Saved settings, repaired: an unknown unit or a spacing off the unit's presets falls back. */
export function parseGrid(kind: GridKind, raw: string | null): GridConfig {
  const d = GRID_KINDS[kind].defaults;
  if (!raw) return d;
  try {
    const v = JSON.parse(raw) as Partial<GridConfig>;
    const sp = typeof v.spacingMm === "number" ? v.spacingMm : d.spacingMm;
    // Settings saved before units existed have none: take the first unit that offers the spacing.
    const unit = v.unit && UNITS.includes(v.unit) ? v.unit : (UNITS.find((u) => presetOf(kind, u, sp) !== undefined) ?? d.unit);
    const spacingMm = presetOf(kind, unit, sp) ?? withUnit(kind, d, unit).spacingMm;
    return { unit, spacingMm, visible: typeof v.visible === "boolean" ? v.visible : d.visible, snap: typeof v.snap === "boolean" ? v.snap : d.snap };
  } catch {
    return d;
  }
}

const cache = new Map<GridKind, { raw: string | null; cfg: GridConfig }>();
const listeners = new Set<() => void>();

/** Current grid settings of an editor kind. */
export function getGrid(kind: GridKind): GridConfig {
  const raw = readRaw(kind);
  const hit = cache.get(kind);
  if (hit && hit.raw === raw) return hit.cfg;
  const cfg = parseGrid(kind, raw);
  cache.set(kind, { raw, cfg });
  return cfg;
}

/** Changes the grid of an editor kind. A unit change alone moves the spacing to that unit's nearest preset. */
export function updateGrid(kind: GridKind, patch: Partial<GridConfig>) {
  const cur = getGrid(kind);
  let next: GridConfig = { ...cur, ...patch };
  if (patch.unit !== undefined && patch.spacingMm === undefined) next = { ...next, spacingMm: withUnit(kind, cur, patch.unit).spacingMm };
  else next = { ...next, spacingMm: presetOf(kind, next.unit, next.spacingMm) ?? cur.spacingMm };
  writeRaw(kind, JSON.stringify(next));
  cache.set(kind, { raw: readRaw(kind), cfg: next });
  for (const l of listeners) l();
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};

/** Grid settings of one editor kind, shared by every component that shows or uses them. */
export function useGridConfig(kind: GridKind): [GridConfig, (patch: Partial<GridConfig>) => void] {
  const snap = () => getGrid(kind);
  const cfg = useSyncExternalStore(subscribe, snap, snap);
  return [cfg, (patch) => updateGrid(kind, patch)];
}

/** Grid size, unit, visibility and snap combos for one editor kind (shown in the main toolbar). */
export function GridControls({ kind }: { kind: GridKind }) {
  const [cfg, set] = useGridConfig(kind);
  return (
    <span className="grid-controls" role="group" aria-label={`${kind === "pcb" ? "PCB" : "Schematic"} grid`}>
      <span className="combo" title="Grid size: the minor grid spacing and the snap step">
        <Icon name="grid" />
        <select aria-label="Grid spacing" value={cfg.spacingMm} onChange={(e) => set({ spacingMm: Number(e.target.value) })}>
          {presetsMm(kind, cfg.unit).map((mm) => (
            <option key={mm} value={mm}>
              {formatLength(mm, cfg.unit)}
            </option>
          ))}
        </select>
      </span>
      <span className="combo" title="Unit of the grid presets, rulers and cursor position">
        <Icon name="unit" />
        <select aria-label="Unit" value={cfg.unit} onChange={(e) => set({ unit: e.target.value as Unit })}>
          {UNITS.map((u) => (
            <option key={u} value={u}>
              {u}
            </option>
          ))}
        </select>
      </span>
      <label title="Show the grid">
        <input type="checkbox" aria-label="Show grid" checked={cfg.visible} onChange={(e) => set({ visible: e.target.checked })} /> Grid
      </label>
      <label title="Snap points and moves to the grid">
        <input type="checkbox" aria-label="Snap to grid" checked={cfg.snap} onChange={(e) => set({ snap: e.target.checked })} /> <Icon name="snap" /> Snap
      </label>
    </span>
  );
}

const DECIMALS: Record<Unit, number> = { mm: 3, mil: 1, inch: 4 };

/** Cursor position readout in design coordinates (Y up), in the grid's unit. */
export function Coords({ at, unit = "mm" }: { at: { x: number; y: number } | null; unit?: Unit }) {
  const v = (nm: number) => fromMm(nm / 1e6, unit).toFixed(DECIMALS[unit]);
  return (
    <span className="coords muted" aria-label="Cursor position">
      {at ? `X ${v(at.x)}  Y ${v(at.y)} ${unitLabel(unit)}` : `X —  Y — ${unitLabel(unit)}`}
    </span>
  );
}
