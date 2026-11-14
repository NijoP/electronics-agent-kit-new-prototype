// Project workspace: toolbar, schematic + PCB views with cross-probing, Engineering
// Monitor, activity log, status bar and command palette. State shown here is always the
// latest engine snapshot; edits are engine calls carrying the rendered revision.
import { useCallback, useEffect, useRef, useState } from "react";
import { api, describeError } from "./api";
import { DomainPanel } from "./DomainPanel";
import { CommandPalette, type PaletteAction } from "./CommandPalette";
import type { EditCtx } from "./EditContext";
import { PcbView } from "./PcbView";
import { SchematicView } from "./SchematicView";
import { formatShortcut, matches, SHORTCUTS } from "./design/shortcuts";
import type { Theme } from "./design/theme";
import { ActivityBar, SIDE_VIEWS, type SideView } from "./shell/ActivityBar";
import { AiPanel, type ChatMsg } from "./shell/AiPanel";
import { interpret, type Proposal } from "./shell/ai";
import { BottomPanel, type BottomTab } from "./shell/BottomPanel";
import { ManufacturingPanel, netNames, ProjectPanel } from "./shell/panels";
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
import { availableDocs, closeDoc, loadDocState, openDoc, saveDocState, visibleDocs } from "./shell/documents";
import { clampSize, DEFAULT_LAYOUT, loadLayout, saveLayout, type Layout } from "./shell/layout";
import { MenuBar, type Menu } from "./shell/MenuBar";
import { Splitter } from "./shell/Splitter";
import { nextRotation } from "./editing";
import type { RecentProject, Check, CheckResult, Command, ConnectivityReport, Finding, CommandResponse, CopperLayer, DomainReport, EngineEvent, ExportResult, Id, LibraryEntry, Snapshot } from "./types";

interface Props {
  initial: Snapshot;
  onClosed: () => void;
  /** A different project was created or opened from inside the workspace. */
  onOpened: (s: Snapshot) => void;
  onSettings: () => void;
  theme: Theme;
  onTheme: () => void;
}


const MAX_EVENTS = 200;

