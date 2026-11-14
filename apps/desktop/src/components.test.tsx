import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandPalette } from "./CommandPalette";
import { EngineeringMonitor, probeTarget } from "./EngineeringMonitor";
import { ercResult, fixture } from "./fixture.test-data";
import { PcbView } from "./PcbView";
import { SchematicView } from "./SchematicView";
import type { Id } from "./types";

afterEach(cleanup);

function Views() {
  const [selected, setSelected] = useState<Id | null>(null);
  const s = fixture();
  return (
    <>
      <SchematicView snapshot={s} selected={selected} onSelect={setSelected} />
      <PcbView snapshot={s} selected={selected} onSelect={setSelected} />
      <EngineeringMonitor snapshot={s} results={{ erc: ercResult() }} onProbe={setSelected} />
    </>
  );
}

const sym = (c: HTMLElement, id: string) => c.querySelector(`[data-symbol-id="${id}"].symbol`)!;
const fp = (c: HTMLElement, id: string) => c.querySelector(`[data-footprint-id="${id}"]`)!;

describe("schematic and PCB views", () => {
  it("render EAK-IR geometry from the snapshot", () => {
    const { container } = render(<Views />);
    expect(container.querySelectorAll(".symbol")).toHaveLength(2);
    expect(container.querySelectorAll(".wire")).toHaveLength(1);
    expect(container.querySelectorAll(".label.power")).toHaveLength(1);
    expect(container.querySelectorAll(".footprint")).toHaveLength(2);
    expect(container.querySelectorAll(".track.top")).toHaveLength(1);
    expect(container.querySelectorAll(".via")).toHaveLength(1);
    expect(container.querySelectorAll(".zone.bottom")).toHaveLength(1);
    expect(container.querySelector(".board")!.getAttribute("points")).toBe("0,0 30,0 30,-20 0,-20");
    // Track width is drawn in millimetres.
    expect(container.querySelector(".track")!.getAttribute("stroke-width")).toBe("0.25");
  });

  it("cross-probes by symbol_id in both directions", () => {
    const { container } = render(<Views />);
    fireEvent.click(sym(container, "sym_r1"));
    expect(sym(container, "sym_r1").classList).toContain("selected");
    expect(fp(container, "fp_r1").classList).toContain("selected");
    expect(fp(container, "fp_d1").classList).not.toContain("selected");

    fireEvent.click(fp(container, "fp_d1"));
    expect(sym(container, "sym_d1").classList).toContain("selected");
    expect(sym(container, "sym_r1").classList).not.toContain("selected");
  });

  it("zooms on wheel without a layout (no NaN view box)", () => {
    render(<Views />);
    const canvas = screen.getByRole("img", { name: "PCB canvas" });
    const before = canvas.getAttribute("viewBox");
    fireEvent.wheel(canvas, { deltaY: 100 });
    expect(canvas.getAttribute("viewBox")).toBe(before);
    expect(before).not.toContain("NaN");
  });
});

describe("Engineering Monitor", () => {
  it("lists findings with not-implemented checks and cross-probes from a finding", () => {
    const { container } = render(<Views />);
    expect(screen.getByText(/ERC-001/)).toBeTruthy();
    expect(screen.getByText(/Not implemented \(1\)/)).toBeTruthy();
    expect(screen.getByText(/Limitations \(1\)/)).toBeTruthy();
    expect(screen.getByText(/DRC/, { selector: "h3" }).textContent).toBe("DRC");
    // DRC and DFM were not run: both say unknown, never passing.
    expect(screen.getAllByText(/unknown, not passing/)).toHaveLength(2);
    expect(screen.getByText(/DFM/, { selector: "h3" }).textContent).toBe("DFM");
    fireEvent.click(screen.getByText(/D1 pin 1/).closest("button")!);
    expect(sym(container, "sym_d1").classList).toContain("selected");
    // A finding without a symbol or footprint cannot be probed.
    expect(screen.getByText(/net-level note/).closest("button")!.disabled).toBe(true);
  });

  it("marks reports from an older revision as stale and never says pass", () => {
    render(<EngineeringMonitor snapshot={fixture(8)} results={{ erc: ercResult(7) }} onProbe={() => {}} />);
    expect(screen.getByText(/stale: project is at revision 8/)).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/\bpass(ed)?\b/i);
  });

  it("maps finding objects to the cross-probe symbol id", () => {
    const s = fixture();
    expect(probeTarget(s, ["sym_r1"])).toBe("sym_r1");
    expect(probeTarget(s, ["trk_1", "fp_d1"])).toBe("sym_d1");
    expect(probeTarget(s, ["nope"])).toBeNull();
  });
});

describe("command palette", () => {
  it("filters, navigates with the keyboard and runs the active action", () => {
    const a = vi.fn();
    const b = vi.fn();
    const onClose = vi.fn();
    render(
      <CommandPalette
        actions={[
          { id: "a", label: "Run ERC", run: a },
          { id: "b", label: "Run DRC", run: b },
          { id: "c", label: "Hidden", run: vi.fn(), disabled: true },
        ]}
        onClose={onClose}
      />,
    );
    const input = screen.getByRole("textbox", { name: "Search commands" });
    expect(document.activeElement).toBe(input);
    expect(screen.getAllByRole("option")).toHaveLength(2);
    fireEvent.change(input, { target: { value: "run" } });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(b).toHaveBeenCalledOnce();
    expect(a).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("confirms destructive commands before running them", () => {
    const run = vi.fn();
    const confirm = vi.fn().mockReturnValueOnce(false).mockReturnValueOnce(true);
    render(
      <CommandPalette actions={[{ id: "x", label: "Clear all routing", run, destructive: true }]} onClose={() => {}} confirm={confirm} />,
    );
    const input = screen.getByRole("textbox");
    fireEvent.keyDown(input, { key: "Enter" });
    expect(run).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: "Enter" });
    expect(run).toHaveBeenCalledOnce();
    expect(confirm).toHaveBeenCalledTimes(2);
  });
});
