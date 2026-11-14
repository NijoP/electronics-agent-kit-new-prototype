// Engineering Monitor: ERC/DRC findings exactly as eak-validation reported them, with
// the report's revision, what was not checked, and its limitations. Never shows "pass".
import type { Check, CheckResult, Id, Snapshot } from "./types";

interface Props {
  snapshot: Snapshot;
  results: Partial<Record<Check, CheckResult>>;
  onProbe: (symbolId: Id) => void;
  /** Inline "Ask AI" on a finding: the message and the part it points at, if any. */
  onAsk?: (text: string, target: Id | null) => void;
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

const OUTCOME_TEXT = {
  NO_ERRORS_IN_IMPLEMENTED_CHECKS: "No errors in implemented checks (not proof of a correct design)",
  ERRORS: "Errors found",
} as const;

function ReportSection({
  title,
  result,
  snapshot,
  onProbe,
  onAsk,
}: {
  title: string;
  result: CheckResult | undefined;
  snapshot: Snapshot;
  onProbe: (id: Id) => void;
  onAsk?: (text: string, target: Id | null) => void;
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
      {result.evidence_path && <p className="muted">Evidence: {result.evidence_path}</p>}
      <ul className="findings">
        {r.findings.map((f, i) => {
          const target = probeTarget(snapshot, f.objects);
          return (
            <li key={i} className={`finding ${f.severity}`}>
              <button type="button" disabled={!target} onClick={() => target && onProbe(target)}>
                <span className="sev">{f.severity}</span> <code>{f.rule_id}</code> {f.message}
              </button>
              {onAsk && (
                <button type="button" className="ask" title="Ask the assistant about this finding" onClick={() => onAsk(`${title} ${f.rule_id}: ${f.message}`, target)}>
                  Ask AI
                </button>
              )}
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

export function EngineeringMonitor({ snapshot, results, onProbe, onAsk }: Props) {
  return (
    <div className="monitor" aria-label="Engineering Monitor">
      <h2>Engineering Monitor</h2>
      <ReportSection title="ERC" result={results.erc} snapshot={snapshot} onProbe={onProbe} onAsk={onAsk} />
      <ReportSection title="DRC" result={results.drc} snapshot={snapshot} onProbe={onProbe} onAsk={onAsk} />
    </div>
  );
}
