// Shell behaviour: shortcut formatting, assistant interpreter, BOM, and the shell flows
// (quick open, assistant Accept/Reject, panel toggles) against a mocked Tauri IPC.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { formatShortcut, matches } from "./design/shortcuts";
import { fixture } from "./fixture.test-data";
import { interpret } from "./shell/ai";
import { bomLines } from "./shell/BottomPanel";
import type { CommandResponse } from "./types";

describe("shortcuts", () => {
  it("formats per platform and matches events", () => {
    expect(formatShortcut("Mod+Shift+P", false)).toBe("Ctrl+Shift+P");
    expect(formatShortcut("Mod+K", true)).toBe("⌘K");
    expect(matches(new KeyboardEvent("keydown", { key: "p", ctrlKey: true, shiftKey: true }), "Mod+Shift+P")).toBe(true);
    expect(matches(new KeyboardEvent("keydown", { key: "p", ctrlKey: true }), "Mod+Shift+P")).toBe(false);
  });
});

describe("assistant interpreter", () => {
  const snapshot = fixture(7);
  const sym = Object.values(snapshot.project.schematic.symbols)[0];
  it("proposes typed commands, never applies them", () => {
    const r = interpret("rotate", { snapshot, selected: sym.id, library: [] });
    expect(r.proposal?.commands[0]).toMatchObject({ type: "RotateSymbol", symbol_id: sym.id });
    expect(r.proposal?.destructive).toBe(false);
  });
  it("flags destructive proposals and needs a selection", () => {
    expect(interpret("delete", { snapshot, selected: sym.id, library: [] }).proposal?.destructive).toBe(true);
    expect(interpret("rotate", { snapshot, selected: null, library: [] }).proposal).toBeUndefined();
    expect(interpret("make it blue", { snapshot, selected: null, library: [] }).proposal).toBeUndefined();
  });
});

describe("BOM", () => {
  it("groups symbols by component and value", () => {
    const lines = bomLines(fixture(7));
    expect(lines.reduce((n, l) => n + l.refdes.length, 0)).toBe(Object.keys(fixture(7).project.schematic.symbols).length);
  });
});

let revision: number;
let executed: unknown[];
const resp = (accepted: boolean): CommandResponse => ({
  op_id: "o", command: "RotateSymbol", class: "reversible_local", accepted, revision_before: revision,
  revision_after: accepted ? revision + 1 : revision, created: [], affected: [], warnings: [], error: null, validation_note: "",
});
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
        return resp(true);
      case "library_components":
        return [{ id: "res", name: "Resistor", category: "passive", refdes_prefix: "R", default_value: "10k", pins: [["1", "1"], ["2", "2"]] }];
      case "model_providers":
        return [];
      default:
        throw new Error(`unmocked ${cmd}`);
    }
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
});

async function open() {
  render(<App />);
  fireEvent.click(await screen.findByText("Fixture board"));
  await screen.findByRole("img", { name: "Schematic canvas" });
}

describe("shell flows", () => {
  it("assistant preview applies only on Accept, through execute_command", async () => {
    await open();
    const box = screen.getByRole("textbox", { name: "Ask the assistant" });
    fireEvent.change(box, { target: { value: "annotate" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await screen.findByRole("group", { name: /Proposal: Annotate/ });
    expect(executed).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(executed).toEqual([{ type: "Annotate" }]));
    await screen.findByText(/Applied through the engine/);
  });

  it("Reject changes nothing", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: "annotate" }));
    await screen.findByRole("group", { name: /Proposal: Annotate/ });
    fireEvent.click(screen.getByRole("button", { name: "Reject" }));
    await screen.findByText(/Rejected — nothing changed/);
    expect(executed).toHaveLength(0);
  });

  it("quick open (Ctrl+P) lists parts and the palette shows shortcuts", async () => {
    await open();
    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    const dlg = await screen.findByRole("dialog", { name: "Quick open" });
    expect(dlg.textContent).toContain("Resistor");
    fireEvent.keyDown(screen.getByRole("textbox", { name: "Search parts, nets and sheets" }), { key: "Escape" });
    fireEvent.keyDown(window, { key: "P", ctrlKey: true, shiftKey: true });
    const pal = await screen.findByRole("dialog", { name: "Command palette" });
    expect(pal.querySelector("kbd")).toBeTruthy();
  });

  it("Ctrl+B and Ctrl+J collapse the side and bottom panels", async () => {
    await open();
    expect(screen.getByRole("region", { name: "Bottom panel" })).toBeTruthy();
    fireEvent.keyDown(window, { key: "j", ctrlKey: true });
    expect(screen.queryByRole("region", { name: "Bottom panel" })).toBeNull();
    fireEvent.keyDown(window, { key: "b", ctrlKey: true });
    expect(screen.getByLabelText("Side panel").className).not.toContain("open");
  });
});

describe("assistant with a selection", () => {
  it("rotate on a selected part previews, then Accept sends RotateSymbol", async () => {
    await open();
    fireEvent.click(screen.getByRole("button", { name: /R1 330/ }));
    fireEvent.click(screen.getByRole("button", { name: "rotate" }));
    await screen.findByRole("group", { name: /Proposal: Rotate R1/ });
    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(executed).toEqual([{ type: "RotateSymbol", symbol_id: "sym_r1", rotation: "R90" }]));
  });
});
