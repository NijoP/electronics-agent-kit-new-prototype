// Problems list (report §10 S3): one list of what stands between the design on screen and a
// release, gathered from real reports only:
//  * ERC, DRC and DFM findings of the session's reports (a stale report's rows are marked stale),
//  * rules a report could not evaluate (UNKNOWN), each naming the missing input the engine gave,
//  * the release gate's blocking reasons for this revision (release_check, read-only),
//  * agent tasks the harness recorded as unfinished (ADR-0036 ledger).
// Each row says where "Go to" leads; findings use the F7 navigation of the Findings tab.
// Pure: the panel and its vitest share it.
import { PHASE_LABEL, type AgentTask, type TaskPhase } from "../agentTasks";
import { inSchematic, probeTarget } from "../EngineeringMonitor";
import type { Check, CheckResult, Finding, ReleaseCheck, Snapshot } from "../types";

export type ProblemSource = "erc" | "drc" | "dfm" | "unknown" | "release" | "agent";
export type ProblemSeverity = "error" | "warning" | "info";

export type ProblemGo =
  | { kind: "finding"; check: Check; finding: Finding }
  /** The Design rules dialog: net voltages and currents, height limit, rule configuration. */
  | { kind: "rules" }
  /** The Assets view: 3D models and datasheets of the parts. */
  | { kind: "assets" }
  | { kind: "release" }
  | { kind: "editor"; editor: "schematic" | "pcb" };

export interface Problem {
  key: string;
  source: ProblemSource;
  severity: ProblemSeverity;
  /** Rule id, gate check or agent name. */
  code: string | null;
  message: string;
  /** Missing input, suggested fix, phase or staleness. */
  detail: string | null;
  /** From a report older than the revision on screen. */
  stale: boolean;
  go: ProblemGo | null;
  /** Why there is nowhere to go, when `go` is null. */
  noGo: string | null;
}

export const SOURCE_LABEL: Record<ProblemSource, string> = {
  erc: "ERC",
  drc: "DRC",
  dfm: "DFM",
  unknown: "Unknown rules",
  release: "Release blockers",
  agent: "Agent tasks",
};
export const SOURCES: ProblemSource[] = ["erc", "drc", "dfm", "unknown", "release", "agent"];
export const SEVERITIES: ProblemSeverity[] = ["error", "warning", "info"];
const CHECKS: Check[] = ["erc", "drc", "dfm"];

const SCHEMATIC_PHASES: TaskPhase[] = ["planning", "generating_schematic", "schematic_committed", "running_erc", "schematic_approved"];
const NOT_WORKING: TaskPhase[] = ["blocked", "partial", "failed", "cancelled", "completed"];

/** Where an agent task works: the editor of the furthest working phase it reached. */
export function taskEditor(t: AgentTask): "schematic" | "pcb" {
  const working = [...t.history.map((h) => h.phase), t.phase].filter((p) => !NOT_WORKING.includes(p));
  const last = working[working.length - 1];
  return !last || SCHEMATIC_PHASES.includes(last) ? "schematic" : "pcb";
}

/** Unfinished: still open in the ledger, or ended as partial (work left undone). */
export const unfinished = (t: AgentTask) => t.ended_ms == null || t.phase === "partial";

const UNKNOWN_LINE = /^([A-Z]+): (\S+) is UNKNOWN: (.*)$/;
const ERRORS_LINE = /^([A-Z]+) reports \d+ error\(s\)$/;

/** Go-to for a rule the engine could not evaluate: the input it names is set in Design rules,
 *  except missing 3D body data, which is attached in the Assets view. */
const unknownGo = (why: string): ProblemGo => (/3D body/i.test(why) ? { kind: "assets" } : { kind: "rules" });

export interface ProblemFacts {
  snapshot: Snapshot;
  results: Partial<Record<Check, CheckResult>>;
  release: ReleaseCheck | null;
  agentTasks: AgentTask[];
}

