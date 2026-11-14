// Project workspace: toolbar, schematic + PCB views with cross-probing, Engineering
// Monitor, activity log, status bar and command palette. State shown here is always the
// latest engine snapshot; edits are engine calls carrying the rendered revision.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, describeError } from "./api";
import { DomainPanel } from "./DomainPanel";
import { CommandPalette, type PaletteAction } from "./CommandPalette";
import type { EditCtx } from "./EditContext";
import { EngineeringMonitor } from "./EngineeringMonitor";
import { ExportPanel } from "./ExportPanel";
import { ManufacturingPanel } from "./ManufacturingPanel";
import { PartsPanel } from "./PartsPanel";
import { PcbView } from "./PcbView";
import { SchematicView } from "./SchematicView";
import type { Check, CheckResult, Command, CommandResponse, DomainReport, EngineEvent, ExportResult, Id, LibraryEntry, Snapshot } from "./types";

interface Props {
  initial: Snapshot;
  onClosed: () => void;
}

const MAX_EVENTS = 200;

export function Workspace({ initial, onClosed }: Props) {
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
  const [layout, setLayout] = useState<"both" | "schematic" | "pcb">("both");
  const [panel, setPanel] = useState<"parts" | "mfg" | null>(null);
  const revision = snapshot.project.revision;
  // Built-in parts plus parts imported into this project (e.g. from LCSC), so an imported
  // part can be placed from the editors' part picker.
  const fullLibrary = useMemo(() => {
    const known = new Set(library.map((l) => l.id));
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
    return [...library, ...imported];
  }, [library, snapshot.project.library]);
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
  const edit: EditCtx = { execute: executeAll, busy, library: fullLibrary, notify: setMessage };
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

  const actions: PaletteAction[] = [
    { id: "undo", label: "Undo", run: undo, disabled: !snapshot.can_undo },
    { id: "redo", label: "Redo", run: redo, disabled: !snapshot.can_redo },
    { id: "export", label: "Export Gerbers + drill", run: exportGerbers },
    { id: "erc", label: "Run ERC", run: () => check("erc") },
    { id: "drc", label: "Run DRC", run: () => check("drc") },
    { id: "annotate", label: "Annotate schematic", run: () => execute({ type: "Annotate" }) },
    { id: "sync", label: "Sync schematic to PCB", run: () => execute({ type: "SyncSchematicToPcb" }) },
    { id: "place", label: "Auto-place footprints", run: () => execute({ type: "AutoPlace", grid: null }) },
    { id: "route", label: "Auto-route all nets", run: () => execute({ type: "AutoRoute", nets: null }) },
    {
      id: "clear",
      label: "Clear all routing",
      run: () => execute({ type: "ClearRouting", net: null }),
      destructive: true,
    },
    { id: "deselect", label: "Clear selection", run: () => setSelected(null) },
    { id: "close", label: "Close project", run: close },
  ];

  // Keep the latest handlers reachable from the single global key listener.
  const keys = useRef({ undo, redo });
  keys.current = { undo, redo };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.ctrlKey || e.metaKey;
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement;
      if (mod && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPalette(true);
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
  }, []);

  const selectedSymbol = selected ? snapshot.project.schematic.symbols[selected] : undefined;

  return (
    <div className="workspace">
      <header className="toolbar">
        <strong>{snapshot.project.name}</strong>
        <span className="muted">revision {revision}</span>
        <span className="saved" title="Every accepted command is written to disk atomically before it returns">
          Saved · r{revision}
        </span>
        <select aria-label="Layout" value={layout} onChange={(e) => setLayout(e.target.value as typeof layout)}>
          <option value="both">Schematic + PCB</option>
          <option value="schematic">Schematic only</option>
          <option value="pcb">PCB only</option>
        </select>
        <button type="button" onClick={undo} disabled={busy || !snapshot.can_undo} title="Undo (Ctrl+Z)">
          Undo
        </button>
        <button type="button" onClick={redo} disabled={busy || !snapshot.can_redo} title="Redo (Ctrl+Shift+Z)">
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
        <button type="button" onClick={() => setPanel(panel === "parts" ? null : "parts")} aria-pressed={panel === "parts"}>
          Parts
        </button>
        <button type="button" onClick={() => setPanel(panel === "mfg" ? null : "mfg")} aria-pressed={panel === "mfg"}>
          Manufacturing
        </button>
        <button type="button" onClick={() => setPalette(true)} title="Ctrl+K">
          Commands
        </button>
        <span className="spacer" />
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
      <div className="main">
        <div className={`views ${layout}`}>
          {layout !== "pcb" && <SchematicView snapshot={snapshot} selected={selected} onSelect={setSelected} edit={edit} />}
          {layout !== "schematic" && <PcbView snapshot={snapshot} selected={selected} onSelect={setSelected} edit={edit} />}
        </div>
        <aside className="side">
          <EngineeringMonitor snapshot={snapshot} results={results} onProbe={setSelected} />
          {panel === "parts" && (
            <PartsPanel
              revision={revision}
              onImported={async (m) => {
                setMessage(m);
                await refresh();
              }}
            />
          )}
          {panel === "mfg" && <ManufacturingPanel revision={revision} onPrepared={setMessage} />}
          <DomainPanel report={domain} revision={revision} error={domainError} />
          <ExportPanel result={exported} />
          <section className="activity" aria-label="Activity">
            <h2>Activity</h2>
            <ol>
              {events
                .slice()
                .reverse()
                .map((e) => (
                  <li key={e.seq}>
                    <span className="muted">r{e.revision}</span> {e.summary} <span className="muted">({e.actor})</span>
                  </li>
                ))}
            </ol>
          </section>
        </aside>
      </div>
      <footer className="status" role="status">
        <span>{snapshot.dir}</span>
        <span>
          {selectedSymbol
            ? `Selected ${selectedSymbol.refdes} (${selectedSymbol.value})`
            : "Nothing selected"}
        </span>
        <span>{busy ? "Working…" : message}</span>
      </footer>
      {palette && <CommandPalette actions={actions} onClose={() => setPalette(false)} />}
    </div>
  );
}
