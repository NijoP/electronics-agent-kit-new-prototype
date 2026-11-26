// Problems tab (report §10 S3) and journey bar (S2) against a mocked engine IPC:
//  * collectProblems: ERC/DRC findings, UNKNOWN rules naming the missing input (once, from the
//    session report or the release gate), release blockers, unfinished agent tasks, stale rows;
//  * the panel's source, severity and text filters;
//  * in the whole app: a Problems row goes through the F7 navigation (the finding's sheet opens with
//    the part selected) while the Problems tab stays shown, and the Findings tab is unchanged;
//    journey stages are coloured from the reports and a click opens the stage's view.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentTask } from "../agentTasks";
import { IPC_COMMANDS } from "../api";
import { App } from "../App";
import { fixture } from "../fixture.test-data";
import { shellCmd } from "../shell.test-data";
import type { CheckResult, Finding, ReleaseCheck, RuleResult, Snapshot } from "../types";
import { collectProblems, filterProblems, NO_FILTER, uncheckedChecks, type Problem } from "./problems";
import { ProblemsPanel } from "./ProblemsPanel";
import { forgetViewports } from "./viewState";

const pt = (x: number, y: number) => ({ x: x * 1_000_000, y: y * 1_000_000 });

const report = (check: "ERC" | "DRC", revision: number, findings: Finding[], rules: RuleResult[] = []): CheckResult => ({
  report: {
    check,
    project_id: "prj_1",
    revision,
    input_sha256: "ab",
    tool: "eak-validation",
    findings,
    outcome: findings.some((f) => f.severity === "error") ? "ERRORS" : "NO_ERRORS_IN_IMPLEMENTED_CHECKS",
    errors: findings.filter((f) => f.severity === "error").length,
    warnings: findings.filter((f) => f.severity === "warning").length,
    not_implemented: [],
    limitations: [],
    rule_results: rules,
    overall: rules.some((r) => r.state === "UNKNOWN") ? "UNKNOWN" : "FAIL",
  },
  evidence_path: null,
});
const ercD1: Finding = { rule_id: "ERC-001", severity: "error", message: "D1 pin 1 is unconnected", objects: ["sym_d1"], location: null };
const unknownSig: RuleResult = { rule_id: "DRC-002", state: "UNKNOWN", reason: "no declared voltage for: SIG" };
const releaseAt = (revision: number, blocking: string[]): ReleaseCheck => ({ revision, input_sha256: "ab", gates: [], blocking, status: "DRAFT", not_verified: [] });
const task = (over: Partial<AgentTask>): AgentTask => ({
  id: "tsk_1",
  project_id: "prj_1",
  session_id: "ses_1",
  agent: "claude",
  title: "Route the 5 V rail",
  starting_revision: 5,
  revision: 7,
  phase: "routing",
  detail: null,
  started_ms: 1,
  updated_ms: 2,
  ended_ms: null,
  last_erc_errors: null,
  last_drc_errors: null,
  history: [{ phase: "routing", revision: 7, at_ms: 2, note: null }],
  ...over,
});

describe("collectProblems", () => {
  it("lists findings, UNKNOWN rules with their missing input, release blockers and unfinished agent tasks", () => {
    const list = collectProblems({
      snapshot: fixture(7),
      results: { erc: report("ERC", 7, [{ ...ercD1, objects: ["fp_d1"] }]), drc: report("DRC", 7, [], [unknownSig]) },
      release: releaseAt(7, [
        "DRC: DRC-002 is UNKNOWN: no declared voltage for: SIG", // already listed from the session's DRC report
        "MECH: MECH-001 is UNKNOWN: no component height limit declared",
        "MECH: MECH-002 is UNKNOWN: 2 part(s) have no 3D body data",
        "ERC reports 1 error(s)", // the ERC findings themselves are listed
        "board outline: the board has no closed outline",
      ]),
      agentTasks: [task({}), task({ id: "tsk_2", title: "Done task", phase: "completed", ended_ms: 3 }), task({ id: "tsk_3", title: "Left half done", phase: "partial", ended_ms: 3 })],
    });
    expect(list.map((p) => [p.source, p.severity, p.code, p.message])).toEqual([
      ["erc", "error", "ERC-001", "D1 pin 1 is unconnected"],
      ["release", "error", null, "board outline: the board has no closed outline"],
      ["unknown", "warning", "DRC-002", "DRC DRC-002 is UNKNOWN, never passing"],
      ["unknown", "warning", "MECH-001", "MECH MECH-001 is UNKNOWN, never passing"],
      ["unknown", "warning", "MECH-002", "MECH MECH-002 is UNKNOWN, never passing"],
      ["agent", "warning", "claude", "Left half done"],
      ["agent", "info", "claude", "Route the 5 V rail"],
    ]);
    const by = (code: string) => list.find((p) => p.code === code)!;
    expect(by("DRC-002").detail).toBe("Missing input: no declared voltage for: SIG");
    expect(by("DRC-002").go).toEqual({ kind: "rules" });
    expect(by("MECH-001").go).toEqual({ kind: "rules" });
    expect(by("MECH-002").go).toEqual({ kind: "assets" });
    expect(by("ERC-001").go).toMatchObject({ kind: "finding", check: "erc" });
    expect(list.find((p) => p.source === "release")!.go).toEqual({ kind: "release" });
    expect(list.find((p) => p.message === "Route the 5 V rail")!.go).toEqual({ kind: "editor", editor: "pcb" });
  });

  it("marks a stale report's rows, reads no release check of another revision, and names unchecked checks", () => {
    const list = collectProblems({ snapshot: fixture(8), results: { erc: report("ERC", 7, [ercD1]) }, release: releaseAt(7, ["ERC report not supplied"]), agentTasks: [] });
    expect(list).toHaveLength(1);
    expect(list[0].stale).toBe(true);
    expect(list[0].detail).toContain("re-run ERC");
    expect(uncheckedChecks({ erc: report("ERC", 7, []) }, 8)).toEqual(["ERC", "DRC", "DFM"]);
    expect(uncheckedChecks({ erc: report("ERC", 8, []) }, 8)).toEqual(["DRC", "DFM"]);
  });

  it("a finding with no object or location in the revision has no go-to, with the reason", () => {
    const [p] = collectProblems({ snapshot: fixture(7), results: { erc: report("ERC", 7, [{ ...ercD1, objects: ["sym_gone"] }]) }, release: null, agentTasks: [] });
    expect(p.go).toBeNull();
    expect(p.noGo).toBe("This finding names no object or location in the current revision");
  });
});

