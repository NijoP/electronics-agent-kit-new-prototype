// Bottom panel: ERC/DRC findings, a BOM derived from the schematic, and the engine log.
import { EngineeringMonitor } from "../EngineeringMonitor";
import type { Check, CheckResult, EngineEvent, Finding, Id, Snapshot } from "../types";

export type BottomTab = "findings" | "bom" | "log";

interface Props {
  tab: BottomTab;
  onTab: (t: BottomTab) => void;
  snapshot: Snapshot;
  results: Partial<Record<Check, CheckResult>>;
  events: EngineEvent[];
  onProbe: (id: Id) => void;
  onAsk: (text: string, target: Id | null) => void;
  onClose: () => void;
  height?: number;
  onNavigate?: (f: Finding, check: Check) => void;
  activeFinding?: string | null;
}

export interface BomLine {
  key: string;
  value: string;
  component: string;
  refdes: string[];
}

/** Group symbols by component + value. Supplier part numbers are not part of this view. */
export function bomLines(s: Snapshot): BomLine[] {
  const map = new Map<string, BomLine>();
  for (const sym of Object.values(s.project.schematic.symbols)) {
    const key = `${sym.component_id}|${sym.value}`;
    const line = map.get(key) ?? { key, value: sym.value, component: sym.component_id, refdes: [] };
    line.refdes.push(sym.refdes);
    map.set(key, line);
  }
  return [...map.values()]
    .map((l) => ({ ...l, refdes: l.refdes.sort((a, b) => a.localeCompare(b, undefined, { numeric: true })) }))
    .sort((a, b) => a.refdes[0].localeCompare(b.refdes[0], undefined, { numeric: true }));
}

const TABS: [BottomTab, string][] = [
  ["findings", "Findings"],
  ["bom", "BOM"],
  ["log", "Log"],
];

export function BottomPanel({ tab, onTab, snapshot, results, events, onProbe, onAsk, onClose, height, onNavigate, activeFinding }: Props) {
  const bom = bomLines(snapshot);
  return (
    <section className="bottom" aria-label="Bottom panel" style={height ? { height } : undefined}>
      <div className="tabs" role="tablist">
        {TABS.map(([id, label]) => (
          <button key={id} type="button" role="tab" aria-selected={tab === id} className={tab === id ? "tab on" : "tab"} onClick={() => onTab(id)}>
            {label}
          </button>
        ))}
        <span className="spacer" />
        <button type="button" className="icon" aria-label="Hide bottom panel" title="Hide panel (Ctrl+J)" onClick={onClose}>×</button>
      </div>
      <div className="bottom-body">
        {tab === "findings" && <EngineeringMonitor snapshot={snapshot} results={results} onProbe={onProbe} onAsk={onAsk} onNavigate={onNavigate} activeFinding={activeFinding} />}
        {tab === "bom" &&
          (bom.length === 0 ? (
            <p className="empty-state">The BOM is empty: place parts in the schematic first.</p>
          ) : (
            <>
              <table className="bom" aria-label="Bill of materials">
                <thead><tr><th>Qty</th><th>Reference</th><th>Value</th><th>Part</th></tr></thead>
                <tbody>
                  {bom.map((l) => (
                    <tr key={l.key}>
                      <td>{l.refdes.length}</td>
                      <td>{l.refdes.join(", ")}</td>
                      <td>{l.value}</td>
                      <td className="muted">{l.component}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <p className="muted hint">Derived from schematic symbols at revision {snapshot.project.revision}. Supplier part numbers and pricing are not included here.</p>
            </>
          ))}
        {tab === "log" && (
          <section className="activity" aria-label="Activity">
            {events.length === 0 ? (
              <p className="empty-state">No engine events yet.</p>
            ) : (
              <ol>
                {events.slice().reverse().map((e) => (
                  <li key={e.seq}>
                    <span className="muted">r{e.revision}</span> {e.summary} <span className="muted">({e.actor})</span>
                  </li>
                ))}
              </ol>
            )}
          </section>
        )}
      </div>
    </section>
  );
}
