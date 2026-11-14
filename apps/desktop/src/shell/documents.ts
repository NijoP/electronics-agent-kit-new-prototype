// Document tabs. A document is a view of something that exists in the engine snapshot
// (a schematic sheet, the PCB board), identified by the engine's own ids. Tab state is
// only "which engine documents are open"; the project, its revision and its dirty/blocked
// state stay authoritative in Rust. Documents that no longer exist are dropped on restore.
import type { Id, Snapshot } from "../types";
import { baseName, fileDocId, isFileDoc, relOfFileDoc } from "./files";

export type DocKind = "schematic" | "pcb" | "file";
export interface Doc {
  id: string;
  kind: DocKind;
  title: string;
  /** Engine id: sheet id for schematics, project id for the (single) PCB board, relative path for a file. */
  ref: Id;
}

export function availableDocs(s: Snapshot): Doc[] {
  const sheets: Doc[] = Object.values(s.project.schematic.sheets).map((sh) => ({ id: `sch:${sh.id}`, kind: "schematic", title: sh.name, ref: sh.id }));
  return [...sheets, { id: `pcb:${s.project.id}`, kind: "pcb", title: "PCB", ref: s.project.id }];
}

/** Read-only viewer documents for project files opened from the explorer. */
export const fileDoc = (rel: string): Doc => ({ id: fileDocId(rel), kind: "file", title: baseName(rel), ref: rel });
export const fileDocs = (ids: string[]): Doc[] => ids.filter(isFileDoc).map((id) => fileDoc(relOfFileDoc(id)));

const key = (projectId: Id) => `eak.docs.${projectId}`;

export interface DocState {
  open: string[];
  active: string | null;
}

/** Default: the schematic and the PCB are both open as tabs, the first schematic active (only the active one is shown). */
export function defaultDocState(s: Snapshot): DocState {
  const docs = availableDocs(s);
  return { open: docs.map((d) => d.id), active: docs[0]?.id ?? null };
}

/** Restore the last tab set for this project, keeping only documents the engine still has. */
export function loadDocState(s: Snapshot): DocState {
  const valid = new Set(availableDocs(s).map((d) => d.id));
  try {
    const raw = JSON.parse(localStorage.getItem(key(s.project.id)) ?? "null") as Partial<DocState> | null;
    if (raw && Array.isArray(raw.open)) {
      // File viewer tabs are not engine documents: keep them; the viewer reports a vanished file itself.
      const open = raw.open.filter((id): id is string => typeof id === "string" && (valid.has(id) || isFileDoc(id)));
      const active = typeof raw.active === "string" && open.includes(raw.active) ? raw.active : (open[0] ?? null);
      return { open, active };
    }
  } catch {
    /* fall through to default */
  }
  return defaultDocState(s);
}

export function saveDocState(projectId: Id, st: DocState): void {
  try {
    localStorage.setItem(key(projectId), JSON.stringify(st));
  } catch {
    /* storage unavailable */
  }
}

export function openDoc(st: DocState, id: string): DocState {
  return { open: st.open.includes(id) ? st.open : [...st.open, id], active: id };
}

export function closeDoc(st: DocState, id: string): DocState {
  const i = st.open.indexOf(id);
  if (i < 0) return st;
  const open = st.open.filter((x) => x !== id);
  const active = st.active === id ? (open[Math.min(i, open.length - 1)] ?? null) : st.active;
  return { open, active };
}

/** The documents that are drawn: the active one, plus its schematic/PCB counterpart when split view is on. */
export function visibleDocs(st: DocState, docs: Doc[], split: boolean): Doc[] {
  const byId = new Map(docs.map((d) => [d.id, d]));
  const active = st.active ? byId.get(st.active) : undefined;
  if (!active) return [];
  if (!split || active.kind === "file") return [active];
  const other = st.open.map((id) => byId.get(id)).find((d) => d && d.kind !== "file" && d.kind !== active.kind);
  if (!other) return [active];
  return active.kind === "schematic" ? [active, other] : [other, active];
}

/**
 * Rewrite or drop the open file viewer tabs (a file was renamed or deleted). `f` returns the new
 * relative path, or null to close the tab; the active tab falls back to a neighbour.
 */
export function remapFileDocs(st: DocState, f: (rel: string) => string | null): DocState {
  const open: string[] = [];
  let active = st.active;
  for (const id of st.open) {
    if (!isFileDoc(id)) {
      open.push(id);
      continue;
    }
    const next = f(relOfFileDoc(id));
    if (next === null) {
      if (active === id) active = null;
      continue;
    }
    const nid = fileDocId(next);
    if (active === id) active = nid;
    if (!open.includes(nid)) open.push(nid);
  }
  return { open, active: active ?? open[Math.min(Math.max(0, st.open.indexOf(st.active ?? "")), open.length - 1)] ?? null };
}
