// Settings › AI tools rows against a mocked Tauri IPC (remap W2-E): a tool that is not installed
// reads "Not found" with every action unavailable and a reason; an installed tool shows its
// version and the sign-in state its own command reported ("unknown" stays "unknown"); Connect,
// Sign in, Launch, Stop and Show log call exactly the registered agent commands.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "../api";
import { AiToolsSettings } from "./AiToolsSettings";
import { connectedText, foundText, launchBlocker, reasonText, signedInText, type AgentSession, type ToolStatus } from "./aiTools";

const row = (t: Partial<ToolStatus> & Pick<ToolStatus, "id" | "label">): ToolStatus => ({
  install_url: `https://example.invalid/${t.id}`,
  binary: null,
  version: null,
  available: false,
  reason: { kind: "missing_binary" },
  signed_in: null,
  auth_detail: null,
  connected: null,
  connection_detail: `${t.label} is not installed`,
  connection_scope: "scope text",
  can_connect: true,
  can_login: true,
  surfaces: ["terminal", "headless"],
  ...t,
});

const claudeFound = row({
  id: "claude",
  label: "Claude Code",
  binary: "/home/u/.local/bin/claude",
  version: "2.1.3",
  available: true,
  reason: null,
  signed_in: true,
  auth_detail: "signed in with a claude.ai account",
  connected: false,
  connection_detail: "EAK is not registered with Claude Code for this project",
});
const codexMissing = row({ id: "codex", label: "Codex" });
const geminiUnknown = row({
  id: "gemini",
  label: "Gemini CLI",
  binary: "/usr/bin/gemini",
  version: "0.9.0",
  available: true,
  reason: null,
  signed_in: null,
  auth_detail: "this tool has no sign-in status command EAK can read",
  connected: null,
  connection_detail: "EAK cannot register its MCP server with this tool yet",
  can_connect: false,
  can_login: false,
  surfaces: ["terminal"],
});

type Call = { cmd: string; args: Record<string, any> };
let calls: Call[];
let sessions: AgentSession[];

