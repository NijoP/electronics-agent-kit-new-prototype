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
import { formatShortcut, matches, SHORTCUTS, tip } from "./design/shortcuts";
import type { Theme } from "./design/theme";
import { ActivityBar, SIDE_VIEWS, type SideView } from "./shell/ActivityBar";
import { AiPanel, type ChatMsg } from "./shell/AiPanel";
import { interpret, type Proposal } from "./shell/ai";
import { BottomPanel, type BottomTab } from "./shell/BottomPanel";
import { ManufacturingPanel, netNames, PartsPanel, ProjectPanel } from "./shell/panels";
import { StatusBar } from "./shell/StatusBar";
import type { Check, CheckResult, Command, CommandResponse, CopperLayer, DomainReport, EngineEvent, ExportResult, Id, LibraryEntry, Snapshot } from "./types";

interface Props {
  initial: Snapshot;
  onClosed: () => void;
  onSettings: () => void;
  theme: Theme;
  onTheme: () => void;
}

type EditorTab = "split" | "schematic" | "pcb";

const MAX_EVENTS = 200;

export function Workspace({ initial, onClosed, onSettings, theme, onTheme }: Props) {
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
  const [tab, setTab] = useState<EditorTab>("split");
  const [sideOpen, setSideOpen] = useState(true);
  const [sideView, setSideView] = useState<SideView>("project");
  const [bottomOpen, setBottomOpen] = useState(true);
  const [bottomTab, setBottomTab] = useState<BottomTab>("log");
  const [aiOpen, setAiOpen] = useState(true);
  const [partQuery, setPartQuery] = useState("");
  const [quickOpen, setQuickOpen] = useState(false);
  const [armPart, setArmPart] = useState<{ id: string; n: number } | null>(null);
  const [chat, setChat] = useState<ChatMsg[]>([]);
  const [aiFocus, setAiFocus] = useState(0);
  const [status, setStatus] = useState<{ layer: CopperLayer; gridMm: number }>({ layer: "top", gridMm: 0.5 });
  const [model, setModel] = useState<string | null>(null);
  const msgId = useRef(1);
  const revision = snapshot.project.revision;
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

  const placePart = (c: LibraryEntry) => {
    setTab((t) => (t === "pcb" ? "schematic" : t));
    setArmPart({ id: c.id, n: Date.now() });
    setMessage(`Place tool armed with ${c.name}: click the schematic to place it.`);
  };

  const actions: PaletteAction[] = [
    { id: "undo", label: "Undo", run: undo, disabled: !snapshot.can_undo, shortcut: SHORTCUTS.undo, group: "Edit" },
    { id: "redo", label: "Redo", run: redo, disabled: !snapshot.can_redo, shortcut: SHORTCUTS.redo, group: "Edit" },
    { id: "export", label: "Export Gerbers + drill", run: exportGerbers, group: "Manufacturing" },
    { id: "erc", label: "Run ERC", run: () => check("erc"), group: "Verify" },
    { id: "drc", label: "Run DRC", run: () => check("drc"), group: "Verify" },
    { id: "annotate", label: "Annotate schematic", run: () => execute({ type: "Annotate" }), group: "Design" },
    { id: "sync", label: "Sync schematic to PCB", run: () => execute({ type: "SyncSchematicToPcb" }), group: "Design" },
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
    { id: "tab-split", label: "View: Schematic + PCB", run: () => setTab("split"), group: "View" },
    { id: "tab-sch", label: "View: Schematic", run: () => setTab("schematic"), group: "View" },
    { id: "tab-pcb", label: "View: PCB", run: () => setTab("pcb"), group: "View" },
    { id: "toggle-side", label: "Toggle side panel", run: () => setSideOpen((o) => !o), shortcut: SHORTCUTS.toggleSide, group: "View" },
    { id: "toggle-bottom", label: "Toggle bottom panel", run: () => setBottomOpen((o) => !o), shortcut: SHORTCUTS.toggleBottom, group: "View" },
    { id: "toggle-ai", label: "Toggle AI assistant", run: () => setAiOpen((o) => !o), shortcut: SHORTCUTS.toggleAi, group: "View" },
    ...SIDE_VIEWS.map((v) => ({ id: `show-${v.id}`, label: `Show ${v.label}`, run: () => showSide(v.id), group: "View" })),
    { id: "bom", label: "Show BOM", run: () => (setBottomTab("bom"), setBottomOpen(true)), group: "View" },
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
      run: () => setTab((t) => (t === "pcb" ? "schematic" : t)),
    })),
  ];

  // Keep the latest handlers reachable from the single global key listener.
  const keys = useRef({ undo, redo });
  keys.current = { undo, redo };
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
        setMessage(`Saved: every accepted command is already on disk (revision ${revision}).`);
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

  const TABS: [EditorTab, string][] = [
    ["split", "Schematic + PCB"],
    ["schematic", "Schematic"],
    ["pcb", "PCB"],
  ];
  const noParts = Object.keys(snapshot.project.schematic.symbols).length === 0;

  return (
    <div className="workspace">
      <header className="toolbar">
        <strong>{snapshot.project.name}</strong>
        <span className="muted">revision {revision}</span>
        <span className="saved" title="Every accepted command is written to disk atomically before it returns">
          Saved · r{revision}
        </span>
        <span className="spacer" />
        <button type="button" onClick={undo} disabled={busy || !snapshot.can_undo} title={tip("Undo", SHORTCUTS.undo)}>
          Undo
        </button>
        <button type="button" onClick={redo} disabled={busy || !snapshot.can_redo} title={tip("Redo", SHORTCUTS.redo)}>
          Redo
        </button>
        <button type="button" onClick={() => check("erc")} disabled={busy}>
          Run ERC
        </button>
        <button type="button" onClick={() => check("drc")} disabled={busy}>
          Run DRC
        </button>
        <button type="button" onClick={exportGerbers} disabled={busy} title="Gerber, Excellon, BOM, CPL">
          Export Gerbers
        </button>
        <button type="button" onClick={() => setPalette(true)} title={tip("Command palette", SHORTCUTS.palette)}>
          Commands
        </button>
        <button type="button" onClick={() => setAiOpen((o) => !o)} aria-pressed={aiOpen} title={tip("AI assistant", SHORTCUTS.toggleAi)}>
          Assistant
        </button>
        <button type="button" onClick={close} disabled={busy}>
          Close project
        </button>
      </header>
      {snapshot.blocked && (
        <div role="alert" className="banner">
          Writes are blocked: {snapshot.blocked}
          <button type="button" onClick={acknowledge} disabled={busy}>
            Accept on-disk state
          </button>
        </div>
      )}
      <div className="main" style={{ gridTemplateColumns: `48px ${sideOpen ? "272px" : "0px"} minmax(0, 1fr) ${aiOpen ? "336px" : "0px"}` }}>
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
              onSheet={() => setTab((t) => (t === "pcb" ? "schematic" : t))}
            />
          )}
          {sideView === "parts" && <PartsPanel library={library} query={partQuery} onQuery={setPartQuery} onPlace={placePart} />}
          {sideView === "domain" && (
            <div className="panel-body">
              <DomainPanel report={domain} revision={revision} error={domainError} />
            </div>
          )}
          {sideView === "manufacturing" && <ManufacturingPanel result={exported} busy={busy} onExport={exportGerbers} />}
        </aside>
        <div className="center">
          <div className="tabs editor-tabs" role="tablist" aria-label="Editors">
            {TABS.map(([id, label]) => (
              <button key={id} type="button" role="tab" aria-selected={tab === id} className={tab === id ? "tab on" : "tab"} onClick={() => setTab(id)}>
                {label}
              </button>
            ))}
            <span className="spacer" />
            {selectedSymbol && (
              <button type="button" className="ask-chip" onClick={askSelection} title="Ask the assistant about the selected part">
                Ask AI about {selectedSymbol.refdes}
              </button>
            )}
          </div>
          <div className={`views ${tab}`}>
            {tab !== "pcb" && (
              <SchematicView snapshot={snapshot} selected={selected} onSelect={setSelected} edit={edit} aiPreview={previewIds} armPart={armPart} />
            )}
            {tab !== "schematic" && (
              <PcbView snapshot={snapshot} selected={selected} onSelect={setSelected} edit={edit} aiPreview={previewIds} onStatus={setStatus} />
            )}
            {noParts && (
              <div className="canvas-empty" aria-label="Empty design">
                <strong>An empty design</strong>
                <span>Open Library / Parts to place a part, press {formatShortcut(SHORTCUTS.quickOpen)} to go anywhere, or ask the assistant.</span>
              </div>
            )}
          </div>
          {bottomOpen && (
            <BottomPanel
              tab={bottomTab}
              onTab={setBottomTab}
              snapshot={snapshot}
              results={results}
              events={events}
              onProbe={setSelected}
              onAsk={askFinding}
              onClose={() => setBottomOpen(false)}
            />
          )}
        </div>
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