describe("Problems panel filters", () => {
  const list = collectProblems({
    snapshot: fixture(7),
    results: { erc: report("ERC", 7, [{ ...ercD1, objects: ["fp_d1"] }, { rule_id: "ERC-009", severity: "warning", message: "net-level note", objects: ["fp_r1"], location: null }]) },
    release: releaseAt(7, ["MECH: MECH-001 is UNKNOWN: no component height limit declared"]),
    agentTasks: [task({})],
  });
  const rows = () => [...document.querySelectorAll("li.problem")].map((li) => li.getAttribute("data-source"));
  afterEach(cleanup);

  it("filterProblems keeps rows by source, severity and text", () => {
    expect(filterProblems(list, NO_FILTER)).toHaveLength(4);
    expect(filterProblems(list, { ...NO_FILTER, hiddenSources: ["erc"] }).map((p) => p.source)).toEqual(["unknown", "agent"]);
    expect(filterProblems(list, { ...NO_FILTER, hiddenSeverities: ["warning", "info"] }).map((p) => p.code)).toEqual(["ERC-001"]);
    expect(filterProblems(list, { ...NO_FILTER, text: "height limit" }).map((p) => p.code)).toEqual(["MECH-001"]);
  });

  it("chips and the text field filter the rows, say how many are hidden, and a row calls go-to", () => {
    const onGo = vi.fn<(p: Problem) => void>();
    render(<ProblemsPanel problems={list} revision={7} unchecked={["DRC", "DFM"]} onGo={onGo} onCheck={() => {}} onRelease={() => {}} busy={false} />);
    const panel = within(screen.getByRole("region", { name: "Problems" }));
    expect(rows()).toEqual(["erc", "erc", "unknown", "agent"]);
    expect(panel.getByText(/Not run for r7: DRC, DFM/)).toBeTruthy();
    fireEvent.click(within(panel.getByRole("group", { name: "Problem sources" })).getByRole("button", { name: /^ERC/ }));
    expect(rows()).toEqual(["unknown", "agent"]);
    expect(panel.getByText("2 of 4 problem(s) hidden by the filter.")).toBeTruthy();
    fireEvent.click(within(panel.getByRole("group", { name: "Problem sources" })).getByRole("button", { name: /^ERC/ }));
    fireEvent.click(within(panel.getByRole("group", { name: "Problem severity" })).getByRole("button", { name: /^Info/ }));
    expect(rows()).toEqual(["erc", "erc", "unknown"]);
    fireEvent.change(panel.getByRole("searchbox", { name: "Filter problems" }), { target: { value: "erc-009" } });
    expect(rows()).toEqual(["erc"]);
    fireEvent.click(panel.getByRole("button", { name: /net-level note/ }));
    expect(onGo).toHaveBeenCalledWith(expect.objectContaining({ code: "ERC-009" }));
  });

  it("Run ERC / Run DRC are disabled with the reason while an operation runs", () => {
    render(<ProblemsPanel problems={[]} revision={7} unchecked={[]} onGo={() => {}} onCheck={() => {}} onRelease={() => {}} busy={true} />);
    const run = screen.getByRole("button", { name: "Run ERC" }) as HTMLButtonElement;
    expect(run.disabled).toBe(true);
    expect(run.title).toBe("Wait for the running operation to finish");
    expect(screen.getByText(/No problems in the reports of r7/).textContent).toContain("not proof of a correct design");
  });
});

