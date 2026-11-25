// UI inventory as CI (slice E9): every visible control has a handler or an explicit
// unavailable-with-reason state; known exceptions are allowlisted with their owning slice.
import { describe, expect, it, vi } from "vitest";
import { fixture } from "../fixture.test-data";
import { GRID_KINDS } from "../grid";
import { SIDE_VIEWS } from "../shell/ActivityBar";
import { workspaceActions, type ActionDeps } from "../actions/sets";
import { quickOpenItems } from "../actions/quickOpen";
import { auditActions, noOps, recorder, renderInventory, scanSource, staleAllow, tagEnd, unallowed, type Audit } from "./uiInventory";
import { UI_INVENTORY_ALLOWLIST } from "./uiInventoryAllowlist";

// @ts-expect-error node types are not installed for the frontend project
const { readFileSync, writeFileSync, existsSync } = await import("node:fs");
const env = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env ?? {};
const DOC = "../../docs/qa/UI_INVENTORY.md"; // vitest runs in apps/desktop

const sources = import.meta.glob(["../**/*.tsx", "!../**/*.test.tsx"], { query: "?raw", import: "default", eager: true }) as Record<string, string>;

function deps(over: Partial<ActionDeps> = {}): ActionDeps {
  const fn = () => vi.fn();
  return {
    snapshot: fixture(7),
    busy: false,
    selected: null,
    selectedSymbol: undefined,
    fpOfSelected: undefined,
    theme: "dark",
    sideOpen: true,
    bottomOpen: true,
    rightOpen: true,
    split: false,
    recent: [{ path: "/o.eak", project_id: "prj_other", name: "Other", revision: 1, last_opened_ms: 1 }],
    library: [{ id: "res", name: "Resistor", category: "passive", refdes_prefix: "R", default_value: "10k", pins: [] }],
    docs: [{ id: "sch:sht_1", kind: "schematic", title: "Sheet 1" }, { id: "pcb:prj_1", kind: "pcb", title: "PCB" }],
    openIds: ["sch:sht_1"],
    projects: [{ id: "prj_2", name: "Second", dir: null, active: false }],
    sideViews: SIDE_VIEWS,
    edaViews: [{ id: "viewer3d", label: "3D view", shortcut: "Mod+3" }],
    dialog: fn(), save: fn(), undo: fn(), redo: fn(), close: fn(), openRecent: fn(), execute: fn(), check: fn(), syncPcb: fn(),
    refillZones: fn(), rotateSel: fn(), deleteSel: fn(), tool: fn(), zoomBy: fn(), fit: fn(), select: fn(), toggleSplit: fn(),
    showSide: fn(), toggleSide: fn(), toggleBottom: fn(), showBottom: fn(), toggleRight: fn(), showRight: fn(), onTheme: fn(), resetLayout: fn(), onSettings: fn(), preferences: fn(),
    palette: fn(), quickOpen: fn(), help: fn(), about: fn(), sideView: "project", notify: fn(), open: fn(), closeAll: fn(), showWorkspace: fn(), switchTo: fn(), placePart: fn(),
    grid: { kind: "schematic", cfg: GRID_KINDS.schematic.defaults, set: fn() },
    toolbarLabels: false,
    toggleToolbarLabels: fn(),
    ...over,
  };
}

/** Workspace states the registry is probed in: idle, everything selectable selected, busy, no grid. */
function probes(): ActionDeps[] {
  const snap = fixture(7);
  // E5: a GND pour on both copper layers, so StitchZones has something to stitch.
  const zones = { ...snap.project.pcb.zones, zon_top: { ...snap.project.pcb.zones.zon_1, id: "zon_top", layer: "top" as const } };
  const rich = deps({
    snapshot: { ...snap, can_undo: true, can_redo: true, project: { ...snap.project, pcb: { ...snap.project.pcb, zones } } },
    selected: "sym_r1",
    selectedSymbol: snap.project.schematic.symbols.sym_r1,
    fpOfSelected: snap.project.pcb.footprints.fp_r1,
    // B3: a schematic editor offering every editing operation (clipboard, arrange, find, net highlight)
    editorOp: () => ({ enabled: () => true, run: vi.fn() }),
  });
  // The 3D document with a drawing viewer (C6): a measurement on screen and a part with a model.
  const fn = () => vi.fn();
  const view3d = { projection: "perspective" as const, measuring: true, measured: true, labels: true, modelCount: 1, preset: fn(), reset: fn(), zoomBy: fn(), setProjection: fn(), toggleMeasure: fn(), clearMeasure: fn(), toggleLabels: fn(), reloadModels: fn() };
  const in3d = deps({ selected: "sym_r1", selectedSymbol: snap.project.schematic.symbols.sym_r1, fpOfSelected: snap.project.pcb.footprints.fp_r1, view3d, probeTo: fn() });
  return [deps(), rich, deps({ busy: true }), deps({ grid: null }), in3d];
}

/** Quick open in the rich state (a library part, the fixture's symbols, nets and sheets), each item run against recorded dependencies. */
function quickOpenAudit(): Audit["quickOpen"] {
  const fn = () => vi.fn();
  const rec = recorder({ snapshot: fixture(7), library: deps().library, placePart: fn(), select: fn(), showNet: fn(), open: fn() });
  const items = quickOpenItems(rec.deps);
  return { ids: items.map((i) => i.id), noOps: noOps(items, rec) };
}

