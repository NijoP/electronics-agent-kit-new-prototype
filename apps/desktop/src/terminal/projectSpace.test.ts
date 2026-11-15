// Regression: an agent launched in a space bound to an older project edited that project, so the
// open schematic/PCB never showed its work (found on Windows; the agent wrote to "hi" while the
// app had "ESP32 DEV BOARD" open).
import { describe, expect, it } from "vitest";
import { boundToOtherProject, sameDir, spaceForProject } from "./projectSpace";
import type { Space } from "./api";

const sp = (id: number, root: string, project_id: string | null, name = `s${id}`): Space => ({ id, name, root, project_id });
const open = { dir: "C:\\test of eak\\New folder\\ESP32 DEV BOARD", name: "ESP32 DEV BOARD", id: "prj_new" };

describe("terminal space for the open project", () => {
  it("compares folders across separators, trailing slash and Windows drive case", () => {
    expect(sameDir("C:\\a\\B dir\\", "c:/a/b dir")).toBe(true);
    expect(sameDir("/home/u/Proj", "/home/u/proj")).toBe(false);
  });
  it("finds the space by project id, else by folder, never another project's space", () => {
    const home = sp(1, "C:\\Users\\HP", null, "Home");
    const old = sp(2, "C:\\test of eak\\hi", "prj_old", "hi");
    expect(spaceForProject([home, old], open)).toBeNull();
    const byDir = sp(3, "c:/test of eak/new folder/esp32 dev board/", null);
    expect(spaceForProject([home, old, byDir], open)?.id).toBe(3);
    const byId = sp(4, "D:\\elsewhere", "prj_new");
    expect(spaceForProject([home, old, byDir, byId], open)?.id).toBe(4);
  });
  it("flags a space bound to a different project, not plain or matching spaces", () => {
    expect(boundToOtherProject(sp(2, "C:\\test of eak\\hi", "prj_old"), open)).toBe(true);
    expect(boundToOtherProject(sp(4, "D:\\x", "prj_new"), open)).toBe(false);
    expect(boundToOtherProject(sp(1, "C:\\Users\\HP", null), open)).toBe(false);
    expect(boundToOtherProject(sp(2, "C:\\x", "prj_old"), null)).toBe(false);
  });
});
