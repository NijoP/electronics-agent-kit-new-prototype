// Document tabs. A document is a view of something that exists in the engine snapshot
// (a schematic sheet, the PCB board), identified by the engine's own ids. Tab state is
// only "which engine documents are open"; the project, its revision and its dirty/blocked
// state stay authoritative in Rust. Documents that no longer exist are dropped on restore.
import type { Id, Snapshot } from "../types";

export type DocKind = "schematic" | "pcb";
export interface Doc {
  id: string;
  kind: DocKind;
  title: string;
  /** Engine id: sheet id for schematics, project id for the (single) PCB board. */
  ref: Id;
}

export function availableDocs(s: Snapshot): Doc[] {
  const sheets: Doc[] = Object.values(s.project.schematic.sheets).map((sh) => ({ id: `sch:${sh.id}`, kind: "schematic", title: sh.name, ref: sh.id }));
  return [...sheets, { id: `pcb:${s.project.id}`, kind: "pcb", title: "PCB", ref: s.project.id }];
}

const key = (projectId: Id) => `eak.docs.${projectId}`;

export interface DocState {
  open: string[];
  active: string | null;
}

/** Default: every document open, first schematic active. */
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
      const open = raw.open.filter((id): id is string => typeof id === "string" && valid.has(id));
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

/** The documents that are drawn: the active one, plus its counterpart when split view is on. */
export function visibleDocs(st: DocState, docs: Doc[], split: boolean): Doc[] {
  const byId = new Map(docs.map((d) => [d.id, d]));
  const active = st.active ? byId.get(st.active) : undefined;
  if (!active) return [];
  if (!split) return [active];
  const other = st.open.map((id) => byId.get(id)).find((d) => d && d.kind !== active.kind);
  if (!other) return [active];
  return active.kind === "schematic" ? [active, other] : [other, active];
}
