// Right dock (Phase 1 A3): Properties, Filter and (in the PCB) Layers. Properties is read-only
// until slice A5 adds editing; the selection filter is slice B3 and is shown disabled with that
// reason, never as a working control.
import type { ReactNode } from "react";
import type { Id, Snapshot } from "../types";
import { mm } from "../viewport";

export type RightTab = "properties" | "filter" | "layers";

/** Tabs of the right dock in a workspace; a tab that does not apply is absent, one not built yet is disabled. */
export function rightTabs(pcb: boolean): { id: RightTab; label: string; disabled?: string }[] {
  return [
    { id: "properties", label: "Properties" },
    { id: "filter", label: "Filter", disabled: "Not available yet: the selection filter arrives with slice B3" },
    ...(pcb ? [{ id: "layers" as const, label: "Layers" }] : []),
  ];
}

/** The tab to show: the stored one when it exists and is enabled here, else Properties. */
export function shownRightTab(stored: string, pcb: boolean): RightTab {
  const t = rightTabs(pcb).find((x) => x.id === stored && !x.disabled);
  return t ? t.id : "properties";
}

const pt = (p: { x: number; y: number }) => `${mm(p.x).toFixed(3)}, ${mm(p.y).toFixed(3)} mm`;

function Rows({ rows }: { rows: [string, string][] }) {
  return (
    <dl className="props">
      {rows.map(([k, v]) => (
        <div key={k}>
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      ))}
    </dl>
  );
}

/** Read-only properties of the selection, or of the document when nothing is selected. */
export function PropertiesPanel({ snapshot, selected, pcb }: { snapshot: Snapshot; selected: Id | null; pcb: boolean }) {
  const { schematic, pcb: board } = snapshot.project;
  const sym = selected ? schematic.symbols[selected] : undefined;
  const fp = sym ? Object.values(board.footprints).find((f) => f.symbol_id === sym.id) : undefined;
  const count = (o: object) => String(Object.keys(o).length);
  let rows: [string, string][];
  if (sym) {
    rows = [
      ["Reference", sym.refdes],
      ["Value", sym.value],
      ["Component", sym.component_id],
      ["Sheet", schematic.sheets[sym.sheet_id]?.name ?? sym.sheet_id],
      ["Symbol position", pt(sym.position)],
      ["Symbol rotation", sym.rotation],
      ["Mirrored", sym.mirror ? "yes" : "no"],
    ];
    if (fp) rows.push(["Footprint position", pt(fp.position)], ["Footprint rotation", fp.rotation], ["Board side", fp.side], ["Locked", fp.locked ? "yes" : "no"]);
    else rows.push(["Footprint", "not on the PCB yet (Update PCB from schematic)"]);
  } else if (pcb) {
    const o = board.board.outline;
    const xs = o.map((p) => mm(p.x));
    const ys = o.map((p) => mm(p.y));
    rows = [
      ["Document", "PCB"],
      ["Board outline", o.length ? `${(Math.max(...xs) - Math.min(...xs)).toFixed(2)} × ${(Math.max(...ys) - Math.min(...ys)).toFixed(2)} mm (${o.length} points)` : "none yet"],
      ["Footprints", count(board.footprints)],
      ["Tracks", count(board.tracks)],
      ["Vias", count(board.vias)],
      ["Zones", count(board.zones)],
    ];
  } else {
    rows = [
      ["Project", snapshot.project.name],
      ["Revision", String(snapshot.project.revision)],
      ["Sheets", count(schematic.sheets)],
      ["Symbols", count(schematic.symbols)],
      ["Wires", count(schematic.wires)],
      ["Net labels", count(schematic.labels)],
    ];
  }
  return (
    <section className="panel-body" aria-label="Properties">
      <h3 className="side-h">{sym ? `Selected ${sym.refdes}` : "Nothing selected"}</h3>
      <Rows rows={rows} />
      <p className="muted hint">Read-only. Editing properties here is not available yet (slice A5); use the editor tools, which send typed commands.</p>
    </section>
  );
}

interface DockProps {
  pcb: boolean;
  tab: RightTab;
  onTab: (t: RightTab) => void;
  onClose: () => void;
  children: ReactNode;
}

/** The dock frame: tab strip, hide button and the active tab's content. */
export function RightDock({ pcb, tab, onTab, onClose, children }: DockProps) {
  return (
    <aside className="right-dock" aria-label="Right panel">
      <div className="tabs" role="tablist" aria-label="Right panel tabs">
        {rightTabs(pcb).map((t) => (
          <button key={t.id} type="button" role="tab" aria-selected={tab === t.id} className={tab === t.id ? "tab on" : "tab"} disabled={!!t.disabled} title={t.disabled} onClick={() => onTab(t.id)}>
            {t.label}
          </button>
        ))}
        <span className="spacer" />
        <button type="button" className="icon" aria-label="Hide right panel" title="Hide panel (Ctrl+Shift+B)" onClick={onClose}>
          ×
        </button>
      </div>
      <div className="right-body">{children}</div>
    </aside>
  );
}