function audit(): Audit {
  const controls = Object.entries(sources)
    .sort(([a], [b]) => a.localeCompare(b))
    .flatMap(([path, src]) => scanSource(`src/${path.replace(/^\.\.\//, "")}`, src));
  return { actions: auditActions(probes()), controls, allow: UI_INVENTORY_ALLOWLIST, quickOpen: quickOpenAudit() };
}

describe("UI inventory scanner", () => {
  it("finds the end of an opening tag past braces, strings and comments", () => {
    const src = `<button onClick={() => { const s = "a>b"; /* > */ return s; }} title='x>y'>Go</button>`;
    expect(src[tagEnd(src, 7)]).toBe(">");
    expect(src.slice(tagEnd(src, 7) + 1)).toBe("Go</button>");
  });

  it("classifies handlers, explicit unavailable controls and dead controls", () => {
    const rows = scanSource(
      "f.tsx",
      [
        `<button onClick={go}>Run</button>`,
        `<button disabled title="Not implemented: no engine">Save as</button>`,
        `<button>Dead</button>`,
        `<button disabled>Mute</button>`,
        `<input type="checkbox" checked={x} onChange={f} aria-label="Snap" />`,
        `<div role="button" aria-label="Fake" />`,
        `<a href="https://x">Docs</a>`,
        `<Comp {...p} />`,
        `const v: Array<string> = [];`,
      ].join("\n"),
    );
    expect(rows.map((r) => [r.element, r.label, r.state])).toEqual([
      ["button", "Run", "handler"],
      ["button", "Not implemented: no engine", "unavailable"],
      ["button", "Dead", "violation"],
      ["button", "Mute", "violation"],
      ["input[type=checkbox]", "Snap", "handler"],
      ["div[role=button]", "Fake", "violation"],
      ["a", "Docs", "handler"],
    ]);
    expect(rows[2].line).toBe(3);
  });

  it("flags a registry action without a handler unless it is explicitly unavailable", () => {
    const rows = auditActions(probes());
    const byId = (id: string) => rows.find((r) => r.id === id)!;
    // slice E1 removed the three handler-less placeholders (Save as, Release check, Junction)
    for (const id of ["saveas", "release", "t-junction"]) expect(rows.find((r) => r.id === id), id).toBeUndefined();
    expect(byId("undo").state).toBe("state-dependent");
  });

  // G2: the gate used to check only `typeof run === "function"`, so an enabled no-op passed.
  it("flags an enabled action whose handler calls nothing, and fails the gate on it", () => {
    const dead = { id: "dead", label: "Dead", menu: "Tools" as const, enabled: () => true as const, run: () => {} };
    const rows = auditActions(probes(), (d) => [...workspaceActions(d), dead]);
    expect(rows.find((r) => r.id === "dead")).toMatchObject({ state: "violation", violation: expect.stringMatching(/no-op/) });
    const a: Audit = { actions: rows, controls: [], allow: [], quickOpen: { ids: [], noOps: [] } };
    expect(unallowed(a)).toEqual(["action:dead: enabled, but running it calls nothing (a no-op handler)"]);
    // a real handler is not flagged
    expect(rows.find((r) => r.id === "undo")?.violation).toBeUndefined();
  });

  it("probes quick-open items too: a no-op item fails the gate", () => {
    const q = quickOpenAudit();
    expect(q.ids.length).toBeGreaterThan(3);
    expect(q.ids).toEqual(expect.arrayContaining(["part-res", "sym-sym_r1", "sheet-sht_1"]));
    expect(q.noOps).toEqual([]);
    const rec = recorder({ go: vi.fn() });
    expect(noOps([{ id: "ok", run: () => rec.deps.go() }, { id: "dead", run: () => {} }, { id: "off", disabled: true, run: () => {} }], rec)).toEqual(["dead"]);
    expect(unallowed({ actions: [], controls: [], allow: [], quickOpen: { ids: ["dead"], noOps: ["dead"] } })).toEqual(["quick:dead: enabled, but running it calls nothing (a no-op handler)"]);
  });
});

describe("UI inventory (CI gate)", () => {
  const a = audit();

  it("scans a meaningful part of the app", () => {
    expect(a.actions.length).toBeGreaterThan(50);
    expect(a.controls.length).toBeGreaterThan(200);
  });

  it("every visible control has a handler or an explicit unavailable-with-reason state (or is allowlisted)", () => {
    expect(unallowed(a)).toEqual([]);
  });

  it("every allowlist entry names its owning slice and why", () => {
    for (const e of UI_INVENTORY_ALLOWLIST) {
      expect(e.owner, e.key).toMatch(/^[A-Z]\d+$/);
      expect(e.why.trim().length, e.key).toBeGreaterThan(10);
    }
    // stale entries are reported in the inventory, not failed, so a fix and its removal can land apart
    if (staleAllow(a).length) console.warn("stale UI inventory allowlist entries:", staleAllow(a).map((e) => e.key));
  });

  it("writes docs/qa/UI_INVENTORY.md when UPDATE_UI_INVENTORY=1", () => {
    const doc = renderInventory(a);
    if (env.UPDATE_UI_INVENTORY === "1") writeFileSync(DOC, doc);
    else if (!existsSync(DOC) || readFileSync(DOC, "utf8") !== doc) console.warn("docs/qa/UI_INVENTORY.md is out of date; CI uploads the regenerated file");
    expect(doc).toContain("## Action registry");
  });
});
