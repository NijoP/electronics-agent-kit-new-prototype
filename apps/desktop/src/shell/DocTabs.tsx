// Real document tabs: each tab is an engine document (schematic sheet or the PCB). The dirty
// dot means the engine has not confirmed a write (command in flight, or writes blocked).
import type { Doc } from "./documents";

interface Props {
  docs: Doc[];
  active: string | null;
  dirty: boolean;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onCloseAll: () => void;
  split: boolean;
  onSplit: () => void;
  askLabel: string | null;
  onAsk: () => void;
}

export function DocTabs({ docs, active, dirty, onSelect, onClose, onCloseAll, split, onSplit, askLabel, onAsk }: Props) {
  return (
    <div className="tabs editor-tabs" role="tablist" aria-label="Documents">
      {docs.map((d) => (
        <span key={d.id} className={d.id === active ? "doc-tab on" : "doc-tab"}>
          <button type="button" role="tab" aria-selected={d.id === active} className="tab-label" onClick={() => onSelect(d.id)} title={`${d.kind === "pcb" ? "PCB board" : "Schematic sheet"} ${d.ref}`}>
            {d.title}
            {dirty && <span className="dirty" role="img" aria-label="unsaved changes">●</span>}
          </button>
          <button type="button" className="tab-close" aria-label={`Close ${d.title}`} title={`Close ${d.title}`} onClick={() => onClose(d.id)}>
            ×
          </button>
        </span>
      ))}
      <span className="spacer" />
      {askLabel && (
        <button type="button" className="ask-chip" onClick={onAsk} title="Ask the assistant about the selected part">
          Ask AI about {askLabel}
        </button>
      )}
      <button type="button" className="icon" aria-pressed={split} title="Show schematic and PCB side by side" onClick={onSplit}>
        Split
      </button>
      {docs.length > 1 && (
        <button type="button" className="icon" title="Close all documents" onClick={onCloseAll}>
          Close all
        </button>
      )}
    </div>
  );
}
