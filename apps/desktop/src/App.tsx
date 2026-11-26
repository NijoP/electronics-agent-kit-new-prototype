// Application shell (product vision §1): the top bar, the navigation rail and the workspace hosts.
//
//   EAK  Electronics Agent Kit  /  <project>                                         [ + ]
//   rail │ Home  ·  EDA workspace (schematic, PCB, 3D and their docks)  ·  Terminal Workspace
//
// The rail lists the areas: Home, Schematic, PCB, 3D, Library, Agents, Checks, Manufacturing and
// Terminal. While the EDA workspace is shown it draws the rail itself, from its live document and
// dock state (shell/ActivityBar.tsx). Beside Home and the registered top-level workspaces (the
// Terminal Workspace, shell/registry.ts) the shell draws the same rail (shell/NavRail.tsx); an EDA
// area picked there switches back to the workspace and asks it to show that area. Every workspace
// that has been opened stays mounted while another is shown, so switching never kills a terminal
// process or loses EDA editor state. The "+" appears only when a registered workspace provides a
// "new space" action; nothing is a placeholder.
import { useCallback, useEffect, useState } from "react";
import { api } from "./api";
import { matches, SHORTCUTS } from "./design/shortcuts";
import { applyTheme, loadTheme, type Theme } from "./design/theme";
import { ModelSettings } from "./ModelSettings";
import type { HomeTarget } from "./home/homeFacts";
import { ProjectHome, saveSession } from "./ProjectHome";
import { AGENTS_TITLE, HOME_TITLE, type ShellArea } from "./shell/ActivityBar";
import { Icons } from "./shell/icons";
import { NavRail, type NavItem } from "./shell/NavRail";
import { Tour } from "./shell/onboarding/Tour";
import { setTourState, tourState } from "./shell/onboarding/tourState";
import { listWorkspaces, useRegistryVersion, type WorkspaceContext } from "./shell/registry";
import { TopBar } from "./shell/TopBar";
import { modalOpen } from "./shell/viewState";
import "./shell/workspaces";
import type { OpenProject, Snapshot } from "./types";
import { Workspace } from "./Workspace";

/** Why the EDA areas of the shell's rail are disabled while no project is open. */
const NO_PROJECT = "Open or create a project first";

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
  // "eda" (the project workspace, or Home while nothing is open), "home", or a registered app id.
  const [current, setCurrent] = useState("eda");
  const [visited, setVisited] = useState<ReadonlySet<string>>(new Set());
  const [projects, setProjects] = useState<OpenProject[]>([]);
  const [navRequest, setNavRequest] = useState<{ area: string; seq: number } | null>(null);
  useRegistryVersion();
  const apps = listWorkspaces("app");
  const edaViews = listWorkspaces("eda");
  const active = current === "home" || apps.some((a) => a.id === current) ? current : "eda";
  const homeShown = active === "home" || (!opened && active === "eda");
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
    if (id !== "eda" && id !== "home") setVisited((v) => new Set(v).add(id));
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
  // Opening a project from Home shows it in the EDA workspace.
  const openedFromHome = (s: Snapshot, to?: HomeTarget) => {
    setOpened(s, to);
    setCurrent("eda");
  };
  // An EDA area picked in the shell's rail: show the workspace, which then shows the area.
  const toEda = (area: string) => {
    show("eda");
    setNavRequest((r) => ({ area, seq: (r?.seq ?? 0) + 1 }));
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

  // Rail entries the shell owns (Home and the top-level workspaces), shared with the EDA rail.
  const homeArea: ShellArea = { id: "home", label: "Home", icon: Icons.home, title: HOME_TITLE, run: () => show("home") };
  const appAreas: ShellArea[] = apps.map((a) => ({ id: a.id, label: a.navLabel ?? a.label, icon: a.icon, title: a.label, shortcut: a.shortcut, run: () => show(a.id) }));
  // The rail beside Home and the Terminal Workspace. EDA areas need an open project.
  const eda = (id: string, label: string, icon: NavItem["icon"], title: string): NavItem => ({ id, label, icon, title, run: () => toEda(id), unavailable: opened ? undefined : NO_PROJECT });
  const railAreas: NavItem[] = [
    { ...homeArea, current: homeShown },
    eda("schematic", "Schematic", Icons.schematic, "Schematic workspace"),
    eda("pcb", "PCB", Icons.pcb, "PCB workspace"),
    ...edaViews.map((w) => eda(w.id, w.label, w.icon, `${w.label} workspace`)),
    eda("library", "Library", Icons.parts, "Library: parts, symbols and footprints"),
    { id: "agents", label: "Agents", icon: Icons.sparkle, title: AGENTS_TITLE, run: () => setSettings(true) },
    eda("checks", "Checks", Icons.checks, "Checks: ERC and DRC findings"),
    eda("manufacturing", "Manufacturing", Icons.manufacturing, "Manufacturing"),
    ...appAreas.map((a) => ({ ...a, current: active === a.id })),
  ];
  const railFooter: NavItem[] = [
    { id: "theme", label: theme === "dark" ? "Switch to light theme" : "Switch to dark theme", icon: theme === "dark" ? Icons.sun : Icons.moon, title: "Toggle light/dark theme", run: toggleTheme },
  ];
  const rail = <NavRail areas={railAreas} footer={railFooter} />;
  const activeProject = projects.find((p) => p.active);

  return (
    <div className="app">
      <TopBar
        project={opened ? { name: activeProject?.name ?? opened.project.name, dir: (activeProject?.dir ?? opened.dir) || null } : null}
        create={creator ? { label: creator.newLabel ?? `New ${creator.label} space`, shortcut: SHORTCUTS.newTerminalSpace, run: newSpace } : null}
      />
      <div className="app-main">
        <div className="ws-host" hidden={active !== "eda" || !opened}>
          {opened && (
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
              home={homeArea}
              apps={appAreas}
              navRequest={navRequest}
            />
          )}
        </div>
        {(!opened || active === "home") && (
          <div className="ws-host rail-host" hidden={!homeShown}>
            {rail}
            <div className="rail-content">
              <ProjectHome
                onOpened={openedFromHome}
                active={homeShown}
                theme={theme}
                onTheme={toggleTheme}
                onSettings={() => setSettings(true)}
                tourOpen={tour !== null}
                onTourClose={closeTour}
                onTourReplay={() => setTour("home")}
              />
            </div>
          </div>
        )}
        {apps
          .filter((a) => a.id === active || visited.has(a.id))
          .map((a) => (
            <div key={a.id} className="ws-host rail-host" hidden={active !== a.id}>
              {rail}
              <div className="rail-content">{a.render(ctx(active === a.id))}</div>
            </div>
          ))}
      </div>
      {tour === "floating" && opened && active === "eda" && <Tour floating onClose={closeTour} />}
      {settings && <ModelSettings onClose={() => setSettings(false)} />}
    </div>
  );
}
