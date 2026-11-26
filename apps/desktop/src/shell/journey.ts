// Journey bar state (report §10 S2): the stages of the design journey in the product vision, each
// coloured only from facts of the revision on screen. A stage is "done" only when a current
// report or a current fact says so; a stale report, a check not run or an UNKNOWN rule leave it
// "open" (never done), and "fail" needs a current report with errors. No agent claim is read.
// Pure: the bar (JourneyBar.tsx) and its vitest use the same mapping.
import type { Check, CheckResult, ConnectivityReport, ReleaseCheck, Snapshot } from "../types";
import { designFacts } from "./ProjectOverview";

export type StageId = "requirements" | "parts" | "schematic" | "erc" | "layout" | "routing" | "drc" | "3d" | "outputs" | "release";
export type StageState = "fail" | "open" | "done";

export interface Stage {
  id: StageId;
  label: string;
  state: StageState;
  /** Why the stage has this state, from the fact it was read from. */
  reason: string;
}

export const STAGE_LABELS: [StageId, string][] = [
  ["requirements", "Requirements"],
  ["parts", "Parts"],
  ["schematic", "Schematic"],
  ["erc", "ERC"],
  ["layout", "Layout"],
  ["routing", "Routing"],
  ["drc", "DRC"],
  ["3d", "3D"],
  ["outputs", "Outputs"],
  ["release", "Release"],
];

export interface JourneyFacts {
  snapshot: Snapshot;
  results: Partial<Record<Check, CheckResult>>;
  conn: ConnectivityReport | null;
  /** The read-only release check (`release_check`); null while unread or when it failed. */
  release: ReleaseCheck | null;
  /** Why the release check is unavailable, when it failed. */
  releaseError?: string | null;
  /** An `output/rev-<N>` folder exists for the current revision; null when not read (yet). */
  outputsCurrent: boolean | null;
}

export const STATE_TEXT: Record<StageState, string> = { fail: "has errors", open: "not done", done: "done" };

const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;

/** ERC / DRC from the session's report: done only from a current report with no error, no
 *  warning and an overall PASS (ADR-0026: no overall state is UNKNOWN). */
export function checkStage(r: CheckResult | undefined, revision: number, name: string): Pick<Stage, "state" | "reason"> {
  if (!r) return { state: "open", reason: `${name} not run for r${revision}` };
  const rep = r.report;
  if (rep.revision !== revision) return { state: "open", reason: `${name} report is for r${rep.revision}, stale: the project is at r${revision}` };
  if (rep.errors > 0 || rep.overall === "FAIL") return { state: "fail", reason: `${name}: ${plural(rep.errors, "error")} at r${revision}` };
  const unknown = (rep.rule_results ?? []).filter((x) => x.state === "UNKNOWN" || x.default_state === "UNKNOWN").map((x) => x.rule_id);
  if (unknown.length > 0) return { state: "open", reason: `${name}: ${unknown.join(", ")} UNKNOWN at r${revision}` };
  if (rep.warnings > 0) return { state: "open", reason: `${name}: ${plural(rep.warnings, "warning")} at r${revision}` };
  if (rep.overall !== "PASS") return { state: "open", reason: `${name} overall state is ${rep.overall ?? "UNKNOWN"} at r${revision}` };
  return { state: "done", reason: `${name}: no errors in implemented checks at r${revision}` };
}

/** The release check, only when it is for the revision on screen. */
function currentRelease(f: JourneyFacts): ReleaseCheck | null {
  const r = f.release;
  return r && r.revision === f.snapshot.project.revision ? r : null;
}

function unreadRelease(f: JourneyFacts): string {
  const rev = f.snapshot.project.revision;
  if (f.releaseError) return `Release check unavailable: ${f.releaseError}`;
  if (f.release) return `Release check is for r${f.release.revision}, stale: the project is at r${rev}`;
  return `Release check not read for r${rev} yet`;
}

