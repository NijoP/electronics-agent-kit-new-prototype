// Design manager (Phase 1 A3): the left dock's Pages / Nets / Components / Objects lists with a
// search box. Clicking an entry asks the workspace to open, select and zoom to it (`onNavigate`).
// Nets come from the engine's netlist, refetched per revision; unrouted counts from its
// connectivity report.
//
// Pages tab (Phase 1 B6, ADR-0043): Add sheet, and per sheet Rename, Move up / Move down and
// Delete. Each is one typed engine command through `onCommand`; a non-empty sheet is deleted, after
// a confirmation that lists what is on it, by one Batch that removes its contents first.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { api } from "../api";
import type { Command, ConnectivityReport, Id, Snapshot } from "../types";
import { componentsOf, filterItems, MANAGER_TABS, netsOf, objectsOf, pagesOf, type ManagerItem, type ManagerTab, type NetRow, type Target } from "./designManagerData";
import { deleteSheetCommand, moveSheetCommand, nextSheetName, renameProblem, sheetContents, sheetsInOrder } from "./sheets";

interface Props {
  snapshot: Snapshot;
  conn: ConnectivityReport | null;
  selected: Id | null;
  activeDoc: string | null;
  tab: ManagerTab;
  onTab: (t: ManagerTab) => void;
  onNavigate: (t: Target, netlist: NetRow[] | null) => void;
  /** Submit a typed sheet command (Pages tab); without it no sheet action is shown. */
  onCommand?: (c: Command) => void;
  /** A command is in flight: the sheet actions wait for it. */
  busy?: boolean;
}

/** The engine's netlist for this revision (null while it loads or when it is unavailable). */
function useNetlist(revision: number): { rows: NetRow[] | null; error: string | null } {
  const [state, setState] = useState<{ rows: NetRow[] | null; error: string | null }>({ rows: null, error: null });
  useEffect(() => {
    let live = true;
    api
      .netlist()
      .then((r) => live && setState({ rows: r, error: null }))
      .catch((e) => live && setState({ rows: null, error: String(e) }));
    return () => {
      live = false;
    };
  }, [revision]);
  return state;
}

const EMPTY: Record<ManagerTab, string> = {
  pages: "No sheets or board in this project.",
  nets: "No nets yet: connect pins or add net labels in the schematic.",
  components: "No components yet: place a part from the Library tab of the bottom panel, or press Ctrl+P.",
  objects: "No wires, labels, tracks, vias or zones yet.",
};

const WAIT = "Waiting for the engine to finish the last command";

export function DesignManager({ snapshot, conn, selected, activeDoc, tab, onTab, onNavigate, onCommand, busy = false }: Props) {
  const [query, setQuery] = useState("");
  const [renaming, setRenaming] = useState<Id | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<Id | null>(null);
  const netlist = useNetlist(snapshot.project.revision);
  const sch = snapshot.project.schematic;
  const order = sheetsInOrder(sch).map((x) => x.id);
  const label = MANAGER_TABS.find((t) => t.id === tab)!.label;
  const all: ManagerItem[] =
    tab === "pages" ? pagesOf(snapshot) : tab === "nets" ? netsOf(snapshot, netlist.rows, conn) : tab === "components" ? componentsOf(snapshot) : objectsOf(snapshot);
  const items = filterItems(all, query);
  const isOn = (i: ManagerItem) => (i.target.kind === "page" ? i.target.docId === activeDoc : i.target.kind === "component" ? i.target.symbolId === selected : false);
  const rows: ReactNode[] = [];
  let lastGroup: string | undefined;
  for (const i of items) {
    if (i.group && i.group !== lastGroup)
      rows.push(
        <li className="dm-group" key={`g:${i.group}`} aria-hidden="true">
          {i.group}
        </li>,
      );
    lastGroup = i.group;
    const nav = (
      <button type="button" className={isOn(i) ? "on" : ""} title={i.target.kind === "page" ? "Open" : "Select and zoom to it"} onClick={() => onNavigate(i.target, netlist.rows)}>
        <strong>{i.label}</strong> <span className="muted">{i.detail}</span>
      </button>
    );
    const sid = onCommand ? i.sheetId : undefined;
    rows.push(
      <li key={i.key}>
        {sid && onCommand && renaming === sid ? (
          <RenameForm
            snapshot={snapshot}
            sheetId={sid}
            busy={busy}
            onCancel={() => setRenaming(null)}
            onSubmit={(name) => {
              setRenaming(null);
              onCommand({ type: "RenameSheet", sheet_id: sid, name });
            }}
          />
        ) : sid && onCommand ? (
          <div className="dm-row">
            {nav}
            <SheetActions
              name={i.label}
              index={order.indexOf(sid)}
              count={order.length}
              busy={busy}
              onRename={() => setRenaming(sid)}
              onMove={(by) => {
                const c = moveSheetCommand(sch, sid, by);
                if (c) onCommand(c);
              }}
              onDelete={() => (sheetContents(snapshot, sid).length > 0 ? setConfirmDelete(sid) : onCommand(deleteSheetCommand(snapshot, sid)))}
            />
          </div>
        ) : (
          nav
        )}
      </li>,
    );
  }
  const pending = confirmDelete && sch.sheets[confirmDelete] ? confirmDelete : null;
  return (
    <div className="design-manager">
      <div className="dm-tabs" role="tablist" aria-label="Design manager">
        {MANAGER_TABS.map((t) => (
          <button key={t.id} type="button" role="tab" aria-selected={tab === t.id} className={tab === t.id ? "tab on" : "tab"} onClick={() => onTab(t.id)}>
            {t.label}
          </button>
        ))}
      </div>
      <input className="dm-search" type="search" value={query} placeholder={`Search ${label.toLowerCase()}`} aria-label="Search design manager" onChange={(e) => setQuery(e.target.value)} />
      {tab === "pages" && onCommand && (
        <div className="dm-actions" role="toolbar" aria-label="Sheet actions">
          <button type="button" disabled={busy} title={busy ? WAIT : "Add an empty schematic sheet at the end of the sheet order"} onClick={() => onCommand({ type: "AddSheet", name: nextSheetName(sch) })}>
            Add sheet
          </button>
        </div>
      )}
      <div className="side-scroll" role="tabpanel" aria-label={label}>
        {tab === "nets" && netlist.error && <p className="empty-state">Engine netlist unavailable ({netlist.error}); the nets below come from the PCB only.</p>}
        {all.length === 0 ? (
          <p className="empty-state">{EMPTY[tab]}</p>
        ) : items.length === 0 ? (
          <p className="empty-state">Nothing matches “{query.trim()}”.</p>
        ) : (
          <ul className="tree dm-list">{rows}</ul>
        )}
      </div>
      {pending && onCommand && (
        <DeleteSheetDialog
          name={sch.sheets[pending].name}
          contents={sheetContents(snapshot, pending)}
          busy={busy}
          onCancel={() => setConfirmDelete(null)}
          onConfirm={() => {
            const c = deleteSheetCommand(snapshot, pending);
            setConfirmDelete(null);
            onCommand(c);
          }}
        />
      )}
    </div>
  );
}

