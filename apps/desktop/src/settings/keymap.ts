// User key map of the action registry (Phase 1 D4). The registry's default key map
// (`design/shortcuts.ts`) stays the source of the EAK preset; this store adds a chosen preset and
// the user's own rebindings on top, persisted per machine. `keysOf` in `actions/registry.ts` reads
// the effective binding from here, so a rebinding shows in menus, toolbar tooltips, the command
// palette, Help and the key handler at once. A view preference, not design data.
import { useSyncExternalStore } from "react";
import { DEFAULT_KEYMAP } from "../design/shortcuts";
import type { Action, DocContext } from "../actions/types";
import { DOC_CONTEXTS } from "../actions/types";

export type PresetId = "eak" | "easyeda-style";
export const PRESETS: readonly { id: PresetId; label: string; note: string }[] = [
  { id: "eak", label: "EAK default", note: "The EAK key map." },
  {
    id: "easyeda-style",
    label: "EasyEDA-style",
    note: "EAK-authored map for users with EasyEDA Pro habits: tools and checks on Alt and Shift letters. Not a copy of any vendor preset.",
  },
];

/**
 * EasyEDA-style preset, written by EAK: only the actions it changes, on top of the EAK default.
 * Bare letters stay with the editors (they own the canvas focus), so tools get Alt+letter.
 */
export const EASYEDA_STYLE: Readonly<Record<string, readonly string[]>> = {
  "t-wire": ["Alt+W"],
  "t-label": ["Alt+N"],
  "t-bus": ["Alt+B"],
  "t-noconnect": ["Alt+Q"],
  "t-place": ["Alt+P"],
  "t-route": ["Alt+T"],
  "t-via": ["Alt+V"],
  "t-zone": ["Alt+Z"],
  "t-outline": ["Alt+O"],
  erc: ["Shift+E"],
  drc: ["Shift+D"],
  sync: ["Shift+U"],
  fit: ["Shift+F"],
  zin: ["Mod+="],
  zout: ["Mod+-"],
  "grid-show": ["Shift+G"],
};

export interface StoredKeymap {
  preset: PresetId;
  /** action id → keys; an empty list means the user removed every key. */
  user: Record<string, string[]>;
}
const KEY = "eak.keymap";
const EMPTY: StoredKeymap = { preset: "eak", user: {} };
let memory: string | null = null;
let storageOk = typeof localStorage !== "undefined";

function readRaw(): string | null {
  if (storageOk)
    try {
      return localStorage.getItem(KEY);
    } catch {
      storageOk = false;
    }
  return memory;
}

/** Stored key map, repaired: an unknown preset or a non-list binding is dropped. */
export function parseKeymap(raw: string | null): StoredKeymap {
  if (!raw) return EMPTY;
  try {
    const v = JSON.parse(raw) as Partial<StoredKeymap>;
    const preset = PRESETS.some((p) => p.id === v.preset) ? (v.preset as PresetId) : "eak";
    const user: Record<string, string[]> = {};
    for (const [id, ks] of Object.entries(v.user ?? {})) if (Array.isArray(ks) && ks.every((k) => typeof k === "string")) user[id] = ks;
    return { preset, user };
  } catch {
    return EMPTY;
  }
}

let cache: { raw: string | null; v: StoredKeymap } | null = null;
const listeners = new Set<() => void>();
export function getKeymap(): StoredKeymap {
  const raw = readRaw();
  if (cache && cache.raw === raw) return cache.v;
  cache = { raw, v: parseKeymap(raw) };
  return cache.v;
}
function write(v: StoredKeymap) {
  const raw = JSON.stringify(v);
  memory = raw;
  if (storageOk)
    try {
      localStorage.setItem(KEY, raw);
    } catch {
      // Storage unavailable: the key map still applies for this session.
      storageOk = false;
    }
  cache = { raw: readRaw(), v };
  for (const l of listeners) l();
}
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => void listeners.delete(l);
};
/** Re-renders the caller when the key map changes. */
export const useKeymap = () => useSyncExternalStore(subscribe, getKeymap, getKeymap);

/** Keys of the preset alone (no user rebindings). */
export function presetKeys(a: Pick<Action, "id" | "keys">, preset: PresetId = getKeymap().preset): readonly string[] {
  if (preset === "easyeda-style" && EASYEDA_STYLE[a.id]) return EASYEDA_STYLE[a.id];
  return a.keys ?? DEFAULT_KEYMAP[a.id] ?? [];
}
/** Effective keys: the user's rebinding, else the preset's. */
export function effectiveKeys(a: Pick<Action, "id" | "keys">): readonly string[] {
  const m = getKeymap();
  return m.user[a.id] ?? presetKeys(a, m.preset);
}

