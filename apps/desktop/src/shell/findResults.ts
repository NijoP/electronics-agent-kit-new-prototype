// Find Results (Phase 1 slice E4), wired to the B3 Find and replace bar. A small store:
//  * the bar publishes its query and options; the bottom dock's Find Results tab recomputes the
//    hits from the live snapshot (so they follow every revision) across all sheets;
//  * "List in Find Results" asks the workspace to show that tab;
//  * a click on a result leaves a pick for the schematic editor that shows the hit's sheet, which
//    takes it and selects the object (symbol, wire or net label) on its canvas. Every other
//    cross-probe into the schematic (Design manager, Quick open, ERC findings; F7) uses the same pick.
import { useSyncExternalStore } from "react";
import type { Id } from "../types";
import type { FindOptions } from "../tools/schematic/find";
import type { Kind } from "../tools/schematic/selection";

/** An object for the schematic editor of `sheet` to select (a Find hit is one). */
export interface CanvasPick {
  kind: Kind;
  id: Id;
  sheet: Id;
}

export interface FindSearch {
  query: string;
  options: FindOptions;
}

let search: FindSearch | null = null;
let signature = "";
let reveal = 0;
let pick: CanvasPick | null = null;
let pickSeq = 0;
let version = 0;
const listeners = new Set<() => void>();
const emit = () => {
  version += 1;
  listeners.forEach((l) => l());
};
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};

/** The bar's current search (republishing the same search notifies nobody). */
export function publishFindSearch(query: string, options: FindOptions): void {
  const sig = JSON.stringify([query, options]);
  if (sig === signature) return;
  signature = sig;
  search = { query, options };
  emit();
}
export const findSearch = (): FindSearch | null => search;
export function useFindSearch(): FindSearch | null {
  useSyncExternalStore(subscribe, () => version);
  return search;
}

/** Ask the workspace to show the Find Results tab. */
export function revealFindResults(): void {
  reveal += 1;
  emit();
}
export const useFindReveal = (): number => useSyncExternalStore(subscribe, () => reveal);

/** Leave an object for the schematic editor of its sheet to select. */
export function requestFindPick(hit: CanvasPick): void {
  pick = hit;
  pickSeq += 1;
  emit();
}
/** The pending pick when it is on `sheetId` (taken once), else null. */
export function takeFindPick(sheetId: Id): CanvasPick | null {
  if (!pick || pick.sheet !== sheetId) return null;
  const h = pick;
  pick = null;
  return h;
}
export const useFindPickSeq = (): number => useSyncExternalStore(subscribe, () => pickSeq);

/** Tests: forget the search and any pending pick. */
export function resetFindStore(): void {
  search = null;
  signature = "";
  pick = null;
  emit();
}