/** Rename, Move up, Move down and Delete for one sheet; each disabled control says why. */
function SheetActions(p: { name: string; index: number; count: number; busy: boolean; onRename: () => void; onMove: (by: -1 | 1) => void; onDelete: () => void }) {
  const why = (blocked: string | null, ok: string) => (p.busy ? WAIT : (blocked ?? ok));
  const first = p.index <= 0 ? "Already the first sheet" : null;
  const last = p.index >= p.count - 1 ? "Already the last sheet" : null;
  const only = p.count <= 1 ? "A project keeps at least one sheet" : null;
  return (
    <span className="dm-sheet-actions" role="group" aria-label={`Sheet ${p.name} actions`}>
      <button type="button" aria-label={`Rename sheet ${p.name}`} title={why(null, "Rename this sheet")} disabled={p.busy} onClick={p.onRename}>
        Rename
      </button>
      <button type="button" aria-label={`Move sheet ${p.name} up`} title={why(first, "Move this sheet one place up in the sheet order")} disabled={p.busy || !!first} onClick={() => p.onMove(-1)}>
        ↑
      </button>
      <button type="button" aria-label={`Move sheet ${p.name} down`} title={why(last, "Move this sheet one place down in the sheet order")} disabled={p.busy || !!last} onClick={() => p.onMove(1)}>
        ↓
      </button>
      <button type="button" aria-label={`Delete sheet ${p.name}`} title={why(only, "Delete this sheet (asks first when anything is on it)")} disabled={p.busy || !!only} onClick={p.onDelete}>
        Delete
      </button>
    </span>
  );
}

/** Inline sheet-name editor: Enter or Rename submits, Escape or Cancel leaves the name as it was. */
function RenameForm(p: { snapshot: Snapshot; sheetId: Id; busy: boolean; onSubmit: (name: string) => void; onCancel: () => void }) {
  const sch = p.snapshot.project.schematic;
  const [name, setName] = useState(sch.sheets[p.sheetId]?.name ?? "");
  const [problem, setProblem] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);
  return (
    <form
      className="dm-rename"
      aria-label="Rename sheet"
      onSubmit={(e) => {
        e.preventDefault();
        if (name.trim() === sch.sheets[p.sheetId]?.name) return p.onCancel();
        const why = renameProblem(sch, p.sheetId, name);
        if (why) setProblem(why);
        else p.onSubmit(name.trim());
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          p.onCancel();
        }
      }}
    >
      <input
        ref={input}
        aria-label="Sheet name"
        value={name}
        onChange={(e) => {
          setName(e.target.value);
          setProblem(null);
        }}
      />
      <button type="submit" disabled={p.busy} title={p.busy ? WAIT : "Rename the sheet"}>
        Rename
      </button>
      <button type="button" onClick={p.onCancel}>
        Cancel
      </button>
      {problem && (
        <p role="alert" className="error">
          {problem}
        </p>
      )}
    </form>
  );
}

/** Confirmation before a non-empty sheet is deleted with everything on it. */
function DeleteSheetDialog(p: { name: string; contents: string[]; busy: boolean; onConfirm: () => void; onCancel: () => void }) {
  return (
    <div className="modal-backdrop" onClick={p.onCancel}>
      <div className="modal" role="dialog" aria-modal="true" aria-label="Delete sheet" onClick={(e) => e.stopPropagation()} onKeyDown={(e) => e.key === "Escape" && p.onCancel()}>
        <div className="modal-head">
          <h2>Delete sheet {p.name}?</h2>
        </div>
        <p>
          {p.name} is not empty: {p.contents.join(", ")}. Deleting the sheet deletes all of it, and the footprints of its symbols leave the PCB. It is one change: Undo
          restores everything.
        </p>
        <div className="row">
          <button type="button" className="danger" disabled={p.busy} title={p.busy ? WAIT : "Delete the sheet and everything on it"} onClick={p.onConfirm}>
            Delete sheet and contents
          </button>
          <button type="button" autoFocus onClick={p.onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
