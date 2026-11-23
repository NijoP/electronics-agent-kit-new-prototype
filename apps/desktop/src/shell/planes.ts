// Planes and stack-up (Phase 1 slice E5): pure helpers that turn Layers-panel edits into typed
// engine commands. The engine validates every stack-up (`Stackup::validate`) and computes every
// fill; nothing here is a second model.
import { layerName, sameLayer } from "../layers";
import type { Command, CopperLayer, Nm, Point, Snapshot, Stackup, Zone } from "../types";

/** Copper thickness per ounce, µm (eak-core `COPPER_UM_PER_OZ`). */
export const COPPER_UM_PER_OZ = 34.8;

/** Build-up total in µm: copper plus every dielectric gap (what `Stackup::validate` sums). */
export function buildUpUm(st: Stackup): number {
  return st.layers.reduce((t, l) => t + l.copper_oz * COPPER_UM_PER_OZ + l.dielectric_after_um, 0);
}

/**
 * The stack-up at a new finished thickness: the copper is kept and every dielectric gap is scaled
 * by the same factor so the build-up totals the new thickness. Gaps are whole µm and at least 1
 * (a thickness the copper alone exceeds is left for the engine to refuse with its reason).
 */
export function withThickness(st: Stackup, thickness: Nm): Stackup {
  const copper = st.layers.reduce((t, l) => t + l.copper_oz * COPPER_UM_PER_OZ, 0);
  const gaps = st.layers.reduce((t, l) => t + l.dielectric_after_um, 0);
  const target = thickness / 1000 - copper;
  const k = gaps > 0 ? target / gaps : 0;
  const last = st.layers.length - 1;
  return {
    ...st,
    board_thickness: thickness,
    layers: st.layers.map((l, i) => ({ ...l, dielectric_after_um: i === last ? 0 : Math.max(1, Math.round(l.dielectric_after_um * k)) })),
  };
}

const zonesOn = (s: Snapshot, l: CopperLayer): Zone[] => Object.values(s.project.pcb.zones).filter((z) => sameLayer(z.layer, l));

/** The net a plane layer carries: its zones' net, "" with no zone, null when they disagree. */
export function planeNet(s: Snapshot, l: CopperLayer): string | null {
  const nets = new Set(zonesOn(s, l).map((z) => z.net));
  if (nets.size === 0) return "";
  return nets.size === 1 ? [...nets][0] : null;
}

/**
 * Assign `net` to a plane layer: every zone on the layer is moved to the net (EditZone), or, with
 * no zone there yet, one zone over the board outline is added (AddZone). One command (a Batch for
 * several zones), so one revision and one undo step. Null when there is nothing to do or no outline.
 */
export function assignPlaneNet(s: Snapshot, l: CopperLayer, net: string, outline: Point[]): Command | null {
  const on = zonesOn(s, l);
  if (on.length === 0) return outline.length >= 3 ? { type: "AddZone", layer: l, net, outline, clearance: null } : null;
  const commands: Command[] = on.filter((z) => z.net !== net).map((z) => ({ type: "EditZone", zone_id: z.id, net }));
  if (commands.length <= 1) return commands[0] ?? null;
  return { type: "Batch", commands, label: `Assign ${net} to ${layerName(l)}` };
}

/** Each zone net with the copper layers its zones are on (StitchZones needs two or more). */
export function zoneNets(s: Snapshot): { net: string; layers: CopperLayer[] }[] {
  const by = new Map<string, CopperLayer[]>();
  for (const z of Object.values(s.project.pcb.zones)) {
    const ls = by.get(z.net) ?? [];
    if (!ls.some((l) => sameLayer(l, z.layer))) ls.push(z.layer);
    by.set(z.net, ls);
  }
  return [...by.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([net, layers]) => ({ net, layers }));
}

/** Why StitchZones cannot run for `net` (null: every net), or null when it can (the engine still decides). */
export function stitchBlocked(s: Snapshot, net: string | null): string | null {
  const nets = zoneNets(s).filter((n) => net === null || n.net === net);
  if (nets.length === 0) return net === null ? "No copper zones to stitch" : `No ${net} zone`;
  if (!nets.some((n) => n.layers.length >= 2))
    return net === null ? "Stitching needs a net with zones on two or more copper layers" : `Stitching needs a ${net} zone on two or more copper layers`;
  return null;
}
