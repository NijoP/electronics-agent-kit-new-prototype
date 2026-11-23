// Engineering Monitor: ERC/DRC findings exactly as eak-validation reported them, with
// the report's revision, what was not checked, and its limitations. Never shows "pass".
// Phase 1 B5: severity filter chips with counts, export of the engine's report (CSV/JSON) and
// clickable object references on every finding.
import { useState } from "react";
import type { Check, CheckResult, Finding, Id, Severity, Snapshot } from "./types";

export type FindingsFormat = "csv" | "json";

interface Props {
  snapshot: Snapshot;
  results: Partial<Record<Check, CheckResult>>;
  onProbe: (symbolId: Id) => void;
  /** Click a finding: open the right editor, zoom to it, highlight it (preferred over onProbe). */
  onNavigate?: (finding: Finding, check: Check) => void;
  /** Click one object a finding names: select it and zoom to it in its editor. */
  onObject?: (finding: Finding, check: Check, id: Id) => void;
  /** Id of the finding last navigated to; its measured/required detail is expanded. */
  activeFinding?: string | null;
  /** Re-run the check on the current revision and write its report into the project's output folder. */
  onExport?: (check: Check, format: FindingsFormat) => void;
  /** Why exporting is unavailable right now (no project folder, an operation running); null when it is available. */
  exportBlocked?: string | null;
}

export const SEVERITIES: Severity[] = ["error", "warning", "info"];
const SEVERITY_LABEL: Record<Severity, string> = { error: "Errors", warning: "Warnings", info: "Info" };

/** Findings per severity, for the filter chips. */
export function severityCounts(findings: Finding[]): Record<Severity, number> {
  const n: Record<Severity, number> = { error: 0, warning: 0, info: 0 };
  for (const f of findings) n[f.severity] += 1;
  return n;
}

/**
 * A readable name for one object a finding names (its canonical EAK-IR id), and whether it is
 * still in the current revision. Ids the snapshot does not hold (a net name, an object deleted
 * since the report ran) are shown as they are and cannot be navigated to.
 */
export function objectRef(s: Snapshot, id: Id): { label: string; present: boolean } {
  const { schematic: sch, pcb } = s.project;
  const sym = sch.symbols[id];
  if (sym) return { label: sym.refdes || id, present: true };
  const fp = pcb.footprints[id];
  if (fp) return { label: fp.refdes || id, present: true };
  const label = sch.labels[id];
  if (label) return { label: `Label ${label.name}`, present: true };
  if (sch.wires[id]) return { label: "Wire", present: true };
  const bus = sch.buses?.[id];
  if (bus) return { label: `Bus ${bus.name}`, present: true };
  if (sch.bus_entries?.[id]) return { label: "Bus entry", present: true };
  if (sch.no_connects[id]) return { label: "No-connect", present: true };
  const t = pcb.tracks[id];
  if (t) return { label: t.net ? `Track ${t.net}` : "Track", present: true };
  const v = pcb.vias[id];
  if (v) return { label: "Via", present: true };
  const z = pcb.zones[id];
  if (z) return { label: z.net ? `Zone ${z.net}` : "Zone", present: true };
  return { label: id, present: false };
}

/** Map a finding's canonical object ids to the symbol id used for cross-probing. */
export function probeTarget(snapshot: Snapshot, objects: Id[]): Id | null {
  const { schematic, pcb } = snapshot.project;
  for (const id of objects) {
    if (schematic.symbols[id]) return id;
    const fp = pcb.footprints[id];
    if (fp) return fp.symbol_id;
  }
  return null;
}

export const findingKey = (f: Finding) => f.id || `${f.rule_id}|${f.message}`;

export const bound = (b: "min" | "max") => (b === "min" ? "≥" : "≤");

/** Measured vs required (quantitative rules only), the location, and the suggested fix. */
export function FindingDetail({ f }: { f: Finding }) {
  return (
    <div className="finding-detail" role="group" aria-label="Finding detail">
      {f.measured && f.required ? (
        <p>
          Measured <strong>{f.measured.value} {f.measured.unit}</strong> · required {bound(f.required.bound)} <strong>{f.required.value} {f.required.unit}</strong>
        </p>
      ) : (
        <p className="muted">Not a quantitative rule: no measured value.</p>
      )}
      {f.suggested_fix && <p className="muted">Suggested: {f.suggested_fix}</p>}
    </div>
  );
}

const OUTCOME_TEXT = {
  NO_ERRORS_IN_IMPLEMENTED_CHECKS: "No errors in implemented checks (not proof of a correct design)",
  ERRORS: "Errors found",
} as const;

