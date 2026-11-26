// Project file explorer: the real files of the active project folder, read lazily through the
// sandboxed backend commands. Open / expand / collapse, and create / rename / delete for files the
// engine does not own. Nothing here writes a design file: engine-owned entries are view-only and
// the backend refuses to change them even if this UI were bypassed.
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { confirmed } from "../confirm";
import { baseName, formatSize, gitMarks, joinRel, nameProblem, parentOf, ROLE_LABEL, visibleRows, type FileEntry, type GitStatus } from "./files";

// Reusable: the EDA Explorer mounts it for the open project; the terminal workspace mounts one per
// space. `projectId` is a root id: an open project's id, or `dir:<absolute path>` for any folder.
interface Props {
  projectId: string;
  /** Shown above the tree with the git branch. */
  rootLabel?: string;
  /** Working directory of the current terminal session (root-relative, "" = root): marked in the tree. */
  cwd?: string | null;
  /** When given, the context menu offers "Use as working directory" for folders. */
  onCwd?: (rel: string) => void;
  /** Engine revision: the tree reloads when it changes (journal and exported outputs change with it). */
  revision: number;
  onOpen: (entry: FileEntry) => void;
  onRemoved: (rel: string, isDir: boolean) => void;
  onRenamed: (from: string, to: string) => void;
  notify: (message: string) => void;
}

type Form = { mode: "new-file" | "new-folder"; dir: string; value: string } | { mode: "rename"; target: FileEntry; value: string };

const OWNED = "Owned by the EAK engine: change the design in the editors, not the file";
const key = (projectId: string) => `eak.explorer.${projectId}`;

function loadExpanded(projectId: string): Set<string> {
  try {
    const raw = JSON.parse(localStorage.getItem(key(projectId)) ?? "[]") as unknown;
    return new Set(Array.isArray(raw) ? raw.filter((x): x is string => typeof x === "string") : []);
  } catch {
    return new Set();
  }
}

