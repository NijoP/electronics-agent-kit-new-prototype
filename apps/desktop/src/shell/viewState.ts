// Per-document canvas state shared between the workspace and the canvases without touching the
// editors: which document a canvas belongs to (context), its last viewport (kept across tab
// closes and sessions), and imperative zoom / tool controls the toolbar drives.
import { createContext, useContext, useEffect, useSyncExternalStore } from "react";
import type { Bounds, ViewBox } from "../viewport";

export const DocIdContext = createContext<string | null>(null);
export const useDocId = () => useContext(DocIdContext);

const KEY = (id: string) => `eak.viewport.${id}`;
const mem = new Map<string, ViewBox>();
const timers = new Map<string, ReturnType<typeof setTimeout>>();

const finite = (n: unknown): n is number => typeof n === "number" && Number.isFinite(n);
export function sanitizeViewBox(raw: unknown): ViewBox | null {
  const r = raw as Partial<ViewBox> | null;
  return r && finite(r.x) && finite(r.y) && finite(r.w) && finite(r.h) && r.w > 0 && r.h > 0 ? { x: r.x, y: r.y, w: r.w, h: r.h } : null;
}

export function loadViewport(docId: string): ViewBox | null {
  const hit = mem.get(docId);
  if (hit) return hit;
  try {
    const vb = sanitizeViewBox(JSON.parse(localStorage.getItem(KEY(docId)) ?? "null"));
    if (vb) mem.set(docId, vb);
    return vb;
  } catch {
    return null;
  }
}

/** Remember immediately in memory; write to localStorage after the pan/zoom settles. */
export function saveViewport(docId: string, vb: ViewBox): void {
  mem.set(docId, vb);
  clearTimeout(timers.get(docId));
  timers.set(
    docId,
    setTimeout(() => {
      try {
        localStorage.setItem(KEY(docId), JSON.stringify(vb));
      } catch {
        /* storage unavailable */
      }
    }, 250),
  );
}

export function forgetViewports(): void {
  mem.clear();
}

export interface CanvasControls {
  zoomBy: (factor: number) => void;
  fit: () => void;
  /** Zoom to view-space bounds (design manager navigation). */
  zoomTo: (b: Bounds) => void;
}
const canvases = new Map<string, CanvasControls>();
export const canvasControls = (docId: string | null) => (docId ? canvases.get(docId) : undefined);
export function useRegisterCanvas(docId: string | null, c: CanvasControls): void {
  useEffect(() => {
    if (!docId) return;
    canvases.set(docId, c);
    return () => {
      if (canvases.get(docId) === c) canvases.delete(docId);
    };
  });
}

const tools = new Map<string, (tool: string) => boolean>();
/** Returns true when the editor accepted the tool id. */
export const setEditorTool = (docId: string | null, tool: string): boolean => (docId ? (tools.get(docId)?.(tool) ?? false) : false);
export function useRegisterTools(docId: string | null, set: (tool: string) => boolean): void {
  useEffect(() => {
    if (!docId) return;
    tools.set(docId, set);
    return () => {
      if (tools.get(docId) === set) tools.delete(docId);
    };
  });
}

// Which editor owns the keyboard. Both editors listen on `window`, so in split view every
// keypress would act in both; each editor ignores keys unless it is the active one. Clicking an
// editor (or its tab) makes it active. No active editor recorded = no restriction (single view).
let activeEditor: string | null = null;
export const setActiveEditor = (id: string | null) => {
  activeEditor = id;
};
export const isActiveEditor = (id: string | null) => id === null || activeEditor === null || activeEditor === id;

// Canvas readout of each document (A3 canvas strip): the pointer position, the anchor of the last
// primary press (for dX / dY) and the scale, all in view millimetres (Y down) as the canvas reports
// them. A small external store, so only the strip re-renders on pointer moves.
export interface CanvasReadout {
  at: { x: number; y: number } | null;
  anchor: { x: number; y: number } | null;
  /** Screen pixels per view millimetre; null until the pane is measured. */
  pxPerMm: number | null;
}
const EMPTY_READOUT: CanvasReadout = { at: null, anchor: null, pxPerMm: null };
const readouts = new Map<string, CanvasReadout>();
const readoutListeners = new Set<() => void>();
export function reportCanvas(docId: string | null, patch: Partial<CanvasReadout>): void {
  if (!docId) return;
  const cur = readouts.get(docId) ?? EMPTY_READOUT;
  const next = { ...cur, ...patch };
  if (next.at === cur.at && next.anchor === cur.anchor && next.pxPerMm === cur.pxPerMm) return;
  readouts.set(docId, next);
  readoutListeners.forEach((l) => l());
}
export const canvasReadout = (docId: string | null): CanvasReadout => (docId ? readouts.get(docId) : undefined) ?? EMPTY_READOUT;
const subscribeReadout = (l: () => void) => {
  readoutListeners.add(l);
  return () => {
    readoutListeners.delete(l);
  };
};
export const useCanvasReadout = (docId: string | null): CanvasReadout => useSyncExternalStore(subscribeReadout, () => canvasReadout(docId));
