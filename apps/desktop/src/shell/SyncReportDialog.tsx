// Result of "Update PCB": what the engine's schematic-to-PCB sync added, removed and modified.
import type { SyncReport } from "./syncReport";

interface Props {
  /** Null when the sync was refused; `error` then says why. */
  report: SyncReport | null;
  error?: string | null;
  revision?: number;
  onOpenPcb: () => void;
  onClose: () => void;
}

function Section({ title, items, empty }: { title: string; items: string[]; empty: string }) {
  return (
    <section className="sr-section" aria-label={title}>
      <h4>
        {title} <span className="count">{items.length}</span>
      </h4>
      {items.length === 0 ? (
        <p className="muted">{empty}</p>
      ) : (
        <ul>
          {items.map((t, i) => (
            <li key={i}>{t}</li>
          ))}
        </ul>
      )}
    </section>
  );
}

export function SyncReportDialog({ report, error, revision, onOpenPcb, onClose }: Props) {
  return (
    <div className="palette-backdrop" onClick={onClose}>
      <div className="palette sync-report" role="dialog" aria-label="Update PCB report" onClick={(e) => e.stopPropagation()}>
        <h3>Update PCB{revision != null && report && !report.inSync ? ` → revision ${revision}` : ""}</h3>
        {error && <p role="alert" className="explorer-error">{error}</p>}
        {report?.inSync && <p>The PCB already matches the schematic. Nothing was added, removed or modified.</p>}
        {report && !report.inSync && (
          <>
            <div className="sr-grid">
              <Section title="Components added" items={report.components.added.map((c) => `${c.refdes} (${c.component})`)} empty="None" />
              <Section title="Components removed" items={report.components.removed.map((c) => `${c.refdes} (${c.component})`)} empty="None" />
              <Section title="Components modified" items={report.components.modified.map((c) => `${c.refdes}: ${c.details.join(", ")}`)} empty="None" />
              <Section title="Connections added" items={report.connections.added.map((c) => `${c.pad} → ${c.net}`)} empty="None" />
              <Section title="Connections removed" items={report.connections.removed.map((c) => `${c.pad} (was ${c.net})`)} empty="None" />
              <Section title="Connections re-assigned" items={report.connections.modified.map((c) => `${c.pad}: ${c.from} → ${c.to}`)} empty="None" />
            </div>
            <p className="muted sr-foot">
              Kept as it was: {report.preserved.footprints} footprint(s), {report.preserved.tracks} track segment(s), {report.preserved.vias} via(s).
              {report.copper.tracks + report.copper.vias + report.copper.zones > 0 &&
                ` The sync also changed ${report.copper.tracks} track(s), ${report.copper.vias} via(s) and ${report.copper.zones} zone(s).`}{" "}
              New footprints are staged beside the board; airwires show the connections still to route.
            </p>
          </>
        )}
        <div className="dlg-actions">
          <button type="button" className="primary" onClick={onOpenPcb} disabled={!report}>Open PCB editor</button>
          <button type="button" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
