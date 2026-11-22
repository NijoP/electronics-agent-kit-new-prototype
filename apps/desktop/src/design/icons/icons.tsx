// The EAK icon set (Phase 1 slice A2). Every icon here was drawn for EAK on a 20 px grid with a
// 1.5 px round stroke in `currentColor`, and is shown at 16 or 20 px. None is traced or copied from
// another product; the audit list is `README.md` next to this file (checked by `icons.test.ts`).
import type { ReactNode } from "react";

const dot = (cx: number, cy: number, r = 1.1) => <circle cx={cx} cy={cy} r={r} fill="currentColor" stroke="none" />;

export const ICONS = {
  // File and history
  new: <><path d="M5 2.5h6l4 4v11H5z" /><path d="M11 2.5v4h4M10 9.5v5M7.5 12h5" /></>,
  open: <><path d="M2.5 15.5V4.5h5l1.5 2h6.5v2" /><path d="M2.5 15.5l2.5-7h12.5l-2.5 7z" /></>,
  save: <><path d="M3.5 3.5h10l3 3v10h-13z" /><path d="M6.5 3.5V7h6V3.5" /><rect x="6" y="11" width="8" height="5.5" /></>,
  "save-as": <><path d="M8.5 16.5h-5v-13h9l2.5 2.5v3" /><path d="M6.5 3.5V7h5V3.5" /><path d="M11 17.5l.5-2.5 5-5 2 2-5 5z" /></>,
  undo: <><path d="M7.5 5L4 8.5 7.5 12" /><path d="M4 8.5h8a4 4 0 0 1 0 8H9" /></>,
  redo: <><path d="M12.5 5L16 8.5 12.5 12" /><path d="M16 8.5H8a4 4 0 0 0 0 8h3" /></>,
  close: <path d="M5 5l10 10M15 5L5 15" />,
  cut: <><circle cx="5.5" cy="14.5" r="2.5" /><circle cx="14.5" cy="14.5" r="2.5" /><path d="M7.3 12.7L14.5 2.5M12.7 12.7L5.5 2.5" /></>,
  copy: <><rect x="7" y="7" width="10" height="10" rx="1.5" /><path d="M13 7V4.5A1.5 1.5 0 0 0 11.5 3h-7A1.5 1.5 0 0 0 3 4.5v7A1.5 1.5 0 0 0 4.5 13H7" /></>,
  paste: <><path d="M7 3.5H4.5v14h11v-14H13" /><rect x="7" y="2" width="6" height="3" rx="1" /><path d="M7.5 9.5h5M7.5 12.5h5" /></>,

  // Edit tools
  select: <path d="M5 3l10 6-4.5 1.2L13 15.5l-2 1-2.5-5.2L5 14.5z" />,
  move: <><path d="M10 2.5v15M2.5 10h15" /><path d="M8 4.5l2-2 2 2M8 15.5l2 2 2-2M4.5 8l-2 2 2 2M15.5 8l2 2-2 2" /></>,
  rotate: <><path d="M15.5 10a5.5 5.5 0 1 1-1.6-3.9" /><path d="M14.5 2.5v4h-4" /></>,
  mirror: <><path d="M10 2.5v2M10 7.5v2M10 12.5v2M10 16.5v1" /><path d="M7.5 5L3 15h4.5zM12.5 5L17 15h-4.5z" /></>,
  "flip-side": <><path d="M2.5 4.5h15M2.5 15.5h15" /><path d="M7 7.5v5M5.5 11L7 12.5 8.5 11M13 12.5v-5M11.5 9L13 7.5 14.5 9" /></>,
  delete: <><path d="M3.5 5.5h13M8 5.5v-2h4v2M5 5.5l1 11h8l1-11" /><path d="M8.5 8.5v5M11.5 8.5v5" /></>,
  "align-left": <><path d="M3 2.5v15" /><rect x="5.5" y="5" width="10" height="3" rx="0.5" /><rect x="5.5" y="12" width="6" height="3" rx="0.5" /></>,
  "align-right": <><path d="M17 2.5v15" /><rect x="4.5" y="5" width="10" height="3" rx="0.5" /><rect x="8.5" y="12" width="6" height="3" rx="0.5" /></>,
  "align-top": <><path d="M2.5 3h15" /><rect x="5" y="5.5" width="3" height="10" rx="0.5" /><rect x="12" y="5.5" width="3" height="6" rx="0.5" /></>,
  "align-bottom": <><path d="M2.5 17h15" /><rect x="5" y="4.5" width="3" height="10" rx="0.5" /><rect x="12" y="8.5" width="3" height="6" rx="0.5" /></>,
  "distribute-h": <><path d="M2.5 3v14M17.5 3v14" /><rect x="8.5" y="6" width="3" height="8" rx="0.5" /></>,
  "distribute-v": <><path d="M3 2.5h14M3 17.5h14" /><rect x="6" y="8.5" width="8" height="3" rx="0.5" /></>,

  // Schematic placement
  part: <><rect x="6" y="4" width="8" height="12" rx="1" /><path d="M6 7.5H3M6 12.5H3M14 7.5h3M14 12.5h3" />{dot(8.5, 6.5, 0.8)}</>,
  wire: <><path d="M4.2 15H8V5h7.8" /><circle cx="3" cy="15" r="1.2" /><circle cx="17" cy="5" r="1.2" /></>,
  bus: <><path d="M2.5 8h15M2.5 12h15" /><path d="M6 5.5v9M14 5.5v9" /></>,
  "net-label": <><path d="M2.5 7h9.5l4 3-4 3H2.5z" /><path d="M5.5 10h3.5" /></>,
  "net-port": <><path d="M2.5 10h4" /><path d="M6.5 6.5h7l4 3.5-4 3.5h-7z" /></>,
  "off-page": <><path d="M2.5 6.5h8l3 3.5-3 3.5h-8z" /><path d="M13.5 10h4M15.5 8l2 2-2 2" /></>,
  power: <><path d="M10 17.5V7M5 7h10" /><path d="M8.5 4h3M10 2.5v3" /></>,
  ground: <path d="M10 2.5v8M4.5 10.5h11M6.5 13.5h7M8.5 16.5h3" />,
  "no-connect": <><path d="M2.5 10H10" /><path d="M7 7l6 6M13 7l-6 6" /></>,
  "pin-label": <><path d="M2.5 13.5h6" /><circle cx="10" cy="13.5" r="1.5" /><path d="M10 4.5h7.5M12 8h5.5" /></>,
  junction: <><path d="M2.5 10h15M10 10v7.5" />{dot(10, 10, 2)}</>,
  text: <path d="M4.5 5V3.5h11V5M10 3.5v13M7.5 16.5h5" />,
  line: <path d="M3.5 16.5l13-13" />,
  arc: <><path d="M3.5 16.5a13 13 0 0 1 13-13" />{dot(3.5, 16.5)}{dot(16.5, 3.5)}</>,
  circle: <circle cx="10" cy="10" r="6.5" />,
  rect: <rect x="3" y="5" width="14" height="10" rx="1" />,
  annotate: <><path d="M2.5 6.5h9.5l3 3.5-3 3.5H2.5z" /><path d="M6 8.5l1-.5v4M14 3.5h3.5M15.75 1.75v3.5" /></>,

  // PCB placement and routing
  via: <><circle cx="10" cy="10" r="6" /><circle cx="10" cy="10" r="2.5" /></>,
  pad: <><rect x="4" y="4" width="12" height="12" rx="2" /><circle cx="10" cy="10" r="2" /></>,
  outline: <><path d="M3.5 5.5h9l4 4v5h-13z" />{dot(3.5, 5.5)}{dot(12.5, 5.5)}{dot(16.5, 9.5)}{dot(16.5, 14.5)}{dot(3.5, 14.5)}</>,
  zone: <><path d="M3 4h14v12H3z" /><path d="M3 9l5-5M3 14L13 4M7 16L17 6M12 16l5-5" /></>,
  keepout: <><rect x="3" y="3" width="14" height="14" rx="1" /><path d="M3 17L17 3" /></>,
  route: <><circle cx="4" cy="15.5" r="1.5" /><circle cx="16" cy="4.5" r="1.5" /><path d="M5.5 15.5H9l5.5-5.5V6" /></>,
  autoroute: <><circle cx="4" cy="16" r="1.5" /><path d="M5.5 16H9l4-4V8" /><path d="M15 2.5l.8 1.7 1.7.8-1.7.8-.8 1.7-.8-1.7-1.7-.8 1.7-.8z" /></>,
  "clear-routing": <><circle cx="4" cy="15.5" r="1.5" /><path d="M5.5 15.5H9l3-3" /><path d="M12 3.5l5 5M17 3.5l-5 5" /></>,
  refill: <><path d="M8.5 12.5h-5v-9h9v5" /><path d="M3.5 8l4.5-4.5M3.5 12.5l9-9" /><path d="M17 14a3 3 0 1 1-.9-2.1" /><path d="M16.5 9.5V12H14" /></>,
  autoplace: <><rect x="3" y="3" width="5" height="5" rx="0.5" /><rect x="3" y="12" width="5" height="5" rx="0.5" /><rect x="12" y="12" width="5" height="5" rx="0.5" /><path d="M14.5 2.5l.7 1.5 1.5.7-1.5.7-.7 1.5-.7-1.5-1.5-.7 1.5-.7z" /></>,
  dimension: <><path d="M3 4v6M17 4v6M3 7h14" /><path d="M5 5.5L3 7l2 1.5M15 5.5L17 7l-2 1.5" /><path d="M7 14h6M10 12v4" /></>,
  layers: <><path d="M10 3l7.5 3.5L10 10 2.5 6.5z" /><path d="M2.5 10L10 13.5 17.5 10M2.5 13.5L10 17l7.5-3.5" /></>,
  "3d": <><path d="M10 2.5l7 4v7l-7 4-7-4v-7z" /><path d="M3 6.5l7 4 7-4M10 10.5v7" /></>,

  // View
  "zoom-in": <><circle cx="8.5" cy="8.5" r="5.5" /><path d="M12.5 12.5l5 5M6 8.5h5M8.5 6v5" /></>,
  "zoom-out": <><circle cx="8.5" cy="8.5" r="5.5" /><path d="M12.5 12.5l5 5M6 8.5h5" /></>,
  fit: <><path d="M2.5 7V2.5H7M13 2.5h4.5V7M17.5 13v4.5H13M7 17.5H2.5V13" /><rect x="6.5" y="6.5" width="7" height="7" rx="1" /></>,
  grid: <path d="M2.5 7.5h15M2.5 12.5h15M7.5 2.5v15M12.5 2.5v15" />,
  snap: <><path d="M4.5 3.5V10a5.5 5.5 0 0 0 11 0V3.5H12V10a2 2 0 0 1-4 0V3.5z" /><path d="M4.5 6.5H8M12 6.5h3.5" /></>,
  unit: <><path d="M2.5 13.5l11-11 4 4-11 11z" /><path d="M6 10l1.5 1.5M8.5 7.5L10 9M11 5l1.5 1.5" /></>,
  crosshair: <><circle cx="10" cy="10" r="5.5" /><path d="M10 2v4M10 14v4M2 10h4M14 10h4" /></>,
  eye: <><path d="M2 10s3-5.5 8-5.5S18 10 18 10s-3 5.5-8 5.5S2 10 2 10z" /><circle cx="10" cy="10" r="2.5" /></>,
  "eye-off": <><path d="M2 10s3-5.5 8-5.5S18 10 18 10s-3 5.5-8 5.5S2 10 2 10z" /><path d="M3.5 16.5l13-13" /></>,
  lock: <><rect x="4" y="9" width="12" height="8.5" rx="1.5" /><path d="M6.5 9V6.5a3.5 3.5 0 0 1 7 0V9" /></>,
  unlock: <><rect x="4" y="9" width="12" height="8.5" rx="1.5" /><path d="M6.5 9V6.5a3.5 3.5 0 0 1 6.8-1.2" /></>,
  labels: <><path d="M3 15.5L6.5 4.5l3.5 11M4.3 11.5h4.4" /><path d="M12.5 8h5M12.5 11.5h5M12.5 15h3" /></>,
  search: <><circle cx="8.5" cy="8.5" r="5.5" /><path d="M12.5 12.5l5 5" /></>,

  // Design, checks and outputs
  sync: <><rect x="2.5" y="3.5" width="6" height="6" rx="1" /><rect x="11.5" y="10.5" width="6" height="6" rx="1" /><path d="M8.5 6.5H12a2.5 2.5 0 0 1 2.5 2.5v1.5" /><path d="M12.5 8.5l2 2 2-2" /></>,
  rules: <><rect x="3.5" y="2.5" width="13" height="15" rx="1.5" /><path d="M7 7h6M7 10.5h6M7 14h3.5" /></>,
  erc: <><path d="M2.5 9h3L7 6l3 6 1.5-3h3" /><path d="M11.5 15l2 2 4-4.5" /></>,
  drc: <><rect x="2.5" y="2.5" width="11" height="11" rx="1" /><path d="M5.5 8.5h3V5.5" /><path d="M11.5 15.5l2 2 4-4.5" /></>,
  dfm: <><path d="M2.5 16.5V9L6 11.5V9l3.5 2.5V6h3v4M2.5 16.5h6" /><path d="M11.5 15l2 2 4-4.5" /></>,
  simulate: <><rect x="2.5" y="3.5" width="15" height="13" rx="1.5" /><path d="M4.5 12c2 0 2-5 4-5s2 6 4 6 2-4 3-4" /></>,
  bom: <><rect x="3" y="3" width="14" height="14" rx="1.5" /><path d="M3 7.5h14M3 12h14M8 3v14" /></>,
  export: <><path d="M10 12.5v-10M6.5 6L10 2.5 13.5 6" /><path d="M3.5 10.5v6h13v-6" /></>,
  release: <><path d="M4.5 17.5V3" /><path d="M4.5 3.5h10l-2 3 2 3h-10" /></>,

  // App
  palette: <><rect x="2.5" y="4" width="15" height="12" rx="1.5" /><path d="M5.5 8L8 10l-2.5 2M10 12.5h4" /></>,
  settings: <><circle cx="10" cy="10" r="2.5" /><path d="M10 2.5V5M10 15v2.5M2.5 10H5M15 10h2.5M4.7 4.7l1.8 1.8M13.5 13.5l1.8 1.8M15.3 4.7l-1.8 1.8M6.5 13.5l-1.8 1.8" /></>,
  help: <><circle cx="10" cy="10" r="7.5" /><path d="M7.8 7.8a2.3 2.3 0 1 1 3.2 2.1c-.6.3-1 .8-1 1.4v.7" />{dot(10, 14.5, 0.9)}</>,
  keyboard: <><rect x="2" y="5" width="16" height="10" rx="1.5" /><path d="M5 8.5h1M8 8.5h1M11 8.5h1M14 8.5h1M6.5 12h7" /></>,
  info: <><circle cx="10" cy="10" r="7.5" /><path d="M10 9v5" />{dot(10, 6.3, 0.9)}</>,
  warning: <><path d="M10 2.5l8 14.5H2z" /><path d="M10 8v4.5" />{dot(10, 14.7, 0.9)}</>,
  error: <><circle cx="10" cy="10" r="7.5" /><path d="M7 7l6 6M13 7l-6 6" /></>,
  check: <path d="M4 10.5l4 4 8-9" />,
  "chevron-down": <path d="M6 8l4 4 4-4" />,
} satisfies Record<string, ReactNode>;

export type IconName = keyof typeof ICONS;
export const ICON_NAMES = Object.keys(ICONS) as IconName[];

/** An EAK icon at 16 or 20 px; decorative to assistive tech (the control carries the name). */
export function Icon({ name, size = 16 }: { name: IconName; size?: 16 | 20 }) {
  return (
    <svg className="icon" data-icon={name} width={size} height={size} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
      {ICONS[name]}
    </svg>
  );
}
