// M4.2/M4.3 (ADR-0060): the Requirements panel shows the record as the engine reports it (an
// unanswered field is UNKNOWN, never a number), and every control sends the engine command a
// person means: entering a value, keeping one of conflicting values, running the classifier and
// confirming or rejecting a class.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PERSON_KEY, RequirementsPanel, showValue } from "./RequirementsPanel";
import type { Command, ReqCompleteness, ReqFieldReport, RequirementsView } from "./types";

afterEach(() => {
  cleanup();
  localStorage.removeItem(PERSON_KEY);
});

const counts = (over: Partial<ReqCompleteness> = {}): ReqCompleteness => ({
  total: 2,
  unknown: 1,
  needs_confirmation: 0,
  confirmed: 1,
  derived: 0,
  not_applicable: 0,
  conflicts: 1,
  missing: 2,
  provisional: 0,
  firm: 0,
  percent_filled: 0,
  percent_firm: 0,
  ...over,
});

const client = { kind: "client" as const, reference: "kick-off call" };
const field = (over: Partial<ReqFieldReport> & { path: string; label: string }): ReqFieldReport => ({
  section: "supply",
  status: "UNKNOWN",
  level: "missing",
  recorded: false,
  material: true,
  na_allowed: false,
  ...over,
});

function view(): RequirementsView {
  const jack = { status: "NEEDS_CONFIRMATION" as const, value: { kind: "text" as const, text: "12 V barrel jack" }, source: client, owner: "Client PM", risk_if_wrong: "the input stage changes", updated_at_ms: 2, updated_revision: 2 };
  return {
    revision: 3,
    record: {
      fields: {
        "supply.sources": { status: "CONFIRMED", value: { kind: "text", text: "USB-C 5 V" }, source: client, owner: "Client PM", updated_at_ms: 1, updated_revision: 1, conflicts: [jack] },
      },
      items: {},
    },
    report: {
      completeness: counts(),
      sections: [{ section: "supply", label: "Supply", completeness: counts() }],
      fields: [
        field({ path: "supply.input_voltage", label: "Input voltage" }),
        field({ path: "supply.sources", label: "Power sources", status: "CONFIRMED", recorded: true, value: { kind: "text", text: "USB-C 5 V" }, source: client, owner: "Client PM", conflicts: [jack] }),
      ],
      operations: [{ operation: "power_sizing", label: "Power sizing", ready: false, blocked_by: [{ path: "supply.input_voltage", status: "UNKNOWN", level: "missing", needs: "provisional" }] }],
      questions: [],
      domain: [],
      issues: [],
      not_implemented: ["enforcement of readiness"],
      limitations: ["advisory only"],
    },
    classification: {
      evaluator: "eak-engineering domain classifier v1 (ADR-0060)",
      classes: [
        {
          id: "usb_interface",
          key: "domain.usb_interface",
          kind: "overlay",
          label: "USB interface",
          desk: "usb",
          verdict: "NEEDS_CONFIRMATION",
          reason: "the strong evidence is not firm yet",
          supports: [{ path: "supply.sources", label: "Power sources", observed: '"USB-C 5 V"', status: "CONFIRMED", level: "missing", strength: "strong", rule: "a power source is USB" }],
          contradicts: [],
        },
      ],
      not_indicated: [{ id: "high_voltage", key: "domain.high_voltage", kind: "overlay", label: "High voltage", unknown_inputs: ["safety.max_working_voltage"], contradicts: [] }],
      not_implemented: [],
      limitations: [],
    },
    recorded_up_to_date: false,
    catalogue: {
      fields: [
        { key: "supply.input_voltage", section: "supply", label: "Input voltage", kind: "number_or_range", measure: "voltage", na_allowed: false, material: true, question: "What input voltage range must the board accept?" },
        { key: "supply.sources", section: "supply", label: "Power sources", kind: "text", measure: null, na_allowed: false, material: true, question: "Where does power come from?" },
      ],
      rail_fields: [],
      interface_fields: [],
      domain_classes: [],
      units: [
        { symbol: "V", measure: "voltage" },
        { symbol: "mV", measure: "voltage" },
        { symbol: "A", measure: "current" },
      ],
    },
  };
}

function setup() {
  const onCommand = vi.fn(async (_c: Command) => true);
  const load = vi.fn(async () => view());
  const r = render(<RequirementsPanel revision={3} busy={false} onCommand={onCommand} load={load} />);
  const row = (path: string) => r.container.querySelector<HTMLElement>(`li[data-path="${path}"]`)!;
  const button = (scope: Element, text: string) => [...scope.querySelectorAll("button")].find((b) => b.textContent === text)!;
  return { ...r, onCommand, load, row, button };
}

