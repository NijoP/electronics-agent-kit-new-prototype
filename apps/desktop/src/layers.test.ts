import { describe, expect, it } from "vitest";
import { layerKey, layerName, stackLayers, viaKind, viaLayers } from "./layers";
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
