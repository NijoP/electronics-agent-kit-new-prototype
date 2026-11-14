// Copper-layer helpers for the PCB editor. Pure functions over the snapshot's stackup;
// the engine owns the stackup, this only names, orders and colours layers for display.
import type { CopperLayer, Stackup } from "./types";

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
