// Proposal review (slice S9, ADR-0051): the rows and selection rules of the review dialog for an
// agent proposal, built from EAK's review (`proposal_review`, recomputed by the engine on a copy
// every time). A proposal is a list of typed commands (steps); Apply sends the chosen steps as one
// Batch, so a step is the smallest unit a person can leave out. Every change a step makes is a row,
// grouped by the object it touches; checking a row checks its step. Pure functions: the IPC calls
// live in the dialog.
import type { EngineChange } from "./syncReport";

export type ProposalStatus = "open" | "ready" | "applied" | "discarded";
export type EvidenceLabel = "verified" | "passed_deterministic_checks" | "simulation_supported" | "datasheet_supported" | "assumed" | "unverified";

/** One proposal in the list (`proposal::summary`). */
export interface ProposalSummary {
  id: string;
  title: string;
  status: ProposalStatus;
  agent: string;
  session_id: string;
  base_revision: number;
  /** Reviewable but made at another revision than the project's: it can no longer be applied. */
  stale: boolean;
  steps: number;
  created_ms: number;
  updated_ms: number;
}

export interface StepEffect {
  index: number;
  /** Harness tool the agent called (a command name, or design_apply). */
  tool: string;
  /** Command type, e.g. PlaceSymbol. */
  command: string;
  class: string;
  /** What the step changes on the copy, after the steps before it. */
  changes: EngineChange[];
  warnings: string[];
  created: string[];
  /** Earlier steps whose objects this step names: it cannot be applied without them. */
  requires: number[];
}

export interface CheckCounts {
  errors: number;
  warnings: number;
  unknown_rules: number;
}

export interface Finding {
  id: string;
  rule_id: string;
  severity: "info" | "warning" | "error";
  message: string;
}

export interface CheckDelta {
  check: string;
  before: CheckCounts;
  after: CheckCounts;
  appeared: Finding[];
  resolved: Finding[];
  unchanged: number;
  newly_unknown: string[];
  not_implemented: string[];
  limitations: string[];
  label: EvidenceLabel;
}

export interface Evidence {
  subject: string;
  label: EvidenceLabel;
  basis: string;
}

export interface ProposalRecord {
  id: string;
  title: string;
  rationale: string;
  agent: string;
  session_id: string;
  base_revision: number;
  status: ProposalStatus;
}

/** EAK's review of a selection (`proposal::Review`). */
export interface ProposalReview {
  proposal: ProposalRecord;
  current_revision: number;
  stale: boolean;
  selected: number[];
  steps: StepEffect[];
  changes: EngineChange[];
  checks: CheckDelta[];
  evidence: Evidence[];
  /** Why the selection cannot be applied as it stands. */
  problem: string | null;
  note: string;
}

export interface ApplyOutcome {
  response: { accepted: boolean; revision_before: number; revision_after: number; command: string; warnings: string[] };
  checkpoint: string;
  proposal: ProposalRecord;
}

/** What a change is about, for grouping (part, net, footprint, rule, …). */
export type ObjectGroup = "Part" | "Net" | "Footprint" | "Rule" | "Routing" | "Board" | "Sheet" | "Project" | "Other";
export const GROUP_ORDER: readonly ObjectGroup[] = ["Part", "Net", "Footprint", "Rule", "Routing", "Board", "Sheet", "Project", "Other"];
export type ProposalGroupBy = "object" | "step";

export interface ProposalRow {
  /** Stable within one review. */
  id: string;
  step: number;
  action: "Add" | "Delete" | "Update";
  group: ObjectGroup;
  /** The kind of object inside its group (Symbol, Library part, Net label, Design rules…). */
  object: string;
  name: string;
  before: string;
  after: string;
}

