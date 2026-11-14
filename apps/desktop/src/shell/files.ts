// Types and pure helpers for the project file explorer. The data comes from the backend's
// sandboxed `project_files*` commands; nothing here touches the disk.

export interface FileEntry {
  name: string;
  /** Project-relative path, `/` separated on every platform. */
  rel: string;
  is_dir: boolean;
  size: number;
  /** Owned by the engine (design, journal, checkpoints, evidence, lock): view only. */
  protected: boolean;
  role: "design" | "history" | "evidence" | "generated" | "library" | "config" | "lock" | "folder" | "file";
  modified_ms: number | null;
}

export interface FileContent {
  rel: string;
  size: number;
  text: string | null;
  binary: boolean;
  truncated: boolean;
}

export const ROLE_LABEL: Record<FileEntry["role"], string> = {
  design: "Schematic + PCB",
  history: "History",
  evidence: "Verification",
  generated: "Generated",
  library: "Library",
  config: "Config",
  lock: "Lock",
  folder: "",
  file: "",
};

export const parentOf = (rel: string): string => (rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : "");
export const joinRel = (dir: string, name: string): string => (dir ? `${dir}/${name}` : name);
export const baseName = (rel: string): string => rel.slice(rel.lastIndexOf("/") + 1);

/** Document id of an open file viewer tab. */
export const fileDocId = (rel: string): string => `file:${rel}`;
export const isFileDoc = (id: string): boolean => id.startsWith("file:");
export const relOfFileDoc = (id: string): string => id.slice("file:".length);

const RESERVED = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/i;

/** Quick client-side check mirroring the backend's rules, so the error shows before the call. */
export function nameProblem(name: string): string | null {
  if (!name.trim()) return "Enter a name";
  if (name === "." || name === "..") return "That is not a valid name";
  // eslint-disable-next-line no-control-regex
  if (/[<>:"|?*/\\\u0000-\u001f]/.test(name)) return 'A name cannot contain < > : " | ? * / \\';
  if (/[ .]$/.test(name)) return "A name cannot end with a space or a dot";
  if (RESERVED.test(name.split(".")[0])) return `"${name}" is a reserved name on Windows`;
  return null;
}

export function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / 1024 / 1024).toFixed(1)} MiB`;
}

/** Flatten the loaded tree into the rows currently visible (expanded folders only). */
export function visibleRows(
  children: Record<string, FileEntry[] | undefined>,
  expanded: ReadonlySet<string>,
  dir = "",
  depth = 0,
): { entry: FileEntry; depth: number }[] {
  const out: { entry: FileEntry; depth: number }[] = [];
  for (const entry of children[dir] ?? []) {
    out.push({ entry, depth });
    if (entry.is_dir && expanded.has(entry.rel)) out.push(...visibleRows(children, expanded, entry.rel, depth + 1));
  }
  return out;
}

export interface GitStatus {
  available: boolean;
  branch: string | null;
  entries: { rel: string; code: string }[];
}

/** One-letter badge and colour class for a porcelain code. */
export function gitBadge(code: string): { letter: string; kind: "modified" | "added" | "deleted" | "untracked" | "conflict" } {
  if (code === "??") return { letter: "U", kind: "untracked" };
  if (code.includes("U") || code === "AA" || code === "DD") return { letter: "!", kind: "conflict" };
  if (code.includes("D")) return { letter: "D", kind: "deleted" };
  if (code.includes("A") || code.includes("R") || code.includes("C")) return { letter: "A", kind: "added" };
  return { letter: "M", kind: "modified" };
}

/** Status per path, with every ancestor folder marked "modified" when something inside changed. */
export function gitMarks(entries: GitStatus["entries"]): Map<string, ReturnType<typeof gitBadge>> {
  const out = new Map<string, ReturnType<typeof gitBadge>>();
  for (const e of entries) {
    out.set(e.rel, gitBadge(e.code));
    for (let p = parentOf(e.rel); p; p = parentOf(p)) if (!out.has(p)) out.set(p, { letter: "•", kind: "modified" });
  }
  return out;
}
