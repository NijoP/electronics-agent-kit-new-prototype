// Project Home (slice S1): what Home says about a project, derived only from the facts the
// backend reads from its folder (src-tauri/src/home.rs `HomeFacts`), and the brief Home records
// from the engineer's own text. Pure functions, so every wording rule is unit-tested.
//
// Honesty rules (AGENTS.md): a check with no evidence for the current revision is "not run",
// never passing; UNKNOWN rules are named as such; nothing here says "manufacturing-ready".
import type { Check } from "../types";

/** Must match `HomeFacts` in src-tauri/src/home.rs. */
export interface HomeFacts {
  path: string;
  name: string;
  revision: number;
  phase: string;
  design: {
    sheets: number;
    parts: number;
    nets: number;
    footprints: number;
    missing_footprints: number;
    tracks: number;
    vias: number;
    zones: number;
    outline: boolean;
    bom_lines: number;
    without_supplier: number;
    without_mpn: number;
    requirements: number;
  };
  /** Evidence for the current revision only. */
  checks: { check: string; errors: number; warnings: number; unknown_rules: number }[];
  unrouted_nets: number;
  unrouted_names: string[];
  unrouted_connections: number;
  shorts: number;
  release: { status: "DRAFT" | "INTERNALLY_VERIFIED_RELEASE_CANDIDATE"; required: number; passing: number; blocking: string[] };
}

/** Where a Home item opens the project to: the workspace view that acts on it. */
export type HomeTarget =
  | { kind: "check"; check: Check }
  | { kind: "view"; view: "schematic" | "pcb" }
  | { kind: "release" }
  | { kind: "library" }
  | { kind: "overview" };

export type AttentionTone = "bad" | "warn" | "unknown" | "neutral";

