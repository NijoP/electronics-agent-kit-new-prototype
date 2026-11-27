// Settings › AI tools: the shapes `agent_*` IPC returns (src-tauri/src/agents.rs over the
// eak-agents crate) and the plain-text facts each row shows. A state EAK could not read is
// "unknown", never "yes".

export type Surface = "terminal" | "headless";

export type Unavailable =
  | { kind: "missing_binary" }
  | { kind: "not_signed_in" }
  | { kind: "unsupported"; detail: string }
  | { kind: "other"; detail: string };

/** Must match `eak_agents::ToolStatus`. */
export interface ToolStatus {
  id: string;
  label: string;
  install_url: string;
  binary: string | null;
  version: string | null;
  available: boolean;
  reason: Unavailable | null;
  signed_in: boolean | null;
  auth_detail: string | null;
  connected: boolean | null;
  connection_detail: string | null;
  connection_scope: string | null;
  can_connect: boolean;
  can_login: boolean;
  surfaces: Surface[];
}

/** Must match `eak_agents::AgentEvent`. */
export type AgentEvent =
  | { kind: "started"; detail: string }
  | { kind: "message"; text: string }
  | { kind: "tool_call"; name: string }
  | { kind: "finished"; ok: boolean; summary: string }
  | { kind: "error"; message: string };

/** `unknown`: recorded as running when EAK last closed; no longer supervised, outcome not known (ADR-0058). */
export type SessionState = "running" | "completed" | "failed" | "stopped" | "unknown";

/** Must match `eak_agents::sessions::SessionView`. */
export interface AgentSession {
  id: string;
  tool: string;
  label: string;
  surface: Surface;
  project: string;
  prompt: string | null;
  started_at: number;
  state: SessionState;
  detail: string;
  exit_code: number | null;
  pid: number | null;
  terminal_session: number | null;
  log_file: string | null;
  events: AgentEvent[];
  /** WEFT agent id the session registers under (`agent:<name>@<session>`), when known. */
  weft_id: string | null;
  /** Restored from an earlier app run: no process, tab or live output here. */
  from_earlier_run: boolean;
}

export const yesNoUnknown = (v: boolean | null): string => (v === null ? "unknown" : v ? "yes" : "no");

export function foundText(t: ToolStatus): string {
  if (!t.binary) return "Not found";
  return t.version ? `Found · v${t.version}` : "Found · version unknown";
}

export function signedInText(t: ToolStatus): string {
  return t.binary ? yesNoUnknown(t.signed_in) : "—";
}

export function connectedText(t: ToolStatus): string {
  if (!t.binary) return "—";
  if (!t.can_connect) return "not supported";
  return yesNoUnknown(t.connected);
}

/** Why the row is not ready, in plain words; null when it is ready. */
export function reasonText(t: ToolStatus): string | null {
  const r = t.reason;
  if (r?.kind === "missing_binary") return `Not installed on this computer. Install it from ${t.install_url}`;
  if (r?.kind === "not_signed_in") return t.auth_detail ?? "Not signed in: use Sign in.";
  if (r?.kind === "unsupported" || r?.kind === "other") return r.detail;
  if (t.connected !== true) return t.connection_detail;
  return null;
}

/** Why Connect cannot run now, or null when it can. */
export function connectBlocker(t: ToolStatus, projectOpen: boolean): string | null {
  if (!t.binary) return `${t.label} is not installed`;
  if (!t.can_connect) return t.connection_detail ?? "EAK cannot register its MCP server with this tool yet";
  if (!projectOpen) return "Open a project first: EAK connects an AI tool to one project";
  if (t.connected === true) return "Already connected to this project";
  return null;
}

export function loginBlocker(t: ToolStatus): string | null {
  if (!t.binary) return `${t.label} is not installed`;
  if (!t.can_login) return `${t.label} has no separate sign-in command; it signs in when you first start it`;
  return null;
}

export function launchBlocker(t: ToolStatus, surface: Surface, prompt: string, projectOpen: boolean): string | null {
  if (!t.binary) return `${t.label} is not installed`;
  if (!projectOpen) return "Open a project first";
  if (!t.surfaces.includes(surface)) return `EAK has no confirmed ${surface} mode for ${t.label}`;
  if (surface === "headless" && !prompt.trim()) return "A headless session needs a first message (the task)";
  if (prompt.trim().startsWith("-")) return "The first message cannot start with '-'";
  return null;
}

export function eventText(e: AgentEvent): string {
  switch (e.kind) {
    case "started":
      return e.detail;
    case "message":
      return e.text;
    case "tool_call":
      return `tool: ${e.name}`;
    case "finished":
      return `${e.ok ? "finished" : "failed"}: ${e.summary}`;
    case "error":
      return `error: ${e.message}`;
  }
}

export const SESSION_STATE_LABEL: Record<SessionState, string> = {
  running: "Running",
  completed: "Completed",
  failed: "Failed",
  stopped: "Stopped",
  unknown: "Unknown (EAK restarted)",
};