beforeEach(() => {
  calls = [];
  sessions = [];
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, any>;
    calls.push({ cmd, args });
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown command ${cmd}`);
    switch (cmd) {
      case "current_project":
        return { project: { id: "p1" } };
      case "agent_tools":
        return [claudeFound, codexMissing, geminiUnknown];
      case "agent_tool_connect":
        return { ...claudeFound, connected: true, connection_detail: "registered: Local config" };
      case "agent_tool_login":
        return { id: 7 };
      case "agent_tool_launch": {
        const s: AgentSession = {
          id: "claude-1-1",
          tool: "claude",
          label: "Claude Code",
          surface: args.surface,
          project: "/work/board",
          prompt: args.prompt,
          started_at: 1,
          state: "running",
          detail: "running",
          exit_code: null,
          pid: 42,
          terminal_session: null,
          log_file: "/data/agent-sessions/claude-1-1.log",
          events: [{ kind: "tool_call", name: "mcp__eak__design_report" }],
        };
        sessions = [s];
        return s;
      }
      case "agent_sessions":
        return sessions;
      case "agent_session_log":
        return "line one\nline two";
      case "agent_tool_stop":
        sessions = sessions.map((s) => ({ ...s, state: "stopped" as const }));
        return sessions[0];
      default:
        throw new Error(`unexpected ${cmd}`);
    }
  });
});
afterEach(() => {
  cleanup();
  clearMocks();
});

describe("AI tool row facts", () => {
  it("never says yes for a state the tool did not report", () => {
    expect(foundText(codexMissing)).toBe("Not found");
    expect(signedInText(codexMissing)).toBe("—");
    expect(reasonText(codexMissing)).toContain("Not installed");
    expect(foundText(claudeFound)).toBe("Found · v2.1.3");
    expect(signedInText(geminiUnknown)).toBe("unknown");
    expect(connectedText(geminiUnknown)).toBe("not supported");
    expect(launchBlocker(claudeFound, "headless", "  ", true)).toContain("first message");
    expect(launchBlocker(geminiUnknown, "headless", "go", true)).toContain("no confirmed headless");
    expect(launchBlocker(claudeFound, "terminal", "", false)).toContain("Open a project");
  });
});

describe("Settings › AI tools", () => {
  const rowOf = async (label: string) => within((await screen.findByRole("rowheader", { name: label })).closest("tr") as HTMLElement);

  it("shows found / version / signed in / connected and disables what cannot run, with a reason", async () => {
    render(<AiToolsSettings />);
    const codex = await rowOf("Codex");
    expect(codex.getByText("Not found")).toBeTruthy();
    for (const name of ["Connect", "Disconnect", "Sign in", "Launch session"]) {
      const b = codex.getByRole("button", { name }) as HTMLButtonElement;
      expect(b.disabled, name).toBe(true);
      expect(b.title, name).not.toBe("");
    }
    const claude = await rowOf("Claude Code");
    expect(claude.getByText("Found · v2.1.3")).toBeTruthy();
    expect(claude.getByText("yes")).toBeTruthy();
    expect(claude.getByText("no")).toBeTruthy();
    const gemini = await rowOf("Gemini CLI");
    expect(gemini.getByText("unknown")).toBeTruthy();
    expect((gemini.getByRole("button", { name: "Sign in" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("Connect registers through the IPC and the row turns connected", async () => {
    render(<AiToolsSettings />);
    const claude = await rowOf("Claude Code");
    await waitFor(() => expect((claude.getByRole("button", { name: "Connect" }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(claude.getByRole("button", { name: "Connect" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "agent_tool_connect" && c.args.tool === "claude")).toBe(true));
    await waitFor(() => expect((claude.getByRole("button", { name: "Disconnect" }) as HTMLButtonElement).disabled).toBe(false));
    expect((claude.getByRole("button", { name: "Connect" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("Sign in opens the tool's own login in a terminal tab", async () => {
    render(<AiToolsSettings />);
    const claude = await rowOf("Claude Code");
    fireEvent.click(claude.getByRole("button", { name: "Sign in" }));
    expect((await screen.findByRole("status")).textContent).toContain("Terminal workspace");
    expect(calls.find((c) => c.cmd === "agent_tool_login")?.args).toEqual({ tool: "claude" });
  });

  it("a headless session is launched, listed with its last event, its log shown, and stopped", async () => {
    render(<AiToolsSettings />);
    const claude = await rowOf("Claude Code");
    await waitFor(() => expect((claude.getByRole("button", { name: "Connect" }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(screen.getByRole("radio", { name: /Headless/ }));
    const launch = () => claude.getByRole("button", { name: "Launch session" }) as HTMLButtonElement;
    expect(launch().disabled).toBe(true);
    fireEvent.change(screen.getByRole("textbox", { name: "First message" }), { target: { value: "Review the power rails" } });
    expect(launch().disabled).toBe(false);
    fireEvent.click(launch());
    await waitFor(() => expect(calls.find((c) => c.cmd === "agent_tool_launch")?.args).toEqual({ tool: "claude", surface: "headless", prompt: "Review the power rails" }));
    const list = await screen.findByRole("table", { name: "AI sessions" });
    expect(within(list).getByText("tool: mcp__eak__design_report")).toBeTruthy();
    expect((await screen.findByRole("region", { name: "Session log" })).textContent).toContain("line two");
    await waitFor(() => expect((within(list).getByRole("button", { name: "Stop" }) as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(within(list).getByRole("button", { name: "Stop" }));
    await waitFor(() => expect(calls.some((c) => c.cmd === "agent_tool_stop" && c.args.id === "claude-1-1")).toBe(true));
  });

  it("an IPC failure is shown, not hidden", async () => {
    mockIPC((cmd) => {
      if (cmd === "agent_tools") throw "probe exploded";
      if (cmd === "agent_sessions") return [];
      if (cmd === "current_project") throw "no project is open";
      throw new Error(`unexpected ${cmd}`);
    });
    render(<AiToolsSettings />);
    expect((await screen.findByRole("alert")).textContent).toContain("probe exploded");
  });
});
