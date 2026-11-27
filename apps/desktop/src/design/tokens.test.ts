// Design-system guard (docs/design-system.md): colours live in design/tokens.css only, and every
// custom property a stylesheet reads is defined there.
import { describe, expect, it } from "vitest";

// @ts-expect-error node types are not installed for the frontend project
const { readFileSync } = await import("node:fs");
const read = (p: string): string => readFileSync(p, "utf8");
const tokens = read("src/design/tokens.css");
const sheets: Record<string, string> = {
  "src/styles.css": read("src/styles.css"),
  "src/grid.css": read("src/grid.css"),
  "src/tools/schematic/schematic.css": read("src/tools/schematic/schematic.css"),
  "src/agents/agents.css": read("src/agents/agents.css"),
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

  // Vision CMF (docs/product/README.md §1): TUSK orange is the one accent; physical materials and
  // status colours never borrow it, in either theme.
  it("keeps the brand accent apart from materials and status colours", () => {
    const block = (sel: string) => {
      const at = tokens.indexOf(sel);
      return tokens.slice(at, tokens.indexOf("\n}", at));
    };
    const values = (css: string) => new Map([...css.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)].map((m) => [m[1], m[2].trim().toLowerCase()]));
    const dark = values(block(":root {"));
    const light = values(block(':root[data-theme="light"]'));
    expect(dark.get("--accent")).toBe("#ee5f1b");
    for (const theme of [dark, light]) {
      const accent = theme.get("--accent")!;
      const physical = ["--mat-copper", "--mat-enig", "--mat-mask", "--mat-silk", "--c-edge", "--c-copper-top", "--c-copper-bottom", "--c-pad", "--c-pad-bottom", "--c-board"];
      for (const n of [...physical, "--err", "--warn", "--ok", "--info", "--unknown", "--ai"]) {
        const v = theme.get(n) ?? dark.get(n)!;
        expect(v, n).toBeDefined();
        expect(v, n).not.toContain("--accent");
        expect(v, n).not.toBe(accent);
      }
    }
  });
});
