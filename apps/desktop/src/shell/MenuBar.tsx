// Application menu bar. Every item either runs a real operation or is disabled with the
// reason shown (never a decorative entry). Items are data so tests can walk all of them.
// Keyboard (WAI-ARIA menu pattern): on a menu button ArrowDown / Enter / Space opens the menu on
// its first item and ArrowUp on its last; ArrowLeft / ArrowRight move between menus; inside a menu
// ArrowUp / ArrowDown / Home / End move between items, Enter runs one, Escape closes the menu and
// returns focus to its button, Tab closes it.
import { useEffect, useRef, useState } from "react";
import { formatShortcut } from "../design/shortcuts";
import { nativeConfirm, whenConfirmed, type Confirm } from "../confirm";

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

type Focus = "first" | "last" | null;

export function MenuBar({ menus, confirm = nativeConfirm }: { menus: Menu[]; confirm?: Confirm }) {
  const [open, setOpen] = useState<number | null>(null);
  const root = useRef<HTMLElement>(null);
  // Which item to focus once the menu that was just opened from the keyboard has rendered.
  const pending = useRef<Focus>(null);
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

  const buttons = () => [...(root.current?.querySelectorAll<HTMLButtonElement>(".menu-btn") ?? [])];
  const items = () => [...(root.current?.querySelectorAll<HTMLButtonElement>(".menu-list [role=menuitem]") ?? [])];
  useEffect(() => {
    if (open === null || !pending.current) return;
    const all = items();
    (pending.current === "first" ? all[0] : all[all.length - 1])?.focus();
    pending.current = null;
  }, [open]);

  const wrap = (i: number) => ((i % menus.length) + menus.length) % menus.length;
  const openAt = (i: number, focus: Focus) => {
    const at = wrap(i);
    if (open === at) {
      // Already open (e.g. opened with the pointer): move into it now; no re-render will.
      const all = items();
      (focus === "last" ? all[all.length - 1] : all[0])?.focus();
      return;
    }
    pending.current = focus;
    setOpen(at);
    buttons()[at]?.focus();
  };

  const onButtonKey = (i: number) => (e: React.KeyboardEvent) => {
    const k = e.key;
    if (k === "ArrowDown" || k === "Enter" || k === " ") openAt(i, "first");
    else if (k === "ArrowUp") openAt(i, "last");
    else if (k === "ArrowRight" || k === "ArrowLeft") {
      const to = i + (k === "ArrowRight" ? 1 : -1);
      if (open !== null) openAt(to, "first");
      else buttons()[wrap(to)]?.focus();
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  const onListKey = (i: number) => (e: React.KeyboardEvent) => {
    const all = items();
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    const k = e.key;
    if (k === "ArrowDown") all[(at + 1) % all.length]?.focus();
    else if (k === "ArrowUp") all[(at - 1 + all.length) % all.length]?.focus();
    else if (k === "Home") all[0]?.focus();
    else if (k === "End") all[all.length - 1]?.focus();
    else if (k === "ArrowRight") openAt(i + 1, "first");
    else if (k === "ArrowLeft") openAt(i - 1, "first");
    else if (k === "Escape") {
      setOpen(null);
      buttons()[i]?.focus();
    } else if (k === "Tab") {
      setOpen(null);
      return;
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  const pick = (it: MenuItem) => {
    if (it.disabled || !it.run) return;
    const run = it.run;
    const go = () => {
      setOpen(null);
      run();
    };
    if (it.destructive) void whenConfirmed(`${it.label}: this is a destructive change. Continue?`, go, confirm);
    else go();
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
            onClick={(e) => {
              // WebKit does not focus a button on a mouse click; focus it so the arrow keys work next.
              e.currentTarget.focus();
              setOpen(open === i ? null : i);
            }}
            onPointerEnter={() => open !== null && setOpen(i)}
            onKeyDown={onButtonKey(i)}
          >
            {m.label}
          </button>
          {open === i && (
            <ul className="menu-list" role="menu" aria-label={m.label} onKeyDown={onListKey(i)}>
              {m.items.map((it) => (
                <li key={it.label} role="none" className={it.sep ? "sep" : ""}>
                  <button
                    type="button"
                    role="menuitem"
                    tabIndex={-1}
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
