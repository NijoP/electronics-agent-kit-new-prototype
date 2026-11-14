// Thin status bar: project, layer, grid, DRC count, model. The role="status" region also
// carries the latest engine message so assistive tech announces command results.
import type { CheckResult } from "../types";

interface Props {
  project: string;
  revision: number;
  layer: string;
  gridMm: number;
  drc: CheckResult | undefined;
  model: string | null;
  selection: string | null;
  busy: boolean;
  message: string | null;
}

export function drcText(drc: CheckResult | undefined, revision: number): string {
  if (!drc) return "DRC not run";
  const r = drc.report;
  return `DRC ${r.errors} err · ${r.warnings} warn${r.revision !== revision ? " (stale)" : ""}`;
}

export function StatusBar({ project, revision, layer, gridMm, drc, model, selection, busy, message }: Props) {
  const stale = drc && drc.report.revision !== revision;
  const sev = drc ? (drc.report.errors > 0 ? "bad" : stale ? "warn" : "") : "";
  return (
    <footer className="status" role="status">
      <span className="item strong">{project}</span>
      <span className="item">Layer {layer}</span>
      <span className="item">Grid {gridMm} mm</span>
      <span className={`item ${sev}`}>{drcText(drc, revision)}</span>
      <span className="item">{model ? `Model ${model}` : "No model"}</span>
      <span className="item">{selection ?? "Nothing selected"}</span>
      <span className="item grow msg">{busy ? "Working…" : message}</span>
    </footer>
  );
}
