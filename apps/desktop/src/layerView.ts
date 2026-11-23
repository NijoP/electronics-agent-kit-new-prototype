// Per-project, per-machine display state for the PCB layer manager: which copper layer is
// active, which layers are hidden or locked, colour overrides and presets, airwire visibility and
// the flip-board / outline view modes. Pure view state: the stackup itself (layers, kinds,
// thickness) always comes from, and is edited through, the engine. A layer lock is an editor
// guard of this view (no selection, no edits on that layer); it is not stored in the design.
import { useCallback, useEffect, useState } from "react";
import { layerColour, layerKey, sameLayer } from "./layers";
import type { CopperLayer, Id } from "./types";

/** A layer the view can show, hide and lock: a copper layer or the board outline. */
export type ViewLayer = CopperLayer | "outline";
export const viewKey = (l: ViewLayer): string => (l === "outline" ? "outline" : layerKey(l));

export interface LayerPrefs {
  active: string;
  hidden: string[];
  locked: string[];
  colours: Record<string, string>;
  airwires: boolean;
  /** View the board from the bottom (mirrored left to right). */
  flip: boolean;
  /** Draw copper, pads and zones as outlines only. */
  outline: boolean;
}
export const DEFAULT_PREFS: LayerPrefs = { active: "top", hidden: [], locked: [], colours: {}, airwires: true, flip: false, outline: false };
const storeKey = (id: Id) => `eak.layers.${id}`;

const HEX = /^#[0-9a-fA-F]{6}$/;
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : []);
export function sanitizePrefs(raw: unknown): LayerPrefs {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const colours: Record<string, string> = {};
  if (r.colours && typeof r.colours === "object")
    for (const [k, v] of Object.entries(r.colours as Record<string, unknown>)) if (typeof v === "string" && HEX.test(v)) colours[k] = v;
  return {
    active: typeof r.active === "string" ? r.active : "top",
    hidden: strings(r.hidden),
    locked: strings(r.locked),
    colours,
    airwires: typeof r.airwires === "boolean" ? r.airwires : true,
    flip: r.flip === true,
    outline: r.outline === true,
  };
}

/** Colour presets: a colour per copper layer key ("top", "bottom", "in1", …). */
export interface ColourPreset {
  id: string;
  label: string;
  colour: (key: string) => string;
}
const innerOf = (inner: string[]) => (k: string) => inner[(Math.max(1, Number(k.slice(2)) || 1) - 1) % inner.length];
export const COLOUR_PRESETS: ColourPreset[] = [
  { id: "default", label: "EAK default", colour: (k) => layerColour(k === "top" || k === "bottom" ? k : { inner: Math.max(1, Number(k.slice(2)) || 1) }) },
  {
    id: "classic",
    label: "Classic (red / blue / green)",
    colour: (k) => (k === "top" ? "#c83434" : k === "bottom" ? "#4d7fc4" : innerOf(["#7fc87f", "#ce7d2c", "#4fcbcb", "#db628b", "#a7a5c6", "#c2c200"])(k)),
  },
  {
    id: "contrast",
    label: "High contrast",
    colour: (k) => (k === "top" ? "#ff2d2d" : k === "bottom" ? "#00b4ff" : innerOf(["#ffd400", "#00e676", "#ff6dff", "#ff9100", "#40c4ff", "#eeff41"])(k)),
  },
];

/**
 * The copper layer `step` places from `current` in stack order (top first), wrapping around:
 * +1 is the next layer down, -1 the one above.
 */
export function stepLayer(order: CopperLayer[], current: CopperLayer, step: 1 | -1): CopperLayer {
  if (!order.length) return current;
  const i = order.findIndex((o) => sameLayer(o, current));
  return order[((i < 0 ? 0 : i) + step + order.length) % order.length];
}

export interface LayerView {
  active: string;
  hidden: Set<string>;
  locked: Set<string>;
  airwires: boolean;
  flip: boolean;
  outline: boolean;
  setActive: (l: CopperLayer) => void;
  toggle: (l: ViewLayer) => void;
  toggleLock: (l: ViewLayer) => void;
  setColour: (l: CopperLayer, hex: string | null) => void;
  /** Set every listed layer's colour from a preset ("default" clears the overrides). */
  applyPreset: (id: string, layers: CopperLayer[]) => void;
  setAirwires: (on: boolean) => void;
  setFlip: (on: boolean) => void;
  setOutline: (on: boolean) => void;
  /** Colour to draw a layer with: the user's override or the default. */
  colourOf: (l: CopperLayer) => string;
}

const flipIn = (list: string[], k: string) => (list.includes(k) ? list.filter((h) => h !== k) : [...list, k]);

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
  const toggle = useCallback((l: ViewLayer) => setP((x) => ({ ...x, hidden: flipIn(x.hidden, viewKey(l)) })), []);
  const toggleLock = useCallback((l: ViewLayer) => setP((x) => ({ ...x, locked: flipIn(x.locked, viewKey(l)) })), []);
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
  const applyPreset = useCallback(
    (id: string, layers: CopperLayer[]) =>
      setP((x) => {
        const preset = COLOUR_PRESETS.find((c) => c.id === id);
        if (!preset) return x;
        const colours = { ...x.colours };
        for (const l of layers) {
          const k = layerKey(l);
          if (id === "default") delete colours[k];
          else colours[k] = preset.colour(k);
        }
        return { ...x, colours };
      }),
    [],
  );
  const setAirwires = useCallback((on: boolean) => setP((x) => ({ ...x, airwires: on })), []);
  const setFlip = useCallback((on: boolean) => setP((x) => ({ ...x, flip: on })), []);
  const setOutline = useCallback((on: boolean) => setP((x) => ({ ...x, outline: on })), []);
  return {
    active: p.active,
    hidden: new Set(p.hidden),
    locked: new Set(p.locked),
    airwires: p.airwires,
    flip: p.flip,
    outline: p.outline,
    setActive,
    toggle,
    toggleLock,
    setColour,
    applyPreset,
    setAirwires,
    setFlip,
    setOutline,
    colourOf: (l) => p.colours[layerKey(l)] ?? layerColour(l),
  };
}
