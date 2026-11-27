// Agents workspace model (docs/product/EAK_AGENT_WORKSPACE_SPEC.md, ADR-0058): pure functions over
// what EAK recorded: the WEFT journal (`weft_state`), the engineer's review queue and conflicts
// (ADR-0053), the AI tools' sessions (ADR-0054) and probes. Nothing here invents a state: a fact the
// journal does not hold (a read receipt, an acceptance criterion) is said to be not recorded.
import { actorName, type EngineerRequest, type WeftConflict } from "../shell/activity";
import type { AgentSession, SessionState, ToolStatus } from "../settings/aiTools";
import type { WeftAgentView, WeftMessageView, WeftTaskStatus, WeftTaskView, WeftView } from "./weft";

export type Selection =
  | { kind: "overview" }
  | { kind: "agent"; id: string }
  | { kind: "task"; id: string }
  | { kind: "session"; id: string }
  | { kind: "runtime"; id: string };

export const OVERVIEW: Selection = { kind: "overview" };

/** Stable key of a selection ("overview", "task:t1", ...). */
export const selectionKey = (s: Selection): string => (s.kind === "overview" ? "overview" : `${s.kind}:${s.id}`);

/** "agent:layout@s1" → "layout (session s1)"; "engineer" → "You (engineer)"; "*" → "Everyone". */
export function who(id: string): string {
  if (id === "engineer") return "You (engineer)";
  if (id === "*") return "Everyone";
  return actorName(id);
}

export const TASK_STATUS_LABEL: Record<WeftTaskStatus, string> = {
  pending: "Pending",
  in_progress: "In progress",
  blocked: "Blocked",
  done: "Done",
  rejected: "Rejected",
};

export type Tone = "neutral" | "info" | "warn" | "ok" | "bad";

export const TASK_TONE: Record<WeftTaskStatus, Tone> = {
  pending: "neutral",
  in_progress: "info",
  blocked: "warn",
  done: "ok",
  rejected: "bad",
};

export const SESSION_TONE: Record<SessionState, Tone> = {
  running: "info",
  completed: "ok",
  failed: "bad",
  stopped: "neutral",
  unknown: "warn",
};

export const MESSAGE_KIND_LABEL: Record<WeftMessageView["message"]["kind"], string> = {
  delegate: "Delegated",
  context_share: "Context",
  status: "Status",
  finding: "Finding",
  review_request: "Review request",
  review_verdict: "Verdict",
  blocker: "Blocker",
};

/** Delivery of one message, from the journal only (WEFT records sends, not reads). */
export interface Delivery {
  /** true / false where a reply is expected and the journal shows it (or not); null otherwise. */
  answered: boolean | null;
  text: string;
}

/**
 * A review request is answered by a verdict on the same proposal from the asked recipient; a
 * delegation by a status or blocker its task's owner sent for that task. Other kinds expect no
 * reply. Reads are not tracked, so "read" is never claimed.
 */
export function delivery(m: WeftMessageView, all: WeftMessageView[]): Delivery {
  const later = all.filter((x) => x.seq > m.seq);
  let answered: boolean | null = null;
  if (m.message.kind === "review_request") {
    answered = later.some((x) => x.message.kind === "review_verdict" && x.message.proposal_id === m.message.proposal_id && x.message.from === m.message.to);
  } else if (m.message.kind === "delegate") {
    answered = later.some((x) => (x.message.kind === "status" || x.message.kind === "blocker") && x.message.task_id === m.message.task_id && x.message.from === m.message.to);
  }
  const reply = answered === null ? "no reply expected" : answered ? "answered" : "awaiting answer";
  return { answered, text: `recorded · ${reply} · read not tracked` };
}

export interface TaskRow {
  task: WeftTaskView;
  depth: number;
}

/** Tasks as a tree: delegated sub-tasks under their parent, each level in creation order. */
export function taskTree(tasks: WeftTaskView[]): TaskRow[] {
  const ids = new Set(tasks.map((t) => t.task.id));
  const byCreated = [...tasks].sort((a, b) => a.created_ms - b.created_ms || a.task.id.localeCompare(b.task.id));
  const out: TaskRow[] = [];
  const seen = new Set<string>();
  const walk = (t: WeftTaskView, depth: number) => {
    if (seen.has(t.task.id)) return;
    seen.add(t.task.id);
    out.push({ task: t, depth });
    for (const c of byCreated) if (c.task.parent === t.task.id) walk(c, depth + 1);
  };
  for (const t of byCreated) if (!t.task.parent || !ids.has(t.task.parent)) walk(t, 0);
  // A parent cycle (WEFT does not make one, but a task is never hidden): what is left goes on top.
  for (const t of byCreated) walk(t, 0);
  return out;
}

/** "created at r3 · project now r5", or "created at r3 · project still at r3". */
export function revisionText(at: number, current: number): string {
  return at === current ? `created at r${at} · project still at r${at}` : `created at r${at} · project now r${current}`;
}

