// Slice C6 / E6: the 3D document's pure rules. The stack-up table is the IR stack-up, row for
// row; presets frame the board from the named side; a measurement is reported in PCB axes; the
// board takes its colours from the IR's manufacturing settings; a part without a loaded model
// always has a placeholder reason.
import { describe, expect, it } from "vitest";
import { fixture } from "../fixture.test-data";
import type { Snapshot } from "../types";
import { appearance, FINISHES, MASK_COLOURS, measure, measureText, orthoHalfHeight, placeholderReason, presetPose, PRESETS, stackTable } from "./view";

const withStackup = (): Snapshot => {
  const s = fixture(7);
  s.project.pcb.board.stackup = {
    board_thickness: 1_600_000,
    dielectric: "FR-4",
    layers: [
      { layer: "top", kind: "signal", copper_oz: 1, dielectric_after_um: 200, er: 4.2 },
      { layer: { inner: 1 }, kind: "plane", copper_oz: 0.5, dielectric_after_um: 1000, er: 4.5 },
      { layer: { inner: 2 }, kind: "plane", copper_oz: 0.5, dielectric_after_um: 200, er: 4.5 },
      { layer: "bottom", kind: "signal", copper_oz: 1, dielectric_after_um: 0, er: 4.2 },
    ],
  };
  return s;
};

describe("stack-up table", () => {
  it("lists the IR stack-up exactly: copper layers with the dielectric between them", () => {
    const s = withStackup();
    const t = stackTable(s);
    expect(t.fromIr).toBe(true);
    expect(t.boardThicknessMm).toBeCloseTo(1.6);
    expect(t.rows.map((r) => r.name)).toEqual(["F.Cu", "Dielectric 1", "In1.Cu", "Dielectric 2", "In2.Cu", "Dielectric 3", "B.Cu"]);
    const ir = s.project.pcb.board.stackup!.layers;
    const copper = t.rows.filter((r) => r.kind === "copper");
    expect(copper.map((r) => r.detail)).toEqual(ir.map((l) => l.kind));
    expect(copper.map((r) => r.thicknessUm)).toEqual(ir.map((l) => l.copper_oz * 35));
    const diel = t.rows.filter((r) => r.kind === "dielectric");
    expect(diel.map((r) => r.thicknessUm)).toEqual(ir.slice(0, -1).map((l) => l.dielectric_after_um));
    expect(diel.map((r) => r.value)).toEqual(ir.slice(0, -1).map((l) => `εr ${l.er}`));
    expect(diel.every((r) => r.detail === "FR-4")).toBe(true);
  });

  it("says when the project has no stack-up instead of presenting assumptions as IR data", () => {
    const s = fixture(7);
    delete s.project.pcb.board.stackup;
    const t = stackTable(s);
    expect(t.fromIr).toBe(false);
    expect(t.rows.map((r) => r.name)).toEqual(["F.Cu", "B.Cu"]);
    expect(t.rows.every((r) => r.value.includes("assumed"))).toBe(true);
  });
});

describe("camera presets", () => {
  const bounds = [0, 0, 40, 20] as const;
  it("offers top, bottom, four sides and iso", () => {
    expect(PRESETS.map((p) => p.id)).toEqual(["top", "bottom", "front", "back", "left", "right", "iso"]);
  });
  it("looks at the board centre from the named side, at the same distance", () => {
    const at = (id: (typeof PRESETS)[number]["id"]) => presetPose(id, bounds, 40);
    for (const p of PRESETS) expect(at(p.id).target).toEqual([20, 0, 10]);
    expect(at("top").position[1]).toBeGreaterThan(0);
    expect(at("bottom").position[1]).toBeLessThan(0);
    expect(at("front").position[2]).toBeGreaterThan(10);
    expect(at("back").position[2]).toBeLessThan(10);
    expect(at("left").position[0]).toBeLessThan(20);
    expect(at("right").position[0]).toBeGreaterThan(20);
    const d = at("top").distance;
    for (const p of PRESETS) expect(at(p.id).distance).toBeCloseTo(d, 1);
  });
  it("sizes the orthographic frustum to what the perspective camera shows", () => {
    expect(orthoHalfHeight(100, 90)).toBeCloseTo(100);
  });
});

describe("measure", () => {
  it("reports the distance and the deltas in PCB axes (world Z is PCB y, world Y is height)", () => {
    const m = measure([0, 0, 0], [3, 1.6, 4]);
    expect(m.dx).toBe(3);
    expect(m.dy).toBe(4);
    expect(m.dz).toBeCloseTo(1.6);
    expect(m.distance).toBeCloseTo(Math.hypot(3, 4, 1.6));
    expect(measureText(measure([0, 0, 0], [3, 0, 4]))).toBe("5.000 mm (Δx 3.000, Δy 4.000, Δz 0.000)");
  });
});

describe("board appearance", () => {
  it("takes mask colour and finish from the IR manufacturing settings", () => {
    const s = fixture(7);
    s.project.manufacturing = { fabricator_profile: "generic-2layer", surface_finish: "ENIG", solder_mask_color: "Black", mask_expansion: 50_000 };
    expect(appearance(s)).toMatchObject({ maskName: "Black", finishName: "ENIG", mask: MASK_COLOURS.Black, pad: FINISHES.ENIG, unknown: [] });
  });
  it("names an IR value it has no colour for instead of hiding it", () => {
    const s = fixture(7);
    s.project.manufacturing = { fabricator_profile: "x", surface_finish: "Hard gold", solder_mask_color: "Matte black", mask_expansion: 0 };
    const a = appearance(s);
    expect(a.maskName).toBe("Matte black");
    expect(a.unknown).toEqual(['mask colour "Matte black"', 'finish "Hard gold"']);
  });
  it("defaults to the engine defaults (Green, HASL) when the project has no settings", () => {
    const s = fixture(7);
    delete s.project.manufacturing;
    expect(appearance(s)).toMatchObject({ maskName: "Green", finishName: "HASL" });
  });
});

describe("placeholders", () => {
  it("labels every part without a loaded model, and only those", () => {
    expect(placeholderReason(true, { ok: true }, true)).toBeNull();
    expect(placeholderReason(false, undefined, true)).toBe("no 3D model attached · box is approximate");
    expect(placeholderReason(false, undefined, false)).toBe("no 3D model attached · no body height, outline only");
    expect(placeholderReason(true, undefined, true)).toMatch(/^model loading…/);
    expect(placeholderReason(true, { ok: false, error: "STEP file has no readable geometry" }, true)).toBe("model failed: STEP file has no readable geometry · box is approximate");
  });
});
