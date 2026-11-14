// Application menu bar. Every item either runs a real operation or is disabled with the
// reason shown (never a decorative entry). Items are data so tests can walk all of them.
import { useEffect, useRef, useState } from "react";
import { formatShortcut } from "../design/shortcuts";

export interface MenuItem {
  label: string;
  run?: () => void;
  shortcut?: string;
  /** When set the item is disabled and this text says why. */
  disabled?: string;
  checked?: boolean;
  destructive?: boolean;
  /** Draw a divider above this item. */
  sep?: boolean;
}
export interface Menu {
  label: string;
  items: MenuItem[];
}

export function MenuBar({ menus, confirm = (m) => window.confirm(m) }: { menus: Menu[]; confirm?: (m: string) => boolean }) {
  const [open, setOpen] = useState<number | null>(null);
  const root = useRef<HTMLElement>(null);
  useEffect(() => {
    if (open === null) return;
    const away = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(null);
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(null);
    };
    window.addEventListener("pointerdown", away);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("pointerdown", away);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  const pick = (it: MenuItem) => {
    if (it.disabled || !it.run) return;
    if (it.destructive && !confirm(`${it.label}: this is a destructive change. Continue?`)) return;
    setOpen(null);
    it.run();
  };

  return (
    <nav className="menubar" aria-label="Application menu" ref={root}>
      {menus.map((m, i) => (
        <div key={m.label} className="menu">
          <button
            type="button"
            className={open === i ? "menu-btn on" : "menu-btn"}
            aria-haspopup="menu"
            aria-expanded={open === i}
            onClick={() => setOpen(open === i ? null : i)}
            onPointerEnter={() => open !== null && setOpen(i)}
          >
            {m.label}
          </button>
          {open === i && (
            <ul className="menu-list" role="menu" aria-label={m.label}>
              {m.items.map((it) => (
                <li key={it.label} role="none" className={it.sep ? "sep" : ""}>
                  <button
                    type="button"
                    role="menuitem"
                    aria-disabled={it.disabled ? true : undefined}
                    aria-checked={it.checked === undefined ? undefined : it.checked}
                    className={it.disabled ? "mi disabled" : "mi"}
                    title={it.disabled}
                    onClick={() => pick(it)}
                  >
                    <span className="check">{it.checked ? "✓" : ""}</span>
                    <span className="grow">
                      {it.label}
                      {it.disabled && <span className="why"> — {it.disabled}</span>}
                    </span>
                    {it.shortcut && <kbd>{formatShortcut(it.shortcut)}</kbd>}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      ))}
    </nav>
  );
}
