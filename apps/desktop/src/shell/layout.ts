// Persisted workspace layout: panel sizes and open/closed flags. Pure helpers so the
// clamping and (de)serialisation are unit-testable; the key is per machine.
export interface Layout {
  sideOpen: boolean;
  bottomOpen: boolean;
  aiOpen: boolean;
  sideWidth: number;
  aiWidth: number;
  bottomHeight: number;
}

export const DEFAULT_LAYOUT: Layout = { sideOpen: true, bottomOpen: true, aiOpen: true, sideWidth: 272, aiWidth: 336, bottomHeight: 240 };
export const LIMITS = { sideWidth: [160, 560], aiWidth: [240, 640], bottomHeight: [96, 640] } as const;
const KEY = "eak.layout.v1";

const clamp = (v: unknown, [lo, hi]: readonly [number, number], fallback: number) =>
  typeof v === "number" && Number.isFinite(v) ? Math.min(hi, Math.max(lo, Math.round(v))) : fallback;

/** Validate untrusted stored JSON; anything malformed falls back to the default per field. */
export function sanitizeLayout(raw: unknown): Layout {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const flag = (k: keyof Layout) => (typeof r[k] === "boolean" ? (r[k] as boolean) : (DEFAULT_LAYOUT[k] as boolean));
  return {
    sideOpen: flag("sideOpen"),
    bottomOpen: flag("bottomOpen"),
    aiOpen: flag("aiOpen"),
    sideWidth: clamp(r.sideWidth, LIMITS.sideWidth, DEFAULT_LAYOUT.sideWidth),
    aiWidth: clamp(r.aiWidth, LIMITS.aiWidth, DEFAULT_LAYOUT.aiWidth),
    bottomHeight: clamp(r.bottomHeight, LIMITS.bottomHeight, DEFAULT_LAYOUT.bottomHeight),
  };
}

export function loadLayout(): Layout {
  try {
    return sanitizeLayout(JSON.parse(localStorage.getItem(KEY) ?? "null"));
  } catch {
    return DEFAULT_LAYOUT;
  }
}

export function saveLayout(l: Layout): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(l));
  } catch {
    /* storage unavailable: the layout still applies for this session */
  }
}

export const clampSize = (k: "sideWidth" | "aiWidth" | "bottomHeight", v: number) => clamp(v, LIMITS[k], DEFAULT_LAYOUT[k]);
