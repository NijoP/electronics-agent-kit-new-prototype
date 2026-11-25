// Main toolbar, rendered from the action registry (src/actions/) like the menus: every button runs a real operation or is disabled
// with its reason as the tooltip (no decorative controls). Buttons show the action's EAK icon
// (`design/icons/`), with the label as accessible name and tooltip (plus the shortcut), and as
// visible text when "Toolbar labels" is on. A tool family shares one split button: the main part
// runs the member picked last from its dropdown, remembered across sessions. An item's `run` comes
// from `toolbarFor`, which goes through `runAction` (enabled only, destructive ones confirmed), so a
// click here can never bypass the confirmation the menus, keys and context menu give.
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { SplitId } from "../actions/types";
import { Icon, type IconName } from "../design/icons/icons";
import { formatShortcut } from "../design/shortcuts";

export interface ToolbarItem {
  id: string;
  label: string;
  run?: () => void;
  shortcut?: string;
  /** Why the control is unavailable; presence disables it. */
  disabled?: string;
  pressed?: boolean;
  title?: string;
  /** Functional group (file/history, edit tools, view, design checks, manufacturing, app). */
  group?: ToolbarGroupId;
  icon?: IconName;
  /** Tool family shown as one split button. */
  split?: SplitId;
}
export type ToolbarGroupId = "file" | "edit" | "view" | "checks" | "mfg" | "app";

/** Caption shown over each group, and the toolbar width (px) below which the group moves into "More". */
export const GROUPS: Record<ToolbarGroupId, { label: string; minWidth: number }> = {
  file: { label: "File · History", minWidth: 0 },
  edit: { label: "Edit tools", minWidth: 640 },
  checks: { label: "Design · Checks", minWidth: 860 },
  view: { label: "View", minWidth: 1000 },
  mfg: { label: "Manufacturing", minWidth: 1100 },
  app: { label: "App", minWidth: 1200 },
};
/** Name of each split-button family (its group label and dropdown title). */
export const SPLITS: Record<SplitId, string> = {
  "sch-marks": "Net marks",
  "sch-labels": "Net labels and ports",
  "sch-power": "Power ports",
  "sch-bus": "Bus tools",
  "sch-graphics": "Graphics and text",
  "pcb-regions": "Board regions",
  "pcb-checks": "PCB checks",
};
export type ToolbarGroup = ToolbarItem[];

const tipOf = (it: ToolbarItem) => (it.disabled ? `${it.label}: ${it.disabled}` : `${it.title ?? it.label}${it.shortcut ? ` (${formatShortcut(it.shortcut)})` : ""}`);

function Btn({ it, labels }: { it: ToolbarItem; labels: boolean }) {
  const tip = tipOf(it);
  return (
    // The wrapper carries the tooltip too: some webviews show no tooltip on a disabled button.
    <span className="tb-item" title={tip}>
      <button type="button" className="tb-btn" aria-label={it.label} aria-pressed={it.pressed} aria-disabled={it.disabled ? true : undefined} disabled={!!it.disabled} title={tip} onClick={it.run}>
        {it.icon && <Icon name={it.icon} />}
        {(labels || !it.icon) && <span className="tb-label">{it.label}</span>}
      </button>
    </span>
  );
}

const pickKey = (id: SplitId) => `eak.toolbar.split.${id}`;
function loadPick(id: SplitId): string | null {
  try {
    return localStorage.getItem(pickKey(id));
  } catch {
    return null;
  }
}
function savePick(id: SplitId, item: string) {
  try {
    localStorage.setItem(pickKey(id), item);
  } catch {
    // Storage unavailable: the pick still holds for this session.
  }
}

/**
 * One button for a tool family. The main part runs the member picked last (the first enabled
 * member until something is picked); the arrow lists every member, disabled ones with their reason.
 */
