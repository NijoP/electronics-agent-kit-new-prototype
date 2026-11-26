// @vitest-environment jsdom
// S9 proposal review (ADR-0051): the rows, grouping and step selection built from EAK's review, and
// the dialog against a mocked Tauri IPC. Every selection change asks EAK to review it again, Apply
// sends exactly the reviewed steps at the reviewed revision, Discard asks to confirm, Export saves
// EAK's review, and a stale proposal cannot be applied. The engine side is tested in
// crates/eak-harness/tests/proposal.rs and src-tauri/tests/proposal.rs; the real window in
// e2e/scenarios/g4-proposal.json.
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "../api";
import { ProposalReviewDialog } from "./ProposalReviewDialog";
import {
  buildProposalRows,
  groupProposalRows,
  proposalApplyState,
  selectionArg,
  toggleStep,
  type CheckDelta,
  type ProposalReview,
  type ProposalSummary,
  type StepEffect,
} from "./proposalReview";

const STEPS: StepEffect[] = [
  {
    index: 0,
    tool: "PlaceSymbol",
    command: "PlaceSymbol",
    class: "reversible_local",
    changes: [
      { path: ["library"], key: "C_0805", before: null, after: { id: "C_0805", name: "C 0805" } },
      { path: ["schematic", "symbols"], key: "sym_a", before: null, after: { id: "sym_a", refdes: "C1", component_id: "C_0805" } },
    ],
    warnings: [],
    created: ["sym_a"],
    requires: [],
  },
  {
    index: 1,
    tool: "LabelPin",
    command: "LabelPin",
    class: "reversible_local",
    changes: [{ path: ["schematic", "labels"], key: "lbl_a", before: null, after: { id: "lbl_a", name: "VIN" } }],
    warnings: [],
    created: ["lbl_a"],
    requires: [0],
  },
  {
    index: 2,
    tool: "SetDesignRules",
    command: "SetDesignRules",
    class: "engineering_impacting",
    changes: [{ path: ["constraints"], key: null, before: { min_clearance: 200000, min_drill: 300000 }, after: { min_clearance: 175000, min_drill: 300000 } }],
    warnings: [],
    created: [],
    requires: [],
  },
];

const delta = (check: string, after: number): CheckDelta => ({
  check,
  before: { errors: 0, warnings: 0, unknown_rules: 1 },
  after: { errors: 0, warnings: after, unknown_rules: 1 },
  appeared: [],
  resolved: [],
  unchanged: 0,
  newly_unknown: [],
  not_implemented: ["ERC-X: not implemented"],
  limitations: [],
  label: "unverified",
});

const review = (selected: number[] | null, over: Partial<ProposalReview> = {}): ProposalReview => ({
  proposal: { id: "prop-1", title: "Input stage", rationale: "adds the input capacitor", agent: "copilot", session_id: "s1", base_revision: 4, status: "ready" },
  current_revision: 4,
  stale: false,
  selected: selected ?? [0, 1, 2],
  steps: STEPS,
  changes: [],
  checks: [delta("ERC", (selected ?? [0, 1, 2]).length), delta("DRC", 0)],
  evidence: [{ subject: "agent rationale", label: "assumed", basis: "the agent's own words" }],
  problem: null,
  note: "computed by EAK on a copy of the project; nothing was applied",
  ...over,
});

const summary: ProposalSummary = { id: "prop-1", title: "Input stage", status: "ready", agent: "copilot", session_id: "s1", base_revision: 4, stale: false, steps: 3, created_ms: 1, updated_ms: 2 };

