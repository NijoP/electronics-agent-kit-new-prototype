// Power-port list (Phase 1 slice B4): the entries of the schematic Power port split button, edited
// in Settings › Power ports and kept in local storage. Each entry is a net name and the symbol it
// is drawn with; placing one sends AddNetLabel with kind "power" and that style. EAK-IR has three
// power-port styles (ground, rail, chassis, ADR-0040), so analog ground is drawn as a ground.
import { useSyncExternalStore } from "react";
import type { PowerStyle } from "../types";
import type { PowerPreset } from "../tools/schematic/placeTools";

export const POWER_STYLES: readonly { id: PowerStyle; label: string }[] = [
  { id: "ground", label: "Ground" },
  { id: "rail", label: "Supply rail" },
  { id: "chassis", label: "Chassis" },
];

/** Default list: ground, supply rails, analog ground and chassis. */
export const DEFAULT_POWER_PORTS: readonly PowerPreset[] = [
  { name: "GND", style: "ground" },
  { name: "+3V3", style: "rail" },
  { name: "+5V", style: "rail" },
  { name: "AGND", style: "ground" },
  { name: "CHASSIS", style: "chassis" },
];

const KEY = "eak.schematic.powerPorts";
const isStyle = (s: unknown): s is PowerStyle => s === "ground" || s === "rail" || s === "chassis";

function load(): PowerPreset[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [...DEFAULT_POWER_PORTS];
    const v: unknown = JSON.parse(raw);
    if (!Array.isArray(v)) return [...DEFAULT_POWER_PORTS];
    return v.filter((p): p is PowerPreset => !!p && typeof p.name === "string" && isStyle(p.style)).map((p) => ({ name: p.name, style: p.style }));
  } catch {
    return [...DEFAULT_POWER_PORTS];
  }
}

let list: PowerPreset[] = load();
const listeners = new Set<() => void>();

export const powerPorts = (): readonly PowerPreset[] => list;

export function setPowerPorts(next: readonly PowerPreset[]): void {
  list = next.map((p) => ({ name: p.name, style: p.style }));
  try {
    localStorage.setItem(KEY, JSON.stringify(list));
  } catch {
    // Storage unavailable: the list still holds for this session.
  }
  listeners.forEach((l) => l());
}

export const resetPowerPorts = () => setPowerPorts(DEFAULT_POWER_PORTS);

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};
export const usePowerPorts = (): readonly PowerPreset[] => useSyncExternalStore(subscribe, powerPorts);

/** Why an entry cannot be placed, or null. */
export function powerPortProblem(p: PowerPreset): string | null {
  const n = p.name.trim();
  if (!n) return "Enter a net name";
  if (/\s/.test(n)) return "A net name has no spaces";
  return null;
}
