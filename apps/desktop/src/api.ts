// Typed client for the Tauri IPC commands in src-tauri/src/lib.rs. This is the only
// module that talks to Rust; every project change is an engine call made here.
import { invoke } from "@tauri-apps/api/core";
import type {
  Check,
  CheckResult,
  Command,
  CommandError,
  CommandResponse,
  EngineEvent,
  RecentProject,
  Snapshot,
} from "./types";

// Must match `IPC_COMMANDS` in src-tauri/src/lib.rs (checked by tests/ipc.rs).
export const IPC_COMMANDS = [
  "create_project",
  "open_project",
  "close_project",
  "current_project",
  "execute_command",
  "undo",
  "redo",
  "events_since",
  "acknowledge_recovery",
  "run_check",
  "recent_projects",
] as const;
type IpcCommand = (typeof IPC_COMMANDS)[number];

function call<T>(cmd: IpcCommand, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(cmd, args);
}

export const api = {
  createProject: (path: string, name: string) => call<Snapshot>("create_project", { path, name }),
  openProject: (path: string, takeOverLock: boolean) =>
    call<Snapshot>("open_project", { path, take_over_lock: takeOverLock }),
  closeProject: () => call<void>("close_project"),
  currentProject: () => call<Snapshot>("current_project"),
  execute: (expectedRevision: number, command: Command) =>
    call<CommandResponse>("execute_command", { expected_revision: expectedRevision, command }),
  undo: (expectedRevision: number) =>
    call<CommandResponse>("undo", { expected_revision: expectedRevision }),
  redo: (expectedRevision: number) =>
    call<CommandResponse>("redo", { expected_revision: expectedRevision }),
  eventsSince: (seq: number) => call<EngineEvent[]>("events_since", { seq }),
  acknowledgeRecovery: () => call<Snapshot>("acknowledge_recovery"),
  runCheck: (check: Check) => call<CheckResult>("run_check", { check }),
  recentProjects: (limit: number) => call<RecentProject[]>("recent_projects", { limit }),
};

export function describeError(e: CommandError): string {
  const m = e.message;
  if (typeof m === "string") return `${e.code}: ${m}`;
  if (m && typeof m === "object") return `${e.code}: ${JSON.stringify(m)}`;
  return e.code;
}
