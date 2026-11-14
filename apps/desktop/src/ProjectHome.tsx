// Project Home: the engineering project launcher. Open a project folder with the operating system's
// dialog (no pasted paths), find one in the recent list (search by name or folder), reopen the last
// session, or create one (name, location, copper layers). The project that opens becomes the shared
// context for the EDA and Terminal workspaces.
//
// Only real data is shown. A recent row carries what the recent-project index stores (name, folder,
// revision and open time) plus a read-only status read from the project folder (`status`): stage
// derived from content, file modification time, checks with no evidence for the current revision,
// and errors in current evidence. An unreadable folder says so; nothing is inferred or invented.
import { useEffect, useMemo, useRef, useState } from "react";
import { api, pickFolder } from "./api";
import { formatShortcut, HOME_SHORTCUTS, matches, tip } from "./design/shortcuts";
import { EmptyState } from "./design/ui";
import type { ProjectStatus, RecentProject, Snapshot } from "./types";

interface Props {
  onOpened: (s: Snapshot) => void;
  /** False while another top-level workspace is shown: Home's shortcuts stay quiet. */
  active?: boolean;
  /** Theme and model settings live on Home's header (they used to float in a corner). */
  theme?: "dark" | "light";
  onTheme?: () => void;
  onSettings?: () => void;
}

const SESSION_KEY = "eak.session.projects";

