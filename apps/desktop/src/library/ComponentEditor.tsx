// Create or edit a component in the user's personal library (`component_save`, which saves
// through the existing `LibraryStore::personal_save`). Starts from an existing definition: keep
// its id to edit the personal copy, or give a new id to create a new part from it. The backend
// refuses a pin map that fails PP-001 and returns the footprint and pin-to-pad reports, shown
// here; a saved part's footprint stays "unverified" until every footprint rule passes.
import { useEffect, useState } from "react";
import { api } from "../api";
import type { ComponentDef } from "../types";
import { allCertify, FootprintStatus, PIN_PAD_RULES, type ComponentSaved } from "./VerificationReport";

export interface EditForm {
  id: string;
  name: string;
  category: string;
  manufacturer: string;
  mpn: string;
  /** pin number -> comma-separated pad numbers */
  pads: Record<string, string>;
}

export const formOf = (def: ComponentDef): EditForm => ({
  id: def.id,
  name: def.name,
  category: def.category,
  manufacturer: def.manufacturer ?? "",
  mpn: def.mpn ?? "",
  pads: Object.fromEntries(def.symbol.pins.map((p) => [p.number, (def.pin_map[p.number] ?? []).join(", ")])),
});

/** Same rule as the backend's `libstore::safe_name`: the id becomes a file name. */
const SAFE_ID = /^[A-Za-z0-9_-][A-Za-z0-9_.-]{0,79}$/;

/** The definition the form describes, or why it cannot be saved yet. */
export function definitionOf(base: ComponentDef, f: EditForm): { def: ComponentDef } | { problem: string } {
  const id = f.id.trim();
  if (!SAFE_ID.test(id)) return { problem: "Use 1-80 letters, digits, _ - . for the id (no leading dot)." };
  if (!f.name.trim()) return { problem: "Enter a part name." };
  const pin_map: Record<string, string[]> = {};
  for (const p of base.symbol.pins) {
    pin_map[p.number] = (f.pads[p.number] ?? "")
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s !== "");
  }
  const text = (s: string) => (s.trim() === "" ? null : s.trim());
  return {
    def: { ...base, id, name: f.name.trim(), category: f.category.trim() || base.category, manufacturer: text(f.manufacturer), mpn: text(f.mpn), pin_map },
  };
}

type Key = "id" | "name" | "category" | "manufacturer" | "mpn";

export function ComponentEditor({ def, onSaved }: { def: ComponentDef; onSaved?: (s: ComponentSaved) => void }) {
  const [f, setF] = useState<EditForm>(() => formOf(def));
  const [saved, setSaved] = useState<ComponentSaved | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    setF(formOf(def));
    setSaved(null);
    setError(null);
  }, [def]);

  const built = definitionOf(def, f);
  const problem = "problem" in built ? built.problem : null;
  const save = async () => {
    if ("problem" in built) return;
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      const r = await api.componentSave(built.def);
      setSaved(r);
      onSaved?.(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };
  const field = (label: string, key: Key) => (
    <label className="field">
      <span>{label}</span>
      <input aria-label={`Personal part ${key}`} value={f[key]} onChange={(e) => setF({ ...f, [key]: e.target.value })} />
    </label>
  );
  return (
    <details className="component-editor" open={open}>
      <summary
        onClick={(e) => {
          e.preventDefault();
          setOpen(!open);
        }}
      >
        Edit in personal library…
      </summary>
      {open && (
        <>
      <p className="muted">Saves to your personal library (all projects). Keep the id to edit your copy, or enter a new id to create a new part. Built-in parts are never changed.</p>
      {field("Id", "id")}
      {field("Name", "name")}
      {field("Category", "category")}
      {field("Manufacturer", "manufacturer")}
      {field("MPN", "mpn")}
      <table aria-label="Edit pin to pad mapping">
        <thead>
          <tr>
            <th>Pin</th>
            <th>Name</th>
            <th>Pads (comma separated)</th>
          </tr>
        </thead>
        <tbody>
          {def.symbol.pins.map((p) => (
            <tr key={p.number}>
              <td>{p.number}</td>
              <td>{p.name}</td>
              <td>
                <input aria-label={`Pads of pin ${p.number}`} value={f.pads[p.number] ?? ""} onChange={(e) => setF({ ...f, pads: { ...f.pads, [p.number]: e.target.value } })} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {problem && <p className="muted">{problem}</p>}
      <button type="button" disabled={problem !== null || saving} title={problem ?? (saving ? "Saving…" : "Save this definition to your personal library")} onClick={() => void save()}>
        Save to personal library
      </button>
      {error && <p data-testid="component-save-error">Not saved: {error}</p>}
      {saved && (
        <p data-testid="component-saved">
          {saved.replaced ? "Updated" : "Created"} {saved.id} in the personal library. <FootprintStatus report={saved.footprint} />
          {" · "}
          {allCertify(saved.pin_pad.rule_results, PIN_PAD_RULES) ? "Pin-to-pad checks passed" : "Pin-to-pad unverified"}
        </p>
      )}
        </>
      )}
    </details>
  );
}
