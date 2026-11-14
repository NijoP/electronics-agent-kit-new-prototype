// One live terminal: an xterm.js instance bound to a backend PTY session. Instances live in a
// module registry, so switching workspaces (or remounting) never loses the screen; after a
// window reload the screen is rebuilt from the backend scrollback, then live output resumes.
import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import "@xterm/xterm/css/xterm.css";
import { term, type OutputEvent } from "./api";
import { profileNow } from "../resourceProfile";

interface Entry { t: Terminal; fit: FitAddon; search: SearchAddon; host: HTMLDivElement; seq: number; ready: boolean; pending: OutputEvent[] }
const registry = new Map<number, Entry>();
let listening: Promise<unknown> | null = null;

function ensureListener() {
  listening ??= term.onOutput((e) => {
    const en = registry.get(e.id);
    if (!en) return;
    if (!en.ready) en.pending.push(e);
    else if (e.seq > en.seq) {
      en.t.write(e.data);
      en.seq = e.seq;
    }
  });
}

function entry(id: number): Entry {
  const have = registry.get(id);
  if (have) return have;
  ensureListener();
  // View scrollback follows the resource profile; the backend keeps its own 2 MiB ring per
  // session, so a redraw after reattach still has that much output.
  const t = new Terminal({ scrollback: profileNow().scrollback_lines, cursorBlink: true, allowProposedApi: true, fontFamily: "ui-monospace, Menlo, Consolas, monospace", fontSize: 13 });
  const fit = new FitAddon();
  const search = new SearchAddon();
  t.loadAddon(fit);
  t.loadAddon(search);
  const host = document.createElement("div");
  host.className = "xterm-host";
  const en: Entry = { t, fit, search, host, seq: 0, ready: false, pending: [] };
  registry.set(id, en);
  t.onData((d) => void term.write(id, d).catch(() => {}));
  // Copy / paste on Ctrl+Shift+C / V (plain Ctrl+C stays an interrupt for the program).
  t.attachCustomKeyEventHandler((ev) => {
    if (ev.type !== "keydown" || !ev.ctrlKey || !ev.shiftKey) return true;
    const k = ev.key.toLowerCase();
    if (k === "c" && t.hasSelection()) {
      void navigator.clipboard?.writeText(t.getSelection());
      return false;
    }
    if (k === "v") {
      void navigator.clipboard?.readText().then((s) => s && t.paste(s));
      return false;
    }
    return true;
  });
  void term.scrollback(id).then(
    (sb) => {
      t.write(sb.data);
      en.seq = sb.seq;
      en.ready = true;
      for (const e of en.pending) if (e.seq > en.seq) {
        t.write(e.data);
        en.seq = e.seq;
      }
      en.pending = [];
    },
    () => {
      en.ready = true;
    },
  );
  return en;
}

/** Forget a closed session's terminal. */
export function disposeTerminal(id: number) {
  registry.get(id)?.t.dispose();
  registry.delete(id);
}

export function XTermView({ id, focused }: { id: number; focused: boolean }) {
  const box = useRef<HTMLDivElement>(null);
  const [searching, setSearching] = useState(false);
  const [query, setQuery] = useState("");
  useEffect(() => {
    const en = entry(id);
    const el = box.current!;
    el.appendChild(en.host);
    if (!en.t.element) en.t.open(en.host);
    const resize = () => {
      if (!el.clientWidth || !el.clientHeight) return; // hidden workspace: keep the size
      try {
        en.fit.fit();
        void term.resize(id, en.t.cols, en.t.rows).catch(() => {});
      } catch {
        /* not laid out yet */
      }
    };
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    resize();
    return () => {
      ro.disconnect();
      if (en.host.parentElement === el) el.removeChild(en.host);
    };
  }, [id]);
  useEffect(() => {
    if (focused) registry.get(id)?.t.focus();
  }, [id, focused]);
  const find = (back = false) => {
    const s = registry.get(id)?.search;
    if (s && query) (back ? s.findPrevious(query) : s.findNext(query));
  };
  return (
    <div
      className="xterm-pane"
      onKeyDownCapture={(e) => {
        if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "f") {
          e.preventDefault();
          setSearching(true);
        }
      }}
    >
      {searching && (
        <div className="term-search" role="search">
          <input
            autoFocus
            aria-label="Search terminal output"
            placeholder="Search output"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") find(e.shiftKey);
              if (e.key === "Escape") setSearching(false);
            }}
          />
          <button type="button" onClick={() => find(true)} aria-label="Previous match">↑</button>
          <button type="button" onClick={() => find()} aria-label="Next match">↓</button>
          <button type="button" onClick={() => setSearching(false)} aria-label="Close search">✕</button>
        </div>
      )}
      <div ref={box} className="xterm-box" data-session={id} />
    </div>
  );
}
