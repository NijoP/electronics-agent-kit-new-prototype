// Thin status bar: project, layer and grid of the active editor, DRC count. The role="status" region also
// carries the latest engine message so assistive tech announces command results.
import type { CheckResult } from "../types";

interface Props {
  project: string;
  revision: number;
  /** Active copper layer; null unless the PCB editor is the active document. */
  layer: string | null;
  /** Grid of the active editor ("1.27 mm", "25 mil (snap off)"); null outside the editors. */
  grid: string | null;
  drc: CheckResult | undefined;
  selection: string | null;
  /** Revision + agent task phase, the same text the Terminal workspace shows (ADR-0036). */
  task?: { text: string; title: string };
  busy: boolean;
  message: string | null;
}

export function drcText(drc: CheckResult | undefined, revision: number): string {
  if (!drc) return "DRC not run";
  const r = drc.report;
  return `DRC ${r.errors} err · ${r.warnings} warn${r.revision !== revision ? " (stale)" : ""}`;
}

export function StatusBar({ project, revision, layer, grid, drc, selection, task, busy, message }: Props) {
  const stale = drc && drc.report.revision !== revision;
  const sev = drc ? (drc.report.errors > 0 ? "bad" : stale ? "warn" : "") : "";
  return (
    <footer className="status" role="status">
      <span className="item strong">{project}</span>
      {layer && <span className="item">Layer {layer}</span>}
      {grid && <span className="item">Grid {grid}</span>}
      <span className={`item ${sev}`}>{drcText(drc, revision)}</span>
      {task && (
        <span className="item" data-testid="agent-task" title={task.title}>
          {task.text}
        </span>
      )}
      <span className="item">{selection ?? "Nothing selected"}</span>
      <span className="item grow msg">{busy ? "Working…" : message}</span>
    </footer>
  );
}
