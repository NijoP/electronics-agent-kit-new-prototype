// Keyboard-driven palette used for both the command palette (Ctrl/Cmd+K, Ctrl/Cmd+Shift+P)
// and quick open (Ctrl/Cmd+P). Actions are supplied by the workspace; every action that
// changes the project calls the Rust engine.
import { useEffect, useRef, useState } from "react";
import { formatShortcut } from "./design/shortcuts";

export interface PaletteAction {
  id: string;
  label: string;
  run: () => void;
  /** Destructive engine commands are confirmed before submission (ADR-0004). */
  destructive?: boolean;
  disabled?: boolean;
  /** Shortcut spec ("Mod+Shift+P") shown beside the action. */
  shortcut?: string;
  /** Section heading ("Part", "Net", "Sheet", "Tool"…) or a secondary hint. */
  group?: string;
}

interface Props {
  actions: PaletteAction[];
  onClose: () => void;
  confirm?: (message: string) => boolean;
  title?: string;
  searchLabel?: string;
  placeholder?: string;
  empty?: string;
}

export function CommandPalette({
  actions,
  onClose,
  confirm = (m) => window.confirm(m),
  title = "Command palette",
  searchLabel = "Search commands",
  placeholder = "Type a command…",
  empty = "No matching command",
}: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const list = useRef<HTMLUListElement>(null);
  useEffect(() => {
    input.current?.focus();
  }, []);

  const q = query.trim().toLowerCase();
  const shown = actions.filter((a) => !a.disabled && `${a.label} ${a.group ?? ""}`.toLowerCase().includes(q));
  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView?.({ block: "nearest" });
  }, [active]);
  const pick = (a: PaletteAction | undefined) => {
    if (!a) return;
    if (a.destructive && !confirm(`${a.label}: this is a destructive change. Continue?`)) return;
    onClose();
    a.run();
  };

  return (
    <div className="palette-backdrop" onClick={onClose}>
      <div className="palette" role="dialog" aria-modal="true" aria-label={title} onClick={(e) => e.stopPropagation()}>
        <input
          ref={input}
          aria-label={searchLabel}
          placeholder={placeholder}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape") onClose();
            else if (e.key === "ArrowDown") setActive((i) => Math.min(i + 1, shown.length - 1));
            else if (e.key === "ArrowUp") setActive((i) => Math.max(i - 1, 0));
            else if (e.key === "Enter") pick(shown[active]);
            else return;
            e.preventDefault();
          }}
        />
        <ul ref={list} role="listbox" aria-label="Commands">
          {shown.map((a, i) => (
            <li
              key={a.id}
              role="option"
              aria-selected={i === active}
              className={i === active ? "active" : ""}
              onMouseEnter={() => setActive(i)}
              onClick={() => pick(a)}
            >
              <span className="grow">
                {a.label}
                {a.destructive && <span className="tag">destructive</span>}
              </span>
              {a.group && <span className="group">{a.group}</span>}
              {a.shortcut && <kbd>{formatShortcut(a.shortcut)}</kbd>}
            </li>
          ))}
          {shown.length === 0 && <li className="muted empty-row">{empty}</li>}
        </ul>
      </div>
    </div>
  );
}
