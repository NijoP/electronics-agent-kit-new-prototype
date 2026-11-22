// Phase 1 A3 acceptance, in the whole app against a mocked engine IPC:
//  * spec UI-01: docks hide, resize and restore, and the layout persists per workspace;
//  * clicking a part or a net in the design manager selects and zooms it;
//  * the canvas strip's coordinates update on pointer move.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { fixture } from "./fixture.test-data";
import { shellCmd } from "./shell.test-data";
import { forgetViewports } from "./shell/viewState";

const realRect = Element.prototype.getBoundingClientRect;
beforeEach(() => {
  localStorage.clear();
  forgetViewports();
  Element.prototype.getBoundingClientRect = () => ({ x: 0, y: 0, left: 0, top: 0, right: 1000, bottom: 1000, width: 1000, height: 1000, toJSON: () => ({}) }) as DOMRect;
  mockIPC((cmd) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    switch (cmd) {
      case "recent_projects":
        return [{ path: "/p.eak", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
      case "open_project":
      case "current_project":
        return fixture(7);
      case "events_since":
        return [];
      case "library_components":
        return [];
      case "model_providers":
        return [];
      case "netlist":
        return [
          { name: "GND", pins: ["D1.1"], label_names: ["GND"], has_power_port: true },
          { name: "N1", pins: ["R1.2", "D1.2"], label_names: [], has_power_port: false },
        ];
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
  Element.prototype.getBoundingClientRect = realRect;
});

async function open() {
  render(<App />);
  fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
  await screen.findByRole("img", { name: "Schematic canvas" });
}
const bar = () => within(screen.getByRole("navigation", { name: "Activity bar" }));
const menuItem = (menu: string, item: RegExp) => {
  fireEvent.click(within(screen.getByRole("navigation", { name: "Application menu" })).getByRole("button", { name: menu }));
  fireEvent.click(screen.getByRole("menuitem", { name: item }));
};
const rightSep = () => screen.getByRole("separator", { name: "Resize right panel" });
const saved = (ws: string) => JSON.parse(localStorage.getItem(`eak.layout.v2.${ws}`) ?? "null");
const viewBox = (name: string) => screen.getByRole("img", { name, hidden: true }).getAttribute("viewBox")!.split(" ").map(Number);
const strip = (k: string) => document.querySelector(`[data-strip="${k}"]`)!.textContent;

describe("three docks (spec UI-01)", () => {
  it("hide, resize and restore; the layout persists per workspace and across sessions", async () => {
    await open();
    // schematic defaults: design manager on the left, Properties on the right
    expect(screen.getByRole("tablist", { name: "Design manager" })).toBeTruthy();
    expect(within(screen.getByRole("tablist", { name: "Right panel tabs" })).getByRole("tab", { name: "Properties" }).getAttribute("aria-selected")).toBe("true");
    // resize the right dock with the keyboard (towards the canvas grows it)
    const before = Number(rightSep().getAttribute("aria-valuenow"));
    fireEvent.keyDown(rightSep(), { key: "ArrowLeft" });
    await waitFor(() => expect(Number(rightSep().getAttribute("aria-valuenow"))).toBe(before + 16));
    expect(saved("schematic").right.size).toBe(before + 16);
    // hide it from its own header
    fireEvent.click(screen.getByRole("button", { name: "Hide right panel" }));
    await waitFor(() => expect(screen.queryByLabelText("Right panel")).toBeNull());
    expect(saved("schematic").right.open).toBe(false);
    // the PCB keeps its own layout: right dock open, on Layers
    fireEvent.click(bar().getByRole("button", { name: "PCB" }));
    const tabs = await screen.findByRole("tablist", { name: "Right panel tabs" });
    expect(within(tabs).getByRole("tab", { name: "Layers" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("list", { name: "Layers" })).toBeTruthy();
    // the Filter tab is not built yet (B3) and says so instead of pretending
    const filter = within(tabs).getByRole("tab", { name: "Filter" }) as HTMLButtonElement;
    expect(filter.disabled).toBe(true);
    expect(filter.title).toMatch(/Not available yet/);
    // back in the schematic the right dock is still hidden
    fireEvent.click(bar().getByRole("button", { name: "Schematic" }));
    await waitFor(() => expect(screen.queryByLabelText("Right panel")).toBeNull());
    // a new session restores it hidden; View > Right panel brings it back at its saved size
    cleanup();
    await open();
    expect(screen.queryByLabelText("Right panel")).toBeNull();
    menuItem("View", /^Right panel/);
    await waitFor(() => expect(Number(rightSep().getAttribute("aria-valuenow"))).toBe(before + 16));
  });

  it("Ctrl+Shift+B toggles the right dock; the Library tab of the bottom dock holds the library", async () => {
    await open();
    await waitFor(() => {
      fireEvent.keyDown(window, { key: "B", ctrlKey: true, shiftKey: true });
      expect(screen.queryByLabelText("Right panel")).toBeNull();
    });
    fireEvent.click(screen.getByRole("tab", { name: "Library" }));
    expect(await screen.findByLabelText("Search library")).toBeTruthy();
    expect(saved("schematic").bottom.tab).toBe("library");
  });
});

describe("design manager", () => {
  it("clicking a component selects it and zooms the schematic to it", async () => {
    await open();
    fireEvent.click(screen.getByRole("tab", { name: "Components" }));
    fireEvent.click(within(screen.getByRole("tabpanel", { name: "Components" })).getByRole("button", { name: /D1 Green/ }));
    await waitFor(() => expect(document.querySelector("footer.status")!.textContent).toContain("Selected D1 (Green)"));
    // D1's body (78..82 mm) widened to 10 mm and fitted with a 40 % margin
    await waitFor(() => {
      const [x, y, w, h] = viewBox("Schematic canvas");
      expect(x).toBeCloseTo(71);
      expect(y).toBeCloseTo(-59);
      expect(w).toBeCloseTo(18);
      expect(h).toBeCloseTo(18);
    });
    // the Properties tab of the right dock shows the selection (read-only)
    expect(within(screen.getByRole("region", { name: "Properties" })).getByText("Green")).toBeTruthy();
  });

  it("clicking a net in the PCB highlights its objects and zooms to them; search filters the list", async () => {
    await open();
    fireEvent.click(bar().getByRole("button", { name: "PCB" }));
    await screen.findByRole("img", { name: "PCB canvas" });
    const before = viewBox("PCB canvas").join(" ");
    fireEvent.click(screen.getByRole("tab", { name: "Nets" }));
    const panel = screen.getByRole("tabpanel", { name: "Nets" });
    await within(panel).findByRole("button", { name: /GND 1 pin/ });
    fireEvent.change(screen.getByRole("searchbox", { name: "Search design manager" }), { target: { value: "gn" } });
    expect(within(panel).queryByRole("button", { name: /^N1/ })).toBeNull();
    fireEvent.click(within(panel).getByRole("button", { name: /GND/ }));
    await waitFor(() => expect(document.querySelector('[data-via-id="via_1"]')!.getAttribute("class")).toContain("finding-hit"));
    await waitFor(() => expect(viewBox("PCB canvas").join(" ")).not.toBe(before));
    expect(document.querySelector("footer.status")!.textContent).toContain("Net GND");
  });
});

describe("canvas strip", () => {
  it("coordinates update on pointer move; dX / dY count from the last press", async () => {
    await open();
    const svg = screen.getByRole("img", { name: "Schematic canvas" });
    expect(strip("Pointer X")).toBe("—");
    fireEvent.pointerMove(svg, { clientX: 400, clientY: 400, buttons: 0 });
    await waitFor(() => expect(strip("Pointer X")).not.toBe("—"));
    const x1 = strip("Pointer X");
    const y1 = strip("Pointer Y");
    fireEvent.pointerDown(svg, { clientX: 400, clientY: 400, button: 0, buttons: 1 });
    fireEvent.pointerUp(svg, { clientX: 400, clientY: 400, button: 0 });
    await waitFor(() => expect(strip("Offset dX")).toBe("0.000"));
    // the next move may be folded into an animation frame: wait for it
    fireEvent.pointerMove(svg, { clientX: 700, clientY: 200, buttons: 0 });
    await waitFor(() => expect(strip("Pointer X")).not.toBe(x1));
    expect(strip("Pointer Y")).not.toBe(y1);
    // right of and above the press: positive dX and dY (design Y is up)
    expect(Number(strip("Offset dX"))).toBeGreaterThan(0);
    expect(Number(strip("Offset dY"))).toBeGreaterThan(0);
    expect(within(screen.getByRole("group", { name: "Canvas strip" })).getByText(/Snap on/)).toBeTruthy();
  });
});
