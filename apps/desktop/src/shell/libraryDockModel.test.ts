// Phase 1 D1 library dock model: category tree, paging, footprint grouping, supplier columns
// (ADR-0028: offer data, never identity), offline scopes and the common-parts palette.
import { describe, expect, it } from "vitest";
import type { ComponentDef, LibraryEntry2 } from "../types";
import { COMMON_PARTS, OFFLINE_SCOPES, categoryTree, footprintRows, inCategory, pageOf, priceText, resolvePalette, scopeUnavailable, supplierCols } from "./libraryDockModel";

const def = (id: string, category: string, footprint = `FP_${id}`): ComponentDef => ({
  id,
  name: `Part ${id}`,
  category,
  refdes_prefix: "U",
  default_value: "",
  manufacturer: null,
  mpn: null,
  distributor: null,
  datasheet: null,
  symbol: { pins: [], body_half_w: 1_000_000, body_half_h: 1_000_000 },
  footprint: { name: footprint, pads: [], courtyard_half_w: 1_000_000, courtyard_half_h: 1_000_000, body_height: null },
  pin_map: {},
  ratings: [],
  provenance: "test",
});

const entry = (d: ComponentDef, kind: "builtin" | "project" = "builtin", over: Partial<LibraryEntry2> = {}): LibraryEntry2 => ({
  scope: { kind },
  scope_label: kind,
  id: d.id,
  def: d,
  favourite: false,
  tags: [],
  recent_rank: null,
  unverified_ratings: 0,
  has_datasheet: false,
  in_project: false,
  supplier: null,
  ...over,
});

describe("library dock model", () => {
  it("groups flat categories into families with counts, Other last", () => {
    const es = [entry(def("r1", "Resistor")), entry(def("r2", "Resistor")), entry(def("c1", "Capacitor")), entry(def("q1", "Transistor")), entry(def("x", "Gizmo")), entry(def("m", "Mech/Bracket"))];
    const t = categoryTree(es);
    expect(t.map((f) => [f.label, f.count])).toEqual([
      ["Mech", 1],
      ["Passives", 3],
      ["Semiconductors", 1],
      ["Other", 1],
    ]);
    expect(t[1].children.map((c) => [c.label, c.count])).toEqual([
      ["Capacitor", 1],
      ["Resistor", 2],
    ]);
    expect(t[0].children[0].label).toBe("Bracket");
    expect(es.filter((e) => inCategory(e, "f:Passives")).map((e) => e.id)).toEqual(["r1", "r2", "c1"]);
    expect(es.filter((e) => inCategory(e, "c:Resistor")).map((e) => e.id)).toEqual(["r1", "r2"]);
    expect(es.filter((e) => inCategory(e, null))).toHaveLength(6);
  });

  it("pages rows and clamps the page", () => {
    const rows = Array.from({ length: 45 }, (_, i) => i);
    expect(pageOf(rows, 0)).toMatchObject({ page: 0, pages: 3, from: 1, to: 20, total: 45 });
    expect(pageOf(rows, 2).rows).toEqual([40, 41, 42, 43, 44]);
    expect(pageOf(rows, 9)).toMatchObject({ page: 2, from: 41, to: 45 });
    expect(pageOf([], 3)).toMatchObject({ page: 0, pages: 1, from: 0, to: 0, total: 0 });
  });

  it("groups parts by footprint name", () => {
    const rows = footprintRows([entry(def("a", "Resistor", "R_0603")), entry(def("b", "Capacitor", "R_0603")), entry(def("c", "LED", "LED_0603"))]);
    expect(rows.map((r) => [r.name, r.parts.map((p) => p.id)])).toEqual([
      ["LED_0603", ["c"]],
      ["R_0603", ["a", "b"]],
    ]);
  });

  it("shows the supplier offer first, then the stored record, then a bare part number", () => {
    const d = def("r", "Resistor");
    const offer = { ...d, supplier_offers: [{ supplier: "distributor", supplier_part: "C25804", stock: 1200, price_breaks: [{ min_quantity: 100, unit_price: "0.0021", currency: "USD" }], fetched_at_unix_s: 1_700_000_000 }] };
    expect(supplierCols(entry(offer))).toEqual({ supplier: "distributor C25804", stock: "1,200", price: "0.0021 USD @ 100+", asOf: 1_700_000_000 });
    const rec = entry(d, "builtin", { supplier: { distributor: "C1", stock: null, as_of_unix_s: 5, price_breaks: [] } });
    expect(supplierCols(rec)).toEqual({ supplier: "distributor C1", stock: "not stated", price: "not stated", asOf: 5 });
    expect(supplierCols(entry({ ...d, distributor: "C9" }))).toMatchObject({ supplier: "distributor C9", stock: "no snapshot", price: "no snapshot", asOf: null });
    expect(supplierCols(entry(d))).toMatchObject({ supplier: "—", asOf: null });
    expect(priceText(undefined)).toBe("not stated");
  });

  it("offline mode keeps only the built-in and project scopes", () => {
    expect(OFFLINE_SCOPES).toEqual(["builtin", "project"]);
    expect(scopeUnavailable("builtin", true)).toBeNull();
    expect(scopeUnavailable("project", true)).toBeNull();
    for (const s of ["personal", "imported", "distributor"] as const) {
      expect(scopeUnavailable(s, true)).toMatch(/^Offline mode/);
      expect(scopeUnavailable(s, false)).toBeNull();
    }
  });

  it("offers only palette variants the built-in library lists, and only built-in ones", () => {
    const lib = [entry(def("R_0603", "Resistor")), entry(def("R_0805", "Resistor")), entry(def("Q_NPN_SOT-23", "Transistor")), entry(def("C_0603", "Capacitor"), "project")];
    const g = resolvePalette(COMMON_PARTS, lib);
    expect(g.map((x) => [x.prefix, x.variants.map((v) => v.entry.id)])).toEqual([
      ["R", ["R_0603", "R_0805"]],
      ["Q", ["Q_NPN_SOT-23"]],
    ]);
    expect(resolvePalette(COMMON_PARTS, [])).toEqual([]);
    // The curated list covers R/C/L/D/Q with package variants.
    expect(COMMON_PARTS.map((x) => x.prefix)).toEqual(["R", "C", "L", "D", "Q"]);
    expect(COMMON_PARTS.every((x) => x.variants.length >= 4)).toBe(true);
  });
});