const NONE = "—";
// IR collection or section → (group, object kind). Paths are the engine's delta paths (delta.rs).
const KINDS: Record<string, [ObjectGroup, string]> = {
  library: ["Part", "Library part"],
  "schematic.symbols": ["Part", "Symbol"],
  "schematic.wires": ["Net", "Wire"],
  "schematic.labels": ["Net", "Net label"],
  "schematic.no_connects": ["Net", "No-connect"],
  "schematic.buses": ["Net", "Bus"],
  "schematic.bus_entries": ["Net", "Bus entry"],
  "pcb.footprints": ["Footprint", "Footprint"],
  constraints: ["Rule", "Design rules"],
  "pcb.routing": ["Rule", "Routing rules"],
  manufacturing: ["Rule", "Manufacturing"],
  "pcb.tracks": ["Routing", "Track"],
  "pcb.vias": ["Routing", "Via"],
  "pcb.zones": ["Routing", "Zone"],
  "pcb.board": ["Board", "Board"],
  "pcb.regions": ["Board", "Region"],
  "pcb.cutouts": ["Board", "Cutout"],
  "pcb.texts": ["Board", "Text"],
  "pcb.dimensions": ["Board", "Dimension"],
  "schematic.sheets": ["Sheet", "Sheet"],
  "schematic.graphics": ["Sheet", "Graphic"],
  "schematic.groups": ["Sheet", "Group"],
  name: ["Project", "Name"],
  requirements: ["Project", "Requirements"],
  properties: ["Project", "Properties"],
};

const obj = (v: unknown): Record<string, unknown> | null => (v && typeof v === "object" && !Array.isArray(v) ? (v as Record<string, unknown>) : null);
const str = (v: unknown): string | null => (typeof v === "string" && v ? v : null);
const short = (v: unknown): string => {
  if (v === null || v === undefined) return NONE;
  if (typeof v === "string") return v;
  if (typeof v === "number" || typeof v === "boolean") return String(v);
  const s = JSON.stringify(v);
  return s.length > 40 ? `${s.slice(0, 39)}…` : s;
};

/** The name a person knows the object by: a reference, a net name, a part id, or the key. */
function nameOf(c: EngineChange, group: ObjectGroup, object: string): string {
  const v = obj(c.after) ?? obj(c.before);
  const named = v && (str(v.refdes) ?? str(v.name) ?? str(v.net) ?? str(v.id));
  if (named) return named;
  if (c.key) return c.key;
  return group === "Other" ? c.path.join(".") : object;
}

/** Before/after text: what was added or deleted, or the fields an update changes. */
function sides(c: EngineChange): [string, string] {
  const b = obj(c.before);
  const a = obj(c.after);
  if (c.before == null) return [NONE, a ? short(a.component_id ?? a.value ?? a.name ?? "added") : short(c.after)];
  if (c.after == null) return [b ? short(b.component_id ?? b.value ?? b.name ?? "deleted") : short(c.before), NONE];
  if (!a || !b) return [short(c.before), short(c.after)];
  const keys = [...new Set([...Object.keys(b), ...Object.keys(a)])].filter((k) => JSON.stringify(b[k]) !== JSON.stringify(a[k]));
  const text = (o: Record<string, unknown>) => keys.map((k) => `${k} ${short(o[k])}`).join(", ") || NONE;
  return [text(b), text(a)];
}

/** One row per change of every step, in step order (all steps, selected or not). */
export function buildProposalRows(steps: StepEffect[]): ProposalRow[] {
  const rows: ProposalRow[] = [];
  for (const s of steps) {
    s.changes.forEach((c, i) => {
      const [group, object] = KINDS[c.path.join(".")] ?? (["Other", c.path.join(".")] as [ObjectGroup, string]);
      const [before, after] = sides(c);
      rows.push({
        id: `s${s.index}:${c.path.join(".")}:${c.key ?? ""}:${i}`,
        step: s.index,
        action: c.before == null ? "Add" : c.after == null ? "Delete" : "Update",
        group,
        object,
        name: nameOf(c, group, object),
        before,
        after,
      });
    });
  }
  return rows;
}

