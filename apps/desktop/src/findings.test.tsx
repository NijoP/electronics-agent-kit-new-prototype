// Phase 1 B5: the Findings panel's severity filter chips, object references and report export.
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { EngineeringMonitor, objectRef, severityCounts } from "./EngineeringMonitor";
import { ercResult, fixture } from "./fixture.test-data";
import type { CheckResult } from "./types";

afterEach(cleanup);

function drcResult(): CheckResult {
  const r = ercResult(7);
  return {
    evidence_path: null,
    report: {
      ...r.report,
      check: "DRC",
      findings: [
        { id: "DRC-002-a", rule_id: "DRC-002", severity: "error", message: "track too close to via", objects: ["trk_1", "via_1"], location: null },
        { id: "DRC-012-b", rule_id: "DRC-012", severity: "warning", message: "dangling track end", objects: ["trk_1"], location: null },
        { id: "DRC-020-c", rule_id: "DRC-020", severity: "info", message: "zone note", objects: ["zone_gone"], location: null },
      ],
      errors: 1,
      warnings: 1,
    },
  };
}

const section = (title: string) => screen.getByRole("region", { name: `${title} report` });

describe("Findings severity filter", () => {
  it("counts findings per severity", () => {
    expect(severityCounts(drcResult().report.findings)).toEqual({ error: 1, warning: 1, info: 1 });
  });

  it("chips show counts and hide or show their severity", () => {
    render(<EngineeringMonitor snapshot={fixture(7)} results={{ drc: drcResult() }} onProbe={() => {}} />);
    const drc = within(section("DRC"));
    const filter = within(drc.getByRole("group", { name: "DRC severity filter" }));
    const errors = filter.getByRole("button", { name: /Errors/ });
    expect(errors.textContent).toBe("Errors 1");
    expect(filter.getByRole("button", { name: /Warnings/ }).textContent).toBe("Warnings 1");
    expect(filter.getByRole("button", { name: /Info/ }).textContent).toBe("Info 1");
    expect(errors.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(errors);
    expect(errors.getAttribute("aria-pressed")).toBe("false");
    expect(drc.queryByText(/track too close to via/)).toBeNull();
    expect(drc.getByText(/dangling track end/)).toBeTruthy();
    expect(drc.getByText(/1 of 3 finding\(s\) hidden by the severity filter/)).toBeTruthy();
    // Counts are of the report, not of what is shown.
    expect(errors.textContent).toBe("Errors 1");
    fireEvent.click(errors);
    expect(drc.getByText(/track too close to via/)).toBeTruthy();
    expect(drc.queryByText(/hidden by the severity filter/)).toBeNull();
  });

  it("each report has its own filter", () => {
    render(<EngineeringMonitor snapshot={fixture(7)} results={{ erc: ercResult(7), drc: drcResult() }} onProbe={() => {}} />);
    fireEvent.click(within(section("DRC")).getByRole("button", { name: /Warnings/ }));
    expect(within(section("DRC")).queryByText(/dangling track end/)).toBeNull();
    expect(within(section("ERC")).getByText(/net-level note/)).toBeTruthy();
  });
});

describe("Finding object references", () => {
  it("names objects by reference designator or kind and flags ids not in the revision", () => {
    const s = fixture(7);
    expect(objectRef(s, "sym_r1")).toEqual({ label: "R1", present: true });
    expect(objectRef(s, "fp_d1")).toEqual({ label: "D1", present: true });
    expect(objectRef(s, "via_1")).toEqual({ label: "Via", present: true });
    expect(objectRef(s, "trk_1")).toEqual({ label: "Track N1", present: true });
    expect(objectRef(s, "net_x")).toEqual({ label: "net_x", present: false });
  });

  it("a click on an object reference navigates to that one object", () => {
    const onObject = vi.fn();
    const onNavigate = vi.fn();
    render(<EngineeringMonitor snapshot={fixture(7)} results={{ drc: drcResult() }} onProbe={() => {}} onNavigate={onNavigate} onObject={onObject} />);
    const drc = within(section("DRC"));
    const first = drc.getByText(/track too close to via/).closest("li")!;
    const refs = within(within(first).getByRole("group", { name: "Objects" })).getAllByRole("button");
    expect(refs.map((b) => b.textContent)).toEqual(["Track N1", "Via"]);
    fireEvent.click(refs[1]);
    expect(onObject).toHaveBeenCalledTimes(1);
    expect(onObject.mock.calls[0][1]).toBe("drc");
    expect(onObject.mock.calls[0][2]).toBe("via_1");
    expect(onObject.mock.calls[0][0].id).toBe("DRC-002-a");
    expect(onNavigate).not.toHaveBeenCalled();
    // An object the current revision does not hold cannot be navigated to, and says why.
    const gone = within(drc.getByText(/zone note/).closest("li")!).getByRole("button", { name: "zone_gone" }) as HTMLButtonElement;
    expect(gone.disabled).toBe(true);
    expect(gone.title).toMatch(/not an object in revision 7/);
  });
});

describe("Findings export", () => {
  it("offers CSV and JSON per report and sends the check and format", () => {
    const onExport = vi.fn();
    render(<EngineeringMonitor snapshot={fixture(7)} results={{ erc: ercResult(7), drc: drcResult() }} onProbe={() => {}} onExport={onExport} exportBlocked={null} />);
    fireEvent.click(within(section("ERC")).getByRole("button", { name: "Export CSV" }));
    fireEvent.click(within(section("DRC")).getByRole("button", { name: "Export JSON" }));
    expect(onExport.mock.calls).toEqual([
      ["erc", "csv"],
      ["drc", "json"],
    ]);
  });

  it("is disabled with the reason when it cannot run", () => {
    const onExport = vi.fn();
    render(<EngineeringMonitor snapshot={fixture(7)} results={{ erc: ercResult(7) }} onProbe={() => {}} onExport={onExport} exportBlocked="Save the project to a folder first" />);
    const csv = within(section("ERC")).getByRole("button", { name: "Export CSV" }) as HTMLButtonElement;
    expect(csv.disabled).toBe(true);
    expect(csv.title).toBe("Save the project to a folder first");
    // A check that was not run has nothing to export and offers no export.
    expect(within(section("DRC")).queryByRole("button", { name: /Export/ })).toBeNull();
  });
});

describe("Real-time DRC toggle", () => {
  it("is a checkbox that reports its new state, shown only when the workspace offers it", () => {
    const onRealtimeDrc = vi.fn();
    const { rerender } = render(<EngineeringMonitor snapshot={fixture(7)} results={{}} onProbe={() => {}} realtimeDrc={false} onRealtimeDrc={onRealtimeDrc} />);
    const box = screen.getByRole("checkbox", { name: "Real-time DRC" }) as HTMLInputElement;
    expect(box.checked).toBe(false);
    fireEvent.click(box);
    expect(onRealtimeDrc).toHaveBeenCalledWith(true);
    rerender(<EngineeringMonitor snapshot={fixture(7)} results={{}} onProbe={() => {}} />);
    expect(screen.queryByRole("checkbox", { name: "Real-time DRC" })).toBeNull();
  });
});
