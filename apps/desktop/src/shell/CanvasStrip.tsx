// Canvas strip (Phase 1 A3): the line under the active editor's canvas with the pointer position
// X / Y, its offset dX / dY from the last primary press, the zoom, and the editor's grid, snap and
// unit (the A2 grid store; the toolbar holds the controls, so this strip only reports them).
// Positions are design coordinates (Y up), snapped to the grid when snap is on, as a click would be.
import { formatLength, fromMm, unitLabel, type GridConfig, type Unit } from "../grid";
import { useCanvasReadout, type CanvasReadout } from "./viewState";

/** 100 % zoom is one millimetre per 96/25.4 CSS px (actual size on a 96 dpi screen). */
export const PX_PER_MM_AT_100 = 96 / 25.4;
const DECIMALS: Record<Unit, number> = { mm: 3, mil: 1, inch: 4 };

const snapTo = (v: number, cfg: GridConfig) => (cfg.snap && cfg.spacingMm > 0 ? Math.round(v / cfg.spacingMm) * cfg.spacingMm : v);

export interface StripText {
  x: string;
  y: string;
  dx: string;
  dy: string;
  zoom: string;
}

/** The strip's numbers from a canvas readout (view mm, Y down) and the editor's grid. */
export function stripText(r: CanvasReadout, cfg: GridConfig): StripText {
  // `+ 0` turns -0 into 0 so the readout never shows "-0.000".
  const f = (mm: number) => (fromMm(mm, cfg.unit) + 0).toFixed(DECIMALS[cfg.unit]);
  const dash = "—";
  // View Y grows downwards; design Y grows upwards.
  const at = r.at ? { x: snapTo(r.at.x, cfg), y: snapTo(-r.at.y, cfg) } : null;
  const anchor = r.anchor ? { x: snapTo(r.anchor.x, cfg), y: snapTo(-r.anchor.y, cfg) } : null;
  return {
    x: at ? f(at.x) : dash,
    y: at ? f(at.y) : dash,
    dx: at && anchor ? f(at.x - anchor.x) : dash,
    dy: at && anchor ? f(at.y - anchor.y) : dash,
    zoom: r.pxPerMm ? `${Math.round((r.pxPerMm / PX_PER_MM_AT_100) * 100)} %` : dash,
  };
}

export function CanvasStrip({ docId, cfg }: { docId: string | null; cfg: GridConfig }) {
  const t = stripText(useCanvasReadout(docId), cfg);
  return (
    <div className="canvas-strip" role="group" aria-label="Canvas strip">
      <span title={`Pointer position, design coordinates (Y up)${cfg.snap ? ", snapped to the grid" : ""}`}>
        X <span className="strip-val" data-strip="Pointer X">{t.x}</span> Y <span className="strip-val" data-strip="Pointer Y">{t.y}</span>
      </span>
      <span title="Offset from the last point pressed on the canvas">
        dX <span className="strip-val" data-strip="Offset dX">{t.dx}</span> dY <span className="strip-val" data-strip="Offset dY">{t.dy}</span>
      </span>
      <span className="strip-unit" title="Unit of the readout, grid and rulers (toolbar Unit combo)">{unitLabel(cfg.unit)}</span>
      <span title="Zoom: 100 % is actual size on a 96 dpi screen">
        Zoom <span className="strip-val" data-strip="Zoom level">{t.zoom}</span>
      </span>
      <span title="Grid spacing (toolbar Grid combo)">
        Grid {formatLength(cfg.spacingMm, cfg.unit)}
        {cfg.visible ? "" : " (hidden)"}
      </span>
      <span className={cfg.snap ? "" : "muted"} title="Snap to grid (toolbar Snap check box)">
        Snap {cfg.snap ? "on" : "off"}
      </span>
    </div>
  );
}