/** The F7 fixture: D1 is on a second sheet. */
function twoSheets(): Snapshot {
  const s = fixture(7);
  const sch = s.project.schematic;
  sch.sheets.sht_2 = { id: "sht_2", name: "Sheet 2", width: 297_000_000, height: 210_000_000 };
  sch.symbols.sym_d1.sheet_id = "sht_2";
  s.geometry.symbols.find((g) => g.id === "sym_d1")!.sheet_id = "sht_2";
  sch.wires.wir_2 = { id: "wir_2", sheet_id: "sht_2", a: pt(60, 60), b: pt(70, 60) };
  return s;
}

describe("Problems and journey bar in the workspace", () => {
  beforeEach(() => {
    localStorage.clear();
    forgetViewports();
    mockIPC((cmd) => {
      if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
      switch (cmd) {
        case "recent_projects":
          return [{ path: "/p.eak", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
        case "open_project":
        case "current_project":
          return twoSheets();
        case "events_since":
        case "library_components":
        case "model_providers":
          return [];
        case "netlist":
          return [{ name: "GND", pins: ["D1.1"], label_names: ["GND"], has_power_port: true }];
        case "run_check":
          return report("ERC", 7, [ercD1]);
        case "release_check":
          return releaseAt(7, ["DRC: DRC-002 is UNKNOWN: no declared voltage for: SIG", "MECH report not supplied"]);
        case "agent_tasks":
          return [task({})];
        default: {
          const h = shellCmd(cmd, null);
          if (h.handled) return h.value;
          throw new Error(`unmocked ${cmd}`);
        }
      }
    });
  });
  afterEach(() => {
    cleanup();
    clearMocks();
    localStorage.clear();
  });

  const sheetTab = (name: string) => within(screen.getByRole("tablist", { name: "Documents" })).queryByRole("tab", { name: new RegExp(name) })?.getAttribute("aria-selected") ?? null;
  const journey = () => within(screen.getByRole("navigation", { name: "Design journey" }));
  const stage = (id: string) => document.querySelector(`nav.journey button[data-stage="${id}"]`) as HTMLButtonElement;
  async function open() {
    render(<App />);
    fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
    await screen.findAllByRole("img", { name: "Schematic canvas" });
    await waitFor(() => expect(sheetTab("Sheet 1")).toBe("true"));
  }

  it("a Problems row goes to the finding's sheet with the part selected (F7) and the Problems tab stays", async () => {
    await open();
    fireEvent.click(screen.getByRole("tab", { name: /^Problems/ }));
    const panel = within(await screen.findByRole("region", { name: "Problems" }));
    // Before ERC: the release gate's UNKNOWN rule and blocker, and the agent task, are listed.
    expect(await panel.findByText("Missing input: no declared voltage for: SIG")).toBeTruthy();
    expect(panel.getByRole("button", { name: /MECH report not supplied/ })).toBeTruthy();
    expect(panel.getByRole("button", { name: /Route the 5 V rail/ })).toBeTruthy();
    expect(stage("erc").getAttribute("data-state")).toBe("open");
    fireEvent.click(panel.getByRole("button", { name: "Run ERC" }));
    fireEvent.click(await panel.findByRole("button", { name: /D1 pin 1 is unconnected/ }));
    await waitFor(() => expect(sheetTab("Sheet 2")).toBe("true"));
    await waitFor(() => expect(document.querySelector('[data-symbol-id="sym_d1"].selected')).not.toBeNull());
    expect(screen.getByRole("tab", { name: /^Problems/ }).getAttribute("aria-selected")).toBe("true");
    // The journey bar reads the current ERC report: the stage now has errors.
    await waitFor(() => expect(stage("erc").getAttribute("data-state")).toBe("fail"));
    expect(stage("erc").title).toBe("ERC: 1 error at r7");
    // The Findings tab is unchanged: the same report, with its own panel.
    fireEvent.click(screen.getByRole("tab", { name: "Findings" }));
    expect(await screen.findByRole("region", { name: "ERC report" })).toBeTruthy();
  });

  it("an UNKNOWN rule opens Design rules, a release blocker the release check, and journey stages open their views", async () => {
    await open();
    fireEvent.click(screen.getByRole("tab", { name: /^Problems/ }));
    const panel = within(await screen.findByRole("region", { name: "Problems" }));
    fireEvent.click(await panel.findByRole("button", { name: /DRC-002 is UNKNOWN/ }));
    await screen.findByRole("dialog", { name: /Design rules/ });
    fireEvent.keyDown(screen.getByRole("dialog", { name: /Design rules/ }), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog", { name: /Design rules/ })).toBeNull());
    fireEvent.click(journey().getByRole("button", { name: /^Layout:/ }));
    await waitFor(() => expect(within(screen.getByRole("tablist", { name: "Documents" })).getByRole("tab", { name: /PCB/ }).getAttribute("aria-selected")).toBe("true"));
    fireEvent.click(journey().getByRole("button", { name: /^Release:/ }));
    expect(await screen.findByRole("dialog", { name: "Release check" })).toBeTruthy();
    // Without a registered 3D view (vitest), the 3D stage says why it cannot open.
    expect(stage("3d").disabled).toBe(true);
    expect(stage("3d").title).toMatch(/^Unavailable: /);
  });
});
