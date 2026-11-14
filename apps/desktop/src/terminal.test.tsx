// Terminal workspace against a mocked Tauri IPC. xterm.js needs a real canvas, so it is
// replaced by a stub; the PTY behaviour itself is covered by crates/eak-terminal/tests/pty.rs.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { IPC_COMMANDS } from "./api";
import { TerminalWorkspace } from "./terminal/TerminalWorkspace";
import type { Overview, SessionInfo } from "./terminal/api";

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
let sessions: SessionInfo[];
let installed: boolean;
const CLIENT = ["/usr/bin/herdr", "--session", "eak-desktop"];

const session = (id: number, argv: string[] | null, state: SessionInfo["status"]["state"] = "running"): SessionInfo => ({
  id,
  name: argv ? "Herdr" : `bash ${id}`,
  space: 1,
  tab: id,
  spec: { program: argv ? { kind: "command", argv } : { kind: "shell", path: null }, cwd: "/projects/board", project: "Board" },
  pid: 100 + id,
  status: { state, source: "process", detail: "", foreground: "bash", agent: null, exit_code: null },
  output_seq: 0,
});

beforeEach(() => {
  calls = [];
  sessions = [];
  installed = true;
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown command ${cmd}`);
    const health = { session: "eak-desktop", bin: CLIENT[0], running: true, version: "0.7.4", started_by_eak: true, restarts: 0, error: null };
    switch (cmd) {
      case "terminal_herdr_status":
      case "terminal_herdr_start":
        return installed ? { installed: true, error: null, health, client: CLIENT } : { installed: false, error: null, health: null, client: null };
      case "terminal_overview":
        return { spaces: [], tabs: [], sessions, focused: sessions.at(-1)?.id ?? null, layout_seq: 1 } satisfies Overview;
      case "terminal_create": {
        const opts = args.opts as { command?: string[] };
        const s = session(sessions.length + 1, opts.command ?? null);
        sessions.push(s);
        return s;
      }
      case "terminal_close":
        sessions = sessions.filter((s) => s.id !== args.id);
        return null;
      case "terminal_scrollback":
        return { data: "", seq: 0 };
      case "terminal_herdr_agents":
        return { agents: [] };
      case "terminal_herdr_open_project":
        return "w1";
      case "terminal_shells":
        return ["/bin/bash", "/bin/zsh"];
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
  it("starts EAK's Herdr session and shows Herdr's own client in a PTY", async () => {
    render(<TerminalWorkspace visible project={project} newSpaceNonce={0} />);
    await screen.findByText(/session eak-desktop · server running/);
    const create = calls.find((c) => c.cmd === "terminal_create");
    expect(create?.args.opts).toEqual({ command: CLIENT, name: "Herdr" });
    await waitFor(() => expect(document.querySelector('[data-session="1"]')).not.toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "Open Board space" }));
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "terminal_herdr_open_project")?.args).toEqual({ dir: "/projects/board", name: "Board", project_id: "prj_1" }),
    );
  });

  it("reattaches to a running Herdr client instead of starting a second one", async () => {
    sessions = [session(5, CLIENT)];
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    await screen.findByText(/session eak-desktop · server running/);
    expect(calls.some((c) => c.cmd === "terminal_create")).toBe(false);
    expect(document.querySelector('[data-session="5"]')).not.toBeNull();
  });

  it("the + button opens a Herdr space for the current project", async () => {
    const { rerender } = render(<TerminalWorkspace visible project={project} newSpaceNonce={0} />);
    await screen.findByText(/session eak-desktop · server running/);
    rerender(<TerminalWorkspace visible project={project} newSpaceNonce={1} />);
    await waitFor(() => expect(calls.some((c) => c.cmd === "terminal_herdr_open_project")).toBe(true));
  });

  it("without Herdr, offers the built-in terminal and confirms before killing a running process", async () => {
    installed = false;
    render(<TerminalWorkspace visible project={project} newSpaceNonce={0} />);
    await screen.findByText(/Herdr is not installed/);
    fireEvent.click(screen.getByRole("button", { name: "+ New terminal" }));
    await screen.findByRole("tab", { name: /bash 1/ });
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await screen.findByRole("alertdialog", { name: "Confirm close" });
    expect(calls.some((c) => c.cmd === "terminal_close")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Close and terminate" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_close")?.args).toEqual({ id: 1 }));
  });
});
