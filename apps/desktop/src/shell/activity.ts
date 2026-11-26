// Agent activity (ADR-0053): the timeline of what the project recorded, read through
// `activity_timeline` (crates/eak-harness/src/activity.rs). The engine journal gives committed
// revisions, proposals/journal.jsonl gives proposal events, weft/journal.jsonl gives WEFT
// messages and evidence/ gives saved check reports. Nothing here is invented: every row is one
// recorded item. Pure helpers for grouping by agent and for the ERC/DRC change across a revision.

/** Must match `activity::Item` in crates/eak-harness/src/activity.rs. */
export interface ActivityItem {
  ts_ms: number;
  source: "revision" | "proposal" | "weft" | "check";
  actor: string;
  kind: string;
  summary: string;
  revision_before: number | null;
  revision_after: number | null;
  op_id: string | null;
  proposal_id: string | null;
  task_id: string | null;
  objects: string[];
  errors: number | null;
  warnings: number | null;
}

/** "agent:layout@s1" → "layout (session s1)"; a person's own edits are "You". */
export function actorName(actor: string): string {
  const m = /^agent:([^@]+)@(.+)$/.exec(actor);
  if (m) return `${m[1]} (session ${m[2]})`;
  if (actor.startsWith("human")) return "You";
  if (actor === "system") return "EAK checks";
  return actor || "unknown";
}

/** One group per actor, in order of each actor's first item; items keep timeline order. */
export function groupByActor(items: ActivityItem[]): { actor: string; items: ActivityItem[] }[] {
  const out = new Map<string, ActivityItem[]>();
  for (const it of items) out.set(it.actor, [...(out.get(it.actor) ?? []), it]);
  return [...out].map(([actor, xs]) => ({ actor, items: xs }));
}

/** The saved report of `check` for `revision`, if one was run there. */
function reportAt(items: ActivityItem[], check: string, revision: number | null): ActivityItem | undefined {
  if (revision === null) return undefined;
  return [...items].reverse().find((i) => i.source === "check" && i.kind.toUpperCase() === check && i.revision_after === revision);
}

/**
 * ERC and DRC errors before and after a revision, from the reports saved for those two revisions.
 * A side with no saved report is "not run", never zero.
 */
export function checkDeltas(items: ActivityItem[], it: ActivityItem): { check: string; text: string }[] {
  if (it.source !== "revision") return [];
  return ["ERC", "DRC"].flatMap((check) => {
    const before = reportAt(items, check, it.revision_before);
    const after = reportAt(items, check, it.revision_after);
    if (!before && !after) return [];
    const side = (r?: ActivityItem) => (r ? `${r.errors} error(s)` : "not run");
    return [{ check, text: `${check}: ${side(before)} → ${side(after)}` }];
  });
}

/** The newest committed revision row, the only one Edit › Undo takes back. */
export function latestRevision(items: ActivityItem[], current: number): ActivityItem | null {
  const revs = items.filter((i) => i.source === "revision" && i.revision_after === current);
  return revs.length ? revs[revs.length - 1] : null;
}

/** The first changed part (schematic symbol) or footprint of a revision, to show on the canvas. */
export function firstChanged(it: ActivityItem): { editor: "schematic" | "pcb"; id: string } | null {
  for (const o of it.objects) {
    const [path, key] = [o.slice(0, o.lastIndexOf(":")), o.slice(o.lastIndexOf(":") + 1)];
    if (!key) continue;
    if (path === "schematic.symbols") return { editor: "schematic", id: key };
    if (path === "pcb.footprints") return { editor: "pcb", id: key };
  }
  return null;
}
