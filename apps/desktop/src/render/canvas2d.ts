// Canvas2D scene backend.
//
// Every non-annotation layer is drawn into its own offscreen surface at the current view and
// composited back to front with the layer's opacity. Surfaces are reused across frames:
//   - pan at the same zoom: the surface is shifted by whole device pixels and only the exposed
//     strips are redrawn (dirty rectangles);
//   - a scene change: objects whose signature changed are diffed per layer and only the union of
//     their old and new boxes is redrawn (dirty rectangles);
//   - zoom, resize or too many changes: the layer is redrawn completely.
// Surfaces are bounded by a memory budget; layers over it are drawn straight to the screen.
// Annotation layers (pad numbers, net names) are drawn live at constant pixel size, and the
// overlay (selection, highlights, drag previews) is drawn last, uncached.
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
  /** Total bytes the layer caches may use (default 96 MiB). */
  cacheBudgetBytes?: number;
}

/** More changed objects than this in one layer redraws the whole layer instead. */
export const MAX_DIRTY_RECTS = 64;
/** Device pixels added around a dirty rectangle for antialiasing. */
const AA_PAD = 2;
const FONT = "system-ui, sans-serif";

interface LayerState {
  layer: RenderLayer;
  objects: RenderObject[];
  index: GridIndex;
  surface: Surface | null;
  /** View the surface currently shows, or null when it must be redrawn completely. */
  drawnFor: ViewTransform | null;
  /** World boxes changed since the surface was drawn. */
  dirty: Box[];
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

const sameView = (a: ViewTransform, b: ViewTransform) =>
  a.x0 === b.x0 && a.y0 === b.y0 && a.scale === b.scale && a.width === b.width && a.height === b.height && a.dpr === b.dpr;

export class Canvas2DRenderer implements SceneRenderer {
  private readonly target: Ctx;
  private readonly createSurface: ((w: number, h: number) => Surface | null) | null;
  private readonly resolve: (token: string) => string;
  private readonly budget: number;
  private readonly colours = new Map<string, string>();
  private layers: LayerState[] = [];
  private annotationKeys = new Set<string>();
  private picker: PickIndex | null = null;
  private scene: Scene | null = null;
  private owners = new Map<string, RenderObject[]>();
  private overlay: OverlayScene = { items: [] };
  private scratch: Surface | null = null;

  constructor(target: Ctx, opts: Canvas2DOptions = {}) {
    this.target = target;
    this.createSurface = opts.createSurface === undefined ? defaultSurface : opts.createSurface;
    this.resolve = opts.resolveColour ?? documentColour;
    this.budget = opts.cacheBudgetBytes ?? 96 * 1024 * 1024;
  }

  setScene(scene: Scene): void {
    const prev = new Map(this.layers.map((l) => [l.layer.key, l]));
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
        const old = prev.get(layer.key);
        const state: LayerState = {
          layer,
          objects,
          index: new GridIndex(objects.map((o) => o.bbox)),
          surface: old?.surface ?? null,
          drawnFor: old?.drawnFor ?? null,
          dirty: old?.dirty ?? [],
        };
        if (old && state.drawnFor) {
          const changed = diffBoxes(old.objects, objects);
          if (changed === null || state.dirty.length + changed.length > MAX_DIRTY_RECTS) {
            state.drawnFor = null;
            state.dirty = [];
          } else state.dirty.push(...changed);
        }
        return state;
      });
    this.annotationKeys = new Set(scene.layers.filter((l) => l.annotation).map((l) => l.key));
    this.scene = scene;
    this.picker = null; // built on the first pick
    this.owners = new Map();
    for (const o of scene.objects)
      if (o.owner && !this.annotationKeys.has(o.layerKey)) {
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
    for (const l of this.layers) {
      l.drawnFor = null;
      l.dirty = [];
    }
  }

  dispose(): void {
    for (const l of this.layers) l.surface = null;
    this.layers = [];
    this.scratch = null;
    this.picker = null;
    this.scene = null;
  }

  render(view: ViewTransform): RenderStats {
    const t0 = now();
    const stats: RenderStats = { fullLayers: 0, dirtyRects: 0, cachedLayers: 0, directLayers: 0, drawn: 0, ms: 0 };
    const W = Math.max(1, Math.round(view.width * view.dpr));
    const H = Math.max(1, Math.round(view.height * view.dpr));
    const ctx = this.target;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.clearRect(0, 0, W, H);
    const visible = worldBox(view, 0, 0, W, H);
    const cached = this.cachedSet(W, H);
    for (const l of this.layers) {
      if (!l.objects.length) continue;
      const surface = !l.layer.annotation && cached.has(l) ? this.prepare(l, W, H) : null;
      if (!surface) {
        stats.directLayers++;
        ctx.save();
        ctx.globalAlpha = l.layer.alpha;
        stats.drawn += this.drawRegion(ctx, l, view, visible, l.layer.annotation ? "labels" : "shapes");
        ctx.restore();
        continue;
      }
      const before = stats.fullLayers + stats.dirtyRects;
      stats.drawn += this.updateSurface(l, surface, view, W, H, stats);
      if (stats.fullLayers + stats.dirtyRects === before) stats.cachedLayers++;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.globalAlpha = l.layer.alpha;
      ctx.drawImage(surface.canvas, 0, 0);
    }
    ctx.globalAlpha = 1;
    this.drawOverlay(ctx, view);
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    stats.ms = now() - t0;
    return stats;
  }