describe("proposal rows and selection", () => {
  it("one row per change, grouped by object, with names and the fields an update changes", () => {
    const rows = buildProposalRows(STEPS);
    expect(rows.map((r) => [r.step, r.action, r.group, r.object, r.name])).toEqual([
      [0, "Add", "Part", "Library part", "C 0805"],
      [0, "Add", "Part", "Symbol", "C1"],
      [1, "Add", "Net", "Net label", "VIN"],
      [2, "Update", "Rule", "Design rules", "Design rules"],
    ]);
    const rule = rows[3];
    expect([rule.before, rule.after]).toEqual(["min_clearance 200000", "min_clearance 175000"]);
    expect(groupProposalRows(rows, STEPS, "object").map((g) => [g.group, g.rows.length])).toEqual([["Part", 2], ["Net", 1], ["Rule", 1]]);
    expect(groupProposalRows(rows, STEPS, "step").map((g) => g.group)).toEqual(["Step 1: PlaceSymbol", "Step 2: LabelPin", "Step 3: SetDesignRules"]);
  });

  it("a step that uses another step's object goes with it, both ways", () => {
    const all = new Set([0, 1, 2]);
    expect([...toggleStep(STEPS, all, 0, false)].sort()).toEqual([2]);
    expect([...toggleStep(STEPS, new Set([2]), 1, true)].sort()).toEqual([0, 1, 2]);
    expect(selectionArg(STEPS, all)).toBeNull();
    expect(selectionArg(STEPS, new Set([2, 0]))).toEqual([0, 2]);
  });

  it("Apply needs EAK's review of exactly the checked steps and no problem", () => {
    expect(proposalApplyState(review(null), new Set([0, 1, 2]), false)).toMatchObject({ ok: true, steps: null, count: 3 });
    expect(proposalApplyState(review(null), new Set([2]), false).reason).toMatch(/reviewing/);
    expect(proposalApplyState(review([2]), new Set([2]), true).reason).toMatch(/reviewing/);
    expect(proposalApplyState(review([2]), new Set([2]), false)).toMatchObject({ ok: true, steps: [2], count: 1 });
    expect(proposalApplyState(review(null), new Set(), false).reason).toMatch(/No step checked/);
    expect(proposalApplyState(review(null, { stale: true, problem: "the project is at revision 5" }), new Set([0, 1, 2]), false).reason).toMatch(/revision 5/);
    expect(proposalApplyState(review(null, { problem: "step 2 fails" }), new Set([0, 1, 2]), false).reason).toBe("step 2 fails");
    expect(proposalApplyState(review(null, { proposal: { ...review(null).proposal, status: "applied" } }), new Set([0, 1, 2]), false).ok).toBe(false);
  });
});

let calls: { cmd: string; args: Record<string, unknown> }[];
let list: ProposalSummary[];
let stale: boolean;

