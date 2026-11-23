// Library dock model (Phase 1 D1, gaps #36-#38): kinds, scopes, the category tree, paging, the
// supplier columns and the common-parts palette. Pure functions over what `library_browse`
// returns; the dock keeps no library of its own. Supplier data (stock, price) comes from a
// `SupplierOffer` or a stored sourcing record and is shown next to the part, never as part of its
// identity (ADR-0028), and never as evidence that a part is suitable.
import type { FootprintDef, LibraryEntry2, OfferPrice } from "../types";

export type LibKind = "part" | "symbol" | "footprint" | "model";
export const KINDS: readonly [LibKind, string][] = [
  ["part", "Part"],
  ["symbol", "Symbol"],
  ["footprint", "Footprint"],
  ["model", "3D"],
];

export type LibScope = "builtin" | "project" | "personal" | "imported" | "distributor";
export const SCOPES: readonly [LibScope, string][] = [
  ["builtin", "Built-in"],
  ["project", "Project"],
  ["personal", "Personal"],
  ["imported", "Imported"],
  ["distributor", "Distributor"],
];

/** The scopes offline mode keeps: nothing outside the app and the open project is read. */
export const OFFLINE_SCOPES: readonly LibScope[] = ["builtin", "project"];

/** Why a scope cannot be used right now, or null when it can. */
export function scopeUnavailable(scope: LibScope, offline: boolean): string | null {
  if (offline && !OFFLINE_SCOPES.includes(scope)) return "Offline mode: only the built-in and project libraries are used";
  return null;
}

// ---- category tree -------------------------------------------------------------------------

/** Families the flat library categories are grouped under. A category not listed here goes to
 *  "Other"; a category written as "Family/Leaf" names its own family. */
const FAMILIES: Record<string, string> = {
  Resistor: "Passives",
  Capacitor: "Passives",
  Inductor: "Passives",
  Crystal: "Passives",
  Diode: "Semiconductors",
  LED: "Semiconductors",
  Transistor: "Semiconductors",
  MCU: "Integrated circuits",
  Power: "Integrated circuits",
  Sensor: "Integrated circuits",
  Connector: "Electromechanical",
  Switch: "Electromechanical",
};

export interface CatNode {
  /** `f:<family>` or `c:<category>`. */
  key: string;
  label: string;
  count: number;
  children: CatNode[];
}

function familyOf(category: string): [string, string] {
  const i = category.indexOf("/");
  if (i > 0) return [category.slice(0, i).trim(), category.slice(i + 1).trim() || category];
  return [FAMILIES[category] ?? "Other", category];
}

/** Families with their categories and part counts, both sorted by name ("Other" last). */
export function categoryTree(entries: readonly LibraryEntry2[]): CatNode[] {
  const fams = new Map<string, Map<string, number>>();
  for (const e of entries) {
    const [fam] = familyOf(e.def.category);
    const cats = fams.get(fam) ?? new Map<string, number>();
    cats.set(e.def.category, (cats.get(e.def.category) ?? 0) + 1);
    fams.set(fam, cats);
  }
  const byName = (a: CatNode, b: CatNode) => (a.label === "Other" ? 1 : b.label === "Other" ? -1 : a.label.localeCompare(b.label));
  return [...fams.entries()]
    .map(([fam, cats]) => {
      const children: CatNode[] = [...cats.entries()].map(([c, n]) => ({ key: `c:${c}`, label: familyOf(c)[1], count: n, children: [] })).sort(byName);
      return { key: `f:${fam}`, label: fam, count: children.reduce((s, c) => s + c.count, 0), children };
    })
    .sort(byName);
}

/** Whether an entry is under a tree node (no node = everything). */
export function inCategory(e: LibraryEntry2, key: string | null): boolean {
  if (!key) return true;
  if (key.startsWith("c:")) return e.def.category === key.slice(2);
  return familyOf(e.def.category)[0] === key.slice(2);
}

// ---- paging --------------------------------------------------------------------------------

export const PAGE_SIZE = 20;

export interface Page<T> {
  rows: T[];
  /** Zero-based, clamped to the pages that exist. */
  page: number;
  pages: number;
  /** One-based first and last row shown (0 and 0 when there are none). */
  from: number;
  to: number;
  total: number;
}

export function pageOf<T>(rows: readonly T[], page: number, size = PAGE_SIZE): Page<T> {
  const pages = Math.max(1, Math.ceil(rows.length / size));
  const p = Math.min(Math.max(0, page), pages - 1);
  const slice = rows.slice(p * size, p * size + size);
  return { rows: slice, page: p, pages, from: slice.length ? p * size + 1 : 0, to: p * size + slice.length, total: rows.length };
}

