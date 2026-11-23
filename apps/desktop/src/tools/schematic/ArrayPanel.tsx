// Array of the selection (slice B3): columns x rows of pasted copies on a fixed step, sent as one
// Batch. Inline in the editor's option strip, not a modal; Esc or Cancel closes it.
import { useState } from "react";
import { MAX_ARRAY_COPIES } from "./selection";

interface Props {
  /** Default step between copies, mm. */
  stepMm: number;
  busy: boolean;
  onCreate: (cols: number, rows: number, stepXmm: number, stepYmm: number) => void;
  onClose: () => void;
}

const int = (s: string) => (/^\d+$/.test(s.trim()) ? Number(s.trim()) : NaN);
const num = (s: string) => (s.trim() === "" ? NaN : Number(s));

export function ArrayPanel({ stepMm, busy, onCreate, onClose }: Props) {
  const [cols, setCols] = useState("3");
  const [rows, setRows] = useState("1");
  const [sx, setSx] = useState(String(stepMm));
  const [sy, setSy] = useState(String(-stepMm));
  const c = int(cols);
  const r = int(rows);
  const copies = c * r - 1;
  const why = busy
    ? "An engine command is running"
    : !(c >= 1 && r >= 1)
      ? "Columns and rows must be whole numbers of 1 or more"
      : !Number.isFinite(num(sx)) || !Number.isFinite(num(sy))
        ? "The steps must be numbers (mm)"
        : copies < 1
          ? "One column and one row is the selection itself: nothing to copy"
          : copies > MAX_ARRAY_COPIES
            ? `At most ${MAX_ARRAY_COPIES} copies in one array`
            : null;
  return (
    <div
      className="array-panel"
      role="group"
      aria-label="Array"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onClose();
        }
      }}
    >
      <strong>Array</strong>
      <label>
        Columns
        <input aria-label="Columns" size={3} value={cols} onChange={(e) => setCols(e.target.value)} />
      </label>
      <label>
        Rows
        <input aria-label="Rows" size={3} value={rows} onChange={(e) => setRows(e.target.value)} />
      </label>
      <label>
        X step (mm)
        <input aria-label="X step (mm)" size={6} value={sx} onChange={(e) => setSx(e.target.value)} />
      </label>
      <label>
        Y step (mm, up is positive)
        <input aria-label="Y step (mm)" size={6} value={sy} onChange={(e) => setSy(e.target.value)} />
      </label>
      <button type="button" disabled={!!why} title={why ?? `Paste ${copies} copies as one Batch (one undo step)`} onClick={() => onCreate(c, r, num(sx), num(sy))}>
        Create {Number.isFinite(copies) && copies > 0 ? `${copies} cop${copies === 1 ? "y" : "ies"}` : "copies"}
      </button>
      <button type="button" onClick={onClose}>
        Cancel
      </button>
    </div>
  );
}