/** The stages, in journey order, from the facts of the current revision (pure). */
export function journeyStages(f: JourneyFacts): Stage[] {
  const s = f.snapshot;
  const rev = s.project.revision;
  const d = designFacts(s);
  const live = f.conn && f.conn.revision === rev ? f.conn : null;
  const release = currentRelease(f);
  const reqs = s.project.requirements?.length ?? 0;

  const stage = (id: StageId): Pick<Stage, "state" | "reason"> => {
    switch (id) {
      case "requirements":
        return reqs > 0 ? { state: "done", reason: `${plural(reqs, "requirement")} recorded` } : { state: "open", reason: "No requirements recorded for this project" };
      case "parts":
        if (d.parts === 0) return { state: "open", reason: "No parts placed" };
        if (d.withDistributor < d.parts) return { state: "open", reason: `${d.parts - d.withDistributor} of ${d.parts} parts have no Distributor part` };
        return { state: "done", reason: `All ${d.parts} parts have a Distributor part` };
      case "schematic":
        if (d.parts === 0) return { state: "open", reason: "The schematic has no parts" };
        if (d.nets === 0) return { state: "open", reason: `${plural(d.parts, "part")}, no nets yet` };
        return { state: "done", reason: `${plural(d.parts, "part")}, ${plural(d.nets, "net")}` };
      case "erc":
        return checkStage(f.results.erc, rev, "ERC");
      case "layout":
        if (d.parts === 0 && d.footprints === 0) return { state: "open", reason: "Nothing to place: no parts" };
        if (d.missingFootprints > 0) return { state: "open", reason: `${d.missingFootprints} of ${d.parts} parts not on the board: run Update PCB` };
        if (!d.outline) return { state: "open", reason: "No board outline" };
        return { state: "done", reason: `${plural(d.footprints, "footprint")} on a defined outline` };
      case "routing":
        if (!live) return { state: "open", reason: `Connectivity not analysed for r${rev}` };
        if (live.shorts.length > 0) return { state: "fail", reason: `${plural(live.shorts.length, "short")} at r${rev}` };
        if (d.footprints === 0) return { state: "open", reason: "No footprints on the board" };
        if (live.ratsnest.length > 0) return { state: "open", reason: `${live.ratsnest.length} unrouted connection${live.ratsnest.length === 1 ? "" : "s"} at r${rev}` };
        return { state: "done", reason: `No unrouted connections at r${rev}` };
      case "drc":
        return checkStage(f.results.drc, rev, "DRC");
      case "3d": {
        // The mechanical check (MECH: heights, 3D bodies) of the release gate, for this revision.
        if (!release) return { state: "open", reason: unreadRelease(f) };
        const g = release.gates.find((x) => x.id === "mech");
        if (!g) return { state: "open", reason: "The release check has no mechanical gate" };
        if (g.state === "PASS") return { state: "done", reason: `MECH passes at r${rev}` };
        if (g.state === "FAIL") return { state: "fail", reason: g.reasons[0] ?? `MECH fails at r${rev}` };
        if (g.state === "NOT_RUN") return { state: "open", reason: `MECH not run for r${rev}: Run release checks` };
        return { state: "open", reason: g.reasons[0] ?? `MECH is ${g.state} at r${rev}` };
      }
      case "outputs":
        if (!s.dir) return { state: "open", reason: "Save the project to a folder first: outputs are written into it" };
        if (f.outputsCurrent === null) return { state: "open", reason: `Outputs not read for r${rev} yet` };
        return f.outputsCurrent ? { state: "done", reason: `Outputs exported for r${rev}` } : { state: "open", reason: `No outputs exported for r${rev}` };
      case "release": {
        if (!release) return { state: "open", reason: unreadRelease(f) };
        const required = release.gates.filter((g) => g.required && g.state !== "DISABLED");
        const failing = required.filter((g) => g.state === "FAIL");
        const open = required.filter((g) => g.state !== "PASS");
        if (release.status === "INTERNALLY_VERIFIED_RELEASE_CANDIDATE" && open.length === 0)
          return { state: "done", reason: `Every required gate passes at r${rev} (not a fabrication certificate)` };
        if (failing.length > 0) return { state: "fail", reason: `${failing.length} of ${required.length} required gates fail at r${rev}` };
        return { state: "open", reason: `${open.length} of ${required.length} required gates do not pass at r${rev}` };
      }
    }
  };
  return STAGE_LABELS.map(([id, label]) => ({ id, label, ...stage(id) }));
}

/** The first stage that is not done: the bar's "Next". Null when every stage is done. */
export const nextStage = (stages: Stage[]): Stage | null => stages.find((x) => x.state !== "done") ?? null;
