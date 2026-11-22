// Side-panel views. All read the latest engine snapshot; none keeps its own project model. (The
// sheet / part / net lists moved to the design manager, `DesignManager.tsx`, in Phase 1 A3.)
import type { Snapshot, ExportResult } from "../types";
import { ExportPanel } from "../ExportPanel";

export function netNames(s: Snapshot): string[] {
  const { schematic, pcb } = s.project;
  const names = new Set<string>();
  for (const l of Object.values(schematic.labels)) names.add(l.name);
  for (const t of Object.values(pcb.tracks)) if (t.net) names.add(t.net);
  for (const f of s.geometry.footprints) for (const p of f.pads) if (p.net) names.add(p.net);
  return [...names].sort();
}

export function ManufacturingPanel({ result, busy, onExport }: { result: ExportResult | null; busy: boolean; onExport: () => void }) {
  return (
    <div className="panel-body">
      <button type="button" className="primary" onClick={onExport} disabled={busy} title="Gerber, Excellon, BOM, CPL">
        Export Gerbers + drill
      </button>
      {result ? <ExportPanel result={result} /> : <p className="empty-state">Nothing exported in this session. Exports are DRAFT packages; EAK never calls them manufacturing-ready.</p>}
    </div>
  );
}
