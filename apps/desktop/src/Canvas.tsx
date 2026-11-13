// Read-only SVG canvas with wheel zoom (about the cursor) and drag pan.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { clientToView, fit, pan, zoomAt, type Bounds, type ViewBox } from "./viewport";

interface Props {
  label: string;
  bounds: Bounds;
  children: ReactNode;
}

export function Canvas({ label, bounds, children }: Props) {
  const ref = useRef<SVGSVGElement>(null);
  const [vb, setVb] = useState<ViewBox>(() => fit(bounds));
  const drag = useRef<{ x: number; y: number } | null>(null);
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

  return (
    <svg
      ref={ref}
      className="canvas"
      role="img"
      aria-label={label}
      viewBox={`${vb.x} ${vb.y} ${vb.w} ${vb.h}`}
      preserveAspectRatio="xMidYMid meet"
      onPointerDown={(e) => {
        drag.current = { x: e.clientX, y: e.clientY };
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        const el = ref.current;
        if (!d || !el || e.buttons === 0) return;
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) return;
        setVb((v) => pan(v, r, e.clientX - d.x, e.clientY - d.y));
        drag.current = { x: e.clientX, y: e.clientY };
      }}
      onPointerUp={() => (drag.current = null)}
      onPointerLeave={() => (drag.current = null)}
      onDoubleClick={() => setVb(fit(bounds))}
    >
      {children}
    </svg>
  );
}
