// Pure view logic for the Terminal workspace: the aggregated Agents list, tab titles and the
// per-space status roll-up. EAK never starts, installs or configures a coding agent: a session is
// listed as an agent only when the backend recognised an agent CLI as its foreground program
// (`status.agent`, from the executable name; see crates/eak-terminal). A plain shell is never
// shown as an agent. Anything the process facts cannot settle is reported as "unknown".
import type { Overview, SessionInfo, SessionState, Tab } from "./api";

export type AgentState = "working" | "blocked" | "idle" | "done" | "unknown";

/** Dot class (design `StatusDot` state) for an agent state. */
export const AGENT_DOT: Record<AgentState, string> = {
  working: "running",
  blocked: "waiting_for_input",
  idle: "idle",
  done: "completed",
  unknown: "ended",
};

/**
 * Agent state from the session's process-derived status. Waiting-for-input and blocked both
 * mean "needs you"; a session restored from a previous run (`ended`) or whose idle verdict is
 * only a quiet-output heuristic is "unknown" rather than guessed.
 */
export function agentState(s: SessionInfo): AgentState {
  switch (s.status.state) {
    case "running":
      return "working";
    case "waiting_for_input":
    case "blocked":
      return "blocked";
    case "idle":
      return s.status.source === "heuristic" ? "unknown" : "idle";
    case "completed":
    case "failed":
      return "done";
    default:
      return "unknown";
  }
}

export interface AgentEntry {
  session: number;
  agent: string;
  state: AgentState;
  space: number;
  spaceName: string;
  tab: number;
  tabTitle: string;
  /** 1-based pane position inside the tab. */
  pane: number;
  panes: number;
}

/** A tab's title: its own name, else its first pane's session name. */
export function tabTitle(t: Tab, byId: Map<number, SessionInfo>): string {
  return t.name?.trim() || byId.get(t.panes[0])?.name || "terminal";
}

/** Detected agent sessions across every space, in space then tab then pane order. */
export function agentEntries(ov: Overview | null): AgentEntry[] {
  if (!ov) return [];
  const byId = new Map(ov.sessions.map((s) => [s.id, s]));
  const out: AgentEntry[] = [];
  for (const sp of ov.spaces) {
    for (const t of ov.tabs.filter((x) => x.space === sp.id)) {
      t.panes.forEach((id, i) => {
        const s = byId.get(id);
        if (!s?.status.agent) return;
        out.push({
          session: id,
          agent: s.status.agent,
          state: agentState(s),
          space: sp.id,
          spaceName: sp.name,
          tab: t.id,
          tabTitle: tabTitle(t, byId),
          pane: i + 1,
          panes: t.panes.length,
        });
      });
    }
  }
  return out;
}

const RANK: SessionState[] = ["blocked", "waiting_for_input", "failed", "running", "idle", "completed", "ended"];

/** The most attention-worthy state among sessions (for a space or tab dot). */
export function rollup(sessions: SessionInfo[]): SessionState {
  const present = new Set(sessions.map((s) => s.status.state));
  return RANK.find((r) => present.has(r)) ?? "ended";
}

/** Short process-derived state word for tabs and pane headers. */
export const STATE_WORD: Record<string, string> = {
  running: "running",
  idle: "idle",
  waiting_for_input: "waiting",
  blocked: "blocked",
  completed: "done",
  failed: "failed",
  ended: "ended",
};
