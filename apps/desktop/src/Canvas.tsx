// SVG canvas with wheel zoom (about the cursor) and drag pan. When `onPoint` is given a
// click (pointer released without dragging) reports the view-space point under it, and
// `onHover` tracks the cursor so editors can draw rubber-band previews.
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { clientToView, fit, pan, zoomAt, type Bounds, type ViewBox } from "./viewport";
import { loadViewport, saveViewport, useDocId, useRegisterCanvas } from "./shell/viewState";

interface Props {
  label: string;
  bounds: Bounds;
  children: ReactNode;
  onPoint?: (x: number, y: number, e: ReactPointerEvent<SVGSVGElement>) => void;
  onHover?: (x: number, y: number) => void;
  /** Pointer cursor while an editing tool is active. */
  tool?: boolean;
  /** Zoom to these view-space bounds whenever `n` changes (finding navigation). */
  zoomTo?: { bounds: Bounds; n: number } | null;
  /**
   * Object drag (move, marquee). `start` is asked on a primary-button press; returning true
   * captures the gesture instead of panning. `move` and `end` follow only once the pointer has
   * moved past the click slop; a press released in place is still a click (`onPoint`).
   */
  onDrag?: DragHandler;
  /** Drawn under the design (the grid), given the current view box. */
  underlay?: (vb: ViewBox) => ReactNode;
  /** Drawn over the design (rulers), given the current view box. */
  overlay?: (vb: ViewBox) => ReactNode;
}

export interface DragHandler {
  start: (x: number, y: number, e: ReactPointerEvent<SVGSVGElement>) => boolean;
  move: (x: number, y: number) => void;
  end: (x: number, y: number, e: ReactPointerEvent<SVGSVGElement>) => void;
  cancel: () => void;
}

const CLICK_SLOP_PX = 4;

export function Canvas({ label, bounds, children, onPoint, onHover, tool, zoomTo, onDrag, underlay, overlay }: Props) {
  const ref = useRef<SVGSVGElement>(null);
  const docId = useDocId();
  // A restored viewport (this tab's last zoom/pan, kept across tab closes and sessions) wins over fit.
  const restored = useRef<ViewBox | null>(docId ? loadViewport(docId) : null);
  const [vb, setVb] = useState<ViewBox>(() => restored.current ?? fit(bounds));
  const vbRef = useRef(vb);
  vbRef.current = vb;
  const drag = useRef<{ x: number; y: number } | null>(null);
  const down = useRef<{ x: number; y: number; moved: boolean; captured: boolean; middle: boolean } | null>(null);
  const key = `${bounds.minX},${bounds.minY},${bounds.maxX},${bounds.maxY}`;

  // Re-fit only when the drawing extents change, not on every re-render.
  useEffect(() => {
    if (restored.current) {
      restored.current = null; // first run after mount: keep the restored viewport
      return;
    }
    setVb(fit(bounds));
  }, [key]);
  useEffect(() => {
    if (docId) saveViewport(docId, vb);
  }, [docId, vb]);
  useRegisterCanvas(docId, {
    zoomBy: (f) => setVb((v) => zoomAt(v, f, v.x + v.w / 2, v.y + v.h / 2)),
    fit: () => setVb(fit(bounds)),
  });
  useEffect(() => {
    if (zoomTo) setVb(fit(zoomTo.bounds, 0.4));
  }, [zoomTo?.n]);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    // Native listener: React's wheel handler is passive and cannot stop page scroll.
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) return;
      setVb((v) => {
        const [cx, cy] = clientToView(v, r, e.clientX - r.left, e.clientY - r.top);
        return zoomAt(v, e.deltaY > 0 ? 1.15 : 1 / 1.15, cx, cy);
      });
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  const toView = (e: ReactPointerEvent<SVGSVGElement>): [number, number] | null => {
    const el = ref.current;
    if (!el) return null;
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return null;
    return clientToView(vbRef.current, r, e.clientX - r.left, e.clientY - r.top);
  };

  return (
    <svg
      ref={ref}
      className={tool ? "canvas tool" : "canvas"}
      role="img"
      aria-label={label}
      viewBox={`${vb.x} ${vb.y} ${vb.w} ${vb.h}`}
      preserveAspectRatio="xMidYMid meet"
      onPointerDown={(e) => {
        drag.current = { x: e.clientX, y: e.clientY };
        const p = e.button === 0 && onDrag ? toView(e) : null;
        const captured = !!p && !!onDrag?.start(p[0], p[1], e);
        down.current = { x: e.clientX, y: e.clientY, moved: false, captured, middle: e.button === 1 };
      }}
      onPointerMove={(e) => {
        const p = toView(e);
        if (p && onHover) onHover(p[0], p[1]);
        const d = drag.current;
        const el = ref.current;
        if (!d || !el || e.buttons === 0) return;
        const g = down.current;
        if (g && Math.hypot(e.clientX - g.x, e.clientY - g.y) > CLICK_SLOP_PX) g.moved = true;
        if (g?.captured) {
          if (g.moved && p) onDrag?.move(p[0], p[1]);
          return;
        }
        // Middle-button drag always pans; a left drag pans only when no tool or drag claimed it.
        if (tool && !g?.moved && !g?.middle) return;
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) return;
        setVb((v) => pan(v, r, e.clientX - d.x, e.clientY - d.y));
        drag.current = { x: e.clientX, y: e.clientY };
      }}
      onPointerUp={(e) => {
        const g = down.current;
        drag.current = null;
        down.current = null;
        if (g?.captured && g.moved) {
          const p = toView(e);
          if (p) onDrag?.end(p[0], p[1], e);
          else onDrag?.cancel();
          return;
        }
        if (g?.captured) onDrag?.cancel();
        if (!g || g.moved || !onPoint || e.button !== 0) return;
        const p = toView(e);
        if (p) onPoint(p[0], p[1], e);
      }}
      onPointerLeave={() => {
        if (down.current?.captured) onDrag?.cancel();
        drag.current = null;
        down.current = null;
      }}
      onMouseDown={(e) => {
        // Middle button: no browser auto-scroll, the drag pans the view instead.
        if (e.button === 1) e.preventDefault();
      }}
      onDoubleClick={() => {
        if (!tool) setVb(fit(bounds));
      }}
    >
      {underlay?.(vb)}
      {children}
      {overlay?.(vb)}
    </svg>
  );
}
