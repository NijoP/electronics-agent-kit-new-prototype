import { describe, expect, it } from "vitest";
import { layerCountOptions, layerKey, layerName, planeFill, stackLayers, viaKind, viaLayers } from "./layers";
import type { CopperLayer, Stackup } from "./types";

const four: Stackup = {
  board_thickness: 1_600_000,
  dielectric: "FR-4",
  layers: [
    { layer: "top", kind: "signal", copper_oz: 1, dielectric_after_um: 200, er: 4.5 },
    { layer: { inner: 1 }, kind: "plane", copper_oz: 0.5, dielectric_after_um: 1000, er: 4.5 },
    { layer: { inner: 2 }, kind: "plane", copper_oz: 0.5, dielectric_after_um: 200, er: 4.5 },
    { layer: "bottom", kind: "signal", copper_oz: 1, dielectric_after_um: 0, er: 4.5 },
  ],
};

describe("layers", () => {
  it("names and keys every layer", () => {
    expect(["top", { inner: 2 }, "bottom"].map((l) => layerName(l as CopperLayer))).toEqual(["F.Cu", "In2.Cu", "B.Cu"]);
    expect(layerKey({ inner: 1 })).toBe("in1");
  });
  it("treats a project without a stackup as two layers", () => {
    expect(stackLayers(undefined).map((s) => layerKey(s.layer))).toEqual(["top", "bottom"]);
  });
  it("lists a four-layer stack with its plane layers", () => {
    expect(stackLayers(four).map((s) => `${layerKey(s.layer)}:${s.kind}`)).toEqual([
      "top:signal",
      "in1:plane",
      "in2:plane",
      "bottom:signal",
    ]);
  });
  it("classifies vias and finds the layers they touch", () => {
    const order = stackLayers(four).map((s) => s.layer);
    expect(viaKind("top", "bottom")).toBe("through");
    expect(viaKind("top", { inner: 1 })).toBe("blind");
    expect(viaKind({ inner: 1 }, { inner: 2 })).toBe("buried");
    expect(viaLayers(order, "bottom", { inner: 1 }).map(layerKey)).toEqual(["in1", "in2", "bottom"]);
  });
});

describe("layer count and plane fill (E2)", () => {
  it("offers every even count 2..16 and keeps an unusual current count selectable", () => {
    expect(layerCountOptions(2)).toEqual([2, 4, 6, 8, 10, 12, 14, 16]);
    expect(layerCountOptions(3)).toEqual([2, 3, 4, 6, 8, 10, 12, 14, 16]);
  });

  // Regression (inventory F3): "Fill planes with net" sent a Command[] that ran one by one
  // (several revisions and undo steps, partial result on a rejection).
  it("fills every plane layer without a zone as ONE Batch", () => {
    const outline = [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 10 }];
    const stack = [
      { layer: "top" as const, kind: "signal" as const },
      { layer: { inner: 1 }, kind: "plane" as const },
      { layer: { inner: 2 }, kind: "plane" as const },
      { layer: { inner: 3 }, kind: "plane" as const },
      { layer: "bottom" as const, kind: "signal" as const },
    ];
    const hasZone = (l: CopperLayer) => typeof l !== "string" && l.inner === 3;
    expect(planeFill(stack, hasZone, "GND", outline)).toEqual({
      type: "Batch",
      label: "Fill planes with GND",
      commands: [
        { type: "AddZone", layer: { inner: 1 }, net: "GND", outline, clearance: null },
        { type: "AddZone", layer: { inner: 2 }, net: "GND", outline, clearance: null },
      ],
    });
    expect(planeFill(stack.slice(0, 2), () => false, "GND", outline)).toEqual({ type: "AddZone", layer: { inner: 1 }, net: "GND", outline, clearance: null });
    expect(planeFill(stack, () => true, "GND", outline)).toBeNull();
  });
});
