// BOM export configuration (slice D3): the column picker, column order, per-column sort,
// grouping and filter rules of the BOM dialog. The configuration is the engine's own type
// (crates/eak-manufacturing/src/bom_config.rs); the engine applies it, both for the dialog's
// preview and for the exported file. The last configuration is remembered on this computer.
import type { BomColumn, BomConfig, BomField, FilterOp, FilterRule, SortDir } from "../types";

/** Every column the picker offers, with the engine's default header, in picker order. */
export const BOM_FIELDS: readonly { field: BomField; label: string }[] = [
  { field: "comment", label: "Comment" },
  { field: "designator", label: "Designator" },
  { field: "quantity", label: "Quantity" },
  { field: "footprint", label: "Footprint" },
  { field: "component_id", label: "Component" },
  { field: "manufacturer", label: "Manufacturer" },
  { field: "mpn", label: "MPN" },
  { field: "distributor_part", label: "Distributor Part #" },
  { field: "smd", label: "SMD" },
  { field: "placed", label: "Placed" },
  { field: "assembled", label: "Assembled by Fabricator" },
  { field: "stock", label: "Stock (stored record)" },
  { field: "unit_price_usd", label: "Unit price USD (stored record)" },
  { field: "price_as_of", label: "Price as of (UTC)" },
  { field: "issues", label: "Issues" },
];

export const FILTER_OPS: readonly { op: FilterOp; label: string; needsValue: boolean }[] = [
  { op: "equals", label: "is", needsValue: true },
  { op: "not_equals", label: "is not", needsValue: true },
  { op: "contains", label: "contains", needsValue: true },
  { op: "is_empty", label: "is empty", needsValue: false },
  { op: "not_empty", label: "is not empty", needsValue: false },
];

export const fieldLabel = (f: BomField) => BOM_FIELDS.find((x) => x.field === f)?.label ?? f;

/** The engine's default configuration (BomConfig::default): what to buy, sorted by designator. */
export function defaultBomConfig(): BomConfig {
  const col = (field: BomField, sort: SortDir = "none"): BomColumn => ({ field, sort });
  return {
    version: 1,
    columns: [col("comment"), col("designator", "asc"), col("footprint"), col("distributor_part"), col("manufacturer"), col("mpn"), col("quantity")],
    grouping: "part",
    exclude: [],
  };
}

const KNOWN = new Set<string>(BOM_FIELDS.map((f) => f.field));
const OPS = new Set<string>(FILTER_OPS.map((o) => o.op));
const SORTS = new Set<string>(["none", "asc", "desc"]);

/** A configuration read from JSON, or null when it is not one this version understands. */
export function parseBomConfig(json: string): BomConfig | null {
  let v: unknown;
  try {
    v = JSON.parse(json);
  } catch {
    return null;
  }
  const c = v as Partial<BomConfig> | null;
  if (!c || typeof c !== "object" || !Array.isArray(c.columns) || c.columns.length === 0) return null;
  const columns: BomColumn[] = [];
  for (const raw of c.columns as Partial<BomColumn>[]) {
    if (!raw || typeof raw.field !== "string" || !KNOWN.has(raw.field)) return null;
    const field = raw.field;
    if (columns.some((x) => x.field === field)) return null;
    const sort: SortDir = typeof raw.sort === "string" && SORTS.has(raw.sort) ? raw.sort : "none";
    columns.push(typeof raw.header === "string" && raw.header !== "" ? { field, header: raw.header, sort } : { field, sort });
  }
  const exclude: FilterRule[] = [];
  for (const raw of (Array.isArray(c.exclude) ? c.exclude : []) as Partial<FilterRule>[]) {
    if (!raw || typeof raw.field !== "string" || !KNOWN.has(raw.field) || typeof raw.op !== "string" || !OPS.has(raw.op)) return null;
    exclude.push({ field: raw.field, op: raw.op, value: typeof raw.value === "string" ? raw.value : "" });
  }
  return { version: typeof c.version === "number" ? c.version : 1, columns, grouping: c.grouping === "designator" ? "designator" : "part", exclude };
}

const KEY = "eak.bomConfig.v1";

/** The configuration last used on this computer (the default when none is stored or it is unreadable). */
export function loadBomConfig(storage: Pick<Storage, "getItem"> | undefined = globalThis.localStorage): BomConfig {
  let raw: string | null = null;
  try {
    raw = storage?.getItem(KEY) ?? null;
  } catch {
    raw = null;
  }
  return (raw ? parseBomConfig(raw) : null) ?? defaultBomConfig();
}

export function saveBomConfig(c: BomConfig, storage: Pick<Storage, "setItem"> | undefined = globalThis.localStorage): void {
  try {
    storage?.setItem(KEY, JSON.stringify(c));
  } catch {
    // Storage full or disabled: the configuration still applies to this export.
  }
}

/** Adds the column at the end, or removes it (the last column cannot be removed). */
export function toggleColumn(c: BomConfig, field: BomField): BomConfig {
  const has = c.columns.some((x) => x.field === field);
  if (has && c.columns.length === 1) return c;
  return { ...c, columns: has ? c.columns.filter((x) => x.field !== field) : [...c.columns, { field, sort: "none" }] };
}

/** Moves column `i` by `by` places (clamped to the table). */
export function moveColumn(c: BomConfig, i: number, by: number): BomConfig {
  const j = Math.max(0, Math.min(c.columns.length - 1, i + by));
  if (i === j || i < 0 || i >= c.columns.length) return c;
  const columns = [...c.columns];
  const [col] = columns.splice(i, 1);
  columns.splice(j, 0, col);
  return { ...c, columns };
}

export function updateColumn(c: BomConfig, i: number, patch: Partial<BomColumn>): BomConfig {
  return { ...c, columns: c.columns.map((x, k) => (k === i ? { ...x, ...patch } : x)) };
}

export function updateRule(c: BomConfig, i: number, patch: Partial<FilterRule>): BomConfig {
  return { ...c, exclude: c.exclude.map((x, k) => (k === i ? { ...x, ...patch } : x)) };
}
