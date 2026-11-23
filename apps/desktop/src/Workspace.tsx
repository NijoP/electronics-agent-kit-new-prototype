// Project workspace: toolbar, schematic + PCB views with cross-probing, Engineering
// Monitor, activity log, status bar and command palette. State shown here is always the
// latest engine snapshot; edits are engine calls carrying the rendered revision.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, describeError } from "./api";
import { DomainPanel } from "./DomainPanel";
import { ManufacturingPanel as FabricatorPrepPanel } from "./ManufacturingPanel";
import { CommandPalette, type PaletteAction } from "./CommandPalette";
import type { EditCtx } from "./EditContext";
import { PcbView } from "./PcbView";
import { SchematicView } from "./SchematicView";
import { formatShortcut, SHORTCUTS } from "./design/shortcuts";
import { actionForKey, contextMenuFor, keysOf, menusFor, paletteFor, runAction, toolbarFor, visibleIn } from "./actions/registry";
import { workspaceActions } from "./actions/sets";
import { usePowerPorts } from "./settings/powerPorts";
import { editorKeys } from "./actions/editorKeys";
import { APP_VERSION } from "./version";
import { useEscape } from "./shell/useEscape";
import { docContext, type Action, type DocContext } from "./actions/types";
import type { Theme } from "./design/theme";
import { ActivityBar, SIDE_VIEWS, type SideView } from "./shell/ActivityBar";
import { BottomPanel, type BottomTab } from "./shell/BottomPanel";
import { ManufacturingPanel, netNames } from "./shell/panels";
import { ProjectOverview } from "./shell/ProjectOverview";
import { LibraryDock } from "./shell/LibraryDock";
import { StatusBar } from "./shell/StatusBar";
import { ExportDialog } from "./shell/ExportDialog";
import { BomExportDialog, DocumentExportDialog } from "./shell/OutputDialogs";
import { RulesDialog } from "./shell/RulesDialog";
import { NewProjectDialog, OpenProjectDialog } from "./shell/ProjectDialogs";
import { Toolbar } from "./shell/Toolbar";
import { canvasControls, DocIdContext, setActiveEditor, setEditorTool } from "./shell/viewState";
import { LayerManager } from "./shell/LayerManager";
import { useLayerView } from "./layerView";
import type { Focus } from "./pcbFocus";
import { findingKey, probeTarget } from "./EngineeringMonitor";
import { DocTabs } from "./shell/DocTabs";
import { FileExplorer } from "./shell/FileExplorer";
import { FileViewer } from "./shell/FileViewer";
import { ProjectTabs } from "./shell/ProjectTabs";
import { EcoPreviewDialog } from "./shell/EcoPreviewDialog";
import type { SyncPreview } from "./shell/ecoPreview";
import { SyncReportDialog } from "./shell/SyncReportDialog";
import { buildSyncReport, summarizeSyncReport, type SyncReport } from "./shell/syncReport";
import { useTasks } from "./shell/tasks";
import { fileDocId, type FileEntry } from "./shell/files";
import { listWorkspaces, useRegistryVersion, type WorkspaceContext } from "./shell/registry";
import "./shell/workspaces";
import { availableDocs, closeDoc, fileDocs, loadDocState, openDoc, partDocId, partDocs, remapFileDocs, saveDocState, viewDoc, viewDocId, visibleDocs, type PartDocKind } from "./shell/documents";
import { PartEditor } from "./partEditor/PartEditor";
import { useView3dControls } from "./viewer3d/controls";
import { clampSize, DEFAULT_LAYOUT, LIMITS, useDockLayouts, type DockId, type LayoutWorkspace } from "./shell/layout";
import { CanvasStrip } from "./shell/CanvasStrip";
import { DesignManager } from "./shell/DesignManager";
import { editorFor, pcbObjects, schematicPlace, type ManagerTab, type NetRow, type Target } from "./shell/designManagerData";
import { RightDock, shownRightTab, type RightTab } from "./shell/RightDock";
import { Inspector } from "./shell/Inspector";
import { editorContext, useEditorContext } from "./shell/inspectorModel";
import { SelectionFilter } from "./shell/SelectionFilter";
import type { Bounds } from "./viewport";
import { MenuBar } from "./shell/MenuBar";
import { Splitter } from "./shell/Splitter";
import { nextRotation } from "./editing";
import { formatLength, GridControls, useGridConfig } from "./grid";
import { SettingsDialog } from "./settings/SettingsDialog";
import { useKeymap } from "./settings/keymap";
import { acceptsSnapshot, exactChange, isStale, needsRefetch, onRevisionChange, reconcileSelection, type RevisionChange, type Shown } from "./revisionSync";
import { currentTask, taskSummary, taskTitle, useAgentTasks } from "./agentTasks";
import { sheetsInOrder } from "./shell/sheets";
import type { OpenProject, RecentProject, Check, CheckResult, Command, ComponentDef, ConnectivityReport, Finding, CommandResponse, CopperLayer, DomainReport, EngineEvent, ExportResult, Id, LibraryEntry, Snapshot } from "./types";

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

