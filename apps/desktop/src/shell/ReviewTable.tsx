// The grouped change table of the review dialogs (Update PCB, ADR-0041; agent proposals, S9 /
// ADR-0051): one tbody per group, one row per change with a checkbox in the first column. Which
// rows are checked, and what checking one does to the others, is the caller's rule.
export interface ReviewRow {
  id: string;
  /** Text of the columns after the checkbox, in `columns` order. */
  cells: string[];
  /** Accessible name of the row's checkbox. */
  checkLabel: string;
  checked: boolean;
  /** Row tooltip (optional). */
  title?: string;
}

interface Props {
  /** Accessible name of the table. */
  label: string;
  /** Column headers after the checkbox column ("Apply"). */
  columns: string[];
  groups: { group: string; rows: ReviewRow[] }[];
  /** Data attribute carrying the row id (e.g. data-eco-row), for tests and evidence. */
  rowAttr: `data-${string}`;
  onToggle: (id: string, on: boolean) => void;
}

export function ReviewTable({ label, columns, groups, rowAttr, onToggle }: Props) {
  return (
    <div className="eco-table-wrap">
      <table className="eco-table" aria-label={label}>
        <thead>
          <tr>
            <th scope="col">Apply</th>
            {columns.map((c) => (
              <th key={c} scope="col">
                {c}
              </th>
            ))}
          </tr>
        </thead>
        {groups.map((g) => (
          <tbody key={g.group} aria-label={`${g.group} (${g.rows.length})`}>
            <tr className="eco-group">
              <th scope="rowgroup" colSpan={columns.length + 1}>
                {g.group} <span className="count">{g.rows.length}</span>
              </th>
            </tr>
            {g.rows.map((r) => (
              <tr key={r.id} {...{ [rowAttr]: r.id }} className={r.checked ? undefined : "off"} title={r.title}>
                <td>
                  <input type="checkbox" aria-label={r.checkLabel} checked={r.checked} onChange={(e) => onToggle(r.id, e.target.checked)} />
                </td>
                {r.cells.map((c, i) => (
                  <td key={i}>{c}</td>
                ))}
              </tr>
            ))}
          </tbody>
        ))}
      </table>
    </div>
  );
}
