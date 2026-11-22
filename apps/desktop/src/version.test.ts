import { describe, expect, it } from "vitest";
import tauri from "../src-tauri/tauri.conf.json";
import { APP_VERSION } from "./version";

describe("app version", () => {
  it("is the version the Tauri bundle is built with", () => {
    expect(APP_VERSION).toMatch(/^\d+\.\d+\.\d+/);
    expect(APP_VERSION).toBe(tauri.version);
  });
});
