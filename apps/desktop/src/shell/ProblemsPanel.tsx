// Problems tab of the bottom panel (report §10 S3): one filtered list over the ERC/DRC/DFM
// findings, UNKNOWN rules, release blockers and unfinished agent tasks (problems.ts). A row's
// "Go to" uses the same navigation as the Findings tab (F7) for findings, and opens the view that
// holds the missing input for the others. Checks with no report for this revision are named, so an
// empty list never reads as "passing".
import { useState } from "react";
import type { Check } from "../types";
import {
  filterProblems,
  NO_FILTER,
  problemCounts,
  SEVERITIES,
  SOURCE_LABEL,
  SOURCES,
  type Problem,
  type ProblemFilter,
  type ProblemSeverity,
  type ProblemSource,
} from "./problems";

const SEVERITY_LABEL: Record<ProblemSeverity, string> = { error: "Errors", warning: "Warnings", info: "Info" };

const GO_TITLE: Record<NonNullable<Problem["go"]>["kind"], string> = {
  finding: "Open the editor, select the object and zoom to it",
  rules: "Open Design rules, where this input is declared",
  assets: "Open the Assets view, where 3D models are attached",
  release: "Open the release check of this revision",
  editor: "Open the editor this agent task works in",
};

interface Props {
  problems: Problem[];
  revision: number;
  /** Checks with no report for this revision (problems.ts `uncheckedChecks`). */
  unchecked: string[];
  onGo: (p: Problem) => void;
  onCheck: (c: Check) => void;
  onRelease: () => void;
  busy: boolean;
  activeKey?: string | null;
}

const toggle = <T,>(list: T[], x: T) => (list.includes(x) ? list.filter((y) => y !== x) : [...list, x]);

export function ProblemsPanel({ problems, revision, unchecked, onGo, onCheck, onRelease, busy, activeKey }: Props) {
  const [filter, setFilter] = useState<ProblemFilter>(NO_FILTER);
  const counts = problemCounts(problems);
  const shown = filterProblems(problems, filter);
  const blocked = busy ? "Wait for the running operation to finish" : null;
  return (
    <section className="problems" aria-label="Problems">
      <div className="findings-bar problems-bar">
        <div role="group" aria-label="Problem sources" className="chips">
          {SOURCES.map((s) => {
            const on = !filter.hiddenSources.includes(s);
            return (
              <button
                key={s}
                type="button"
                className={`chip src-${s}`}
                data-source={s}
                aria-pressed={on}
                title={on ? `Hide ${SOURCE_LABEL[s].toLowerCase()}` : `Show ${SOURCE_LABEL[s].toLowerCase()}`}
                onClick={() => setFilter((f) => ({ ...f, hiddenSources: toggle<ProblemSource>(f.hiddenSources, s) }))}
              >
                {SOURCE_LABEL[s]} <span className="count">{counts.source[s]}</span>
              </button>
            );
          })}
        </div>
        <div role="group" aria-label="Problem severity" className="chips">
          {SEVERITIES.map((sev) => {
            const on = !filter.hiddenSeverities.includes(sev);
            return (
              <button
                key={sev}
                type="button"
                className={`chip ${sev}`}
                aria-pressed={on}
                title={on ? `Hide ${SEVERITY_LABEL[sev].toLowerCase()}` : `Show ${SEVERITY_LABEL[sev].toLowerCase()}`}
                onClick={() => setFilter((f) => ({ ...f, hiddenSeverities: toggle<ProblemSeverity>(f.hiddenSeverities, sev) }))}
              >
                {SEVERITY_LABEL[sev]} <span className="count">{counts.severity[sev]}</span>
              </button>
            );
          })}
        </div>
        <input type="search" className="problems-text" aria-label="Filter problems" placeholder="Filter by text, rule or net" value={filter.text} onChange={(e) => setFilter((f) => ({ ...f, text: e.target.value }))} />
        <div role="group" aria-label="Problem checks" className="chips">
          <button type="button" disabled={!!blocked} title={blocked ?? `Run ERC on revision ${revision}`} onClick={() => onCheck("erc")}>
            Run ERC
          </button>
          <button type="button" disabled={!!blocked} title={blocked ?? `Run DRC on revision ${revision}`} onClick={() => onCheck("drc")}>
            Run DRC
          </button>
          <button type="button" title={`Open the read-only release check of revision ${revision}`} onClick={onRelease}>
            Release check…
          </button>
        </div>
      </div>
      {unchecked.length > 0 && (
        <p className="muted hint" data-unchecked={unchecked.join(" ")}>
          Not run for r{revision}: {unchecked.join(", ")}. Their problems are unknown, not absent.
        </p>
      )}
      {shown.length < problems.length && (
        <p className="muted hint">
          {problems.length - shown.length} of {problems.length} problem(s) hidden by the filter.
        </p>
      )}
      {problems.length === 0 ? (
        <p className="empty-state">No problems in the reports of r{revision}. Checks find what they implement; this is not proof of a correct design.</p>
      ) : (
        <ul className="findings problem-list">
          {shown.map((p) => (
            <li key={p.key} className={`finding problem ${p.severity}`} data-source={p.source} data-stale={p.stale ? "true" : undefined}>
              <button
                type="button"
                disabled={!p.go}
                title={p.go ? GO_TITLE[p.go.kind] : (p.noGo ?? undefined)}
                aria-current={activeKey === p.key ? "true" : undefined}
                onClick={() => onGo(p)}
              >
                <span className="sev">{p.severity}</span> <span className="problem-src">{SOURCE_LABEL[p.source]}</span> {p.code && <code>{p.code}</code>} {p.message}
                {p.stale && <span className="tag warn">stale</span>}
              </button>
              {p.detail && <div className="problem-detail muted">{p.detail}</div>}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
