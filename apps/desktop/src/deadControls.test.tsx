// Slice E1 (dead-control sweep and shell honesty) in the real workspace against a mocked IPC:
// the Saved badge and the per-document marker follow the engine's busy/blocked state, Help is
// generated, About shows the version, "Show <view>" shows, quick-open goes to the sheet picked,
// the status bar shows what the active editor has, menus and modals work from the keyboard.
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { TOOLS as PCB_TOOLS } from "./PcbView";
import { TOOLS as SCH_TOOLS } from "./SchematicView";
import { shellCmd } from "./shell.test-data";
import { fixture } from "./fixture.test-data";
import { MenuBar } from "./shell/MenuBar";
import { SyncReportDialog } from "./shell/SyncReportDialog";
import { forgetViewports } from "./shell/viewState";
import { APP_VERSION } from "./version";
import type { CommandResponse, Snapshot } from "./types";

let revision: number;
let blocked: string | null;
let twoSheets: boolean;
let pending: ((r: CommandResponse) => void) | null;
const resp = (): CommandResponse => ({ op_id: "o", command: "Annotate", class: "reversible_local", accepted: true, revision_before: revision, revision_after: revision + 1, created: [], affected: [], warnings: [], error: null, validation_note: "" });
const snap = (): Snapshot => {
  const s = fixture(revision);
  s.blocked = blocked;
  if (twoSheets) s.project.schematic.sheets.sht_2 = { id: "sht_2", name: "Sheet 2", width: 297_000_000, height: 210_000_000 };
  return s;
};

