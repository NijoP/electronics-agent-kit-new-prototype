// Canvas context menu (slice B3; EAK's version of EasyEDA `s20`). It shows only what the action
// registry generates for the clicked object type (`contextMenuFor`), so every entry is the same
// operation as its menu-bar, palette and key route, disabled with the registry's reason when it
// cannot run. Escape, a click elsewhere or a pick closes it.
import { useEffect, useRef } from "react";
import { formatShortcut } from "../design/shortcuts";
import type { ContextItem } from "../actions/registry";

interface Props {
  x: number;
  y: number;
  /** Accessible name: what was clicked ("Part R1", "Wire", "Sheet"). */
  label: string;
  items: ContextItem[];
  onClose: () => void;
}

export function CanvasContextMenu({ x, y, label, items, onClose }: Props) {
  const ref = useRef<HTMLUListElement>(null);
  // The latest onClose, so re-renders of the editor neither re-bind the listeners nor move focus.
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const away = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) close.current();
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && close.current();
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("keydown", esc);
    ref.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("keydown", esc);
    };
  }, []);
  // Keep the menu inside the window.
  const left = Math.max(0, Math.min(x, (typeof window === "undefined" ? x : window.innerWidth) - 240));
  const top = Math.max(0, Math.min(y, (typeof window === "undefined" ? y : window.innerHeight) - 28 * items.length - 8));
  return (
    <ul ref={ref} className="ctx-menu canvas-ctx" role="menu" aria-label={label} style={{ left, top }}>
      {items.length === 0 && <li className="muted empty">No actions for this object</li>}
      {items.map((it) => (
        <li key={it.id} role="none" className={it.sep ? "sep" : undefined}>
          <button
            type="button"
            role="menuitem"
            disabled={!!it.disabled}
            title={it.disabled}
            onClick={() => {
              onClose();
              it.run();
            }}
          >
            <span>{it.label}</span>
            {it.shortcut && <kbd>{formatShortcut(it.shortcut)}</kbd>}
          </button>
        </li>
      ))}
    </ul>
  );
}
