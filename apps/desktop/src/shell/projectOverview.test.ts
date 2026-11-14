// Project overview derivations (written during the testing freeze; not yet run).
import { describe, expect, it } from "vitest";
import { fixture } from "../fixture.test-data";
import type { CheckResult } from "../types";
import { checkState, designFacts } from "./ProjectOverview";

const result = (errors: number, warnings: number, revision: number): CheckResult =>
  ({ evidence_path: null, report: { check: "drc", project_id: "p", revision, input_sha256: "", tool: "", findings: [], outcome: errors ? "ERRORS" : "NO_ERRORS_IN_IMPLEMENTED_CHECKS", errors, warnings, not_implemented: [], limitations: [] } }) as CheckResult;

describe("project overview", () => {
  it("never reports an unrun check as passing", () => {
    expect(checkState(undefined, 3)).toEqual({ tone: "neutral", text: "Not run" });
  });
  it("marks errors, warnings and stale reports", () => {
    expect(checkState(result(2, 1, 3), 3).tone).toBe("bad");
    expect(checkState(result(0, 1, 3), 3).tone).toBe("warn");
    expect(checkState(result(0, 0, 3), 3)).toEqual({ tone: "ok", text: "No errors in implemented checks" });
    expect(checkState(result(0, 0, 2), 3)).toEqual({ tone: "neutral", text: "No errors in implemented checks · r2, stale" });
  });
  it("derives counts from the snapshot only", () => {
    const s = fixture(1);
    const f = designFacts(s);
    expect(f.parts).toBe(Object.keys(s.project.schematic.symbols).length);
    expect(f.footprints).toBe(Object.keys(s.project.pcb.footprints).length);
    expect(f.missingFootprints).toBeGreaterThanOrEqual(0);
  });
});
