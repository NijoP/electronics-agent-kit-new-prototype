// Scene renderer contract (Phase 1 slice A4, architecture decision AD-4).
//
// A scene is a flat render-object table plus a layer draw-order stack. The same table feeds
// drawing (any `SceneRenderer` backend), picking (`pick.ts`) and, later, the object tree and the
// property panel. Coordinates are view millimetres with Y pointing down (`viewport.ts` `toView`),
// the same space the SVG canvas and the pointer contract (`onPoint`, `onHover`, `onDrag`) use.
//
// The object model follows the render-output design of the EasyEDA viewer (scene + flat pick table
// + layer order + constant-pixel annotations), re-implemented here; no code is taken from it.
import type { Id } from "../types";

/** Axis-aligned box in view millimetres. */
export interface Box {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

export type Pt = [number, number];

/** What a pick can return: the IR object a render object stands for. */
/** `pin`, `pad` and `graphic` are the symbol and footprint editors' objects (D2): a pin or pad number, a graphic index. */
export type PickKind = "footprint" | "track" | "via" | "zone" | "pin" | "pad" | "graphic";
export interface PickRef {
  kind: PickKind;
  id: Id;
}

/**
 * One drawing primitive. Colours are a CSS colour, or a design-token name starting with `--`
 * (resolved by the backend against the document, so the canvas follows the theme).
 */
export type DrawOp =
  | { t: "seg"; a: Pt; b: Pt; width: number; colour: string; alpha?: number }
  | { t: "poly"; pts: Pt[]; fill?: string; stroke?: string; lineWidth?: number; alpha?: number }
  | { t: "rect"; x: number; y: number; w: number; h: number; radius: number; fill?: string; stroke?: string; lineWidth?: number }
  | { t: "circle"; cx: number; cy: number; r: number; fill?: string; stroke?: string; lineWidth?: number }
  /** World-size text (scales with zoom), e.g. a reference designator. Hidden under `minPx` pixels. */
  | { t: "text"; x: number; y: number; text: string; size: number; colour: string; minPx: number }
  /**
   * Constant-pixel annotation: drawn at `px` screen pixels whatever the zoom, and only when it fits
   * inside `fitW` x `fitH` view millimetres (a pad, a track run). Pad numbers and net names.
   */
  | { t: "label"; x: number; y: number; text: string; px: number; colour: string; fitW: number; fitH: number; angle: number };

export interface RenderObject {
  /** Unique within the scene, stable across rebuilds (used for dirty-rect diffs). */
  key: string;
  /** The IR object this stands for when picked; null for decoration (outline, labels, airwires). */
  ref: PickRef | null;
  /** Render layer it is drawn on (see `RenderLayer.key`). */
  layerKey: string;
  /** Coarse pick and fit box. */
  bbox: Box;
  /** Precise pick: is view point (x, y) on the object within `tol` mm? */
  hit: (x: number, y: number, tol: number) => boolean;
  /** Selection outline that hugs the object's path (tracks, zones). */
  pathPts?: Pt[];
  /** Selection box polygon (footprints, pads). */
  selPoly?: Pt[];
  /** Copper layers it can be picked on; through-hole objects list every layer they span. */
  pickLayers?: string[];
  /** Footprint that owns it (pads, courtyard, refdes), for drag previews. */
  owner?: Id;
  ops: DrawOp[];
  /** Content signature: equal signatures draw identically (dirty-rect diff). */
  sig: string;
}

export interface RenderLayer {
  key: string;
  /** Draw order, back to front. */
  order: number;
  /** Layer opacity (the far board side is drawn semi-transparent). */
  alpha: number;
  /** Annotation layers are drawn live at constant pixel size and never cached. */
  annotation?: boolean;
}

export interface Scene {
  /** Sorted back to front. */
  layers: RenderLayer[];
  objects: RenderObject[];
  /** Extents of everything in the scene, or null when empty. */
  bounds: Box | null;
}

/** A view: world point (x0, y0) is the top-left pixel; `scale` CSS pixels per millimetre. */
export interface ViewTransform {
  x0: number;
  y0: number;
  scale: number;
  /** Pane size in CSS pixels. */
  width: number;
  height: number;
  /** Device pixel ratio. */
  dpr: number;
}

/** Dynamic, uncached drawing over the scene: selection, highlights, drag previews. */
export interface OverlayItem {
  pts: Pt[];
  closed: boolean;
  colour: string;
  /** Line width in screen pixels (constant whatever the zoom). */
  px: number;
  fill?: string;
  dash?: number[];
}

export interface OverlayScene {
  items: OverlayItem[];
  /** Footprints drawn translated by `delta` (view mm) while they are dragged. */
  moving?: { owners: Set<Id>; delta: Pt } | null;
}

export interface PickOptions {
  /** Pick tolerance in view mm (usually a few screen pixels / scale). */
  tol?: number;
  /** Only objects of these kinds. */
  kinds?: PickKind[];
  /**
   * Active-layer-exclusive picking: only objects pickable on this copper layer (`pickLayers`);
   * a miss is a miss and never falls through to other layers. Objects without `pickLayers`
   * (footprints) stay pickable.
   */
  activeLayer?: string;
}

export interface PickHit {
  ref: PickRef;
  object: RenderObject;
}

export interface RenderStats {
  /** Layers redrawn completely this frame. */
  fullLayers: number;
  /** Dirty rectangles redrawn this frame (pan strips and changed objects). */
  dirtyRects: number;
  /** Layers shown from the offscreen composite without redrawing any of it. */
  cachedLayers: number;
  /** Layers drawn straight to the screen (over the cache memory budget, or annotations). */
  directLayers: number;
  /** Annotation layers skipped because none of their labels fits at this zoom (level of detail). */
  lodLayers: number;
  /** Render objects drawn this frame. */
  drawn: number;
  /**
   * The frame shows the cache scaled while a zoom moves; it is redrawn sharp by a frame rendered
   * once the zoom has been still for `SETTLE_MS` (`canvas2d.ts`). The host must request that frame.
   */
  stale: boolean;
  ms: number;
}

/**
 * A scene backend. Canvas2D is the first (`canvas2d.ts`); the SVG markup in `PcbView.tsx` stays as
 * the fallback path behind the renderer flag (`flag.ts`), and SVG remains the export backend.
 */
export interface SceneRenderer {
  setScene(scene: Scene): void;
  setOverlay(overlay: OverlayScene): void;
  render(view: ViewTransform): RenderStats;
  pick(x: number, y: number, opts?: PickOptions): PickHit | null;
  /** Drop every cache (theme change, context loss). */
  invalidate(): void;
  dispose(): void;
}