export interface Attention {
  id: string;
  tone: AttentionTone;
  title: string;
  detail: string;
  target: HomeTarget;
  /** Plain words for where the item goes, for its accessible name and tooltip. */
  goes: string;
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** The saved report of `check` for the current revision, if there is one. */
export const evidenceOf = (f: HomeFacts, check: string) => f.checks.find((c) => c.check === check.toUpperCase());

const CHECK_TARGET: Record<string, HomeTarget> = {
  ERC: { kind: "check", check: "erc" },
  DRC: { kind: "check", check: "drc" },
  DFM: { kind: "check", check: "dfm" },
  // MECH has no Findings run of its own in the workspace: the Release check dialog shows it.
  MECH: { kind: "release" },
};

export function goesTo(t: HomeTarget): string {
  switch (t.kind) {
    case "check":
      return `opens the project and runs ${t.check.toUpperCase()} into Findings`;
    case "view":
      return `opens the ${t.view === "pcb" ? "PCB" : "schematic"} editor`;
    case "release":
      return "opens the Release check";
    case "library":
      return "opens the Library";
    case "overview":
      return "opens the project Overview";
  }
}

const item = (id: string, tone: AttentionTone, title: string, detail: string, target: HomeTarget): Attention => ({ id, tone, title, detail, target, goes: goesTo(target) });

/**
 * "Needs attention": the open problems of a project, worst first, each with the place that acts
 * on it. Only facts read from the folder are used; nothing is estimated.
 */
export function attentionItems(f: HomeFacts): Attention[] {
  const out: Attention[] = [];
  const d = f.design;
  const r = `r${f.revision}`;
  if (f.shorts > 0) out.push(item("shorts", "bad", `${plural(f.shorts, "copper short")} on the board`, "Copper of different nets touches.", { kind: "view", view: "pcb" }));
  for (const c of f.checks) {
    if (c.errors > 0) {
      out.push(item(`errors-${c.check}`, "bad", `${c.check}: ${plural(c.errors, "error")}`, `Saved ${c.check} report for ${r}${c.warnings ? `, ${plural(c.warnings, "warning")}` : ""}.`, CHECK_TARGET[c.check] ?? { kind: "release" }));
    }
  }
  if (d.parts > 0 && d.missing_footprints > 0) {
    out.push(item("off-board", "warn", `${plural(d.missing_footprints, "part")} not on the board`, "The board is behind the schematic: run Update PCB.", { kind: "view", view: "schematic" }));
  }
  if (d.parts > 0 && !d.outline) out.push(item("outline", "warn", "No board outline", "Draw the outline before placement and DRC.", { kind: "view", view: "pcb" }));
  if (f.unrouted_nets > 0) {
    const names = f.unrouted_names.join(", ");
    const more = f.unrouted_nets > f.unrouted_names.length ? ` and ${f.unrouted_nets - f.unrouted_names.length} more` : "";
    out.push(item("unrouted", "warn", plural(f.unrouted_nets, "unrouted net"), `${names}${more} (${plural(f.unrouted_connections, "connection")} left).`, { kind: "view", view: "pcb" }));
  }
  for (const c of f.checks) {
    if (c.unknown_rules > 0) {
      out.push(item(`unknown-${c.check}`, "unknown", `${c.check} could not evaluate ${plural(c.unknown_rules, "rule")}`, "UNKNOWN is never a pass: a required input (such as a net voltage or current) is missing.", CHECK_TARGET[c.check] ?? { kind: "release" }));
    }
  }
  if (d.parts > 0 && (d.without_supplier > 0 || d.without_mpn > 0)) {
    const bits = [d.without_supplier > 0 ? `${d.without_supplier} of ${d.parts} parts with no supplier part number` : "", d.without_mpn > 0 ? `${d.without_mpn} with no MPN` : ""].filter(Boolean);
    out.push(item("bom", "warn", "BOM risk", `${bits.join("; ")}: sourcing cannot be checked for them.`, { kind: "library" }));
  }
  if (d.parts > 0) {
    for (const check of ["ERC", "DRC"] as const) {
      if (check === "DRC" && d.footprints === 0) continue;
      if (!evidenceOf(f, check)) out.push(item(`not-run-${check}`, "neutral", `${check} not run on ${r}`, "No saved report for this revision, so its result is unknown.", CHECK_TARGET[check]));
    }
  }
  if (d.requirements === 0) out.push(item("brief", "neutral", "No design brief", "Write down what the board must do: rails, currents, interfaces.", { kind: "overview" }));
  return out;
}

/** Release readiness in words: gates passing out of the required ones, and the package status. */
export function releaseLine(f: HomeFacts): string {
  const status = f.release.status === "DRAFT" ? "Draft" : "Internally verified release candidate (still needs independent review)";
  return `${f.release.passing} of ${f.release.required} release gates pass · ${status}`;
}

/** Errors in current evidence, or null when no check has evidence for this revision. */
export function currentErrors(f: HomeFacts): number | null {
  return f.checks.length === 0 ? null : f.checks.reduce((n, c) => n + c.errors, 0);
}

export const unknownRules = (f: HomeFacts) => f.checks.reduce((n, c) => n + c.unknown_rules, 0);

/** Where "Continue" lands for the project's stage: the editor of its furthest content. */
export function continueTarget(phase: string): HomeTarget {
  return phase === "PCB placement" || phase === "Routing" ? { kind: "view", view: "pcb" } : { kind: "view", view: "schematic" };
}

/** Longest project name Home derives from a prompt. */
const NAME_MAX = 48;

/**
 * The brief Home records for a typed description, without AI: the engineer's own sentences, one
 * per line (a sentence ends at ".", ";", "!" or "?" followed by a space, or at a line break;
 * "3.3 V" is not split), and a project name from its first words. Nothing is inferred or added.
 */
export function briefFromPrompt(text: string): { name: string; lines: string[] } {
  const lines = text
    .split(/\r?\n/)
    .flatMap((l) => l.split(/(?<=[.;!?])\s+/))
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
  const first = (lines[0] ?? "").replace(/[.;:!?,]+$/, "");
  const words = first.split(/\s+/).filter(Boolean);
  let name = "";
  for (const w of words) {
    const next = name ? `${name} ${w}` : w;
    if (next.length > NAME_MAX || next.split(" ").length > 7) break;
    name = next;
  }
  if (!name && words[0]) name = words[0].slice(0, NAME_MAX);
  return { name: name ? name[0].toUpperCase() + name.slice(1) : "", lines };
}
