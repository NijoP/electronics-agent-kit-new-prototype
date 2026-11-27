// Agents workspace model (ADR-0058): delivery state from the journal only (never "read"), the task
// tree, needs-you, revision binding, session ↔ WEFT agent join and the Stop blocker of a session
// restored from an earlier run.
import { describe, expect, it } from "vitest";
import type { AgentSession } from "../settings/aiTools";
import { delivery, inProject, matches, needsYou, revisionText, sessionOf, stopBlocker, taskTree, threadOf, who } from "./agentsModel";
import type { WeftMessageView, WeftTaskView, WeftView } from "./weft";

const msg = (seq: number, m: Partial<WeftMessageView["message"]>): WeftMessageView => ({
  seq,
  ts_ms: seq * 1000,
  revision: 3,
  message: { id: `m${seq}`, kind: "context_share", from: "agent:a@1", to: "*", task_id: null, proposal_id: null, body: "", verdict: null, status: null, ...m },
});
const task = (id: string, extra: Partial<WeftTaskView["task"]> = {}, created_ms = 1): WeftTaskView => ({
  task: { id, title: `task ${id}`, owner: "agent:b@2", created_by: "agent:a@1", depends_on: [], parent: null, status: "pending", ...extra },
  created_ms,
  created_revision: 2,
  updated_ms: created_ms,
  waiting_on: [],
  ready: true,
});
const session = (over: Partial<AgentSession> = {}): AgentSession => ({
  id: "claude-1-1",
  tool: "claude",
  label: "Claude Code",
  surface: "headless",
  project: "/p/demo/",
  prompt: "check",
  started_at: 100,
  state: "running",
  detail: "running",
  exit_code: null,
  pid: 1,
  terminal_session: null,
  log_file: null,
  events: [],
  weft_id: "agent:claude-code@claude-1-1",
  from_earlier_run: false,
  ...over,
});