export function Workspace({ initial, onClosed, onOpened, onSettings, theme, onTheme }: Props) {
  const [dialog, setDialog] = useState<null | "new" | "open" | "export" | "rules">(null);
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const [snapshot, setSnapshot] = useState(initial);
  const [events, setEvents] = useState<EngineEvent[]>([]);
  const lastSeq = useRef(0);
  const [results, setResults] = useState<Partial<Record<Check, CheckResult>>>({});
  const [selected, setSelected] = useState<Id | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [palette, setPalette] = useState(false);
  const [busy, setBusy] = useState(false);
  const [library, setLibrary] = useState<LibraryEntry[]>([]);
  const [exported, setExported] = useState<ExportResult | null>(null);
  const [docState, setDocState] = useState(() => loadDocState(initial));
  const [split, setSplit] = useState(true);
  const [layout, setLayout] = useState<Layout>(loadLayout);
  const [help, setHelp] = useState(false);
  const [conn, setConn] = useState<ConnectivityReport | null>(null);
  const [focus, setFocus] = useState<Focus | null>(null);
  const [activeFinding, setActiveFinding] = useState<string | null>(null);
  const layerView = useLayerView(initial.project.id);
  const flag = (k: "sideOpen" | "bottomOpen" | "aiOpen") => (f: boolean | ((o: boolean) => boolean)) =>
    setLayout((l) => ({ ...l, [k]: typeof f === "function" ? f(l[k]) : f }));
  const { sideOpen, bottomOpen, aiOpen } = layout;
  const setSideOpen = flag("sideOpen");
  const setBottomOpen = flag("bottomOpen");
  const setAiOpen = flag("aiOpen");
  useEffect(() => saveLayout(layout), [layout]);
  const [sideView, setSideView] = useState<SideView>("project");
  const [bottomTab, setBottomTab] = useState<BottomTab>("log");
  const [quickOpen, setQuickOpen] = useState(false);
  const [armPart, setArmPart] = useState<{ id: string; n: number } | null>(null);
  const [chat, setChat] = useState<ChatMsg[]>([]);
  const [aiFocus, setAiFocus] = useState(0);
  const [status, setStatus] = useState<{ layer: CopperLayer; gridMm: number }>({ layer: "top", gridMm: 0.5 });
  const [model, setModel] = useState<string | null>(null);
  const msgId = useRef(1);
  const revision = snapshot.project.revision;
  const docs = availableDocs(snapshot);
  // Only documents the engine still has stay open (the snapshot is authoritative).
  const liveIds = new Set(docs.map((d) => d.id));
  const openIds = docState.open.filter((id) => liveIds.has(id));
  const activeId = docState.active && liveIds.has(docState.active) ? docState.active : (openIds[0] ?? null);
  const effective = { open: openIds, active: activeId };
  useEffect(() => saveDocState(snapshot.project.id, effective), [snapshot.project.id, openIds.join("|"), activeId]);
  const shown = visibleDocs(effective, docs, split);
  setActiveEditor(activeId);
  const openDocs = openIds.map((id) => docs.find((d) => d.id === id)!);
  const dirty = busy || !!snapshot.blocked;
  const [domain, setDomain] = useState<DomainReport | null>(null);
  const [domainError, setDomainError] = useState<string | null>(null);

  const pullEvents = useCallback(async () => {
    const fresh = await api.eventsSince(lastSeq.current);
    if (fresh.length === 0) return;
    lastSeq.current = fresh[fresh.length - 1].seq;
    setEvents((old) => [...old, ...fresh].slice(-MAX_EVENTS));
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
      .modelProviders()
      .then((ps) => {
        const p = ps.find((x) => x.model);
        setModel(p ? `${p.label} · ${p.model}` : null);
      })
      .catch(() => setModel(null));
  }, []);

  useEffect(() => {
    api
      .recentProjects(6)
      .then(setRecent)
      .catch(() => setRecent([]));
  }, [initial.project.id, dialog]);

  // Airwires come from the engine's connectivity analysis (the one DRC-004 uses), per revision.
  useEffect(() => {
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

  const guarded = useCallback(
    async (f: () => Promise<void>) => {
      if (busy) return;
      setBusy(true);
      try {
        await f();
      } catch (e) {
        setMessage(String(e));
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
  // "Update PCB from schematic": the typed SyncSchematicToPcb command, then show the PCB where the
  // new footprints (staged beside the board) and the ratsnest airwires appear.
  const syncPcb = () =>
    guarded(async () => {
      const r = await api.execute(revision, { type: "SyncSchematicToPcb" });
      setMessage(
        r.accepted
          ? `Update PCB → revision ${r.revision_after}. Footprints are staged beside the board: use Auto-place or the Move tool; airwires show the unrouted connections.`
          : `Update PCB rejected: ${r.error ? describeError(r.error) : "unknown error"}`,
      );
      await refresh();
      if (r.accepted) {
        const d = availableDocs(snapshot).find((x) => x.kind === "pcb");
        if (d) setDocState((st) => openDoc({ open: st.open.includes(d.id) ? st.open : [...st.open, d.id], active: st.active }, d.id));
      }
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
      const r = await api.exportManufacturing();
      setExported(r);
      setMessage(`Exported ${r.manifest.files.length} files to ${r.out_dir} (${r.manifest.status})`);
      await pullEvents();
    });
  const undo = () => submit(() => api.undo(revision));
  const redo = () => submit(() => api.redo(revision));
  const check = (c: Check) =>
    guarded(async () => {
      const result = await api.runCheck(c);
      setResults((old) => ({ ...old, [c]: result }));
      setBottomTab("findings");
      setBottomOpen(true);
      setMessage(`${result.report.check} ran on revision ${result.report.revision}`);
      await pullEvents();
    });
  const close = () =>
    guarded(async () => {
      await api.closeProject();
      onClosed();
    });
  const acknowledge = () =>
    guarded(async () => {
      setSnapshot(await api.acknowledgeRecovery());
      await pullEvents();
    });

  const selectedSymbol = selected ? snapshot.project.schematic.symbols[selected] : undefined;
  const selLabel = selectedSymbol ? `${selectedSymbol.refdes} (${selectedSymbol.value})` : null;
  const pending = chat.find((m) => m.status === "pending")?.proposal;
  const previewIds = pending?.targets ?? [];

  const showSide = (v: SideView) => {
    if (sideOpen && sideView === v) setSideOpen(false);
    else {
      setSideView(v);
      setSideOpen(true);
    }
  };

  // Ask the assistant: a user line, then either a custom reply or the local interpreter's answer.
  const ask = (text: string, opts: { select?: Id | null; reply?: string } = {}) => {
    const sel = opts.select ?? selected;
    if (opts.select) setSelected(opts.select);
    setAiOpen(true);
    setAiFocus((n) => n + 1);
    const reply = opts.reply !== undefined ? { text: opts.reply } : interpret(text, { snapshot, selected: sel, library });
    setChat((old) => [
      ...old.map((m): ChatMsg => (m.status === "pending" ? { ...m, status: "rejected" } : m)),
      { id: msgId.current++, role: "user", text },
      { id: msgId.current++, role: "assistant", text: reply.text, proposal: (reply as { proposal?: Proposal }).proposal, status: (reply as { proposal?: Proposal }).proposal ? "pending" : undefined },
    ]);
  };
  const askSelection = () =>
    selectedSymbol &&
    ask(`About ${selLabel}`, {
      reply: `${selectedSymbol.refdes} (${selectedSymbol.value}, ${selectedSymbol.component_id}) is selected. I can rotate it, set its value, or delete it — say which, and you will see a preview first.`,
    });
  const askFinding = (text: string, target: Id | null) =>
    ask(`Finding: ${text}`, {
      select: target,
      reply: `Finding noted: ${text}. I have no automatic fix for this rule. ${target ? "The related part is selected; ask me to rotate it, set its value or delete it, or edit it on the canvas." : "No part is attached to this finding."}`,
    });
  const askNet = (name: string) => {
    const fps = snapshot.geometry.footprints;
    const pads = fps.flatMap((f) => f.pads.filter((p) => p.net === name).map(() => 1)).length;
    const tracks = Object.values(snapshot.project.pcb.tracks).filter((t) => t.net === name).length;
    ask(`About net ${name}`, { reply: `Net ${name}: ${pads} pad(s) and ${tracks} track segment(s) on the PCB. Ask me to auto-route all nets, or route it by hand with the Route tool.` });
  };
  const acceptProposal = (m: ChatMsg) =>
    guarded(async () => {
      const p = m.proposal;
      if (!p) return;
      if (p.destructive && !window.confirm(`${p.title}: this is a destructive change. Continue?`)) return;
      let rev = revision;
      let last: CommandResponse | null = null;
      for (const c of p.commands) {
        last = await api.execute(rev, c);
        if (!last.accepted) break;
        rev = last.revision_after;
      }
      const ok = !!last?.accepted;
      setChat((old) => old.map((x) => (x.id === m.id ? { ...x, status: ok ? "accepted" : "rejected" } : x)));
      setMessage(
        ok
          ? `${p.title} applied → revision ${last!.revision_after}`
          : `${p.title} rejected by the engine: ${last?.error ? describeError(last.error) : "unknown error"}`,
      );
      await refresh();
    });
  const rejectProposal = (m: ChatMsg) => setChat((old) => old.map((x) => (x.id === m.id ? { ...x, status: "rejected" } : x)));

  const open = (id: string) => setDocState((st) => openDoc({ open: openIds, active: st.active }, id));
  const showKind = (kind: "schematic" | "pcb") => {
    if (shown.some((d) => d.kind === kind)) return;
    const d = docs.find((x) => x.kind === kind);
    if (d) open(d.id);
  };
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
    { id: "dfm", label: "Run DFM (JLCPCB standard profile)", run: () => check("dfm"), group: "Verify" },
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
    { id: "quick-open", label: "Go to part, net or sheet…", run: () => setQuickOpen(true), shortcut: SHORTCUTS.quickOpen, group: "Navigate" },
    { id: "rules", label: "Design rules and net classes…", run: () => setDialog("rules"), group: "Design" },
    { id: "tab-split", label: "View: Toggle split (schematic + PCB)", run: () => setSplit((v) => !v), group: "View" },
    ...docs.map((d) => ({ id: `open-${d.id}`, label: `Open ${d.kind === "pcb" ? "PCB" : `schematic ${d.title}`}`, run: () => open(d.id), group: "Documents" })),
    { id: "close-all", label: "Close all documents", run: closeAll, disabled: openIds.length === 0, group: "Documents" },
    { id: "reset-layout", label: "Reset panel layout", run: resetLayout, group: "View" },
    { id: "help", label: "Keyboard shortcuts", run: () => setHelp(true), group: "Help" },
    { id: "toggle-side", label: "Toggle side panel", run: () => setSideOpen((o) => !o), shortcut: SHORTCUTS.toggleSide, group: "View" },
    { id: "toggle-bottom", label: "Toggle bottom panel", run: () => setBottomOpen((o) => !o), shortcut: SHORTCUTS.toggleBottom, group: "View" },
    { id: "toggle-ai", label: "Toggle AI assistant", run: () => setAiOpen((o) => !o), shortcut: SHORTCUTS.toggleAi, group: "View" },
    ...SIDE_VIEWS.map((v) => ({ id: `show-${v.id}`, label: `Show ${v.label}`, run: () => showSide(v.id), group: "View" })),
    { id: "bom", label: "Show BOM", run: () => (setBottomTab("bom"), setBottomOpen(true)), group: "View" },
    { id: "sim", label: "Show simulation", run: () => (setBottomTab("sim"), setBottomOpen(true)), group: "View" },
    { id: "log", label: "Show log", run: () => (setBottomTab("log"), setBottomOpen(true)), group: "View" },
    { id: "ask", label: selLabel ? `Ask AI about ${selLabel}` : "Ask AI", run: () => (selectedSymbol ? askSelection() : (setAiOpen(true), setAiFocus((n) => n + 1))), group: "AI" },
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
    ...netNames(snapshot).map((n) => ({ id: `net-${n}`, label: n, group: "Net", run: () => askNet(n) })),
    ...Object.values(snapshot.project.schematic.sheets).map((sh) => ({
      id: `sheet-${sh.id}`,
      label: sh.name,
      group: "Sheet",
      run: () => showKind("schematic"),
    })),
  ];

  // Keep the latest handlers reachable from the single global key listener.
  const keys = useRef({ undo, redo });
  keys.current = { undo, redo };
  const saveRef = useRef<() => void>(() => {});
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
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
      } else if (matches(e, SHORTCUTS.toggleAi)) {
        e.preventDefault();
        setAiOpen((o) => !o);
        setAiFocus((n) => n + 1);
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
      { id: "new", label: "New project", run: () => setDialog("new"), disabled: busy ? "An engine command is running" : undefined },
      { id: "open", label: "Open project", run: () => setDialog("open"), disabled: busy ? "An engine command is running" : undefined },
      { id: "save", label: "Save", shortcut: SHORTCUTS.save, run: save, disabled: busy ? "An engine command is running" : undefined },
      { id: "saveas", label: "Save as", disabled: "Not implemented: the engine saves the project in place" },
    ],
    [
      { id: "undo", label: "Undo", shortcut: SHORTCUTS.undo, run: undo, disabled: busy ? "An engine command is running" : !snapshot.can_undo ? "Nothing to undo" : undefined },
      { id: "redo", label: "Redo", shortcut: SHORTCUTS.redo, run: redo, disabled: busy ? "An engine command is running" : !snapshot.can_redo ? "Nothing to redo" : undefined },
    ],
    [
      { id: "select", label: "Select tool", title: "Select tool (key 1)", run: toolTo("select"), disabled: noDoc },
      { id: "move", label: "Move tool", title: "Move tool (schematic key 6, PCB key 3)", run: toolTo("move"), disabled: noDoc },
      { id: "rotate", label: "Rotate selection", title: "Rotate selection (key R)", run: rotateSel, disabled: needSel ?? (busy ? "An engine command is running" : undefined) },
      { id: "delete", label: "Delete selection", title: "Delete selection (asks first)", run: deleteSel, disabled: needSel ?? (busy ? "An engine command is running" : undefined) },
    ],
    [
      { id: "zout", label: "Zoom out", run: () => canvasControls(activeId)?.zoomBy(1.4), disabled: noDoc },
      { id: "zin", label: "Zoom in", run: () => canvasControls(activeId)?.zoomBy(1 / 1.4), disabled: noDoc },
      { id: "fit", label: "Fit to view", title: "Fit to view (or double-click the canvas)", run: () => canvasControls(activeId)?.fit(), disabled: noDoc },
      { id: "grid", label: "Grid", disabled: "Not implemented: the grid is fixed (schematic 1.27 mm, PCB 0.5 mm)" },
    ],
    [
      { id: "rules", label: "Design rules", title: "Design rules and net classes", run: () => setDialog("rules") },
      { id: "sync", label: "Update PCB", title: "Update PCB from schematic", run: syncPcb, disabled: busy ? "An engine command is running" : undefined },
      { id: "erc", label: "Run ERC", run: () => check("erc"), disabled: busy ? "An engine command is running" : undefined },
      { id: "drc", label: "Run DRC", run: () => check("drc"), disabled: busy ? "An engine command is running" : undefined },
      { id: "dfm", label: "Run DFM", title: "Design for manufacture against the JLCPCB standard profile", run: () => check("dfm"), disabled: busy ? "An engine command is running" : undefined },
      { id: "route", label: "Auto-route", title: "Auto-route all nets", run: () => execute({ type: "AutoRoute", nets: null }), disabled: busy ? "An engine command is running" : undefined },
      { id: "sim", label: "Simulate", disabled: "Not implemented: no SPICE integration yet" },
      { id: "export", label: "Export manufacturing files", title: "Gerber, Excellon, BOM, CPL, fab-pack ZIP; runs ERC/DRC/MECH/DFM", run: () => setDialog("export"), disabled: busy ? "An engine command is running" : undefined },
    ],
    [
      { id: "cmd", label: "Commands", shortcut: SHORTCUTS.palette, run: () => setPalette(true) },
      { id: "ai", label: "Assistant", shortcut: SHORTCUTS.toggleAi, run: () => setAiOpen((o) => !o), pressed: aiOpen },
      { id: "close", label: "Close project", run: close, disabled: busy ? "An engine command is running" : undefined },
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
      { label: "AI assistant", shortcut: SHORTCUTS.toggleAi, checked: aiOpen, run: () => setAiOpen((o) => !o) },
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
      { label: "Run DFM (JLCPCB standard)", run: () => check("dfm"), disabled: noProject },
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
    { label: "AI Assistant", items: [
      { label: "Show assistant", shortcut: SHORTCUTS.toggleAi, run: () => { setAiOpen(true); setAiFocus((n) => n + 1); } },
      { label: "Ask about selection", run: askSelection, disabled: needSel },
      { label: "Clear conversation", run: () => setChat([]), disabled: chat.length === 0 ? "The conversation is empty" : undefined },
    ] },
    { label: "Settings", items: [
      { label: "Model settings", run: onSettings },
      { label: theme === "dark" ? "Switch to light theme" : "Switch to dark theme", run: onTheme },
      { label: "Reset panel layout", run: resetLayout },
    ] },
    { label: "Help", items: [
      { label: "Keyboard shortcuts", run: () => setHelp(true) },
      { label: "About EAK", run: () => setMessage("EAK: Rust engineering core, typed commands, independent validation. The assistant never certifies engineering numbers.") },
    ] },
  ];
  const noParts = Object.keys(snapshot.project.schematic.symbols).length === 0;

  return (
    <div className="workspace">
      <MenuBar menus={menus} />
      <Toolbar
        groups={toolbarGroups}
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
      <div className="main" style={{ gridTemplateColumns: `48px ${sideOpen ? layout.sideWidth : 0}px ${sideOpen ? 4 : 0}px minmax(0, 1fr) ${aiOpen ? 4 : 0}px ${aiOpen ? layout.aiWidth : 0}px` }}>
        <ActivityBar view={sideView} open={sideOpen} onSelect={showSide} onSettings={onSettings} theme={theme} onTheme={onTheme} />
        <aside className={sideOpen ? "side open" : "side"} aria-label="Side panel" aria-hidden={!sideOpen}>
          <header className="panel-head">
            <span className="title">{SIDE_VIEWS.find((v) => v.id === sideView)?.label}</span>
          </header>
          {sideView === "project" && (
            <ProjectPanel
              snapshot={snapshot}
              selected={selected}
              onSelect={setSelected}
              onNet={askNet}
              onSheet={(id) => open(`sch:${id}`)}
            />
          )}
          {sideView === "layers" && (
            <LayerManager snapshot={snapshot} view={layerView} unrouted={conn && conn.revision === revision ? conn.ratsnest.length : null} edit={edit.execute} busy={busy} />
          )}
          {sideView === "parts" && <LibraryPanel revision={revision} busy={busy} execute={edit.execute} onPlace={placePart} />}
          {sideView === "domain" && (
            <div className="panel-body">
              <DomainPanel report={domain} revision={revision} error={domainError} />
            </div>
          )}
          {sideView === "manufacturing" && <ManufacturingPanel result={exported} busy={busy} onExport={() => setDialog("export")} />}
        </aside>
        {sideOpen ? (
          <Splitter orientation="vertical" label="Resize side panel" value={layout.sideWidth} onResize={(d) => setLayout((l) => ({ ...l, sideWidth: clampSize("sideWidth", l.sideWidth + d) }))} onReset={() => setLayout((l) => ({ ...l, sideWidth: DEFAULT_LAYOUT.sideWidth }))} />
        ) : (
          <div />
        )}
        <div className="center">
          <DocTabs docs={openDocs} active={activeId} dirty={dirty} onSelect={open} onClose={closeOne} onCloseAll={closeAll} split={split} onSplit={() => setSplit((v) => !v)} askLabel={selectedSymbol?.refdes ?? null} onAsk={askSelection} />
          <div className={`views ${shown.length > 1 ? "split" : "single"}`}>
            {openDocs.length === 0 && (
              <div className="canvas-empty static" aria-label="No documents open">
                <strong>No documents open</strong>
                <span>Open the schematic or the PCB from the Project panel or the Project menu.</span>
              </div>
            )}
            {shown.map((d) => (
              <DocIdContext.Provider key={d.id} value={d.id}>
              <div className="view-slot" onPointerDownCapture={() => d.id !== activeId && open(d.id)}>
              {d.kind === "schematic" ? (
                <SchematicView key={d.id} snapshot={snapshot} selected={selected} onSelect={setSelected} edit={edit} aiPreview={previewIds} armPart={armPart} sheetId={d.ref} />
              ) : (
                <PcbView key={d.id} snapshot={snapshot} selected={selected} onSelect={setSelected} edit={edit} aiPreview={previewIds} onStatus={setStatus} layerView={layerView} focus={focus} airwires={conn && conn.revision === revision ? conn.ratsnest : null} />
              )}
              </div>
              </DocIdContext.Provider>
            ))}
            {noParts && (
              <div className="canvas-empty" aria-label="Empty design">
                <strong>An empty design</strong>
                <span>Open Library / Parts to place a part, press {formatShortcut(SHORTCUTS.quickOpen)} to go anywhere, or ask the assistant.</span>
              </div>
            )}
          </div>
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
              onAsk={askFinding}
              onNavigate={navigateFinding}
              activeFinding={activeFinding}
              onClose={() => setBottomOpen(false)}
            />
          )}
        </div>
        {aiOpen ? (
          <Splitter orientation="vertical" label="Resize assistant panel" value={layout.aiWidth} onResize={(d) => setLayout((l) => ({ ...l, aiWidth: clampSize("aiWidth", l.aiWidth - d) }))} onReset={() => setLayout((l) => ({ ...l, aiWidth: DEFAULT_LAYOUT.aiWidth }))} />
        ) : (
          <div />
        )}
        {aiOpen && (
          <AiPanel
            messages={chat}
            snapshot={snapshot}
            library={library}
            contextLabel={selLabel}
            busy={busy}
            modelLabel={model}
            focusNonce={aiFocus}
            onSend={(t) => ask(t)}
            onAccept={acceptProposal}
            onReject={rejectProposal}
            onClose={() => setAiOpen(false)}
          />
        )}
      </div>
      <StatusBar
        project={snapshot.project.name}
        revision={revision}
        layer={status.layer === "top" ? "Top" : "Bottom"}
        gridMm={status.gridMm}
        drc={results.drc}
        model={model}
        selection={selLabel ? `Selected ${selLabel}` : null}
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
