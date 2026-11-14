import { describe, expect, it } from "vitest";
import { buildSyncReport, summarizeSyncReport, type EngineChange } from "./syncReport";

const fp = (refdes: string, pad_nets: Record<string, string> = {}, extra: object = {}) => ({ refdes, component_id: "R_0603", side: "top", locked: false, pad_nets, ...extra });
const ch = (key: string, before: unknown, after: unknown, path = ["pcb", "footprints"]): EngineChange => ({ path, key, before, after });
const pcb = (n: number, tracks = 0) => ({
  footprints: Object.fromEntries(Array.from({ length: n }, (_, i) => [`f${i}`, {}])),
  tracks: Object.fromEntries(Array.from({ length: tracks }, (_, i) => [`t${i}`, {}])),
  vias: {},
});

describe("Update PCB report (from the engine's change list)", () => {
  it("reports added components and their connections", () => {
    const r = buildSyncReport([ch("f1", null, fp("R2", { "1": "N1", "2": "GND" }))], pcb(2));
    expect(r.components.added).toEqual([{ refdes: "R2", component: "R_0603" }]);
    expect(r.connections.added).toEqual([{ pad: "R2.1", net: "N1" }, { pad: "R2.2", net: "GND" }]);
    expect(r.preserved.footprints).toBe(1);
    expect(r.inSync).toBe(false);
  });
  it("reports removed components and the connections they took with them", () => {
    const r = buildSyncReport([ch("f1", fp("C1", { "1": "VCC" }), null)], pcb(1));
    expect(r.components.removed).toEqual([{ refdes: "C1", component: "R_0603" }]);
    expect(r.connections.removed).toEqual([{ pad: "C1.1", net: "VCC" }]);
  });
  it("reports modified components: part change, rename, and re-assigned or dropped nets", () => {
    const r = buildSyncReport(
      [
        ch("f1", fp("R1", { "1": "A", "2": "B" }), fp("R1", { "1": "A", "2": "C" })),
        ch("f2", fp("R2", { "1": "X" }), fp("R5", { "1": "X" })),
        ch("f3", fp("R3", { "1": "X" }), fp("R3", {}, { component_id: "R_0805" })),
      ],
      pcb(3),
    );
    expect(r.connections.modified).toEqual([{ pad: "R1.2", from: "B", to: "C" }]);
    expect(r.components.modified).toContainEqual({ refdes: "R5", details: ["reference R2 → R5"] });
    expect(r.connections.removed).toEqual([{ pad: "R3.1", net: "X" }]);
    expect(r.connections.added).toEqual([]); // a rename alone does not read as every pad changing
    expect(r.preserved.footprints).toBe(0);
  });
  it("an empty change list means the PCB was already in sync", () => {
    const r = buildSyncReport([], pcb(2, 3));
    expect(r.inSync).toBe(true);
    expect(r.preserved).toEqual({ footprints: 2, tracks: 3, vias: 0 });
    expect(summarizeSyncReport(r)).toMatch(/already matches/);
  });
  it("counts copper the sync touched instead of claiming it was preserved", () => {
    const r = buildSyncReport([ch("t0", {}, null, ["pcb", "tracks"])], pcb(0, 2));
    expect(r.copper.tracks).toBe(1);
    expect(r.preserved.tracks).toBe(1);
  });
});
