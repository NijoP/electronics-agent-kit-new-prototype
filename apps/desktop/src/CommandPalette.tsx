// Keyboard-driven command palette (Ctrl/Cmd+K). Actions are supplied by the workspace;
// every action that changes the project calls the Rust engine.
import { useEffect, useRef, useState } from "react";

export interface PaletteAction {
  id: string;
  label: string;
  run: () => void;
  /** Destructive engine commands are confirmed before submission (ADR-0004). */
  destructive?: boolean;
  disabled?: boolean;
}

interface Props {
  actions: PaletteAction[];
  onClose: () => void;
  confirm?: (message: string) => boolean;
}

export function CommandPalette({ actions, onClose, confirm = (m) => window.confirm(m) }: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);

  const q = query.trim().toLowerCase();
  const shown = actions.filter((a) => !a.disabled && a.label.toLowerCase().includes(q));
  const pick = (a: PaletteAction | undefined) => {
    if (!a) return;
    if (a.destructive && !confirm(`${a.label}: this is a destructive change. Continue?`)) return;
    onClose();
    a.run();
  };

  return (
    <div className="palette-backdrop" onClick={onClose}>
      <div className="palette" role="dialog" aria-label="Command palette" onClick={(e) => e.stopPropagation()}>
        <input
          ref={input}
          aria-label="Search commands"
          placeholder="Type a command…"
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
        <ul role="listbox" aria-label="Commands">
          {shown.map((a, i) => (
            <li
              key={a.id}
              role="option"
              aria-selected={i === active}
              className={i === active ? "active" : ""}
              onMouseEnter={() => setActive(i)}
              onClick={() => pick(a)}
            >
              {a.label}
              {a.destructive && <span className="tag">destructive</span>}
            </li>
          ))}
          {shown.length === 0 && <li className="muted">No matching command</li>}
        </ul>
      </div>
    </div>
  );
}
