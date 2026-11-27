// @vitest-environment jsdom
// Agents workspace (M2.1/M2.2, ADR-0058) against a mocked Tauri IPC: the roster, task graph,
// needs-you and messages come from `weft_state` and the review queue only; a session restored from
// an earlier run cannot be stopped and says why; a runtime that is not installed cannot launch and
// says why; a verdict goes on the WEFT bus; nothing is read while the workspace is hidden. The
// backend is covered by src-tauri/tests/agents.rs and eak-agents; the real window by
// e2e/scenarios/m2-agents.json.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "../api";
import type { AgentSession, ToolStatus } from "../settings/aiTools";
import { AgentsWorkspace } from "./AgentsWorkspace";
import type { WeftView } from "./weft";

const PLAN = "agent:planner@s1";
const LAYOUT = "agent:layout@s2";
const view: WeftView = {
  project_dir: "/p/demo",
  revision: 4,
  entries: 6,
  unreadable: 0,
  agents: [
    { card: { id: PLAN, name: "planner", session_id: "s1", tool: "codex", model: "", capabilities: ["plan"] }, registered_ms: 1000, registered_revision: 2 },
    { card: { id: LAYOUT, name: "layout", session_id: "s2", tool: "claude-code", model: "", capabilities: [] }, registered_ms: 1100, registered_revision: 2 },
  ],
  tasks: [
    { task: { id: "t1", title: "Input decoupling", owner: LAYOUT, created_by: PLAN, depends_on: [], parent: null, status: "in_progress" }, created_ms: 1200, created_revision: 2, updated_ms: 1400, waiting_on: [], ready: false },
    { task: { id: "t2", title: "Route the rails", owner: LAYOUT, created_by: PLAN, depends_on: ["t1"], parent: null, status: "pending" }, created_ms: 1300, created_revision: 2, updated_ms: 1300, waiting_on: ["t1"], ready: false },
  ],
  messages: [
    { seq: 3, ts_ms: 1200, revision: 2, message: { id: "m1", kind: "delegate", from: PLAN, to: LAYOUT, task_id: "t1", proposal_id: null, body: "", verdict: null, status: null } },
    { seq: 5, ts_ms: 1400, revision: 2, message: { id: "m2", kind: "status", from: LAYOUT, to: "*", task_id: "t1", proposal_id: null, body: "", verdict: null, status: "in_progress" } },
    { seq: 6, ts_ms: 1500, revision: 2, message: { id: "m3", kind: "review_request", from: LAYOUT, to: "engineer", task_id: null, proposal_id: "prop-1", body: "please review", verdict: null, status: null } },
  ],
};
const restored: AgentSession = {
  id: "claude-9-1",
  tool: "claude",
  label: "Claude Code",
  surface: "terminal",
  project: "/p/demo",
  prompt: "review the rails",
  started_at: 1,
  state: "unknown",
  detail: "EAK closed while this session was running; it is not supervised any more and its outcome was not recorded",
  exit_code: null,
  pid: null,
  terminal_session: null,
  log_file: null,
  events: [],
  weft_id: "agent:claude-code@4",
  from_earlier_run: true,
};
const missing: ToolStatus = {
  id: "claude",
  label: "Claude Code",
  install_url: "https://example.invalid/install",
  binary: null,
  version: null,
  available: false,
  reason: { kind: "missing_binary" },
  signed_in: null,
  auth_detail: null,
  connected: null,
  connection_detail: null,
  connection_scope: null,
  can_connect: true,
  can_login: true,
  surfaces: ["terminal", "headless"],
};

let indexError: string | null = null;
let calls: { cmd: string; args: Record<string, unknown> }[];
beforeEach(() => {
  calls = [];
  indexError = null;
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    switch (cmd) {
      case "weft_state":
        return view;
      case "weft_engineer_queue":
        return [{ seq: 6, proposal_id: "prop-1", from: LAYOUT, body: "please review", title: "Input decoupling", base_revision: 2 }];
      case "weft_conflicts":
        return [];
      case "activity_timeline":
        return [{ ts_ms: 1200, source: "weft", actor: PLAN, kind: "delegate", summary: "delegated t1", revision_before: null, revision_after: 2, op_id: null, proposal_id: null, task_id: "t1", objects: [], errors: null, warnings: null }];
      case "agent_sessions":
        return [restored];
      case "agent_tools":
        return [missing];
      case "agent_session_index_error":
        return indexError;
      case "agent_session_log":
        return Promise.reject(new Error("this session ran before EAK last closed and has no log file: a terminal tab's output is not kept after EAK closes"));
      case "agent_session_restart":
        return { ...restored, id: "claude-10-1", state: "running", from_earlier_run: false, detail: "running in a terminal tab", terminal_session: 2 };
      case "weft_engineer_verdict":
        return {};
      default:
        throw new Error(`unmocked ${cmd}`);
    }
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
});

const mount = (over: Partial<Parameters<typeof AgentsWorkspace>[0]> = {}) =>
  render(<AgentsWorkspace active projectId="prj_1" projectName="Fixture board" projectDir="/p/demo" {...over} />);
const roster = () => within(screen.getByRole("navigation", { name: "Roster" }));
const inspector = () => within(screen.getByRole("complementary", { name: "Inspector" }));

