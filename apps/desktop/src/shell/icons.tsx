// EAK's own geometric icons (24px grid, 1.5 stroke). Drawn for EAK; not taken from any other product.
import type { ReactNode } from "react";

const svg = (children: ReactNode) => (
  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {children}
  </svg>
);

export const Icons = {
  project: svg(<><rect x="3.5" y="4.5" width="17" height="15" rx="2" /><path d="M9 4.5v15M3.5 10h5.5" /></>),
  parts: svg(<><rect x="8" y="8" width="8" height="8" rx="1" /><path d="M10 8V4M14 8V4M10 20v-4M14 20v-4M8 10H4M8 14H4M20 10h-4M20 14h-4" /></>),
  domain: svg(<><circle cx="12" cy="12" r="8" /><path d="M12 4a12 12 0 0 1 0 16M12 4a12 12 0 0 0 0 16M4 12h16" /></>),
  manufacturing: svg(<><path d="M4 18h16M6 18V9l4 3V9l4 3V6h4v12" /></>),
  settings: svg(<><circle cx="12" cy="12" r="3" /><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1" /></>),
  layers: svg(<><path d="M12 4l8 4-8 4-8-4z" /><path d="M4 12l8 4 8-4M4 16l8 4 8-4" /></>),
  sparkle: svg(<><path d="M12 4l1.8 4.7L18.5 10.5 13.8 12.3 12 17l-1.8-4.7L5.5 10.5l4.7-1.8z" /><path d="M18 16l.7 1.8L20.5 18.5 18.7 19.2 18 21l-.7-1.8-1.8-.7 1.8-.7z" /></>),
  sun: svg(<><circle cx="12" cy="12" r="4" /><path d="M12 3v2M12 19v2M3 12h2M19 12h2" /></>),
  schematic: svg(<><path d="M3 12h4l1.5-4 3 8 3-8 1.5 4h4" /><circle cx="3" cy="12" r="1" /><circle cx="21" cy="12" r="1" /></>),
  pcb: svg(<><rect x="4" y="4" width="16" height="16" rx="2" /><circle cx="8.5" cy="8.5" r="1.3" /><circle cx="15.5" cy="15.5" r="1.3" /><path d="M9.8 8.5H13v7h1.2" /></>),
  moon: svg(<path d="M20 14.5A8 8 0 0 1 9.5 4 8 8 0 1 0 20 14.5z" />),
};
