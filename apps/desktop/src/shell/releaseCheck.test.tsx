// @vitest-environment jsdom
// E7 release check against a mocked Tauri IPC: opening the view only reads (release_check), each
// gate shows its state (PASS, FAIL, UNKNOWN, NOT_RUN, DISABLED) and whether it counts, a NOT_RUN
// required gate means DRAFT, the run button saves the four reports (run_release_checks), a new
// revision is read again, and no wording ever claims the board is ready to manufacture. The Rust
// side is tested in src-tauri/tests/ipc.rs (release_check_*).
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { IPC_COMMANDS } from "../api";
import type { ReleaseCheck, ReleaseGate } from "../types";
import { NOT_CERTIFIED, ReleaseCheckDialog, counts, statusLine } from "./ReleaseCheckDialog";

let calls: string[];

const gate = (id: string, state: ReleaseGate["state"], required = true): ReleaseGate => ({
  id,
  title: `Gate ${id}`,
  state,
  required,
  reasons: state === "PASS" ? [] : [`${id} reason`],
  evidence: [`${id} evidence`],
});

/** Before the run: no report saved for the revision (NOT_RUN), the outline gate failing. */
const notRun = (revision = 4): ReleaseCheck => ({
  revision,
  input_sha256: "abc",
  gates: [
    gate("erc", "NOT_RUN"),
    gate("drc", "NOT_RUN"),
    gate("board_outline", "FAIL"),
    gate("fabricator_capability", "DISABLED", false),
    gate("return_path_continuity", "UNKNOWN", false),
  ],
  blocking: ["ERC report not supplied", "DRC report not supplied", "gate board_outline is Fail: no outline"],
  status: "DRAFT",
  not_verified: ["Independent senior electronics engineering review"],
});

/** After the run: both reports saved and passing, the outline gate still failing. */
const afterRun = (): ReleaseCheck => {
  const c = notRun();
  return { ...c, gates: c.gates.map((g) => (g.state === "NOT_RUN" ? { ...g, state: "PASS" as const, reasons: [] } : g)), blocking: ["gate board_outline is Fail: no outline"] };
};

/** Every required gate passing: the engine's release-candidate status. */
const allPass: ReleaseCheck = {
  revision: 4,
  input_sha256: "abc",
  gates: [gate("erc", "PASS"), gate("drc", "PASS"), gate("fabricator_capability", "DISABLED", false)],
  blocking: [],
  status: "INTERNALLY_VERIFIED_RELEASE_CANDIDATE",
  not_verified: ["Fabricator DFM review of these exact files"],
};

let reply: (cmd: string) => ReleaseCheck;

beforeEach(() => {
  calls = [];
  reply = (cmd) => (cmd === "run_release_checks" ? afterRun() : notRun());
  mockIPC((cmd) => {
    if (!(IPC_COMMANDS as readonly string[]).includes(cmd)) throw new Error(`unknown ${cmd}`);
    calls.push(cmd);
    if (cmd === "release_check" || cmd === "run_release_checks") return reply(cmd);
    throw new Error(`unmocked ${cmd}`);
  });
});

afterEach(() => {
  cleanup();
  clearMocks();
});

const row = (id: string) => document.querySelector(`tr[data-gate="${id}"]`) as HTMLElement;
const READY = /manufacturing[- ]ready|ready (for|to) (manufactur|fabricat|production|order)|production[- ]ready|certified for/i;

