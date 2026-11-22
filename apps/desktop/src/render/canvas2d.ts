// Canvas2D scene backend.
//
// The non-annotation layers are composited back to front (each with its opacity) into ONE
// offscreen surface, the composite, drawn for a view. A frame then costs about one full-canvas
// copy whatever the layer count:
//   - pan at the same zoom: the composite is shifted by whole device pixels and only the exposed
//     strips are redrawn (dirty rectangles). A pan that is not a whole number of device pixels is
//     snapped to one: the frame is drawn up to half a device pixel from the exact view;
//   - a scene change: objects whose signature changed are diffed per layer and only the union of
//     their old and new boxes is redrawn (dirty rectangles);
//   - zoom: while the zoom is moving the composite is shown scaled (blurred when zooming in, with
//     empty margins when zooming out) and redrawn once the zoom has been still for `SETTLE_MS`
//     (`RenderStats.stale` tells the host to ask for that frame), or at once when the scaled
//     image would be too far off (`STALE_ZOOM_RANGE`);
//   - resize, a layer-stack change, too many changes or `invalidate()`: a complete redraw.
// A dirty rectangle is redrawn layer by layer in draw order: opaque layers straight into the
// composite (source-over is associative, so this equals compositing the layer), translucent ones
// through a scratch surface so the opacity applies to the layer as a whole. Without surfaces, or
// over the memory budget, every layer is drawn straight to the screen each frame.
// Level of detail: an annotation layer (pad numbers, net names; drawn live at constant pixel
// size) is skipped at zooms where none of its labels can fit, and objects under half a CSS pixel
// both ways are not drawn. The overlay (selection, highlights, drag previews) is drawn last,
// uncached.
import { GridIndex, intersects } from "./geometry";
import { PickIndex } from "./pick";
import type { Box, DrawOp, OverlayScene, PickHit, PickOptions, RenderLayer, RenderObject, RenderStats, Scene, SceneRenderer, ViewTransform } from "./types";

type Ctx = CanvasRenderingContext2D;
export interface Surface {
  canvas: HTMLCanvasElement;
  ctx: Ctx;
}

export interface Canvas2DOptions {
  /** Offscreen surface factory (tests pass a stub). Null: draw every layer directly. */
  createSurface?: ((w: number, h: number) => Surface | null) | null;
  /** Colour resolver for design-token names (`--c-pad`); defaults to the document's computed style. */
  resolveColour?: (token: string) => string;
  /** Total bytes the composite and scratch surfaces may use (default 96 MiB). */
  cacheBudgetBytes?: number;
  /** Clock in ms (tests pass a fake one); defaults to `performance.now`. */
  now?: () => number;
}

/** More changed objects than this in one scene change redraws the whole composite instead. */
export const MAX_DIRTY_RECTS = 64;
/** A zoom shown from the scaled composite is redrawn once the scale has been still this long. */
export const SETTLE_MS = 120;
/** Scale ratios (new / drawn) outside this range redraw at once instead of showing the scaled image. */
export const STALE_ZOOM_RANGE: [number, number] = [0.75, 1.6];
/** Objects smaller than this many CSS pixels both ways are not drawn. */
const LOD_PX = 0.5;
/** Device pixels added around a dirty rectangle for antialiasing. */
const AA_PAD = 2;
const FONT = "system-ui, sans-serif";

interface LayerState {
  layer: RenderLayer;
  objects: RenderObject[];
  index: GridIndex;
  /** Annotation layers: the smallest scale at which any of its labels fits (level of detail). */
  minLabelScale: number;
}

export function defaultSurface(w: number, h: number): Surface | null {
  if (typeof document === "undefined") return null;
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  return ctx ? { canvas, ctx } : null;
}

export function documentColour(token: string): string {
  if (typeof document === "undefined" || typeof getComputedStyle === "undefined") return "#888888";
  return getComputedStyle(document.documentElement).getPropertyValue(token).trim() || "#888888";
}

const layerSig = (ls: LayerState[]) => ls.map((l) => `${l.layer.key}|${l.layer.alpha}|${l.layer.annotation ? 1 : 0}`).join(";");

function minLabelScale(objects: RenderObject[]): number {
  let m = Infinity;
  for (const o of objects) for (const op of o.ops) if (op.t === "label" && op.fitH > 0) m = Math.min(m, op.px / (op.fitH * 1.05));
  return m;
}

