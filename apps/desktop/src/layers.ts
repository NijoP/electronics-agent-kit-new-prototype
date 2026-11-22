// Copper-layer helpers for the PCB editor. Pure functions over the snapshot's stackup;
// the engine owns the stackup, this only names, orders and colours layers for display.
import type { Command, CopperLayer, Point, Stackup } from "./types";

/** Stable string key for a layer: "top", "bottom" or "in1", "in2", … */
export const layerKey = (l: CopperLayer): string =>
  typeof l === "string" ? l : `in${l.inner}`;

/** KiCad-style name: F.Cu, In1.Cu, B.Cu. */
export const layerName = (l: CopperLayer): string =>
  l === "top" ? "F.Cu" : l === "bottom" ? "B.Cu" : `In${l.inner}.Cu`;

export const sameLayer = (a: CopperLayer, b: CopperLayer): boolean => layerKey(a) === layerKey(b);

const INNER_COLOURS = ["#c9a227", "#3fa66b", "#8a5cc6", "#d9743c", "#2e9fb5", "#b34a7a"];

/** Display colour per layer (outer layers keep the familiar red/blue). */
export function layerColour(l: CopperLayer): string {
  if (l === "top") return "#c0392b";
  if (l === "bottom") return "#2f6fb5";
  return INNER_COLOURS[(l.inner - 1) % INNER_COLOURS.length];
}

/** Copper layers top to bottom; a project without a stackup is a 2-layer board. */
export function stackLayers(s: Stackup | undefined): { layer: CopperLayer; kind: "signal" | "plane" }[] {
  if (!s || s.layers.length === 0)
    return [
      { layer: "top", kind: "signal" },
      { layer: "bottom", kind: "signal" },
    ];
  return s.layers.map((l) => ({ layer: l.layer, kind: l.kind ?? "signal" }));
}

/** Layers a via from `from` to `to` touches, in stack order (either argument order). */
export function viaLayers(order: CopperLayer[], from: CopperLayer, to: CopperLayer): CopperLayer[] {
  const i = order.findIndex((l) => sameLayer(l, from));
  const j = order.findIndex((l) => sameLayer(l, to));
  if (i < 0 || j < 0) return [];
  return order.slice(Math.min(i, j), Math.max(i, j) + 1);
}

export type ViaKind = "through" | "blind" | "buried";
export function viaKind(from: CopperLayer, to: CopperLayer): ViaKind {
  const outer = (l: CopperLayer) => typeof l === "string";
  return outer(from) && outer(to) ? "through" : !outer(from) && !outer(to) ? "buried" : "blind";
}

/** Copper-layer counts offered for `SetLayerCount` (even, 2..16), plus `current` when it is not one. */
export const LAYER_COUNTS = [2, 4, 6, 8, 10, 12, 14, 16];
export const layerCountOptions = (current: number): number[] =>
  [...new Set([...LAYER_COUNTS, current])].sort((a, b) => a - b);

/**
 * "Fill planes with net": one zone over the board outline on every plane layer that has none,
 * as ONE command (a `Batch` when there are several), so it is one revision, one undo step and
 * all-or-nothing. Null when no plane layer needs a zone.
 */
export function planeFill(
  stack: { layer: CopperLayer; kind: "signal" | "plane" }[],
  hasZone: (l: CopperLayer) => boolean,
  net: string,
  outline: Point[],
): Command | null {
  const commands: Command[] = stack
    .filter((s) => s.kind === "plane" && !hasZone(s.layer))
    .map((s): Command => ({ type: "AddZone", layer: s.layer, net, outline, clearance: null }));
  if (commands.length <= 1) return commands[0] ?? null;
  return { type: "Batch", commands, label: `Fill planes with ${net}` };
}
