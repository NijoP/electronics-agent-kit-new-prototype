// "Update PCB" report: what the schematic-to-PCB synchronisation added, removed and modified.
// It is computed from the engine's own journalled change list for the SyncSchematicToPcb
// operation (the before/after values it applied), so the report cannot disagree with the
// project. Pure functions: the IPC call lives in the workspace.
import type { Id } from "../types";

export interface EngineChange {
  path: string[];
  key: string | null;
  before: unknown;
  after: unknown;
}

interface FootprintLike {
  refdes?: string;
  component_id?: string;
  side?: string;
  locked?: boolean;
  position?: { x: number; y: number };
  rotation?: unknown;
  pad_nets?: Record<string, string>;
}

export interface ComponentChange {
  refdes: string;
  component: string;
}
export interface ComponentEdit {
  refdes: string;
  /** Human-readable field changes, e.g. `reference R3 → R4`, `part R_0603 → R_0805`. */
  details: string[];
}
export interface ConnectionChange {
  /** `R1.2` style pad reference. */
  pad: string;
  net: string;
}
export interface ConnectionEdit {
  pad: string;
  from: string;
  to: string;
}

export interface SyncReport {
  components: { added: ComponentChange[]; removed: ComponentChange[]; modified: ComponentEdit[] };
  connections: { added: ConnectionChange[]; removed: ConnectionChange[]; modified: ConnectionEdit[] };
  /** Copper and zones the sync itself touched (normally none: existing routing is preserved). */
  copper: { tracks: number; vias: number; zones: number };
  /** Existing footprints, tracks and vias the sync left exactly as they were. */
  preserved: { footprints: number; tracks: number; vias: number };
  /** True when the sync changed nothing at all. */
  inSync: boolean;
}

const asFp = (v: unknown): FootprintLike | null => (v && typeof v === "object" ? (v as FootprintLike) : null);
const isPath = (c: EngineChange, ...p: string[]) => c.path.length === p.length && p.every((s, i) => c.path[i] === s);
const posText = (p?: { x: number; y: number }) => (p ? `${p.x},${p.y}` : "");

/** pad number -> net, for one footprint. */
const padMap = (fp: FootprintLike | null): Map<string, string> => new Map(Object.entries(fp?.pad_nets ?? {}));

/**
 * Build the report from the engine's change list. `after` is the project's PCB state after the
 * operation, used only to count what was preserved.
 */
export function buildSyncReport(
  changes: EngineChange[],
  after: { footprints: Record<Id, unknown>; tracks: Record<Id, unknown>; vias: Record<Id, unknown> },
): SyncReport {
  const report: SyncReport = {
    components: { added: [], removed: [], modified: [] },
    connections: { added: [], removed: [], modified: [] },
    copper: { tracks: 0, vias: 0, zones: 0 },
    preserved: { footprints: 0, tracks: 0, vias: 0 },
    inSync: changes.length === 0,
  };
  let footprintsChanged = 0; // footprints present after the sync that it added or modified
  for (const c of changes) {
    if (isPath(c, "pcb", "footprints")) {
      const before = asFp(c.before);
      const now = asFp(c.after);
      if (!before && now) {
        footprintsChanged += 1;
        const ref = now.refdes ?? c.key ?? "?";
        report.components.added.push({ refdes: ref, component: now.component_id ?? "" });
        for (const [pad, net] of padMap(now)) report.connections.added.push({ pad: `${ref}.${pad}`, net });
      } else if (before && !now) {
        const ref = before.refdes ?? c.key ?? "?";
        report.components.removed.push({ refdes: ref, component: before.component_id ?? "" });
        for (const [pad, net] of padMap(before)) report.connections.removed.push({ pad: `${ref}.${pad}`, net });
      } else if (before && now) {
        const ref = now.refdes ?? c.key ?? "?";
        const details: string[] = [];
        if (before.refdes !== now.refdes) details.push(`reference ${before.refdes} → ${now.refdes}`);
        if (before.component_id !== now.component_id) details.push(`part ${before.component_id} → ${now.component_id}`);
        if (before.side !== now.side) details.push(`side ${before.side} → ${now.side}`);
        if (posText(before.position) !== posText(now.position)) details.push("position changed");
        if (JSON.stringify(before.rotation) !== JSON.stringify(now.rotation)) details.push("rotation changed");
        if (before.locked !== now.locked) details.push(now.locked ? "locked" : "unlocked");
        // Compare connections by pad number so a rename does not read as every pad changing.
        const bp = padMap(before);
        const np = padMap(now);
        let connectionsChanged = false;
        for (const [pad, net] of np) {
          const old = bp.get(pad);
          if (old === undefined) report.connections.added.push({ pad: `${ref}.${pad}`, net });
          else if (old !== net) report.connections.modified.push({ pad: `${ref}.${pad}`, from: old, to: net });
          connectionsChanged ||= old !== net;
        }
        for (const [pad, net] of bp)
          if (!np.has(pad)) {
            report.connections.removed.push({ pad: `${before.refdes ?? ref}.${pad}`, net });
            connectionsChanged = true;
          }
        if (details.length || connectionsChanged) {
          footprintsChanged += 1;
          report.components.modified.push({ refdes: ref, details: details.length ? details : ["connections updated"] });
        }
      }
    } else if (isPath(c, "pcb", "tracks")) {
      report.copper.tracks += 1;
    } else if (isPath(c, "pcb", "vias")) {
      report.copper.vias += 1;
    } else if (isPath(c, "pcb", "zones")) {
      report.copper.zones += 1;
    }
  }
  const total = (m: Record<Id, unknown>) => Object.keys(m).length;
  report.preserved = {
    footprints: Math.max(0, total(after.footprints) - footprintsChanged),
    tracks: Math.max(0, total(after.tracks) - report.copper.tracks),
    vias: Math.max(0, total(after.vias) - report.copper.vias),
  };
  return report;
}

export function summarizeSyncReport(r: SyncReport): string {
  if (r.inSync) return "The PCB already matches the schematic: nothing to update.";
  const c = r.components;
  const k = r.connections;
  return `Update PCB: ${c.added.length} component(s) added, ${c.removed.length} removed, ${c.modified.length} modified; ${k.added.length} connection(s) added, ${k.removed.length} removed, ${k.modified.length} re-assigned.`;
}