function ReportSection({
  title,
  result,
  snapshot,
  onProbe,
  onNavigate,
  onObject,
  activeFinding,
  onExport,
  exportBlocked,
}: {
  title: string;
  result: CheckResult | undefined;
  snapshot: Snapshot;
  onProbe: (id: Id) => void;
  onNavigate?: (finding: Finding, check: Check) => void;
  onObject?: (finding: Finding, check: Check, id: Id) => void;
  activeFinding?: string | null;
  onExport?: (check: Check, format: FindingsFormat) => void;
  exportBlocked?: string | null;
}) {
  const [hidden, setHidden] = useState<Severity[]>([]);
  const check = title.toLowerCase() as Check;
  if (!result) {
    return (
      <section aria-label={`${title} report`}>
        <h3>{title}</h3>
        <p className="muted">Not run for this session. Results are unknown, not passing.</p>
      </section>
    );
  }
  const r = result.report;
  const stale = r.revision !== snapshot.project.revision;
  const counts = severityCounts(r.findings);
  const shown = r.findings.filter((f) => !hidden.includes(f.severity));
  return (
    <section aria-label={`${title} report`}>
      <h3>
        {title} · revision {r.revision}
        {stale && <span className="tag warn">stale: project is at revision {snapshot.project.revision}</span>}
      </h3>
      <p>
        {OUTCOME_TEXT[r.outcome]} · {r.errors} error(s), {r.warnings} warning(s)
      </p>
      <p>
        State: <span className={`tag ${r.overall === "PASS" ? "" : "warn"}`}>{stale ? "UNKNOWN (stale)" : (r.overall ?? "UNKNOWN")}</span>
      </p>
      {(r.rule_results ?? []).some((x) => x.state !== "PASS" && x.state !== "NOT_APPLICABLE") && (
        <ul aria-label={`${title} rules not passing`}>
          {(r.rule_results ?? [])
            .filter((x) => x.state !== "PASS" && x.state !== "NOT_APPLICABLE")
            .map((x) => (
              <li key={x.rule_id}>
                <code>{x.rule_id}</code> {x.state}
                {x.reason && <span className="muted"> · {x.reason}</span>}
              </li>
            ))}
        </ul>
      )}
      {result.evidence_path && <p className="muted">Evidence: {result.evidence_path}</p>}
      <div className="findings-bar">
        <div role="group" aria-label={`${title} severity filter`} className="chips">
          {SEVERITIES.map((sev) => (
            <button
              key={sev}
              type="button"
              className={`chip ${sev}`}
              aria-pressed={!hidden.includes(sev)}
              title={hidden.includes(sev) ? `Show ${SEVERITY_LABEL[sev].toLowerCase()}` : `Hide ${SEVERITY_LABEL[sev].toLowerCase()}`}
              onClick={() => setHidden((h) => (h.includes(sev) ? h.filter((x) => x !== sev) : [...h, sev]))}
            >
              {SEVERITY_LABEL[sev]} <span className="count">{counts[sev]}</span>
            </button>
          ))}
        </div>
        {onExport && (
          <div role="group" aria-label={`Export ${title}`} className="chips">
            {(["csv", "json"] as const).map((fmt) => (
              <button
                key={fmt}
                type="button"
                disabled={!!exportBlocked}
                title={exportBlocked ?? `Re-run ${title} on revision ${snapshot.project.revision} and write its report as ${fmt.toUpperCase()} into the project's output folder`}
                onClick={() => onExport(check, fmt)}
              >
                Export {fmt.toUpperCase()}
              </button>
            ))}
          </div>
        )}
      </div>
      {shown.length < r.findings.length && (
        <p className="muted hint">
          {r.findings.length - shown.length} of {r.findings.length} finding(s) hidden by the severity filter.
        </p>
      )}
      <ul className="findings">
        {shown.map((f, i) => {
          const target = probeTarget(snapshot, f.objects);
          return (
            <li key={`${findingKey(f)}|${i}`} className={`finding ${f.severity}`}>
              <button
                type="button"
                disabled={!target && !f.location}
                aria-current={activeFinding !== undefined && activeFinding === findingKey(f) ? "true" : undefined}
                onClick={() => (onNavigate ? onNavigate(f, check) : target && onProbe(target))}
              >
                <span className="sev">{f.severity}</span> <code>{f.rule_id}</code> {f.message}
              </button>
              {onObject && f.objects.length > 0 && (
                <span className="obj-refs" role="group" aria-label="Objects">
                  {[...new Set(f.objects)].map((id) => {
                    const o = objectRef(snapshot, id);
                    return (
                      <button
                        key={id}
                        type="button"
                        className="obj-ref"
                        disabled={!o.present}
                        title={o.present ? `Select and zoom to ${o.label}` : `${id} is not an object in revision ${snapshot.project.revision}`}
                        onClick={() => onObject(f, check, id)}
                      >
                        {o.label}
                      </button>
                    );
                  })}
                </span>
              )}
              {activeFinding !== undefined && activeFinding === findingKey(f) && <FindingDetail f={f} />}
            </li>
          );
        })}
      </ul>
      {r.not_implemented.length > 0 && (
        <details>
          <summary>Not implemented ({r.not_implemented.length})</summary>
          <ul>
            {r.not_implemented.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </details>
      )}
      {r.limitations.length > 0 && (
        <details>
          <summary>Limitations ({r.limitations.length})</summary>
          <ul>
            {r.limitations.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}

export function EngineeringMonitor({ snapshot, results, onProbe, onNavigate, onObject, activeFinding, onExport, exportBlocked }: Props) {
  const common = { snapshot, onProbe, onNavigate, onObject, activeFinding, onExport, exportBlocked };
  return (
    <div className="monitor" aria-label="Engineering Monitor">
      <h2>Engineering Monitor</h2>
      <ReportSection title="ERC" result={results.erc} {...common} />
      <ReportSection title="DRC" result={results.drc} {...common} />
      <ReportSection title="DFM" result={results.dfm} {...common} />
    </div>
  );
}
