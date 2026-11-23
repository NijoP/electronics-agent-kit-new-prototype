// Find Results tab of the bottom dock (Phase 1 slice E4): the matches of the schematic's Find and
// replace bar (slice B3) on every sheet, recomputed from the live snapshot. A click opens the
// hit's sheet and selects the object there.
import { formatShortcut } from "../design/shortcuts";
import type { Snapshot } from "../types";
import { FIND_FIELDS, findHits, type FindHit } from "../tools/schematic/find";
import { useFindSearch } from "./findResults";

interface Props {
  snapshot: Snapshot;
  onPick?: (hit: FindHit) => void;
}

export function FindResultsTab({ snapshot, onPick }: Props) {
  const search = useFindSearch();
  if (!search || !search.query)
    return <p className="empty-state">No search yet: open Find and replace in the schematic ({formatShortcut("Mod+F")}) and type what to find. Its matches on every sheet are listed here.</p>;
  const hits = findHits(snapshot, search.query, search.options);
  const sheetName = (id: string) => snapshot.project.schematic.sheets[id]?.name ?? id;
  const field = (h: FindHit) => FIND_FIELDS.find((f) => f.field === h.field)?.label ?? h.field;
  const owner = (h: FindHit) => (h.kind === "symbol" ? (snapshot.project.schematic.symbols[h.id]?.refdes ?? h.id) : "net label");
  return (
    <section className="find-results" aria-label="Find results">
      <p className="muted" role="status">
        {hits.length} match{hits.length === 1 ? "" : "es"} for “{search.query}”{search.options.matchCase ? " · match case" : ""}
        {search.options.whole ? " · whole text" : ""} at revision {snapshot.project.revision}
      </p>
      {hits.length > 0 && (
        <table className="bom">
          <thead>
            <tr>
              <th>Match</th>
              <th>Field</th>
              <th>Object</th>
              <th>Sheet</th>
            </tr>
          </thead>
          <tbody>
            {hits.map((h) => (
              <tr key={`${h.kind}:${h.id}:${h.field}`} data-hit-id={h.id}>
                <td>
                  <button type="button" disabled={!onPick} title={onPick ? `Open ${sheetName(h.sheet)} and select it` : "No schematic editor in this window"} onClick={() => onPick?.(h)}>
                    {h.text}
                  </button>
                </td>
                <td className="muted">{field(h)}</td>
                <td>{owner(h)}</td>
                <td className="muted">{sheetName(h.sheet)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