beforeEach(() => {
  calls = [];
  list = [summary];
  stale = false;
  mockIPC((cmd, payload) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    if (cmd === "proposal_list") return list;
    if (cmd === "proposal_review") {
      const steps = args.steps as number[] | null;
      return stale ? review(null, { stale: true, steps: [], problem: "the project is at revision 5 but the proposal was made at revision 4" }) : review(steps);
    }
    if (cmd === "proposal_apply") return { response: { accepted: true, revision_before: 4, revision_after: 5, command: "Batch", warnings: [] }, checkpoint: "r000004-before-prop-1", proposal: { ...review(null).proposal, status: "applied" } };
    if (cmd === "proposal_export") return "/p/evidence/proposal-prop-1-r000004.json";
    if (cmd === "proposal_discard") {
      list = [{ ...summary, status: "discarded" }];
      return list[0];
    }
    throw new Error(`unmocked ${cmd}`);
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

const dialog = () => screen.getByRole("dialog", { name: "Review agent proposal" });
const rowsOff = () => document.querySelectorAll("tr[data-proposal-row].off").length;
const reviews = () => calls.filter((c) => c.cmd === "proposal_review").map((c) => c.args.steps);

describe("proposal review dialog", () => {
  it("shows EAK's review grouped by object with ERC/DRC before and after, and applies only the reviewed steps", async () => {
    const onApplied = vi.fn();
    render(<ProposalReviewDialog revision={4} busy={false} onApplied={onApplied} onClose={() => {}} />);
    await waitFor(() => expect(document.querySelectorAll("tr[data-proposal-row]").length).toBe(4));
    expect(dialog().querySelector('tbody[aria-label="Part (2)"]')).not.toBeNull();
    expect(dialog().querySelector('tbody[aria-label="Net (1)"]')).not.toBeNull();
    expect(dialog().querySelector('tbody[aria-label="Rule (1)"]')).not.toBeNull();
    const erc = document.querySelector('tr[data-check="ERC"]')!;
    expect(erc.textContent).toContain("0 → 3");
    expect(erc.textContent).toContain("unverified");
    expect(document.querySelector('tr[data-check="DRC"]')).not.toBeNull();
    expect(dialog().textContent).toContain("assumed (agent's claim)");
    expect(reviews()).toEqual([null]);

    // Unchecking the part unchecks the label that names it; EAK reviews the remaining step.
    fireEvent.click(screen.getByLabelText("Apply step 1 Add Symbol C1"));
    expect(rowsOff()).toBe(3);
    await waitFor(() => expect(reviews()).toEqual([null, [2]]));
    await screen.findByText(/Applying 1 of 3 step\(s\) as one undo step/);
    await waitFor(() => expect(document.querySelector('tr[data-check="ERC"]')!.textContent).toContain("0 → 1"));

    fireEvent.click(screen.getByRole("button", { name: "Export report" }));
    await screen.findByText(/Report saved: \/p\/evidence\/proposal-prop-1/);
    expect(calls.find((c) => c.cmd === "proposal_export")!.args).toEqual({ id: "prop-1", steps: [2] });

    const apply = screen.getByRole("button", { name: "Apply proposal" });
    await waitFor(() => expect((apply as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(apply);
    await waitFor(() => expect(onApplied).toHaveBeenCalledOnce());
    expect(calls.find((c) => c.cmd === "proposal_apply")!.args).toEqual({ id: "prop-1", expected_revision: 4, steps: [2] });
  });

  it("Apply is refused while nothing is checked, and Discard asks to confirm", async () => {
    render(<ProposalReviewDialog revision={4} busy={false} onApplied={() => {}} onClose={() => {}} />);
    await waitFor(() => expect(document.querySelectorAll("tr[data-proposal-row]").length).toBe(4));
    fireEvent.click(screen.getByLabelText("Apply step 1 Add Symbol C1"));
    fireEvent.click(screen.getByLabelText("Apply step 3 Update Design rules Design rules"));
    expect(rowsOff()).toBe(4);
    await screen.findByText(/No step checked/);
    expect((screen.getByRole("button", { name: "Apply proposal" }) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(screen.getByRole("button", { name: "Discard proposal" }));
    expect(calls.some((c) => c.cmd === "proposal_discard")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Confirm discard" }));
    await screen.findByText(/Proposal discarded/);
    expect(calls.find((c) => c.cmd === "proposal_discard")!.args).toEqual({ id: "prop-1" });
    expect(calls.filter((c) => c.cmd === "proposal_apply")).toEqual([]);
  });

  it("a stale proposal shows why and cannot be applied", async () => {
    stale = true;
    render(<ProposalReviewDialog revision={5} busy={false} onApplied={() => {}} onClose={() => {}} />);
    await screen.findByText(/the project is at revision 5 but the proposal was made at revision 4/);
    expect((screen.getByRole("button", { name: "Apply proposal" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("with no proposals it says how to make one and offers nothing to apply", async () => {
    list = [];
    render(<ProposalReviewDialog revision={0} busy={false} onApplied={() => {}} onClose={() => {}} />);
    await screen.findByText(/No agent proposals in this project/);
    expect(screen.queryByRole("button", { name: "Apply proposal" })).toBeNull();
    expect(calls.map((c) => c.cmd)).toEqual(["proposal_list"]);
  });
});
