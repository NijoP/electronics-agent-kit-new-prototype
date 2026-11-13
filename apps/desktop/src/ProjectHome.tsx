// Project Home: recent projects (SQLite index in Rust), open and create.
import { useEffect, useState } from "react";
import { api } from "./api";
import type { RecentProject, Snapshot } from "./types";

interface Props {
  onOpened: (s: Snapshot) => void;
}

// shortcut: folders are typed as absolute paths; upgrade to a native folder picker
// (tauri-plugin-dialog) when Project Home gains file browsing.
export function ProjectHome({ onOpened }: Props) {
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [recentError, setRecentError] = useState<string | null>(null);
  const [openPath, setOpenPath] = useState("");
  const [takeOver, setTakeOver] = useState(false);
  const [createPath, setCreatePath] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

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
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="home">
      <h1>EAK</h1>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <section aria-label="Recent projects">
        <h2>Recent projects</h2>
        {recentError && <p className="error">{recentError}</p>}
        {!recentError && recent.length === 0 && <p className="muted">No recent projects.</p>}
        <ul className="recent">
          {recent.map((r) => (
            <li key={r.path}>
              <button type="button" disabled={busy} onClick={() => attempt(() => api.openProject(r.path, takeOver))}>
                <strong>{r.name}</strong> <span className="muted">revision {r.revision}</span>
                <br />
                <span className="muted">{r.path}</span>
              </button>
            </li>
          ))}
        </ul>
      </section>
      <form
        aria-label="Open project"
        onSubmit={(e) => {
          e.preventDefault();
          attempt(() => api.openProject(openPath, takeOver));
        }}
      >
        <h2>Open project</h2>
        <label>
          Project folder (absolute path)
          <input value={openPath} onChange={(e) => setOpenPath(e.target.value)} />
        </label>
        <label className="inline">
          <input type="checkbox" checked={takeOver} onChange={(e) => setTakeOver(e.target.checked)} />
          Take over a stale lock (only after a crash; never while the project is open elsewhere)
        </label>
        <button type="submit" disabled={busy}>
          Open
        </button>
      </form>
      <form
        aria-label="Create project"
        onSubmit={(e) => {
          e.preventDefault();
          attempt(() => api.createProject(createPath, name));
        }}
      >
        <h2>New project</h2>
        <label>
          New project folder (absolute path)
          <input value={createPath} onChange={(e) => setCreatePath(e.target.value)} />
        </label>
        <label>
          Name
          <input value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <button type="submit" disabled={busy}>
          Create
        </button>
      </form>
    </main>
  );
}
