// Layer locks and active-layer picking (Phase 1 slice C1). Pure functions over the snapshot:
// which view layers ("top", "in1", "bottom", "outline") an object sits on or a command would
// change, so a locked layer blocks both selecting and editing what is on it. Locks are view state
// (`layerView.ts`), checked here before a command is sent; the engine is not asked to know them.
import { layerKey, layerName, stackLayers, viaLayers } from "./layers";
import type { Command, CopperLayer, Snapshot } from "./types";

export type LockKind = "footprint" | "track" | "via" | "zone";

const order = (s: Snapshot): CopperLayer[] => stackLayers(s.project.pcb.board.stackup).map((l) => l.layer);
const viaSpan = (s: Snapshot, from: CopperLayer | null | undefined, to: CopperLayer | null | undefined) =>
  viaLayers(order(s), from ?? "top", to ?? "bottom").map(layerKey);
const footprintSide = (s: Snapshot, id: string): string => {
  const side = s.geometry.footprints.find((f) => f.id === id)?.side ?? s.project.pcb.footprints[id]?.side;
  return side === "bottom" ? "bottom" : "top";
};

/** View layers an object sits on: its copper layer, a via's span, a footprint's board side. */
export function objectLayers(s: Snapshot, kind: LockKind, id: string): string[] {
  const { pcb } = s.project;
  switch (kind) {
    case "track":
      return pcb.tracks[id] ? [layerKey(pcb.tracks[id].layer)] : [];
    case "zone":
      return pcb.zones[id] ? [layerKey(pcb.zones[id].layer)] : [];
    case "via":
      return pcb.vias[id] ? viaSpan(s, pcb.vias[id].from, pcb.vias[id].to) : [];
    case "footprint":
      return [footprintSide(s, id)];
  }
}

/** True when the object is on a locked layer (it cannot be selected or edited). */
export const objectLocked = (s: Snapshot, locked: Set<string>, kind: LockKind, id: string): boolean =>
  locked.size > 0 && objectLayers(s, kind, id).some((k) => locked.has(k));

/**
 * Active-layer-exclusive picking (the EasyEDA viewer's rule): tracks and zones pick only on their
 * own layer, vias on any layer they span; footprints pick on every layer. A miss is a miss.
 */
export function pickableOn(s: Snapshot, kind: LockKind, id: string, active: string): boolean {
  if (kind === "footprint") return true;
  return objectLayers(s, kind, id).includes(active);
}

/** View layers a command would change. Board-wide routing commands touch every copper layer. */
export function commandLayers(s: Snapshot, c: Command): string[] {
  const all = order(s).map(layerKey);
  switch (c.type) {
    case "Batch":
      return [...new Set(c.commands.flatMap((x) => commandLayers(s, x)))];
    case "SetBoardOutline":
      return ["outline"];
    case "MoveFootprint":
      return [...new Set([footprintSide(s, c.footprint_id), ...(c.side ? [c.side === "bottom" ? "bottom" : "top"] : [])])];
    case "SetFootprintLocked":
      return [footprintSide(s, c.footprint_id)];
    case "AddTrack":
    case "AddZone":
      return [layerKey(c.layer)];
    case "DeleteTrack":
      return objectLayers(s, "track", c.track_id);
    case "DeleteZone":
      return objectLayers(s, "zone", c.zone_id);
    case "AddVia":
      return viaSpan(s, c.from, c.to);
    case "DeleteVia":
      return objectLayers(s, "via", c.via_id);
    case "AutoRoute":
    case "ClearRouting":
    case "StitchZones":
      return all;
    case "AutoPlace":
      return ["top", "bottom"];
    default:
      return [];
  }
}

const viewName = (k: string) => (k === "outline" ? "Board outline" : k === "top" ? layerName("top") : k === "bottom" ? layerName("bottom") : layerName({ inner: Number(k.slice(2)) }));

/** Why a command is refused because of locked layers, or null when it touches none. */
export function lockProblem(s: Snapshot, locked: Set<string>, c: Command): string | null {
  if (!locked.size) return null;
  const hit = commandLayers(s, c).filter((k) => locked.has(k));
  if (!hit.length) return null;
  const names = hit.map(viewName).join(", ");
  return `${names} ${hit.length === 1 ? "is" : "are"} locked: unlock ${hit.length === 1 ? "it" : "them"} in the Layers panel to edit.`;
}
