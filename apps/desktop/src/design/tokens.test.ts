// Design-system guard (docs/design-system.md): colours live in design/tokens.css only, and every
// custom property a stylesheet reads is defined there. Written without running (testing freeze).
import { describe, expect, it } from "vitest";

// @ts-expect-error node types are not installed for the frontend project
const { readFileSync } = await import("node:fs");
const read = (p: string): string => readFileSync(p, "utf8");
const tokens = read("src/design/tokens.css");
const sheets: Record<string, string> = {
  "src/styles.css": read("src/styles.css"),
  "src/grid.css": read("src/grid.css"),
  "src/tools/schematic/schematic.css": read("src/tools/schematic/schematic.css"),
};
const defined = new Set([...tokens.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]));

describe("design tokens", () => {
  it("stylesheets use no raw colour literals", () => {
    for (const [file, css] of Object.entries(sheets)) {
      const raw = css.replace(/\/\*[\s\S]*?\*\//g, "").match(/#[0-9a-fA-F]{3,8}\b|rgba?\(/g) ?? [];
      expect(raw, file).toEqual([]);
    }
  });

  it("every var() a stylesheet reads is a defined token, without a fallback", () => {
    for (const [file, css] of Object.entries(sheets)) {
      const used = [...css.matchAll(/var\((--[a-z0-9-]+)(,?)/g)];
      expect(used.filter((m) => m[2] === ",").map((m) => m[1]), `${file} fallbacks`).toEqual([]);
      expect(used.map((m) => m[1]).filter((v) => !defined.has(v)), `${file} undefined`).toEqual([]);
    }
  });

  it("light theme overrides only names the dark theme defines", () => {
    const light = tokens.slice(tokens.indexOf('[data-theme="light"]'));
    const names = [...light.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]);
    expect(names.length).toBeGreaterThan(20);
    for (const n of names) expect(defined.has(n), n).toBe(true);
  });
});