export function FileExplorer({ projectId, rootLabel, cwd, onCwd, revision, onOpen, onRemoved, onRenamed, notify }: Props) {
  const [children, setChildren] = useState<Record<string, FileEntry[] | undefined>>({});
  const [expanded, setExpanded] = useState<Set<string>>(() => loadExpanded(projectId));
  const [selected, setSelected] = useState<string | null>(null);
  const [form, setForm] = useState<Form | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; entry: FileEntry | null } | null>(null);
  const [busy, setBusy] = useState(false);
  const [git, setGit] = useState<GitStatus | null>(null);
  const expandedRef = useRef(expanded);
  expandedRef.current = expanded;

  const load = useCallback(
    async (dir: string) => {
      try {
        const list = await api.projectFiles(projectId, dir);
        setChildren((c) => ({ ...c, [dir]: list }));
        if (dir === "") setError(null);
      } catch (e) {
        if (dir === "") setError(`Cannot read the project folder: ${String(e)}`);
        else {
          // The folder vanished (deleted outside EAK): drop it from the open set.
          setExpanded((x) => new Set([...x].filter((p) => p !== dir && !p.startsWith(`${dir}/`))));
          setChildren((c) => ({ ...c, [dir]: undefined }));
        }
      }
    },
    [projectId],
  );

  const reload = useCallback(async () => {
    await Promise.all([
      ...["", ...expandedRef.current].map(load),
      api.projectGitStatus(projectId).then(setGit).catch(() => setGit(null)),
    ]);
  }, [load, projectId]);

  useEffect(() => {
    void reload();
  }, [projectId, revision, reload]);

  useEffect(() => {
    try {
      localStorage.setItem(key(projectId), JSON.stringify([...expanded]));
    } catch {
      /* storage unavailable: the tree still works for this session */
    }
  }, [projectId, expanded]);

  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("click", close);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", esc);
    };
  }, [menu]);

  const rows = visibleRows(children, expanded);
  const marks = git?.available ? gitMarks(git.entries) : null;
  const selectedEntry = rows.find((r) => r.entry.rel === selected)?.entry ?? null;

  const toggle = (entry: FileEntry) => {
    const open = expanded.has(entry.rel);
    setExpanded((x) => {
      const next = new Set(x);
      if (open) next.delete(entry.rel);
      else next.add(entry.rel);
      return next;
    });
    if (!open) void load(entry.rel);
  };

  const activate = (entry: FileEntry) => {
    setSelected(entry.rel);
    if (entry.is_dir) toggle(entry);
    else onOpen(entry);
  };

  const targetDir = (entry: FileEntry | null): string => (entry ? (entry.is_dir ? entry.rel : parentOf(entry.rel)) : "");

  const startNew = (mode: "new-file" | "new-folder", entry: FileEntry | null = selectedEntry) => {
    const dir = targetDir(entry);
    if (dir && !expanded.has(dir)) setExpanded((x) => new Set(x).add(dir));
    if (dir && !children[dir]) void load(dir);
    setError(null);
    setForm({ mode, dir, value: "" });
  };
  const startRename = (entry: FileEntry | null = selectedEntry) => {
    if (!entry || entry.protected) return;
    setError(null);
    setForm({ mode: "rename", target: entry, value: entry.name });
  };

  const fail = (e: unknown) => {
    const m = String(e).replace(/^Error: /, "");
    setError(m);
    notify(m);
  };

  const submit = async () => {
    if (!form || busy) return;
    const problem = nameProblem(form.value);
    if (problem) return setError(problem);
    setBusy(true);
    try {
      if (form.mode === "rename") {
        const from = form.target.rel;
        const to = joinRel(parentOf(from), form.value);
        if (to !== from) {
          await api.projectFileRename(projectId, from, to);
          setExpanded((x) => new Set([...x].map((p) => (p === from ? to : p.startsWith(`${from}/`) ? to + p.slice(from.length) : p))));
          onRenamed(from, to);
          setSelected(to);
          notify(`Renamed ${baseName(from)} to ${form.value}`);
        }
      } else {
        const rel = joinRel(form.dir, form.value);
        const made = await api.projectFileCreate(projectId, rel, form.mode === "new-folder");
        setSelected(made.rel);
        notify(`Created ${made.rel}`);
        if (!made.is_dir) onOpen(made);
      }
      setForm(null);
      setError(null);
      await reload();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  const remove = async (entry: FileEntry | null = selectedEntry) => {
    if (!entry || entry.protected || busy) return;
    const what = entry.is_dir ? `the folder "${entry.name}" and everything in it` : `"${entry.name}"`;
    if (!(await confirmed(`Delete ${what}? This cannot be undone.`))) return;
    setBusy(true);
    try {
      await api.projectFileDelete(projectId, entry.rel, entry.is_dir);
      onRemoved(entry.rel, entry.is_dir);
      if (selected === entry.rel) setSelected(null);
      notify(`Deleted ${entry.rel}`);
      setError(null);
      await reload();
    } catch (e) {
      fail(e);
    } finally {
      setBusy(false);
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (form) return;
    const i = rows.findIndex((r) => r.entry.rel === selected);
    const go = (n: number) => rows[n] && setSelected(rows[n].entry.rel);
    if (e.key === "ArrowDown") go(Math.min(rows.length - 1, i + 1));
    else if (e.key === "ArrowUp") go(Math.max(0, i - 1));
    else if (e.key === "ArrowRight" && selectedEntry?.is_dir && !expanded.has(selectedEntry.rel)) toggle(selectedEntry);
    else if (e.key === "ArrowLeft" && selectedEntry?.is_dir && expanded.has(selectedEntry.rel)) toggle(selectedEntry);
    else if (e.key === "Enter" && selectedEntry) activate(selectedEntry);
    else if (e.key === "F2") startRename();
    else if (e.key === "Delete") void remove();
    else return;
    e.preventDefault();
  };

  const collapseAll = () => setExpanded(new Set());
  const protectedWhy = selectedEntry?.protected ? OWNED : undefined;
  const noSel = selectedEntry ? undefined : "Select a file or folder first";

  const menuItems = (entry: FileEntry | null) => [
    ...(entry && !entry.is_dir ? [{ label: "Open", run: () => onOpen(entry) }] : []),
    ...(entry?.is_dir ? [{ label: expanded.has(entry.rel) ? "Collapse" : "Expand", run: () => toggle(entry) }] : []),
    { label: "New file…", run: () => startNew("new-file", entry) },
    { label: "New folder…", run: () => startNew("new-folder", entry) },
    ...(entry ? [{ label: "Rename…", run: () => startRename(entry), why: entry.protected ? OWNED : undefined }] : []),
    ...(entry ? [{ label: "Delete", run: () => void remove(entry), why: entry.protected ? OWNED : undefined }] : []),
    ...(onCwd && (!entry || entry.is_dir) ? [{ label: "Use as working directory", run: () => onCwd(entry ? entry.rel : "") }] : []),
    { label: "Refresh", run: () => void reload() },
  ];

  return (
    <div className="explorer" onContextMenu={(e) => { if (e.target === e.currentTarget) { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, entry: null }); } }}>
      <div className="explorer-actions" role="toolbar" aria-label="Explorer actions">
        <button type="button" onClick={() => startNew("new-file")} title="New file in the selected folder">New file</button>
        <button type="button" onClick={() => startNew("new-folder")} title="New folder in the selected folder">New folder</button>
        <button type="button" onClick={() => startRename()} disabled={!selectedEntry || selectedEntry.protected} title={noSel ?? protectedWhy ?? "Rename (F2)"}>Rename</button>
        <button type="button" onClick={() => void remove()} disabled={!selectedEntry || selectedEntry.protected || busy} title={noSel ?? protectedWhy ?? "Delete (Del)"}>Delete</button>
        <button type="button" onClick={() => void reload()} title="Reload the folder from disk">Refresh</button>
        <button type="button" onClick={collapseAll} disabled={expanded.size === 0} title={expanded.size === 0 ? "No folder is expanded" : "Collapse all folders"}>Collapse all</button>
      </div>
      {form && (
        <form
          className="explorer-form"
          onSubmit={(e) => {
            e.preventDefault();
            void submit();
          }}
        >
          <label>
            {form.mode === "rename" ? `Rename ${form.target.name} to` : `${form.mode === "new-file" ? "New file" : "New folder"} in ${form.dir || "project root"}`}
            <input
              autoFocus
              aria-label={form.mode === "rename" ? "New name" : "Name"}
              value={form.value}
              onChange={(e) => setForm({ ...form, value: e.target.value })}
              onKeyDown={(e) => e.key === "Escape" && (setForm(null), setError(null))}
            />
          </label>
          <button type="submit" className="primary" disabled={busy}>{form.mode === "rename" ? "Rename" : "Create"}</button>
          <button type="button" onClick={() => (setForm(null), setError(null))}>Cancel</button>
        </form>
      )}
      {(rootLabel || git?.branch || cwd != null) && (
        <div className="explorer-branch" aria-label="Project root">
          {rootLabel}
          {git?.available && git.branch ? ` · git ${git.branch}` : ""}
          {cwd != null ? ` · cwd ${cwd || "(root)"}` : ""}
        </div>
      )}
      {error && <p role="alert" className="explorer-error">{error}</p>}
      <ul className="explorer-tree" role="tree" aria-label="Project files" tabIndex={0} onKeyDown={onKeyDown}>
        {rows.map(({ entry, depth }) => {
          const open = entry.is_dir && expanded.has(entry.rel);
          const badge = depth === 0 ? ROLE_LABEL[entry.role] : "";
          return (
            <li key={entry.rel} role="treeitem" aria-level={depth + 1} aria-expanded={entry.is_dir ? open : undefined} aria-selected={entry.rel === selected}>
              <button
                type="button"
                className={`ex-row${entry.rel === selected ? " on" : ""}${entry.protected ? " owned" : ""}${cwd != null && cwd === entry.rel ? " cwd" : ""}`}
                style={{ paddingLeft: 8 + depth * 14 }}
                title={entry.protected ? `${entry.rel} — ${OWNED}` : entry.rel}
                onClick={() => activate(entry)}
                onContextMenu={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  setSelected(entry.rel);
                  setMenu({ x: e.clientX, y: e.clientY, entry });
                }}
              >
                <span className="ex-twist" aria-hidden="true">{entry.is_dir ? (open ? "▾" : "▸") : ""}</span>
                <span className="ex-name">{entry.name}</span>
                {badge && <span className="ex-badge">{badge}</span>}
                {entry.protected && <span className="ex-badge owned" aria-label="engine-owned, view only">view only</span>}
                {cwd != null && cwd === entry.rel && <span className="ex-cwd">cwd</span>}
                {!entry.is_dir && <span className="ex-size muted">{formatSize(entry.size)}</span>}
                {marks?.get(entry.rel) && (
                  <span className={`ex-git ${marks.get(entry.rel)!.kind}`} title={`git: ${marks.get(entry.rel)!.kind}`}>
                    {marks.get(entry.rel)!.letter}
                  </span>
                )}
              </button>
            </li>
          );
        })}
        {rows.length === 0 && !error && <li className="empty-state">The project folder is empty.</li>}
      </ul>
      {menu && (
        <ul className="ctx-menu" role="menu" aria-label="File actions" style={{ left: menu.x, top: menu.y }}>
          {menuItems(menu.entry).map((it) => (
            <li key={it.label} role="none">
              <button type="button" role="menuitem" disabled={!!it.why} title={it.why} onClick={() => { setMenu(null); it.run(); }}>
                {it.label}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