export function saveSession(dirs: string[]): void {
  try {
    localStorage.setItem(SESSION_KEY, JSON.stringify(dirs));
  } catch {
    /* storage unavailable */
  }
}
function loadSession(): string[] {
  try {
    const raw = JSON.parse(localStorage.getItem(SESSION_KEY) ?? "[]") as unknown;
    return Array.isArray(raw) ? raw.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

/** Folder a new project is created in: `<parent>/<name>` with the platform's separator. */
export function projectFolder(parent: string, name: string): string {
  const sep = parent.includes("\\") && !parent.includes("/") ? "\\" : "/";
  const clean = name.trim().replace(/[<>:"/\\|?*\u0000-\u001f]/g, "-");
  return `${parent.replace(/[\\/]+$/, "")}${sep}${clean}`;
}

export function ago(ms: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return `${Math.floor(s / 86400)} d ago`;
}


/** One line of real project status for a recent row: stage, last modified, validation, errors. */
export function statusLine(s: ProjectStatus | null | undefined, now = Date.now()): string {
  if (!s) return "status unavailable (folder moved or unreadable)";
  const parts = [s.phase];
  if (s.last_modified_ms != null) parts.push(`modified ${ago(s.last_modified_ms, now)}`);
  parts.push(
    s.pending_validation.length === 0
      ? "validation current"
      : `validation pending: ${s.pending_validation.join(", ")}`,
  );
  if (s.unresolved_errors != null) parts.push(`${s.unresolved_errors} error${s.unresolved_errors === 1 ? "" : "s"} in current checks`);
  return parts.join(" · ");
}

/** Recent projects whose name or folder contains every word of the query (case-insensitive). */
export function filterRecent(recent: RecentProject[], query: string): RecentProject[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return recent;
  return recent.filter((r) => {
    const hay = `${r.name} ${r.path}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

export function ProjectHome({ onOpened, active = true, theme, onTheme, onSettings }: Props) {
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [recentError, setRecentError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [takeOver, setTakeOver] = useState(false);
  const [parent, setParent] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [layers, setLayers] = useState(2);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const session = loadSession();
  const searchRef = useRef<HTMLInputElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    api
      .recentProjects(50)
      .then((r) => setRecent(Array.isArray(r) ? r : []))
      .catch((e) => setRecentError(String(e)))
      .finally(() => setLoaded(true));
  }, []);
  const shown = useMemo(() => filterRecent(recent, query), [recent, query]);

  const attempt = async (f: () => Promise<Snapshot>) => {
    setBusy(true);
    setError(null);
    try {
      onOpened(await f());
    } catch (e) {
      setError(String(e).replace(/^Error: /, ""));
    } finally {
      setBusy(false);
    }
  };
  const choose = async (title: string): Promise<string | null> => {
    try {
      return await pickFolder(title);
    } catch (e) {
      setError(`The folder dialog failed: ${String(e)}`);
      return null;
    }
  };
  const openFolder = async () => {
    const dir = await choose("Open an EAK project folder");
    if (dir) await attempt(() => api.openProject(dir, takeOver));
  };
  const chooseParent = async () => {
    const dir = await choose("Choose where the new project folder is created");
    if (dir) setParent(dir);
  };
  const create = () =>
    attempt(async () => {
      let snap = await api.createProject(projectFolder(parent!, name), name.trim());
      // Config: the chosen layer count goes through the same typed command the editor uses.
      if (layers !== 2) {
        const r = await api.execute(snap.project.revision, { type: "SetLayerCount", count: layers, board_thickness: null });
        if (!r.accepted) throw new Error(`Project created, but ${layers} layers could not be set: ${r.error ? JSON.stringify(r.error) : "rejected"}`);
        snap = await api.currentProject();
      }
      return snap;
    });
  const reopenSession = () =>
    attempt(async () => {
      let last: Snapshot | null = null;
      for (const dir of session) last = await api.openProject(dir, false);
      if (!last) throw new Error("Nothing to reopen");
      return last;
    });
  const ready = !!parent && name.trim().length > 0;

  // Mod+O opens a folder, Mod+N starts a new project, Mod+F or "/" searches the recent list.
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      const t = e.target;
      const typing = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;
      if (matches(e, HOME_SHORTCUTS.openProject)) {
        e.preventDefault();
        if (!busy) void openFolder();
      } else if (matches(e, HOME_SHORTCUTS.newProject)) {
        e.preventDefault();
        nameRef.current?.focus();
      } else if (matches(e, HOME_SHORTCUTS.find) || (!typing && e.key === "/")) {
        e.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // Arrow keys move between rows; from the search box, ArrowDown enters the list.
  const rowNav = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const rows = Array.from(listRef.current?.querySelectorAll<HTMLButtonElement>("button.home-row") ?? []);
    if (rows.length === 0) return;
    e.preventDefault();
    const i = rows.indexOf(document.activeElement as HTMLButtonElement);
    if (i < 0) {
      if (e.key === "ArrowDown") rows[0].focus();
      return;
    }
    if (e.key === "ArrowUp" && i === 0) searchRef.current?.focus();
    else rows[Math.max(0, Math.min(rows.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))].focus();
  };

  return (
    <main className="home">
      <div className="home-inner">
        <header className="home-head">
          <div className="home-brand">
            <span className="home-mark" aria-hidden="true" />
            <div>
              <h1>EAK</h1>
              <p className="muted">Electronics engineering workstation · schematic, PCB, validation and terminal agents on one project</p>
            </div>
          </div>
          <div className="home-head-actions">
            {onTheme && (
              <button type="button" onClick={onTheme} title="Toggle light/dark theme">
                {theme === "dark" ? "Light theme" : "Dark theme"}
              </button>
            )}
            {onSettings && (
              <button type="button" onClick={onSettings}>
                Model settings
              </button>
            )}
          </div>
        </header>

        {error && (
          <p role="alert" className="home-alert">
            {error}
          </p>
        )}

        <div className="home-grid">
          <section aria-label="Recent projects" className="home-recent">
            <header className="home-section-head">
              <h2>Recent projects</h2>
              {recent.length > 0 && <span className="count">{recent.length}</span>}
              <span className="spacer" />
              <input
                ref={searchRef}
                type="search"
                className="home-search"
                aria-label="Search projects"
                placeholder={`Search name or folder (${formatShortcut(HOME_SHORTCUTS.find)})`}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Escape") setQuery("");
                  else if (e.key === "Enter" && shown.length > 0 && !busy) void attempt(() => api.openProject(shown[0].path, takeOver));
                  else rowNav(e);
                }}
              />
            </header>
            {recentError && <p className="error">{recentError}</p>}
            {!recentError && loaded && recent.length === 0 && (
              <EmptyState title="No recent projects.">
                Open an existing project folder or create a new project; it is listed here with its location, revision and when it was last opened.
              </EmptyState>
            )}
            {recent.length > 0 && shown.length === 0 && <p className="muted home-none">No project matches “{query}”.</p>}
            {shown.length > 0 && (
              <div className="home-table">
                <div className="home-cols" aria-hidden="true">
                  <span>Project</span>
                  <span>Location</span>
                  <span className="num">Revision</span>
                  <span className="num">Opened</span>
                </div>
                <ul className="recent" ref={listRef} onKeyDown={rowNav}>
                  {shown.map((r) => (
                    <li key={r.path}>
                      <button
                        type="button"
                        className="home-row"
                        disabled={busy}
                        title={`Open ${r.path}`}
                        onClick={() => attempt(() => api.openProject(r.path, takeOver))}
                      >
                        <strong className="home-name">{r.name}</strong>
                        <span className="home-loc">{r.path}</span>
                        <span className="num muted">revision {r.revision}</span>
                        <span className="num muted">{ago(r.last_opened_ms)}</span>
                        <span className="home-status muted">{statusLine(r.status)}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </section>

          <aside className="home-side">
            <section aria-label="Start" className="home-card">
              <h2>Open a project</h2>
              <button type="button" className="primary home-wide" disabled={busy} onClick={openFolder} title={tip("Open an EAK project folder", HOME_SHORTCUTS.openProject)}>
                Open project folder…
              </button>
              {session.length > 0 && (
                <button type="button" className="home-wide" disabled={busy} onClick={reopenSession} title={session.join("\n")}>
                  Reopen last session ({session.length} project{session.length > 1 ? "s" : ""})
                </button>
              )}
              <label className="inline home-small">
                <input type="checkbox" checked={takeOver} onChange={(e) => setTakeOver(e.target.checked)} />
                Take over a stale lock (only after a crash; never while the project is open elsewhere)
              </label>
            </section>

            <form
              aria-label="Create project"
              className="home-card"
              onSubmit={(e) => {
                e.preventDefault();
                if (ready) void create();
              }}
            >
              <h2>New project</h2>
              <label>
                Name
                <input ref={nameRef} value={name} onChange={(e) => setName(e.target.value)} placeholder={`e.g. motor-driver (${formatShortcut(HOME_SHORTCUTS.newProject)})`} />
              </label>
              <div className="home-field">
                <span className="home-label">Location</span>
                <div className="home-actions">
                  <button type="button" disabled={busy} onClick={chooseParent}>
                    Choose parent folder…
                  </button>
                  <span className="muted home-path">{parent ?? "No folder chosen"}</span>
                </div>
              </div>
              <label>
                Copper layers
                <select value={layers} onChange={(e) => setLayers(Number(e.target.value))}>
                  {[2, 4, 6].map((n) => (
                    <option key={n} value={n}>{n}</option>
                  ))}
                </select>
              </label>
              {ready && (
                <p className="muted home-small">
                  Creates and opens <code>{projectFolder(parent!, name)}</code>
                </p>
              )}
              <button type="submit" className="primary" disabled={busy || !ready} title={ready ? undefined : "Enter a name and choose a parent folder"}>
                Create
              </button>
            </form>
          </aside>
        </div>
      </div>
    </main>
  );
}
