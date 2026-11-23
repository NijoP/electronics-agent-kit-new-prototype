// Assets view (Phase 1 slice E9, assets half): what the project refers to outside its own design
// data, read from real state only. Pure helpers over the engine snapshot (the project library's
// definitions as the IR stores them: 3D model references, datasheets, provenance), the library
// catalog (licence and source per library) and the project's `output/rev-<N>` folders. Nothing
// here edits the project; the IR is unchanged (assets = IR model refs).
import type { FileEntry } from "./files";
import type { ModelRef, Snapshot } from "../types";

/** Fields of a project-library definition the snapshot carries but `Project["library"]` does not type. */
interface DefAssets {
  name: string;
  datasheet?: string | null;
  provenance?: string;
  model3d?: { kind?: string; model_ref?: ModelRef | null };
}

export interface UsedBy {
  symbolId: string;
  refdes: string;
}

export interface PartAssets {
  componentId: string;
  name: string;
  /** Placed schematic parts using the definition (refdes order). */
  usedBy: UsedBy[];
  model: ModelRef | null;
  datasheet: string | null;
  provenance: string | null;
}

const refdesOrder = (a: UsedBy, b: UsedBy) => a.refdes.localeCompare(b.refdes, undefined, { numeric: true });

/** One row per definition in the project library, with the parts that use it (pure). */
export function partAssets(s: Snapshot): PartAssets[] {
  const lib = (s.project.library ?? {}) as Record<string, DefAssets>;
  const used = new Map<string, UsedBy[]>();
  for (const sym of Object.values(s.project.schematic.symbols)) {
    const list = used.get(sym.component_id) ?? [];
    list.push({ symbolId: sym.id, refdes: sym.refdes });
    used.set(sym.component_id, list);
  }
  return Object.entries(lib)
    .map(([id, d]) => ({
      componentId: id,
      name: d.name,
      usedBy: (used.get(id) ?? []).sort(refdesOrder),
      model: d.model3d?.model_ref ?? null,
      datasheet: d.datasheet?.trim() ? d.datasheet.trim() : null,
      provenance: d.provenance?.trim() ? d.provenance.trim() : null,
    }))
    .sort((a, b) => a.componentId.localeCompare(b.componentId));
}

/** Plain-words trust level of a model reference; an undeclared one is never shown as exact. */
export function modelQuality(m: ModelRef): string {
  switch (m.quality) {
    case "VENDOR_EXACT":
      return "Vendor exact";
    case "LIBRARY_VERIFIED":
      return "Library verified";
    case "DERIVED_VALIDATED":
      return "Derived, validated";
    default:
      return "Quality not declared";
  }
}

export interface OutputRevision {
  revision: number;
  entry: FileEntry;
  /** Exported from the revision the project is at now. */
  current: boolean;
}

/** `output/rev-<N>` folders, newest first; anything else in `output/` is not a revision (pure). */
export function outputRevisions(entries: FileEntry[], revision: number): OutputRevision[] {
  const out: OutputRevision[] = [];
  for (const e of entries) {
    const m = /^rev-(\d+)$/.exec(e.name);
    if (e.is_dir && m) out.push({ revision: Number(m[1]), entry: e, current: Number(m[1]) === revision });
  }
  return out.sort((a, b) => b.revision - a.revision);
}

/** Status of an export manifest's JSON text, or null when it cannot be read as one. */
export function manifestStatus(text: string | null): { status: string; revision: number | null; files: number } | null {
  if (!text) return null;
  try {
    const m = JSON.parse(text) as { status?: unknown; revision?: unknown; files?: unknown };
    if (typeof m.status !== "string") return null;
    return { status: m.status, revision: typeof m.revision === "number" ? m.revision : null, files: Array.isArray(m.files) ? m.files.length : 0 };
  } catch {
    return null;
  }
}

export const formatSize = (n: number) => (n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`);