/** Group rows by object (in GROUP_ORDER) or by step, keeping the row order inside a group. */
export function groupProposalRows(rows: ProposalRow[], steps: StepEffect[], by: ProposalGroupBy): { group: string; rows: ProposalRow[] }[] {
  if (by === "step") {
    return steps
      .map((s) => ({ group: `Step ${s.index + 1}: ${s.command}`, rows: rows.filter((r) => r.step === s.index) }))
      .filter((g) => g.rows.length > 0);
  }
  return GROUP_ORDER.map((g) => ({ group: g as string, rows: rows.filter((r) => r.group === g) })).filter((g) => g.rows.length > 0);
}

/**
 * Check or uncheck one step, keeping the selection applicable: checking a step checks every step
 * it requires (transitively); unchecking it unchecks every step that requires it (transitively).
 */
export function toggleStep(steps: StepEffect[], selected: ReadonlySet<number>, index: number, on: boolean): Set<number> {
  const next = new Set(selected);
  const seen = new Set<number>();
  const todo = [index];
  while (todo.length) {
    const i = todo.pop()!;
    if (seen.has(i)) continue;
    seen.add(i);
    if (on) {
      next.add(i);
      todo.push(...(steps.find((s) => s.index === i)?.requires ?? []));
    } else {
      next.delete(i);
      todo.push(...steps.filter((s) => s.requires.includes(i)).map((s) => s.index));
    }
  }
  return next;
}

/** The selection as the IPC takes it: null for every step, otherwise the indices in order. */
export function selectionArg(steps: StepEffect[], selected: ReadonlySet<number>): number[] | null {
  if (steps.length > 0 && steps.every((s) => selected.has(s.index))) return null;
  return [...selected].sort((a, b) => a - b);
}

export interface ProposalApply {
  ok: boolean;
  /** Why Apply is refused, or null. */
  reason: string | null;
  /** Steps Apply sends (null: all). */
  steps: number[] | null;
  count: number;
}

const sameSelection = (a: readonly number[], b: readonly number[]) => a.length === b.length && a.every((x, i) => x === b[i]);

/**
 * Whether Apply may run for the checked steps. What is on screen must be EAK's review of exactly
 * this selection (the dialog re-reviews after every change), and EAK must have found no problem
 * with it (stale revision, a step failing on the copy, a dropped dependency).
 */
export function proposalApplyState(review: ProposalReview | null, selected: ReadonlySet<number>, reviewing: boolean): ProposalApply {
  const steps = review ? selectionArg(review.steps, selected) : null;
  const chosen = [...selected].sort((a, b) => a - b);
  const count = chosen.length;
  const refuse = (reason: string): ProposalApply => ({ ok: false, reason, steps, count });
  if (!review) return refuse("Loading EAK's review…");
  if (review.proposal.status === "applied") return refuse("This proposal was already applied.");
  if (review.proposal.status === "discarded") return refuse("This proposal was discarded.");
  if (review.stale) return refuse(review.problem ?? "The project moved on since the proposal was made: it can no longer be applied.");
  if (review.steps.length === 0) return refuse(review.problem ?? "The proposal has no steps.");
  if (count === 0) return refuse("No step checked: check the changes to apply, or close.");
  if (reviewing || !sameSelection(review.selected, chosen)) return refuse("EAK is reviewing the checked steps…");
  if (review.problem) return refuse(review.problem);
  return { ok: true, reason: null, steps, count };
}

/** Plain words for an evidence label (ADR-0020); never stronger than the label. */
export const LABEL_TEXT: Record<EvidenceLabel, string> = {
  verified: "verified",
  passed_deterministic_checks: "passed deterministic checks",
  simulation_supported: "simulation supported",
  datasheet_supported: "datasheet supported",
  assumed: "assumed (agent's claim)",
  unverified: "unverified",
};

/** "3 → 5", or "3" when unchanged. */
export const countChange = (before: number, after: number): string => (before === after ? String(after) : `${before} → ${after}`);
