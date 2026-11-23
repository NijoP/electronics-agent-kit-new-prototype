// Find and replace bar of the schematic editor (slice B3): designators, values and net labels
// across every sheet. "Select matches" selects the hits on this sheet; "Replace all" sends one
// Batch (one revision, one undo step) and the engine accepts or refuses it as a whole.
import { useState } from "react";
import type { Command, Id, Snapshot } from "../../types";
import { DEFAULT_FIND, FIND_FIELDS, findHits, replaceCommands, type FindHit, type FindOptions } from "./find";

interface Props {
  snapshot: Snapshot;
  sheetId: Id;
  busy: boolean;
  onSelect: (hits: FindHit[]) => void;
  onReplace: (commands: Command[], label: string) => void;
  onClose: () => void;
}

export function FindBar({ snapshot, sheetId, busy, onSelect, onReplace, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [replacement, setReplacement] = useState("");
  const [o, setO] = useState<FindOptions>(DEFAULT_FIND);
  const hits = findHits(snapshot, query, o);
  const here = hits.filter((h) => h.sheet === sheetId);
  const replace = replaceCommands(snapshot, hits, query, replacement, o);
  const changed = replace.filter((c) => c.type !== "DeleteNetLabel").length;
  const noneWhy = !query ? "Type what to find first" : "Nothing matches";
  return (
    <div
      className="find-bar"
      role="search"
      aria-label="Find and replace"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onClose();
        }
      }}
    >
      <label>
        Find
        <input aria-label="Find" autoFocus value={query} placeholder="R1, 10k, GND…" onChange={(e) => setQuery(e.target.value)} />
      </label>
      <label>
        Replace with
        <input aria-label="Replace with" value={replacement} onChange={(e) => setReplacement(e.target.value)} />
      </label>
      {FIND_FIELDS.map((f) => (
        <label key={f.field} className="check">
          <input type="checkbox" checked={o.fields[f.field]} onChange={(e) => setO({ ...o, fields: { ...o.fields, [f.field]: e.target.checked } })} />
          {f.label}
        </label>
      ))}
      <label className="check">
        <input type="checkbox" checked={o.matchCase} onChange={(e) => setO({ ...o, matchCase: e.target.checked })} />
        Match case
      </label>
      <label className="check">
        <input type="checkbox" checked={o.whole} onChange={(e) => setO({ ...o, whole: e.target.checked })} />
        Whole text
      </label>
      <span className="muted" role="status">
        {query ? `${hits.length} match${hits.length === 1 ? "" : "es"}${hits.length !== here.length ? ` (${here.length} on this sheet)` : ""}` : ""}
      </span>
      <button type="button" disabled={here.length === 0} title={here.length === 0 ? (hits.length ? "No match on this sheet" : noneWhy) : "Select the matches on this sheet"} onClick={() => onSelect(here)}>
        Select matches
      </button>
      <button
        type="button"
        disabled={busy || changed === 0}
        title={busy ? "An engine command is running" : changed === 0 ? (hits.length ? "The replacement changes nothing" : noneWhy) : `Replace in ${changed} object(s) as one Batch (one undo step)`}
        onClick={() => onReplace(replace, `Replace "${query}" with "${replacement}" in ${changed} object(s)`)}
      >
        Replace all
      </button>
      <button type="button" aria-label="Close find and replace" title="Close (Esc)" onClick={onClose}>
        ×
      </button>
    </div>
  );
}
