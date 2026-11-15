// Pure Agents-list logic: only detected agent CLIs are listed, states come from process facts.
import { describe, expect, it } from "vitest";
import { agentEntries, agentState, rollup, tabTitle } from "./agents";
import type { Overview, SessionInfo } from "./api";

const s = (id: number, state: SessionInfo["status"]["state"], agent: string | null = null, source: SessionInfo["status"]["source"] = "process"): SessionInfo => ({
  id,
  name: `t${id}`,
  space: 1,
  tab: 1,
  spec: { program: { kind: "shell", path: null }, cwd: "/", project: null },
  pid: id,
  status: { state, source, detail: "", foreground: agent, agent, exit_code: null },
  output_seq: 0,
});

describe("agent state", () => {
  it("maps process states to working/blocked/idle/done and never guesses", () => {
    expect(agentState(s(1, "running", "claude"))).toBe("working");
    expect(agentState(s(1, "waiting_for_input", "claude"))).toBe("blocked");
    expect(agentState(s(1, "blocked", "claude"))).toBe("blocked");
    expect(agentState(s(1, "idle", "claude"))).toBe("idle");
    expect(agentState(s(1, "completed", "claude"))).toBe("done");
    expect(agentState(s(1, "failed", "claude"))).toBe("done");
    expect(agentState(s(1, "ended", "claude"))).toBe("unknown");
    // A quiet-output idle guess (Windows) is not evidence of idleness.
    expect(agentState(s(1, "idle", "claude", "heuristic"))).toBe("unknown");
  });
});

describe("agentEntries", () => {
  const ov: Overview = {
    spaces: [{ id: 1, name: "Board", root: "/b", project_id: null }],
    tabs: [
      { id: 1, space: 1, name: null, direction: "row", panes: [1, 2] },
      { id: 2, space: 1, name: "Review", direction: "row", panes: [3] },
    ],
    sessions: [s(1, "idle"), s(2, "running", "opencode"), s(3, "waiting_for_input", "codex")],
    focused: 1,
    layout_seq: 1,
  };
  it("lists detected agents in space/tab/pane order and skips plain shells", () => {
    const e = agentEntries(ov);
    expect(e.map((x) => [x.agent, x.state, x.tabTitle, x.pane, x.panes])).toEqual([
      ["opencode", "working", "t1", 2, 2],
      ["codex", "blocked", "Review", 1, 1],
    ]);
    expect(agentEntries(null)).toEqual([]);
  });
  it("tab title prefers the tab's own name", () => {
    const by = new Map(ov.sessions.map((x) => [x.id, x]));
    expect(tabTitle(ov.tabs[0], by)).toBe("t1");
    expect(tabTitle(ov.tabs[1], by)).toBe("Review");
  });
  it("rollup surfaces the most attention-worthy state", () => {
    expect(rollup([s(1, "idle"), s(2, "waiting_for_input")])).toBe("waiting_for_input");
    expect(rollup([s(1, "idle"), s(2, "running")])).toBe("running");
    expect(rollup([])).toBe("ended");
  });
});