export class Canvas2DRenderer implements SceneRenderer {
  private readonly target: Ctx;
  private readonly createSurface: ((w: number, h: number) => Surface | null) | null;
  private readonly resolve: (token: string) => string;
  private readonly budget: number;
  private readonly clock: () => number;
  private readonly colours = new Map<string, string>();
  private layers: LayerState[] = [];
  private picker: PickIndex | null = null;
  private scene: Scene | null = null;
  private owners = new Map<string, RenderObject[]>();
  private overlay: OverlayScene = { items: [] };
  private comp: Surface | null = null;
  /** View the composite currently shows, or null when it must be redrawn completely. */
  private compView: ViewTransform | null = null;
  /** World boxes changed since the composite was drawn. */
  private dirty: Box[] = [];
  private scratch: Surface | null = null;
  private lastScale = NaN;
  private scaleChangedAt = -Infinity;

  constructor(target: Ctx, opts: Canvas2DOptions = {}) {
    this.target = target;
    this.createSurface = opts.createSurface === undefined ? defaultSurface : opts.createSurface;
    this.resolve = opts.resolveColour ?? documentColour;
    this.budget = opts.cacheBudgetBytes ?? 96 * 1024 * 1024;
    this.clock = opts.now ?? now;
  }

  setScene(scene: Scene): void {
    const prev = this.layers;
    const prevByKey = new Map(prev.map((l) => [l.layer.key, l]));
    const byLayer = new Map<string, RenderObject[]>();
    for (const o of scene.objects) {
      const list = byLayer.get(o.layerKey);
      if (list) list.push(o);
      else byLayer.set(o.layerKey, [o]);
    }
    this.layers = [...scene.layers]
      .sort((a, b) => a.order - b.order)
      .map((layer) => {
        const objects = byLayer.get(layer.key) ?? [];
        return { layer, objects, index: new GridIndex(objects.map((o) => o.bbox)), minLabelScale: layer.annotation ? minLabelScale(objects) : 0 };
      });
    if (this.compView) {
      if (layerSig(prev) !== layerSig(this.layers)) this.dropComposite();
      else
        for (const l of this.layers) {
          if (l.layer.annotation) continue;
          const changed = diffBoxes(prevByKey.get(l.layer.key)?.objects ?? [], l.objects);
          if (changed === null || this.dirty.length + changed.length > MAX_DIRTY_RECTS) {
            this.dropComposite();
            break;
          }
          this.dirty.push(...changed);
        }
    }
    const annotation = new Set(scene.layers.filter((l) => l.annotation).map((l) => l.key));
    this.scene = scene;
    this.picker = null; // built on the first pick
    this.owners = new Map();
    for (const o of scene.objects)
      if (o.owner && !annotation.has(o.layerKey)) {
        const list = this.owners.get(o.owner);
        if (list) list.push(o);
        else this.owners.set(o.owner, [o]);
      }
    this.colours.clear();
  }

  setOverlay(overlay: OverlayScene): void {
    this.overlay = overlay;
  }

  pick(x: number, y: number, opts?: PickOptions): PickHit | null {
    if (!this.picker && this.scene) this.picker = new PickIndex(this.scene);
    return this.picker?.pick(x, y, opts) ?? null;
  }

  invalidate(): void {
    this.colours.clear();
    this.dropComposite();
  }

  dispose(): void {
    this.layers = [];
    this.comp = null;
    this.scratch = null;
    this.dropComposite();
    this.picker = null;
    this.scene = null;
  }

  private dropComposite() {
    this.compView = null;
    this.dirty = [];
  }

  render(view: ViewTransform): RenderStats {
    const t0 = now();
    const stats: RenderStats = { fullLayers: 0, dirtyRects: 0, cachedLayers: 0, directLayers: 0, lodLayers: 0, drawn: 0, stale: false, ms: 0 };
    const W = Math.max(1, Math.round(view.width * view.dpr));
    const H = Math.max(1, Math.round(view.height * view.dpr));
    const t = this.clock();
    if (view.scale !== this.lastScale) {
      if (!Number.isNaN(this.lastScale)) this.scaleChangedAt = t;
      this.lastScale = view.scale;
    }
    const ctx = this.target;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.clearRect(0, 0, W, H);
    const comp = this.compositeSurface(W, H);
    let shown = view;
    if (comp) shown = this.drawComposite(comp, view, W, H, t, stats);
    else
      for (const l of this.layers) {
        if (!l.objects.length || l.layer.annotation) continue;
        stats.directLayers++;
        ctx.save();
        ctx.globalAlpha = l.layer.alpha;
        stats.drawn += this.drawRegion(ctx, l, view, worldBox(view, 0, 0, W, H), "shapes");
        ctx.restore();
      }
    const visible = worldBox(shown, 0, 0, W, H);
    for (const l of this.layers) {
      if (!l.objects.length || !l.layer.annotation) continue;
      if (shown.scale < l.minLabelScale) {
        stats.lodLayers++;
        continue;
      }
      stats.directLayers++;
      ctx.save();
      ctx.globalAlpha = l.layer.alpha;
      stats.drawn += this.drawRegion(ctx, l, shown, visible, "labels");
      ctx.restore();
    }
    ctx.globalAlpha = 1;
    this.drawOverlay(ctx, shown);
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    stats.ms = now() - t0;
    return stats;
  }

