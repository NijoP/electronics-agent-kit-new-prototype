// Unrun (no builds or tests during the testing freeze): model cache is keyed by file, not transform.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { cachedModelCount, clearModelCache, loadModel, modelKey } from "./modelLoader";
import type { ModelRef } from "../types";

const ref = (path: string, scale = 1): ModelRef => ({ source: { kind: "file", path }, format: "glb", transform: { offset_mm: [0, 0, 0], rotation_deg: [0, 0, 0], scale: [scale, scale, scale] }, licence: null, provenance: "t" });

describe("loadModel cache", () => {
  beforeEach(() => clearModelCache());
  it("keys by format and path only", () => {
    expect(modelKey(ref("/a.glb", 1))).toBe(modelKey(ref("/a.glb", 2)));
    expect(modelKey(ref("/a.glb"))).not.toBe(modelKey(ref("/b.glb")));
  });
  it("reads a file once however often the project revision changes", async () => {
    const read = vi.fn().mockRejectedValue(new Error("no such file"));
    await expect(loadModel(ref("/a.glb"), read)).rejects.toThrow("no such file");
    await expect(loadModel(ref("/a.glb", 3), read)).rejects.toThrow("no such file");
    expect(read).toHaveBeenCalledTimes(1);
    expect(cachedModelCount()).toBe(1);
  });
});
