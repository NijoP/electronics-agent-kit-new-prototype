// Agent task status (ADR-0036). The backend (crates/eak-harness/src/agent_task.rs) records phases
// from what the harness observed; this module only reads them and presents one shared summary for
// the EDA workspace status bar and the Terminal workspace, so both show the same revision and
// task phase. Pure helpers are testable; the hook wires IPC and the change event.
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "./api";

/** Must match `AGENT_TASK_EVENT` in src-tauri/src/sync.rs. */
export const AGENT_TASK_EVENT = "eak://agent-task-changed";

/** Must match `Phase` in crates/eak-harness/src/agent_task.rs. */
export type TaskPhase =
  | "planning"
  | "generating_schematic"
  | "schematic_committed"
  | "running_erc"
  | "schematic_approved"
  | "syncing_pcb"
  | "placement"
  | "routing"
  | "running_drc"
  | "manufacturing_preview"
  | "completed"
  | "partial"
  | "blocked"
  | "failed"
  | "cancelled";

/** Must match `AgentTask` in crates/eak-harness/src/agent_task.rs. */
export interface AgentTask {
  id: string;
  project_id: string;
  session_id: string;
  agent: string;
  title: string;
  starting_revision: number;
  revision: number;
  phase: TaskPhase;
  detail: string | null;
  started_ms: number;
  updated_ms: number;
  ended_ms: number | null;
  last_erc_errors: number | null;
  last_drc_errors: number | null;
  history: { phase: TaskPhase; revision: number; at_ms: number; note: string | null }[];
}

/** Must match `AgentTasksChanged` in src-tauri/src/sync.rs. */
export interface AgentTasksChanged {
  dir: string;
  tasks: AgentTask[];
}

export const PHASE_LABEL: Record<TaskPhase, string> = {
  planning: "Planning",
  generating_schematic: "Generating schematic",
  schematic_committed: "Schematic committed",
  running_erc: "ERC run",
  schematic_approved: "Schematic approved for PCB sync",
  syncing_pcb: "PCB synchronised",
  placement: "Placement",
  routing: "Routing",
  running_drc: "DRC run",
  manufacturing_preview: "Manufacturing preview (draft)",
  completed: "Completed",
  partial: "Partial",
  blocked: "Blocked",
  failed: "Failed",
  cancelled: "Cancelled",
};

/** The task to show: the newest unfinished one, else the newest of all. */
export function currentTask(tasks: AgentTask[]): AgentTask | null {
  for (let i = tasks.length - 1; i >= 0; i--) if (tasks[i].ended_ms == null) return tasks[i];
  return tasks.length ? tasks[tasks.length - 1] : null;
}

/** One line, identical in both workspaces: revision on screen plus the task phase. */
export function taskSummary(task: AgentTask | null, revision: number): string {
  if (!task) return `rev ${revision}`;
  const behind = task.revision < revision ? ` (task last saw rev ${task.revision})` : "";
  return `rev ${revision} · ${PHASE_LABEL[task.phase]} · ${task.agent}${behind}`;
}

/** Tooltip: starting revision, title and the grounded detail (ERC/DRC counts, gate refusal). */
export function taskTitle(task: AgentTask | null): string {
  if (!task) return "No agent task recorded for this project";
  const bits = [`${task.title}`, `started at revision ${task.starting_revision}`];
  if (task.detail) bits.push(task.detail);
  return bits.join(" — ");
}

/** Task ledger of the open project, kept current by the backend's change event. */
export function useAgentTasks(projectId: string | null, revision: number): AgentTask[] {
  const [tasks, setTasks] = useState<AgentTask[]>([]);
  useEffect(() => {
    if (!projectId) {
      setTasks([]);
      return;
    }
    let live = true;
    api
      .agentTasks()
      .then((t) => live && setTasks(Array.isArray(t) ? t : []))
      .catch(() => live && setTasks([]));
    return () => {
      live = false;
    };
  }, [projectId, revision]);
  useEffect(() => {
    if (!projectId) return;
    let live = true;
    let off: (() => void) | undefined;
    listen<AgentTasksChanged>(AGENT_TASK_EVENT, (e) => {
      const mine = e.payload.tasks.filter((t) => t.project_id === projectId);
      if (live && mine.length) setTasks(mine);
    })
      .then((un) => (live ? (off = un) : un()))
      .catch(() => undefined);
    return () => {
      live = false;
      off?.();
    };
  }, [projectId]);
  return tasks;
}
