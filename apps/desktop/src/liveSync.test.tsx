// Live synchronisation: the backend's revision-changed event makes the open workspace show an
// agent's commit at once, refetching once, only for the areas that changed, and never from an
// event that is older than what is on screen.
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { fixture } from "./fixture.test-data";
import { domainReport } from "./domain.test-data";
import { acceptsSnapshot, exactChange, isStale, needsRefetch, REVISION_EVENT, type RevisionChange } from "./revisionSync";
import { shellCmd } from "./shell.test-data";

const ev = (over: Partial<RevisionChange>): RevisionChange => ({
  project_id: "prj_1",
  dir: "/projects/fixture.eak",
  revision_before: 7,
  revision_after: 8,
  source: "harness",
  session_id: "sess-1",
  areas: ["pcb"],
  ...over,
});

describe("revision event decisions", () => {
  const shown = { projectId: "prj_1", revision: 7 };

  it("treats other projects and already-shown revisions as stale", () => {
    expect(isStale(ev({}), shown)).toBe(false);
    expect(isStale(ev({ revision_after: 7 }), shown)).toBe(true);
    expect(isStale(ev({ revision_after: 5, revision_before: 4 }), shown)).toBe(true);
    expect(isStale(ev({ project_id: "prj_other" }), shown)).toBe(true);
  });

  it("accepts a fetched snapshot only when it is the same project and strictly newer", () => {
    expect(acceptsSnapshot({ projectId: "prj_1", revision: 8 }, shown)).toBe(true);
    expect(acceptsSnapshot({ projectId: "prj_1", revision: 7 }, shown)).toBe(false);
    expect(acceptsSnapshot({ projectId: "prj_2", revision: 9 }, shown)).toBe(false);
  });

  it("knows which areas an exact change touched, and refetches everything otherwise", () => {
    const exact = exactChange(ev({}), shown);
    expect(exact).not.toBeNull();
    expect(needsRefetch(exact, 8, ["schematic", "pcb"])).toBe(true);
    expect(needsRefetch(exact, 8, ["library"])).toBe(false);
    // The event skipped revisions the screen never saw: its areas say nothing about them.
    expect(exactChange(ev({ revision_before: 9, revision_after: 10 }), shown)).toBeNull();
    expect(needsRefetch(null, 8, ["library"])).toBe(true);
    // An exact change for another revision, or one without area information, never skips a refetch.
    expect(needsRefetch(exact, 9, ["library"])).toBe(true);
    expect(needsRefetch(ev({ areas: [] }), 8, ["library"])).toBe(true);
  });
});

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];
let revision: number;
const count = (cmd: string) => calls.filter((c) => c.cmd === cmd).length;

beforeEach(() => {
  calls = [];
  revision = 7;
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      calls.push({ cmd, args });
      if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown command ${cmd}`);
      switch (cmd) {
        case "recent_projects":
          return [{ path: "/projects/fixture.eak", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
        case "open_project":
        case "current_project":
          return fixture(revision);
        case "events_since":
          return [];
        case "connectivity":
          return { revision, net_groups: {}, shorts: [], ratsnest: [] };
        case "domain_classification":
          return domainReport(revision);
        default: {
          const h = shellCmd(cmd, null);
          if (h.handled) return h.value;
          throw new Error(`unexpected ${cmd}`);
        }
      }
    },
    { shouldMockEvents: true },
  );
});

afterEach(() => {
  cleanup();
  clearMocks();
});

async function openFromRecent() {
  render(<App />);
  fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
  await screen.findByText("revision 7");
  // Let the workspace's effects (including the event listener registration) settle.
  await act(async () => {
    await new Promise((r) => setTimeout(r, 50));
  });
}

const announce = (e: RevisionChange) => act(async () => emit(REVISION_EVENT, e));

describe("workspace live synchronisation", () => {
  it("shows an agent's commit from the event, refetching the snapshot once and only the changed areas", async () => {
    await openFromRecent();
    const snapshots = count("current_project");
    const airwires = count("connectivity");
    const classifications = count("domain_classification");

    revision = 8;
    await announce(ev({ areas: ["pcb"] }));
    await screen.findByText("revision 8");
    expect(count("current_project")).toBe(snapshots + 1);
    // PCB changed: the airwires are recomputed, the schematic-level classification is not.
    await waitFor(() => expect(count("connectivity")).toBe(airwires + 1));
    expect(count("domain_classification")).toBe(classifications);
  });

  it("skips derived refetches when nothing they read changed", async () => {
    await openFromRecent();
    const airwires = count("connectivity");
    revision = 8;
    await announce(ev({ areas: ["manufacturing"] }));
    await screen.findByText("revision 8");
    expect(count("connectivity")).toBe(airwires);
  });

  it("ignores stale events and events for another project", async () => {
    await openFromRecent();
    const snapshots = count("current_project");
    await announce(ev({ revision_before: 6, revision_after: 7 }));
    await announce(ev({ project_id: "prj_other", revision_before: 7, revision_after: 9 }));
    // Give any (wrongly) triggered fetch time to happen before asserting that none did.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });
    expect(count("current_project")).toBe(snapshots);
    expect(screen.getByText("revision 7")).toBeTruthy();
  });
});
