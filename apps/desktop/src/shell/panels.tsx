// Side-panel views. All read the latest engine snapshot; none keeps its own project model.
import type { Id, LibraryEntry, Snapshot, ExportResult } from "../types";
import { ExportPanel } from "../ExportPanel";

export function netNames(s: Snapshot): string[] {
  const { schematic, pcb } = s.project;
  const names = new Set<string>();
  for (const l of Object.values(schematic.labels)) names.add(l.name);
  for (const t of Object.values(pcb.tracks)) if (t.net) names.add(t.net);
  for (const f of s.geometry.footprints) for (const p of f.pads) if (p.net) names.add(p.net);
  return [...names].sort();
}

export function ProjectPanel({
  snapshot,
  selected,
  onSelect,
  onNet,
  onSheet,
}: {
  snapshot: Snapshot;
  selected: Id | null;
  onSelect: (id: Id) => void;
  onNet: (name: string) => void;
  onSheet: (id: Id) => void;
}) {
  const { schematic } = snapshot.project;
  const sheets = Object.values(schematic.sheets);
  const symbols = Object.values(schematic.symbols).sort((a, b) => a.refdes.localeCompare(b.refdes, undefined, { numeric: true }));
  const nets = netNames(snapshot);
  return (
    <div className="panel-body">
      <h3>Sheets</h3>
      <ul className="tree">
        {sheets.map((s) => (
          <li key={s.id}><button type="button" onClick={() => onSheet(s.id)}>{s.name}</button></li>
        ))}
      </ul>
      <h3>Parts <span className="count">{symbols.length}</span></h3>
      {symbols.length === 0 ? (
        <p className="empty-state">No parts yet. Open Library / Parts and pick one to place, or press Ctrl+P.</p>
      ) : (
        <ul className="tree">
          {symbols.map((s) => (
            <li key={s.id}>
              <button type="button" className={s.id === selected ? "on" : ""} onClick={() => onSelect(s.id)}>
                <strong>{s.refdes}</strong> <span className="muted">{s.value}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
      <h3>Nets <span className="count">{nets.length}</span></h3>
      {nets.length === 0 ? (
        <p className="empty-state">No nets yet. Connect pins or add net labels in the schematic.</p>
      ) : (
        <ul className="tree">
          {nets.map((n) => (
            <li key={n}><button type="button" onClick={() => onNet(n)}>{n}</button></li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function PartsPanel({ library, query, onQuery, onPlace }: { library: LibraryEntry[]; query: string; onQuery: (q: string) => void; onPlace: (c: LibraryEntry) => void }) {
  const q = query.trim().toLowerCase();
  const shown = library.filter((c) => `${c.name} ${c.id} ${c.category}`.toLowerCase().includes(q));
  return (
    <div className="panel-body">
      <input aria-label="Filter parts" placeholder="Filter parts…" value={query} onChange={(e) => onQuery(e.target.value)} />
      {shown.length === 0 ? (
        <p className="empty-state">{library.length === 0 ? "The part library is empty or unavailable." : "No part matches the filter."}</p>
      ) : (
        <ul className="tree">
          {shown.map((c) => (
            <li key={c.id}>
              <button type="button" title={`Place ${c.name}`} onClick={() => onPlace(c)}>
                <strong>{c.name}</strong> <span className="muted">{c.category} · {c.pins.length} pins</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
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