describe("agents model", () => {
  it("says who: agents by name and session, the engineer, everyone", () => {
    expect(who("agent:layout@s1")).toBe("layout (session s1)");
    expect(who("engineer")).toBe("You (engineer)");
    expect(who("*")).toBe("Everyone");
  });

  it("delivery: recorded always, answered only when the journal shows the reply, read never claimed", () => {
    const ask = msg(1, { kind: "review_request", from: "agent:a@1", to: "engineer", proposal_id: "p1" });
    const del = msg(2, { kind: "delegate", from: "agent:a@1", to: "agent:b@2", task_id: "t1" });
    const ctx = msg(3, { kind: "context_share" });
    const before = [ask, del, ctx];
    expect(delivery(ask, before)).toEqual({ answered: false, text: "recorded · awaiting answer · read not tracked" });
    expect(delivery(del, before).answered).toBe(false);
    expect(delivery(ctx, before)).toEqual({ answered: null, text: "recorded · no reply expected · read not tracked" });
    // A verdict from someone else, or a status from a non-owner, is not the answer.
    const wrong = [...before, msg(4, { kind: "review_verdict", from: "agent:c@3", to: "agent:a@1", proposal_id: "p1", verdict: "approve" }), msg(5, { kind: "status", from: "agent:c@3", task_id: "t1", status: "in_progress" })];
    expect(delivery(ask, wrong).answered).toBe(false);
    expect(delivery(del, wrong).answered).toBe(false);
    const right = [...before, msg(6, { kind: "review_verdict", from: "engineer", to: "agent:a@1", proposal_id: "p1", verdict: "reject" }), msg(7, { kind: "blocker", from: "agent:b@2", task_id: "t1", status: "blocked" })];
    expect(delivery(ask, right).answered).toBe(true);
    expect(delivery(del, right).text).toBe("recorded · answered · read not tracked");
    for (const m of right) expect(delivery(m, right).text).not.toMatch(/\bread\b(?! not tracked)/);
  });

  it("task tree: sub-tasks under their parent in creation order; an orphan or a cycle is never hidden", () => {
    const rows = taskTree([task("c", { parent: "a" }, 3), task("a", {}, 1), task("b", {}, 2), task("d", { parent: "c" }, 4), task("o", { parent: "gone" }, 5)]);
    expect(rows.map((r) => [r.task.task.id, r.depth])).toEqual([["a", 0], ["c", 1], ["d", 2], ["b", 0], ["o", 0]]);
    const cyc = taskTree([task("x", { parent: "y" }, 1), task("y", { parent: "x" }, 2)]);
    expect(cyc.map((r) => r.task.task.id).sort()).toEqual(["x", "y"]);
  });

  it("revision binding says when the project moved on", () => {
    expect(revisionText(3, 3)).toBe("created at r3 · project still at r3");
    expect(revisionText(3, 5)).toBe("created at r3 · project now r5");
  });

  it("needs you: review requests, conflicts, blocked tasks and failed or unsupervised sessions", () => {
    const view: WeftView = { project_dir: "/p/demo", revision: 4, entries: 3, unreadable: 0, agents: [], tasks: [task("t1", { status: "blocked" }), task("t2")], messages: [] };
    const out = needsYou(
      view,
      [{ seq: 1, proposal_id: "p1", from: "agent:a@1", body: "please", title: "Input cap", base_revision: 4 }],
      [{ kind: "stale", proposals: ["p2"], base_revision: 2, current_revision: 4, resolve: "reject it" }],
      [session({ state: "failed", detail: "exited with code 2" }), session({ id: "s2", state: "unknown" }), session({ id: "s3", state: "completed" })],
    );
    expect(out.map((n) => n.kind)).toEqual(["review", "conflict", "blocked", "session", "session"]);
    expect(out[0]).toMatchObject({ proposalId: "p1", title: "Review: Input cap", select: { kind: "agent", id: "agent:a@1" } });
    expect(out[2].select).toEqual({ kind: "task", id: "t1" });
    expect(needsYou(null, [], [], [])).toEqual([]);
  });

  it("joins a session to its WEFT agent by id, preferring the run that registered it", () => {
    const agent = { card: { id: "agent:claude-code@3", name: "claude-code", session_id: "3", tool: "claude-code", model: "", capabilities: [] }, registered_ms: 200_500, registered_revision: 1 };
    const old = session({ id: "old", weft_id: "agent:claude-code@3", started_at: 100 });
    const now = session({ id: "new", weft_id: "agent:claude-code@3", started_at: 200 });
    const later = session({ id: "later", weft_id: "agent:claude-code@3", started_at: 900 });
    expect(sessionOf(agent, [old, now, later])?.id).toBe("new");
    expect(sessionOf(agent, [session({ weft_id: "agent:other@1" })])).toBeNull();
  });

  it("Stop says why it cannot run: an earlier run's session, or one that is not running", () => {
    expect(stopBlocker(session())).toBeNull();
    expect(stopBlocker(session({ state: "completed" }))).toBe("The session is not running");
    expect(stopBlocker(session({ state: "unknown", from_earlier_run: true }))).toMatch(/no longer supervises/);
  });

  it("filters, threads and project folders", () => {
    expect(matches("", "x")).toBe(true);
    expect(matches("LAY", "agent:layout@1")).toBe(true);
    expect(matches("zzz", "a", null, undefined)).toBe(false);
    expect(inProject(session(), "/p/demo")).toBe(true);
    expect(inProject(session(), null)).toBe(false);
    const ms = [msg(1, { from: "agent:a@1", to: "agent:b@2" }), msg(2, { from: "agent:c@3", to: "*" }), msg(3, { from: "agent:c@3", to: "agent:d@4", task_id: "t9" })];
    expect(threadOf({ kind: "agent", id: "agent:b@2" }, ms).map((m) => m.seq)).toEqual([1, 2]);
    expect(threadOf({ kind: "task", id: "t9" }, ms).map((m) => m.seq)).toEqual([3]);
  });
});
