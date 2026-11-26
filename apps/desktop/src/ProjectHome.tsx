// Project Home: the start screen (slice S1, built to the product vision's Home).
//
//   ┌ header: brand · Open · New · Recent ·························· Help · Settings ┐
//   │ [first-run tour, until finished or skipped; replayable from Help]              │
//   │ Start: describe the board → project + brief  │ Continue: the last project, with │
//   │ (no AI)  · New / Open / sample / Import       │ facts read from its folder and   │
//   │ Recent projects (search, sort)                │ Needs attention, each a jump     │
//   └────────────────────────────────────────────────────────────────────────────────┘
//
// Only real data is shown. A recent row carries what the recent-project index stores plus a
// read-only status read from the folder (`status`). The Continue cards read `home_project_facts`
// (src-tauri/src/home.rs): content counts, the evidence saved for the current revision, the
// engine's copper connectivity and the release gates. A check with no evidence for this revision
// says "not run"; nothing is inferred, estimated or called passing.
//
// Every start goes through the engine: New Project is `create_project`, the brief is one Batch of
// `AddRequirement` commands, the sample board is the reference design built by typed commands.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, pickFolder } from "./api";
import { formatShortcut, HOME_SHORTCUTS, matches, tip } from "./design/shortcuts";
import { EmptyState, Pill, type Tone } from "./design/ui";
import { attentionItems, briefFromPrompt, continueTarget, currentErrors, releaseLine, unknownRules, type Attention, type HomeFacts, type HomeTarget } from "./home/homeFacts";
import { KicadImport } from "./KicadImport";
import { Tour } from "./shell/onboarding/Tour";
import type { TourStep } from "./shell/onboarding/tourState";
import { modalOpen } from "./shell/viewState";
import type { LockState, ProjectStatus, RecentProject, Snapshot } from "./types";

interface Props {
  /** A project was opened or created; `landing` names the workspace place to show first. */
  onOpened: (s: Snapshot, landing?: HomeTarget) => void;
  /** False while another top-level workspace is shown: Home's shortcuts stay quiet. */
  active?: boolean;
  /** Theme and model settings live on Home's header (they used to float in a corner). */
  theme?: "dark" | "light";
  onTheme?: () => void;
  onSettings?: () => void;
  /** The first-run tour is shown in Home's page (first launch, or replayed from Help). */
  tourOpen?: boolean;
  onTourClose?: (how: "done" | "skipped") => void;
  onTourReplay?: () => void;
}

