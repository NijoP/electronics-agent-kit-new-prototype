// Right dock (Phase 1 A3): Properties (the A5 inspector, shell/Inspector.tsx), Filter (the schematic
// selection filter, slice B3) and (in the PCB) Layers. The filter covers the schematic only; in the
// PCB the tab is disabled with that reason, never shown as a working control.
import type { ReactNode } from "react";

export type RightTab = "properties" | "filter" | "layers";

/** Tabs of the right dock in a workspace; a tab that does not apply is absent, one not built yet is disabled. */
export function rightTabs(pcb: boolean): { id: RightTab; label: string; disabled?: string }[] {
  return [
    { id: "properties", label: "Properties" },
    { id: "filter", label: "Filter", ...(pcb ? { disabled: "Not available yet in the PCB: the selection filter (slice B3) covers the schematic only" } : {}) },
    ...(pcb ? [{ id: "layers" as const, label: "Layers" }] : []),
  ];
}

/** The tab to show: the stored one when it exists and is enabled here, else Properties. */
export function shownRightTab(stored: string, pcb: boolean): RightTab {
  const t = rightTabs(pcb).find((x) => x.id === stored && !x.disabled);
  return t ? t.id : "properties";
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
