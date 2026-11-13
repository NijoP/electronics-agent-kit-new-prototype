// End-to-end UI flow against a mocked Tauri IPC: verifies command names and argument
// shapes the Rust layer expects, and that the UI re-renders engine state only.
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { ercResult, fixture } from "./fixture.test-data";
import type { CommandResponse } from "./types";

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];
let revision: number;

const response = (command: string, accepted: boolean): CommandResponse => ({
  op_id: "op_1",
  command,
  class: "reversible_local",
  accepted,
  revision_before: revision,
  revision_after: accepted ? revision + 1 : revision,
  created: [],
  affected: [],
  warnings: [],
  error: accepted ? null : { code: "StaleRevision", message: { expected: 1, actual: 2 } },
  validation_note: "command accepted is not engineering validation; run ERC/DRC for evidence",
});

beforeEach(() => {
  calls = [];
  revision = 7;
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown command ${cmd}`);
    switch (cmd) {
      case "recent_projects":
        return [{ path: "/projects/fixture.eak", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
      case "open_project":
      case "create_project":
      case "current_project":
        return fixture(revision);
      case "events_since":
        return args.seq === 0
          ? [{ seq: 1, op_id: "-", kind: "project_opened", actor: "system", summary: "opened at revision 7", revision: 7, timestamp_ms: 1 }]
          : [];
      case "undo": {
        const ok = args.expected_revision === revision;
        const r = response("Undo", ok);
        if (ok) revision += 1;
        return r;
      }
      case "execute_command":
        return response(String((args.command as { type: string }).type), false);
      case "run_check":
        return ercResult(revision);
      case "close_project":
        return null;
      default:
        throw new Error(`unexpected ${cmd}`);
    }
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

async function openFromRecent() {
  render(<App />);
  fireEvent.click(await screen.findByText("Fixture board"));
  await screen.findByRole("img", { name: "Schematic canvas" });
}

describe("desktop shell flow", () => {
  it("lists recent projects and opens one into the workspace", async () => {
    await openFromRecent();
    expect(calls.find((c) => c.cmd === "open_project")!.args).toEqual({
      path: "/projects/fixture.eak",
      take_over_lock: false,
    });
    expect(await screen.findByText(/opened at revision 7/)).toBeTruthy();
    expect(screen.getByText("revision 7")).toBeTruthy();
  });

  it("creates a project with the typed path and name", async () => {
    render(<App />);
    fireEvent.change(screen.getByLabelText(/New project folder/), { target: { value: "/p/new.eak" } });
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "New" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    await screen.findByRole("img", { name: "PCB canvas" });
    expect(calls.find((c) => c.cmd === "create_project")!.args).toEqual({ path: "/p/new.eak", name: "New" });
  });

  it("undo sends the rendered revision and the view refreshes from the engine", async () => {
    await openFromRecent();
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    await screen.findByText("revision 8");
    expect(calls.find((c) => c.cmd === "undo")!.args).toEqual({ expected_revision: 7 });
    expect(screen.getByRole("status").textContent).toContain("Undo → revision 8");
  });

  it("palette commands go to execute_command and rejections are shown", async () => {
    await openFromRecent();
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    const input = await screen.findByRole("textbox", { name: "Search commands" });
    fireEvent.change(input, { target: { value: "sync" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(screen.getByRole("status").textContent).toContain("rejected"));
    expect(calls.find((c) => c.cmd === "execute_command")!.args).toEqual({
      expected_revision: 7,
      command: { type: "SyncSchematicToPcb" },
    });
  });

  it("runs ERC and shows findings in the Engineering Monitor", async () => {
    await openFromRecent();
    fireEvent.click(screen.getByRole("button", { name: "Run ERC" }));
    expect(await screen.findByText(/D1 pin 1 \(K\) is unconnected/)).toBeTruthy();
    expect(calls.find((c) => c.cmd === "run_check")!.args).toEqual({ check: "erc" });
  });

  it("close returns to Project Home", async () => {
    await openFromRecent();
    fireEvent.click(screen.getByRole("button", { name: "Close project" }));
    await screen.findByRole("heading", { name: "Recent projects" });
  });
});
