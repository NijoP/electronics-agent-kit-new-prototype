// New / Open project dialogs for the running workspace (File menu, toolbar). They use the same
// backend calls as Project Home. The backend replaces the open project when a new one is created
// or opened, and drops the old one *before* opening the new one: if that fails the workspace
// would show a project the backend no longer holds, so a failure checks the session and
// returns to Project Home when the old project is gone.
import { useEffect, useRef, useState } from "react";
import { api, pickFolder } from "../api";
import type { RecentProject, Snapshot } from "../types";

interface Common {
  onOpened: (s: Snapshot) => void;
  /** The backend no longer has an open project (a failed replace). */
  onLost: () => void;
  onClose: () => void;
}

async function attempt(f: () => Promise<Snapshot>, c: Common, setError: (e: string | null) => void, setBusy: (b: boolean) => void) {
  setBusy(true);
  setError(null);
  try {
    const snap = await f();
    c.onClose();
    c.onOpened(snap);
  } catch (e) {
    setError(String(e));
    try {
      await api.currentProject();
    } catch {
      c.onLost();
    }
  } finally {
    setBusy(false);
  }
}

function Shell({ title, onClose, children }: { title: string; onClose: () => void; children: React.ReactNode }) {
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={title} onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <div className="modal-head">
          <h2>{title}</h2>
          <button type="button" onClick={onClose}>Close</button>
        </div>
        {children}
      </div>
    </div>
  );
}

export function NewProjectDialog(c: Common) {
  const [path, setPath] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const first = useRef<HTMLInputElement>(null);
  useEffect(() => {
    first.current?.focus();
  }, []);
  return (
    <Shell title="New project" onClose={c.onClose}>
      <form
        aria-label="New project form"
        onSubmit={(e) => {
          e.preventDefault();
          void attempt(() => api.createProject(path, name), c, setError, setBusy);
        }}
      >
        <p className="muted hint">The current project is closed and the new one opens in its place. Every accepted command is saved as you go.</p>
        <label>
          Project folder (absolute path)
          <input ref={first} value={path} onChange={(e) => setPath(e.target.value)} />
          <button type="button" onClick={() => void pickFolder("Choose the new project folder").then((d) => d && setPath(d)).catch((e) => setError(String(e)))}>Browse…</button>
        </label>
        <label>
          Name
          <input value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        {error && <p role="alert" className="error">{error}</p>}
        <div className="row">
          <button type="submit" className="primary" disabled={busy || !path.trim() || !name.trim()}>Create</button>
          <button type="button" onClick={c.onClose}>Cancel</button>
        </div>
      </form>
    </Shell>
  );
}

export function OpenProjectDialog(c: Common & { recent: RecentProject[] }) {
  const [path, setPath] = useState("");
  const [takeOver, setTakeOver] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const open = (p: string) => void attempt(() => api.openProject(p, takeOver), c, setError, setBusy);
  return (
    <Shell title="Open project" onClose={c.onClose}>
      {c.recent.length > 0 && (
        <ul className="recent" aria-label="Recent projects">
          {c.recent.map((r) => (
            <li key={r.path}>
              <button type="button" disabled={busy} onClick={() => open(r.path)}>
                <strong>{r.name}</strong> <span className="muted">revision {r.revision}</span>
                <br />
                <span className="muted">{r.path}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
      <form
        aria-label="Open project form"
        onSubmit={(e) => {
          e.preventDefault();
          open(path);
        }}
      >
        <label>
          Project folder (absolute path)
          <input value={path} onChange={(e) => setPath(e.target.value)} />
          <button type="button" onClick={() => void pickFolder("Open an EAK project folder").then((d) => d && setPath(d)).catch((e) => setError(String(e)))}>Browse…</button>
        </label>
        <label className="inline">
          <input type="checkbox" checked={takeOver} onChange={(e) => setTakeOver(e.target.checked)} />
          Take over a stale lock (only after a crash; never while the project is open elsewhere)
        </label>
        {error && <p role="alert" className="error">{error}</p>}
        <div className="row">
          <button type="submit" className="primary" disabled={busy || !path.trim()}>Open</button>
          <button type="button" onClick={c.onClose}>Cancel</button>
        </div>
      </form>
    </Shell>
  );
}