describe("E7 release check dialog", () => {
  it("opening it only reads; every gate shows its state and whether it counts; NOT_RUN keeps it DRAFT", async () => {
    render(<ReleaseCheckDialog revision={4} busy={false} onClose={() => {}} />);
    const dlg = await screen.findByRole("dialog", { name: "Release check" });
    await within(dlg).findByText("DRAFT — outputs of this revision are not for fabrication");
    expect(calls).toEqual(["release_check"]);
    expect(within(row("erc")).getByText("NOT_RUN")).toBeTruthy();
    expect(within(row("board_outline")).getByText("FAIL")).toBeTruthy();
    expect(within(row("fabricator_capability")).getByText("DISABLED")).toBeTruthy();
    expect(within(row("fabricator_capability")).getByText("not counted")).toBeTruthy();
    expect(within(row("return_path_continuity")).getByText("advisory")).toBeTruthy();
    expect(within(row("erc")).getByText("required")).toBeTruthy();
    expect(within(dlg).getByText("3 of 3 required gates do not pass for revision 4.")).toBeTruthy();
    expect(within(within(dlg).getByRole("list", { name: "Blocking reasons" })).getByText("ERC report not supplied")).toBeTruthy();
    expect(dlg.textContent).toContain(NOT_CERTIFIED);
    expect(dlg.textContent).not.toMatch(READY);
  });

  it("Run release checks saves the reports and shows the new states; still DRAFT while a gate fails", async () => {
    render(<ReleaseCheckDialog revision={4} busy={false} onClose={() => {}} />);
    const dlg = await screen.findByRole("dialog", { name: "Release check" });
    await within(dlg).findByText("3 of 3 required gates do not pass for revision 4.");
    fireEvent.click(within(dlg).getByRole("button", { name: "Run release checks" }));
    await within(dlg).findByText("1 of 3 required gates do not pass for revision 4.");
    expect(calls).toEqual(["release_check", "run_release_checks"]);
    expect(within(row("erc")).getByText("PASS")).toBeTruthy();
    expect(within(row("board_outline")).getByText("FAIL")).toBeTruthy();
    expect(within(dlg).getByText(/^DRAFT/)).toBeTruthy();
  });

  it("while an engine command runs the run button is disabled with the reason; a new revision is read again", async () => {
    const { rerender } = render(<ReleaseCheckDialog revision={4} busy={true} onClose={() => {}} />);
    const dlg = await screen.findByRole("dialog", { name: "Release check" });
    const btn = within(dlg).getByRole("button", { name: "Run release checks" }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    expect(btn.title).toBe("An engine command is running");
    await within(dlg).findByText(/required gates do not pass for revision 4/);
    reply = () => notRun(5);
    rerender(<ReleaseCheckDialog revision={5} busy={false} onClose={() => {}} />);
    await within(dlg).findByText(/required gates do not pass for revision 5/);
    expect(calls).toEqual(["release_check", "release_check"]);
  });

  it("an engine error is shown, and Close and Escape close the view", async () => {
    reply = () => {
      throw new Error("no project is open");
    };
    let closed = 0;
    render(<ReleaseCheckDialog revision={4} busy={false} onClose={() => closed++} />);
    const dlg = await screen.findByRole("dialog", { name: "Release check" });
    expect((await within(dlg).findByRole("alert")).textContent).toContain("no project is open");
    fireEvent.keyDown(dlg, { key: "Escape" });
    fireEvent.click(within(dlg).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(closed).toBe(2));
  });

  it("wording lint: no state ever claims the board is ready to manufacture", () => {
    for (const c of [notRun(), allPass]) {
      expect(statusLine(c)).not.toMatch(READY);
      for (const g of c.gates) expect(counts(g)).not.toMatch(READY);
    }
    expect(statusLine(notRun())).toMatch(/^DRAFT/);
    expect(statusLine(allPass)).not.toMatch(/DRAFT/);
    expect(NOT_CERTIFIED).not.toMatch(READY);
  });

  it("the all-pass view says which label outputs get, never more", async () => {
    reply = () => allPass;
    render(<ReleaseCheckDialog revision={4} busy={false} onClose={() => {}} />);
    const dlg = await screen.findByRole("dialog", { name: "Release check" });
    await within(dlg).findByText("All 2 required gates pass for revision 4.");
    expect(within(dlg).queryByRole("list", { name: "Blocking reasons" })).toBeNull();
    expect(dlg.textContent).toContain("internally verified release candidate");
    expect(dlg.textContent).not.toMatch(READY);
  });

  it("every gate passing is still DRAFT when the engine blocks on an unverified footprint (ADR-0054)", async () => {
    const footprint = "footprint R_0805 of R1,R2 is not verified (3 of 9 rules do not certify): FP-002 is Unknown: no land-pattern table";
    reply = () => ({ ...allPass, blocking: [footprint], status: "DRAFT" });
    render(<ReleaseCheckDialog revision={4} busy={false} onClose={() => {}} />);
    const dlg = await screen.findByRole("dialog", { name: "Release check" });
    const blocking = await within(dlg).findByRole("list", { name: "Blocking reasons" });
    expect(blocking.textContent).toContain(footprint);
    within(dlg).getByText("All 2 required gates pass, but 1 other blocking reason(s) keep revision 4 DRAFT.");
    expect(within(dlg).getByRole("region", { name: "Release status" }).textContent).toMatch(/DRAFT/);
    expect(dlg.textContent).not.toContain("internally verified release candidate");
    expect(dlg.textContent).not.toMatch(READY);
  });
});
