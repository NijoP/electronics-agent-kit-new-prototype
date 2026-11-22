// Design manager (Phase 1 A3): the left dock's Pages / Nets / Components / Objects lists with a
// search box. Clicking an entry asks the workspace to open, select and zoom to it (`onNavigate`).
// Nets come from the engine's netlist, refetched per revision; unrouted counts from its
// connectivity report.
import { useEffect, useState, type ReactNode } from "react";
import { api } from "../api";
import type { ConnectivityReport, Id, Snapshot } from "../types";
import { componentsOf, filterItems, MANAGER_TABS, netsOf, objectsOf, pagesOf, type ManagerItem, type ManagerTab, type NetRow, type Target } from "./designManagerData";

interface Props {
  snapshot: Snapshot;
  conn: ConnectivityReport | null;
  selected: Id | null;
  activeDoc: string | null;
  tab: ManagerTab;
  onTab: (t: ManagerTab) => void;
  onNavigate: (t: Target, netlist: NetRow[] | null) => void;
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

export function DesignManager({ snapshot, conn, selected, activeDoc, tab, onTab, onNavigate }: Props) {
  const [query, setQuery] = useState("");
  const netlist = useNetlist(snapshot.project.revision);
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
    rows.push(
      <li key={i.key}>
        <button type="button" className={isOn(i) ? "on" : ""} title={i.target.kind === "page" ? "Open" : "Select and zoom to it"} onClick={() => onNavigate(i.target, netlist.rows)}>
          <strong>{i.label}</strong> <span className="muted">{i.detail}</span>
        </button>
      </li>,
    );
  }
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
    </div>
  );
}
