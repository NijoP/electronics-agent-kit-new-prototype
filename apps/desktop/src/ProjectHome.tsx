// Project Home: pick a project with the operating system's folder dialogs (no pasted paths),
// create one (name, parent folder, layer count) or reopen from the recent list / last session.
// The project that opens becomes the shared context for the EDA and Terminal workspaces.
import { useEffect, useState } from "react";
import { api, pickFolder } from "./api";
import type { RecentProject, Snapshot } from "./types";

interface Props {
  onOpened: (s: Snapshot) => void;
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

export function ProjectHome({ onOpened }: Props) {
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [recentError, setRecentError] = useState<string | null>(null);
  const [takeOver, setTakeOver] = useState(false);
  const [parent, setParent] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [layers, setLayers] = useState(2);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const session = loadSession();

  useEffect(() => {
    api
      .recentProjects(20)
      .then(setRecent)
      .catch((e) => setRecentError(String(e)));
  }, []);

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

  return (
    <main className="home">
      <h1>EAK</h1>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <section aria-label="Start">
        <h2>Open a project</h2>
        <div className="home-actions">
          <button type="button" className="primary" disabled={busy} onClick={openFolder}>
            Open project folder…
          </button>
          {session.length > 0 && (
            <button type="button" disabled={busy} onClick={reopenSession} title={session.join("\n")}>
              Reopen last session ({session.length} project{session.length > 1 ? "s" : ""})
            </button>
          )}
        </div>
        <label className="inline">
          <input type="checkbox" checked={takeOver} onChange={(e) => setTakeOver(e.target.checked)} />
          Take over a stale lock (only after a crash; never while the project is open elsewhere)
        </label>
      </section>
      <section aria-label="Recent projects">
        <h2>Recent projects</h2>
        {recentError && <p className="error">{recentError}</p>}
        {!recentError && recent.length === 0 && <p className="muted">No recent projects.</p>}
        <ul className="recent">
          {recent.map((r) => (
            <li key={r.path}>
              <button type="button" disabled={busy} onClick={() => attempt(() => api.openProject(r.path, takeOver))}>
                <strong>{r.name}</strong> <span className="muted">revision {r.revision} · opened {ago(r.last_opened_ms)}</span>
                <br />
                <span className="muted">{r.path}</span>
              </button>
            </li>
          ))}
        </ul>
      </section>
      <form
        aria-label="Create project"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready) void create();
        }}
      >
        <h2>New project</h2>
        <label>
          Name
          <input value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <div className="home-actions">
          <button type="button" disabled={busy} onClick={chooseParent}>
            Choose parent folder…
          </button>
          <span className="muted">{parent ?? "No folder chosen"}</span>
        </div>
        <label>
          Copper layers
          <select value={layers} onChange={(e) => setLayers(Number(e.target.value))}>
            {[2, 4, 6].map((n) => (
              <option key={n} value={n}>{n}</option>
            ))}
          </select>
        </label>
        {ready && <p className="muted">Creates and opens <code>{projectFolder(parent!, name)}</code></p>}
        <button type="submit" disabled={busy || !ready} title={ready ? undefined : "Enter a name and choose a parent folder"}>
          Create
        </button>
      </form>
    </main>
  );
}
