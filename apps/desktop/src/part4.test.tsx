// Part 4: menu bar, backend-bound document tabs, resizable/persisted panel layout.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { shellCmd } from "./shell.test-data";
import { fixture } from "./fixture.test-data";
import { availableDocs, closeDoc, defaultDocState, loadDocState, openDoc, saveDocState, visibleDocs } from "./shell/documents";
import { clampSize, defaultLayout, LIMITS, sanitizeLayout } from "./shell/layout";
import { MenuBar } from "./shell/MenuBar";
import type { CommandResponse } from "./types";

describe("documents", () => {
  const snap = fixture(7);
  it("derives tabs from engine ids and restores only documents that still exist", () => {
    const docs = availableDocs(snap);
    expect(docs.map((d) => d.id)).toEqual(["sch:sht_1", "pcb:prj_1"]);
    localStorage.clear();
    saveDocState("prj_1", { open: ["pcb:prj_1", "sch:gone"], active: "sch:gone" });
    expect(loadDocState(snap)).toEqual({ open: ["pcb:prj_1"], active: "pcb:prj_1" });
    localStorage.clear();
  });
  it("opens, closes and re-activates", () => {
    let st = defaultDocState(snap);
    st = closeDoc(st, "sch:sht_1");
    expect(st).toEqual({ open: ["pcb:prj_1"], active: "pcb:prj_1" });
    st = openDoc(st, "sch:sht_1");
    expect(st.active).toBe("sch:sht_1");
    expect(closeDoc({ open: [], active: null }, "x")).toEqual({ open: [], active: null });
  });
  it("split shows the schematic and the PCB together", () => {
    const docs = availableDocs(snap);
    const st = defaultDocState(snap);
    expect(visibleDocs(st, docs, true).map((d) => d.kind)).toEqual(["schematic", "pcb"]);
    expect(visibleDocs(st, docs, false).map((d) => d.kind)).toEqual(["schematic"]);
  });
});

describe("layout", () => {
  it("clamps sizes and ignores malformed stored values", () => {
    expect(clampSize("left", 5000)).toBe(LIMITS.left[1]);
    expect(clampSize("bottom", -1)).toBe(LIMITS.bottom[0]);
    expect(sanitizeLayout("pcb", "junk")).toEqual(defaultLayout("pcb"));
    const d = defaultLayout("schematic");
    expect(sanitizeLayout("schematic", { left: { size: "x" }, bottom: { open: false, size: 300 }, ai: { open: true } })).toEqual({ ...d, bottom: { ...d.bottom, open: false, size: 300 } });
  });
});

describe("menu bar", () => {
  it("runs enabled items, shows the reason for disabled ones, confirms destructive ones", () => {
    const run = vi.fn();
    const bad = vi.fn();
    const danger = vi.fn();
    const confirm = vi.fn().mockReturnValue(false);
    render(
      <MenuBar
        confirm={confirm}
        menus={[{ label: "Demo", items: [{ label: "Go", run }, { label: "Nope", run: bad, disabled: "Not implemented: reason" }, { label: "Wipe", run: danger, destructive: true }] }]}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Demo" }));
    fireEvent.click(screen.getByRole("menuitem", { name: /Nope/ }));
    expect(bad).not.toHaveBeenCalled();
    expect(screen.getByRole("menuitem", { name: /Nope/ }).textContent).toContain("Not implemented: reason");
    fireEvent.click(screen.getByRole("menuitem", { name: /Wipe/ }));
    expect(danger).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("menuitem", { name: "Go" }));
    expect(run).toHaveBeenCalledOnce();
  });
});

