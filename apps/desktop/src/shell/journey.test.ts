// Journey bar state mapping (report §10 S2): a stage is done only from a current, non-stale fact
// or report; a stale report, a check not run or an UNKNOWN rule is never done, and "has errors"
// needs a current report with errors. Pure, against the shared fixture.
import { describe, expect, it } from "vitest";
import { ercResult, fixture } from "../fixture.test-data";
import type { CheckResult, ConnectivityReport, ReleaseCheck, ReleaseGate, RuleResult } from "../types";
import { checkStage, journeyStages, nextStage, STAGE_LABELS, type JourneyFacts, type StageId } from "./journey";

const facts = (over: Partial<JourneyFacts> = {}): JourneyFacts => ({ snapshot: fixture(7), results: {}, conn: null, release: null, outputsCurrent: null, ...over });
const state = (f: JourneyFacts, id: StageId) => journeyStages(f).find((s) => s.id === id)!;

const clean = (check: "ERC" | "DRC", revision = 7, rules: RuleResult[] = [{ rule_id: `${check}-001`, state: "PASS", reason: "" }]): CheckResult => ({
  report: { check, project_id: "prj_1", revision, input_sha256: "ab", tool: "eak-validation", findings: [], outcome: "NO_ERRORS_IN_IMPLEMENTED_CHECKS", errors: 0, warnings: 0, not_implemented: [], limitations: [], rule_results: rules, overall: rules.some((r) => r.state === "UNKNOWN") ? "UNKNOWN" : "PASS" },
  evidence_path: null,
});
const conn = (revision: number, over: Partial<ConnectivityReport> = {}): ConnectivityReport => ({ revision, net_groups: {}, shorts: [], ratsnest: [], ...over });
const gate = (id: string, state: ReleaseGate["state"], reasons: string[] = []): ReleaseGate => ({ id, title: id, state, required: true, reasons, evidence: [] });
const release = (revision: number, gates: ReleaseGate[], status: ReleaseCheck["status"] = "DRAFT"): ReleaseCheck => ({ revision, input_sha256: "ab", gates, blocking: [], status, not_verified: [] });

