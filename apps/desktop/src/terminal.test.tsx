// Terminal workspace against a mocked Tauri IPC. xterm.js needs a real canvas, so it is
// replaced by a stub; real PTY behaviour is covered by crates/eak-terminal/tests/pty.rs.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import css from "./styles.css?raw";
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
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() {} proposeDimensions() { return { cols: 80, rows: 24 }; } } }));
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
let launchers: { name: string; command: string; path: string | null }[];

const session = (id: number, space: number, tab: number, state: SessionInfo["status"]["state"] = "idle", agent: string | null = null): SessionInfo => ({
  id,
  name: `bash ${id}`,
  space,
  tab,
  spec: { program: { kind: "shell", path: null }, cwd: spaces.find((s) => s.id === space)?.root ?? "/", project: null },
  pid: 100 + id,
  status: { state, source: "process", detail: "shell at its prompt", foreground: agent ?? "bash", agent, exit_code: null },
  output_seq: 0,
});

beforeEach(() => {
  calls = [];
  spaces = [];
  tabs = [];
  sessions = [];
  focused = null;
  nextId = 1;
  launchers = [];
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
        } else tabs.push({ id: nextId++, space, name: null, direction: "row", panes: [id] });
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
        return launchers;
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
    await screen.findByText("AGENTS");
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
    fireEvent.click(screen.getByRole("button", { name: "New terminal" }));
    await screen.findByRole("tab", { name: /bash 2/ });
    expect(calls.find((c) => c.cmd === "terminal_create")?.args.opts).toEqual({ space: 1 });
    fireEvent.click(screen.getByRole("button", { name: "Split bash 2 right" }));
    await waitFor(() => expect(calls.filter((c) => c.cmd === "terminal_create")[1]?.args.opts).toEqual({ split_of: 2, direction: "row" }));
    await waitFor(() => expect(document.querySelectorAll(".xterm-box").length).toBe(2));
  });

  it("closing a running terminal asks before terminating its process", async () => {
    spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: null }];
    tabs = [{ id: 3, space: 1, name: null, direction: "row", panes: [2] }];
    sessions = [session(2, 1, 3, "running")];
    focused = 2;
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    await screen.findByRole("tab", { name: /bash 2/ });
    fireEvent.click(screen.getByRole("button", { name: "Close pane bash 2" }));
    await screen.findByRole("alertdialog", { name: "Confirm close" });
    expect(calls.some((c) => c.cmd === "terminal_close")).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Close and terminate" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_close")?.args).toEqual({ id: 2 }));
  });

  // Regression (unrun): the terminal must be fitted and the PTY resized when it mounts in a box
  // with a real size, and panes must share the whole area (flex fractions sum to 1, no dead band).
  it("sends PTY rows/cols on mount and panes fill the area", async () => {
    const w = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    const h = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, value: 800 });
    Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, value: 600 });
    try {
      spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: null }];
      tabs = [{ id: 3, space: 1, name: null, direction: "column", panes: [2, 4] }];
      sessions = [session(2, 1, 3, "idle"), session(4, 1, 3, "idle")];
      focused = 2;
      render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
      await waitFor(() => expect(calls.filter((c) => c.cmd === "terminal_resize").map((c) => c.args.id).sort()).toEqual([2, 4]));
      expect(calls.find((c) => c.cmd === "terminal_resize")?.args).toMatchObject({ cols: 80, rows: 24 });
      const wraps = [...document.querySelectorAll<HTMLElement>(".term-pane-wrap")];
      expect(wraps.map((e) => e.style.flexGrow)).toEqual(["0.5", "0.5"]);
    } finally {
      if (w) Object.defineProperty(HTMLElement.prototype, "clientWidth", w);
      if (h) Object.defineProperty(HTMLElement.prototype, "clientHeight", h);
    }
  });

  it("a pane whose session is gone shows a session picker, not a blank rectangle", async () => {
    spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: null }];
    tabs = [{ id: 3, space: 1, name: null, direction: "row", panes: [2, 9] }];
    sessions = [session(2, 1, 3, "idle")];
    focused = 2;
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    await screen.findByRole("group", { name: "Empty pane" });
  });

  // The default action opens a normal shell: no agent launcher bar, no agent command is ever sent.
  it("the + control opens a normal shell tab and there is no agent launcher", async () => {
    spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: null }];
    nextId = 2;
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    await screen.findByText("Board");
    expect(screen.queryByText(/New agent/i)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "New terminal" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_create")?.args.opts).toEqual({ space: 1 }));
    await screen.findByRole("tab", { name: /bash 2/ });
  });

  it("the shell menu opens a terminal with the chosen shell", async () => {
    spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: null }];
    nextId = 2;
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    await screen.findByText("Board");
    fireEvent.click(screen.getByRole("button", { name: "Choose shell" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "/bin/zsh" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_create")?.args.opts).toEqual({ space: 1, shell: "/bin/zsh" }));
  });

  it("Agents lists only detected agent sessions with their place and state, and a click navigates", async () => {
    spaces = [
      { id: 1, name: "Board", root: "/projects/board", project_id: null },
      { id: 5, name: "Power", root: "/projects/power", project_id: null },
    ];
    tabs = [
      { id: 3, space: 1, name: null, direction: "row", panes: [2] },
      { id: 6, space: 5, name: "Review", direction: "row", panes: [7, 8] },
    ];
    sessions = [session(2, 1, 3, "idle"), session(7, 5, 6, "running", "claude"), session(8, 5, 6, "waiting_for_input", "codex")];
    focused = 2;
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    const list = await screen.findByRole("region", { name: "Agents" });
    // The plain shell (session 2) is never shown as an agent.
    expect(within(list).getAllByRole("button")).toHaveLength(2);
    const claude = within(list).getByRole("button", { name: /claude/ });
    expect(claude.textContent).toContain("Power / Review / pane 1");
    expect(claude.textContent).toContain("working");
    expect(within(list).getByRole("button", { name: /codex/ }).textContent).toContain("blocked");
    fireEvent.click(claude);
    await waitFor(() => expect(calls.some((c) => c.cmd === "terminal_focus" && c.args.id === 7)).toBe(true));
  });

  it("with no detected agent the list says EAK never starts one", async () => {
    spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: null }];
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    const list = await screen.findByRole("region", { name: "Agents" });
    expect(list.textContent).toContain("never starts, installs or configures an agent");
  });

  it("tabs: rename, close and reorder go through the backend", async () => {
    spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: null }];
    tabs = [
      { id: 3, space: 1, name: null, direction: "row", panes: [2] },
      { id: 4, space: 1, name: "Build", direction: "row", panes: [5] },
    ];
    sessions = [session(2, 1, 3, "idle"), session(5, 1, 4, "idle")];
    focused = 2;
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    const build = await screen.findByRole("tab", { name: /Build/ });
    fireEvent.doubleClick(build);
    const form = await screen.findByRole("form", { name: "Rename tab" });
    fireEvent.change(within(form).getByRole("textbox"), { target: { value: "Fab" } });
    fireEvent.click(within(form).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_tab_rename")?.args).toEqual({ tab: 4, name: "Fab" }));
    // Context menu: move the second tab left.
    fireEvent.contextMenu(build);
    fireEvent.click(await screen.findByRole("menuitem", { name: "Move left" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_move_tab")?.args).toEqual({ tab: 4, index: 0 }));
    // Closing an idle tab needs no confirmation.
    fireEvent.click(screen.getByRole("button", { name: "Close tab Build" }));
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_close")?.args).toEqual({ id: 5 }));
  });

  it("every pane has its own header; the details panel is hidden until toggled", async () => {
    spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: null }];
    tabs = [{ id: 3, space: 1, name: null, direction: "row", panes: [2, 4] }];
    sessions = [session(2, 1, 3, "idle"), session(4, 1, 3, "running")];
    focused = 2;
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    await screen.findByRole("tab", { name: /bash 2/ });
    expect(document.querySelectorAll(".term-pane-head").length).toBe(2);
    expect(screen.queryByRole("complementary", { name: "Session details" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    const aside = await screen.findByRole("complementary", { name: "Session details" });
    expect(aside.textContent).toContain("bash 2");
    expect(screen.getByLabelText("Session status").textContent).toContain("pane 1/2");
  });

  it("Ctrl+Shift+T opens a normal shell and Ctrl+Shift+P toggles the details panel", async () => {
    spaces = [{ id: 1, name: "Board", root: "/projects/board", project_id: null }];
    nextId = 2;
    render(<TerminalWorkspace visible project={null} newSpaceNonce={0} />);
    const ws = await screen.findByLabelText("Terminal workspace");
    fireEvent.keyDown(ws, { key: "T", ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(calls.find((c) => c.cmd === "terminal_create")?.args.opts).toEqual({ space: 1 }));
    fireEvent.keyDown(ws, { key: "P", ctrlKey: true, shiftKey: true });
    await screen.findByRole("complementary", { name: "Session details" });
  });
});

// Regression: tab.direction is "row"/"column" and is used as a class on .term-panes. The global
// `.row` utility (align-items:center, wrap) used to shrink the pane to content height (10 px host,
// blank terminal). jsdom cannot lay out, so assert the override in the stylesheet source.
describe("terminal pane layout css", () => {
  it(".term-panes resets the global .row utility so panes stretch to the full area", () => {
    const rule = css.split("\n").find((line: string) => line.startsWith(".term-panes {")) ?? "";
    expect(rule).toContain("align-items: stretch");
    expect(rule).toContain("flex-wrap: nowrap");
  });
});
