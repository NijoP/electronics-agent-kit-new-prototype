// Real document tabs: each tab is an engine document (schematic sheet or the PCB). The marker is
// per document: it means the engine has not confirmed a write for that document (writes blocked,
// or a command from it in flight). There is no other "unsaved" state (A-SAVE).
import type { Doc } from "./documents";

interface Props {
  docs: Doc[];
  active: string | null;
  /** Why the document is marked (not confirmed by the engine), or null when it is not. */
  marker: (d: Doc) => string | null;
  onSelect: (id: string) => void;
  onClose: (id: string) => void;
  onCloseAll: () => void;
  split: boolean;
  onSplit: () => void;
}

export function DocTabs({ docs, active, marker, onSelect, onClose, onCloseAll, split, onSplit }: Props) {
  return (
    <div className="tabs editor-tabs" role="tablist" aria-label="Documents">
      {docs.map((d) => {
        const why = marker(d);
        return (
          <span key={d.id} className={d.id === active ? "doc-tab on" : "doc-tab"}>
            <button type="button" role="tab" aria-selected={d.id === active} className="tab-label" onClick={() => onSelect(d.id)} title={d.kind === "view" ? `${d.title} view of the board` : `${d.kind === "pcb" ? "PCB board" : d.kind === "file" ? "File" : "Schematic sheet"} ${d.ref}`}>
              {d.title}
              {why && (
                <span className="dirty" role="img" aria-label={`not confirmed: ${why}`} title={why}>
                  ●
                </span>
              )}
            </button>
            <button type="button" className="tab-close" aria-label={`Close ${d.title}`} title={`Close ${d.title}`} onClick={() => onClose(d.id)}>
              ×
            </button>
          </span>
        );
      })}
      <span className="spacer" />
      <button type="button" className="icon" aria-pressed={split} title="Show the schematic and the PCB side by side (off by default: each is its own view)" onClick={onSplit}>
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
