// SVG canvas with wheel zoom (about the cursor) and drag pan. When `onPoint` is given a
// click (pointer released without dragging) reports the view-space point under it, and
// `onHover` tracks the cursor so editors can draw rubber-band previews.
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { clientToView, fit, pan, zoomAt, type Bounds, type ViewBox } from "./viewport";

interface Props {
  label: string;
  bounds: Bounds;
  children: ReactNode;
  onPoint?: (x: number, y: number, e: ReactPointerEvent<SVGSVGElement>) => void;
  onHover?: (x: number, y: number) => void;
  /** Pointer cursor while an editing tool is active. */
  tool?: boolean;
}

const CLICK_SLOP_PX = 4;

export function Canvas({ label, bounds, children, onPoint, onHover, tool }: Props) {
  const ref = useRef<SVGSVGElement>(null);
  const [vb, setVb] = useState<ViewBox>(() => fit(bounds));
  const vbRef = useRef(vb);
  vbRef.current = vb;
  const drag = useRef<{ x: number; y: number } | null>(null);
  const down = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const key = `${bounds.minX},${bounds.minY},${bounds.maxX},${bounds.maxY}`;

  // Re-fit only when the drawing extents change, not on every re-render.
  useEffect(() => setVb(fit(bounds)), [key]);

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
        down.current = { x: e.clientX, y: e.clientY, moved: false };
      }}
      onPointerMove={(e) => {
        const p = toView(e);
        if (p && onHover) onHover(p[0], p[1]);
        const d = drag.current;
        const el = ref.current;
        if (!d || !el || e.buttons === 0) return;
        const g = down.current;
        if (g && Math.hypot(e.clientX - g.x, e.clientY - g.y) > CLICK_SLOP_PX) g.moved = true;
        if (tool && !g?.moved) return;
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) return;
        setVb((v) => pan(v, r, e.clientX - d.x, e.clientY - d.y));
        drag.current = { x: e.clientX, y: e.clientY };
      }}
      onPointerUp={(e) => {
        const g = down.current;
        drag.current = null;
        down.current = null;
        if (!g || g.moved || !onPoint || e.button !== 0) return;
        const p = toView(e);
        if (p) onPoint(p[0], p[1], e);
      }}
      onPointerLeave={() => {
        drag.current = null;
        down.current = null;
      }}
      onDoubleClick={() => {
        if (!tool) setVb(fit(bounds));
      }}
    >
      {children}
    </svg>
  );
}
