// Project workspace: toolbar, schematic + PCB views with cross-probing, Engineering
// Monitor, activity log, status bar and command palette. State shown here is always the
// latest engine snapshot; edits are engine calls carrying the rendered revision.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, describeError } from "./api";
import { DomainPanel } from "./DomainPanel";
import { ManufacturingPanel as FabricatorPrepPanel } from "./ManufacturingPanel";
import { PartsPanel as DistributorPartsPanel } from "./PartsPanel";
import { CommandPalette, type PaletteAction } from "./CommandPalette";
import type { EditCtx } from "./EditContext";
import { PcbView } from "./PcbView";
import { SchematicView } from "./SchematicView";
import { formatShortcut, matches, SHORTCUTS } from "./design/shortcuts";
import type { Theme } from "./design/theme";
import { ActivityBar, SIDE_VIEWS, type SideView } from "./shell/ActivityBar";
import { BottomPanel, type BottomTab } from "./shell/BottomPanel";
import { ManufacturingPanel, netNames, ProjectPanel } from "./shell/panels";
import { ProjectOverview } from "./shell/ProjectOverview";
import { LibraryPanel } from "./shell/LibraryPanel";
import { StatusBar } from "./shell/StatusBar";
import { ExportDialog } from "./shell/ExportDialog";
import { RulesDialog } from "./shell/RulesDialog";
import { NewProjectDialog, OpenProjectDialog } from "./shell/ProjectDialogs";
import { Toolbar, type ToolbarGroup } from "./shell/Toolbar";
import { canvasControls, DocIdContext, setActiveEditor, setEditorTool } from "./shell/viewState";
import { LayerManager } from "./shell/LayerManager";
import { useLayerView } from "./layerView";
import type { Focus } from "./pcbFocus";
import { findingKey, probeTarget } from "./EngineeringMonitor";
import { DocTabs } from "./shell/DocTabs";
import { FileExplorer } from "./shell/FileExplorer";
import { FileViewer } from "./shell/FileViewer";
import { ProjectTabs } from "./shell/ProjectTabs";
import { SyncReportDialog } from "./shell/SyncReportDialog";
import { buildSyncReport, summarizeSyncReport, type SyncReport } from "./shell/syncReport";
import { useTasks } from "./shell/tasks";
import { fileDocId, type FileEntry } from "./shell/files";
import { listWorkspaces, useRegistryVersion, type WorkspaceContext } from "./shell/registry";
import "./shell/workspaces";
import { filterToolbar } from "./shell/Toolbar";
import { availableDocs, closeDoc, fileDocs, loadDocState, openDoc, remapFileDocs, saveDocState, visibleDocs } from "./shell/documents";
import { clampSize, DEFAULT_LAYOUT, loadLayout, saveLayout, type Layout } from "./shell/layout";
import { MenuBar, type Menu } from "./shell/MenuBar";
import { Splitter } from "./shell/Splitter";
import { nextRotation } from "./editing";
import { acceptsSnapshot, exactChange, isStale, needsRefetch, onRevisionChange, reconcileSelection, type RevisionChange, type Shown } from "./revisionSync";
import { currentTask, taskSummary, taskTitle, useAgentTasks } from "./agentTasks";
import type { OpenProject, RecentProject, Check, CheckResult, Command, ConnectivityReport, Finding, CommandResponse, CopperLayer, DomainReport, EngineEvent, ExportResult, Id, LibraryEntry, Snapshot } from "./types";

interface Props {
  initial: Snapshot;
  /** False while another top-level workspace (the terminal) is shown: global shortcuts and editor keys are then off. */
  visible: boolean;
  onClosed: () => void;
  /** A different project was created or opened from inside the workspace. */
  onOpened: (s: Snapshot) => void;
  onSettings: () => void;
  theme: Theme;
  onTheme: () => void;
  /** Every project open in the window (the engine keeps them all; one is active). */
  projects: OpenProject[];
  onProjectsChanged: () => void;
}


const MAX_EVENTS = 200;