/** Every problem, errors first, in source order within a severity (pure). */
export function collectProblems({ snapshot, results, release, agentTasks }: ProblemFacts): Problem[] {
  const rev = snapshot.project.revision;
  const out: Problem[] = [];
  const unknownSeen = new Set<string>();
  const currentChecks = new Set<string>();

  for (const c of CHECKS) {
    const r = results[c]?.report;
    if (!r) continue;
    const stale = r.revision !== rev;
    if (!stale) currentChecks.add(r.check);
    const staleNote = stale ? `From the ${r.check} report of r${r.revision}; the project is at r${rev}: re-run ${r.check}` : null;
    r.findings.forEach((f, i) => {
      const target = probeTarget(snapshot, f.objects);
      const placed = c === "erc" && f.objects.some((id) => inSchematic(snapshot, id));
      const canGo = !!target || !!f.location || placed;
      out.push({
        key: `${c}|${f.id ?? `${f.rule_id}|${f.message}`}|${i}`,
        source: c,
        severity: f.severity,
        code: f.rule_id,
        message: f.message,
        detail: [f.suggested_fix ? `Suggested: ${f.suggested_fix}` : null, staleNote].filter(Boolean).join(" · ") || null,
        stale,
        go: canGo ? { kind: "finding", check: c, finding: f } : null,
        noGo: canGo ? null : "This finding names no object or location in the current revision",
      });
    });
    for (const x of r.rule_results ?? []) {
      if (x.state !== "UNKNOWN" && x.default_state !== "UNKNOWN") continue;
      const why = x.reason || "not evaluated";
      unknownSeen.add(`${r.check}|${x.rule_id}`);
      out.push({
        key: `unknown|${r.check}|${x.rule_id}`,
        source: "unknown",
        severity: "warning",
        code: x.rule_id,
        message: `${r.check} ${x.rule_id} is UNKNOWN, never passing`,
        detail: [`Missing input: ${why}`, staleNote].filter(Boolean).join(" · "),
        stale,
        go: unknownGo(why),
        noGo: null,
      });
    }
  }

  // The release gate of this revision. Its per-rule UNKNOWN lines join the Unknown rules (once),
  // and "<check> reports N error(s)" is left out where that check's findings are listed above.
  if (release && release.revision === rev) {
    release.blocking.forEach((b, i) => {
      const u = UNKNOWN_LINE.exec(b);
      if (u) {
        const [, check, rule, why] = u;
        if (unknownSeen.has(`${check}|${rule}`)) return;
        unknownSeen.add(`${check}|${rule}`);
        out.push({
          key: `unknown|${check}|${rule}`,
          source: "unknown",
          severity: "warning",
          code: rule,
          message: `${check} ${rule} is UNKNOWN, never passing`,
          detail: `Missing input: ${why}`,
          stale: false,
          go: unknownGo(why),
          noGo: null,
        });
        return;
      }
      const e = ERRORS_LINE.exec(b);
      if (e && currentChecks.has(e[1])) return;
      out.push({ key: `release|${i}|${b}`, source: "release", severity: "error", code: null, message: b, detail: `Keeps every output of r${rev} DRAFT`, stale: false, go: { kind: "release" }, noGo: null });
    });
  }

  for (const t of agentTasks.filter(unfinished)) {
    const behind = t.revision < rev ? ` · last saw r${t.revision}` : "";
    out.push({
      key: `agent|${t.id}`,
      source: "agent",
      severity: t.phase === "blocked" ? "error" : t.phase === "partial" ? "warning" : "info",
      code: t.agent,
      message: t.title,
      detail: `${PHASE_LABEL[t.phase]}${t.ended_ms == null ? " (unfinished)" : ""}${behind}${t.detail ? ` · ${t.detail}` : ""}`,
      stale: false,
      go: { kind: "editor", editor: taskEditor(t) },
      noGo: null,
    });
  }

  const rank: Record<ProblemSeverity, number> = { error: 0, warning: 1, info: 2 };
  return out
    .map((p, i) => ({ p, i }))
    .sort((a, b) => rank[a.p.severity] - rank[b.p.severity] || a.i - b.i)
    .map(({ p }) => p);
}

export interface ProblemFilter {
  hiddenSources: ProblemSource[];
  hiddenSeverities: ProblemSeverity[];
  text: string;
}

export const NO_FILTER: ProblemFilter = { hiddenSources: [], hiddenSeverities: [], text: "" };

/** The rows a filter keeps: source and severity chips, and case-insensitive text over the row. */
export function filterProblems(list: Problem[], f: ProblemFilter): Problem[] {
  const q = f.text.trim().toLowerCase();
  return list.filter(
    (p) =>
      !f.hiddenSources.includes(p.source) &&
      !f.hiddenSeverities.includes(p.severity) &&
      (!q || [p.message, p.code ?? "", p.detail ?? "", SOURCE_LABEL[p.source]].some((s) => s.toLowerCase().includes(q))),
  );
}

/** Rows per source and per severity, for the chip counts. */
export function problemCounts(list: Problem[]) {
  const source = Object.fromEntries(SOURCES.map((s) => [s, 0])) as Record<ProblemSource, number>;
  const severity: Record<ProblemSeverity, number> = { error: 0, warning: 0, info: 0 };
  for (const p of list) {
    source[p.source] += 1;
    severity[p.severity] += 1;
  }
  return { source, severity };
}

/** Checks with no report for the revision on screen: their problems are unknown, not absent. */
export function uncheckedChecks(results: Partial<Record<Check, CheckResult>>, revision: number): string[] {
  return CHECKS.filter((c) => results[c]?.report.revision !== revision).map((c) => c.toUpperCase());
}