const SESSION_KEY = "eak.session.projects";
/** The last parent folder chosen for a new project, offered again next time. */
const PARENT_KEY = "eak.home.parent";

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
function loadParent(): string | null {
  try {
    return localStorage.getItem(PARENT_KEY);
  } catch {
    return null;
  }
}
function saveParent(dir: string): void {
  try {
    localStorage.setItem(PARENT_KEY, dir);
  } catch {
    /* storage unavailable */
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

/** "Good morning" / "Good afternoon" / "Good evening" for the local hour. */
export function greeting(hour: number): string {
  return hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
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
function validation(s: ProjectStatus | null | undefined): { tone: Tone; text: string } {
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

/** Example descriptions: each only fills the box; nothing is created until Start. */
const EXAMPLES: readonly { label: string; text: string }[] = [
  { label: "3.3 V LDO supply", text: "5 V input from a 2-pin header, 3.3 V rail from an LDO, at most 300 mA. A green LED shows the 3.3 V rail is on." },
  { label: "Sensor node", text: "Battery-powered temperature and humidity sensor on I2C. Runs from 2.0 to 3.6 V. Fits a 40 x 25 mm enclosure." },
  { label: "Motor driver", text: "12 V brushed DC motor driver, 2 A continuous, with reverse-polarity protection and a 4-pin control header." },
];

const ATTENTION_TONE: Record<Attention["tone"], Tone> = { bad: "bad", warn: "warn", unknown: "info", neutral: "neutral" };

/** One headline number on the Continue card; `null` shows a dash with the reason in `note`. */
function Stat({ label, value, note }: { label: string; value: number | string | null; note?: string }) {
  return (
    <div className="home-stat" title={note}>
      <span className="home-stat-n">{value ?? "—"}</span>
      <span className="home-stat-l">{label}</span>
    </div>
  );
}

function ContinueCard(p: { recent: RecentProject; facts: HomeFacts | null; error: string | null; busy: boolean; open: (target?: HomeTarget) => void }) {
  const { recent: r, facts: f } = p;
  const items = f ? attentionItems(f) : [];
  const errors = f ? currentErrors(f) : null;
  const target = continueTarget(f?.phase ?? r.status?.phase ?? "");
  const where = target.kind === "view" && target.view === "pcb" ? "PCB" : "Schematic";
  return (
    <section aria-label="Continue" className="home-card home-continue-card">
      <div className="home-card-head">
        <div className="home-card-title">
          <span className="home-label-caps">Continue</span>
          <strong className="home-card-name">{f?.name ?? r.name}</strong>
          <span className="muted">
            {f ? `${f.phase} · r${f.revision}` : r.status ? `${r.status.phase} · r${r.revision}` : "Status unavailable"} · opened {ago(r.last_opened_ms)}
          </span>
        </div>
        <button type="button" className="primary" disabled={p.busy} title={`Open ${r.path} in the ${where.toLowerCase()} editor`} onClick={() => p.open(target)}>
          Continue · {where}
        </button>
      </div>
      {p.error && (
        <p className="muted home-note" role="status">
          Facts unavailable: {p.error}
        </p>
      )}
      {!f && !p.error && (
        <p className="muted home-note" role="status">
          Reading the project folder…
        </p>
      )}
      {f && (
        <>
          <div className="home-stats" aria-label="Project facts">
            <Stat label="Errors" value={errors} note={errors === null ? `No check has a saved report for r${f.revision}` : `Errors in the reports saved for r${f.revision}`} />
            <Stat label="Unknown rules" value={f.checks.length ? unknownRules(f) : null} note="Rules a check could not evaluate; never a pass" />
            <Stat label="Unrouted nets" value={f.design.footprints ? f.unrouted_nets : null} note={f.design.footprints ? `${f.unrouted_connections} connections left` : "No footprints on the board yet"} />
            <Stat label="BOM lines" value={f.design.bom_lines} />
            <Stat label="Release gates" value={`${f.release.passing}/${f.release.required}`} note={releaseLine(f)} />
          </div>
          <p className="muted home-note">
            {releaseLine(f)}
            {f.checks.length === 0 ? ` · no check has a saved report for r${f.revision}` : ` · current reports: ${f.checks.map((c) => c.check).join(", ")}`}
          </p>
          <div className="home-attention">
            <span className="home-label-caps">Needs attention</span>
            {items.length === 0 ? (
              <p className="muted home-note">Nothing open in the facts EAK reads from this folder.</p>
            ) : (
              <ul aria-label="Needs attention">
                {items.slice(0, 6).map((a) => (
                  <li key={a.id}>
                    <button type="button" className="home-att" data-tone={a.tone} disabled={p.busy} title={`${a.detail} Click: ${a.goes}.`} onClick={() => p.open(a.target)}>
                      <span className={`home-att-dot tone-${ATTENTION_TONE[a.tone]}`} aria-hidden="true" />
                      <span className="home-att-text">
                        <strong>{a.title}</strong>
                        <span className="muted">{a.detail}</span>
                      </span>
                      <span className="muted home-att-go" aria-hidden="true">›</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </>
      )}
    </section>
  );
}

function SmallContinue(p: { recent: RecentProject; facts: HomeFacts | null; busy: boolean; open: () => void }) {
  const v = validation(p.recent.status);
  const n = p.facts ? attentionItems(p.facts).filter((a) => a.tone === "bad" || a.tone === "warn").length : null;
  return (
    <li>
      <button type="button" className="home-continue-item" disabled={p.busy} title={`Open ${p.recent.path}`} onClick={p.open}>
        <strong>{p.recent.name}</strong>
        <span className="muted">
          {p.facts ? `${p.facts.phase} · r${p.facts.revision}` : p.recent.status ? p.recent.status.phase : "Status unavailable"} · opened {ago(p.recent.last_opened_ms)}
        </span>
        <span className="home-actions">
          <Pill tone={v.tone}>{v.text}</Pill>
          {n !== null && n > 0 && <Pill tone="warn">{n} to look at</Pill>}
        </span>
      </button>
    </li>
  );
}

export function ProjectHome({ onOpened, active = true, theme, onTheme, onSettings, tourOpen = false, onTourClose, onTourReplay }: Props) {
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [recentError, setRecentError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<RecentSort>("opened");
  const [menu, setMenu] = useState<null | "settings" | "help" | "import">(null);
  const [creating, setCreating] = useState(false);
  const [parent, setParentState] = useState<string | null>(loadParent);
  const [name, setName] = useState("");
  const [layers, setLayers] = useState(2);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | false>(false);
  const [lock, setLock] = useState<{ path: string; state: LockState; landing?: HomeTarget } | null>(null);
  const [confirmRecover, setConfirmRecover] = useState(false);
  const [prompt, setPrompt] = useState("");
  const [brief, setBrief] = useState<{ name: string; lines: string[] } | null>(null);
  const [importing, setImporting] = useState(false);
  const [facts, setFacts] = useState<Record<string, { facts: HomeFacts | null; error: string | null }>>({});
  const [tourTarget, setTourTarget] = useState<string | undefined>(undefined);
  const session = loadSession();
  const searchRef = useRef<HTMLInputElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const briefNameRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const setParent = (dir: string) => {
    saveParent(dir);
    setParentState(dir);
  };
  // The default projects folder (<Documents>/EAK Projects) when no folder was chosen before.
  // It only computes a path; the engine creates the folder with the first project.
  useEffect(() => {
    if (parent) return;
    let live = true;
    api
      .homeDefaultDir()
      .then((d) => {
        if (live && typeof d === "string" && d) setParentState((p) => p ?? d);
      })
      .catch(() => {
        /* no default: the folder dialog is the only way to choose */
      });
    return () => {
      live = false;
    };
  }, []);

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
  const continuePaths = continueList.map((r) => r.path).join("\n");

  // Facts of the Continue projects, read from their folders (one read-only IPC call each).
  useEffect(() => {
    let live = true;
    for (const r of continueList) {
      api
        .homeProjectFacts(r.path)
        .then((f) => {
          if (live) setFacts((old) => ({ ...old, [r.path]: f && typeof f === "object" ? { facts: f, error: null } : { facts: null, error: "no facts returned" } }));
        })
        .catch((e) => {
          if (live) setFacts((old) => ({ ...old, [r.path]: { facts: null, error: String(e).replace(/^Error: /, "") } }));
        });
    }
    return () => {
      live = false;
    };
  }, [continuePaths]);

  const attempt = async (label: string, f: () => Promise<Snapshot>, landing?: HomeTarget) => {
    setBusy(label);
    setError(null);
    try {
      onOpened(await f(), landing);
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
  const openPath = async (dir: string, takeOver = false, landing?: HomeTarget) => {
    setLock(null);
    setConfirmRecover(false);
    setBusy("Opening the project…");
    setError(null);
    try {
      onOpened(await api.openProject(dir, takeOver), landing);
    } catch (e) {
      const message = String(e).replace(/^Error: /, "");
      const state = await api.projectLockState(dir).catch(() => null);
      if (state && state.state !== "unlocked") setLock({ path: dir, state, landing });
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
    attempt("Creating the project…", async () => {
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
    attempt("Reopening the last session…", async () => {
      let last: Snapshot | null = null;
      for (const dir of session) last = await api.openProject(dir, false);
      if (!last) throw new Error("Nothing to reopen");
      return last;
    });
  const ready = !!parent && name.trim().length > 0;
  const startNew = () => {
    setBrief(null);
    setCreating(true);
    setTimeout(() => nameRef.current?.focus(), 0);
  };
  // Start: the typed description becomes a brief to review; nothing is created until confirmed.
  const startBrief = () => {
    const b = briefFromPrompt(prompt);
    if (b.lines.length === 0) {
      setError("Describe the board first: what it does, its supply and its interfaces.");
      return;
    }
    setError(null);
    setCreating(false);
    setBrief(b);
    setTimeout(() => briefNameRef.current?.focus(), 0);
  };
  const createBrief = () =>
    brief &&
    attempt(
      "Creating the project and its brief…",
      async () => {
        const dir = await api.homeSuggestFolder(parent!, brief.name);
        return api.createProjectWithBrief(dir, brief.name.trim(), brief.lines);
      },
      { kind: "overview" },
    );
  const openSample = async () => {
    let where = parent;
    if (!where) {
      where = await choose("Choose where the sample board folder is created");
      if (!where) return;
      setParent(where);
    }
    const base = where;
    await attempt("Building the sample board through the engine…", async () => api.createSampleProject(await api.homeSuggestFolder(base, "Sample board")), { kind: "view", view: "pcb" });
  };
  const closeMenus = () => setMenu(null);
  const onTourStep = useCallback((s: TourStep) => setTourTarget(s.target), []);
  const tourOn = (id: string) => (tourOpen && tourTarget === id ? "" : undefined);

  // Mod+O opens a folder, Mod+N starts a new project, Mod+F or "/" searches the recent list.
  useEffect(() => {
    if (!active) return;
    const onKey = (e: KeyboardEvent) => {
      const t = e.target;
      const typing = t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;
      if (e.key === "Escape") setMenu(null);
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

  const isBusy = busy !== false;
  const last = continueList[0];
  const lastFacts = last ? facts[last.path] : undefined;
  const headline = last ? `${last.name} is at ${(lastFacts?.facts?.phase ?? last.status?.phase ?? "its last stage").toLowerCase()}.` : "Your engineering workspace.";
  const location = (
    <div className="home-field">
      <span className="home-label">Location</span>
      <div className="home-actions">
        <button type="button" disabled={isBusy} onClick={chooseParent}>
          Choose parent folder…
        </button>
        {/* Typed or pasted as well (as in File › New project): the native chooser is not always
            usable (no portal, a remote session), and the backend checks the path. */}
        <input
          aria-label="Parent folder"
          className="home-path"
          value={parent ?? ""}
          placeholder="No folder chosen: choose one or type an absolute path"
          onChange={(e) => setParentState(e.target.value.trim() ? e.target.value : null)}
        />
      </div>
    </div>
  );

  return (
    <main className="home">
      <header className="home-bar">
        <div className="home-brand">
          <span className="home-mark" aria-hidden="true" />
          <strong className="home-wordmark">EAK</strong>
          <span className="muted home-descriptor">Electronics Agent Kit</span>
        </div>
        <nav className="home-nav" aria-label="Project navigation">
          <button type="button" className="ghost" disabled={isBusy} onClick={() => void openFolder()}>Open</button>
          <button type="button" className="ghost" onClick={startNew}>New</button>
          <a className="ghost-link" href="#home-recent">Recent</a>
        </nav>
        <div className="home-settings">
          <button type="button" className="ghost" aria-haspopup="menu" aria-expanded={menu === "help"} onClick={() => setMenu((m) => (m === "help" ? null : "help"))}>
            Help
          </button>
          {menu === "help" && (
            <div className="home-menu" role="menu" aria-label="Help">
              {onTourReplay && (
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    onTourReplay();
                    closeMenus();
                  }}
                >
                  Replay first-run tour
                </button>
              )}
            </div>
          )}
        </div>
        <div className="home-settings">
          <button type="button" className="ghost" aria-haspopup="menu" aria-expanded={menu === "settings"} onClick={() => setMenu((m) => (m === "settings" ? null : "settings"))}>
            Settings
          </button>
          {menu === "settings" && (
            <div className="home-menu" role="menu" aria-label="Settings">
              {onTheme && (
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    onTheme();
                    closeMenus();
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
                    closeMenus();
                  }}
                >
                  Model and provider settings (optional)…
                </button>
              )}
            </div>
          )}
        </div>
      </header>

      <div className="home-inner home-grid">
        {tourOpen && onTourClose && (
          <div className="home-tour-slot">
            <Tour onClose={onTourClose} onStep={onTourStep} />
          </div>
        )}

        <div className="home-col home-left">
          <section className="home-hero" aria-label="Start" data-tour="prompt" data-tour-on={tourOn("prompt")}>
            <span className="home-label-caps">{greeting(new Date().getHours())}</span>
            <h1>{headline}</h1>
            <form
              aria-label="Describe a board"
              className="home-intent"
              onSubmit={(e) => {
                e.preventDefault();
                startBrief();
              }}
            >
              <textarea
                aria-label="Describe the board"
                rows={2}
                value={prompt}
                placeholder="Describe the board you want to build: what it does, its supply and its interfaces…"
                onChange={(e) => setPrompt(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                    e.preventDefault();
                    startBrief();
                  }
                }}
              />
              <button type="submit" className="primary" disabled={isBusy} title="Review the brief, then create the project. No AI is used.">
                Start
              </button>
            </form>
            <div className="home-chips" aria-label="Examples">
              {EXAMPLES.map((x) => (
                <button key={x.label} type="button" className="chip-btn" title={`Fill the box with: ${x.text}`} onClick={() => setPrompt(x.text)}>
                  {x.label}
                </button>
              ))}
            </div>
          </section>

          {busy && (
            <p className="muted home-note" role="status">
              {busy}
            </p>
          )}
          {error && (
            <p role="alert" className="home-alert">
              {error}
            </p>
          )}

          {lock && (
            <LockNotice
              lock={lock}
              busy={isBusy}
              confirming={confirmRecover}
              onConfirm={() => setConfirmRecover(true)}
              onCancel={() => setConfirmRecover(false)}
              onRecover={() => void openPath(lock.path, true, lock.landing)}
              onRetry={() => void openPath(lock.path, false, lock.landing)}
            />
          )}

          {brief && (
            <form
              aria-label="Create from brief"
              className="home-new"
              onSubmit={(e) => {
                e.preventDefault();
                if (parent && brief.name.trim()) void createBrief();
              }}
            >
              <label>
                Project name
                <input ref={briefNameRef} value={brief.name} onChange={(e) => setBrief({ ...brief, name: e.target.value })} />
              </label>
              {location}
              <div className="home-field">
                <span className="home-label">Design brief: your text, one requirement per line, recorded as written (no AI)</span>
                <ol className="home-brief" aria-label="Brief lines">
                  {brief.lines.map((l, i) => (
                    <li key={i}>{l}</li>
                  ))}
                </ol>
              </div>
              <div className="home-actions">
                <button type="submit" className="primary" disabled={isBusy || !parent || !brief.name.trim()} title={parent ? undefined : "Choose a folder first"}>
                  Create project and brief
                </button>
                <button type="button" className="ghost" onClick={() => setBrief(null)}>Cancel</button>
              </div>
            </form>
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
              {location}
              <label>
                Copper layers
                <select value={layers} onChange={(e) => setLayers(Number(e.target.value))}>
                  {[2, 4, 6].map((n) => (
                    <option key={n} value={n}>{n}</option>
                  ))}
                </select>
              </label>
              <div className="home-actions">
                <button type="submit" className="primary" disabled={isBusy || !ready} title={ready ? undefined : "Enter a name and choose a folder"}>
                  Create
                </button>
                <button type="button" className="ghost" onClick={() => setCreating(false)}>Cancel</button>
                {ready && <span className="muted home-path">Creates <code>{projectFolder(parent!, name)}</code></span>}
              </div>
            </form>
          )}

          {importing && (
            <section aria-label="Import" className="home-new home-import">
              <div className="home-section-head">
                <h2>Import from KiCad</h2>
                <span className="spacer" />
                <button type="button" className="ghost" onClick={() => setImporting(false)}>Close</button>
              </div>
              <p className="muted home-note">Symbols and footprints go into your own library (outside any project), with the licence and source you name.</p>
              <KicadImport />
            </section>
          )}

          <section aria-label="Projects" className="home-projects" data-tour="projects" data-tour-on={tourOn("projects")}>
            <div className="home-cta">
              <button type="button" className="primary" disabled={isBusy} onClick={() => void openFolder()} title={tip("Open an EAK project folder", HOME_SHORTCUTS.openProject)}>
                Open Project…
              </button>
              <button type="button" aria-expanded={creating} disabled={isBusy} onClick={startNew} title={tip("Create a project", HOME_SHORTCUTS.newProject)}>
                New Project
              </button>
              <button type="button" disabled={isBusy} onClick={() => void openSample()} title="Build the reference board (5 V header, AMS1117 LDO, ATtiny85, LED) in a new folder through the engine's typed commands, then open it">
                Open sample board
              </button>
              <div className="home-settings">
                <button type="button" aria-haspopup="menu" aria-expanded={menu === "import"} disabled={isBusy} onClick={() => setMenu((m) => (m === "import" ? null : "import"))}>
                  Import…
                </button>
                {menu === "import" && (
                  <div className="home-menu" role="menu" aria-label="Import">
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        setImporting(true);
                        closeMenus();
                      }}
                    >
                      KiCad symbols or footprints (.kicad_sym, .kicad_mod)…
                    </button>
                    <button type="button" role="menuitem" disabled title="Not available yet: EAK imports KiCad symbols and footprints into your library; importing a KiCad board or project is not built">
                      KiCad board or project (not available yet)
                    </button>
                  </div>
                )}
              </div>
              {session.length > 0 && (
                <button type="button" className="ghost" disabled={isBusy} onClick={reopenSession} title={session.join("\n")}>
                  Reopen last session ({session.length})
                </button>
              )}
            </div>

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
                    else if (e.key === "Enter" && shown.length > 0 && !isBusy) void openPath(shown[0].path);
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
                  Describe a board above, create a new project, or open the sample board; it is listed here with its location, revision and when it was last opened.
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
                          <button type="button" className="home-row" disabled={isBusy} title={`Open ${r.path}`} onClick={() => void openPath(r.path)}>
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
          </section>
        </div>

        <div className="home-col home-right" data-tour="continue" data-tour-on={tourOn("continue")}>
          {last ? (
            <>
              <ContinueCard
                recent={last}
                facts={lastFacts?.facts ?? null}
                error={lastFacts?.error ?? null}
                busy={isBusy}
                open={(target) => void openPath(last.path, false, target)}
              />
              {continueList.length > 1 && (
                <section aria-label="Also recent" className="home-continue">
                  <h2>Also recent</h2>
                  <ul>
                    {continueList.slice(1).map((r) => (
                      <SmallContinue key={r.path} recent={r} facts={facts[r.path]?.facts ?? null} busy={isBusy} open={() => void openPath(r.path, false, continueTarget(facts[r.path]?.facts?.phase ?? r.status?.phase ?? ""))} />
                    ))}
                  </ul>
                </section>
              )}
            </>
          ) : (
            <section aria-label="Continue" className="home-card">
              <span className="home-label-caps">Continue</span>
              <p className="muted home-note">
                {loaded
                  ? "Nothing to continue yet. A project you open or create appears here with its check results on the current revision, unrouted nets, BOM risk and release gates, read from its folder."
                  : "Reading recent projects…"}
              </p>
            </section>
          )}
        </div>
      </div>
    </main>
  );
}
