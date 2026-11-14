// Typed client for the terminal workspace IPC (src-tauri/src/terminal.rs).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { TERMINAL_IPC } from "../api";

type Cmd = (typeof TERMINAL_IPC)[number];
const call = <T>(cmd: Cmd, args?: Record<string, unknown>) => invoke<T>(cmd, args);

export type SessionState = "running" | "idle" | "waiting_for_input" | "blocked" | "completed" | "failed" | "ended";
export interface Status {
  state: SessionState;
  source: "process" | "integration" | "heuristic";
  detail: string;
  foreground: string | null;
  agent: string | null;
  exit_code: number | null;
}
export interface SessionInfo {
  id: number;
  name: string;
  space: number;
  tab: number;
  spec: { program: { kind: "shell"; path: string | null } | { kind: "command"; argv: string[] }; cwd: string; project: string | null };
  pid: number | null;
  status: Status;
  output_seq: number;
}
export interface Space { id: number; name: string; root: string; project_id: string | null }
export interface Tab { id: number; space: number; direction: "row" | "column"; panes: number[] }
export interface Overview { spaces: Space[]; tabs: Tab[]; sessions: SessionInfo[]; focused: number | null; layout_seq: number }
export interface HerdrHealth { session: string; bin: string; running: boolean; version: string | null; started_by_eak: boolean; restarts: number; error: string | null }
export interface HerdrView { installed: boolean; error: string | null; health: HerdrHealth | null; client: string[] | null }
export interface CreateOptions { space?: number; name?: string; cwd?: string; shell?: string; command?: string[]; split_of?: number; direction?: "row" | "column"; cols?: number; rows?: number }
export interface OutputEvent { id: number; data: string; seq: number }

export const term = {
  herdrStatus: () => call<HerdrView>("terminal_herdr_status"),
  herdrStart: () => call<HerdrView>("terminal_herdr_start"),
  herdrOpenProject: (dir: string, name: string, projectId: string | null) =>
    call<string>("terminal_herdr_open_project", { dir, name, project_id: projectId }),
  herdrAgents: () => call<{ agents: { agent?: string; status?: string; agent_status?: string; pane_id?: string }[] }>("terminal_herdr_agents"),
  overview: () => call<Overview>("terminal_overview"),
  shells: () => call<string[]>("terminal_shells"),
  setContext: (dir: string | null, project: string | null, projectId: string | null) =>
    call<void>("terminal_set_context", { dir, project, project_id: projectId }),
  create: (opts: CreateOptions) => call<SessionInfo>("terminal_create", { opts }),
  write: (id: number, data: string) => call<void>("terminal_write", { id, data }),
  resize: (id: number, cols: number, rows: number) => call<void>("terminal_resize", { id, cols, rows }),
  scrollback: (id: number) => call<{ data: string; seq: number }>("terminal_scrollback", { id }),
  restart: (id: number) => call<SessionInfo>("terminal_restart", { id }),
  close: (id: number) => call<void>("terminal_close", { id }),
  focus: (id: number) => call<void>("terminal_focus", { id }),
  onOutput: (f: (e: OutputEvent) => void): Promise<UnlistenFn> => listen<OutputEvent>("terminal-output", (e) => f(e.payload)),
};

export const STATE_LABEL: Record<SessionState, string> = {
  running: "Working",
  idle: "Idle",
  waiting_for_input: "Waiting for input",
  blocked: "Blocked",
  completed: "Completed",
  failed: "Failed",
  ended: "Ended (restart to run again)",
};

/** "Waiting for input (heuristic)": the source is shown unless it is a process fact. */
export function describeStatus(s: Status): string {
  const base = STATE_LABEL[s.state];
  const code = s.exit_code !== null && (s.state === "completed" || s.state === "failed") ? ` · exit ${s.exit_code}` : "";
  const src = s.source === "process" ? "" : ` (${s.source})`;
  return `${base}${code}${src}`;
}
