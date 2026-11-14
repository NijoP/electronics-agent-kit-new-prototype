import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { DomainPanel } from "./DomainPanel";
import { domainReport } from "./domain.test-data";

afterEach(cleanup);

describe("DomainPanel", () => {
  it("shows ranked classes with reasons and the DESK maturity label", () => {
    render(<DomainPanel report={domainReport(3)} revision={3} />);
    const list = screen.getByRole("list", { name: "Top domain classes" });
    expect(within(list).getByText("ESP32 development board")).toBeTruthy();
    expect(within(list).getByText("DESK: OFFLINE TESTED")).toBeTruthy();
    expect(within(list).getByText("DESK: DRAFT")).toBeTruthy();
    expect(screen.getByText("keyword 'esp32' in project name (+6)")).toBeTruthy();
    expect(screen.getByText("input 24 V far outside 4.5-12 V (-2)")).toBeTruthy();
  });

  it("never presents guidance as a requirement below a human-verified label", () => {
    render(<DomainPanel report={domainReport(3)} revision={3} />);
    expect(screen.getByText(/Review prompts \(unverified\)/)).toBeTruthy();
    expect(screen.queryByText(/Requirements to confirm/)).toBeNull();
  });

  it("states what EAK does not check and marks a stale report", () => {
    render(<DomainPanel report={domainReport(3)} revision={5} />);
    expect(screen.getByText(/Stale: the design is at revision 5/)).toBeTruthy();
    expect(screen.getByText(/Not checked by EAK: DESK rule-set evaluation/)).toBeTruthy();
  });

  it("reports an unclassified design honestly", () => {
    const r = domainReport(1);
    r.classification = { ...r.classification, status: "UNCLASSIFIED", primary: null, candidates: [], guidance: null };
    render(<DomainPanel report={r} revision={1} />);
    expect(screen.getByText("Unclassified")).toBeTruthy();
    expect(screen.getByText(/No domain class scored/)).toBeTruthy();
  });

  it("surfaces an IPC failure instead of an empty panel", () => {
    render(<DomainPanel report={null} revision={1} error="no project is open" />);
    expect(screen.getByRole("alert").textContent).toContain("no project is open");
  });
});