export function setPreset(preset: PresetId) {
  write({ ...getKeymap(), preset });
}
/** Drops every user rebinding (the preset stays). */
export function resetAll() {
  write({ ...getKeymap(), user: {} });
}
export function resetAction(id: string) {
  const user = { ...getKeymap().user };
  delete user[id];
  write({ ...getKeymap(), user });
}
export function clearAction(id: string) {
  write({ ...getKeymap(), user: { ...getKeymap().user, [id]: [] } });
}

/** Canonical spec of a key event ("Mod+Shift+K"), or null for a lone modifier. */
export function specOf(e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">): string | null {
  if (["Control", "Meta", "Shift", "Alt", "AltGraph", "CapsLock", "Dead", "Unidentified", "OS"].includes(e.key)) return null;
  const key = e.key === " " ? "Space" : e.key.length === 1 ? e.key.toUpperCase() : e.key;
  return [e.ctrlKey || e.metaKey ? "Mod" : "", e.altKey ? "Alt" : "", e.shiftKey ? "Shift" : "", key].filter(Boolean).join("+");
}
/** Spec with modifiers in canonical order and the key case-folded, so "shift+mod+k" equals "Mod+Shift+K". */
export function normalize(spec: string): string {
  const parts = spec.split("+");
  // "Mod++" (the plus key) splits into a trailing empty part.
  const key = parts[parts.length - 1] === "" ? "+" : parts[parts.length - 1];
  const mods = parts.slice(0, -1).map((p) => p.toLowerCase());
  const has = (m: string) => mods.includes(m.toLowerCase());
  return [has("Mod") ? "Mod" : "", has("Alt") ? "Alt" : "", has("Shift") ? "Shift" : "", key.length === 1 ? key.toUpperCase() : key].filter(Boolean).join("+");
}

const EDITING: readonly DocContext[] = ["schematic", "pcb"];
/** Keys the editors handle themselves (they own the canvas focus; `actions/editorKeys.ts`): never bindable to an action there. */
export const EDITOR_RESERVED: Readonly<Record<string, readonly DocContext[]>> = {
  ...Object.fromEntries(["1", "2", "3", "4", "5", "6", "7", "8", "9"].map((d) => [d, EDITING])),
  W: ["schematic"],
  R: EDITING,
  X: ["schematic"],
  Y: ["schematic"],
  F: ["pcb"],
  L: ["pcb"],
  "/": EDITING,
  Delete: EDITING,
  Backspace: EDITING,
  Enter: EDITING,
  Space: ["schematic"],
  "Mod+C": ["schematic"],
  "Mod+X": ["schematic"],
  "Mod+V": ["schematic"],
  "Mod+D": ["schematic"],
  "Mod+F": ["schematic"],
  "Mod+G": ["schematic"],
  "Mod+Shift+G": ["schematic"],
  "Mod+A": EDITING,
};

const ctxOf = (a: Pick<Action, "docs">): readonly DocContext[] => a.docs ?? DOC_CONTEXTS;
const overlap = (x: readonly DocContext[], y: readonly DocContext[]) => x.some((c) => y.includes(c));

/**
 * Why `spec` cannot be bound to `target`, or null when it can. A key is refused when another
 * action that exists in a shared document already uses it, or an editor owns it there.
 */
export function conflictFor(actions: readonly Action[], target: Action, spec: string): string | null {
  const n = normalize(spec);
  if (n === "Escape") return "Escape closes dialogs and clears the selection; it cannot be rebound";
  const reserved = EDITOR_RESERVED[n];
  if (reserved && overlap(reserved, ctxOf(target))) return `${n} is used by the ${reserved.filter((c) => ctxOf(target).includes(c)).join(" and ")} editor`;
  for (const b of actions) {
    if (b.id === target.id || !overlap(ctxOf(b), ctxOf(target))) continue;
    if (effectiveKeys(b).some((k) => normalize(k) === n)) return `${n} is already bound to "${b.label}"`;
  }
  return null;
}

/** Binds `spec` as the only key of `target`; returns the refusal reason instead when it conflicts. */
export function bind(actions: readonly Action[], target: Action, spec: string): string | null {
  const why = conflictFor(actions, target, spec);
  if (why) return why;
  write({ ...getKeymap(), user: { ...getKeymap().user, [target.id]: [normalize(spec)] } });
  return null;
}

/** Every pair of actions in a shared document bound to the same key (empty for a sound key map). */
export function conflicts(actions: readonly Action[]): { key: string; a: string; b: string }[] {
  const out: { key: string; a: string; b: string }[] = [];
  actions.forEach((a, i) =>
    actions.slice(i + 1).forEach((b) => {
      if (!overlap(ctxOf(a), ctxOf(b))) return;
      for (const k of effectiveKeys(a)) if (effectiveKeys(b).some((j) => normalize(j) === normalize(k))) out.push({ key: normalize(k), a: a.id, b: b.id });
    }),
  );
  return out;
}

/** For tests: forget the stored key map. */
export function resetKeymapStore() {
  memory = null;
  cache = null;
  try {
    localStorage.removeItem(KEY);
  } catch {
    /* storage unavailable */
  }
  for (const l of listeners) l();
}
