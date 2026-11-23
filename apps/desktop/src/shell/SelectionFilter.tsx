// Right dock > Filter (slice B3, EasyEDA `s24`): which schematic object kinds can be selected.
// A click, a box, Select all and Find similar skip the kinds switched off here.
import { FILTER_KINDS, setSelectFilter, useSelectFilter } from "../tools/schematic/filter";
import { ALL_KINDS } from "../tools/schematic/selection";

export function SelectionFilter() {
  const f = useSelectFilter();
  const all = FILTER_KINDS.every((k) => f[k.kind]);
  return (
    <section className="sel-filter" aria-label="Selection filter">
      <p className="muted">Objects a click, a selection box, Select all or Find similar may pick in the schematic.</p>
      <ul>
        {FILTER_KINDS.map((k) => (
          <li key={k.kind}>
            <label>
              <input type="checkbox" checked={f[k.kind]} onChange={(e) => setSelectFilter({ [k.kind]: e.target.checked })} />
              {k.label}
            </label>
          </li>
        ))}
      </ul>
      <button type="button" disabled={all} title={all ? "Every kind is already selectable" : undefined} onClick={() => setSelectFilter(ALL_KINDS)}>
        Allow every kind
      </button>
    </section>
  );
}
