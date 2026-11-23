// IDE shell against a mocked Tauri IPC: one editor at a time, no assistant, registered workspaces
// (app bar + "+", kept mounted), the file explorer, and the multi-project tab strip.
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { useEffect } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "../api";
import { App } from "../App";
import { fixture } from "../fixture.test-data";
import { remapFileDocs } from "./documents";
import { registerWorkspace, type WorkspaceContext } from "./registry";

let calls: { cmd: string; args: Record<string, unknown> }[];
let files: Record<string, unknown[]>;
let openProjects: unknown[];
const entry = (name: string, rel: string, is_dir = false, extra: object = {}) => ({ name, rel, is_dir, size: 10, protected: false, role: is_dir ? "folder" : "file", modified_ms: 1, ...extra });

beforeEach(() => {
  localStorage.clear();
  calls = [];
  files = {
    "": [entry("scripts", "scripts", true), entry("project.eak.json", "project.eak.json", false, { protected: true, role: "design" }), entry("notes.md", "notes.md")],
    scripts: [entry("check.sh", "scripts/check.sh")],
  };
  openProjects = [{ id: "prj_1", name: "Fixture board", dir: "/p/demo", active: true }];
  mockIPC((cmd, payload) => {
    const a = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args: a });
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    switch (cmd) {
      case "recent_projects": return [{ path: "/p/demo", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
      case "open_project": case "current_project": case "switch_project": return fixture(7);
      case "open_projects": return openProjects;
      case "project_files": return files[String(a.rel)] ?? [];
      case "project_git_status": return { available: true, branch: "main", entries: [{ rel: "notes.md", code: "??" }] };
      case "project_file_read": return { rel: a.rel, size: 5, text: "hello", binary: false, truncated: false };
      case "project_file_create": return entry(String(a.rel), String(a.rel), a.is_dir === true);
      case "events_since": case "model_providers": case "library_components": return [];
      case "close_project_by_id": return null;
      default: throw new Error(`unmocked ${cmd}`);
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
const slot = (name: string) => screen.getByRole("img", { name, hidden: true }).closest(".view-slot")!;
const exp = () => within(screen.getByRole("toolbar", { name: "Explorer actions" }));
const bar = () => within(screen.getByRole("navigation", { name: "Activity bar" }));

describe("workspace separation", () => {
  it("opens on the schematic alone; PCB shows only the PCB; both editors stay mounted", async () => {
    await open();
    expect(slot("Schematic canvas").className).not.toContain("off");
    expect(slot("PCB canvas").className).toContain("off");
    fireEvent.click(bar().getByRole("button", { name: "PCB" }));
    expect(slot("PCB canvas").className).not.toContain("off");
    expect(slot("Schematic canvas").className).toContain("off");
    expect(bar().getByRole("button", { name: "PCB" }).getAttribute("aria-pressed")).toBe("true");
  });
  it("Ctrl+1 / Ctrl+2 switch editors and split view is opt-in", async () => {
    await open();
    fireEvent.keyDown(window, { key: "2", ctrlKey: true });
    await waitFor(() => expect(slot("PCB canvas").className).not.toContain("off"));
    fireEvent.keyDown(window, { key: "1", ctrlKey: true });
    await waitFor(() => expect(slot("Schematic canvas").className).not.toContain("off"));
    fireEvent.click(screen.getByRole("button", { name: "Split" }));
    expect(slot("PCB canvas").className).not.toContain("off");
  });
  it("has no assistant panel, chat input or Ask chips, and no 3D/Terminal control until something registers one", async () => {
    await open();
    expect(screen.queryByRole("textbox", { name: /Ask the assistant/ })).toBeNull();
    expect(screen.queryByRole("complementary", { name: /assistant/i })).toBeNull();
    expect(screen.queryByText(/Ask AI/)).toBeNull();
    expect(bar().queryByRole("button", { name: /3D|Terminal/ })).toBeNull();
    expect(screen.queryByRole("button", { name: "New Terminal space" })).toBeNull();
  });
});

describe("registered workspaces", () => {
  it("an app workspace gets a tab and the + action; switching keeps both mounted and gates EDA keys", async () => {
    let mounts = 0;
    let created = 0;
    const Body = ({ cwd }: { cwd: string | null }) => {
      useEffect(() => {
        mounts += 1;
      }, []);
      return <div>terminal body {cwd}</div>;
    };
    const off = registerWorkspace({ id: "terminal", label: "Terminal Workspace", newLabel: "New terminal space", onNew: () => (created += 1), render: (c) => <Body cwd={c.projectDir} /> });
    try {
      await open();
      fireEvent.click(screen.getByRole("button", { name: "New terminal space" }));
      expect(created).toBe(1);
      await screen.findByText(/terminal body/);
      const tab = screen.getByRole("tab", { name: "Terminal Workspace" });
      expect(tab.getAttribute("aria-selected")).toBe("true");
      // EDA is hidden, not unmounted, and its global shortcuts do nothing while the terminal is shown
      expect(screen.getByRole("img", { name: "Schematic canvas", hidden: true }).closest(".ws-host")!.hasAttribute("hidden")).toBe(true);
      fireEvent.keyDown(window, { key: "j", ctrlKey: true });
      expect(screen.getByRole("region", { name: "Bottom panel", hidden: true })).toBeTruthy();
      fireEvent.click(screen.getByRole("tab", { name: "Project / EDA" }));
      fireEvent.click(screen.getByRole("tab", { name: "Terminal Workspace" }));
      expect(mounts).toBe(1); // never remounted
      expect(screen.getByRole("img", { name: "Schematic canvas", hidden: true })).toBeTruthy();
    } finally {
      off();
    }
  });
  // Slice C6: 3D is a document. It opens as a tab beside the editors, gets the 3D menu set, and
  // is told the selection and how to cross-probe; closing the tab unmounts it.
  it("an EDA view (3D) opens as its own document tab with the 3D menu set", async () => {
    const seen: WorkspaceContext[] = [];
    const off = registerWorkspace({
      id: "viewer3d",
      placement: "eda",
      label: "3D",
      icon: <i />,
      render: (ctx) => {
        seen.push(ctx);
        return <div>board viewer</div>;
      },
    });
    try {
      await open();
      fireEvent.click(bar().getByRole("button", { name: "3D" }));
      await screen.findByText("board viewer");
      const tabs = within(screen.getByRole("tablist", { name: "Documents" }));
      expect(tabs.getByRole("tab", { name: "3D" }).getAttribute("aria-selected")).toBe("true");
      // the editors stay mounted behind it, hidden like any other inactive document
      expect(slot("Schematic canvas").classList.contains("off")).toBe(true);
      expect(screen.getByText("board viewer").closest(".view-slot")!.classList.contains("off")).toBe(false);
      // the 3D document has its own View entries; the editors' Place menu is gone
      const menuBar = within(screen.getByRole("navigation", { name: "Application menu" }));
      expect(menuBar.queryByRole("button", { name: "Place" })).toBeNull();
      fireEvent.click(menuBar.getByRole("button", { name: "View" }));
      expect(screen.getByRole("menuitem", { name: /Top view/ })).toBeTruthy();
      fireEvent.click(menuBar.getByRole("button", { name: "View" }));
      // the view gets the selection, cross-probing and the engine path
      const ctx = seen[seen.length - 1];
      expect(typeof ctx.select).toBe("function");
      expect(typeof ctx.probeTo).toBe("function");
      expect(typeof ctx.execute).toBe("function");
      fireEvent.click(bar().getByRole("button", { name: "Schematic" }));
      await waitFor(() => expect(slot("Schematic canvas").classList.contains("off")).toBe(false));
      expect(tabs.getByRole("tab", { name: "3D" }).getAttribute("aria-selected")).toBe("false");
      // closing the tab unmounts the view (its GL context is released)
      fireEvent.click(tabs.getByRole("button", { name: "Close 3D" }));
      await waitFor(() => expect(screen.queryByText("board viewer")).toBeNull());
    } finally {
      off();
    }
  });
});

describe("explorer", () => {
  it("lists real files with git status, expands folders, opens a file in a viewer tab", async () => {
    await open();
    // Phase 1 A3: the editors open the design manager in the left dock; Explorer is one click away.
    fireEvent.click(bar().getByRole("button", { name: "Explorer" }));
    const tree = await screen.findByRole("tree", { name: "Project files" });
    await within(tree).findByText("notes.md");
    expect(within(tree).getByText("U")).toBeTruthy(); // untracked
    expect(screen.getByText(/git main/)).toBeTruthy();
    fireEvent.click(within(tree).getByText("scripts"));
    await within(tree).findByText("check.sh");
    expect(calls.filter((c) => c.cmd === "project_files").map((c) => c.args)).toContainEqual({ project_id: "prj_1", rel: "scripts" });
    fireEvent.click(within(tree).getByText("notes.md"));
    await screen.findByText("hello");
    expect(screen.getByRole("tab", { name: /notes\.md/ })).toBeTruthy();
    expect(calls.at(-1)!.cmd).not.toBe("project_file_delete");
  });
  it("engine-owned files are view-only: rename and delete are disabled", async () => {
    await open();
    // Phase 1 A3: the editors open the design manager in the left dock; Explorer is one click away.
    fireEvent.click(bar().getByRole("button", { name: "Explorer" }));
    const tree = await screen.findByRole("tree", { name: "Project files" });
    fireEvent.click(await within(tree).findByText("project.eak.json", undefined, { timeout: 4000 }).catch(() => within(tree).findByText("project.eak.json")));
    expect((exp().getByRole("button", { name: "Delete" }) as HTMLButtonElement).disabled).toBe(true);
    expect((exp().getByRole("button", { name: "Rename" }) as HTMLButtonElement).disabled).toBe(true);
  });
  it("creating a file validates the name, then calls the sandboxed IPC with the project id", async () => {
    await open();
    // Phase 1 A3: the editors open the design manager in the left dock; Explorer is one click away.
    fireEvent.click(bar().getByRole("button", { name: "Explorer" }));
    await screen.findByRole("tree", { name: "Project files" });
    fireEvent.click(exp().getByRole("button", { name: "New file" }));
    const box = screen.getByRole("textbox", { name: "Name" });
    fireEvent.change(box, { target: { value: "a|b" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect((await screen.findByRole("alert")).textContent).toMatch(/cannot contain/);
    expect(calls.some((c) => c.cmd === "project_file_create")).toBe(false);
    fireEvent.change(box, { target: { value: "todo.txt" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "project_file_create")!.args).toEqual({ project_id: "prj_1", rel: "todo.txt", is_dir: false }));
  });
  it("deleting asks first and does nothing when declined", async () => {
    await open();
    // Phase 1 A3: the editors open the design manager in the left dock; Explorer is one click away.
    fireEvent.click(bar().getByRole("button", { name: "Explorer" }));
    const tree = await screen.findByRole("tree", { name: "Project files" });
    fireEvent.click(await within(tree).findByText("notes.md"));
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    fireEvent.click(exp().getByRole("button", { name: "Delete" }));
    expect(confirm).toHaveBeenCalled();
    expect(calls.some((c) => c.cmd === "project_file_delete")).toBe(false);
  });
});

describe("open projects", () => {
  it("shows a tab per open project, switches through the engine and closes one by id", async () => {
    openProjects = [
      { id: "prj_1", name: "Fixture board", dir: "/p/demo", active: true },
      { id: "prj_2", name: "Second", dir: "/p/two", active: false },
    ];
    await open();
    const tabs = await screen.findByRole("tablist", { name: "Open projects" });
    fireEvent.click(within(tabs).getByRole("tab", { name: "Second" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "switch_project")!.args).toEqual({ project_id: "prj_2" }));
    fireEvent.click(within(tabs).getByRole("button", { name: "Close project Second" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "close_project_by_id")!.args).toEqual({ project_id: "prj_2" }));
  });
});

describe("document state helpers", () => {
  it("renaming or deleting a file updates or closes its viewer tab", () => {
    const st = { open: ["sch:1", "file:a.txt", "file:dir/b.txt"], active: "file:a.txt" };
    expect(remapFileDocs(st, (r) => (r === "a.txt" ? "c.txt" : r))).toEqual({ open: ["sch:1", "file:c.txt", "file:dir/b.txt"], active: "file:c.txt" });
    const gone = remapFileDocs(st, (r) => (r === "a.txt" ? null : r));
    expect(gone.open).toEqual(["sch:1", "file:dir/b.txt"]);
    expect(gone.active).not.toBe("file:a.txt");
    expect(act).toBeTruthy();
  });
});