/** One thing that waits on the engineer. */
export interface NeedsYou {
  key: string;
  kind: "review" | "conflict" | "blocked" | "session";
  title: string;
  detail: string;
  select?: Selection;
  proposalId?: string;
}

/** Review requests to the engineer, conflicts, blocked tasks, and sessions that failed or are no longer supervised. */
export function needsYou(view: WeftView | null, queue: EngineerRequest[], conflicts: WeftConflict[], sessions: AgentSession[]): NeedsYou[] {
  const out: NeedsYou[] = [];
  for (const r of queue) {
    out.push({
      key: `review:${r.proposal_id}`,
      kind: "review",
      title: `Review: ${r.title || r.proposal_id}`,
      detail: `${who(r.from)} asks you (made at r${r.base_revision})${r.body ? `: ${r.body}` : ""}`,
      proposalId: r.proposal_id,
      select: { kind: "agent", id: r.from },
    });
  }
  conflicts.forEach((c, i) => {
    out.push({ key: `conflict:${i}:${c.proposals.join(",")}`, kind: "conflict", title: `${c.kind === "stale" ? "Stale proposal" : "Overlapping proposals"}: ${c.proposals.join(", ")}`, detail: c.resolve });
  });
  for (const t of view?.tasks ?? []) {
    if (t.task.status === "blocked") out.push({ key: `blocked:${t.task.id}`, kind: "blocked", title: `Blocked: ${t.task.title}`, detail: `owner ${who(t.task.owner)}`, select: { kind: "task", id: t.task.id } });
  }
  for (const s of sessions) {
    if (s.state === "failed" || s.state === "unknown") {
      out.push({ key: `session:${s.id}`, kind: "session", title: `${s.label} session ${s.state === "failed" ? "failed" : "outcome unknown"}`, detail: s.detail, select: { kind: "session", id: s.id } });
    }
  }
  return out;
}

const norm = (p: string) => p.replace(/[\\/]+$/, "");

/** Was the session started in this project folder? */
export const inProject = (s: AgentSession, dir: string | null): boolean => !!dir && norm(s.project) === norm(dir);

/**
 * The EAK-started session behind a WEFT agent: the one with its WEFT id. Terminal session ids
 * repeat across app runs, so among several the newest one started before the registration wins.
 */
export function sessionOf(agent: WeftAgentView, sessions: AgentSession[]): AgentSession | null {
  const mine = sessions.filter((s) => s.weft_id === agent.card.id).sort((a, b) => b.started_at - a.started_at);
  if (!mine.length) return null;
  return mine.find((s) => s.started_at * 1000 <= agent.registered_ms + 5000) ?? mine[mine.length - 1];
}

/** Why Stop cannot run on a session now, or null when it can. */
export function stopBlocker(s: AgentSession): string | null {
  if (s.from_earlier_run) return "Started before EAK last closed: EAK no longer supervises its process, so it cannot stop it";
  if (s.state !== "running") return "The session is not running";
  return null;
}

/** The mode an agent's harness runs in, as far as EAK knows it. */
export function permissionText(session: AgentSession | null): string {
  return session ? "Propose mode: it creates proposals and only you apply them (started from EAK)" : "Set by whoever started this agent: EAK did not start it and does not guess";
}

/** Task-contract fields of the brief (§10.4) that WEFT tasks do not record yet. */
export const CONTRACT_GAPS: { field: string; goal: string }[] = [
  { field: "Acceptance criteria and validation gates", goal: "M3" },
  { field: "Permission tier", goal: "M3" },
  { field: "Affected design objects and constraints", goal: "M3" },
  { field: "Deliverable type and output schema", goal: "M3" },
  { field: "Timeout, retry and escalation policy", goal: "M3" },
  { field: "Priority and deadline", goal: "M3" },
];

/** Case-insensitive match of a filter against any of the fields. */
export function matches(filter: string, ...fields: (string | null | undefined)[]): boolean {
  const f = filter.trim().toLowerCase();
  return !f || fields.some((x) => !!x && x.toLowerCase().includes(f));
}

/** Messages of an agent (sent, received, or to everyone) or of a task, oldest first. */
export function threadOf(sel: Selection, messages: WeftMessageView[]): WeftMessageView[] {
  if (sel.kind === "agent") return messages.filter((m) => m.message.from === sel.id || m.message.to === sel.id || m.message.to === "*");
  if (sel.kind === "task") return messages.filter((m) => m.message.task_id === sel.id);
  return messages;
}

/** Runtime line for the roster: version and sign-in state as the tool reported them. */
export function runtimeText(t: ToolStatus): string {
  if (!t.binary) return "Not found";
  const signed = t.signed_in === null ? "sign-in unknown" : t.signed_in ? "signed in" : "not signed in";
  return `${t.version ? `v${t.version}` : "version unknown"} · ${signed}`;
}

/** Clock time of a millisecond stamp ("—" for none). */
export const clock = (ms: number): string => (ms ? new Date(ms).toLocaleTimeString() : "—");