export function Workspace({ initial, visible, onClosed, onOpened, onSettings, theme, onTheme, projects, onProjectsChanged }: Props) {
  const [dialog, setDialog] = useState<null | "new" | "open" | "export" | "rules">(null);
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [snapshot, setSnapshot] = useState(initial);
  const [events, setEvents] = useState<EngineEvent[]>([]);
  const lastSeq = useRef(0);
  // What is on screen, and the engine's own account of the last external commit (src-tauri/src/sync.rs).
  const onScreen = useRef<Shown>({ projectId: initial.project.id, revision: initial.project.revision });
  const lastChange = useRef<RevisionChange | null>(null);
  const [results, setResults] = useState<Partial<Record<Check, CheckResult>>>({});
  const [selected, setSelected] = useState<Id | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [palette, setPalette] = useState(false);
  const [busy, setBusy] = useState(false);
  const [baseLibrary, setLibrary] = useState<LibraryEntry[]>([]);
  const [exported, setExported] = useState<ExportResult | null>(null);
  const [docState, setDocState] = useState(() => loadDocState(initial));
  // One view at a time by default: Schematic and PCB are separate documents; split is opt-in.
  const [split, setSplit] = useState(false);
  const [workspace, setWorkspace] = useState<string>(() => {
    try {
      return localStorage.getItem(`eak.workspace.${initial.project.id}`) ?? "editor";
    } catch {
      return "editor";
    }
  });
  const [visitedViews, setVisitedViews] = useState<ReadonlySet<string>>(new Set());
  const [syncDialog, setSyncDialog] = useState<null | { report: SyncReport | null; error: string | null; revision: number }>(null);
  const { tasks, track, clearFinished } = useTasks();
  useRegistryVersion();
  const edaViews = listWorkspaces("eda").map((w, i) => ({ ...w, shortcut: w.shortcut ?? (i < 7 ? `Mod+${3 + i}` : undefined) }));
  const wsId = workspace !== "editor" && edaViews.some((w) => w.id === workspace) ? workspace : "editor";
  useEffect(() => {
    try {
      localStorage.setItem(`eak.workspace.${initial.project.id}`, workspace);
    } catch {
      /* storage unavailable */
    }
  }, [initial.project.id, workspace]);
  const [layout, setLayout] = useState<Layout>(loadLayout);
  const [help, setHelp] = useState(false);
  const [conn, setConn] = useState<ConnectivityReport | null>(null);
  const [focus, setFocus] = useState<Focus | null>(null);
  const [activeFinding, setActiveFinding] = useState<string | null>(null);
  const layerView = useLayerView(initial.project.id);
  const flag = (k: "sideOpen" | "bottomOpen") => (f: boolean | ((o: boolean) => boolean)) =>
    setLayout((l) => ({ ...l, [k]: typeof f === "function" ? f(l[k]) : f }));
  const { sideOpen, bottomOpen } = layout;
  const setSideOpen = flag("sideOpen");
  const setBottomOpen = flag("bottomOpen");
  useEffect(() => saveLayout(layout), [layout]);
  const [sideView, setSideView] = useState<SideView>("project");
  const [bottomTab, setBottomTab] = useState<BottomTab>("log");
  const [quickOpen, setQuickOpen] = useState(false);
  const [armPart, setArmPart] = useState<{ id: string; n: number } | null>(null);
  const [status, setStatus] = useState<{ layer: CopperLayer; gridMm: number }>({ layer: "top", gridMm: 0.5 });
  const revision = snapshot.project.revision;
  const agentTasks = useAgentTasks(snapshot.project.id, revision);
  // An external commit may delete what is selected; otherwise selection (and camera) survive it.
  useEffect(() => {
    setSelected((s) => reconcileSelection(s, snapshot.project));
  }, [snapshot]);
  onScreen.current = { projectId: snapshot.project.id, revision };
  const rank = (k: string) => (k === "schematic" ? 0 : k === "pcb" ? 1 : 2);
  const wsCtx = (active: boolean): WorkspaceContext => ({
    projectId: snapshot.project.id,
    projectName: snapshot.project.name,
    projectDir: snapshot.dir,
    revision,
    openProjects: projects,
    active,
  });
  // Built-in parts plus parts imported into this project (e.g. from Distributor), so an imported
  // part can be placed from the part picker.
  const library = useMemo(() => {
    const known = new Set(baseLibrary.map((l) => l.id));
    const imported = Object.values(snapshot.project.library ?? {})
      .filter((d) => !known.has(d.id))
      .map<LibraryEntry>((d) => ({
        id: d.id,
        name: d.name,
        category: d.category,
        refdes_prefix: d.refdes_prefix,
        default_value: d.default_value,
        pins: d.symbol.pins.map((p) => [p.number, p.name]),
      }));
    return [...baseLibrary, ...imported];
  }, [baseLibrary, snapshot.project.library]);
  const docs = [...availableDocs(snapshot), ...fileDocs(docState.open)];
  // Only documents the engine still has stay open (the snapshot is authoritative).
  const liveIds = new Set(docs.map((d) => d.id));
  const openIds = docState.open.filter((id) => liveIds.has(id));
  const activeId = docState.active && liveIds.has(docState.active) ? docState.active : (openIds[0] ?? null);
  const effective = { open: openIds, active: activeId };
  useEffect(() => saveDocState(snapshot.project.id, effective), [snapshot.project.id, openIds.join("|"), activeId]);
  const shown = visibleDocs(effective, docs, split);
  // Editors listen on window: while this workspace (or its editor area) is hidden none may act.
  setActiveEditor(visible && wsId === "editor" ? activeId : "__hidden__");
  const openDocs = openIds.map((id) => docs.find((d) => d.id === id)!);
  const dirty = busy || !!snapshot.blocked;
  const [domain, setDomain] = useState<DomainReport | null>(null);
  const [domainError, setDomainError] = useState<string | null>(null);

  const pullEvents = useCallback(async () => {
    const fresh = await api.eventsSince(lastSeq.current);
    if (fresh.length === 0) return fresh;
    lastSeq.current = fresh[fresh.length - 1].seq;
    setEvents((old) => [...old, ...fresh].slice(-MAX_EVENTS));
    return fresh;
  }, []);

  const refresh = useCallback(async () => {
    setSnapshot(await api.currentProject());
    await pullEvents();
  }, [pullEvents]);

  useEffect(() => {
    api
      .libraryComponents()
      .then(setLibrary)
      .catch((e) => setMessage(`Library unavailable: ${String(e)}`));
  }, []);

  useEffect(() => {
    api
      .recentProjects(6)
      .then(setRecent)
      .catch(() => setRecent([]));
  }, [initial.project.id, dialog]);

  // Airwires come from the engine's connectivity analysis (the one DRC-004 uses), per revision.
  useEffect(() => {
    // An external commit that touched neither the schematic nor the PCB cannot change the airwires.
    if (!needsRefetch(lastChange.current, revision, ["schematic", "pcb"])) return;
    let live = true;
    api
      .connectivity()
      .then((r) => live && setConn(r))
      .catch(() => live && setConn(null));
    return () => {
      live = false;
    };
  }, [revision]);

  // Deterministic and local: re-run whenever the design revision changes.
  useEffect(() => {
    if (!needsRefetch(lastChange.current, revision, ["schematic", "library", "requirements", "constraints"])) return;
    let live = true;
    api
      .domainClassification()
      .then((r) => live && (setDomain(r), setDomainError(null)))
      .catch((e) => live && setDomainError(String(e)));
    return () => {
      live = false;
    };
  }, [revision]);

  useEffect(() => {
    pullEvents().catch((e) => setMessage(String(e)));
  }, [pullEvents]);

  // The backend announces every harness commit (docs/AGENT_HARNESS.md, "Live synchronisation"):
  // fetch the new snapshot once, and drop anything older than what is already on screen.
  useEffect(() => {
    let live = true;
    let off: (() => void) | undefined;
    onRevisionChange(async (ev) => {
      if (isStale(ev, onScreen.current)) return;
      const before = onScreen.current;
      try {
        const fresh = await api.currentProject();
        // A local commit may have landed while this was in flight: only a strictly newer snapshot wins.
        if (!live || !acceptsSnapshot({ projectId: fresh.project.id, revision: fresh.project.revision }, onScreen.current)) return;
        lastChange.current = exactChange(ev, before);
        setSnapshot(fresh);
        await pullEvents();
      } catch {
        /* the 1.5 s event poll below catches up */
      }
    })
      .then((un) => (live ? (off = un) : un()))
      .catch(() => undefined);
    return () => {
      live = false;
      off?.();
    };
  }, [pullEvents]);

  // Edits made by terminal agents through the harness endpoint run on this same engine; poll its
  // events and re-render when one moved the project past the revision on screen (ADR-0018).
  useEffect(() => {
    const timer = setInterval(() => {
      // Nothing to show while the window is hidden; the first visible tick catches up.
      if (typeof document !== "undefined" && document.hidden) return;
      pullEvents()
        .then(async (fresh) => {
          if (fresh.some((e) => e.revision !== revision)) setSnapshot(await api.currentProject());
        })
        .catch(() => undefined);
    }, 1500);
    return () => clearInterval(timer);
  }, [pullEvents, revision]);

  const guarded = useCallback(
    async (f: () => Promise<void>) => {
      if (busy) return;
      setBusy(true);
      try {
        await f();
      } catch (e) {
        setMessage(String(e).replace(/^Error: /, ""));
      } finally {
        setBusy(false);
      }
    },
    [busy],
  );

  const submit = (f: () => Promise<CommandResponse>) =>
    guarded(async () => {
      const r = await f();
      setMessage(
        r.accepted
          ? `${r.command} → revision ${r.revision_after}${r.warnings.length ? ` (${r.warnings.length} warning(s))` : ""}. ${r.validation_note}`
          : `${r.command} rejected: ${r.error ? describeError(r.error) : "unknown error"}`,
      );
      await refresh();
    });

  const execute = (command: Command) => submit(() => api.execute(revision, command));
  // Chain commands so each one carries the revision the previous one produced; stop at the
  // first rejection (the engine already left the project untouched for it).
  const executeAll = (commands: Command | Command[]) =>
    guarded(async () => {
      let rev = revision;
      let last: CommandResponse | null = null;
      for (const c of Array.isArray(commands) ? commands : [commands]) {
        last = await api.execute(rev, c);
        if (!last.accepted) break;
        rev = last.revision_after;
      }
      if (last)
        setMessage(
          last.accepted
            ? `${last.command} → revision ${last.revision_after}, saved`
            : `${last.command} rejected: ${last.error ? describeError(last.error) : "unknown error"}`,
        );
      await refresh();
    });
  const edit: EditCtx = { execute: executeAll, busy, library, notify: setMessage };
  // "Update PCB from schematic": the typed SyncSchematicToPcb command, then a report built from the
  // engine's own journalled change list for that operation (added / removed / modified components
  // and connections), so the report cannot disagree with what was applied.
  const syncPcb = () =>
    guarded(async () => {
      const pid = snapshot.project.id;
      await track(
        "Update PCB",
        async () => {
          // Gate (ADR-0036): the PCB is synchronised only from a schematic ERC has run on.
          if (results.erc?.report.revision !== revision) {
            const erc = await api.runCheck("erc");
            setResults((old) => ({ ...old, erc }));
          }
          const r = await api.execute(revision, { type: "SyncSchematicToPcb" });
          if (!r.accepted) {
            if (r.error?.code === "NothingTo") {
              const report = buildSyncReport([], snapshot.project.pcb);
              setSyncDialog({ report, error: null, revision });
              setMessage(summarizeSyncReport(report));
              return summarizeSyncReport(report);
            }
            const why = `Update PCB rejected: ${r.error ? describeError(r.error) : "unknown error"}`;
            setSyncDialog({ report: null, error: why, revision });
            throw new Error(why);
          }
          const fresh = await api.currentProject();
          setSnapshot(fresh);
          await pullEvents();
          try {
            const report = buildSyncReport(await api.operationChanges(pid, r.op_id), fresh.project.pcb);
            setSyncDialog({ report, error: null, revision: r.revision_after });
            setMessage(summarizeSyncReport(report));
            return summarizeSyncReport(report);
          } catch (e) {
            const why = `Update PCB applied (revision ${r.revision_after}), but the engine's change list could not be read: ${String(e)}`;
            setSyncDialog({ report: null, error: why, revision: r.revision_after });
            setMessage(why);
            return why;
          }
        },
        (d) => d,
      );
    });
  // Reopen a recent project: the backend drops the current one before opening the next, so a
  // failure that leaves no project open returns to Project Home instead of a stale workspace.
  const openRecent = (path: string) =>
    guarded(async () => {
      try {
        onOpened(await api.openProject(path, false));
      } catch (e) {
        setMessage(`Could not open ${path}: ${String(e)}`);
        try {
          await api.currentProject();
        } catch {
          onClosed();
        }
      }
    });
  const exportGerbers = () =>
    guarded(async () => {
      const r = await track("Export manufacturing files", () => api.exportManufacturing(), (x) => `${x.manifest.files.length} files, status ${x.manifest.status}`);
      setExported(r);
      setMessage(`Exported ${r.manifest.files.length} files to ${r.out_dir} (${r.manifest.status}, revision ${r.manifest.revision})`);
      await syncTo(r.manifest.revision);
    });
  // Checks and exports run on the engine's current revision. If a terminal agent moved it past
  // the one on screen, re-render so the editor shows exactly what was evaluated.
  const syncTo = async (evaluated: number) => {
    if (evaluated !== revision) await refresh();
    else await pullEvents();
  };
  const undo = () => submit(() => api.undo(revision));
  const redo = () => submit(() => api.redo(revision));
  const check = (c: Check) =>
    guarded(async () => {
      const result = await track(`Run ${c.toUpperCase()}`, () => api.runCheck(c), (r) => `${r.report.findings.length} finding(s) at revision ${r.report.revision}`);
      setResults((old) => ({ ...old, [c]: result }));
      setBottomTab("findings");
      setBottomOpen(true);
      setMessage(`${result.report.check} ran on revision ${result.report.revision}`);
      await syncTo(result.report.revision);
    });
  // Switch to or close one of the projects open in this window. Closing the active one activates
  // the previous one (or returns to Project Home when none is left).
  const switchTo = (id: string) =>
    guarded(async () => {
      onOpened(await api.switchProject(id));
    });
  const closeProjectTab = (id: string) =>
    guarded(async () => {
      const next = await api.closeProjectById(id);
      if (id !== snapshot.project.id) return onProjectsChanged();
      if (next) onOpened(next);
      else onClosed();
    });
  const close = () => closeProjectTab(snapshot.project.id);
  const acknowledge = () =>
    guarded(async () => {
      setSnapshot(await api.acknowledgeRecovery());
      await pullEvents();
    });

  const selectedSymbol = selected ? snapshot.project.schematic.symbols[selected] : undefined;
  const selLabel = selectedSymbol ? `${selectedSymbol.refdes} (${selectedSymbol.value})` : null;

  const showSide = (v: SideView) => {
    if (sideOpen && sideView === v) setSideOpen(false);
    else {
      setSideView(v);
      setSideOpen(true);
    }
  };

  // Net summary from the geometry the engine derived (no model involved).
  const showNet = (name: string) => {
    const pads = snapshot.geometry.footprints.reduce((n, f) => n + f.pads.filter((p) => p.net === name).length, 0);
    const tracks = Object.values(snapshot.project.pcb.tracks).filter((t) => t.net === name).length;
    setMessage(`Net ${name}: ${pads} pad(s) and ${tracks} track segment(s) on the PCB.`);
  };

  const open = (id: string) => {
    setWorkspace("editor");
    setDocState((st) => openDoc({ open: openIds, active: st.active }, id));
  };
  const showKind = (kind: "schematic" | "pcb") => {
    setWorkspace("editor");
    if (shown.some((d) => d.kind === kind)) return;
    const d = docs.find((x) => x.kind === kind);
    if (d) open(d.id);
  };
  // Activity-bar / shortcut navigation: "schematic" and "pcb" open that editor on its own;
  // anything else is a registered EDA view (e.g. 3D) that fills the editor area.
  const showWorkspace = (id: string) => {
    if (id === "schematic" || id === "pcb") {
      const active = docs.find((d) => d.id === activeId);
      if (wsId === "editor" && active?.kind === id) return;
      const same = openIds.map((i) => docs.find((d) => d.id === i)).find((d) => d?.kind === id) ?? docs.find((d) => d.kind === id);
      if (same) open(same.id);
      else setWorkspace("editor");
    } else {
      setWorkspace(id);
      setVisitedViews((v) => new Set(v).add(id));
    }
  };
  const openFile = (entry: FileEntry) => open(fileDocId(entry.rel));
  const fileRemoved = (rel: string, isDir: boolean) =>
    setDocState((st) => remapFileDocs({ open: openIds, active: st.active }, (r) => (r === rel || (isDir && r.startsWith(`${rel}/`)) ? null : r)));
  const fileRenamed = (from: string, to: string) =>
    setDocState((st) => remapFileDocs({ open: openIds, active: st.active }, (r) => (r === from ? to : r.startsWith(`${from}/`) ? to + r.slice(from.length) : r)));
  const closeOne = (id: string) => {
    const d = docs.find((x) => x.id === id);
    // The engine writes every accepted command before it returns, so "unsaved" only means a
    // command is in flight or writes are blocked; closing then needs an explicit yes.
    if (dirty && !window.confirm(`${d?.title ?? "Document"} has changes the engine has not confirmed (command in flight or writes blocked). Close anyway?`)) return;
    setDocState(closeDoc({ open: openIds, active: activeId }, id));
  };
  const closeAll = () => {
    if (dirty && !window.confirm("The engine has not confirmed the latest change. Close all documents anyway?")) return;
    setDocState({ open: [], active: null });
  };
  const resetLayout = () => setLayout(DEFAULT_LAYOUT);
  // Click a finding: open the right editor, select the object, zoom to it, highlight it.
  // DRC and DFM findings live in the PCB editor (zoom + highlight); ERC findings select the symbol in
  // the schematic (no zoom there yet).
  const navigateFinding = (f: Finding, checkName: Check) => {
    const kind = checkName === "erc" ? "schematic" : "pcb";
    const d = docs.find((x) => x.kind === kind);
    if (d) open(d.id);
    const target = probeTarget(snapshot, f.objects);
    if (target) setSelected(target);
    setActiveFinding(findingKey(f));
    if (kind === "pcb") setFocus({ n: Date.now(), objects: f.objects, location: f.location });
    else setFocus(null);
    setBottomTab("findings");
    setBottomOpen(true);
  };
  const placePart = (c: Pick<LibraryEntry, "id" | "name">) => {
    showKind("schematic");
    setArmPart({ id: c.id, n: Date.now() });
    setMessage(`Place tool armed with ${c.name}: click the schematic to place it.`);
  };

  const actions: PaletteAction[] = [
    { id: "undo", label: "Undo", run: undo, disabled: !snapshot.can_undo, shortcut: SHORTCUTS.undo, group: "Edit" },
    { id: "redo", label: "Redo", run: redo, disabled: !snapshot.can_redo, shortcut: SHORTCUTS.redo, group: "Edit" },
    { id: "export", label: "Export manufacturing files", run: () => setDialog("export"), group: "Manufacturing" },
    { id: "erc", label: "Run ERC", run: () => check("erc"), group: "Verify" },
    { id: "drc", label: "Run DRC", run: () => check("drc"), group: "Verify" },
    { id: "dfm", label: "Run DFM (Fabricator standard profile)", run: () => check("dfm"), group: "Verify" },
    { id: "annotate", label: "Annotate schematic", run: () => execute({ type: "Annotate" }), group: "Design" },
    { id: "sync", label: "Update PCB from schematic (sync)", run: syncPcb, group: "Design" },
    { id: "place", label: "Auto-place footprints", run: () => execute({ type: "AutoPlace", grid: null }), group: "Design" },
    { id: "route", label: "Auto-route all nets", run: () => execute({ type: "AutoRoute", nets: null }), group: "Design" },
    {
      id: "clear",
      label: "Clear all routing",
      run: () => execute({ type: "ClearRouting", net: null }),
      destructive: true,
      group: "Design",
    },
    { id: "deselect", label: "Clear selection", run: () => setSelected(null), group: "Edit" },
    { id: "ws-schematic", label: "Go to Schematic editor", run: () => showWorkspace("schematic"), shortcut: SHORTCUTS.showSchematic, group: "Navigate" },
    { id: "ws-pcb", label: "Go to PCB editor", run: () => showWorkspace("pcb"), shortcut: SHORTCUTS.showPcb, group: "Navigate" },
    ...edaViews.map((w) => ({ id: `ws-${w.id}`, label: `Go to ${w.label}`, run: () => showWorkspace(w.id), shortcut: w.shortcut, group: "Navigate" })),
    { id: "open-project", label: "Open another project…", run: () => setDialog("open"), group: "Project" },
    ...projects.filter((p) => !p.active).map((p) => ({ id: `proj-${p.id}`, label: `Switch to project ${p.name}`, run: () => switchTo(p.id), group: "Project" })),
    { id: "toggle-split", label: "View: Show schematic and PCB side by side", run: () => setSplit((v) => !v), group: "View" },
    { id: "quick-open", label: "Go to part, net or sheet…", run: () => setQuickOpen(true), shortcut: SHORTCUTS.quickOpen, group: "Navigate" },
    { id: "rules", label: "Design rules and net classes…", run: () => setDialog("rules"), group: "Design" },
    ...docs.map((d) => ({ id: `open-${d.id}`, label: `Open ${d.kind === "pcb" ? "PCB" : `schematic ${d.title}`}`, run: () => open(d.id), group: "Documents" })),
    { id: "close-all", label: "Close all documents", run: closeAll, disabled: openIds.length === 0, group: "Documents" },
    { id: "reset-layout", label: "Reset panel layout", run: resetLayout, group: "View" },
    { id: "help", label: "Keyboard shortcuts", run: () => setHelp(true), group: "Help" },
    { id: "toggle-side", label: "Toggle side panel", run: () => setSideOpen((o) => !o), shortcut: SHORTCUTS.toggleSide, group: "View" },
    { id: "toggle-bottom", label: "Toggle bottom panel", run: () => setBottomOpen((o) => !o), shortcut: SHORTCUTS.toggleBottom, group: "View" },
    ...SIDE_VIEWS.map((v) => ({ id: `show-${v.id}`, label: `Show ${v.label}`, run: () => showSide(v.id), group: "View" })),
    { id: "bom", label: "Show BOM", run: () => (setBottomTab("bom"), setBottomOpen(true)), group: "View" },
    { id: "sim", label: "Show simulation", run: () => (setBottomTab("sim"), setBottomOpen(true)), group: "View" },
    { id: "log", label: "Show log", run: () => (setBottomTab("log"), setBottomOpen(true)), group: "View" },
    { id: "theme", label: theme === "dark" ? "Switch to light theme" : "Switch to dark theme", run: onTheme, group: "Appearance" },
    { id: "settings", label: "Model settings", run: onSettings, group: "Settings" },
    { id: "close", label: "Close project", run: close, group: "Project" },
  ];

  const quickItems: PaletteAction[] = [
    ...library.map((c) => ({ id: `part-${c.id}`, label: c.name, group: "Part · place", run: () => placePart(c) })),
    ...Object.values(snapshot.project.schematic.symbols).map((sy) => ({
      id: `sym-${sy.id}`,
      label: `${sy.refdes} ${sy.value}`,
      group: "Symbol",
      run: () => setSelected(sy.id),
    })),
    ...netNames(snapshot).map((n) => ({ id: `net-${n}`, label: n, group: "Net", run: () => showNet(n) })),
    ...Object.values(snapshot.project.schematic.sheets).map((sh) => ({
      id: `sheet-${sh.id}`,
      label: sh.name,
      group: "Sheet",
      run: () => showKind("schematic"),
    })),
  ];

  // Keep the latest handlers reachable from the single global key listener.
  const viewKey = (e: KeyboardEvent) => {
    const w = edaViews.find((v) => v.shortcut && matches(e, v.shortcut));
    if (w) showWorkspace(w.id);
    return !!w;
  };
  const keys = useRef({ undo, redo, showWorkspace, showSide, viewKey });
  keys.current = { undo, redo, showWorkspace, showSide, viewKey };
  const visibleRef = useRef(visible);
  visibleRef.current = visible;
  const saveRef = useRef<() => void>(() => {});
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!visibleRef.current) return;
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.target instanceof HTMLTextAreaElement;
      const mod = e.ctrlKey || e.metaKey;
      if (matches(e, SHORTCUTS.palette) || matches(e, SHORTCUTS.paletteAlt)) {
        e.preventDefault();
        setQuickOpen(false);
        setPalette(true);
      } else if (matches(e, SHORTCUTS.quickOpen)) {
        e.preventDefault();
        setPalette(false);
        setQuickOpen(true);
      } else if (matches(e, SHORTCUTS.toggleSide)) {
        e.preventDefault();
        setSideOpen((o) => !o);
      } else if (matches(e, SHORTCUTS.toggleBottom)) {
        e.preventDefault();
        setBottomOpen((o) => !o);
      } else if (matches(e, SHORTCUTS.showSchematic)) {
        e.preventDefault();
        keys.current.showWorkspace("schematic");
      } else if (matches(e, SHORTCUTS.showPcb)) {
        e.preventDefault();
        keys.current.showWorkspace("pcb");
      } else if (matches(e, SHORTCUTS.showExplorer)) {
        e.preventDefault();
        keys.current.showSide("project");
      } else if (keys.current.viewKey(e)) {
        e.preventDefault();
      } else if (mod && e.key.toLowerCase() === "s") {
        e.preventDefault();
        saveRef.current();
      } else if (typing) {
        return;
      } else if (mod && e.key.toLowerCase() === "z" && !e.shiftKey) {
        e.preventDefault();
        keys.current.undo();
      } else if (mod && (e.key.toLowerCase() === "y" || (e.key.toLowerCase() === "z" && e.shiftKey))) {
        e.preventDefault();
        keys.current.redo();
      } else if (e.key === "Escape") {
        setSelected(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [revision]);

  const fpOfSelected = selected ? Object.values(snapshot.project.pcb.footprints).find((f) => f.symbol_id === selected) : undefined;
  const rotateSel = () => {
    if (!selectedSymbol) return;
    // In the PCB editor the selected part's footprint rotates; otherwise the schematic symbol.
    const activeKind = docs.find((d) => d.id === activeId)?.kind;
    if (activeKind === "pcb" && fpOfSelected)
      execute({ type: "MoveFootprint", footprint_id: fpOfSelected.id, position: fpOfSelected.position, rotation: nextRotation(fpOfSelected.rotation), side: null });
    else execute({ type: "RotateSymbol", symbol_id: selectedSymbol.id, rotation: nextRotation(selectedSymbol.rotation) });
  };
  const deleteSel = () => {
    if (!selectedSymbol) return;
    if (!window.confirm(`Delete ${selectedSymbol.refdes}? This is a destructive change.`)) return;
    execute({ type: "DeleteSymbol", symbol_id: selectedSymbol.id });
  };
  // Save: the engine writes every accepted command before it returns, so "Save" asks it for its
  // state and reports whether anything is blocked; it never claims more than the engine says.
  const save = () =>
    guarded(async () => {
      const s2 = await api.currentProject();
      setSnapshot(s2);
      setMessage(
        s2.blocked
          ? `Writes are blocked: ${s2.blocked}`
          : `Revision ${s2.project.revision}: every accepted command is already written (engine reports no blocked writes).`,
      );
    });
  saveRef.current = save;
  const noDoc = activeId ? undefined : "Open a document first";
  const toolTo = (tool: string) => () => {
    if (!setEditorTool(activeId, tool)) setMessage(`The active editor has no ${tool} tool.`);
  };
  const needSel = selectedSymbol ? undefined : "Select a part first";
  const toolbarGroups: ToolbarGroup[] = [
    [
      { id: "new", group: "file", label: "New project", run: () => setDialog("new"), disabled: busy ? "An engine command is running" : undefined },
      { id: "open", group: "file", label: "Open project", run: () => setDialog("open"), disabled: busy ? "An engine command is running" : undefined },
      { id: "save", group: "file", label: "Save", shortcut: SHORTCUTS.save, run: save, disabled: busy ? "An engine command is running" : undefined },
      { id: "saveas", group: "file", label: "Save as", disabled: "Not implemented: the engine saves the project in place" },
      { id: "undo", group: "file", label: "Undo", shortcut: SHORTCUTS.undo, run: undo, disabled: busy ? "An engine command is running" : !snapshot.can_undo ? "Nothing to undo" : undefined },
      { id: "redo", group: "file", label: "Redo", shortcut: SHORTCUTS.redo, run: redo, disabled: busy ? "An engine command is running" : !snapshot.can_redo ? "Nothing to redo" : undefined },
    ],
    [
      { id: "select", group: "edit", label: "Select tool", title: "Select tool (key 1)", run: toolTo("select"), disabled: noDoc },
      { id: "move", group: "edit", label: "Move tool", title: "Move tool (schematic key 6, PCB key 3)", run: toolTo("move"), disabled: noDoc },
      { id: "rotate", group: "edit", label: "Rotate selection", title: "Rotate selection (key R)", run: rotateSel, disabled: needSel ?? (busy ? "An engine command is running" : undefined) },
      { id: "t-place", group: "edit", label: "Place part", title: "Place a part from the library (editor Place tool)", run: toolTo("place"), disabled: noDoc },
      { id: "t-wire", group: "edit", label: "Wire", title: "Draw a wire; ends snap to pins (AddWire)", run: toolTo("wire"), disabled: noDoc },
      { id: "t-label", group: "edit", label: "Net / power label", title: "Net label or power port (AddNetLabel; pick Net or Power in the tool options)", run: toolTo("label"), disabled: noDoc },
      { id: "t-noconnect", group: "edit", label: "No connect", title: "Mark a pin as intentionally unconnected (SetNoConnect)", run: toolTo("noconnect"), disabled: noDoc },
      { id: "t-pinlabel", group: "edit", label: "Label pin", title: "Name the net on a pin (LabelPin)", run: toolTo("pinlabel"), disabled: noDoc },
      { id: "t-mirror", group: "edit", label: "Mirror selection", title: "Mirror the selected symbol horizontally (FlipSymbol)", run: () => selectedSymbol && execute({ type: "FlipSymbol", symbol_id: selectedSymbol.id, axis: "horizontal" }), disabled: needSel ?? (busy ? "An engine command is running" : undefined) },
      { id: "t-route", group: "edit", label: "Route track", title: "Route a track with the chosen width and layer (AddTrack)", run: toolTo("route"), disabled: noDoc },
      { id: "t-via", group: "edit", label: "Via", title: "Place a via (AddVia)", run: toolTo("via"), disabled: noDoc },
      { id: "t-outline", group: "edit", label: "Board outline", title: "Draw the board outline (SetBoardOutline)", run: toolTo("outline"), disabled: noDoc },
      { id: "t-zone", group: "edit", label: "Copper zone", title: "Draw a copper zone (AddZone). The engine has no separate refill command", run: toolTo("zone"), disabled: noDoc },
      { id: "t-flip", group: "edit", label: "Flip footprint", title: "Move the selected footprint to the other board side (MoveFootprint)", run: () => fpOfSelected && execute({ type: "MoveFootprint", footprint_id: fpOfSelected.id, position: fpOfSelected.position, rotation: null, side: fpOfSelected.side === "top" ? "bottom" : "top" }), disabled: fpOfSelected ? (busy ? "An engine command is running" : fpOfSelected.locked ? "The footprint is locked" : undefined) : "Select a part that has a footprint first" },
      { id: "delete", group: "edit", label: "Delete selection", title: "Delete selection (asks first)", run: deleteSel, disabled: needSel ?? (busy ? "An engine command is running" : undefined) },
    ],
    [
      { id: "zout", group: "view", label: "Zoom out", run: () => canvasControls(activeId)?.zoomBy(1.4), disabled: noDoc },
      { id: "zin", group: "view", label: "Zoom in", run: () => canvasControls(activeId)?.zoomBy(1 / 1.4), disabled: noDoc },
      { id: "fit", group: "view", label: "Fit to view", title: "Fit to view (or double-click the canvas)", run: () => canvasControls(activeId)?.fit(), disabled: noDoc },
      { id: "grid", group: "view", label: "Grid", disabled: "Not implemented: the grid is fixed (schematic 1.27 mm, PCB 0.5 mm)" },
    ],
    [
      { id: "rules", group: "checks", label: "Design rules", title: "Design rules and net classes", run: () => setDialog("rules") },
      { id: "sync", group: "checks", label: "Update PCB", title: "Update PCB from schematic", run: syncPcb, disabled: busy ? "An engine command is running" : undefined },
      { id: "erc", group: "checks", label: "Run ERC", run: () => check("erc"), disabled: busy ? "An engine command is running" : undefined },
      { id: "drc", group: "checks", label: "Run DRC", run: () => check("drc"), disabled: busy ? "An engine command is running" : undefined },
      { id: "dfm", group: "checks", label: "Run DFM", title: "Design for manufacture against the Fabricator standard profile", run: () => check("dfm"), disabled: busy ? "An engine command is running" : undefined },
      { id: "route", group: "checks", label: "Auto-route", title: "Auto-route all nets", run: () => execute({ type: "AutoRoute", nets: null }), disabled: busy ? "An engine command is running" : undefined },
      { id: "sim", group: "checks", label: "Simulate", title: "Open the Simulation panel (ngspice)", run: () => (setBottomTab("sim"), setBottomOpen(true)) },
    ],
    [
      { id: "export", group: "mfg", label: "Export manufacturing files", title: "Gerber, Excellon, BOM, CPL, fab-pack ZIP; runs ERC/DRC/MECH/DFM", run: () => setDialog("export"), disabled: busy ? "An engine command is running" : undefined },
    ],
    [
      { id: "cmd", group: "app", label: "Commands", shortcut: SHORTCUTS.palette, run: () => setPalette(true) },
      { id: "close", group: "app", label: "Close project", run: close, disabled: busy ? "An engine command is running" : undefined },
    ],
  ];
  const noProject = busy ? "An engine command is running" : undefined;
  const menus: Menu[] = [
    { label: "File", items: [
      { label: "New project…", run: () => setDialog("new"), disabled: noProject },
      { label: "Open project…", run: () => setDialog("open"), disabled: noProject },
      ...recent.filter((r) => r.project_id !== snapshot.project.id).slice(0, 5).map((r, i) => ({ label: `Recent: ${r.name}`, run: () => openRecent(r.path), disabled: noProject, sep: i === 0 })),
      { label: "Save", shortcut: SHORTCUTS.save, run: save, disabled: noProject },
      { label: "Save as…", disabled: "Not implemented: the engine saves the project in place" },
      { label: "Close project", run: close, disabled: noProject, sep: true },
    ] },
    { label: "Edit", items: [
      { label: "Undo", shortcut: SHORTCUTS.undo, run: undo, disabled: !snapshot.can_undo ? "Nothing to undo" : noProject },
      { label: "Redo", shortcut: SHORTCUTS.redo, run: redo, disabled: !snapshot.can_redo ? "Nothing to redo" : noProject },
      { label: "Rotate selection", run: rotateSel, disabled: needSel ?? noProject, sep: true },
      { label: "Delete selection", run: () => selectedSymbol && execute({ type: "DeleteSymbol", symbol_id: selectedSymbol.id }), disabled: needSel ?? noProject, destructive: true },
      { label: "Clear selection", run: () => setSelected(null), disabled: selected ? undefined : "Nothing is selected" },
    ] },
    { label: "View", items: [
      { label: "Side panel", shortcut: SHORTCUTS.toggleSide, checked: sideOpen, run: () => setSideOpen((o) => !o) },
      { label: "Bottom panel", shortcut: SHORTCUTS.toggleBottom, checked: bottomOpen, run: () => setBottomOpen((o) => !o) },
      { label: "Split schematic and PCB", checked: split, run: () => setSplit((v) => !v) },
      ...SIDE_VIEWS.map((v, i) => ({ label: `Show ${v.label}`, run: () => showSide(v.id), sep: i === 0 })),
      { label: "Light theme", checked: theme === "light", run: onTheme, sep: true },
      { label: "Reset panel layout", run: resetLayout },
    ] },
    { label: "Project", items: [
      { label: "Annotate schematic", run: () => execute({ type: "Annotate" }), disabled: noProject },
      { label: "Update PCB from schematic", run: syncPcb, disabled: noProject },
      ...docs.map((d, i) => ({ label: `Open ${d.kind === "pcb" ? "PCB" : `sheet ${d.title}`}`, run: () => open(d.id), sep: i === 0 })),
      { label: "Close all documents", run: closeAll, disabled: openIds.length === 0 ? "No documents are open" : undefined },
    ] },
    { label: "Place", items: [
      ...(library.length === 0 ? [{ label: "Part…", disabled: "The part library is unavailable" }] : library.map((c) => ({ label: `Part: ${c.name}`, run: () => placePart(c) }))),
      { label: "Wire, label, junction…", disabled: "Use the schematic toolbar (keys 1-7); not available from the menu", sep: true },
    ] },
    { label: "Design", items: [
      { label: "Run ERC", run: () => check("erc"), disabled: noProject },
      { label: "Run DRC", run: () => check("drc"), disabled: noProject },
      { label: "Run DFM (Fabricator standard)", run: () => check("dfm"), disabled: noProject },
      { label: "Auto-place footprints", run: () => execute({ type: "AutoPlace", grid: null }), disabled: noProject, sep: true },
      { label: "Design rules…", run: () => setDialog("rules") },
    ] },
    { label: "Tools", items: [
      { label: "Command palette", shortcut: SHORTCUTS.palette, run: () => setPalette(true) },
      { label: "Go to part, net or sheet", shortcut: SHORTCUTS.quickOpen, run: () => setQuickOpen(true) },
      { label: "Model settings", run: onSettings },
    ] },
    { label: "Route", items: [
      { label: "Auto-route all nets", run: () => execute({ type: "AutoRoute", nets: null }), disabled: noProject },
      { label: "Clear all routing", run: () => execute({ type: "ClearRouting", net: null }), disabled: noProject, destructive: true },
      { label: "Interactive routing…", disabled: "Use the Route tool in the PCB editor (key 4); not available from the menu", sep: true },
    ] },
    { label: "Simulation", items: [{ label: "Run simulation…", run: () => (setBottomTab("sim"), setBottomOpen(true)) }] },
    { label: "Manufacturing", items: [
      { label: "Export manufacturing files…", run: () => setDialog("export"), disabled: noProject },
      { label: "Show manufacturing panel", run: () => showSide("manufacturing") },
      { label: "Release check…", disabled: "Not implemented: exports are always DRAFT" },
    ] },
    { label: "Settings", items: [
      { label: "Model settings", run: onSettings },
      { label: theme === "dark" ? "Switch to light theme" : "Switch to dark theme", run: onTheme },
      { label: "Reset panel layout", run: resetLayout },
    ] },
    { label: "Help", items: [
      { label: "Keyboard shortcuts", run: () => setHelp(true) },
      { label: "About EAK", run: () => setMessage("EAK: Rust engineering core, typed commands, independent validation. AI output never certifies engineering numbers.") },
    ] },
  ];
  const ctxKind = wsId !== "editor" ? "view" : (docs.find((d) => d.id === activeId)?.kind ?? "none");
  const shownToolbar = filterToolbar(toolbarGroups, ctxKind);
  const noParts = Object.keys(snapshot.project.schematic.symbols).length === 0;

  return (
    <div className="workspace">
      <MenuBar menus={menus} />
      <Toolbar
        groups={shownToolbar}
        left={
          <div className="tb-title">
            <strong>{snapshot.project.name}</strong>
            <span className="muted">revision {revision}</span>
            <span className="saved" title="Every accepted command is written to disk atomically before it returns">
              Saved · r{revision}
            </span>
          </div>
        }
      />
      {snapshot.blocked && (
        <div role="alert" className="banner">
          Writes are blocked: {snapshot.blocked}
          <button type="button" onClick={acknowledge} disabled={busy}>
            Accept on-disk state
          </button>
        </div>
      )}
      <div className="main" style={{ gridTemplateColumns: `48px ${sideOpen ? layout.sideWidth : 0}px ${sideOpen ? 4 : 0}px minmax(0, 1fr)` }}>
        <ActivityBar
          sideView={sideView}
          sideOpen={sideOpen}
          onSide={showSide}
          workspace={wsId !== "editor" ? wsId : (docs.find((d) => d.id === activeId)?.kind === "schematic" || docs.find((d) => d.id === activeId)?.kind === "pcb" ? (docs.find((d) => d.id === activeId)!.kind as string) : "")}
          onWorkspace={showWorkspace}
          registered={edaViews}
          onSettings={onSettings}
          theme={theme}
          onTheme={onTheme}
        />
        <aside className={sideOpen ? "side open" : "side"} aria-label="Side panel" aria-hidden={!sideOpen}>
          <header className="panel-head">
            <span className="title">{SIDE_VIEWS.find((v) => v.id === sideView)?.label}</span>
          </header>
          {sideView === "overview" && (
            <div className="side-scroll">
              <ProjectOverview
                snapshot={snapshot}
                results={results}
                conn={conn}
                domain={domain}
                events={events}
                busy={busy}
                views={edaViews.map((w) => w.id)}
                onView={showWorkspace}
                onCheck={check}
                onFindings={() => {
                  setBottomTab("findings");
                  setBottomOpen(true);
                }}
                onLayers={() => showSide("layers")}
                onParts={() => showSide("parts")}
              />
            </div>
          )}
          {sideView === "project" && (
            <div className="side-scroll">
              <h3 className="side-h">Files</h3>
              <FileExplorer projectId={snapshot.project.id} revision={revision} onOpen={openFile} onRemoved={fileRemoved} onRenamed={fileRenamed} notify={setMessage} />
              <h3 className="side-h">Design</h3>
              <ProjectPanel snapshot={snapshot} selected={selected} onSelect={setSelected} onNet={showNet} onSheet={(id) => open(`sch:${id}`)} />
            </div>
          )}
          {sideView === "layers" && (
            <LayerManager snapshot={snapshot} view={layerView} unrouted={conn && conn.revision === revision ? conn.ratsnest.length : null} edit={edit.execute} busy={busy} />
          )}
          {sideView === "parts" && (
            <>
              <LibraryPanel revision={revision} busy={busy} execute={edit.execute} onPlace={placePart} />
              <DistributorPartsPanel
                revision={revision}
                onImported={async (m) => {
                  setMessage(m);
                  await refresh();
                }}
              />
            </>
          )}
          {sideView === "domain" && (
            <div className="panel-body">
              <DomainPanel report={domain} revision={revision} error={domainError} />
            </div>
          )}
          {sideView === "manufacturing" && (
            <div className="side-scroll">
              <ManufacturingPanel result={exported} busy={busy} onExport={() => setDialog("export")} />
              <FabricatorPrepPanel revision={revision} onPrepared={setMessage} />
            </div>
          )}
        </aside>
        {sideOpen ? (
          <Splitter orientation="vertical" label="Resize side panel" value={layout.sideWidth} onResize={(d) => setLayout((l) => ({ ...l, sideWidth: clampSize("sideWidth", l.sideWidth + d) }))} onReset={() => setLayout((l) => ({ ...l, sideWidth: DEFAULT_LAYOUT.sideWidth }))} />
        ) : (
          <div />
        )}
        <div className="center">
          {projects.length > 1 && (
            <ProjectTabs projects={projects} onSwitch={switchTo} onClose={closeProjectTab} onOpen={() => setDialog("open")} disabled={busy ? "An engine command is running" : undefined} />
          )}
          <div className="editor-area" hidden={wsId !== "editor"}>
            <DocTabs docs={openDocs} active={activeId} dirty={dirty} onSelect={open} onClose={closeOne} onCloseAll={closeAll} split={split} onSplit={() => setSplit((v) => !v)} />
            <div className={`views ${shown.length > 1 ? "split" : "single"}`}>
              {openDocs.length === 0 && (
                <div className="canvas-empty static" aria-label="No documents open">
                  <strong>No documents open</strong>
                  <span>Open the schematic or the PCB from the activity bar, the Explorer or the Project menu.</span>
                </div>
              )}
              {[...openDocs].sort((a, b) => rank(a.kind) - rank(b.kind)).map((d) => {
                const visible = shown.some((x) => x.id === d.id);
                // Editors stay mounted while another view is shown, so their tool, selection and
                // viewport survive switching. File viewers are cheap and only mounted when shown.
                if (d.kind === "file" && !visible) return null;
                return (
                  <DocIdContext.Provider key={d.id} value={d.id}>
                    <div className={visible ? "view-slot" : "view-slot off"} onPointerDownCapture={() => d.id !== activeId && open(d.id)}>
                      {d.kind === "schematic" ? (
                        <SchematicView snapshot={snapshot} selected={selected} onSelect={setSelected} edit={edit} armPart={armPart} sheetId={d.ref} />
                      ) : d.kind === "pcb" ? (
                        <PcbView snapshot={snapshot} selected={selected} onSelect={setSelected} edit={edit} onStatus={setStatus} layerView={layerView} focus={focus} airwires={conn && conn.revision === revision ? conn.ratsnest : null} />
                      ) : (
                        <FileViewer projectId={snapshot.project.id} rel={d.ref} revision={revision} />
                      )}
                    </div>
                  </DocIdContext.Provider>
                );
              })}
              {noParts && wsId === "editor" && (
                <div className="canvas-empty" aria-label="Empty design">
                  <strong>An empty design</strong>
                  <span>Open Library / Parts to place a part, or press {formatShortcut(SHORTCUTS.quickOpen)} to go anywhere.</span>
                </div>
              )}
            </div>
          </div>
          {edaViews
            .filter((w) => w.id === wsId || visitedViews.has(w.id))
            .map((w) => (
              <div key={w.id} className="ws-host" hidden={wsId !== w.id}>
                {w.render(wsCtx(wsId === w.id))}
              </div>
            ))}
          {bottomOpen && (
            <Splitter orientation="horizontal" label="Resize bottom panel" value={layout.bottomHeight} onResize={(d) => setLayout((l) => ({ ...l, bottomHeight: clampSize("bottomHeight", l.bottomHeight - d) }))} onReset={() => setLayout((l) => ({ ...l, bottomHeight: DEFAULT_LAYOUT.bottomHeight }))} />
          )}
          {bottomOpen && (
            <BottomPanel
              height={layout.bottomHeight}
              tab={bottomTab}
              onTab={setBottomTab}
              snapshot={snapshot}
              results={results}
              events={events}
              onProbe={setSelected}
              tasks={tasks}
              onClearTasks={clearFinished}
              onNavigate={navigateFinding}
              activeFinding={activeFinding}
              onClose={() => setBottomOpen(false)}
            />
          )}
        </div>
      </div>
      <StatusBar
        project={snapshot.project.name}
        revision={revision}
        layer={status.layer === "top" ? "Top" : "Bottom"}
        gridMm={status.gridMm}
        drc={results.drc}
        selection={selLabel ? `Selected ${selLabel}` : null}
        task={{ text: taskSummary(currentTask(agentTasks), revision), title: taskTitle(currentTask(agentTasks)) }}
        busy={busy}
        message={message}
      />
      {dialog === "new" && <NewProjectDialog onOpened={onOpened} onLost={onClosed} onClose={() => setDialog(null)} />}
      {dialog === "open" && <OpenProjectDialog recent={recent} onOpened={onOpened} onLost={onClosed} onClose={() => setDialog(null)} />}
      {dialog === "rules" && <RulesDialog snapshot={snapshot} busy={busy} apply={edit.execute} onClose={() => setDialog(null)} />}
      {dialog === "export" && (
        <ExportDialog
          result={exported}
          revision={revision}
          busy={busy}
          blocked={snapshot.project.pcb.board.outline.length === 0 ? "This board has no outline yet: draw one in the PCB editor (Board outline tool) before exporting." : undefined}
          onRun={exportGerbers}
          onClose={() => setDialog(null)}
        />
      )}
      {help && (
        <div className="palette-backdrop" onClick={() => setHelp(false)}>
          <div className="palette help" role="dialog" aria-label="Keyboard shortcuts" onClick={(e) => e.stopPropagation()}>
            <h3>Keyboard shortcuts</h3>
            <ul>
              {(Object.entries(SHORTCUTS) as [string, string][]).filter(([k]) => k !== "paletteAlt").map(([k, v]) => (
                <li key={k}><span className="grow">{k.replace(/([A-Z])/g, " $1").toLowerCase()}</span><kbd>{formatShortcut(v)}</kbd></li>
              ))}
              <li><span className="grow">select editor tool</span><kbd>1–7</kbd></li>
              <li><span className="grow">rotate / flip selection</span><kbd>R / F</kbd></li>
            </ul>
            <button type="button" onClick={() => setHelp(false)}>Close</button>
          </div>
        </div>
      )}
      {syncDialog && (
        <SyncReportDialog
          report={syncDialog.report}
          error={syncDialog.error}
          revision={syncDialog.revision}
          onOpenPcb={() => {
            setSyncDialog(null);
            showWorkspace("pcb");
          }}
          onClose={() => setSyncDialog(null)}
        />
      )}
      {palette && <CommandPalette actions={actions} onClose={() => setPalette(false)} />}
      {quickOpen && (
        <CommandPalette
          actions={quickItems}
          onClose={() => setQuickOpen(false)}
          title="Quick open"
          searchLabel="Search parts, nets and sheets"
          placeholder="Go to part, net or sheet…"
          empty="Nothing matches"
        />
      )}
    </div>
  );
}
