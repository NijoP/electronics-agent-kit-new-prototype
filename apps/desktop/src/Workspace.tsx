// Project workspace: toolbar, schematic + PCB views with cross-probing, Engineering
// Monitor, activity log, status bar and command palette. State shown here is always the
// latest engine snapshot; edits are engine calls carrying the rendered revision.
import { useCallback, useEffect, useRef, useState } from "react";
import { api, describeError } from "./api";
import { CommandPalette, type PaletteAction } from "./CommandPalette";
import { EngineeringMonitor } from "./EngineeringMonitor";
import { PcbView } from "./PcbView";
import { SchematicView } from "./SchematicView";
import type { Check, CheckResult, Command, CommandResponse, EngineEvent, Id, Snapshot } from "./types";

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
  const revision = snapshot.project.revision;

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
        <div className="views">
          <SchematicView snapshot={snapshot} selected={selected} onSelect={setSelected} />
          <PcbView snapshot={snapshot} selected={selected} onSelect={setSelected} />
        </div>
        <aside className="side">
          <EngineeringMonitor snapshot={snapshot} results={results} onProbe={setSelected} />
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