describe("Agents workspace", () => {
  it("shows the roster, task graph, needs-you and messages from the WEFT journal only", async () => {
    mount();
    await within(screen.getByRole("region", { name: "Agents" })).findByRole("button", { name: "Agent planner (session s1)" });
    expect(roster().getByRole("button", { name: "Agent layout (session s2)" })).toBeTruthy();
    expect(roster().getByRole("button", { name: "Task Input decoupling" })).toBeTruthy();
    expect(roster().getByRole("button", { name: "Task Route the rails" })).toBeTruthy();
    expect(screen.getAllByText("r4").length).toBeGreaterThan(0);
    const graph = within(screen.getByRole("table", { name: "Tasks" }));
    expect(graph.getAllByRole("row")).toHaveLength(3);
    expect(graph.getByText("t1")).toBeTruthy(); // t2 waits on t1
    const needs = within(screen.getByRole("region", { name: "Needs you now" }));
    expect(needs.getByText("Review: Input decoupling")).toBeTruthy();
    expect(await needs.findByText(/Claude Code session outcome unknown/)).toBeTruthy();
    // The review request is recorded and awaits an answer; reads are never claimed.
    const recent = within(screen.getByRole("region", { name: "Recent messages" }));
    expect(recent.getByText("recorded · awaiting answer · read not tracked")).toBeTruthy();
    expect(recent.getAllByText("recorded · answered · read not tracked")).toHaveLength(1); // the delegation, answered by its owner's status
    expect(await within(screen.getByRole("region", { name: "Activity" })).findByText("delegated t1")).toBeTruthy();
  });

  it("a task shows its contract, revision binding and the fields WEFT does not record yet; Escape returns to the overview", async () => {
    mount();
    fireEvent.click(await roster().findByRole("button", { name: "Task Route the rails" }));
    expect(screen.getByRole("heading", { name: "Route the rails", level: 2 })).toBeTruthy();
    expect(inspector().getByText("created at r2 · project now r4")).toBeTruthy();
    expect(inspector().getAllByText("t1", { selector: "dd" })).toHaveLength(2); // depends on, waits on
    expect(within(inspector().getByRole("list", { name: "Not recorded" })).getByText(/Acceptance criteria/)).toBeTruthy();
    fireEvent.click(within(screen.getByRole("main", { name: "Work surface" })).getByRole("button", { name: "Task Input decoupling" }));
    expect(screen.getByRole("heading", { name: "Input decoupling", level: 2 })).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("main", { name: "Work surface" }), { key: "Escape" });
    expect(screen.getByRole("heading", { name: "Overview", level: 2 })).toBeTruthy();
  });

  it("a session from an earlier run cannot be stopped and says why; Restart starts a new one", async () => {
    mount();
    fireEvent.click(await roster().findByRole("button", { name: "Session claude-9-1" }));
    const stop = screen.getByRole("button", { name: "Stop" }) as HTMLButtonElement;
    expect(stop.disabled).toBe(true);
    expect(stop.title).toMatch(/no longer supervises its process/);
    expect((screen.getByRole("button", { name: "Show in Terminal" }) as HTMLButtonElement).disabled).toBe(true);
    await screen.findByText(/terminal tab's output is not kept/);
    expect(inspector().getByText("earlier run: not supervised")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Restart" }));
    await screen.findByText("Claude Code started again as session claude-10-1");
    expect(calls.filter((c) => c.cmd === "agent_session_restart")[0].args).toEqual({ id: "claude-9-1" });
    expect(calls.some((c) => c.cmd === "agent_tool_stop")).toBe(false);
  });

  it("a runtime that is not installed cannot launch, sign in or connect, and says why", async () => {
    mount();
    fireEvent.click(await roster().findByRole("button", { name: "Runtime Claude Code" }));
    for (const name of ["Launch session", "Sign in", "Connect"]) {
      const b = screen.getByRole("button", { name }) as HTMLButtonElement;
      expect(b.disabled, name).toBe(true);
      expect(b.title, name).toBe("Claude Code is not installed");
    }
    expect(inspector().getByText("Not found")).toBeTruthy();
  });

  it("Request changes answers on the WEFT bus and discards the proposal (nothing is applied)", async () => {
    mount();
    const needs = within(await screen.findByRole("region", { name: "Needs you now" }));
    fireEvent.click(await needs.findByRole("button", { name: "Request changes" }));
    await screen.findByText(/Proposal prop-1 sent back/);
    const sent = calls.filter((c) => c.cmd === "weft_engineer_verdict");
    expect(sent).toHaveLength(1);
    expect(sent[0].args).toMatchObject({ proposal_id: "prop-1", verdict: "request_changes", discard: true });
    expect(calls.some((c) => c.cmd === "proposal_apply")).toBe(false);
  });

  it("with no project it reads no journal and says so; hidden, it reads nothing", async () => {
    const { unmount } = mount({ projectId: null, projectName: null, projectDir: null });
    expect((await screen.findAllByText("No project open")).length).toBeGreaterThan(0);
    await waitFor(() => expect(calls.some((c) => c.cmd === "agent_sessions")).toBe(true));
    expect(calls.some((c) => c.cmd === "weft_state")).toBe(false);
    unmount();
    calls = [];
    mount({ active: false });
    await new Promise((r) => setTimeout(r, 50));
    expect(calls).toEqual([]);
  });
});

describe("Agents workspace session index", () => {
  it("says so when EAK could not write its session index, instead of hiding it", async () => {
    indexError = "EAK could not record session claude-9-1 in its session index /data/sessions.jsonl: Permission denied";
    mount();
    const sessions = within(await screen.findByRole("region", { name: "Sessions" }));
    expect((await sessions.findByRole("alert")).textContent).toContain("could not record session claude-9-1");
  });
});
