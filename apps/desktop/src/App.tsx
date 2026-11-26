// Application shell: the app bar with the top-level workspaces and the workspace hosts.
//
//   EAK  [ Project / EDA ]  [ Terminal Workspace ]  ...        [ + ]
//
// "Project / EDA" is the electronics workspace (project, schematic, PCB, libraries, checks). Other
// top-level workspaces (the Terminal Workspace) register themselves in shell/registry.ts; each one
// fills the whole main area. Every workspace that has been opened stays mounted while another is
// shown, so switching never kills a terminal process or loses EDA editor state. The "+" appears
// only when a registered workspace provides a "new space" action; nothing is a placeholder.
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { formatShortcut, matches, SHORTCUTS, tip } from "./design/shortcuts";
import { applyTheme, loadTheme, type Theme } from "./design/theme";
import { ModelSettings } from "./ModelSettings";
import type { HomeTarget } from "./home/homeFacts";
import { ProjectHome, saveSession } from "./ProjectHome";
import { Tour } from "./shell/onboarding/Tour";
import { setTourState, tourState } from "./shell/onboarding/tour";
import { listWorkspaces, useRegistryVersion, type WorkspaceContext } from "./shell/registry";
import { modalOpen } from "./shell/viewState";
import "./shell/workspaces";
import type { OpenProject, Snapshot } from "./types";
import { Workspace } from "./Workspace";

export function App() {
  const [opened, setOpenedState] = useState<Snapshot | null>(null);
  // Where a project opened from Home lands first (a Needs attention item, Continue); used once.
  const [landing, setLanding] = useState<HomeTarget | null>(null);
  const setOpened = (s: Snapshot | null, to?: HomeTarget) => {
    setLanding(to ?? null);
    setOpenedState(s);
  };
  // First-run tour (slice S1): shown in Home at first launch until finished or skipped; replayed
  // from Help, in Home or (from the workspace Help menu) as a corner panel over the workspace.
  const [tour, setTour] = useState<null | "home" | "floating">(() => (tourState() === "unseen" ? "home" : null));
  const closeTour = (how: "done" | "skipped") => {
    setTourState(how);
    setTour(null);
  };
  const [settings, setSettings] = useState(false);
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const [current, setCurrent] = useState("eda");
  const [visited, setVisited] = useState<ReadonlySet<string>>(new Set());
  const [projects, setProjects] = useState<OpenProject[]>([]);
  useRegistryVersion();
  const apps = listWorkspaces("app");
  const active = apps.some((a) => a.id === current) ? current : "eda";
  const creator = apps.find((a) => a.onNew);
  useEffect(() => {
    applyTheme(theme);
  }, [theme]);
  const toggleTheme = () => setTheme((t) => (t === "dark" ? "light" : "dark"));

  const refreshProjects = useCallback(
    () =>
      api
        .openProjects()
        .then((p) => setProjects(Array.isArray(p) ? p : []))
        .catch(() => setProjects([])),
    [],
  );
  useEffect(() => {
    void refreshProjects();
  }, [opened?.project.id, refreshProjects]);

  // Remember which projects are open so Project Home can offer to reopen them next time.
  useEffect(() => {
    const dirs = projects.map((p) => p.dir).filter((d): d is string => !!d);
    if (dirs.length > 0) saveSession(dirs);
  }, [projects]);

  const show = (id: string) => {
    setCurrent(id);
    if (id !== "eda") setVisited((v) => new Set(v).add(id));
  };
  const newSpace = () => {
    if (!creator) return;
    creator.onNew?.();
    show(creator.id);
  };
  // After a project closes the engine may still hold another one: go back to it, else to Home.
  const onClosed = async () => {
    try {
      setOpened(await api.currentProject());
    } catch {
      setOpened(null);
    }
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (creator && !modalOpen() && matches(e, SHORTCUTS.newTerminalSpace)) {
        e.preventDefault();
        creator.onNew?.();
        show(creator.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const ctx = (isActive: boolean): WorkspaceContext => ({
    projectId: opened?.project.id ?? null,
    projectName: opened?.project.name ?? null,
    projectDir: opened?.dir ?? null,
    revision: opened?.project.revision ?? null,
    openProjects: projects,
    active: isActive,
  });

  return (
    <div className="app">
      <header className="appbar">
        <span className="brand">EAK</span>
        <div
          className="app-tabs"
          role="tablist"
          aria-label="Workspaces"
          onKeyDown={(e) => {
            // Arrow / Home / End move between workspaces (roving tabindex, automatic activation).
            const ids = ["eda", ...apps.map((a) => a.id)];
            const i = ids.indexOf(active);
            const next = e.key === "ArrowRight" ? i + 1 : e.key === "ArrowLeft" ? i - 1 : e.key === "Home" ? 0 : e.key === "End" ? ids.length - 1 : null;
            if (next === null) return;
            e.preventDefault();
            const id = ids[(next + ids.length) % ids.length];
            show(id);
            requestAnimationFrame(() => document.getElementById(`app-tab-${id}`)?.focus());
          }}
        >
        <button type="button" role="tab" id="app-tab-eda" tabIndex={active === "eda" ? 0 : -1} aria-selected={active === "eda"} className={active === "eda" ? "app-tab on" : "app-tab"} onClick={() => show("eda")} title="Electronics workspace: project, schematic, PCB">
          Project / EDA
        </button>
        {apps.map((a) => (
          <button key={a.id} type="button" role="tab" id={`app-tab-${a.id}`} tabIndex={active === a.id ? 0 : -1} aria-selected={active === a.id} className={active === a.id ? "app-tab on" : "app-tab"} onClick={() => show(a.id)} title={tip(a.label, a.shortcut)}>
            {a.label}
          </button>
        ))}
        </div>
        {creator && (
          <button type="button" className="app-new" aria-label={creator.newLabel ?? `New ${creator.label} space`} title={`${creator.newLabel ?? `New ${creator.label} space`} (${formatShortcut(SHORTCUTS.newTerminalSpace)})`} onClick={newSpace}>
            +
          </button>
        )}
        {opened && (
          <span className="appbar-project" title={(projects.find((p) => p.active)?.dir ?? opened.dir) || undefined}>
            {projects.find((p) => p.active)?.name ?? opened.project.name}
          </span>
        )}
      </header>
      <div className="app-main">
        <div className="ws-host" hidden={active !== "eda"}>
          {opened ? (
            <Workspace
              key={opened.project.id}
              initial={opened}
              visible={active === "eda"}
              onClosed={onClosed}
              onOpened={setOpened}
              landing={landing}
              onTour={() => setTour("floating")}
              onSettings={() => setSettings(true)}
              theme={theme}
              onTheme={toggleTheme}
              projects={projects}
              onProjectsChanged={() => void refreshProjects()}
            />
          ) : (
            <ProjectHome
              onOpened={setOpened}
              active={active === "eda"}
              theme={theme}
              onTheme={toggleTheme}
              onSettings={() => setSettings(true)}
              tourOpen={tour !== null}
              onTourClose={closeTour}
              onTourReplay={() => setTour("home")}
            />
          )}
        </div>
        {apps
          .filter((a) => a.id === active || visited.has(a.id))
          .map((a) => (
            <div key={a.id} className="ws-host" hidden={active !== a.id}>
              {a.render(ctx(active === a.id))}
            </div>
          ))}
      </div>
      {tour === "floating" && opened && active === "eda" && <Tour floating onClose={closeTour} />}
      {settings && <ModelSettings onClose={() => setSettings(false)} />}
    </div>
  );
}
