// @vitest-environment jsdom
// Agent activity (ADR-0053): grouping by actor, ERC/DRC around a revision from the saved reports
// only (a revision with no report is "not run", never zero), the newest-revision undo, the
// changed object to show; and the dialog against a mocked Tauri IPC (rows from the timeline,
// Review proposal, Show on canvas and Undo calling back). The backend is tested by
// src-tauri/tests/proposal.rs; the real window by e2e/scenarios/w2-activity.json.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "../api";
import { ActivityDialog } from "./ActivityDialog";
import { actorName, checkDeltas, firstChanged, groupByActor, latestRevision, type ActivityItem } from "./activity";

const base: ActivityItem = { ts_ms: 1, source: "revision", actor: "", kind: "", summary: "", revision_before: null, revision_after: null, op_id: null, proposal_id: null, task_id: null, objects: [], errors: null, warnings: null };
const AGENT = "agent:layout@s1";
const items: ActivityItem[] = [
  { ...base, ts_ms: 1, source: "weft", actor: AGENT, kind: "agent_registered", summary: "registered (codex)", revision_after: 0 },
  { ...base, ts_ms: 2, source: "check", actor: "system", kind: "ERC", summary: "ERC on revision 0", revision_after: 0, errors: 0, warnings: 0 },
  { ...base, ts_ms: 3, source: "proposal", actor: AGENT, kind: "submitted", summary: "proposal prop-1 submitted", proposal_id: "prop-1", revision_after: 0 },
  { ...base, ts_ms: 4, source: "revision", actor: AGENT, kind: "Batch", summary: "ran Batch → revision 1 (2 change(s))", revision_before: 0, revision_after: 1, op_id: "op_1", objects: ["library:C_0805", "schematic.symbols:sym_c1"] },
  { ...base, ts_ms: 5, source: "check", actor: "system", kind: "ERC", summary: "ERC on revision 1", revision_after: 1, errors: 2, warnings: 1 },
];

describe("activity helpers", () => {
  it("names actors, groups by actor in first-seen order and finds the changed part", () => {
    expect(actorName(AGENT)).toBe("layout (session s1)");
    expect(actorName("human:you")).toBe("You");
    expect(groupByActor(items).map((g) => [g.actor, g.items.length])).toEqual([[AGENT, 3], ["system", 2]]);
    expect(firstChanged(items[3])).toEqual({ editor: "schematic", id: "sym_c1" });
    expect(firstChanged(items[0])).toBeNull();
  });

  it("ERC/DRC around a revision come from saved reports only; no report is 'not run'", () => {
    expect(checkDeltas(items, items[3])).toEqual([{ check: "ERC", text: "ERC: 0 error(s) → 2 error(s)" }]);
    const noBefore = items.filter((i) => !(i.source === "check" && i.revision_after === 0));
    expect(checkDeltas(noBefore, items[3])).toEqual([{ check: "ERC", text: "ERC: not run → 2 error(s)" }]);
    expect(checkDeltas(items, items[0])).toEqual([]);
  });

  it("only the newest revision is offered for undo", () => {
    expect(latestRevision(items, 1)).toBe(items[3]);
    expect(latestRevision(items, 2)).toBeNull();
  });
});

let calls: string[];
beforeEach(() => {
  calls = [];
  mockIPC((cmd) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    calls.push(cmd);
    if (cmd === "activity_timeline") return items;
    throw new Error(`unmocked ${cmd}`);
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("activity dialog", () => {
  it("shows one timeline per actor from the journal and routes actions to the existing paths", async () => {
    const onReview = vi.fn();
    const onUndo = vi.fn();
    const onShow = vi.fn();
    render(<ActivityDialog revision={1} busy={false} onReview={onReview} onUndo={onUndo} onShow={onShow} onClose={() => {}} />);
    await waitFor(() => expect(document.querySelectorAll("li[data-activity]").length).toBe(5));
    const agent = document.querySelector(`section[data-actor="${AGENT}"]`)!;
    expect(agent.querySelectorAll("li").length).toBe(3);
    const rev = agent.querySelector('li[data-activity="revision"]')!;
    expect(rev.textContent).toContain("r0 → r1");
    expect(rev.querySelector('[data-check="ERC"]')!.textContent).toContain("0 error(s) → 2 error(s)");

    fireEvent.click(screen.getByRole("button", { name: "Review proposal" }));
    expect(onReview).toHaveBeenCalledWith("prop-1");
    fireEvent.click(screen.getByRole("button", { name: "Show on canvas" }));
    expect(onShow).toHaveBeenCalledWith({ editor: "schematic", id: "sym_c1" });
    fireEvent.click(screen.getByRole("button", { name: "Undo this revision" }));
    expect(onUndo).toHaveBeenCalledOnce();
    expect(calls[0]).toBe("activity_timeline");
  });

  it("offers no undo when the project moved past the row", async () => {
    render(<ActivityDialog revision={2} busy={false} onReview={() => {}} onUndo={() => {}} onShow={() => {}} onClose={() => {}} />);
    await waitFor(() => expect(document.querySelectorAll("li[data-activity]").length).toBe(5));
    expect(screen.queryByRole("button", { name: "Undo this revision" })).toBeNull();
  });
});