export function SplitButton({ id, items, labels }: { id: SplitId; items: ToolbarItem[]; labels: boolean }) {
  const [pick, setPick] = useState(() => loadPick(id));
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", away);
    return () => document.removeEventListener("pointerdown", away);
  }, [open]);
  const current = items.find((i) => i.id === pick) ?? items.find((i) => !i.disabled) ?? items[0];
  const choose = (it: ToolbarItem) => {
    if (it.disabled) return;
    setPick(it.id);
    savePick(id, it.id);
    setOpen(false);
    it.run?.();
  };
  return (
    <span
      ref={root}
      className="tb-split"
      role="group"
      aria-label={SPLITS[id]}
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation();
          setOpen(false);
        }
      }}
    >
      <Btn it={current} labels={labels} />
      <button type="button" className="tb-arrow" aria-label={`${SPLITS[id]}: choose`} aria-haspopup="menu" aria-expanded={open} title={`${SPLITS[id]}: choose which one this button runs`} onClick={() => setOpen((o) => !o)}>
        <Icon name="chevron-down" />
      </button>
      {open && (
        <div className="tb-menu" role="menu" aria-label={SPLITS[id]}>
          {items.map((it) => (
            <button key={it.id} type="button" role="menuitemradio" aria-checked={it.id === current.id} aria-disabled={it.disabled ? true : undefined} disabled={!!it.disabled} title={tipOf(it)} onClick={() => choose(it)}>
              {it.icon && <Icon name={it.icon} />}
              <span className="tb-label">{it.label}</span>
              {it.disabled ? <span className="tb-why">{it.disabled}</span> : it.shortcut && <span className="tb-key">{formatShortcut(it.shortcut)}</span>}
            </button>
          ))}
        </div>
      )}
    </span>
  );
}

/** A group's buttons, each tool family folded into one split button at its first member's place. */
function Row({ g, labels, extra }: { g: ToolbarGroup; labels: boolean; extra?: ReactNode }) {
  const out: ReactNode[] = [];
  const seen = new Set<SplitId>();
  for (const it of g) {
    const family = it.split ? g.filter((x) => x.split === it.split) : [];
    if (it.split && family.length > 1) {
      if (seen.has(it.split)) continue;
      seen.add(it.split);
      out.push(<SplitButton key={`split-${it.split}`} id={it.split} items={family} labels={labels} />);
    } else out.push(<Btn key={it.id} it={it} labels={labels} />);
  }
  return (
    <div className="tb-row">
      {out}
      {extra}
    </div>
  );
}

const groupId = (g: ToolbarGroup): ToolbarGroupId => g[0]?.group ?? "file";

/**
 * Grouped toolbar. Controls are 28 px high with captions per group; on a narrow window the groups
 * with the highest minWidth fold into a "More" menu (same buttons, same reasons for disabled ones).
 * The width is unknown until measured, in which case everything is shown inline. `extras` adds
 * non-action controls to a group (the active editor's grid combos in View).
 */
export function Toolbar({ groups, left, labels = false, extras = {} }: { groups: ToolbarGroup[]; left?: ReactNode; labels?: boolean; extras?: Partial<Record<ToolbarGroupId, ReactNode>> }) {
  const ref = useRef<HTMLElement>(null);
  const [width, setWidth] = useState(0);
  const [more, setMore] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  const folded = (g: ToolbarGroup) => width > 0 && width < GROUPS[groupId(g)].minWidth;
  const inline = groups.filter((g) => !folded(g));
  const hidden = groups.filter(folded);
  const group = (g: ToolbarGroup) => (
    <div key={groupId(g)} className="tb-group" role="group" aria-label={GROUPS[groupId(g)].label}>
      <span className="tb-cap">{GROUPS[groupId(g)].label}</span>
      <Row g={g} labels={labels} extra={extras[groupId(g)]} />
    </div>
  );
  return (
    <header
      ref={ref}
      className={labels ? "toolbar labels" : "toolbar"}
      role="toolbar"
      aria-label="Main toolbar"
      onKeyDown={(e) => {
        if (e.key === "Escape") setMore(false);
      }}
    >
      {left}
      {inline.map(group)}
      {hidden.length > 0 && (
        <div className="tb-more">
          <button type="button" className="tb-morebtn" aria-haspopup="menu" aria-expanded={more} onClick={() => setMore((m) => !m)}>
            More ▾
          </button>
          {more && (
            <div
              className="tb-pop"
              role="menu"
              aria-label="More toolbar commands"
              onClick={(e) => {
                // A command closes the popover; a combo, a check box or a split-button arrow keeps it open.
                const b = (e.target as HTMLElement).closest("button");
                if (b && !b.hasAttribute("aria-haspopup")) setMore(false);
              }}
            >
              {hidden.map(group)}
            </div>
          )}
        </div>
      )}
    </header>
  );
}
