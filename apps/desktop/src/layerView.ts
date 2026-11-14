// Per-project, per-machine display state for the PCB layer manager: which copper layer is
// active, which are hidden, colour overrides and airwire visibility. Pure display state: the
// stackup itself (layers, kinds, thickness) always comes from, and is edited through, the engine.
import { useCallback, useEffect, useState } from "react";
import { layerColour, layerKey } from "./layers";
import type { CopperLayer, Id } from "./types";

export interface LayerPrefs {
  active: string;
  hidden: string[];
  colours: Record<string, string>;
  airwires: boolean;
}
export const DEFAULT_PREFS: LayerPrefs = { active: "top", hidden: [], colours: {}, airwires: true };
const storeKey = (id: Id) => `eak.layers.${id}`;

const HEX = /^#[0-9a-fA-F]{6}$/;
export function sanitizePrefs(raw: unknown): LayerPrefs {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const colours: Record<string, string> = {};
  if (r.colours && typeof r.colours === "object")
    for (const [k, v] of Object.entries(r.colours as Record<string, unknown>)) if (typeof v === "string" && HEX.test(v)) colours[k] = v;
  return {
    active: typeof r.active === "string" ? r.active : "top",
    hidden: Array.isArray(r.hidden) ? r.hidden.filter((x): x is string => typeof x === "string") : [],
    colours,
    airwires: typeof r.airwires === "boolean" ? r.airwires : true,
  };
}

export interface LayerView {
  active: string;
  hidden: Set<string>;
  airwires: boolean;
  setActive: (l: CopperLayer) => void;
  toggle: (l: CopperLayer) => void;
  setColour: (l: CopperLayer, hex: string | null) => void;
  setAirwires: (on: boolean) => void;
  /** Colour to draw a layer with: the user's override or the default. */
  colourOf: (l: CopperLayer) => string;
}

export function useLayerView(projectId: Id | null): LayerView {
  const [p, setP] = useState<LayerPrefs>(() => {
    if (!projectId) return DEFAULT_PREFS;
    try {
      return sanitizePrefs(JSON.parse(localStorage.getItem(storeKey(projectId)) ?? "null"));
    } catch {
      return DEFAULT_PREFS;
    }
  });
  useEffect(() => {
    if (!projectId) return;
    try {
      localStorage.setItem(storeKey(projectId), JSON.stringify(p));
    } catch {
      /* storage unavailable */
    }
  }, [projectId, p]);
  const setActive = useCallback((l: CopperLayer) => setP((x) => ({ ...x, active: layerKey(l) })), []);
  const toggle = useCallback(
    (l: CopperLayer) =>
      setP((x) => {
        const k = layerKey(l);
        return { ...x, hidden: x.hidden.includes(k) ? x.hidden.filter((h) => h !== k) : [...x.hidden, k] };
      }),
    [],
  );
  const setColour = useCallback(
    (l: CopperLayer, hex: string | null) =>
      setP((x) => {
        const colours = { ...x.colours };
        if (hex && HEX.test(hex)) colours[layerKey(l)] = hex;
        else delete colours[layerKey(l)];
        return { ...x, colours };
      }),
    [],
  );
  const setAirwires = useCallback((on: boolean) => setP((x) => ({ ...x, airwires: on })), []);
  return {
    active: p.active,
    hidden: new Set(p.hidden),
    airwires: p.airwires,
    setActive,
    toggle,
    setColour,
    setAirwires,
    colourOf: (l) => p.colours[layerKey(l)] ?? layerColour(l),
  };
}
