// Selection filter of the schematic (slice B3, EasyEDA `s24`): which object kinds a click, a
// marquee, Select all and Find similar may pick. A view preference kept per machine, never
// design data; the right dock's Filter tab edits it and every schematic editor reads it.
import { useSyncExternalStore } from "react";
import { ALL_KINDS, type Filter, type Kind } from "./selection";

export const FILTER_KINDS: readonly { kind: Kind; label: string }[] = [
  { kind: "symbol", label: "Parts (symbols)" },
  { kind: "wire", label: "Wires" },
  { kind: "label", label: "Net labels and power ports" },
];

const KEY = "eak.schematic.selectFilter";
const listeners = new Set<() => void>();

function load(): Filter {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? "null") as Partial<Filter> | null;
    if (raw && typeof raw === "object") return { symbol: raw.symbol !== false, wire: raw.wire !== false, label: raw.label !== false };
  } catch {
    /* storage unavailable or corrupt: everything selectable */
  }
  return ALL_KINDS;
}
let state: Filter = load();

export const selectFilter = (): Filter => state;
export function setSelectFilter(patch: Partial<Filter>): void {
  state = { ...state, ...patch };
  try {
    localStorage.setItem(KEY, JSON.stringify(state));
  } catch {
    /* storage unavailable: kept for this session */
  }
  listeners.forEach((l) => l());
}
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};
export const useSelectFilter = (): Filter => useSyncExternalStore(subscribe, selectFilter);
