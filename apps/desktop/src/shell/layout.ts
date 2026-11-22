// Persisted dock layout (Phase 1 A3, spec UI-01): three docks (left, right, bottom), each with an
// open flag, a size in px and its active tab, saved per workspace. A workspace is what the editor
// area shows (`DocContext`: home, schematic, PCB, 3D, file), so the PCB keeps its own docks and
// tabs (e.g. Layers on the right) apart from the schematic. Pure helpers, so clamping, migration
// and (de)serialisation are unit-testable; storage is per machine (localStorage).
import { useCallback, useState } from "react";
import type { DocContext } from "../actions/types";

export type DockId = "left" | "right" | "bottom";
export const DOCK_IDS: readonly DockId[] = ["left", "right", "bottom"];

export interface Dock {
  open: boolean;
  /** Width (left, right) or height (bottom) in px. */
  size: number;
  /** Active tab of the dock (left: a side view; right: properties / filter / layers; bottom: a bottom tab). */
  tab: string;
}
export type Layout = Record<DockId, Dock>;
export type LayoutWorkspace = DocContext;

export const LIMITS: Record<DockId, readonly [number, number]> = { left: [160, 560], right: [200, 560], bottom: [96, 640] };

/** Defaults per workspace: the editors open the design manager and the right dock; other views keep it closed. */
export function defaultLayout(ws: LayoutWorkspace): Layout {
  const editor = ws === "schematic" || ws === "pcb";
  return {
    left: { open: true, size: 272, tab: editor ? "design" : "project" },
    right: { open: editor, size: 280, tab: ws === "pcb" ? "layers" : "properties" },
    bottom: { open: true, size: 240, tab: "log" },
  };
}
/** The PCB editor's defaults (the sizes are the same in every workspace). */
export const DEFAULT_LAYOUT: Layout = defaultLayout("pcb");

export const layoutKey = (ws: LayoutWorkspace) => `eak.layout.v2.${ws}`;
/** Single machine-wide layout saved before A3 (side and bottom panel only). */
const V1_KEY = "eak.layout.v1";

const clamp = (v: unknown, [lo, hi]: readonly [number, number], fallback: number) =>
  typeof v === "number" && Number.isFinite(v) ? Math.min(hi, Math.max(lo, Math.round(v))) : fallback;

export const clampSize = (dock: DockId, v: number) => clamp(v, LIMITS[dock], DEFAULT_LAYOUT[dock].size);

const TAB_RE = /^[a-z0-9][a-z0-9_-]{0,39}$/;

function sanitizeDock(dock: DockId, raw: unknown, fallback: Dock): Dock {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  return {
    open: typeof r.open === "boolean" ? r.open : fallback.open,
    size: clamp(r.size, LIMITS[dock], fallback.size),
    tab: typeof r.tab === "string" && TAB_RE.test(r.tab) ? r.tab : fallback.tab,
  };
}

/** Validate untrusted stored JSON; anything malformed falls back to the workspace default per field. */
export function sanitizeLayout(ws: LayoutWorkspace, raw: unknown): Layout {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const d = defaultLayout(ws);
  return { left: sanitizeDock("left", r.left, d.left), right: sanitizeDock("right", r.right, d.right), bottom: sanitizeDock("bottom", r.bottom, d.bottom) };
}

/** A pre-A3 layout (`sideOpen`, `sideWidth`, `bottomOpen`, `bottomHeight`) seeds the left and bottom docks. */
export function migrateV1(ws: LayoutWorkspace, raw: unknown): Layout {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const d = defaultLayout(ws);
  return sanitizeLayout(ws, {
    left: { ...d.left, open: r.sideOpen, size: r.sideWidth },
    right: d.right,
    bottom: { ...d.bottom, open: r.bottomOpen, size: r.bottomHeight },
  });
}

const read = (key: string): unknown => {
  try {
    const s = localStorage.getItem(key);
    return s === null ? undefined : JSON.parse(s);
  } catch {
    return undefined;
  }
};

export function loadLayout(ws: LayoutWorkspace): Layout {
  const v2 = read(layoutKey(ws));
  if (v2 !== undefined) return sanitizeLayout(ws, v2);
  const v1 = read(V1_KEY);
  return v1 !== undefined ? migrateV1(ws, v1) : defaultLayout(ws);
}

export function saveLayout(ws: LayoutWorkspace, l: Layout): void {
  try {
    localStorage.setItem(layoutKey(ws), JSON.stringify(l));
  } catch {
    /* storage unavailable: the layout still applies for this session */
  }
}

export type DockPatch = Partial<Dock> | ((d: Dock) => Partial<Dock>);

/** Apply a patch to one dock of a layout (size clamped, tab validated). */
export function patchLayout(l: Layout, dock: DockId, p: DockPatch): Layout {
  const cur = l[dock];
  return { ...l, [dock]: sanitizeDock(dock, { ...cur, ...(typeof p === "function" ? p(cur) : p) }, cur) };
}

/**
 * Layouts of every workspace, loaded on first use and written back on each change. `patch` and
 * `reset` name the workspace explicitly, so a navigation that switches workspace (e.g. a finding
 * that opens the PCB) changes the docks of the workspace it lands in.
 */
export function useDockLayouts() {
  const [all, setAll] = useState<Partial<Record<LayoutWorkspace, Layout>>>({});
  const get = useCallback((ws: LayoutWorkspace): Layout => all[ws] ?? loadLayout(ws), [all]);
  const patch = useCallback((ws: LayoutWorkspace, dock: DockId, p: DockPatch) => {
    setAll((a) => {
      const next = patchLayout(a[ws] ?? loadLayout(ws), dock, p);
      saveLayout(ws, next);
      return { ...a, [ws]: next };
    });
  }, []);
  const reset = useCallback((ws: LayoutWorkspace) => {
    const d = defaultLayout(ws);
    saveLayout(ws, d);
    setAll((a) => ({ ...a, [ws]: d }));
  }, []);
  return { get, patch, reset };
}
