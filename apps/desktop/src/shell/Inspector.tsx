// Context properties inspector (Phase 1 A5) in the right dock's Properties tab. Three modes:
//  * tool options: the active editor tool's option schema (wire, label, place, route, via, zone);
//  * selection: the selected objects' fields (common fields for several, edits as one Batch);
//  * document: sheet size and title block, or the board and its stack-up.
// Every edit is one typed engine command, so one revision and one undo step. Fields EAK cannot
// hold yet are shown as unavailable with the reason, never as controls.
import { useState, type ReactNode } from "react";
import type { EditCtx } from "../EditContext";
import type { Id, Snapshot } from "../types";
import {
  NO_SELECTION,
  documentModel,
  editorContext,
  selectionModel,
  useEditorContext,
  useInspectorHost,
  type Commit,
  type OptionField,
  type PropField,
} from "./inspectorModel";

const IN_FLIGHT = "A command is in flight; the engine has not confirmed it yet";

/**
 * A tool's options. In the inspector (`docId` given) a change is routed to the editor's latest
 * published callback at event time; inline (no inspector shown) it calls the field's own.
 */
export function ToolOptionFields({ fields, docId, layout }: { fields: OptionField[]; docId?: string | null; layout: "strip" | "panel" }) {
  const change = (f: OptionField, v: string) => {
    const live = docId ? editorContext(docId)?.options.find((o) => o.name === f.name) : f;
    if (live && (live.kind === "select" || live.kind === "text")) live.onChange(v);
  };
  const control = (f: OptionField) => {
    switch (f.kind) {
      case "select":
        return (
          <select aria-label={f.name} title={f.title} value={f.value} onChange={(e) => change(f, e.target.value)}>
            {f.options.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        );
      case "text":
        return <input aria-label={f.name} title={f.title} inputMode="decimal" placeholder={f.placeholder} size={layout === "strip" ? 8 : undefined} value={f.value} onChange={(e) => change(f, e.target.value)} />;
      case "info":
        return <span className="opt-info">{f.value}</span>;
      case "unavailable":
        return (
          <span className="unavail" title={f.reason}>
            {f.reason}
          </span>
        );
    }
  };
  if (layout === "strip")
    return (
      <>
        {fields
          .filter((f) => f.kind !== "unavailable")
          .map((f) =>
            f.label && f.label !== f.name ? (
              <label key={f.name}>
                {f.label}
                {control(f)}
              </label>
            ) : (
              <span key={f.name}>{control(f)}</span>
            ),
          )}
      </>
    );
  return (
    <dl className="props inspector-fields">
      {fields.map((f) => (
        <div key={f.name} data-field={f.name} className={f.kind === "unavailable" ? "unavailable" : undefined}>
          <dt>{f.label || f.name}</dt>
          <dd>{control(f)}</dd>
        </div>
      ))}
    </dl>
  );
}

/** A text field that commits on Enter or when it loses focus; Escape restores the value. */
function TextField({ f, busy, apply }: { f: PropField; busy: boolean; apply: (v: string) => void }) {
  const [text, setText] = useState(f.value);
  const commit = () => {
    if (text !== f.value && !(f.mixed && text === "")) apply(text);
  };
  return (
    <input
      aria-label={f.label}
      value={text}
      placeholder={f.mixed ? "mixed" : undefined}
      disabled={busy}
      title={busy ? IN_FLIGHT : undefined}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        else if (e.key === "Escape") setText(f.value);
      }}
    />
  );
}

function Fields({ fields, edit }: { fields: PropField[]; edit: EditCtx | null }) {
  const apply = (commit: (v: string) => Commit, v: string) => {
    if (!edit) return;
    const r = commit(v);
    if (typeof r === "string") edit.notify(r);
    else if (r) void edit.execute(r);
  };
  const busy = !!edit?.busy;
  return (
    <dl className="props inspector-fields">
      {fields.map((f) => {
        let body: ReactNode;
        if (f.unavailable)
          body = (
            <span className="unavail" title={f.unavailable}>
              {f.unavailable}
            </span>
          );
        else if (f.edit && edit && f.edit.kind === "text") {
          const commit = f.edit.commit;
          body = <TextField key={`${f.value}|${f.mixed ? 1 : 0}`} f={f} busy={busy} apply={(v) => apply(commit, v)} />;
        } else if (f.edit && edit && f.edit.kind === "select") {
          const commit = f.edit.commit;
          body = (
            <select aria-label={f.label} value={f.mixed ? "" : f.value} disabled={busy} title={busy ? IN_FLIGHT : undefined} onChange={(e) => apply(commit, e.target.value)}>
              {f.mixed && (
                <option value="" disabled>
                  mixed
                </option>
              )}
              {f.edit.options.map((o) => (
                <option key={o.value} value={o.value}>
                  {o.label}
                </option>
              ))}
            </select>
          );
        } else body = f.mixed ? <span className="muted">mixed</span> : f.value;
        return (
          <div key={f.id} data-field={f.id} className={f.unavailable ? "unavailable" : undefined}>
            <dt>{f.label}</dt>
            <dd>{body}</dd>
          </div>
        );
      })}
    </dl>
  );
}

interface Props {
  snapshot: Snapshot;
  /** The active document (its editor's published context drives the mode). */
  docId: string | null;
  pcb: boolean;
  /** Selection made outside an editor (explorer, findings) when the editor published none. */
  selected: Id | null;
  edit: EditCtx | null;
}

/** The inspector: tool options while a tool is active, else the selection, else the document. */
export function Inspector({ snapshot, docId, pcb, selected, edit }: Props) {
  useInspectorHost();
  const ctx = useEditorContext(docId);
  const tool = ctx && ctx.tool !== "select" ? ctx : null;
  const sel = ctx ? ctx.selection : selected && snapshot.project.schematic.symbols[selected] ? { ...NO_SELECTION, symbols: [selected] } : NO_SELECTION;
  const selModel = selectionModel(snapshot, sel);
  const model = selModel ?? documentModel(snapshot, pcb, ctx?.sheetId);
  const mode = tool ? "tool" : selModel ? "selection" : "document";
  // A tool without options leaves room for the selection or document below it.
  const showModel = !tool || tool.options.length === 0;
  return (
    <section className="panel-body inspector" aria-label="Properties" data-mode={mode}>
      {tool && (
        <>
          <h3 className="side-h">{`${tool.toolLabel} tool`}</h3>
          {tool.options.length ? <ToolOptionFields fields={tool.options} docId={docId} layout="panel" /> : <p className="muted hint">This tool has no options.</p>}
        </>
      )}
      {showModel && (
        <>
          <h3 className="side-h">{model.title}</h3>
          <Fields fields={model.fields} edit={edit} />
        </>
      )}
      {!edit && <p className="muted hint">Read-only: no project is open for editing.</p>}
    </section>
  );
}