  /** The composite at W x H device pixels, or null to draw straight to the screen. */
  private compositeSurface(W: number, H: number): Surface | null {
    if (!this.createSurface || this.budget < 2 * W * H * 4) {
      this.comp = null; // a scratch surface may be needed too: both must fit
      this.dropComposite();
      return null;
    }
    const s = this.comp;
    if (s && s.canvas.width === W && s.canvas.height === H) return s;
    this.dropComposite();
    if (s) {
      s.canvas.width = W;
      s.canvas.height = H;
      return s;
    }
    this.comp = this.createSurface(W, H);
    return this.comp;
  }

  /**
   * Bring the composite up to `view` (or show it scaled while a zoom moves) and copy it to the
   * screen. Returns the view the screen now shows: `view`, or `view` snapped to whole device pixels.
   */
  private drawComposite(comp: Surface, view: ViewTransform, W: number, H: number, t: number, stats: RenderStats): ViewTransform {
    const ctx = this.target;
    const c = this.compView;
    const cached = this.layers.filter((l) => l.objects.length && !l.layer.annotation).length;
    const k = view.scale * view.dpr;
    let shown = view;
    let reuse = false;
    if (c && c.dpr === view.dpr && c.width === view.width && c.height === view.height) {
      if (c.scale === view.scale) {
        const rx = Math.round((c.x0 - view.x0) * k);
        const ry = Math.round((c.y0 - view.y0) * k);
        if (Math.abs(rx) < W && Math.abs(ry) < H) {
          reuse = true;
          shown = rx === 0 && ry === 0 ? c : { ...view, x0: c.x0 - rx / k, y0: c.y0 - ry / k };
          if (rx || ry) {
            const s = comp.ctx;
            s.setTransform(1, 0, 0, 1, 0, 0);
            s.globalAlpha = 1;
            s.globalCompositeOperation = "copy";
            s.drawImage(comp.canvas, rx, ry);
            s.globalCompositeOperation = "source-over";
            this.compView = shown;
            // Exposed strips, in device pixels.
            if (rx > 0) stats.drawn += this.redrawRect(comp, shown, 0, 0, rx, H, stats);
            if (rx < 0) stats.drawn += this.redrawRect(comp, shown, W + rx, 0, -rx, H, stats);
            if (ry > 0) stats.drawn += this.redrawRect(comp, shown, 0, 0, W, ry, stats);
            if (ry < 0) stats.drawn += this.redrawRect(comp, shown, 0, H + ry, W, -ry, stats);
          }
          stats.drawn += this.redrawDirty(comp, shown, W, H, stats);
        }
      } else {
        const ratio = view.scale / c.scale;
        const settled = t - this.scaleChangedAt >= SETTLE_MS;
        const near = ratio >= STALE_ZOOM_RANGE[0] && ratio <= STALE_ZOOM_RANGE[1];
        if (!settled && near && !this.dirty.length) {
          // The zoom is moving: show the composite scaled; the redraw waits for it to settle.
          const kc = c.scale * c.dpr;
          ctx.setTransform(k / kc, 0, 0, k / kc, (c.x0 - view.x0) * k, (c.y0 - view.y0) * k);
          ctx.globalAlpha = 1;
          ctx.drawImage(comp.canvas, 0, 0);
          ctx.setTransform(1, 0, 0, 1, 0, 0);
          stats.stale = true;
          stats.cachedLayers = cached;
          return view;
        }
      }
    }
    if (!reuse) {
      this.dirty = [];
      stats.drawn += this.redrawRect(comp, view, 0, 0, W, H, null);
      stats.fullLayers = cached;
      this.compView = view;
    } else if (!stats.dirtyRects) stats.cachedLayers = cached;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.drawImage(comp.canvas, 0, 0);
    return shown;
  }

