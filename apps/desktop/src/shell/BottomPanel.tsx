// Bottom panel: ERC/DRC/DFM findings (diagnostics), a BOM derived from the schematic, simulation,
// the engine log and this session's tasks.
import { useState } from "react";
import { EngineeringMonitor } from "../EngineeringMonitor";
import { SimulationPanel } from "../SimulationPanel";
import type { Check, CheckResult, EngineEvent, Finding, Id, Snapshot } from "../types";
import type { Task } from "./tasks";

export type BottomTab = "findings" | "bom" | "sim" | "log" | "tasks" | (string & {});

interface Props {
  tab: BottomTab;
  onTab: (t: BottomTab) => void;
  snapshot: Snapshot;
  results: Partial<Record<Check, CheckResult>>;
  events: EngineEvent[];
  onProbe: (id: Id) => void;
  onClose: () => void;
  height?: number;
  onNavigate?: (f: Finding, check: Check) => void;
  activeFinding?: string | null;
  tasks?: Task[];
  onClearTasks?: () => void;
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

export interface SearchHit {
  id: Id;
  kind: "symbol" | "footprint";
  label: string;
  detail: string;
}

/** Case-insensitive search over the real symbols and footprints (refdes, value, component). */
export function searchDesign(s: Snapshot, query: string): SearchHit[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const hit = (...f: string[]) => f.some((x) => x.toLowerCase().includes(q));
  const out: SearchHit[] = [];
  for (const y of Object.values(s.project.schematic.symbols)) {
    if (hit(y.refdes, y.value, y.component_id)) out.push({ id: y.id, kind: "symbol", label: y.refdes, detail: `${y.value} · ${y.component_id}` });
  }
  for (const f of Object.values(s.project.pcb.footprints)) {
    if (hit(f.refdes, f.component_id)) out.push({ id: f.symbol_id, kind: "footprint", label: f.refdes, detail: `footprint · ${f.component_id}` });
  }
  return out.sort((a, b) => a.label.localeCompare(b.label, undefined, { numeric: true }));
}

/** Tabs that have no backend yet are shown disabled with the reason (never decorative). */
const DISABLED: Partial<Record<string, string>> = {
  netlist: "Netlist view needs an engine netlist IPC that is not exposed to the desktop yet",
};

const TABS: [BottomTab, string][] = [
  ["findings", "Findings"],
  ["netlist", "Netlist"],
  ["bom", "BOM"],
  ["search", "Search"],
  ["sim", "Simulation"],
  ["log", "Log"],
  ["tasks", "Tasks"],
];

function TasksList({ tasks, onClear }: { tasks: Task[]; onClear?: () => void }) {
  if (tasks.length === 0) return <p className="empty-state">No tasks yet: checks, Update PCB and exports you run appear here with their outcome.</p>;
  const secs = (t: Task) => (t.endedAt ? `${((t.endedAt - t.startedAt) / 1000).toFixed(1)} s` : "running");
  return (
    <section className="tasks" aria-label="Tasks">
      <ol>
        {tasks.slice().reverse().map((t) => (
          <li key={t.id} className={`task ${t.status}`}>
            <span className="task-state" aria-label={t.status}>{t.status === "running" ? "…" : t.status === "done" ? "✓" : "✗"}</span> <strong>{t.label}</strong>{" "}
            <span className="muted">{secs(t)}</span>
            {t.detail && <div className="muted">{t.detail}</div>}
          </li>
        ))}
      </ol>
      {onClear && tasks.some((t) => t.status !== "running") && (
        <button type="button" onClick={onClear} title="Remove finished tasks from the list">Clear finished</button>
      )}
    </section>
  );
}

function SearchTab({ snapshot, onProbe }: { snapshot: Snapshot; onProbe: (id: Id) => void }) {
  const [q, setQ] = useState("");
  const hits = searchDesign(snapshot, q);
  return (
    <section className="search" aria-label="Search design">
      <input type="search" value={q} placeholder="Search reference, value or component" aria-label="Search query" onChange={(e) => setQ(e.target.value)} />
      {q.trim() && hits.length === 0 && <p className="empty-state">No symbol or footprint matches “{q.trim()}”.</p>}
      <ul>
        {hits.map((h) => (
          <li key={`${h.kind}:${h.id}`}>
            <button type="button" onClick={() => onProbe(h.id)} title="Select in the editor">
              <strong>{h.label}</strong> <span className="muted">{h.detail}</span>
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function BottomPanel({ tab, onTab, snapshot, results, events, onProbe, onClose, height, onNavigate, activeFinding, tasks = [], onClearTasks }: Props) {
  const bom = bomLines(snapshot);
  const running = tasks.filter((t) => t.status === "running").length;
  return (
    <section className="bottom" aria-label="Bottom panel" style={height ? { height } : undefined}>
      <div className="tabs" role="tablist">
        {TABS.map(([id, label]) => (
          <button key={id} type="button" role="tab" aria-selected={tab === id} className={tab === id ? "tab on" : "tab"} disabled={!!DISABLED[id]} title={DISABLED[id]} onClick={() => onTab(id)}>
            {label}
            {id === "tasks" && running > 0 && <span className="count">{running}</span>}
          </button>
        ))}
        <span className="spacer" />
        <button type="button" className="icon" aria-label="Hide bottom panel" title="Hide panel (Ctrl+J)" onClick={onClose}>×</button>
      </div>
      <div className="bottom-body">
        {tab === "findings" && <EngineeringMonitor snapshot={snapshot} results={results} onProbe={onProbe} onNavigate={onNavigate} activeFinding={activeFinding} />}
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
        {tab === "tasks" && <TasksList tasks={tasks} onClear={onClearTasks} />}
        {tab === "search" && <SearchTab snapshot={snapshot} onProbe={onProbe} />}
        {tab === "sim" && <SimulationPanel revision={snapshot.project.revision} />}
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
