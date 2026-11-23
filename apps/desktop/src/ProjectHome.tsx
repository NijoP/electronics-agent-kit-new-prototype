// Project Home: the launchpad. Header (identity, navigation, settings), a short introduction, the two
// primary actions (Open Project, New Project; both use the operating system folder dialog, no pasted
// paths), Continue Working (the most recently opened projects) and a searchable, sortable recent list.
//
// Only real data is shown. A recent row carries what the recent-project index stores (name, folder,
// revision and open time) plus a read-only status read from the project folder (`status`): stage
// derived from content, file modification time, checks with no evidence for the current revision,
// and errors in current evidence. An unreadable folder says so; nothing is inferred or invented.
// Nothing engineering-related is initialised here: the list is one IPC call.
import { useEffect, useMemo, useRef, useState } from "react";
import { api, pickFolder } from "./api";
import { formatShortcut, HOME_SHORTCUTS, matches, tip } from "./design/shortcuts";
import { EmptyState, Pill } from "./design/ui";
import { modalOpen } from "./shell/viewState";
import type { LockState, ProjectStatus, RecentProject, Snapshot } from "./types";

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

export type RecentSort = "opened" | "name" | "modified";

/** Sorted copy of the recent list: last opened (default), name, or project-folder modification time. */
export function sortRecent(recent: RecentProject[], by: RecentSort): RecentProject[] {
  const r = [...recent];
  if (by === "name") return r.sort((a, b) => a.name.localeCompare(b.name));
  if (by === "modified") return r.sort((a, b) => (b.status?.last_modified_ms ?? -1) - (a.status?.last_modified_ms ?? -1));
  return r.sort((a, b) => b.last_opened_ms - a.last_opened_ms);
}

/** Validation wording from real evidence only; never "passed" without evidence. */
function validation(s: ProjectStatus | null | undefined): { tone: "ok" | "warn" | "bad" | "neutral"; text: string } {
  if (!s) return { tone: "neutral", text: "Unavailable" };
  if (s.unresolved_errors != null && s.unresolved_errors > 0) return { tone: "bad", text: `${s.unresolved_errors} error${s.unresolved_errors === 1 ? "" : "s"}` };
  if (s.pending_validation.length > 0) return { tone: "warn", text: `${s.pending_validation.length} check${s.pending_validation.length === 1 ? "" : "s"} pending` };
  return { tone: "ok", text: "Checks current" };
}

/** Plain-language explanation of a locked project and the one safe action, if there is one. */
export function lockCopy(s: LockState): { title: string; body: string } {
  switch (s.state) {
    case "open_elsewhere":
      return s.this_app
        ? { title: "Already open", body: "This project is already open in this EAK app. Switch to it instead of opening it again." }
        : { title: "Open elsewhere", body: "This project is open in another EAK window or process. Close it there, then try again. EAK does not override a lock held by a running session." };
    case "interrupted":
      return {
        title: "Previous session interrupted",
        body:
          s.pending_operations > 0
            ? `The last session ended without closing the project, with ${s.pending_operations} unfinished operation${s.pending_operations === 1 ? "" : "s"}. Recovery is available: EAK checks them against the saved project and never replays them blindly.`
            : "The last session ended without closing the project. No unfinished operations were found, and the project file is untouched.",
      };
    case "unverifiable":
      return { title: "Cannot confirm it is safe to open", body: `${s.reason[0].toUpperCase()}${s.reason.slice(1)}. EAK will not take over a lock it cannot prove is stale.` };
    case "unavailable":
      return { title: "Project access unavailable", body: `${s.reason[0].toUpperCase()}${s.reason.slice(1)}. Check that the folder exists and that you can read and write it.` };
    default:
      return { title: "Project could not be opened", body: "Try again." };
  }
}

function LockNotice(p: { lock: { path: string; state: LockState }; busy: boolean; confirming: boolean; onConfirm: () => void; onCancel: () => void; onRecover: () => void; onRetry: () => void }) {
  const { title, body } = lockCopy(p.lock.state);
  const interrupted = p.lock.state.state === "interrupted";
  return (
    <section role="alert" className="home-lock" aria-label="Project lock">
      <strong>{title}</strong>
      <p className="muted">{body}</p>
      <p className="home-path muted">{p.lock.path}</p>
      <div className="home-actions">
        {interrupted && !p.confirming && (
          <button type="button" className="primary" disabled={p.busy} onClick={p.onConfirm}>Recover and open…</button>
        )}
        {interrupted && p.confirming && (
          <>
            <button type="button" className="primary" disabled={p.busy} onClick={p.onRecover}>Confirm: recover and open</button>
            <button type="button" className="ghost" onClick={p.onCancel}>Cancel</button>
          </>
        )}
        {!interrupted && <button type="button" disabled={p.busy} onClick={p.onRetry}>Try again</button>}
      </div>
    </section>
  );
}