  private scratchSurface(W: number, H: number): Surface | null {
    const s = this.scratch;
    if (s && s.canvas.width === W && s.canvas.height === H) return s;
    if (s) {
      s.canvas.width = W;
      s.canvas.height = H;
      return s;
    }
    this.scratch = this.createSurface ? this.createSurface(W, H) : null;
    return this.scratch;
  }

  private redrawDirty(comp: Surface, view: ViewTransform, W: number, H: number, stats: RenderStats): number {
    let n = 0;
    const k = view.scale * view.dpr;
    for (const b of this.dirty) {
      const x = Math.floor((b.minX - view.x0) * k) - AA_PAD;
      const y = Math.floor((b.minY - view.y0) * k) - AA_PAD;
      const x2 = Math.ceil((b.maxX - view.x0) * k) + AA_PAD;
      const y2 = Math.ceil((b.maxY - view.y0) * k) + AA_PAD;
      const cx = Math.max(0, x);
      const cy = Math.max(0, y);
      const cw = Math.min(W, x2) - cx;
      const ch = Math.min(H, y2) - cy;
      if (cw <= 0 || ch <= 0) continue;
      n += this.redrawRect(comp, view, cx, cy, cw, ch, stats);
    }
    this.dirty = [];
    return n;
  }

  /**
   * Clear and redraw one device-pixel rectangle of the composite: every cached layer in draw
   * order, with its opacity. Counts a dirty rectangle in `stats` (null: a complete redraw).
   */
  private redrawRect(comp: Surface, view: ViewTransform, x: number, y: number, w: number, h: number, stats: RenderStats | null): number {
    const c = comp.ctx;
    const q = worldBox(view, x, y, w, h);
    c.save();
    c.setTransform(1, 0, 0, 1, 0, 0);
    c.globalAlpha = 1;
    c.beginPath();
    c.rect(x, y, w, h);
    c.clip();
    c.clearRect(x, y, w, h);
    let n = 0;
    for (const l of this.layers) {
      if (!l.objects.length || l.layer.annotation) continue;
      const objs = l.index.query(q).map((i) => l.objects[i]);
      if (!objs.length) continue;
      n += objs.length;
      const sc = l.layer.alpha < 1 ? this.scratchSurface(comp.canvas.width, comp.canvas.height) : null;
      if (!sc) {
        c.globalAlpha = l.layer.alpha;
        this.drawShapes(c, objs, view, null);
        continue;
      }
      const s = sc.ctx;
      s.save();
      s.setTransform(1, 0, 0, 1, 0, 0);
      s.globalAlpha = 1;
      s.beginPath();
      s.rect(x, y, w, h);
      s.clip();
      s.clearRect(x, y, w, h);
      this.drawShapes(s, objs, view, null);
      s.restore();
      c.setTransform(1, 0, 0, 1, 0, 0);
      c.globalAlpha = l.layer.alpha;
      c.drawImage(sc.canvas, x, y, w, h, x, y, w, h);
    }
    c.restore();
    if (stats) stats.dirtyRects++;
    return n;
  }

  /** Draw the layer's objects that meet world box `q`. */
  private drawRegion(ctx: Ctx, l: LayerState, view: ViewTransform, q: Box, what: "shapes" | "labels"): number {
    const objs = l.index.query(q).map((i) => l.objects[i]);
    if (what === "labels") this.drawLabels(ctx, objs, view);
    else this.drawShapes(ctx, objs, view, null);
    return objs.length;
  }

  private colour(c: string): string {
    if (!c.startsWith("--")) return c;
    let v = this.colours.get(c);
    if (v === undefined) {
      v = this.resolve(c);
      this.colours.set(c, v);
    }
    return v;
  }

  /** World transform for `view`, optionally offset by a world delta. */
  private world(ctx: Ctx, view: ViewTransform, delta: [number, number] | null) {
    const k = view.scale * view.dpr;
    const ox = delta ? delta[0] : 0;
    const oy = delta ? delta[1] : 0;
    ctx.setTransform(k, 0, 0, k, (ox - view.x0) * k, (oy - view.y0) * k);
  }