describe("journey stages", () => {
  it("has the vision's ten stages in journey order", () => {
    expect(journeyStages(facts()).map((s) => s.label)).toEqual(["Requirements", "Parts", "Schematic", "ERC", "Layout", "Routing", "DRC", "3D", "Outputs", "Release"]);
    expect(STAGE_LABELS).toHaveLength(10);
  });

  it("with no report and nothing read, no check-based stage is done and none claims errors", () => {
    const st = journeyStages(facts());
    for (const id of ["erc", "routing", "drc", "3d", "outputs", "release"] as StageId[]) expect(st.find((s) => s.id === id)!.state, id).toBe("open");
    expect(state(facts(), "erc").reason).toBe("ERC not run for r7");
    expect(state(facts(), "release").reason).toBe("Release check not read for r7 yet");
  });

  it("ERC: errors in a current report are 'fail'; the same report once stale is only 'open', never done or failing", () => {
    expect(state(facts({ results: { erc: ercResult(7) } }), "erc").state).toBe("fail");
    const stale = state(facts({ snapshot: fixture(8), results: { erc: ercResult(7) } }), "erc");
    expect(stale.state).toBe("open");
    expect(stale.reason).toContain("stale");
  });

  it("ERC/DRC: done needs a current report with overall PASS; an UNKNOWN rule or a warning keeps it open", () => {
    expect(checkStage(clean("DRC"), 7, "DRC").state).toBe("done");
    expect(checkStage(clean("DRC", 6), 7, "DRC").state).toBe("open");
    const unknown = checkStage(clean("DRC", 7, [{ rule_id: "DRC-002", state: "UNKNOWN", reason: "no declared voltage for: SIG" }]), 7, "DRC");
    expect(unknown).toEqual({ state: "open", reason: "DRC: DRC-002 UNKNOWN at r7" });
    const warned = clean("ERC");
    warned.report.warnings = 1;
    expect(checkStage(warned, 7, "ERC").state).toBe("open");
    // A saved report that predates per-rule states (no overall) is not a pass.
    const old = clean("ERC");
    delete old.report.overall;
    expect(checkStage(old, 7, "ERC").state).toBe("open");
  });

  it("Requirements, Parts and Schematic come from the snapshot of this revision", () => {
    expect(state(facts(), "requirements").state).toBe("open");
    const s = fixture(7);
    s.project.requirements = [{ id: "req_1", text: "5 V at 3 A from 12-24 V" }];
    expect(state(facts({ snapshot: s }), "requirements")).toMatchObject({ state: "done", reason: "1 requirement recorded" });
    // The fixture's parts have no Distributor part: sourcing is not done.
    expect(state(facts(), "parts")).toMatchObject({ state: "open", reason: "2 of 2 parts have no Distributor part" });
    expect(state(facts(), "schematic").state).toBe("done");
  });

  it("Layout is a sync fact: a schematic part with no footprint keeps it open until Update PCB", () => {
    expect(state(facts(), "layout").state).toBe("done");
    const s = fixture(7);
    delete s.project.pcb.footprints.fp_d1;
    expect(state(facts({ snapshot: s }), "layout")).toMatchObject({ state: "open", reason: "1 of 2 parts not on the board: run Update PCB" });
    s.project.pcb.board.outline = [];
    s.project.pcb.footprints.fp_d1 = fixture(7).project.pcb.footprints.fp_d1;
    expect(state(facts({ snapshot: s }), "layout").reason).toBe("No board outline");
  });

  it("Routing reads only the connectivity of this revision: shorts fail, unrouted is open, a stale analysis is open", () => {
    expect(state(facts({ conn: conn(7) }), "routing").state).toBe("done");
    expect(state(facts({ conn: conn(6) }), "routing")).toMatchObject({ state: "open", reason: "Connectivity not analysed for r7" });
    expect(state(facts({ conn: conn(7, { ratsnest: [["N1", { x: 0, y: 0 }, { x: 1, y: 1 }]] }) }), "routing").reason).toBe("1 unrouted connection at r7");
    expect(state(facts({ conn: conn(7, { shorts: [["GND", "N1", "fp_r1", "fp_d1"]] }) }), "routing").state).toBe("fail");
  });

  it("3D and Release come from the release check of this revision only", () => {
    const pass = release(7, [gate("erc", "PASS"), gate("mech", "PASS")], "INTERNALLY_VERIFIED_RELEASE_CANDIDATE");
    expect(state(facts({ release: pass }), "3d").state).toBe("done");
    expect(state(facts({ release: pass }), "release").state).toBe("done");
    // The same check for an older revision is stale: neither stage is done.
    expect(state(facts({ snapshot: fixture(8), release: pass }), "release")).toMatchObject({ state: "open", reason: "Release check is for r7, stale: the project is at r8" });
    expect(state(facts({ snapshot: fixture(8), release: pass }), "3d").state).toBe("open");
    const draft = release(7, [gate("erc", "NOT_RUN"), gate("board_outline", "FAIL", ["no outline"]), gate("mech", "UNKNOWN", ["MECH: MECH-001 is UNKNOWN: no component height limit declared"])]);
    expect(state(facts({ release: draft }), "release")).toMatchObject({ state: "fail", reason: "1 of 3 required gates fail at r7" });
    expect(state(facts({ release: draft }), "3d")).toMatchObject({ state: "open", reason: "MECH: MECH-001 is UNKNOWN: no component height limit declared" });
    expect(state(facts({ releaseError: "no project is open" }), "release").reason).toBe("Release check unavailable: no project is open");
  });

  it("Outputs need an output folder of this revision on disk", () => {
    expect(state(facts({ outputsCurrent: true }), "outputs").state).toBe("done");
    expect(state(facts({ outputsCurrent: false }), "outputs").reason).toBe("No outputs exported for r7");
    const s = fixture(7);
    s.dir = null;
    expect(state(facts({ snapshot: s, outputsCurrent: true }), "outputs").state).toBe("open");
  });

  it("Next is the first stage that is not done", () => {
    expect(nextStage(journeyStages(facts()))?.id).toBe("requirements");
    expect(nextStage([{ id: "erc", label: "ERC", state: "done", reason: "" }])).toBeNull();
  });
});