export function ProjectHome({ onOpened, active = true, theme, onTheme, onSettings }: Props) {
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [recentError, setRecentError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<RecentSort>("opened");
  const [menu, setMenu] = useState(false);
  const [creating, setCreating] = useState(false);
  const [parent, setParent] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [layers, setLayers] = useState(2);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [lock, setLock] = useState<{ path: string; state: LockState } | null>(null);
  const [confirmRecover, setConfirmRecover] = useState(false);
  const session = loadSession();
  const searchRef = useRef<HTMLInputElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const load = () => {
    setLoaded(false);
    setRecentError(null);
    api
      .recentProjects(50)
      .then((r) => setRecent(Array.isArray(r) ? r : []))
      .catch((e) => setRecentError(String(e).replace(/^Error: /, "")))
      .finally(() => setLoaded(true));
  };
  useEffect(load, []);
  const shown = useMemo(() => sortRecent(filterRecent(recent, query), sort), [recent, query, sort]);
  const continueList = useMemo(() => sortRecent(recent, "opened").slice(0, 3), [recent]);

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
  // A refused open is explained from the typed lock state (never raw process ids); takeover is only
  // offered for an interrupted session and only after an explicit confirmation.
  const openPath = async (dir: string, takeOver = false) => {
    setLock(null);
    setConfirmRecover(false);
    setBusy(true);
    setError(null);
    try {
      onOpened(await api.openProject(dir, takeOver));
    } catch (e) {
      const message = String(e).replace(/^Error: /, "");
      const state = await api.projectLockState(dir).catch(() => null);
      if (state && state.state !== "unlocked") setLock({ path: dir, state });
      else setError(message);
    } finally {
      setBusy(false);
    }
  };
  const openFolder = async () => {
    const dir = await choose("Open an EAK project folder");
    if (dir) await openPath(dir);
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
  const startNew = () => {
    setCreating(true);
    setTimeout(() => nameRef.current?.focus(), 0);
  };

  // Mod+O opens a folder, Mod+N starts a new project, Mod+F or "/" searches the recent list.
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      const t = e.target;
      const typing = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;
      if (e.key === "Escape") setMenu(false);
      if (modalOpen()) return;
      if (matches(e, HOME_SHORTCUTS.openProject)) {
        e.preventDefault();
        if (!busy) void openFolder();
      } else if (matches(e, HOME_SHORTCUTS.newProject)) {
        e.preventDefault();
        startNew();
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
      <header className="home-bar">
        <div className="home-brand">
          <span className="home-mark" aria-hidden="true" />
          <strong className="home-wordmark">EAK</strong>
          <span className="muted home-descriptor">Electronics Agent Kit</span>
        </div>
        <nav className="home-nav" aria-label="Project navigation">
          <button type="button" className="ghost" disabled={busy} onClick={() => void openFolder()}>Open</button>
          <button type="button" className="ghost" onClick={startNew}>New</button>
          <a className="ghost-link" href="#home-recent">Recent</a>
        </nav>
        <div className="home-settings">
          <button type="button" className="ghost" aria-haspopup="menu" aria-expanded={menu} onClick={() => setMenu((m) => !m)}>
            Settings
          </button>
          {menu && (
            <div className="home-menu" role="menu">
              {onTheme && (
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    onTheme();
                    setMenu(false);
                  }}
                >
                  Theme: {theme === "dark" ? "Dark (switch to light)" : "Light (switch to dark)"}
                </button>
              )}
              {onSettings && (
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    onSettings();
                    setMenu(false);
                  }}
                >
                  Model and provider settings (optional)…
                </button>
              )}
            </div>
          )}
        </div>
      </header>

      <div className="home-inner">
        <section className="home-hero" aria-label="Start">
          <h1>Your engineering workspace.</h1>
          <p className="muted">
            Schematic, PCB, engineering checks and terminal agents on one project, with every change recorded and verified by the engine.
          </p>
          <div className="home-cta">
            <button type="button" className="primary" disabled={busy} onClick={() => void openFolder()} title={tip("Open an EAK project folder", HOME_SHORTCUTS.openProject)}>
              Open Project…
            </button>
            <button type="button" aria-expanded={creating} disabled={busy} onClick={startNew} title={tip("Create a project", HOME_SHORTCUTS.newProject)}>
              New Project
            </button>
            {session.length > 0 && (
              <button type="button" className="ghost" disabled={busy} onClick={reopenSession} title={session.join("\n")}>
                Reopen last session ({session.length})
              </button>
            )}
          </div>
        </section>

        {error && (
          <p role="alert" className="home-alert">
            {error}
          </p>
        )}

        {lock && (
          <LockNotice
            lock={lock}
            busy={busy}
            confirming={confirmRecover}
            onConfirm={() => setConfirmRecover(true)}
            onCancel={() => setConfirmRecover(false)}
            onRecover={() => void openPath(lock.path, true)}
            onRetry={() => void openPath(lock.path)}
          />
        )}

        {creating && (
          <form
            aria-label="Create project"
            className="home-new"
            onSubmit={(e) => {
              e.preventDefault();
              if (ready) void create();
            }}
          >
            <label>
              Name
              <input ref={nameRef} value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. motor-driver" />
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
            <div className="home-actions">
              <button type="submit" className="primary" disabled={busy || !ready} title={ready ? undefined : "Enter a name and choose a folder"}>
                Create
              </button>
              <button type="button" className="ghost" onClick={() => setCreating(false)}>Cancel</button>
              {ready && <span className="muted home-path">Creates <code>{projectFolder(parent!, name)}</code></span>}
            </div>
          </form>
        )}

        {continueList.length > 0 && (
          <section aria-label="Continue working" className="home-continue">
            <h2>Continue working</h2>
            <ul>
              {continueList.map((r) => {
                const v = validation(r.status);
                return (
                  <li key={r.path}>
                    <button type="button" className="home-continue-item" disabled={busy} title={`Open ${r.path}`} onClick={() => void openPath(r.path)}>
                      <strong>{r.name}</strong>
                      <span className="muted">{r.status ? r.status.phase : "Status unavailable"} · opened {ago(r.last_opened_ms)}</span>
                      <Pill tone={v.tone}>{v.text}</Pill>
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        )}

        <section aria-label="Recent projects" className="home-recent" id="home-recent">
          <header className="home-section-head">
            <h2>Recent projects</h2>
            {recent.length > 0 && <span className="count">{recent.length}</span>}
            <span className="spacer" />
            <select aria-label="Sort projects" value={sort} onChange={(e) => setSort(e.target.value as RecentSort)}>
              <option value="opened">Last opened</option>
              <option value="modified">Last modified</option>
              <option value="name">Name</option>
            </select>
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
                else if (e.key === "Enter" && shown.length > 0 && !busy) void openPath(shown[0].path);
                else rowNav(e);
              }}
            />
          </header>
          {!loaded && <p className="muted home-none" role="status">Loading recent projects…</p>}
          {loaded && recentError && (
            <EmptyState
              title="Recent projects could not be read."
              action={<button type="button" onClick={load}>Try again</button>}
            >
              {recentError} You can still open a project folder.
            </EmptyState>
          )}
          {loaded && !recentError && recent.length === 0 && (
            <EmptyState title="No recent projects.">
              Open an existing project folder or create a new project; it is listed here with its location, revision and when it was last opened.
            </EmptyState>
          )}
          {recent.length > 0 && shown.length === 0 && <p className="muted home-none">No project matches “{query}”.</p>}
          {shown.length > 0 && (
            <div className="home-table">
              <div className="home-cols" aria-hidden="true">
                <span>Project</span>
                <span>Stage</span>
                <span>Validation</span>
                <span className="num">Revision</span>
                <span className="num">Modified</span>
                <span className="num">Opened</span>
              </div>
              <ul className="recent" ref={listRef} onKeyDown={rowNav}>
                {shown.map((r) => {
                  const v = validation(r.status);
                  return (
                    <li key={r.path}>
                      <button type="button" className="home-row" disabled={busy} title={`Open ${r.path}`} onClick={() => void openPath(r.path)}>
                        <span className="home-id">
                          <strong className="home-name">{r.name}</strong>
                          <span className="home-loc">{r.path}</span>
                        </span>
                        <span>{r.status ? r.status.phase : <span className="muted">Unavailable</span>}</span>
                        <span title={r.status ? statusLine(r.status) : "The project folder moved or cannot be read"}>
                          <Pill tone={v.tone}>{v.text}</Pill>
                        </span>
                        <span className="num muted">{r.revision}</span>
                        <span className="num muted">{r.status?.last_modified_ms != null ? ago(r.status.last_modified_ms) : "—"}</span>
                        <span className="num muted">{ago(r.last_opened_ms)}</span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            </div>
          )}
        </section>
      </div>
    </main>
  );
}