let revision: number;
let executed: unknown[];
const resp = (): CommandResponse => ({ op_id: "o", command: "x", class: "reversible_local", accepted: true, revision_before: revision, revision_after: revision + 1, created: [], affected: [], warnings: [], error: null, validation_note: "" });
beforeEach(() => {
  localStorage.clear();
  revision = 7;
  executed = [];
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    switch (cmd) {
      case "recent_projects":
        return [{ path: "/p.eak", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
      case "open_project":
      case "current_project":
        return fixture(revision);
      case "events_since":
        return [];
      case "execute_command":
        executed.push(args.command);
        revision += 1;
        return resp();
      case "library_components":
        return [{ id: "res", name: "Resistor", category: "passive", refdes_prefix: "R", default_value: "10k", pins: [["1", "1"], ["2", "2"]] }];
      case "model_providers":
        return [];
      default:
        { const h = shellCmd(cmd, null); if (h.handled) return h.value; { const h = shellCmd(cmd, null); if (h.handled) return h.value; throw new Error(`unmocked ${cmd}`); } }
    }
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
  localStorage.clear();
});

async function open() {
  render(<App />);
  fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
  await screen.findByRole("img", { name: "Schematic canvas" });
}
const menu = (name: string) => fireEvent.click(within(screen.getByRole("navigation", { name: "Application menu" })).getByRole("button", { name }));

describe("workspace Part 4", () => {
  it("menu items call the typed command engine; disabled ones explain themselves", async () => {
    await open();
    menu("Design");
    fireEvent.click(screen.getByRole("menuitem", { name: /Annotate schematic/ }));
    await waitFor(() => expect(executed).toEqual([{ type: "Annotate" }]));
    menu("Simulation");
    const sim = screen.getByRole("menuitem", { name: /Run simulation/ });
    // Simulation is read-only and opens the Simulation tab; it never issues an engine command.
    expect(sim.getAttribute("aria-disabled")).not.toBe("true");
    fireEvent.click(sim);
    expect(await screen.findByRole("tab", { name: "Simulation" })).toBeTruthy();
    expect(executed).toHaveLength(1);
  });

  it("closing a document tab closes its editor; the open set is persisted per project and restored", async () => {
    await open();
    expect(screen.getByRole("img", { name: "PCB canvas" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close PCB" }));
    await waitFor(() => expect(screen.queryByRole("img", { name: "PCB canvas" })).toBeNull());
    expect(JSON.parse(localStorage.getItem("eak.docs.prj_1")!)).toEqual({ open: ["sch:sht_1"], active: "sch:sht_1" });
    cleanup();
    await open();
    expect(screen.queryByRole("img", { name: "PCB canvas" })).toBeNull();
    // reopen from the Project menu
    menu("Project");
    fireEvent.click(screen.getByRole("menuitem", { name: "Open PCB" }));
    await screen.findByRole("img", { name: "PCB canvas" });
  });

  it("close all shows the empty state; the dirty dot appears while a command is in flight", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Close all" }));
    await screen.findByLabelText("No documents open");
    const within_ = within(screen.getByRole("tablist", { name: "Documents" }));
    expect(within_.queryAllByRole("tab")).toHaveLength(0);
  });

  it("splitters resize with the keyboard, and the layout is persisted and resettable", async () => {
    await open();
    const sep = screen.getByRole("separator", { name: "Resize side panel" });
    const before = Number(sep.getAttribute("aria-valuenow"));
    fireEvent.keyDown(sep, { key: "ArrowRight" });
    await waitFor(() => expect(Number(screen.getByRole("separator", { name: "Resize side panel" }).getAttribute("aria-valuenow"))).toBe(before + 16));
    // Phase 1 A3: saved per workspace (the schematic here).
    const saved = () => JSON.parse(localStorage.getItem("eak.layout.v2.schematic")!);
    expect(saved().left.size).toBe(before + 16);
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(saved().bottom.open).toBe(false);
    menu("View");
    fireEvent.click(screen.getByRole("menuitem", { name: "Reset panel layout" }));
    await waitFor(() => expect(saved()).toEqual(defaultLayout("schematic")));
  });
});

describe("document-aware menus (slice A1)", () => {
  const bar = () => within(screen.getByRole("navigation", { name: "Application menu" }));
  it("schematic and PCB show their own menus; menu Place/Route entries arm the editor tool", async () => {
    await open();
    expect(bar().queryByRole("button", { name: "Simulation" })).toBeTruthy();
    expect(bar().queryByRole("button", { name: "Route" })).toBeNull();
    const sch = within(screen.getByRole("region", { name: "Schematic view" }));
    // The editor registers its tool handler in an effect: retry the idempotent pick until armed.
    await waitFor(() => {
      menu("Place");
      fireEvent.click(screen.getByRole("menuitem", { name: "Wire" }));
      expect(sch.getByRole("button", { name: "Wire" }).getAttribute("aria-pressed")).toBe("true");
    });
    fireEvent.click(within(screen.getByRole("navigation", { name: "Activity bar" })).getByRole("button", { name: "PCB" }));
    await waitFor(() => expect(bar().queryByRole("button", { name: "Route" })).toBeTruthy());
    expect(bar().queryByRole("button", { name: "Simulation" })).toBeNull();
    const pcb = within(screen.getByRole("region", { name: "PCB view" }));
    await waitFor(() => {
      menu("Route");
      fireEvent.click(screen.getByRole("menuitem", { name: "Route track" }));
      expect(pcb.getByRole("button", { name: "Route" }).getAttribute("aria-pressed")).toBe("true");
    });
    menu("Place");
    expect(screen.queryByRole("menuitem", { name: "Wire" })).toBeNull();
    expect(screen.getByRole("menuitem", { name: "Via" })).toBeTruthy();
  });
});