// ---- footprints ------------------------------------------------------------------------------

export interface FootprintRow {
  name: string;
  fp: FootprintDef;
  /** Every listed part that uses a footprint of this name. */
  parts: LibraryEntry2[];
}

/** One row per footprint name, sorted by name; the first part's footprint is the one shown. */
export function footprintRows(entries: readonly LibraryEntry2[]): FootprintRow[] {
  const m = new Map<string, FootprintRow>();
  for (const e of entries) {
    const r = m.get(e.def.footprint.name) ?? { name: e.def.footprint.name, fp: e.def.footprint, parts: [] };
    r.parts.push(e);
    m.set(r.name, r);
  }
  return [...m.values()].sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
}

// ---- supplier columns ------------------------------------------------------------------------

export interface SupplierCols {
  supplier: string;
  stock: string;
  price: string;
  /** When the supplier data was fetched (Unix s); null = no snapshot. */
  asOf: number | null;
}

const NONE: SupplierCols = { supplier: "—", stock: "—", price: "—", asOf: null };

export function priceText(breaks: readonly OfferPrice[] | undefined): string {
  const b = breaks?.[0];
  return b ? `${b.unit_price} ${b.currency} @ ${b.min_quantity}+` : "not stated";
}

const stockText = (n: number | null | undefined) => (n === null || n === undefined ? "not stated" : n.toLocaleString("en-US"));

/** Supplier, stock and price of a part: its own supplier offer first, then the sourcing record
 *  stored with the project, then a bare supplier part number with no snapshot. */
export function supplierCols(e: LibraryEntry2): SupplierCols {
  const o = e.def.supplier_offers?.[0];
  if (o) return { supplier: `${o.supplier} ${o.supplier_part}`, stock: stockText(o.stock), price: priceText(o.price_breaks), asOf: o.fetched_at_unix_s ?? null };
  if (e.supplier) return { supplier: `distributor ${e.supplier.distributor}`, stock: stockText(e.supplier.stock), price: priceText(e.supplier.price_breaks), asOf: e.supplier.as_of_unix_s };
  if (e.def.distributor) return { supplier: `distributor ${e.def.distributor}`, stock: "no snapshot", price: "no snapshot", asOf: null };
  return NONE;
}

// ---- common parts palette -------------------------------------------------------------------

export interface CommonGroup {
  label: string;
  /** Reference designator prefix, shown on the palette button. */
  prefix: string;
  variants: { id: string; pkg: string }[];
}

const CHIP = ["0402", "0603", "0805", "1206", "1210"];

/** EAK-curated generic parts with their package variants, by built-in library id. Only ids the
 *  built-in library actually lists are offered (see `resolvePalette`). */
export const COMMON_PARTS: readonly CommonGroup[] = [
  { label: "Resistor", prefix: "R", variants: CHIP.map((s) => ({ id: `R_${s}`, pkg: s })) },
  { label: "Capacitor (MLCC)", prefix: "C", variants: CHIP.map((s) => ({ id: `C_${s}`, pkg: s })) },
  { label: "Inductor", prefix: "L", variants: CHIP.map((s) => ({ id: `L_${s}`, pkg: s })) },
  {
    label: "Diode",
    prefix: "D",
    variants: [
      { id: "D_SOD-123", pkg: "SOD-123" },
      { id: "D_SMA", pkg: "SMA Schottky" },
      { id: "LED_0603", pkg: "LED 0603" },
      { id: "LED_0805", pkg: "LED 0805" },
    ],
  },
  {
    label: "Transistor",
    prefix: "Q",
    variants: [
      { id: "Q_NPN_SOT-23", pkg: "NPN SOT-23" },
      { id: "Q_PNP_SOT-23", pkg: "PNP SOT-23" },
      { id: "Q_NMOS_SOT-23", pkg: "N-MOS SOT-23" },
      { id: "Q_PMOS_SOT-23", pkg: "P-MOS SOT-23" },
    ],
  },
];

export interface PaletteGroup {
  label: string;
  prefix: string;
  variants: { pkg: string; entry: LibraryEntry2 }[];
}

/** The palette restricted to built-in parts that exist (a disabled built-in library = none). */
export function resolvePalette(groups: readonly CommonGroup[], builtin: readonly LibraryEntry2[]): PaletteGroup[] {
  const byId = new Map(builtin.filter((e) => e.scope.kind === "builtin").map((e) => [e.id, e] as const));
  return groups
    .map((g) => ({
      label: g.label,
      prefix: g.prefix,
      variants: g.variants.flatMap((v) => {
        const entry = byId.get(v.id);
        return entry ? [{ pkg: v.pkg, entry }] : [];
      }),
    }))
    .filter((g) => g.variants.length > 0);
}