beforeEach(() => {
  localStorage.clear();
  forgetViewports();
  revision = 7;
  blocked = null;
  twoSheets = false;
  pending = null;
  mockIPC((cmd) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    switch (cmd) {
      case "recent_projects":
        return [{ path: "/p.eak", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
      case "open_project":
      case "current_project":
        return snap();
      case "events_since":
        return [];
      case "execute_command":
        // Held until the test releases it: the command is "in flight" meanwhile.
        return new Promise<CommandResponse>((res) => {
          pending = (r) => {
            revision += 1;
            res(r);
          };
        });
      case "simulation_status":
        return { ngspice: { available: false, path: null, version: null, message: "ngspice is not installed or not on PATH." }, revision, nets: [], elements: [], skipped: [] };
      case "library_components":
      case "model_providers":
        return [];
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
  vi.restoreAllMocks();
});

async function open() {
  render(<App />);
  fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
  // every open schematic sheet keeps its editor mounted (hidden when not active), so there may be several
  await screen.findAllByRole("img", { name: "Schematic canvas" });
}
const bar = () => within(screen.getByRole("navigation", { name: "Application menu" }));
const menu = (name: string, item: RegExp | string) => {
  fireEvent.click(bar().getByRole("button", { name }));
  fireEvent.click(screen.getByRole("menuitem", { name: item }));
};
const tabs = () => within(screen.getByRole("tablist", { name: "Documents" }));
const badge = () => document.querySelector(".tb-title .saved")!;
const markers = () => tabs().queryAllByRole("img", { name: /^not confirmed/ });

describe("Saved badge and document marker (F5, A-SAVE)", () => {
  it("say Writing while a command is in flight, only on the document that issued it, then Saved", async () => {
    await open();
    expect(badge().textContent).toBe("Saved · r7");
    expect(markers()).toHaveLength(0);
    menu("Design", "Annotate schematic");
    await waitFor(() => expect(badge().getAttribute("data-state")).toBe("busy"));
    expect(badge().textContent).toContain("Writing");
    // one marker, on the active tab (Sheet 1), none on the PCB tab
    expect(markers()).toHaveLength(1);
    expect(within(tabs().getByRole("tab", { name: /Sheet 1/ })).getByRole("img", { name: /in flight/ })).toBeTruthy();
    expect(within(tabs().getByRole("tab", { name: /^PCB/ })).queryByRole("img")).toBeNull();
    await act(async () => pending!(resp()));
    await waitFor(() => expect(badge().textContent).toBe("Saved · r8"));
    expect(markers()).toHaveLength(0);
  });

  it("say Writes blocked when the engine reports blocked writes, on every engine document", async () => {
    blocked = "the project lock is held by another process";
    await open();
    expect(badge().getAttribute("data-state")).toBe("blocked");
    expect(badge().textContent).toContain("Writes blocked");
    expect(badge().getAttribute("title")).toContain("lock is held");
    expect(markers()).toHaveLength(2);
    // closing a marked document asks first, and says why
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    fireEvent.click(tabs().getByRole("button", { name: "Close PCB" }));
    expect(confirm.mock.calls[0][0]).toContain("Writes are blocked");
    expect(tabs().getByRole("tab", { name: /^PCB/ })).toBeTruthy();
  });

  it("closing an unmarked document does not ask", async () => {
    await open();
    const confirm = vi.spyOn(window, "confirm");
    fireEvent.click(tabs().getByRole("button", { name: "Close PCB" }));
    await waitFor(() => expect(tabs().queryByRole("tab", { name: /^PCB/ })).toBeNull());
    expect(confirm).not.toHaveBeenCalled();
  });
});

describe("Help and About", () => {
  it("Help > Keyboard shortcuts is generated: registry keys plus every tool key of the active editor; Escape closes it", async () => {
    await open();
    menu("Help", "Keyboard shortcuts");
    const dlg = screen.getByRole("dialog", { name: "Keyboard shortcuts" });
    const editor = within(within(dlg).getByRole("list", { name: "Editor shortcuts" }));
    for (const [i, [, label]] of SCH_TOOLS.entries()) expect(editor.getByText(`${label} tool`).parentElement!.querySelector("kbd")!.textContent).toBe(String(i + 1));
    expect(SCH_TOOLS.length).toBe(9);
    expect(dlg.textContent).not.toContain("1–7");
    const ws = within(within(dlg).getByRole("list", { name: "Workspace shortcuts" }));
    expect(ws.getByText("Save (confirm written)")).toBeTruthy();
    expect(ws.getByText("Undo")).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Keyboard shortcuts" })).toBeNull();
    // in the PCB editor the list follows the PCB's own tools
    fireEvent.click(screen.getByRole("button", { name: "PCB" }));
    await screen.findByRole("img", { name: "PCB canvas" });
    menu("Help", "Keyboard shortcuts");
    const pcb = within(screen.getByRole("list", { name: "Editor shortcuts" }));
    for (const [, label] of PCB_TOOLS) expect(pcb.getByText(`${label} tool`)).toBeTruthy();
    expect(pcb.getByText(/Rotate \/ flip selected footprints \(Select and Move tools\)/)).toBeTruthy();
  });

  it("Help > About EAK opens a dialog with the version; Escape closes it", async () => {
    await open();
    menu("Help", /About EAK/);
    const dlg = screen.getByRole("dialog", { name: "About EAK" });
    expect(within(dlg).getByTestId("app-version").textContent).toBe(`version ${APP_VERSION}`);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "About EAK" })).toBeNull();
  });
});

describe("navigation and state display", () => {
  it("View > Show <view> shows the view and never collapses it", async () => {
    await open();
    const side = () => screen.getByRole("complementary", { hidden: true, name: "Side panel" });
    menu("View", "Show Design manager");
    await waitFor(() => expect(side().getAttribute("aria-hidden")).toBe("false"));
    expect(side().querySelector(".panel-head .title")!.textContent).toBe("Design manager");
    menu("View", /Show Design manager/);
    expect(side().getAttribute("aria-hidden")).toBe("false");
    expect(side().querySelector(".panel-head .title")!.textContent).toBe("Design manager");
  });

  it("quick open > a sheet opens that sheet, not the first schematic", async () => {
    twoSheets = true;
    await open();
    expect(tabs().getByRole("tab", { name: /Sheet 1/ }).getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(window, { key: "p", ctrlKey: true });
    const dlg = await screen.findByRole("dialog", { name: "Quick open" });
    fireEvent.click(within(dlg).getAllByRole("option", { name: /^Sheet 2/ })[0]);
    await waitFor(() => expect(tabs().getByRole("tab", { name: /Sheet 2/ }).getAttribute("aria-selected")).toBe("true"));
  });

  it("the status bar shows a copper layer only while the PCB editor is active", async () => {
    await open();
    const status = () => screen.getByRole("status").textContent ?? "";
    expect(status()).not.toContain("Layer");
    fireEvent.click(screen.getByRole("button", { name: "PCB" }));
    await screen.findByRole("img", { name: "PCB canvas" });
    await waitFor(() => expect(status()).toContain("Layer Top"));
  });

  it("the activity bar does not advertise Mod+B for views it cannot open", async () => {
    await open();
    const ab = within(screen.getByRole("navigation", { name: "Activity bar" }));
    for (const name of ["Design manager", "Domain", "Manufacturing", "Overview"]) expect(ab.getByRole("button", { name }).getAttribute("title")).not.toMatch(/\+B\)/);
  });
});

describe("keyboard on menus and modals", () => {
  it("MenuBar: arrows move between items and menus, Home/End jump, Escape returns to the button, Enter runs", () => {
    const go = vi.fn();
    const other = vi.fn();
    render(
      <MenuBar
        menus={[
          { label: "One", items: [{ label: "A", run: vi.fn() }, { label: "B", run: go }, { label: "C", run: vi.fn(), disabled: "Select a part first" }] },
          { label: "Two", items: [{ label: "X", run: other }] },
        ]}
      />,
    );
    const focused = () => document.activeElement?.textContent ?? "";
    const one = screen.getByRole("button", { name: "One" });
    one.focus();
    fireEvent.keyDown(one, { key: "ArrowDown" });
    expect(focused()).toBe("A");
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    expect(focused()).toBe("B");
    fireEvent.keyDown(document.activeElement!, { key: "End" });
    expect(focused()).toMatch(/^C/);
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    expect(focused()).toBe("A"); // wraps
    fireEvent.keyDown(document.activeElement!, { key: "ArrowUp" });
    expect(focused()).toMatch(/^C/);
    fireEvent.keyDown(document.activeElement!, { key: "Home" });
    expect(focused()).toBe("A");
    fireEvent.keyDown(document.activeElement!, { key: "ArrowRight" });
    expect(screen.getByRole("menu", { name: "Two" })).toBeTruthy();
    expect(focused()).toBe("X");
    fireEvent.keyDown(document.activeElement!, { key: "ArrowLeft" });
    expect(screen.getByRole("menu", { name: "One" })).toBeTruthy();
    expect(focused()).toBe("A");
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    fireEvent.click(document.activeElement!); // Enter on a focused button activates it
    expect(go).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).toBeNull();
    one.focus();
    fireEvent.keyDown(one, { key: "ArrowUp" });
    expect(focused()).toMatch(/^C/);
    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(one);
    fireEvent.keyDown(one, { key: "ArrowRight" });
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Two" }));
    expect(screen.queryByRole("menu")).toBeNull();
    // a menu opened with the pointer: ArrowDown moves into it (no re-render needed)
    fireEvent.click(one);
    expect(document.activeElement).toBe(one);
    fireEvent.keyDown(one, { key: "ArrowDown" });
    expect(focused()).toBe("A");
    expect(other).not.toHaveBeenCalled();
  });

  it("the Update PCB report closes on Escape", () => {
    const onClose = vi.fn();
    render(<SyncReportDialog report={null} error="rejected" onOpenPcb={() => {}} onClose={onClose} />);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });
});
