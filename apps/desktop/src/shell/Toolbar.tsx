// Main toolbar, rendered from the action registry (src/actions/) like the menus: every button runs a real operation or is disabled
// with its reason as the tooltip (no decorative controls).
import { useEffect, useRef, useState } from "react";
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
export type ToolbarGroup = ToolbarItem[];

function Btn({ it }: { it: ToolbarItem }) {
  const tip = it.disabled ? `${it.label}: ${it.disabled}` : `${it.title ?? it.label}${it.shortcut ? ` (${formatShortcut(it.shortcut)})` : ""}`;
  return (
    // The wrapper carries the tooltip too: some webviews show no tooltip on a disabled button.
    <span className="tb-item" title={tip}>
      <button type="button" aria-label={it.label} aria-pressed={it.pressed} aria-disabled={it.disabled ? true : undefined} disabled={!!it.disabled} title={tip} onClick={it.run}>
        {it.label}
      </button>
    </span>
  );
}

const groupId = (g: ToolbarGroup): ToolbarGroupId => g[0]?.group ?? "file";

/**
 * Grouped toolbar. Controls are 28 px high with captions per group; on a narrow window the groups
 * with the highest minWidth fold into a "More" menu (same buttons, same reasons for disabled ones).
 * The width is unknown until measured, in which case everything is shown inline.
 */
export function Toolbar({ groups, left }: { groups: ToolbarGroup[]; left?: React.ReactNode }) {
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
  return (
    <header
      ref={ref}
      className="toolbar"
      role="toolbar"
      aria-label="Main toolbar"
      onKeyDown={(e) => {
        if (e.key === "Escape") setMore(false);
      }}
    >
      {left}
      {inline.map((g, i) => (
        <div key={i} className="tb-group" role="group" aria-label={GROUPS[groupId(g)].label}>
          <span className="tb-cap">{GROUPS[groupId(g)].label}</span>
          <div className="tb-row">{g.map((it) => <Btn key={it.id} it={it} />)}</div>
        </div>
      ))}
      {hidden.length > 0 && (
        <div className="tb-more">
          <button type="button" className="tb-morebtn" aria-haspopup="menu" aria-expanded={more} onClick={() => setMore((m) => !m)}>
            More ▾
          </button>
          {more && (
            <div className="tb-pop" role="menu" aria-label="More toolbar commands" onClick={() => setMore(false)}>
              {hidden.map((g, i) => (
                <div key={i} className="tb-group" role="group" aria-label={GROUPS[groupId(g)].label}>
                  <span className="tb-cap">{GROUPS[groupId(g)].label}</span>
                  <div className="tb-row">{g.map((it) => <Btn key={it.id} it={it} />)}</div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </header>
  );
}