  /**
   * Draw shape ops in world space. Consecutive segments of one colour, width and opacity share a
   * single path and stroke call, which is what keeps 20,000 tracks cheap.
   */
  private drawShapes(ctx: Ctx, objs: RenderObject[], view: ViewTransform, delta: [number, number] | null) {
    this.world(ctx, view, delta);
    const minW = 1 / view.scale; // never thinner than one CSS pixel
    const lod = LOD_PX / view.scale; // smaller than this both ways: not drawn
    const base = ctx.globalAlpha;
    let batch: string | null = null;
    const flush = () => {
      if (batch !== null) ctx.stroke();
      batch = null;
    };
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    for (const o of objs) {
      if (o.bbox.maxX - o.bbox.minX < lod && o.bbox.maxY - o.bbox.minY < lod) continue;
      for (const op of o.ops) {
        if (op.t === "seg") {
          const w = Math.max(op.width, minW);
          const key = `${op.colour}|${w}|${op.alpha ?? 1}`;
          if (key !== batch) {
            flush();
            ctx.globalAlpha = base * (op.alpha ?? 1);
            ctx.strokeStyle = this.colour(op.colour);
            ctx.lineWidth = w;
            ctx.beginPath();
            batch = key;
          }
          ctx.moveTo(op.a[0], op.a[1]);
          ctx.lineTo(op.b[0], op.b[1]);
          continue;
        }
        flush();
        ctx.globalAlpha = base;
        this.drawOp(ctx, op, view, minW, delta);
      }
    }
    flush();
    ctx.globalAlpha = base;
  }

  private fillStroke(ctx: Ctx, fill: string | undefined, stroke: string | undefined, lineWidth: number | undefined, minW: number, fillAlpha = 1) {
    if (fill) {
      const a = ctx.globalAlpha;
      ctx.globalAlpha = a * fillAlpha;
      ctx.fillStyle = this.colour(fill);
      ctx.fill();
      ctx.globalAlpha = a;
    }
    if (stroke) {
      ctx.strokeStyle = this.colour(stroke);
      ctx.lineWidth = Math.max(lineWidth ?? 0, minW);
      ctx.stroke();
    }
  }

  private drawOp(ctx: Ctx, op: DrawOp, view: ViewTransform, minW: number, delta: [number, number] | null) {
    switch (op.t) {
      case "poly": {
        if (op.pts.length < 2) return;
        ctx.beginPath();
        ctx.moveTo(op.pts[0][0], op.pts[0][1]);
        for (let i = 1; i < op.pts.length; i++) ctx.lineTo(op.pts[i][0], op.pts[i][1]);
        ctx.closePath();
        this.fillStroke(ctx, op.fill, op.stroke, op.lineWidth, minW, op.alpha ?? 1);
        return;
      }
      case "rect":
        ctx.beginPath();
        roundRect(ctx, op.x, op.y, op.w, op.h, op.radius);
        this.fillStroke(ctx, op.fill, op.stroke, op.lineWidth, minW);
        return;
      case "circle":
        ctx.beginPath();
        ctx.arc(op.cx, op.cy, Math.max(op.r, minW / 2), 0, Math.PI * 2);
        this.fillStroke(ctx, op.fill, op.stroke, op.lineWidth, minW);
        return;
      case "text": {
        // Fonts are sized in device pixels (tiny world-unit font sizes are unreliable).
        const px = op.size * view.scale;
        if (px < op.minPx) return;
        const k = view.scale * view.dpr;
        const ox = delta ? delta[0] : 0;
        const oy = delta ? delta[1] : 0;
        ctx.save();
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.font = `${px * view.dpr}px ${FONT}`;
        ctx.textAlign = "center";
        ctx.textBaseline = "alphabetic";
        ctx.fillStyle = this.colour(op.colour);
        ctx.fillText(op.text, (op.x + ox - view.x0) * k, (op.y + oy - view.y0) * k);
        ctx.restore();
        return;
      }
      case "seg":
      case "label":
        return; // segments are batched by drawShapes; labels belong to annotation layers
    }
  }

  /** Constant-pixel annotations: drawn at `px` pixels, only where they fit their object. */
  private drawLabels(ctx: Ctx, objs: RenderObject[], view: ViewTransform) {
    const k = view.scale * view.dpr;
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    let font = "";
    for (const o of objs)
      for (const op of o.ops) {
        if (op.t !== "label") continue;
        if (op.px > op.fitH * view.scale * 1.05) continue; // taller than the pad or track
        const f = `${op.px * view.dpr}px ${FONT}`;
        if (f !== font) {
          ctx.font = f;
          font = f;
        }
        const w = ctx.measureText(op.text).width / view.dpr;
        if (w > op.fitW * view.scale * 0.9) continue; // longer than the pad or track run
        ctx.fillStyle = this.colour(op.colour);
        const x = (op.x - view.x0) * k;
        const y = (op.y - view.y0) * k;
        if (op.angle) {
          ctx.save();
          ctx.translate(x, y);
          ctx.rotate(op.angle);
          ctx.fillText(op.text, 0, 0);
          ctx.restore();
        } else ctx.fillText(op.text, x, y);
      }
    ctx.restore();
  }

