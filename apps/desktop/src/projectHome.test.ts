// Project Home helpers (written during the testing freeze; not yet run).
import { describe, expect, it } from "vitest";
import { ago, filterRecent, projectFolder } from "./ProjectHome";
import type { RecentProject } from "./types";

const row = (name: string, path: string): RecentProject => ({ name, path, project_id: name, revision: 1, last_opened_ms: 0 });
const recent = [row("Motor driver", "/work/boards/motor"), row("Amp", "C:\\eda\\audio\\amp"), row("Sensor", "/home/me/sensor")];

describe("Project Home search", () => {
  it("matches every word against name or folder, case-insensitively", () => {
    expect(filterRecent(recent, "").map((r) => r.name)).toEqual(["Motor driver", "Amp", "Sensor"]);
    expect(filterRecent(recent, "MOTOR").map((r) => r.name)).toEqual(["Motor driver"]);
    expect(filterRecent(recent, "audio amp").map((r) => r.name)).toEqual(["Amp"]);
    expect(filterRecent(recent, "boards sensor")).toEqual([]);
  });
  it("keeps the existing path and time helpers", () => {
    expect(projectFolder("/p/", "Board A")).toBe("/p/Board A");
    expect(projectFolder("C:\\eda", "x")).toBe("C:\\eda\\x");
    expect(ago(0, 90_000)).toBe("1 min ago");
  });
});

describe("Project Home status line (unrun)", () => {
  it("says so when status is unavailable instead of inventing one", async () => {
    const { statusLine } = await import("./ProjectHome");
    expect(statusLine(null)).toContain("unavailable");
  });
  it("lists pending checks and never claims validation passed", async () => {
    const { statusLine } = await import("./ProjectHome");
    const line = statusLine({ last_modified_ms: 0, revision: 3, phase: "Routing", pending_validation: ["DRC"], evidence: [], unresolved_errors: null }, 60_000);
    expect(line).toContain("validation pending: DRC");
    expect(line).not.toContain("error");
  });
});

describe("Project Home lock wording (unrun)", () => {
  it("explains an interrupted session without process ids", async () => {
    const { lockCopy } = await import("./ProjectHome");
    const c = lockCopy({ state: "interrupted", pending_operations: 2 });
    expect(c.title).toBe("Previous session interrupted");
    expect(c.body).toContain("2 unfinished operations");
    expect(c.body).not.toMatch(/process \d/);
  });
  it("never offers takeover wording for a live or unverifiable holder", async () => {
    const { lockCopy } = await import("./ProjectHome");
    expect(lockCopy({ state: "open_elsewhere", this_app: false }).title).toBe("Open elsewhere");
    expect(lockCopy({ state: "unverifiable", reason: "the lock was written on another computer" }).body).toContain("will not take over");
  });
});
describe("Project Home sort (unrun)", () => {
  it("sorts by name, modified and last opened without mutating the input", async () => {
    const { sortRecent } = await import("./ProjectHome");
    const a = { ...row("B", "/b"), last_opened_ms: 1 };
    const b = { ...row("A", "/a"), last_opened_ms: 2 };
    const input = [a, b];
    expect(sortRecent(input, "name").map((r) => r.name)).toEqual(["A", "B"]);
    expect(sortRecent(input, "opened").map((r) => r.name)).toEqual(["A", "B"]);
    expect(input[0]).toBe(a);
  });
});
