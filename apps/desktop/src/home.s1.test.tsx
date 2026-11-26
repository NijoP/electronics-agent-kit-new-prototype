// Project Home (slice S1) against a mocked Tauri IPC: the first-run tour (skip, replay from Help
// on Home and in the workspace), the Continue card's facts and Needs attention jumps, the brief
// created from the prompt box without AI, and the sample board. Every start is checked by the
// IPC command and arguments it sends.
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => "/chosen") }));
import { IPC_COMMANDS } from "./api";
import { App } from "./App";
import { domainReport } from "./domain.test-data";
import { ercResult, fixture } from "./fixture.test-data";
import type { HomeFacts } from "./home/homeFacts";
import { TOUR_KEY } from "./shell/onboarding/tour";
import { shellCmd } from "./shell.test-data";

type Call = { cmd: string; args: Record<string, unknown> };
let calls: Call[];
let defaultDir: string | null;
let lastSnap: ReturnType<typeof fixture>;

const facts = (): HomeFacts => ({
  path: "/projects/fixture",
  name: "Fixture board",
  revision: 7,
  phase: "Schematic capture",
  design: { sheets: 1, parts: 3, nets: 2, footprints: 0, missing_footprints: 3, tracks: 0, vias: 0, zones: 0, outline: false, bom_lines: 2, without_supplier: 3, without_mpn: 2, requirements: 0 },
  checks: [{ check: "ERC", errors: 2, warnings: 1, unknown_rules: 0 }],
  unrouted_nets: 0,
  unrouted_names: [],
  unrouted_connections: 0,
  shorts: 0,
  release: { status: "DRAFT", required: 6, passing: 1, blocking: ["ERC reports 2 error(s)"] },
});

const briefed = () => {
  const s = fixture(1);
  return { ...s, project: { ...s.project, requirements: [{ id: "req_1", text: "5 V input from a 2-pin header." }] } };
};

