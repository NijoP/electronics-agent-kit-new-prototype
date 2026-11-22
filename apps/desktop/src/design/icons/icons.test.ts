// Icon audit (slice A2): every icon of the EAK set is listed in design/icons/README.md as
// EAK-authored, the README lists nothing that does not exist, and no icon carries its own colour.
import { describe, expect, it } from "vitest";
import { ICON_NAMES } from "./icons";

// @ts-expect-error node types are not installed for the frontend project
const { readFileSync } = await import("node:fs");
const readme: string = readFileSync("src/design/icons/README.md", "utf8");
const source: string = readFileSync("src/design/icons/icons.tsx", "utf8");
const rows = [...readme.matchAll(/^\| `([a-z0-9-]+)` \| ([^|]+) \| ([^|]+) \|$/gm)].map((m) => ({ name: m[1], drawing: m[2].trim(), source: m[3].trim() }));

describe("EAK icon set audit", () => {
  it("starts with a usable set (about 60 or more)", () => {
    expect(ICON_NAMES.length).toBeGreaterThanOrEqual(60);
  });

  it("every icon is listed once in the README as EAK-authored, and the README lists only real icons", () => {
    expect(rows.map((r) => r.name).sort()).toEqual([...ICON_NAMES].sort());
    expect(new Set(rows.map((r) => r.name)).size).toBe(rows.length);
    for (const r of rows) {
      expect(r.source, r.name).toMatch(/^EAK-authored/);
      expect(r.drawing.length, r.name).toBeGreaterThan(3);
    }
  });

  it("icons draw in currentColor only: no colour literals in the icon source", () => {
    expect(source.match(/#[0-9a-fA-F]{3,8}\b|rgba?\(|hsla?\(/g) ?? []).toEqual([]);
    expect([...source.matchAll(/(?:fill|stroke)="([^"]+)"/g)].map((m) => m[1]).filter((v) => v !== "none" && v !== "currentColor")).toEqual([]);
  });
});