// "Toolbar labels" (View menu): text beside the toolbar icons, remembered across sessions.
const TB_LABELS_KEY = "eak.toolbar.labels";
const readFlag = (key: string): boolean => {
  try {
    return localStorage.getItem(key) === "1";
  } catch {
    return false;
  }
};
const writeFlag = (key: string, on: boolean) => {
  try {
    localStorage.setItem(key, on ? "1" : "0");
  } catch {
    // Storage unavailable: the option still applies for this session.
  }
};

export function Workspace({ initial, visible, onClosed, onOpened, onSettings, theme, onTheme, projects, onProjectsChanged }: Props) {
  const [dialog, setDialog] = useState<null | "new" | "open" | "export" | "rules" | "bom" | "outputs">(null);
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
  // Symbol/footprint editor documents with a draft the engine does not have yet (D2).
  const [dirtyParts, setDirtyParts] = useState<ReadonlySet<string>>(new Set());
  const markPartDirty = (id: string, dirty: boolean) =>
    setDirtyParts((s) => (s.has(id) === dirty ? s : dirty ? new Set(s).add(id) : new Set([...s].filter((x) => x !== id))));
  // Grid settings of both editors (shared with their snapping) and the toolbar labels option.
  const schGrid = useGridConfig("schematic");
  const pcbGrid = useGridConfig("pcb");
  const [tbLabels, setTbLabels] = useState(() => readFlag(TB_LABELS_KEY));
  const [syncDialog, setSyncDialog] = useState<null | { report: SyncReport | null; error: string | null; revision: number }>(null);
  // ECO review of a pending Update PCB (slice C2): the engine's read-only preview at one revision.
  const [ecoDialog, setEcoDialog] = useState<SyncPreview | null>(null);
  const { tasks, track, clearFinished } = useTasks();
  useRegistryVersion();
  // Registered EDA views (the 3D board viewer) open as document tabs next to the editors (C6).
  const edaViews = listWorkspaces("eda").map((w, i) => ({ ...w, shortcut: w.shortcut ?? (i < 7 ? `Mod+${3 + i}` : undefined) }));
  const view3d = useView3dControls();
  const [help, setHelp] = useState(false);
  const [about, setAbout] = useState(false);
  const [prefs, setPrefs] = useState(false);
  // Menus, tooltips and the palette show the effective keys: re-render when a key is rebound.
  useKeymap();
  const [conn, setConn] = useState<ConnectivityReport | null>(null);
  const [focus, setFocus] = useState<Focus | null>(null);
  const [activeFinding, setActiveFinding] = useState<string | null>(null);
  const layerView = useLayerView(initial.project.id);
  const docks = useDockLayouts();
  const [dmTab, setDmTab] = useState<ManagerTab>("pages");
  // Design manager zoom into a schematic sheet, applied once that sheet's canvas is registered.
  const [pendingZoom, setPendingZoom] = useState<{ docId: string; bounds: Bounds } | null>(null);
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
  const rank = (k: string) => (k === "schematic" ? 0 : k === "pcb" ? 1 : k === "view" ? 3 : 2);
  // What a view document (3D) gets: the selection and cross-probing, the engine path the editors
  // use, and the PCB layer manager's colours.
  const wsCtx = (active: boolean): WorkspaceContext => ({
    projectId: snapshot.project.id,
    projectName: snapshot.project.name,
    projectDir: snapshot.dir,
    revision,
    openProjects: projects,
    active,
    selected,
    select: setSelected,
    probeTo,
    execute: (c) => edit.execute(c),
    busy,
    layerColour: layerView.colourOf,
  });
  // Built-in parts plus parts imported into this project (e.g. from Distributor), so an imported
  // part can be placed from the part picker.
  // A personal or imported part armed from the Library panel: kept in the Place tool's list so
  // the tool places that part (the backend copies it into the project on placement, F8).
  const [armedEntry, setArmedEntry] = useState<LibraryEntry | null>(null);
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
    const extra = armedEntry && !known.has(armedEntry.id) && !imported.some((l) => l.id === armedEntry.id) ? [armedEntry] : [];
    return [...baseLibrary, ...imported, ...extra];
  }, [baseLibrary, snapshot.project.library, armedEntry]);
  const docs = [...availableDocs(snapshot), ...fileDocs(docState.open), ...partDocs(docState.open, snapshot), ...edaViews.map((w) => viewDoc(w.id, w.label))];
  // Only documents the engine still has stay open (the snapshot is authoritative).
  const liveIds = new Set(docs.map((d) => d.id));
  const openIds = docState.open.filter((id) => liveIds.has(id));
  const activeId = docState.active && liveIds.has(docState.active) ? docState.active : (openIds[0] ?? null);
  const effective = { open: openIds, active: activeId };
  useEffect(() => saveDocState(snapshot.project.id, effective), [snapshot.project.id, openIds.join("|"), activeId]);
  const shown = visibleDocs(effective, docs, split);
  const activeKind = docs.find((d) => d.id === activeId)?.kind;
  // The workspace whose dock layout is shown (spec UI-01: layout persists per workspace).
  const ctx: DocContext = docContext(activeKind);
  const layout = docks.get(ctx);
  const sideOpen = layout.left.open;
  const bottomOpen = layout.bottom.open;
  const rightOpen = layout.right.open;
  const sideView: SideView = SIDE_VIEWS.some((v) => v.id === layout.left.tab) ? (layout.left.tab as SideView) : "project";
  const bottomTab: BottomTab = layout.bottom.tab;
  const rightTab = shownRightTab(layout.right.tab, ctx === "pcb");
  const toggler = (dock: DockId) => (f: boolean | ((o: boolean) => boolean)) => docks.patch(ctx, dock, (d) => ({ open: typeof f === "function" ? f(d.open) : f }));
  const setSideOpen = toggler("left");
  const setBottomOpen = toggler("bottom");
  const setRightOpen = toggler("right");
  const setBottomTab = (tab: BottomTab, ws: LayoutWorkspace = ctx) => docks.patch(ws, "bottom", { tab, open: true });
  const showRight = (tab: RightTab, ws: LayoutWorkspace = ctx) => docks.patch(ws, "right", { tab, open: true });
  const resize = (dock: DockId, delta: number) => docks.patch(ctx, dock, (d) => ({ size: clampSize(dock, d.size + delta) }));
  const resetSize = (dock: DockId) => docks.patch(ctx, dock, { size: DEFAULT_LAYOUT[dock].size });
  // Editors listen on window: while this workspace is hidden none may act (a view tab being
  // active leaves no editor active either, since its id is no editor's).
  setActiveEditor(visible ? activeId : "__hidden__");
  const openDocs = openIds.map((id) => docs.find((d) => d.id === id)!);
  // Per-document marker (A-SAVE): the engine writes every accepted command before it returns, so
  // the only "unconfirmed" states are writes blocked (every engine document) or a command in
  // flight (the active document, which issued it). File viewers are not engine documents.
  const docMarker = (d: { id: string; kind: string } | undefined): string | null =>
    !d || d.kind === "file" || d.kind === "view"
      ? null
      : dirtyParts.has(d.id)
        ? "Unsaved changes in this editor; Save sends them to the engine"
        : snapshot.blocked
          ? `Writes are blocked: ${snapshot.blocked}`
          : busy && d.id === activeId
            ? "A command is in flight; the engine has not confirmed it yet"
            : null;
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
            ? `${last.command} → revision ${last.revision_after}, saved${last.warnings.length ? ` (${last.warnings.join("; ")})` : ""}`
            : `${last.command} rejected: ${last.error ? describeError(last.error) : "unknown error"}`,
        );
      await refresh();
    });
  const edit: EditCtx = { execute: executeAll, busy, library, notify: setMessage };
  // "Update PCB from schematic" (slice C2, ADR-0041): first the engine's read-only preview of what
  // SyncSchematicToPcb would change, reviewed in the ECO dialog. Nothing changes until Apply.
  const syncPcb = () =>
    guarded(async () => {
      await track(
        "Update PCB preview",
        async () => {
          // Gate (ADR-0036): the PCB is synchronised only from a schematic ERC has run on.
          if (results.erc?.report.revision !== revision) {
            const erc = await api.runCheck("erc");
            setResults((old) => ({ ...old, erc }));
          }
          const preview = await api.syncPreview(revision);
          if (preview.changes.length === 0 && preview.warnings.length === 0) {
            const report = buildSyncReport([], snapshot.project.pcb);
            setSyncDialog({ report, error: null, revision });
            setMessage(summarizeSyncReport(report));
            return summarizeSyncReport(report);
          }
          setEcoDialog(preview);
          return `${preview.changes.length} change(s) to review at revision ${preview.revision}`;
        },
        (d) => d,
      );
    });
  // Apply the reviewed preview: the sync as one Batch at the previewed revision (a project that
  // moved on since the preview rejects it as stale, spec AG-03), then a report built from the
  // engine's own journalled change list for that operation, so it cannot disagree with what was
  // applied.
  const applyEco = (preview: SyncPreview) =>
    guarded(async () => {
      setEcoDialog(null);
      const pid = snapshot.project.id;
      const revision = preview.revision;
      await track(
        "Update PCB",
        async () => {
          const r = await api.execute(revision, { type: "Batch", commands: [{ type: "SyncSchematicToPcb" }], label: "Update PCB from schematic" });
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
  // D3 BOM and document exports (their dialogs run the export and show the manifest).
  const outputExported = (r: ExportResult) => {
    setMessage(`Exported ${r.manifest.files.length} file(s) to ${r.out_dir} (${r.manifest.status}, revision ${r.manifest.revision})`);
    void syncTo(r.manifest.revision).catch((e) => setMessage(String(e)));
  };
  const undo = () => submit(() => api.undo(revision));
  const redo = () => submit(() => api.redo(revision));
  // Zone fill is derived from the board at analysis time (the IR stores only the outline), so
  // "refill" recomputes the engine's connectivity (islands, airwires) and re-runs DRC for this
  // revision, and shows the results in Findings. It changes no design data.
  const refillZones = () =>
    guarded(async () => {
      const out = await track(
        "Refill zones",
        async () => ({ conn: await api.connectivity(), drc: await api.runCheck("drc") }),
        (r) => `DRC ${r.drc.report.errors} error(s), ${r.drc.report.warnings} warning(s) at revision ${r.drc.report.revision}`,
      );
      setConn(out.conn);
      setResults((old) => ({ ...old, drc: out.drc }));
      setBottomTab("findings");
      setMessage(`Zones refilled: connectivity and DRC recomputed for revision ${out.drc.report.revision}`);
    });
  const check = (c: Check) =>
    guarded(async () => {
      const result = await track(`Run ${c.toUpperCase()}`, () => api.runCheck(c), (r) => `${r.report.findings.length} finding(s) at revision ${r.report.revision}`);
      setResults((old) => ({ ...old, [c]: result }));
      setBottomTab("findings");
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

  // "Show <view>" (menus, overview links) always shows; the activity bar button toggles, so
  // pressing the open view again collapses the panel (E1). Both act on this workspace's left dock.
  const showSide = (v: SideView) => docks.patch(ctx, "left", { open: true, tab: v });
  const toggleSideView = (v: SideView) => docks.patch(ctx, "left", (d) => (d.open && d.tab === v ? { open: false } : { open: true, tab: v }));

  // Net summary from the geometry the engine derived (no model involved).
  const showNet = (name: string) => {
    const pads = snapshot.geometry.footprints.reduce((n, f) => n + f.pads.filter((p) => p.net === name).length, 0);
    const tracks = Object.values(snapshot.project.pcb.tracks).filter((t) => t.net === name).length;
    setMessage(`Net ${name}: ${pads} pad(s) and ${tracks} track segment(s) on the PCB.`);
  };

  const open = (id: string) => {
    setDocState((st) => openDoc({ open: openIds, active: st.active }, id));
  };
  const showKind = (kind: "schematic" | "pcb") => {
    if (shown.some((d) => d.kind === kind)) return;
    const d = docs.find((x) => x.kind === kind);
    if (d) open(d.id);
  };
  // Activity-bar / shortcut navigation: "schematic" and "pcb" open that editor on its own;
  // anything else is a registered EDA view (e.g. 3D) that opens as its own document tab (C6).
  const showWorkspace = (id: string) => {
    if (id === "schematic" || id === "pcb") {
      const active = docs.find((d) => d.id === activeId);
      if (active?.kind === id) return;
      const same = openIds.map((i) => docs.find((d) => d.id === i)).find((d) => d?.kind === id) ?? docs.find((d) => d.kind === id);
      if (same) open(same.id);
    } else if (edaViews.some((w) => w.id === id)) open(viewDocId(id));
  };
  // Cross-probe from the 3D document: open the PCB zoomed to the selected footprint, or the
  // schematic sheet that holds the selected symbol (the selection itself is shared).
  const probeTo = (kind: "pcb" | "schematic") => {
    const sym = selected ? snapshot.project.schematic.symbols[selected] : undefined;
    if (!sym) return;
    if (kind === "pcb") {
      const fp = Object.values(snapshot.project.pcb.footprints).find((f) => f.symbol_id === sym.id);
      const d = docs.find((x) => x.kind === "pcb");
      if (!fp || !d) return;
      open(d.id);
      setFocus({ n: Date.now(), objects: [fp.id], location: null });
    } else {
      const d = docs.find((x) => x.kind === "schematic" && x.ref === sym.sheet_id);
      if (d) open(d.id);
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
    const why = docMarker(d);
    if (why && !window.confirm(`${d?.title ?? "Document"}: ${why}. Close anyway?`)) return;
    setDocState(closeDoc({ open: openIds, active: activeId }, id));
  };
  const closeAll = () => {
    if (openDocs.some((d) => docMarker(d)) && !window.confirm("The engine has not confirmed the latest change. Close all documents anyway?")) return;
    setDocState({ open: [], active: null });
  };
  const resetLayout = () => docks.reset(ctx);
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
    // The findings stay in view in the workspace the finding opened.
    setBottomTab("findings", kind);
  };
  // Design manager entry: open the page, or select the object and zoom to it in the editor it
  // belongs to (components and nets follow the active editor; nothing on the board falls back to
  // the schematic).
  const navigateManager = (t: Target, netlist: NetRow[] | null) => {
    if (t.kind === "page") return open(t.docId);
    if (t.kind === "component") setSelected(t.symbolId);
    const name = t.kind === "component" ? (snapshot.project.schematic.symbols[t.symbolId]?.refdes ?? t.symbolId) : t.kind === "net" ? `Net ${t.name}` : t.id;
    const objs = editorFor(t, activeKind) === "pcb" ? pcbObjects(snapshot, t) : [];
    if (objs.length > 0) {
      const d = docs.find((x) => x.kind === "pcb");
      if (d) open(d.id);
      setActiveFinding(null);
      setFocus({ n: Date.now(), objects: objs, location: null });
      if (t.kind === "net") showNet(t.name);
      else setMessage(`${name}: selected and zoomed in the PCB`);
      return;
    }
    const place = schematicPlace(snapshot, t, netlist);
    if (!place) {
      setMessage(`${name} has no position to zoom to in the schematic or on the PCB`);
      return;
    }
    const docId = `sch:${place.sheetId}`;
    open(docId);
    setPendingZoom({ docId, bounds: place.bounds });
    const pins = t.kind === "net" ? netlist?.find((n) => n.name === t.name)?.pins.length : undefined;
    setMessage(t.kind === "net" ? `${name}: ${pins ?? 0} pin(s) in the engine netlist; zoomed in the schematic` : `${name}: selected and zoomed in the schematic`);
  };
  useEffect(() => {
    if (!pendingZoom) return;
    const c = canvasControls(pendingZoom.docId);
    if (!c) return;
    c.zoomTo(pendingZoom.bounds);
    setPendingZoom(null);
  }, [pendingZoom, activeId]);
  const placePart = (c: Pick<LibraryEntry, "id" | "name"> | LibraryEntry) => {
    if ("pins" in c) setArmedEntry(c);
    showKind("schematic");
    setArmPart({ id: c.id, n: Date.now() });
    setMessage(`Place tool armed with ${c.name}: click the schematic to place it.`);
  };

  const quickItems: PaletteAction[] = [
    ...library.map((c) => ({ id: `part-${c.id}`, label: c.name, group: "Part · place", run: () => placePart(c) })),
    ...Object.values(snapshot.project.schematic.symbols).map((sy) => ({
      id: `sym-${sy.id}`,
      label: `${sy.refdes} ${sy.value}`,
      group: "Symbol",
      run: () => setSelected(sy.id),
    })),
    ...netNames(snapshot).map((n) => ({ id: `net-${n}`, label: n, group: "Net", run: () => showNet(n) })),
    ...sheetsInOrder(snapshot.project.schematic).map((sh) => ({
      id: `sheet-${sh.id}`,
      label: sh.name,
      group: "Sheet",
      run: () => open(`sch:${sh.id}`),
    })),
  ];

  // The single global key listener runs the registry action bound to the key (default key map in
  // design/shortcuts.ts); it reads the latest actions through this ref.
  const keys = useRef<{ actions: Action[]; ctx: DocContext }>({ actions: [], ctx: "home" });
  const visibleRef = useRef(visible);
  visibleRef.current = visible;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!visibleRef.current) return;
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement || e.target instanceof HTMLTextAreaElement;
      const a = actionForKey(keys.current.actions, keys.current.ctx, e, typing);
      if (!a) return;
      if (a.id !== "deselect") e.preventDefault();
      runAction(a);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

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
  const toolTo = (tool: string) => {
    if (!setEditorTool(activeId, tool)) setMessage(`The active editor has no ${tool} tool.`);
  };
  // One definition per operation (src/actions/): menus, toolbar, palette and keys of the active
  // document are all generated from it.
  const gridKind = ctx === "schematic" || ctx === "pcb" ? ctx : null;
  const [gridCfg, setGridCfg] = gridKind === "pcb" ? pcbGrid : schGrid;
  const activeGrid = gridKind ? { kind: gridKind, cfg: gridCfg, set: setGridCfg } : null;
  // B3: the active editor's own editing operations (clipboard, arrange, find, net highlight), read
  // from the context it publishes; subscribing re-renders the menus when their state changes.
  const editorDoc = visible && activeKind !== "view" ? activeId : null;
  useEditorContext(editorDoc);
  const editorOp = (op: string) => {
    const o = editorContext(editorDoc)?.ops?.[op];
    return o ? { enabled: () => editorContext(editorDoc)?.ops?.[op]?.state ?? o.state, run: () => (editorContext(editorDoc)?.ops?.[op] ?? o).run() } : null;
  };
  // The Power port split button follows Settings › Power ports.
  const powerPortList = usePowerPorts();
  const allActions = workspaceActions({
    editorOp,
    snapshot, busy, powerPorts: powerPortList, selected, selectedSymbol, fpOfSelected, theme, sideOpen, bottomOpen, rightOpen, split, recent, library, docs, openIds, projects,
    sideViews: SIDE_VIEWS,
    edaViews,
    dialog: setDialog,
    save, undo, redo, close, openRecent, execute, check, syncPcb, refillZones, rotateSel, deleteSel,
    tool: toolTo,
    zoomBy: (f) => canvasControls(activeId)?.zoomBy(f),
    fit: () => canvasControls(activeId)?.fit(),
    select: setSelected,
    toggleSplit: () => setSplit((v) => !v),
    showSide,
    toggleSide: () => setSideOpen((o) => !o),
    toggleBottom: () => setBottomOpen((o) => !o),
    toggleRight: () => setRightOpen((o) => !o),
    showBottom: (t) => setBottomTab(t),
    showRight: (t) => showRight(t),
    onTheme, resetLayout, onSettings,
    palette: () => (setQuickOpen(false), setPalette(true)),
    quickOpen: () => (setPalette(false), setQuickOpen(true)),
    help: () => setHelp(true),
    about: () => setAbout(true),
    preferences: () => setPrefs(true),
    sideView,
    notify: setMessage,
    open, closeAll, showWorkspace, switchTo, placePart,
    grid: activeGrid,
    toolbarLabels: tbLabels,
    view3d,
    probeTo,
    toggleToolbarLabels: () => {
      writeFlag(TB_LABELS_KEY, !tbLabels);
      setTbLabels(!tbLabels);
    },
  });
  keys.current = { actions: allActions, ctx };
  const menus = menusFor(allActions, ctx);
  const shownToolbar = toolbarFor(allActions, ctx);
  const noParts = Object.keys(snapshot.project.schematic.symbols).length === 0;

  return (
    <div className="workspace">
      <MenuBar menus={menus} />
      <Toolbar
        groups={shownToolbar}
        labels={tbLabels}
        extras={gridKind ? { view: <GridControls kind={gridKind} /> } : {}}
        left={
          <div className="tb-title">
            <strong>{snapshot.project.name}</strong>
            <span className="muted">revision {revision}</span>
            {snapshot.blocked ? (
              <span className="saved bad" data-state="blocked" title={`Writes are blocked: ${snapshot.blocked}`}>
                Writes blocked · r{revision}
              </span>
            ) : busy ? (
              <span className="saved busy" data-state="busy" title="A command is in flight; the engine has not confirmed it yet">
                Writing… · r{revision}
              </span>
            ) : (
              <span className="saved" data-state="saved" title="Every accepted command is written to disk atomically before it returns">
                Saved · r{revision}
              </span>
            )}
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
      <div
        className="main"
        style={{ gridTemplateColumns: `48px ${sideOpen ? layout.left.size : 0}px ${sideOpen ? 4 : 0}px minmax(0, 1fr) ${rightOpen ? 4 : 0}px ${rightOpen ? layout.right.size : 0}px` }}
      >
        <ActivityBar
          sideView={sideView}
          sideOpen={sideOpen}
          onSide={toggleSideView}
          workspace={activeKind === "view" ? (docs.find((d) => d.id === activeId)?.ref ?? "") : activeKind === "schematic" || activeKind === "pcb" ? activeKind : ""}
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
                onLayers={() => {
                  showWorkspace("pcb");
                  showRight("layers", "pcb");
                }}
                onParts={() => setBottomTab("library")}
              />
            </div>
          )}
          {sideView === "project" && (
            <div className="side-scroll">
              <h3 className="side-h">Files</h3>
              <FileExplorer projectId={snapshot.project.id} revision={revision} onOpen={openFile} onRemoved={fileRemoved} onRenamed={fileRenamed} notify={setMessage} />
            </div>
          )}
          {sideView === "design" && (
            <DesignManager snapshot={snapshot} conn={conn} selected={selected} activeDoc={activeId} tab={dmTab} onTab={setDmTab} onNavigate={navigateManager} onCommand={execute} busy={busy} />
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
          <Splitter orientation="vertical" label="Resize side panel" value={layout.left.size} min={LIMITS.left[0]} max={LIMITS.left[1]} onResize={(d) => resize("left", d)} onReset={() => resetSize("left")} />
        ) : (
          <div />
        )}
        <div className="center">
          {projects.length > 1 && (
            <ProjectTabs projects={projects} onSwitch={switchTo} onClose={closeProjectTab} onOpen={() => setDialog("open")} disabled={busy ? "An engine command is running" : undefined} />
          )}
          <div className="editor-area">
            <DocTabs docs={openDocs} active={activeId} marker={docMarker} onSelect={open} onClose={closeOne} onCloseAll={closeAll} split={split} onSplit={() => setSplit((v) => !v)} />
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
                        <SchematicView snapshot={snapshot} selected={selected} onSelect={setSelected} edit={edit} armPart={armPart} sheetId={d.ref} contextMenu={(t) => contextMenuFor(allActions, "schematic", t)} />
                      ) : d.kind === "pcb" ? (
                        <PcbView snapshot={snapshot} selected={selected} onSelect={setSelected} edit={edit} onStatus={setStatus} layerView={layerView} focus={focus} airwires={conn && conn.revision === revision ? conn.ratsnest : null} />
                      ) : d.kind === "view" ? (
                        // A registered view document (3D): mounted from its first opening until its tab is closed.
                        (edaViews.find((w) => w.id === d.ref)?.render(wsCtx(visible)) ?? null)
                      ) : d.kind === "symbol" || d.kind === "footprint" ? (
                        <PartEditor
                          kind={d.kind}
                          def={snapshot.project.library![d.ref] as unknown as ComponentDef}
                          disabled={snapshot.blocked ? `Writes are blocked: ${snapshot.blocked}` : busy ? "An engine command is running" : null}
                          placedOnPcb={Object.values(snapshot.project.pcb.footprints).filter((f) => f.component_id === d.ref).length}
                          execute={edit.execute}
                          onDirty={(dirty) => markPartDirty(d.id, dirty)}
                        />
                      ) : (
                        <FileViewer projectId={snapshot.project.id} rel={d.ref} revision={revision} />
                      )}
                    </div>
                  </DocIdContext.Provider>
                );
              })}
              {noParts && (activeKind === "schematic" || activeKind === "pcb") && (
                <div className="canvas-empty" aria-label="Empty design">
                  <strong>An empty design</strong>
                  <span>Open the Library tab of the bottom panel to place a part, or press {formatShortcut(SHORTCUTS.quickOpen)} to go anywhere.</span>
                </div>
              )}
            </div>
            {activeGrid && <CanvasStrip docId={activeId} cfg={activeGrid.cfg} />}
          </div>
          {bottomOpen && (
            <Splitter orientation="horizontal" label="Resize bottom panel" value={layout.bottom.size} min={LIMITS.bottom[0]} max={LIMITS.bottom[1]} onResize={(d) => resize("bottom", -d)} onReset={() => resetSize("bottom")} />
          )}
          {bottomOpen && (
            <BottomPanel
              height={layout.bottom.size}
              tab={bottomTab}
              onTab={(t) => setBottomTab(t)}
              snapshot={snapshot}
              results={results}
              events={events}
              onProbe={setSelected}
              tasks={tasks}
              onClearTasks={clearFinished}
              onNavigate={navigateFinding}
              activeFinding={activeFinding}
              onClose={() => setBottomOpen(false)}
              library={
                <LibraryDock
                  revision={revision}
                  busy={busy}
                  execute={edit.execute}
                  onPlace={placePart}
                  onEdit={(kind: PartDocKind, id: string) => open(partDocId(kind, id))}
                  onImported={async (m) => {
                    setMessage(m);
                    await refresh();
                  }}
                />
              }
            />
          )}
        </div>
        {rightOpen ? (
          <Splitter orientation="vertical" label="Resize right panel" value={layout.right.size} min={LIMITS.right[0]} max={LIMITS.right[1]} onResize={(d) => resize("right", -d)} onReset={() => resetSize("right")} />
        ) : (
          <div />
        )}
        {rightOpen ? (
          <RightDock pcb={ctx === "pcb"} tab={rightTab} onTab={(t) => showRight(t)} onClose={() => setRightOpen(false)}>
            {rightTab === "filter" ? (
              <SelectionFilter />
            ) : rightTab === "layers" ? (
              <LayerManager snapshot={snapshot} view={layerView} unrouted={conn && conn.revision === revision ? conn.ratsnest.length : null} edit={edit.execute} busy={busy} />
            ) : (
              <Inspector snapshot={snapshot} docId={activeKind === "view" ? null : activeId} selected={selected} pcb={ctx === "pcb"} edit={edit} />
            )}
          </RightDock>
        ) : (
          <div />
        )}
      </div>
      <StatusBar
        project={snapshot.project.name}
        revision={revision}
        layer={ctx === "pcb" ? (status.layer === "top" ? "Top" : "Bottom") : null}
        grid={activeGrid ? `${formatLength(activeGrid.cfg.spacingMm, activeGrid.cfg.unit)}${activeGrid.cfg.snap ? "" : " (snap off)"}` : null}
        drc={results.drc}
        selection={selLabel ? `Selected ${selLabel}` : null}
        task={{ text: taskSummary(currentTask(agentTasks), revision), title: taskTitle(currentTask(agentTasks)) }}
        busy={busy}
        message={message}
      />
      {dialog === "new" && <NewProjectDialog onOpened={onOpened} onLost={onClosed} onClose={() => setDialog(null)} />}
      {dialog === "open" && <OpenProjectDialog recent={recent} onOpened={onOpened} onLost={onClosed} onClose={() => setDialog(null)} />}
      {dialog === "rules" && <RulesDialog snapshot={snapshot} busy={busy} apply={edit.execute} onClose={() => setDialog(null)} />}
      {dialog === "bom" && <BomExportDialog revision={revision} busy={busy} onExported={outputExported} onClose={() => setDialog(null)} />}
      {dialog === "outputs" && <DocumentExportDialog revision={revision} busy={busy} onExported={outputExported} onClose={() => setDialog(null)} />}
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
      {help && <ShortcutHelp actions={visibleIn(allActions, ctx).filter((a) => keysOf(a).length > 0)} editor={editorKeys(ctx)} onClose={() => setHelp(false)} />}
      {about && <AboutDialog onClose={() => setAbout(false)} />}
      {prefs && <SettingsDialog actions={allActions} theme={theme} onTheme={onTheme} onModel={() => (setPrefs(false), onSettings())} onClose={() => setPrefs(false)} />}
      {ecoDialog && (
        <EcoPreviewDialog
          preview={ecoDialog}
          busy={busy}
          onApply={() => applyEco(ecoDialog)}
          onExport={(excluded) => api.syncPreviewExport(ecoDialog.revision, excluded)}
          onClose={() => setEcoDialog(null)}
        />
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
      {palette && <CommandPalette actions={paletteFor(allActions, ctx)} onClose={() => setPalette(false)} />}
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

/** Help > Keyboard shortcuts: generated from the registry actions of the active document and the
 * active editor's own keys (actions/editorKeys.ts); nothing here is typed by hand. */
function ShortcutHelp({ actions, editor, onClose }: { actions: Action[]; editor: { keys: string; label: string }[]; onClose: () => void }) {
  useEscape(onClose);
  const fmt = (spec: string) => spec.split(" / ").map((k) => formatShortcut(k)).join(" / ");
  return (
    <div className="palette-backdrop" onClick={onClose}>
      <div className="palette help" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts" onClick={(e) => e.stopPropagation()}>
        <h3>Keyboard shortcuts</h3>
        <ul aria-label="Workspace shortcuts">
          {actions.map((a) => (
            <li key={a.id}><span className="grow">{a.label}</span><kbd>{keysOf(a).map((k) => formatShortcut(k)).join(" / ")}</kbd></li>
          ))}
        </ul>
        {editor.length > 0 && (
          <>
            <h4>In the active editor</h4>
            <ul aria-label="Editor shortcuts">
              {editor.map((k) => (
                <li key={k.keys + k.label}><span className="grow">{k.label}</span><kbd>{fmt(k.keys)}</kbd></li>
              ))}
            </ul>
          </>
        )}
        <button type="button" onClick={onClose}>Close</button>
      </div>
    </div>
  );
}

/** Help > About EAK: the version this build reports and what EAK does and does not claim. */
function AboutDialog({ onClose }: { onClose: () => void }) {
  useEscape(onClose);
  return (
    <div className="palette-backdrop" onClick={onClose}>
      <div className="palette help" role="dialog" aria-modal="true" aria-label="About EAK" onClick={(e) => e.stopPropagation()}>
        <h3>About EAK</h3>
        <p>
          Electronics Agent Kit <strong data-testid="app-version">version {APP_VERSION}</strong>
        </p>
        <p>
          A Rust engineering core with one design representation, typed commands and independent validation. Every edit, from the UI, the CLI or an
          agent, is one engine command; AI output never certifies engineering numbers.
        </p>
        <p className="muted">Exports are drafts for review. Checks that are not implemented are listed as such, never as passing.</p>
        <button type="button" onClick={onClose}>Close</button>
      </div>
    </div>
  );
}
