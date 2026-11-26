// Project Home facts (slice S1): attention items, release wording and the brief, all derived
// from folder facts only. Regression tests for the honesty rules: not run is never a pass.
import { describe, expect, it } from "vitest";
import { attentionItems, briefFromPrompt, continueTarget, currentErrors, goesTo, releaseLine, unknownRules, type HomeFacts } from "./homeFacts";

function facts(over: Partial<HomeFacts> = {}, design: Partial<HomeFacts["design"]> = {}): HomeFacts {
  return {
    path: "/p/board",
    name: "Board",
    revision: 12,
    phase: "Routing",
    design: { sheets: 1, parts: 10, nets: 6, footprints: 10, missing_footprints: 0, tracks: 20, vias: 2, zones: 1, outline: true, bom_lines: 7, without_supplier: 0, without_mpn: 0, requirements: 2, ...design },
    checks: [],
    unrouted_nets: 0,
    unrouted_names: [],
    unrouted_connections: 0,
    shorts: 0,
    release: { status: "DRAFT", required: 6, passing: 2, blocking: [] },
    ...over,
  };
}

const ids = (f: HomeFacts) => attentionItems(f).map((a) => a.id);

describe("Needs attention", () => {
  it("says a check is not run on this revision when it has no saved report, and never that it passed", () => {
    const items = attentionItems(facts());
    const erc = items.find((a) => a.id === "not-run-ERC")!;
    expect(erc.title).toBe("ERC not run on r12");
    expect(erc.target).toEqual({ kind: "check", check: "erc" });
    expect(items.map((a) => `${a.title} ${a.detail}`).join(" ")).not.toMatch(/\bpass(ed|es)?\b|manufacturing-ready/i);
  });

  it("puts shorts and check errors first and sends each to the place that fixes it", () => {
    const f = facts({ shorts: 1, checks: [{ check: "DRC", errors: 3, warnings: 1, unknown_rules: 0 }, { check: "ERC", errors: 0, warnings: 0, unknown_rules: 0 }] });
    const items = attentionItems(f);
    expect(items[0]).toMatchObject({ id: "shorts", tone: "bad", target: { kind: "view", view: "pcb" } });
    expect(items[1]).toMatchObject({ id: "errors-DRC", title: "DRC: 3 errors", detail: "Saved DRC report for r12, 1 warning.", target: { kind: "check", check: "drc" } });
    // ERC and DRC both have current evidence: neither is listed as not run.
    expect(ids(f)).not.toContain("not-run-ERC");
    expect(ids(f)).not.toContain("not-run-DRC");
  });

  it("names unrouted nets, missing footprints, outline, UNKNOWN rules and BOM risk", () => {
    const f = facts(
      { unrouted_nets: 10, unrouted_names: ["GND", "+3V3"], unrouted_connections: 14, checks: [{ check: "DRC", errors: 0, warnings: 0, unknown_rules: 2 }] },
      { missing_footprints: 2, outline: false, without_supplier: 4, without_mpn: 1 },
    );
    const by = Object.fromEntries(attentionItems(f).map((a) => [a.id, a]));
    expect(by.unrouted.title).toBe("10 unrouted nets");
    expect(by.unrouted.detail).toBe("GND, +3V3 and 8 more (14 connections left).");
    expect(by["off-board"]).toMatchObject({ title: "2 parts not on the board", target: { kind: "view", view: "schematic" } });
    expect(by.outline.target).toEqual({ kind: "view", view: "pcb" });
    expect(by["unknown-DRC"]).toMatchObject({ tone: "unknown", title: "DRC could not evaluate 2 rules" });
    expect(by["unknown-DRC"].detail).toContain("never a pass");
    expect(by.bom).toMatchObject({ title: "BOM risk", target: { kind: "library" } });
    expect(by.bom.detail).toBe("4 of 10 parts with no supplier part number; 1 with no MPN: sourcing cannot be checked for them.");
  });

  it("asks for a brief only when there is none, and nothing about checks on an empty project", () => {
    const empty = facts({ phase: "Empty project" }, { parts: 0, footprints: 0, requirements: 0, bom_lines: 0 });
    expect(ids(empty)).toEqual(["brief"]);
    expect(attentionItems(empty)[0].target).toEqual({ kind: "overview" });
    // A schematic with no board yet: ERC not run is listed, DRC (nothing on the board) is not.
    expect(ids(facts({}, { footprints: 0, missing_footprints: 10 }))).toEqual(["off-board", "not-run-ERC"]);
  });

  it("sends MECH errors to the release check, which is where MECH is shown", () => {
    const items = attentionItems(facts({ checks: [{ check: "MECH", errors: 1, warnings: 0, unknown_rules: 0 }] }));
    expect(items.find((a) => a.id === "errors-MECH")!.target).toEqual({ kind: "release" });
  });

  it("describes where every target goes", () => {
    expect(goesTo({ kind: "check", check: "erc" })).toBe("opens the project and runs ERC into Findings");
    expect(goesTo({ kind: "release" })).toBe("opens the Release check");
  });
});

describe("Continue card numbers", () => {
  it("has no error count without current evidence (a dash, not zero)", () => {
    expect(currentErrors(facts())).toBeNull();
    expect(currentErrors(facts({ checks: [{ check: "ERC", errors: 2, warnings: 0, unknown_rules: 1 }, { check: "DRC", errors: 1, warnings: 0, unknown_rules: 3 }] }))).toBe(3);
    expect(unknownRules(facts({ checks: [{ check: "DRC", errors: 0, warnings: 0, unknown_rules: 3 }] }))).toBe(3);
  });
  it("words release readiness as gates passing and a draft status", () => {
    expect(releaseLine(facts())).toBe("2 of 6 release gates pass · Draft");
    expect(releaseLine(facts({ release: { status: "INTERNALLY_VERIFIED_RELEASE_CANDIDATE", required: 6, passing: 6, blocking: [] } }))).toContain("still needs independent review");
  });
  it("continues in the editor of the project's furthest content", () => {
    expect(continueTarget("Routing")).toEqual({ kind: "view", view: "pcb" });
    expect(continueTarget("PCB placement")).toEqual({ kind: "view", view: "pcb" });
    expect(continueTarget("Schematic capture")).toEqual({ kind: "view", view: "schematic" });
    expect(continueTarget("Empty project")).toEqual({ kind: "view", view: "schematic" });
  });
});

describe("brief from a typed description (no AI)", () => {
  it("keeps the engineer's sentences, one per line, without splitting decimals", () => {
    const b = briefFromPrompt("5 V input from a header. 3.3 V rail from an LDO, at most 300 mA; a green LED.\nFits 40 x 25 mm");
    expect(b.lines).toEqual(["5 V input from a header.", "3.3 V rail from an LDO, at most 300 mA;", "a green LED.", "Fits 40 x 25 mm"]);
    expect(b.name).toBe("5 V input from a header");
  });
  it("names the project from the first words only and drops blank input", () => {
    expect(briefFromPrompt("battery powered temperature and humidity sensor on I2C with a long tail of words").name).toBe("Battery powered temperature and humidity sensor");
    expect(briefFromPrompt("   \n  ")).toEqual({ name: "", lines: [] });
    expect(briefFromPrompt("x".repeat(80)).name).toHaveLength(48);
  });
});