describe("RequirementsPanel", () => {
  it("shows an unanswered field as UNKNOWN, never as a number, and what it blocks", async () => {
    const { row, container } = setup();
    await screen.findByLabelText("Completeness");
    const unknown = row("supply.input_voltage");
    expect(unknown.dataset.status).toBe("UNKNOWN");
    expect(unknown.textContent).toContain("no value: UNKNOWN");
    expect(unknown.textContent).not.toMatch(/\b0\s*V\b/);
    expect(container.querySelector('li[data-operation="power_sizing"]')!.textContent).toContain("supply.input_voltage (missing, needs provisional)");
    expect(screen.getByText(/High voltage: still UNKNOWN safety.max_working_voltage/)).toBeTruthy();
  });

  it("shows every conflicting value and resolves only once a person names themself", async () => {
    const { row, onCommand } = setup();
    await screen.findByLabelText("Completeness");
    const disputed = row("supply.sources");
    expect(disputed.dataset.conflicts).toBe("1");
    expect(disputed.textContent).toContain("12 V barrel jack");
    const keep1 = disputed.querySelector<HTMLButtonElement>('button[data-keep="1"]')!;
    expect(keep1.disabled).toBe(true);
    expect(keep1.title).toMatch(/Enter your name first/);
    fireEvent.change(screen.getByLabelText("Your name"), { target: { value: "A. Engineer" } });
    expect(keep1.disabled).toBe(false);
    fireEvent.click(keep1);
    await waitFor(() => expect(onCommand).toHaveBeenCalledWith({ type: "ResolveRequirementConflict", path: "supply.sources", keep: 1, owner: "A. Engineer", note: null }));
    expect(localStorage.getItem(PERSON_KEY)).toBe("A. Engineer");
  });

  it("enters a value as NEEDS_CONFIRMATION with its unit, source and risk, never forced over another", async () => {
    localStorage.setItem(PERSON_KEY, "A. Engineer");
    const { row, button, onCommand } = setup();
    await screen.findByLabelText("Completeness");
    fireEvent.click(button(row("supply.input_voltage"), "Enter"));
    fireEvent.change(screen.getByLabelText("Nominal"), { target: { value: "5" } });
    fireEvent.change(screen.getByLabelText("Risk if wrong"), { target: { value: "the regulator is re-selected" } });
    expect((screen.getByLabelText("Unit") as HTMLSelectElement).value).toBe("V");
    fireEvent.click(button(row("supply.input_voltage"), "Record"));
    await waitFor(() =>
      expect(onCommand).toHaveBeenCalledWith({
        type: "SetRequirementField",
        path: "supply.input_voltage",
        status: "NEEDS_CONFIRMATION",
        value: { kind: "range", min: undefined, nom: 5, max: undefined, unit: "V" },
        source: { kind: "engineer", reference: "entered in the Requirements panel" },
        derived_from: [],
        owner: "A. Engineer",
        risk_if_wrong: "the regulator is re-selected",
        note: null,
        supersede: false,
      }),
    );
  });

  it("runs the classifier and records a person's decision on a class", async () => {
    localStorage.setItem(PERSON_KEY, "A. Engineer");
    const { container, button, onCommand } = setup();
    await screen.findByLabelText("Completeness");
    fireEvent.click(screen.getByRole("button", { name: "Classify" }));
    await waitFor(() => expect(onCommand).toHaveBeenCalledWith({ type: "ClassifyDomain" }));
    const usb = container.querySelector<HTMLElement>('li[data-class="usb_interface"]')!;
    expect(usb.dataset.verdict).toBe("NEEDS_CONFIRMATION");
    expect(usb.textContent).toContain("a power source is USB");
    fireEvent.click(button(usb, "Reject"));
    await waitFor(() =>
      expect(onCommand).toHaveBeenLastCalledWith(
        expect.objectContaining({ type: "SetRequirementField", path: "domain.usb_interface", status: "CONFIRMED", value: { kind: "flag", value: false }, owner: "A. Engineer", supersede: true }),
      ),
    );
  });

  it("says when the record cannot be read instead of showing an empty record", async () => {
    const load = () => Promise.reject(new Error("no project is open"));
    render(<RequirementsPanel revision={1} busy={false} onCommand={vi.fn(async () => true)} load={load} />);
    expect((await screen.findByRole("alert")).textContent).toContain("no project is open");
  });

  it("formats values without inventing any", () => {
    expect(showValue(null)).toBeNull();
    expect(showValue({ kind: "range", min: 4.5, max: 5.5, unit: "V" })).toBe("4.5 / – / 5.5 V (min / nom / max)");
    expect(showValue({ kind: "flag", value: false })).toBe("no");
  });
});
