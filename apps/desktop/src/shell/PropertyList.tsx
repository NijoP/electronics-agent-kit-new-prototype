// Property List tab of the bottom dock (Phase 1 slice E4). A grid of every object of one kind;
// click a cell to pick it, Ctrl+click to add or remove cells, Shift+click for a range, or a
// column header to pick the whole column. Cells are picked in one column at a time; Apply sends
// the new value for all of them as one command (a `Batch` for several: one revision, one undo
// step), the same edit the Inspector makes for that selection.
import { useState, type MouseEvent } from "react";
import type { Command, Id, Snapshot } from "../types";
import { commitCells, commonValue, kindCount, LIST_KINDS, listModel, rowRange, type ListKind } from "./propertyList";

interface Props {
  snapshot: Snapshot;
  busy: boolean;
  execute?: (c: Command) => Promise<void>;
}

export function PropertyList({ snapshot, busy, execute }: Props) {
  const [kind, setKind] = useState<ListKind>("symbols");
  const [col, setCol] = useState<string | null>(null);
  const [picked, setPicked] = useState<Id[]>([]);
  const [anchor, setAnchor] = useState<Id | null>(null);
  const [value, setValue] = useState("");
  const [note, setNote] = useState<{ text: string; error: boolean } | null>(null);
  const model = listModel(snapshot, kind);
  const column = model.columns.find((c) => c.id === col && c.editable) ?? null;
  // Picks of objects that no longer exist (deleted, undone) drop out.
  const live = column ? picked.filter((id) => model.rows.some((r) => r.id === id)) : [];

  const choose = (ids: Id[], colId: string) => {
    setCol(colId);
    setPicked(ids);
    setValue(commonValue(model.rows, colId, ids));
    setNote(null);
  };
  const pickCell = (e: MouseEvent, rowId: Id, colId: string) => {
    if (colId !== col || (!e.shiftKey && !e.ctrlKey && !e.metaKey)) {
      setAnchor(rowId);
      return choose([rowId], colId);
    }
    if (e.shiftKey && anchor) return choose(rowRange(model.rows, anchor, rowId), colId);
    setAnchor(rowId);
    choose(live.includes(rowId) ? live.filter((x) => x !== rowId) : [...live, rowId], colId);
  };
  const apply = async () => {
    if (!column || !execute) return;
    const c = commitCells(snapshot, kind, column.id, live, value);
    if (c === null) return setNote({ text: "No change: every picked cell already holds this value.", error: false });
    if (typeof c === "string") return setNote({ text: c, error: true });
    setNote({ text: `${live.length} ${column.label} cell${live.length === 1 ? "" : "s"} sent as one ${c.type} command.`, error: false });
    await execute(c);
  };

  const n = live.length;
  const why = !execute ? "This project is read-only here" : busy ? "An engine command is running" : n === 0 ? "Pick cells in an editable column first" : `Set ${n} cell${n === 1 ? "" : "s"} as one command (one undo step)`;
  return (
    <section className="proplist" aria-label="Property list">
      <div className="proplist-bar" role="group" aria-label="Batch edit">
        <label>
          Objects{" "}
          <select
            aria-label="Object kind"
            value={kind}
            onChange={(e) => {
              setKind(e.target.value as ListKind);
              setCol(null);
              setPicked([]);
              setNote(null);
            }}
          >
            {LIST_KINDS.map((k) => (
              <option key={k.kind} value={k.kind}>
                {k.label} ({kindCount(snapshot, k.kind)})
              </option>
            ))}
          </select>
        </label>
        <span className="muted" aria-label="Batch selection">
          {column && n > 0 ? `${n} ${column.label} cell${n === 1 ? "" : "s"} picked` : "Click cells of one editable column (Ctrl+click adds, Shift+click a range)"}
        </span>
        {column?.choices ? (
          <select aria-label="New value" value={value} disabled={n === 0} title={n === 0 ? "Pick cells first" : `New ${column.label}`} onChange={(e) => setValue(e.target.value)}>
            {value === "" && <option value="">(mixed)</option>}
            {column.choices.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        ) : (
          <input
            aria-label="New value"
            value={value}
            disabled={n === 0}
            title={n === 0 ? "Pick cells first" : `New ${column?.label ?? "value"}`}
            placeholder={n > 1 && value === "" ? "(mixed)" : ""}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                void apply();
              }
            }}
          />
        )}
        <button type="button" disabled={busy || n === 0 || !execute} title={why} onClick={() => void apply()}>
          Apply
        </button>
        {note && (
          <span role="status" className={note.error ? "error" : "muted"}>
            {note.text}
          </span>
        )}
      </div>
      {model.rows.length === 0 ? (
        <p className="empty-state">No {LIST_KINDS.find((k) => k.kind === kind)?.label.toLowerCase()} in this project yet.</p>
      ) : (
        <div className="proplist-scroll">
          <table className="bom proplist-grid" aria-label="Property grid">
            <thead>
              <tr>
                <th scope="col">Object</th>
                {model.columns.map((c) => (
                  <th key={c.id} scope="col">
                    {c.editable ? (
                      <button type="button" className="col-pick" aria-label={`Pick every ${c.label} cell`} title={`Pick all ${model.rows.length} ${c.label} cells`} onClick={() => choose(model.rows.map((r) => r.id), c.id)}>
                        {c.label}
                      </button>
                    ) : (
                      <span title="Read-only here">{c.label}</span>
                    )}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {model.rows.map((r) => (
                <tr key={r.id} data-object-id={r.id}>
                  <th scope="row">{r.label}</th>
                  {model.columns.map((c) => (
                    <td key={c.id}>
                      {c.editable ? (
                        <button
                          type="button"
                          className={column?.id === c.id && live.includes(r.id) ? "cell on" : "cell"}
                          aria-pressed={column?.id === c.id && live.includes(r.id)}
                          aria-label={`${r.label} ${c.label}`}
                          onClick={(e) => pickCell(e, r.id, c.id)}
                        >
                          {r.cells[c.id] || " "}
                        </button>
                      ) : (
                        <span className="muted">{r.cells[c.id]}</span>
                      )}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
