// @vitest-environment jsdom
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "./api";
import { LibraryPanel } from "./shell/LibraryPanel";
import type { ComponentDef, LibraryPart } from "./types";

const def = (id: string, name: string, extra: Partial<ComponentDef> = {}): ComponentDef => ({
  id,
  name,
  category: "Resistor",
  refdes_prefix: "R",
  default_value: "10k",
  manufacturer: null,
  mpn: null,
  lcsc: null,
  datasheet: null,
  symbol: { pins: [{ number: "1", name: "~", electrical_type: "passive", position: { x: -5_000_000, y: 0 } }, { number: "2", name: "~", electrical_type: "passive", position: { x: 5_000_000, y: 0 } }], body_half_w: 2_000_000, body_half_h: 1_000_000 },
  footprint: { name: "R_0603", pads: [{ number: "1", kind: "smd", shape: "rect", position: { x: -800_000, y: 0 }, size_x: 800_000, size_y: 900_000, drill: null }, { number: "2", kind: "smd", shape: "rect", position: { x: 800_000, y: 0 }, size_x: 800_000, size_y: 900_000, drill: null }], courtyard_half_w: 1_500_000, courtyard_half_h: 800_000, body_height: null },
  pin_map: { "1": ["1"], "2": ["2"] },
  ratings: [],
  provenance: "Generic EAK land pattern; not datasheet-verified",
  ...extra,
});
const part = (d: ComponentDef, source: "builtin" | "project", in_project: boolean): LibraryPart => ({ id: d.id, source, in_project, def: d, unverified_ratings: 0 });

let searches: unknown[];
beforeEach(() => {
  searches = [];
  mockIPC((cmd, payload) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    const a = (payload ?? {}) as Record<string, string>;
    if (cmd === "library_search") {
      searches.push([a.query, a.category, a.source]);
      const all = [part(def("R_0603", "Resistor 0603"), "builtin", true), part(def("LED_0603", "LED 0603", { category: "LED" }), "builtin", false)];
      return all.filter((p) => (!a.query || p.def.name.toLowerCase().includes(a.query.toLowerCase())) && (!a.category || p.def.category === a.category));
    }
    if (cmd === "library_footprint_candidates") return [{ name: "R_0805", donor: "R_0805", footprint: { ...def("x", "x").footprint, name: "R_0805" }, pin_map: { "1": ["1"], "2": ["2"] } }];
    throw new Error(`unmocked ${cmd}`);
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
});

function mount() {
  const execute = vi.fn(async (_c: unknown) => undefined);
  const onPlace = vi.fn();
  render(<LibraryPanel revision={1} busy={false} execute={execute} onPlace={onPlace} />);
  return { execute, onPlace };
}

describe("LibraryPanel", () => {
  it("searches through the engine IPC and filters by category", async () => {
    mount();
    await screen.findByText("LED 0603");
    fireEvent.change(screen.getByLabelText("Search library"), { target: { value: "led" } });
    await waitFor(() => expect(screen.queryByText("Resistor 0603")).toBeNull());
    expect(searches).toContainEqual(["led", "", "all"]);
    fireEvent.change(screen.getByLabelText("Search library"), { target: { value: "" } });
    fireEvent.change(screen.getByLabelText("Category filter"), { target: { value: "Resistor" } });
    await waitFor(() => expect(screen.queryByText("LED 0603")).toBeNull());
  });

  it("previews symbol and footprint and shows provenance and the missing licence field honestly", async () => {
    mount();
    fireEvent.click(await screen.findByText("Resistor 0603"));
    expect(screen.getByRole("img", { name: /Symbol preview of Resistor 0603/ })).toBeTruthy();
    expect(screen.getByRole("img", { name: /Footprint preview of R_0603/ })).toBeTruthy();
    expect(screen.getByTestId("lib-provenance").textContent).toBe("Generic EAK land pattern; not datasheet-verified");
    expect(screen.getByText(/no licence field/i)).toBeTruthy();
  });

  it("places through the place tool, imports through ImportLibraryComponent", async () => {
    const { execute, onPlace } = mount();
    fireEvent.click(await screen.findByText("LED 0603"));
    fireEvent.click(screen.getByRole("button", { name: "Place on schematic" }));
    expect(onPlace).toHaveBeenCalledWith({ id: "LED_0603", name: "LED 0603" });
    fireEvent.click(screen.getByRole("button", { name: "Add to project" }));
    expect(execute).toHaveBeenCalledWith({ type: "ImportLibraryComponent", component_id: "LED_0603" });
    expect(screen.queryByLabelText("MPN")).toBeNull();
  });

  it("edits MPN / manufacturer / LCSC as one typed command, blank clears", async () => {
    const { execute } = mount();
    fireEvent.click(await screen.findByText("Resistor 0603"));
    const save = screen.getByRole("button", { name: "Save properties" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("MPN"), { target: { value: " RC0603FR-0710KL " } });
    fireEvent.change(screen.getByLabelText("Manufacturer"), { target: { value: "Yageo" } });
    fireEvent.change(screen.getByLabelText("LCSC id"), { target: { value: "C25804" } });
    fireEvent.click(save);
    expect(execute).toHaveBeenCalledWith({ type: "SetComponentProperties", component_id: "R_0603", manufacturer: "Yageo", mpn: "RC0603FR-0710KL", lcsc: "C25804", datasheet: null });
  });

  it("assigns a validated footprint candidate through AssignComponentFootprint", async () => {
    const { execute } = mount();
    fireEvent.click(await screen.findByText("Resistor 0603"));
    const sel = await screen.findByLabelText("Assign footprint");
    await screen.findByRole("option", { name: /R_0805/ });
    fireEvent.change(sel, { target: { value: "R_0805" } });
    fireEvent.click(screen.getByRole("button", { name: "Assign" }));
    const c = execute.mock.calls[0][0] as { type: string; component_id: string; footprint: { name: string }; pin_map: unknown };
    expect(c.type).toBe("AssignComponentFootprint");
    expect(c.component_id).toBe("R_0603");
    expect(c.footprint.name).toBe("R_0805");
    expect(c.pin_map).toEqual({ "1": ["1"], "2": ["2"] });
  });
});
