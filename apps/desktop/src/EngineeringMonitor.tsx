// Engineering Monitor: ERC/DRC findings exactly as eak-validation reported them, with
// the report's revision, what was not checked, and its limitations. Never shows "pass".
import type { Check, CheckResult, Finding, Id, Snapshot } from "./types";

interface Props {
  snapshot: Snapshot;
  results: Partial<Record<Check, CheckResult>>;
  onProbe: (symbolId: Id) => void;
  /** Inline "Ask AI" on a finding: the message and the part it points at, if any. */
  /** Click a finding: open the right editor, zoom to it, highlight it (preferred over onProbe). */
  onNavigate?: (finding: Finding, check: Check) => void;
  /** Id of the finding last navigated to; its measured/required detail is expanded. */
  activeFinding?: string | null;
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
  activeFinding,
}: {
  title: string;
  result: CheckResult | undefined;
  snapshot: Snapshot;
  onProbe: (id: Id) => void;
  onNavigate?: (finding: Finding, check: Check) => void;
  activeFinding?: string | null;
}) {
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
      <ul className="findings">
        {r.findings.map((f, i) => {
          const target = probeTarget(snapshot, f.objects);
          return (
            <li key={i} className={`finding ${f.severity}`}>
              <button
                type="button"
                disabled={!target && !f.location}
                aria-current={activeFinding !== undefined && activeFinding === findingKey(f) ? "true" : undefined}
                onClick={() => (onNavigate ? onNavigate(f, title.toLowerCase() as Check) : target && onProbe(target))}
              >
                <span className="sev">{f.severity}</span> <code>{f.rule_id}</code> {f.message}
              </button>
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

export function EngineeringMonitor({ snapshot, results, onProbe, onNavigate, activeFinding }: Props) {
  return (
    <div className="monitor" aria-label="Engineering Monitor">
      <h2>Engineering Monitor</h2>
      <ReportSection title="ERC" result={results.erc} snapshot={snapshot} onProbe={onProbe} onNavigate={onNavigate} activeFinding={activeFinding} />
      <ReportSection title="DRC" result={results.drc} snapshot={snapshot} onProbe={onProbe} onNavigate={onNavigate} activeFinding={activeFinding} />
      <ReportSection title="DFM" result={results.dfm} snapshot={snapshot} onProbe={onProbe} onNavigate={onNavigate} activeFinding={activeFinding} />
    </div>
  );
}
