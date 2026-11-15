// Pure helpers of the shared agent task status and the selection reconcile (ADR-0036).
// Code-reviewed only: not run locally; CI runs it.
import { describe, expect, it } from "vitest";
import { currentTask, taskSummary, taskTitle, type AgentTask } from "./agentTasks";
import { reconcileSelection } from "./revisionSync";

const task = (over: Partial<AgentTask>): AgentTask => ({
  id: "task-1",
  project_id: "prj_1",
  session_id: "s1",
  agent: "claude",
  title: "Blink board",
  starting_revision: 4,
  revision: 9,
  phase: "routing",
  detail: null,
  started_ms: 1,
  updated_ms: 2,
  ended_ms: null,
  last_erc_errors: 0,
  last_drc_errors: null,
  history: [],
  ...over,
});

describe("agent task status", () => {
  it("shows the newest unfinished task, else the newest one", () => {
    const done = task({ id: "a", ended_ms: 5, phase: "completed" });
    const live = task({ id: "b" });
    expect(currentTask([live, done])?.id).toBe("b");
    expect(currentTask([done])?.id).toBe("a");
    expect(currentTask([])).toBeNull();
  });

  it("gives both workspaces the same revision and phase text", () => {
    expect(taskSummary(task({}), 9)).toBe("rev 9 · Routing · claude");
    expect(taskSummary(null, 9)).toBe("rev 9");
    expect(taskSummary(task({ revision: 7 }), 9)).toContain("task last saw rev 7");
  });

  it("explains the phase from the recorded detail and starting revision", () => {
    const t = taskTitle(task({ phase: "blocked", detail: "ERC has not run" }));
    expect(t).toContain("started at revision 4");
    expect(t).toContain("ERC has not run");
  });
});

describe("reconcileSelection", () => {
  const project = { schematic: { symbols: { sym_1: {} } }, pcb: { footprints: { fp_1: {} } } };
  it("keeps a selection that still exists and drops one an external commit deleted", () => {
    expect(reconcileSelection("sym_1", project)).toBe("sym_1");
    expect(reconcileSelection("fp_1", project)).toBe("fp_1");
    expect(reconcileSelection("sym_gone", project)).toBeNull();
    expect(reconcileSelection(null, project)).toBeNull();
  });
});
