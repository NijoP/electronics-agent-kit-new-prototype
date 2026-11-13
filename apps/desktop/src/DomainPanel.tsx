// Domain intelligence: the top DESK domain classes for the open design, with the reasons for
// each score and the DESK maturity label that caps what EAK may say. A classification is a
// hypothesis about the design, never a pass/fail result; what EAK does not check is listed.
import type { DomainReport, Maturity } from "./types";

const LABEL: Record<Maturity, string> = {
  DRAFT: "DRAFT",
  ADVISORY: "ADVISORY",
  IMPLEMENTED: "IMPLEMENTED",
  OFFLINE_TESTED: "OFFLINE TESTED",
  VERIFIED_AGAINST_SOURCE: "VERIFIED AGAINST SOURCE",
  LIVE_VERIFIED: "LIVE VERIFIED",
  NEEDS_HARDWARE_VALIDATION: "NEEDS HARDWARE VALIDATION",
  BLOCKED: "BLOCKED",
};

interface Props {
  report: DomainReport | null;
  /** Revision of the open design; a report for another revision is marked stale. */
  revision: number;
  error?: string | null;
}

export function DomainPanel({ report, revision, error }: Props) {
  if (error) {
    return (
      <section className="domain" aria-label="Domain intelligence">
        <h2>Domain</h2>
        <p role="alert" className="error">
          Domain classification failed: {error}
        </p>
      </section>
    );
  }
  if (!report) {
    return (
      <section className="domain" aria-label="Domain intelligence">
        <h2>Domain</h2>
        <p className="muted">Classifying…</p>
      </section>
    );
  }
  const c = report.classification;
  const stale = report.revision !== revision;
  const top = c.candidates.filter((x) => x.score > 0).slice(0, 5);
  return (
    <section className="domain" aria-label="Domain intelligence">
      <h2>Domain · revision {report.revision}</h2>
      {stale && <p className="muted">Stale: the design is at revision {revision}.</p>}
      <p>
        <strong>
          {c.status === "CLASSIFIED"
            ? "Classified"
            : c.status === "REVIEW_REQUIRED"
              ? "Review required"
              : "Unclassified"}
        </strong>
        {c.primary && <span className="muted"> · confidence {Math.round(c.confidence * 100)}%</span>}
      </p>
      {top.length === 0 ? (
        <p className="muted">
          No domain class scored. Add requirements, a project name or components that describe the design (checked
          against {c.registry_classes} DESK classes).
        </p>
      ) : (
        <ol aria-label="Top domain classes" className="domain-list">
          {top.map((x) => (
            <li key={x.archetype_id}>
              <strong>{x.name}</strong> <span className="muted">({x.domain_name})</span> · score {x.score}{" "}
              <span className="badge" title={x.ceiling.claim}>
                DESK: {LABEL[x.maturity]}
              </span>
              <ul aria-label={`Reasons for ${x.name}`}>
                {x.reasons.slice(0, 4).map((r) => (
                  <li key={r}>{r}</li>
                ))}
                {x.penalties.map((r) => (
                  <li key={r} className="muted">
                    {r}
                  </li>
                ))}
              </ul>
            </li>
          ))}
        </ol>
      )}
      {c.guidance && (
        <details>
          <summary>
            {c.guidance.heading} · {LABEL[c.guidance.ceiling.label]}
          </summary>
          <p className="muted">{c.guidance.ceiling.claim}</p>
          <ul aria-label="Guidance">
            {c.guidance.critical_electrical_constraints.map((g) => (
              <li key={g}>{g}</li>
            ))}
          </ul>
        </details>
      )}
      {c.conflicts.length > 0 && (
        <ul aria-label="Conflicts" className="error">
          {c.conflicts.map((x) => (
            <li key={x}>{x}</li>
          ))}
        </ul>
      )}
      {c.overlays.length > 0 && <p className="muted">Overlays: {c.overlays.join(", ")}</p>}
      {c.missing_information.length > 0 && <p className="muted">Missing: {c.missing_information.join("; ")}.</p>}
      <p className="muted">Not checked by EAK: {c.not_implemented.join("; ")}.</p>
      <p className="muted">{c.limitations.join(" ")}</p>
    </section>
  );
}