  /** Layers that get an offscreen surface: the busiest ones that fit the memory budget. */
  private cachedSet(W: number, H: number): Set<LayerState> {
    if (!this.createSurface) return new Set();
    const per = W * H * 4;
    const slots = Math.floor(this.budget / per) - 1; // one slot is the pan scratch surface
    const candidates = this.layers.filter((l) => l.objects.length && !l.layer.annotation).sort((a, b) => b.objects.length - a.objects.length);
    const chosen = new Set(candidates.slice(0, Math.max(0, slots)));
    for (const l of this.layers)
      if (!chosen.has(l) && l.surface) {
        l.surface = null; // over budget now: release it
        l.drawnFor = null;
      }
    return chosen;
  }

  /** The layer's surface at W x H device pixels (a resize forces a full redraw), or null. */
  private prepare(l: LayerState, W: number, H: number): Surface | null {
    const s = l.surface;
    if (s && s.canvas.width === W && s.canvas.height === H) return s;
    l.drawnFor = null;
    l.dirty = [];
    if (s) {
      s.canvas.width = W;
      s.canvas.height = H;
      return s;
    }
    l.surface = this.createSurface ? this.createSurface(W, H) : null;
    return l.surface;
  }

  /** Bring a layer's surface up to `view`; returns the number of objects drawn. */
  private updateSurface(l: LayerState, s: Surface, view: ViewTransform, W: number, H: number, stats: RenderStats): number {
    const was = l.drawnFor;
    if (was && sameView(was, view)) return this.redrawDirty(l, s, view, stats);
    if (was && was.scale === view.scale && was.dpr === view.dpr && was.width === view.width && was.height === view.height) {
      // Pan: the content moves by (dx, dy) device pixels. Reuse it when that is a whole-pixel shift.
      const k = view.scale * view.dpr;
      const dx = (was.x0 - view.x0) * k;
      const dy = (was.y0 - view.y0) * k;
      const rx = Math.round(dx);
      const ry = Math.round(dy);
      if (Math.abs(dx - rx) < 0.01 && Math.abs(dy - ry) < 0.01 && Math.abs(rx) < W && Math.abs(ry) < H) {
        const sc = this.scratchSurface(W, H);
        if (sc) {
          sc.ctx.setTransform(1, 0, 0, 1, 0, 0);
          sc.ctx.globalAlpha = 1;
          sc.ctx.clearRect(0, 0, W, H);
          sc.ctx.drawImage(s.canvas, 0, 0);
          s.ctx.setTransform(1, 0, 0, 1, 0, 0);
          s.ctx.globalAlpha = 1;
          s.ctx.clearRect(0, 0, W, H);
          s.ctx.drawImage(sc.canvas, rx, ry);
          // Exposed strips, in device pixels.
          const strips: [number, number, number, number][] = [];
          if (rx > 0) strips.push([0, 0, rx, H]);
          if (rx < 0) strips.push([W + rx, 0, -rx, H]);
          if (ry > 0) strips.push([0, 0, W, ry]);
          if (ry < 0) strips.push([0, H + ry, W, -ry]);
          let drawn = 0;
          for (const [x, y, w, h] of strips) {
            drawn += this.clipDraw(s.ctx, l, view, x, y, w, h);
            stats.dirtyRects++;
          }
          l.drawnFor = view;
          return drawn + this.redrawDirty(l, s, view, stats);
        }
      }
    }
    return this.full(l, s, view, W, H, stats);
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

  private full(l: LayerState, s: Surface, view: ViewTransform, W: number, H: number, stats: RenderStats): number {
    s.ctx.setTransform(1, 0, 0, 1, 0, 0);
    s.ctx.globalAlpha = 1;
    s.ctx.clearRect(0, 0, W, H);
    const n = this.drawRegion(s.ctx, l, view, worldBox(view, 0, 0, W, H), "shapes");
    l.drawnFor = view;
    l.dirty = [];
    stats.fullLayers++;
    return n;
  }

  private redrawDirty(l: LayerState, s: Surface, view: ViewTransform, stats: RenderStats): number {
    let n = 0;
    const k = view.scale * view.dpr;
    const W = s.canvas.width;
    const H = s.canvas.height;
    for (const b of l.dirty) {
      const x = Math.floor((b.minX - view.x0) * k) - AA_PAD;
      const y = Math.floor((b.minY - view.y0) * k) - AA_PAD;
      const x2 = Math.ceil((b.maxX - view.x0) * k) + AA_PAD;
      const y2 = Math.ceil((b.maxY - view.y0) * k) + AA_PAD;
      const cx = Math.max(0, x);
      const cy = Math.max(0, y);
      const cw = Math.min(W, x2) - cx;
      const ch = Math.min(H, y2) - cy;
      if (cw <= 0 || ch <= 0) continue;
      n += this.clipDraw(s.ctx, l, view, cx, cy, cw, ch);
      stats.dirtyRects++;
    }
    l.dirty = [];
    return n;
  }

  /** Clear and redraw one device-pixel rectangle of a layer surface. */
  private clipDraw(ctx: Ctx, l: LayerState, view: ViewTransform, x: number, y: number, w: number, h: number): number {
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.beginPath();
    ctx.rect(x, y, w, h);
    ctx.clip();
    ctx.clearRect(x, y, w, h);
    const n = this.drawRegion(ctx, l, view, worldBox(view, x, y, w, h), "shapes");
    ctx.restore();
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
    const base = ctx.globalAlpha;
    let batch: string | null = null;
    const flush = () => {
      if (batch !== null) ctx.stroke();
      batch = null;
    };
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    for (const o of objs)
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
 * more than `MAX_DIRTY_RECTS` changed: the caller redraws the whole layer.
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
