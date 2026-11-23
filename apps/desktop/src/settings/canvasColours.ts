// Per-object canvas colours (Phase 1 D4), on top of the design tokens (`design/tokens.css`).
// The theme's tokens stay the default; a preset ("paper": printable black on white) and the user's
// own colours override single `--c-*` tokens on the document element, so every renderer that reads
// the tokens (SVG schematic, Canvas2D PCB scene; both redraw on a style change) follows. Copper
// layer colours are per layer in the Layers panel (`layers.ts`) and are not repeated here.
import { useSyncExternalStore } from "react";

export interface CanvasObject {
  token: string;
  label: string;
  editor: "Schematic" | "PCB";
}
export const CANVAS_OBJECTS: readonly CanvasObject[] = [
  { token: "--c-sheet", label: "Sheet background", editor: "Schematic" },
  { token: "--c-sheet-line", label: "Sheet border", editor: "Schematic" },
  { token: "--c-wire", label: "Wire", editor: "Schematic" },
  { token: "--c-label", label: "Net label", editor: "Schematic" },
  { token: "--c-power", label: "Power port", editor: "Schematic" },
  { token: "--c-text", label: "Text", editor: "Schematic" },
  { token: "--c-sym-fill", label: "Symbol body", editor: "Schematic" },
  { token: "--c-sym-line", label: "Symbol outline", editor: "Schematic" },
  { token: "--c-sym-sel-fill", label: "Selected symbol", editor: "Schematic" },
  { token: "--c-board", label: "Board", editor: "PCB" },
  { token: "--c-pad", label: "Pad (top)", editor: "PCB" },
  { token: "--c-pad-bottom", label: "Pad (bottom)", editor: "PCB" },
  { token: "--c-courtyard", label: "Courtyard", editor: "PCB" },
  { token: "--c-via", label: "Via", editor: "PCB" },
  { token: "--c-preview", label: "Route preview", editor: "PCB" },
];

export type ColourPreset = "theme" | "paper";
export const COLOUR_PRESETS: readonly { id: ColourPreset; label: string }[] = [
  { id: "theme", label: "Theme colours" },
  { id: "paper", label: "Paper (printable, black on white)" },
];
/** Printable preset: white sheet, black ink; PCB objects in dark greys on a light board. */
export const PAPER: Readonly<Record<string, string>> = {
  "--c-sheet": "#ffffff",
  "--c-sheet-line": "#000000",
  "--c-wire": "#000000",
  "--c-label": "#000000",
  "--c-power": "#000000",
  "--c-text": "#000000",
  "--c-sym-fill": "#ffffff",
  "--c-sym-line": "#000000",
  "--c-sym-sel-fill": "#d9d9d9",
  "--c-board": "#f2f2f2",
  "--c-pad": "#4d4d4d",
  "--c-pad-bottom": "#808080",
  "--c-courtyard": "#999999",
  "--c-via": "#333333",
  "--c-preview": "#000000",
};

export interface StoredColours {
  preset: ColourPreset;
  user: Record<string, string>;
}
const KEY = "eak.canvasColours";
const EMPTY: StoredColours = { preset: "theme", user: {} };
const HEX = /^#[0-9a-f]{6}$/i;
const KNOWN = new Set(CANVAS_OBJECTS.map((o) => o.token));

export function parseColours(raw: string | null): StoredColours {
  if (!raw) return EMPTY;
  try {
    const v = JSON.parse(raw) as Partial<StoredColours>;
    const user: Record<string, string> = {};
    for (const [t, c] of Object.entries(v.user ?? {})) if (KNOWN.has(t) && typeof c === "string" && HEX.test(c)) user[t] = c;
    return { preset: v.preset === "paper" ? "paper" : "theme", user };
  } catch {
    return EMPTY;
  }
}

let memory: string | null = null;
let cache: { raw: string | null; v: StoredColours } | null = null;
const listeners = new Set<() => void>();
function readRaw(): string | null {
  try {
    return localStorage.getItem(KEY) ?? memory;
  } catch {
    return memory;
  }
}
export function getColours(): StoredColours {
  const raw = readRaw();
  if (cache && cache.raw === raw) return cache.v;
  cache = { raw, v: parseColours(raw) };
  return cache.v;
}
function write(v: StoredColours) {
  memory = JSON.stringify(v);
  try {
    localStorage.setItem(KEY, memory);
  } catch {
    // Storage unavailable: the colours still apply for this session.
  }
  cache = { raw: readRaw(), v };
  applyColours();
  for (const l of listeners) l();
}
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};
export const useColours = () => useSyncExternalStore(subscribe, getColours, getColours);

/** Override of a token: the user's colour, else the preset's; undefined means the theme token. */
export function overrideOf(token: string, v: StoredColours = getColours()): string | undefined {
  return v.user[token] ?? (v.preset === "paper" ? PAPER[token] : undefined);
}

/** Writes the overrides onto the document element (and removes the ones no longer set). */
export function applyColours(root: HTMLElement | undefined = typeof document === "undefined" ? undefined : document.documentElement) {
  if (!root) return;
  const v = getColours();
  for (const o of CANVAS_OBJECTS) {
    const c = overrideOf(o.token, v);
    if (c) root.style.setProperty(o.token, c);
    else root.style.removeProperty(o.token);
  }
}

export const setColourPreset = (preset: ColourPreset) => write({ ...getColours(), preset });
export function setColour(token: string, colour: string) {
  if (!KNOWN.has(token) || !HEX.test(colour)) return;
  write({ ...getColours(), user: { ...getColours().user, [token]: colour.toLowerCase() } });
}
export function resetColour(token: string) {
  const user = { ...getColours().user };
  delete user[token];
  write({ ...getColours(), user });
}
export const resetColours = () => write(EMPTY);

/** For tests. */
export function resetColourStore() {
  memory = null;
  cache = null;
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* storage unavailable */
  }
  applyColours();
}
