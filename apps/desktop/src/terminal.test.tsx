// Terminal workspace against a mocked Tauri IPC. xterm.js needs a real canvas, so it is
// replaced by a stub; real PTY behaviour is covered by crates/eak-terminal/tests/pty.rs.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "./api";
import { TerminalWorkspace } from "./terminal/TerminalWorkspace";
import type { CreateOptions, Overview, SessionInfo, Space, Tab } from "./terminal/api";

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    element?: HTMLElement;
    cols = 80;
    rows = 24;
    loadAddon() {}
    onData() {}
    attachCustomKeyEventHandler() {}
    open(el: HTMLElement) { this.element = el; }
    write() {}
    focus() {}
    dispose() {}
    hasSelection() { return false; }
  },
}));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() {} } }));
vi.mock("@xterm/addon-search", () => ({ SearchAddon: class { findNext() {} findPrevious() {} } }));
vi.mock("@xterm/xterm/css/xterm.css", () => ({}));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
globalThis.ResizeObserver ??= class { observe() {} disconnect() {} unobserve() {} } as unknown as typeof ResizeObserver;

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];
let spaces: Space[];
let tabs: Tab[];
let sessions: SessionInfo[];
let focused: number | null;
let nextId: number;

const session = (id: number, space: number, tab: number, state: SessionInfo["status"]["state"] = "idle"): SessionInfo => ({
  id,
  name: `bash ${id}`,
  space,
  tab,
  spec: { program: { kind: "shell", path: null }, cwd: spaces.find((s) => s.id === space)?.root ?? "/", project: null },
  pid: 100 + id,
  status: { state, source: "process", detail: "shell at its prompt", foreground: "bash", agent: null, exit_code: null },
  output_seq: 0,
});

beforeEach(() => {
  calls = [];
  spaces = [];
  tabs = [];
  sessions = [];
  focused = null;
  nextId = 1;
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown command ${cmd}`);
    switch (cmd) {
      case "terminal_overview":
        return { spaces, tabs, sessions, focused, layout_seq: 1 } satisfies Overview;
      case "terminal_space_create": {
        const sp = { id: nextId++, name: String(args.name), root: String(args.root), project_id: null };
        spaces.push(sp);
        return sp;
      }
      case "terminal_create": {
        const o = args.opts as CreateOptions;
        const id = nextId++;
        const target = o.split_of != null ? tabs.find((t) => t.panes.includes(o.split_of!)) : undefined;
        const space = target?.space ?? o.space ?? spaces[0].id;
        if (target) {
          target.panes.push(id);
          target.direction = o.direction ?? "row";
        } else tabs.push({ id: nextId++, space, direction: "row", panes: [id] });
        const s = session(id, space, target?.id ?? tabs[tabs.length - 1].id, "running");
        sessions.push(s);
        focused = id;
        return s;
      }
      case "terminal_close":
        sessions = sessions.filter((s) => s.id !== args.id);
        tabs = tabs.map((t) => ({ ...t, panes: t.panes.filter((p) => p !== args.id) })).filter((t) => t.panes.length > 0);
        return null;
      case "terminal_scrollback":
        return { data: "", seq: 0 };
      case "terminal_shells":
        return ["/bin/bash", "/bin/zsh"];
      case "terminal_launchers":
        return [];
      default:
        return null;
    }
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

const project = { dir: "/projects/board", name: "Board", id: "prj_1" };

describe("terminal workspace", () => {
  it("the + control creates the project's space with a terminal rooted at the project", async () => {
    const { rerender } = render(<TerminalWorkspace visible project={project} newSpaceNonce={0} />);
    await screen.findByText("SPACES");
    rerender(<TerminalWorkspace visible project={project} newSpaceNonce={1} />);
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_space_create")?.args).toEqual({ name: "Board", root: "/projects/board" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_create")?.args.opts).toEqual({ space: 1 }));
    await screen.findByText("Board");
    await waitFor(() => expect(document.querySelector('[data-session="2"]')).not.toBeNull());
  });

  it("new terminals open in the active space and split into the same tab", async () => {
    spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: "prj_1" }];
    nextId = 2;
    render(<TerminalWorkspace visible project={project} newSpaceNonce={0} />);
    await screen.findByText("Board");
    fireEvent.click(screen.getByRole("button", { name: "+ New terminal" }));
    await screen.findByRole("tab", { name: /bash 2/ });
    expect(calls.find((c) => c.cmd === "terminal_create")?.args.opts).toEqual({ space: 1 });
    fireEvent.click(screen.getByRole("button", { name: "Split right" }));
    await waitFor(() => expect(calls.filter((c) => c.cmd === "terminal_create")[1]?.args.opts).toEqual({ split_of: 2, direction: "row" }));
    await waitFor(() => expect(document.querySelectorAll(".xterm-box").length).toBe(2));
  });

  it("closing a running terminal asks before terminating its process", async () => {
    spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: null }];
    tabs = [{ id: 3, space: 1, direction: "row", panes: [2] }];
    sessions = [session(2, 1, 3, "running")];
    focused = 2;
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    await screen.findByRole("tab", { name: /bash 2/ });
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await screen.findByRole("alertdialog", { name: "Confirm close" });
    expect(calls.some((c) => c.cmd === "terminal_close")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Close and terminate" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_close")?.args).toEqual({ id: 2 }));
  });
});
