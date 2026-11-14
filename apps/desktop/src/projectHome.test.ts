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
