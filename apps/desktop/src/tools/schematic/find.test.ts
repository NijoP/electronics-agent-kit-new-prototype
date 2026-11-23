// Find and replace (slice B3): designators, values and net labels; a replace is one command list.
import { describe, expect, it } from "vitest";
import { fixture } from "../../fixture.test-data";
import { DEFAULT_FIND, findHits, replaceCommands, replaced, type FindOptions } from "./find";

const only = (f: Partial<FindOptions["fields"]>): FindOptions => ({ ...DEFAULT_FIND, fields: { refdes: false, value: false, label: false, ...f } });

describe("find and replace", () => {
  it("finds designators, values and net labels, case-insensitively by default", () => {
    const s = fixture();
    expect(findHits(s, "r1", DEFAULT_FIND).map((h) => [h.field, h.text])).toEqual([["refdes", "R1"]]);
    expect(findHits(s, "gnd", DEFAULT_FIND).map((h) => [h.kind, h.id])).toEqual([["label", "lab_1"]]);
    expect(findHits(s, "gr", only({ value: true })).map((h) => h.text)).toEqual(["Green"]);
    expect(findHits(s, "r1", { ...DEFAULT_FIND, matchCase: true })).toEqual([]);
    expect(findHits(s, "G", { ...DEFAULT_FIND, whole: true })).toEqual([]);
    expect(findHits(s, "", DEFAULT_FIND)).toEqual([]);
    // regex characters are literal
    expect(findHits(s, ".", DEFAULT_FIND)).toEqual([]);
  });

  it("replaces every match: SetRefdes, SetSymbolValue, and a label as delete + add at the same point", () => {
    const s = fixture();
    s.project.schematic.labels.lab_1.scope = "local";
    expect(replaced("R1", "r", "U", DEFAULT_FIND)).toBe("U1");
    const hits = findHits(s, "1", only({ refdes: true }));
    expect(replaceCommands(s, hits, "1", "7", only({ refdes: true }))).toEqual([
      { type: "SetRefdes", symbol_id: "sym_d1", refdes: "D7" },
      { type: "SetRefdes", symbol_id: "sym_r1", refdes: "R7" },
    ]);
    expect(replaceCommands(s, findHits(s, "330", DEFAULT_FIND), "330", "1k", DEFAULT_FIND)).toEqual([{ type: "SetSymbolValue", symbol_id: "sym_r1", value: "1k" }]);
    expect(replaceCommands(s, findHits(s, "GND", DEFAULT_FIND), "GND", "AGND", DEFAULT_FIND)).toEqual([
      { type: "DeleteNetLabel", label_id: "lab_1" },
      { type: "AddNetLabel", sheet_id: "sht_1", name: "AGND", position: s.project.schematic.labels.lab_1.position, kind: "power", scope: "local" },
    ]);
    // a designator never becomes empty; a replacement that changes nothing sends nothing
    expect(replaceCommands(s, findHits(s, "R1", only({ refdes: true })), "R1", "", only({ refdes: true }))).toEqual([]);
    expect(replaceCommands(s, findHits(s, "R1", DEFAULT_FIND), "R1", "R1", DEFAULT_FIND)).toEqual([]);
  });
});
