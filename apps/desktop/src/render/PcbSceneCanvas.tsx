// React host for a SceneRenderer: one <canvas> sized to the pane, drawn for the canvas view box.
// Pan, zoom and pointer handling stay in `Canvas.tsx` (the interactive SVG laid over this), so the
// editing tools keep their pointer contract; this only paints the scene under it.
import { useEffect, useLayoutEffect, useRef } from "react";
import type { Size, ViewBox } from "../viewport";
import { Canvas2DRenderer } from "./canvas2d";
import type { OverlayScene, RenderStats, Scene, SceneRenderer, ViewTransform } from "./types";

interface Props {
  scene: Scene;
  overlay: OverlayScene;
  /** The visible view box (already grown to the pane's aspect ratio). */
  vb: ViewBox;
  size: Size;
  /** Called after each frame with what the renderer did. */
  onFrame?: (s: RenderStats) => void;
  /** Called when the 2D context cannot be created, so the view can fall back to SVG. */
  onUnavailable?: () => void;
}

/** View transform for a visible view box drawn into a pane of `size` CSS pixels. */
export function viewTransform(vb: ViewBox, size: Size, dpr: number): ViewTransform {
  return { x0: vb.x, y0: vb.y, scale: size.width / vb.w, width: size.width, height: size.height, dpr };
}

export function PcbSceneCanvas({ scene, overlay, vb, size, onFrame, onUnavailable }: Props) {
  const ref = useRef<HTMLCanvasElement>(null);
  const renderer = useRef<SceneRenderer | null>(null);
  const frame = useRef<number | null>(null);
  const latest = useRef({ vb, size, onFrame, scene, overlay });
  latest.current = { vb, size, onFrame, scene, overlay };
  const dpr = typeof window !== "undefined" && window.devicePixelRatio > 0 ? window.devicePixelRatio : 1;

  const draw = () => {
    frame.current = null;
    const r = renderer.current;
    const { vb: v, size: s, onFrame: cb } = latest.current;
    if (!r || s.width <= 0 || s.height <= 0 || v.w <= 0) return;
    cb?.(r.render(viewTransform(v, s, dpr)));
  };
  const schedule = () => {
    if (frame.current === null && typeof requestAnimationFrame !== "undefined") frame.current = requestAnimationFrame(draw);
  };

  useEffect(() => {
    const ctx = ref.current?.getContext("2d") ?? null;
    if (!ctx) {
      onUnavailable?.();
      return;
    }
    const r = new Canvas2DRenderer(ctx);
    r.setScene(latest.current.scene);
    r.setOverlay(latest.current.overlay);
    renderer.current = r;
    schedule();
    // Scene colours come from design tokens: redraw everything when the theme changes.
    const mo =
      typeof MutationObserver !== "undefined"
        ? new MutationObserver(() => {
            renderer.current?.invalidate();
            schedule();
          })
        : null;
    mo?.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "class", "style"] });
    return () => {
      mo?.disconnect();
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      frame.current = null;
      r.dispose();
      renderer.current = null;
    };
  }, []);

  useLayoutEffect(() => {
    renderer.current?.setScene(scene);
    schedule();
  }, [scene]);
  useLayoutEffect(() => {
    renderer.current?.setOverlay(overlay);
    schedule();
  }, [overlay]);
  useLayoutEffect(schedule, [vb.x, vb.y, vb.w, vb.h, size.width, size.height]);

  return (
    <canvas
      ref={ref}
      className="scene-canvas"
      aria-hidden="true"
      data-renderer="canvas2d"
      width={Math.max(1, Math.round(size.width * dpr))}
      height={Math.max(1, Math.round(size.height * dpr))}
      style={{ width: `${size.width}px`, height: `${size.height}px` }}
    />
  );
}