  private drawOverlay(ctx: Ctx, view: ViewTransform) {
    const { items, moving } = this.overlay;
    if (moving && moving.owners.size) {
      ctx.save();
      ctx.globalAlpha = 0.85;
      for (const id of moving.owners) this.drawShapes(ctx, this.owners.get(id) ?? [], view, moving.delta);
      ctx.restore();
    }
    if (!items.length) return;
    ctx.save();
    this.world(ctx, view, null);
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    for (const it of items) {
      if (it.pts.length < 2) continue;
      ctx.beginPath();
      ctx.moveTo(it.pts[0][0], it.pts[0][1]);
      for (let i = 1; i < it.pts.length; i++) ctx.lineTo(it.pts[i][0], it.pts[i][1]);
      if (it.closed) ctx.closePath();
      if (it.fill && it.closed) {
        ctx.fillStyle = this.colour(it.fill);
        ctx.fill();
      }
      ctx.setLineDash(it.dash ? it.dash.map((d) => d / view.scale) : []);
      ctx.strokeStyle = this.colour(it.colour);
      ctx.lineWidth = it.px / view.scale;
      ctx.stroke();
    }
    ctx.setLineDash([]);
    ctx.restore();
  }
}

/** World box covered by device-pixel rectangle (x, y, w, h). */
function worldBox(view: ViewTransform, x: number, y: number, w: number, h: number): Box {
  const k = view.scale * view.dpr;
  return { minX: view.x0 + x / k, minY: view.y0 + y / k, maxX: view.x0 + (x + w) / k, maxY: view.y0 + (y + h) / k };
}

/**
 * Boxes that need redrawing between two object lists of one layer: old and new boxes of every
 * object whose signature changed, appeared or disappeared (merged where they overlap). Null when
 * more than `MAX_DIRTY_RECTS` changed: the caller redraws everything.
 */
export function diffBoxes(before: RenderObject[], after: RenderObject[]): Box[] | null {
  const old = new Map(before.map((o) => [o.key, o]));
  const out: Box[] = [];
  for (const o of after) {
    const p = old.get(o.key);
    if (p) old.delete(o.key);
    if (p && p.sig === o.sig) continue;
    out.push(o.bbox);
    if (p) out.push(p.bbox);
    if (out.length > MAX_DIRTY_RECTS) return null;
  }
  for (const p of old.values()) {
    out.push(p.bbox);
    if (out.length > MAX_DIRTY_RECTS) return null;
  }
  return mergeBoxes(out);
}

function mergeBoxes(boxes: Box[]): Box[] {
  const out: Box[] = [];
  for (const b of boxes) {
    let m = { ...b };
    for (let i = out.length - 1; i >= 0; i--)
      if (intersects(out[i], m)) {
        const o = out.splice(i, 1)[0];
        m = { minX: Math.min(o.minX, m.minX), minY: Math.min(o.minY, m.minY), maxX: Math.max(o.maxX, m.maxX), maxY: Math.max(o.maxY, m.maxY) };
      }
    out.push(m);
  }
  return out;
}

function roundRect(ctx: Ctx, x: number, y: number, w: number, h: number, r: number) {
  const rr = Math.min(r, w / 2, h / 2);
  if (rr <= 0) {
    ctx.rect(x, y, w, h);
    return;
  }
  ctx.moveTo(x + rr, y);
  ctx.lineTo(x + w - rr, y);
  ctx.arcTo(x + w, y, x + w, y + rr, rr);
  ctx.lineTo(x + w, y + h - rr);
  ctx.arcTo(x + w, y + h, x + w - rr, y + h, rr);
  ctx.lineTo(x + rr, y + h);
  ctx.arcTo(x, y + h, x, y + h - rr, rr);
  ctx.lineTo(x, y + rr);
  ctx.arcTo(x, y, x + rr, y, rr);
  ctx.closePath();
}

const now = () => (typeof performance !== "undefined" ? performance.now() : Date.now());
