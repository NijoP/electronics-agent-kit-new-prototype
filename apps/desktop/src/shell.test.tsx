// Shell behaviour: shortcut formatting, BOM, and the shell flows
// (quick open, panel toggles) against a mocked Tauri IPC.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { formatShortcut, matches } from "./design/shortcuts";
import { shellCmd } from "./shell.test-data";
import { fixture } from "./fixture.test-data";
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
        { const h = shellCmd(cmd, null); if (h.handled) return h.value; { const h = shellCmd(cmd, null); if (h.handled) return h.value; throw new Error(`unmocked ${cmd}`); } }
    }
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
});

async function open() {
  render(<App />);
  fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
  await screen.findByRole("img", { name: "Schematic canvas" });
}

describe("shell flows", () => {
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