beforeEach(() => {
  localStorage.clear();
  calls = [];
  defaultDir = "/docs/EAK Projects";
  lastSnap = fixture(7);
  mockIPC((cmd, payload) => {
    const args = (payload ?? {}) as Record<string, unknown>;
    calls.push({ cmd, args });
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown command ${cmd}`);
    switch (cmd) {
      case "recent_projects":
        return [{ path: "/projects/fixture", project_id: "prj_1", name: "Fixture board", revision: 7, last_opened_ms: 1 }];
      case "home_project_facts":
        return facts();
      case "home_default_dir":
        return defaultDir;
      case "home_suggest_folder":
        return `${args.parent}/${args.name}`;
      case "create_project_with_brief":
        lastSnap = briefed();
        return lastSnap;
      case "create_sample_project":
      case "open_project":
        lastSnap = fixture(7);
        return lastSnap;
      case "current_project":
        return lastSnap;
      case "events_since":
        return [];
      case "domain_classification":
        return domainReport(7);
      case "run_check":
        return ercResult(7);
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
});

const ran = (cmd: string) => calls.filter((c) => c.cmd === cmd);
const tour = () => screen.queryByRole("region", { name: "First-run tour" });

describe("first-run tour", () => {
  it("shows on first launch, steps, can be skipped, and is replayed from Home's Help menu", async () => {
    render(<App />);
    const t = await screen.findByRole("region", { name: "First-run tour" });
    expect(within(t).getByText("Step 1 of 4")).toBeTruthy();
    fireEvent.click(within(t).getByRole("button", { name: "Next" }));
    expect(within(t).getByText("Step 2 of 4")).toBeTruthy();
    fireEvent.click(within(t).getByRole("button", { name: "Back" }));
    expect(within(t).getByText("Step 1 of 4")).toBeTruthy();
    fireEvent.click(within(t).getByRole("button", { name: "Skip tour" }));
    expect(tour()).toBeNull();
    expect(localStorage.getItem(TOUR_KEY)).toBe("skipped");

    fireEvent.click(screen.getByRole("button", { name: "Help" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Replay first-run tour" }));
    const again = await screen.findByRole("region", { name: "First-run tour" });
    for (let i = 0; i < 3; i++) fireEvent.click(within(again).getByRole("button", { name: "Next" }));
    expect(within(again).getByText("Step 4 of 4")).toBeTruthy();
    fireEvent.click(within(again).getByRole("button", { name: "Done" }));
    expect(tour()).toBeNull();
    expect(localStorage.getItem(TOUR_KEY)).toBe("done");
  });

  it("stays closed once finished, and Help in the workspace replays it as a corner panel", async () => {
    localStorage.setItem(TOUR_KEY, "done");
    render(<App />);
    await screen.findByRole("heading", { name: "Recent projects" });
    expect(tour()).toBeNull();
    fireEvent.click((await screen.findAllByText("Fixture board"))[0]);
    await screen.findAllByRole("img", { name: "Schematic canvas" });
    const bar = within(screen.getByRole("navigation", { name: "Application menu" }));
    fireEvent.click(bar.getByRole("button", { name: "Help" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Replay first-run tour" }));
    const t = await screen.findByRole("region", { name: "First-run tour" });
    expect(t.className).toContain("floating");
    fireEvent.click(within(t).getByRole("button", { name: "Skip tour" }));
    expect(tour()).toBeNull();
  });
});

describe("Continue and Needs attention", () => {
  it("shows facts read from the folder and opens the project where an item points", async () => {
    localStorage.setItem(TOUR_KEY, "done");
    render(<App />);
    const card = within(await screen.findByRole("region", { name: "Continue" }));
    await card.findByText("1 of 6 release gates pass · Draft · current reports: ERC");
    expect(ran("home_project_facts")[0].args).toEqual({ path: "/projects/fixture" });
    const list = within(card.getByRole("list", { name: "Needs attention" }));
    // Worst first: the saved ERC errors, then the board behind the schematic and the BOM risk.
    const items = list.getAllByRole("button").map((b) => b.querySelector("strong")!.textContent);
    expect(items).toEqual(["ERC: 2 errors", "3 parts not on the board", "No board outline", "BOM risk", "No design brief"]);
    fireEvent.click(list.getByText("ERC: 2 errors"));
    await screen.findAllByRole("img", { name: "Schematic canvas" });
    expect(ran("open_project").at(-1)!.args).toEqual({ path: "/projects/fixture", take_over_lock: false });
    // The jump re-runs the read-only check so its findings are listed for this revision.
    await waitFor(() => expect(ran("run_check").map((c) => c.args.check)).toContain("erc"));
  });

  it("says when the facts cannot be read instead of showing numbers", async () => {
    localStorage.setItem(TOUR_KEY, "done");
    mockIPC((cmd) => {
      if (cmd === "recent_projects") return [{ path: "/gone", project_id: "prj_1", name: "Moved board", revision: 3, last_opened_ms: 1 }];
      if (cmd === "home_project_facts") throw new Error("the project folder cannot be read");
      const h = shellCmd(cmd, null);
      if (h.handled) return h.value;
      throw new Error(`unmocked ${cmd}`);
    });
    render(<App />);
    const card = within(await screen.findByRole("region", { name: "Continue" }));
    await card.findByText(/Facts unavailable: the project folder cannot be read/);
    expect(card.queryByRole("list", { name: "Needs attention" })).toBeNull();
  });
});

describe("starting a design from Home", () => {
  it("records the typed description as the brief of a new project in the default folder (no AI)", async () => {
    localStorage.setItem(TOUR_KEY, "done");
    render(<App />);
    fireEvent.change(await screen.findByLabelText("Describe the board"), { target: { value: "5 V input from a 2-pin header. 3.3 V rail, at most 300 mA." } });
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    const form = within(await screen.findByRole("form", { name: "Create from brief" }));
    expect((form.getByLabelText("Project name") as HTMLInputElement).value).toBe("5 V input from a 2-pin header");
    expect(await form.findByText("/docs/EAK Projects")).toBeTruthy();
    expect(within(form.getByRole("list", { name: "Brief lines" })).getAllByRole("listitem").map((li) => li.textContent)).toEqual(["5 V input from a 2-pin header.", "3.3 V rail, at most 300 mA."]);
    fireEvent.change(form.getByLabelText("Project name"), { target: { value: "LDO board" } });
    fireEvent.click(form.getByRole("button", { name: "Create project and brief" }));
    await screen.findAllByRole("img", { name: "Schematic canvas" });
    expect(ran("home_suggest_folder")[0].args).toEqual({ parent: "/docs/EAK Projects", name: "LDO board" });
    expect(ran("create_project_with_brief")[0].args).toEqual({ path: "/docs/EAK Projects/LDO board", name: "LDO board", brief: ["5 V input from a 2-pin header.", "3.3 V rail, at most 300 mA."] });
    expect(ran("ai_propose")).toHaveLength(0);
    // It lands on the Overview, whose Brief section lists the recorded lines.
    const lines = await screen.findByRole("list", { name: "Brief lines" });
    expect(within(lines).getByText("5 V input from a 2-pin header.")).toBeTruthy();
  });

  it("refuses an empty description without creating anything", async () => {
    localStorage.setItem(TOUR_KEY, "done");
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Start" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Describe the board first");
    expect(ran("create_project_with_brief")).toHaveLength(0);
  });

  it("builds the sample board in a free folder through the engine and opens its PCB", async () => {
    localStorage.setItem(TOUR_KEY, "done");
    render(<App />);
    // The default folder is shown once it is known (New Project form), then the sample uses it.
    fireEvent.click(await screen.findByRole("button", { name: "New Project" }));
    await screen.findByText("/docs/EAK Projects");
    fireEvent.click(await screen.findByRole("button", { name: "Open sample board" }));
    await waitFor(() => expect(ran("create_sample_project")).toHaveLength(1));
    expect(ran("home_suggest_folder")[0].args).toEqual({ parent: "/docs/EAK Projects", name: "Sample board" });
    expect(ran("create_sample_project")[0].args).toEqual({ path: "/docs/EAK Projects/Sample board" });
  });

  it("asks for a folder for the sample when there is no default, and remembers it", async () => {
    localStorage.setItem(TOUR_KEY, "done");
    defaultDir = null;
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Open sample board" }));
    await waitFor(() => expect(ran("create_sample_project")).toHaveLength(1));
    expect(ran("create_sample_project")[0].args).toEqual({ path: "/chosen/Sample board" });
    expect(localStorage.getItem("eak.home.parent")).toBe("/chosen");
  });

  it("offers KiCad library import and shows board import as not available", async () => {
    localStorage.setItem(TOUR_KEY, "done");
    render(<App />);
    fireEvent.click(await screen.findByRole("button", { name: "Import…" }));
    const board = screen.getByRole("menuitem", { name: /KiCad board or project/ });
    expect((board as HTMLButtonElement).disabled).toBe(true);
    expect(board.getAttribute("title")).toContain("Not available yet");
    fireEvent.click(screen.getByRole("menuitem", { name: /KiCad symbols or footprints/ }));
    expect(await screen.findByRole("region", { name: "Import from KiCad" })).toBeTruthy();
  });
});
