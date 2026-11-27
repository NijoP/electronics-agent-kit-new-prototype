// Requirements (M4.2/M4.3, ADR-0057 and ADR-0060): the structured requirement record of the open
// project, read through the `requirements_report` IPC and edited only through engine commands
// (`SetRequirementField`, `ResolveRequirementConflict`, `ClassifyDomain`, `UpsertRequirementItem`,
// `ClearRequirementField`), the same ones the CLI and agents use. Nothing here computes or
// guesses a value: a field nobody answered is shown as UNKNOWN, a field in conflict shows every
// competing value until a person keeps one, and the domain classification is the deterministic
// classifier's, with its evidence for and against each class.
import { useEffect, useState } from "react";
import { api } from "./api";
import type {
  Command,
  DomainClassResult,
  DomainEvidence,
  ReqEntry,
  ReqFieldDef,
  ReqFieldReport,
  ReqItemKind,
  ReqSource,
  ReqSourceKind,
  ReqStatus,
  ReqValue,
  RequirementsView,
} from "./types";

interface Props {
  /** Revision of the open design: the report is read again whenever it changes. */
  revision: number;
  busy: boolean;
  /** Runs one engine command; resolves to whether it was accepted (the workspace reports why not). */
  onCommand: (c: Command) => Promise<boolean>;
  /** Reads the report (the IPC by default; tests pass their own). */
  load?: () => Promise<RequirementsView>;
}

export const PERSON_KEY = "eak.requirements.person";
const STATUS_LABEL: Record<ReqStatus, string> = {
  UNKNOWN: "UNKNOWN",
  NEEDS_CONFIRMATION: "NEEDS CONFIRMATION",
  CONFIRMED: "CONFIRMED",
  DERIVED: "DERIVED",
  NOT_APPLICABLE: "NOT APPLICABLE",
};
const VERDICT_LABEL: Record<DomainClassResult["verdict"], string> = {
  APPLIES: "applies (strong, firm evidence)",
  NEEDS_CONFIRMATION: "needs confirmation",
  CONFLICT: "conflict: evidence for and against",
};
const SOURCE_KINDS: ReqSourceKind[] = ["engineer", "client", "document", "standard", "datasheet", "measurement", "assumption", "calculation"];
const PANEL_SOURCE: ReqSource = { kind: "engineer", reference: "entered in the Requirements panel" };
const EDIT_STATUSES: ReqStatus[] = ["NEEDS_CONFIRMATION", "CONFIRMED", "UNKNOWN", "NOT_APPLICABLE"];

/** A requirement value as text; `null` when nothing is recorded (shown as UNKNOWN, never as 0). */
export function showValue(v?: ReqValue | null): string | null {
  if (!v) return null;
  switch (v.kind) {
    case "number":
      return `${v.value} ${v.unit}${v.tolerance_pct != null ? ` ±${v.tolerance_pct} %` : ""}`;
    case "range": {
      const part = (x?: number) => (x == null ? "–" : String(x));
      return `${part(v.min)} / ${part(v.nom)} / ${part(v.max)} ${v.unit} (min / nom / max)`;
    }
    case "text":
      return v.text;
    case "flag":
      return v.value ? "yes" : "no";
  }
}

/** The catalogue definition of a requirement path (project field, rail or interface field, domain class). */
export function defOf(view: RequirementsView, path: string): ReqFieldDef | undefined {
  const c = view.catalogue;
  const slash = path.indexOf("/");
  if (slash < 0) return c.fields.find((d) => d.key === path) ?? c.domain_classes.find((d) => d.field.key === path)?.field;
  const kind = view.record.items?.[path.slice(0, slash)]?.kind;
  const key = path.slice(slash + 1);
  return (kind === "rail" ? c.rail_fields : kind === "interface" ? c.interface_fields : []).find((d) => d.key === key);
}

const setField = (path: string, status: ReqStatus, value: ReqValue | null, source: ReqSource | null, owner: string | null, risk: string | null, note: string | null, supersede: boolean): Command => ({
  type: "SetRequirementField",
  path,
  status,
  value,
  source,
  derived_from: [],
  owner,
  risk_if_wrong: risk,
  note,
  supersede,
});

export function RequirementsPanel({ revision, busy, onCommand, load = api.requirementsReport }: Props) {
  const [view, setView] = useState<RequirementsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [person, setPerson] = useState(() => localStorage.getItem(PERSON_KEY) ?? "");
  const [editing, setEditing] = useState<string | null>(null);
  // Sections stay as the person left them; one with a conflict or an open edit is opened (and
  // stays open after the conflict is resolved, so the row just acted on does not disappear).
  const [opened, setOpened] = useState<ReadonlySet<string>>(new Set());
  const toggled = (id: string, open: boolean) =>
    setOpened((old) => {
      if (old.has(id) === open) return old;
      const next = new Set(old);
      if (open) next.add(id);
      else next.delete(id);
      return next;
    });

  useEffect(() => {
    let live = true;
    load()
      .then((r) => live && (setView(r), setError(null)))
      .catch((e) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, [revision, load]);

  if (error)
    return (
      <section className="reqs" aria-label="Requirements">
        <p role="alert" className="error">
          Requirements could not be read: {error}
        </p>
      </section>
    );
  if (!view)
    return (
      <section className="reqs" aria-label="Requirements">
        <p className="muted">Reading the requirement record…</p>
      </section>
    );

  const name = person.trim();
  const ctx: Ctx = {
    view,
    busy,
    person: name,
    needName: name ? undefined : "Enter your name first: a confirmation or a resolved conflict records who decided it",
    editing,
    setEditing,
    run: (c) => onCommand(c).then((ok) => (ok && setEditing(null), ok)),
    opened,
    toggled,
  };
  const wants = (fields: ReqFieldReport[]) => fields.some((f) => f.path === editing || (f.conflicts?.length ?? 0) > 0);
  const r = view.report;
  const c = r.completeness;
  const blocked = r.operations.filter((o) => !o.ready);

  return (
    <section className="reqs" aria-label="Requirements">
      {view.revision !== revision && <p className="muted">Stale: read at revision {view.revision}, the design is at revision {revision}.</p>}
      <label className="reqs-person">
        Your name{" "}
        <input
          aria-label="Your name"
          value={person}
          placeholder="who confirms and resolves"
          onChange={(e) => {
            setPerson(e.target.value);
            localStorage.setItem(PERSON_KEY, e.target.value);
          }}
        />
      </label>
      <p aria-label="Completeness" className="reqs-summary">
        {c.total} fields: {c.firm} firm, {c.provisional} provisional, {c.missing} missing ({c.unknown} UNKNOWN
        {c.conflicts ? `, ${c.conflicts} in conflict` : ""}) · {c.percent_firm}% firm
      </p>

      <Classification ctx={ctx} />

      <details>
        <summary>
          Operations: {r.operations.length - blocked.length} of {r.operations.length} ready (advisory)
        </summary>
        <ul aria-label="Blocked operations" className="reqs-list">
          {blocked.map((o) => (
            <li key={o.operation} data-operation={o.operation}>
              <strong>{o.label}</strong>: blocked by {o.blocked_by.length} value(s){" "}
              <span className="muted">{o.blocked_by.slice(0, 6).map((b) => `${b.path} (${b.level}, needs ${b.needs})`).join(", ")}</span>
            </li>
          ))}
        </ul>
        {r.issues.length > 0 && (
          <ul aria-label="Record issues" className="reqs-list">
            {r.issues.map((i) => (
              <li key={i} className="error">
                {i}
              </li>
            ))}
          </ul>
        )}
      </details>

      {r.sections.map((s) => {
        const fields = r.fields.filter((f) => f.section === s.section);
        return (
          <details key={s.section} data-section={s.section} open={opened.has(s.section) || wants(fields)} onToggle={(e) => toggled(s.section, e.currentTarget.open)}>
            <summary>
              {s.label} · {s.completeness.firm}/{s.completeness.total} firm
              {s.completeness.conflicts ? ` · ${s.completeness.conflicts} in conflict` : ""}
            </summary>
            <ul className="reqs-fields" aria-label={`${s.label} requirements`}>
              {fields.map((f) => (
                <FieldRow key={f.path} f={f} ctx={ctx} />
              ))}
            </ul>
          </details>
        );
      })}

      <AddItem busy={busy} run={ctx.run} />

      <details>
        <summary>What EAK does not do here yet</summary>
        <ul className="reqs-list muted">
          {[...r.not_implemented, ...r.limitations, ...view.classification.not_implemented, ...view.classification.limitations].map((x) => (
            <li key={x}>{x}</li>
          ))}
        </ul>
      </details>
    </section>
  );
}

interface Ctx {
  view: RequirementsView;
  busy: boolean;
  /** The person's name, trimmed ("" until entered). */
  person: string;
  /** Why a person-only action is unavailable (no name yet), or undefined. */
  needName: string | undefined;
  editing: string | null;
  setEditing: (p: string | null) => void;
  run: (c: Command) => Promise<boolean>;
  /** Sections and lists the person has open. */
  opened: ReadonlySet<string>;
  toggled: (id: string, open: boolean) => void;
}

function FieldRow({ f, ctx }: { f: ReqFieldReport; ctx: Ctx }) {
  const { view, busy, person, needName, editing, setEditing, run } = ctx;
  const shown = showValue(f.value);
  const conflicts = f.conflicts ?? [];
  const status = f.status ?? (f.items?.length ? null : "UNKNOWN");
  const def = defOf(view, f.path);
  const items = !!def && typeof def.kind === "object";
  const canConfirm = !!f.value && (f.status === "NEEDS_CONFIRMATION" || f.status === "DERIVED") && conflicts.length === 0;
  const keep = (k: number) => run({ type: "ResolveRequirementConflict", path: f.path, keep: k, owner: person, note: null });
  const source = (x: { status: ReqStatus; source?: ReqSource | null }) => `${STATUS_LABEL[x.status]}${x.source ? `, ${x.source.kind}: ${x.source.reference}` : ""}`;
  return (
    <li className="req-field" data-path={f.path} data-status={status ?? "items"} data-level={f.level} data-conflicts={conflicts.length || undefined}>
      <div className="row">
        <span className="req-label">{f.label}</span>
        {status ? <span className="badge">{STATUS_LABEL[status]}</span> : <span className="badge">{f.items?.length} item(s)</span>}
        <span className="muted">{f.level}</span>
      </div>
      <div className="req-value">
        {shown ?? <span className="muted">{status === "NOT_APPLICABLE" ? "not applicable" : status === null ? "see its items" : "no value: UNKNOWN"}</span>}
        {f.source && <span className="muted"> · {f.source.kind}: {f.source.reference}</span>}
        {f.owner && <span className="muted"> · owner {f.owner}</span>}
        {f.risk_if_wrong && <span className="muted"> · if wrong: {f.risk_if_wrong}</span>}
        {f.derived_from?.length ? <span className="muted"> · derived from {f.derived_from.join(", ")}</span> : null}
      </div>
      {conflicts.length > 0 && (
        <div aria-label="Conflicting values" className="req-conflicts">
          <p className="error">{conflicts.length + 1} values disagree: this field counts as missing until a person keeps one.</p>
          <ol start={0}>
            <li>
              Recorded: {shown} <span className="muted">({source({ status: f.status ?? "UNKNOWN", source: f.source })})</span>{" "}
              <button data-keep="0" disabled={busy || !person} title={needName ?? "Keep the recorded value; it becomes CONFIRMED by you"} onClick={() => keep(0)}>
                Keep
              </button>
            </li>
            {conflicts.map((x: ReqEntry, i) => (
              <li key={i}>
                {showValue(x.value)} <span className="muted">({source(x)})</span>
                {x.note && <span className="muted"> · {x.note}</span>}{" "}
                <button data-keep={i + 1} disabled={busy || !person} title={needName ?? "Keep this value; it becomes CONFIRMED by you and the others are dropped"} onClick={() => keep(i + 1)}>
                  Keep
                </button>
              </li>
            ))}
          </ol>
        </div>
      )}
      {editing === f.path && def ? (
        <EditForm f={f} def={def} units={view.catalogue.units.filter((u) => u.measure === def.measure).map((u) => u.symbol)} ctx={ctx} />
      ) : (
        <div className="row">
          {!items && (
            <button disabled={busy} onClick={() => setEditing(f.path)}>
              {f.recorded ? "Edit" : "Enter"}
            </button>
          )}
          {canConfirm && (
            <button disabled={busy || !person} title={needName ?? "Record this value as CONFIRMED by you"} onClick={() => run(setField(f.path, "CONFIRMED", f.value ?? null, f.source ?? PANEL_SOURCE, person, null, f.note ?? null, false))}>
              Confirm
            </button>
          )}
          {f.recorded && !items && (
            <button disabled={busy} onClick={() => run({ type: "ClearRequirementField", path: f.path })}>
              Clear
            </button>
          )}
        </div>
      )}
    </li>
  );
}

function EditForm({ f, def, units, ctx }: { f: ReqFieldReport; def: ReqFieldDef; units: string[]; ctx: Ctx }) {
  const { busy, person, setEditing, run } = ctx;
  const v = f.value;
  const num = (x?: number) => (x == null ? "" : String(x));
  // An unanswered (UNKNOWN) or derived field is edited to give it a value: start at NEEDS_CONFIRMATION.
  const [status, setStatus] = useState<ReqStatus>(f.status === "CONFIRMED" || f.status === "NOT_APPLICABLE" ? f.status : "NEEDS_CONFIRMATION");
  const [text, setText] = useState(v?.kind === "text" ? v.text : "");
  const [flag, setFlag] = useState<boolean | null>(v?.kind === "flag" ? v.value : null);
  const [min, setMin] = useState(v?.kind === "range" ? num(v.min) : "");
  const [nom, setNom] = useState(v?.kind === "range" ? num(v.nom) : v?.kind === "number" ? String(v.value) : "");
  const [max, setMax] = useState(v?.kind === "range" ? num(v.max) : "");
  const [unit, setUnit] = useState(v && (v.kind === "range" || v.kind === "number") ? v.unit : (units[0] ?? ""));
  const [sourceKind, setSourceKind] = useState<ReqSourceKind>(f.source?.kind && f.source.kind !== "agent_proposal" ? f.source.kind : "engineer");
  const [reference, setReference] = useState(f.source?.reference ?? PANEL_SOURCE.reference);
  const [owner, setOwner] = useState(f.owner ?? person);
  const [risk, setRisk] = useState(f.risk_if_wrong ?? "");
  const [note, setNote] = useState(f.note ?? "");
  const withValue = status === "NEEDS_CONFIRMATION" || status === "CONFIRMED";
  const quantity = def.kind === "number" || def.kind === "range" || def.kind === "number_or_range";
  const value = (): ReqValue | null => {
    if (!withValue) return null;
    const n = (s: string) => (s.trim() === "" ? undefined : Number(s));
    switch (def.kind) {
      case "text":
        return { kind: "text", text };
      case "flag":
        return flag === null ? null : { kind: "flag", value: flag };
      case "number":
        return { kind: "number", value: Number(nom), unit };
      default:
        return { kind: "range", min: n(min), nom: n(nom), max: n(max), unit };
    }
  };
  const submit = (supersede: boolean) =>
    run(setField(f.path, status, value(), withValue ? { kind: sourceKind, reference } : null, owner.trim() || null, status === "NEEDS_CONFIRMATION" ? risk : null, note.trim() || null, supersede));
  return (
    <div className="req-edit" aria-label={`Edit ${f.label}`}>
      <div className="row" role="group" aria-label="Status">
        {EDIT_STATUSES.filter((s) => s !== "NOT_APPLICABLE" || def.na_allowed).map((s) => (
          <button key={s} aria-pressed={status === s} onClick={() => setStatus(s)}>
            {STATUS_LABEL[s]}
          </button>
        ))}
      </div>
      {withValue && def.kind === "text" && <textarea aria-label="Value" value={text} onChange={(e) => setText(e.target.value)} />}
      {withValue && def.kind === "flag" && (
        <div className="row" role="group" aria-label="Value">
          <button aria-pressed={flag === true} onClick={() => setFlag(true)}>
            Yes
          </button>
          <button aria-pressed={flag === false} onClick={() => setFlag(false)}>
            No
          </button>
        </div>
      )}
      {withValue && quantity && (
        <div className="row">
          {def.kind !== "number" && <input aria-label="Minimum" inputMode="decimal" size={6} value={min} onChange={(e) => setMin(e.target.value)} />}
          <input aria-label={def.kind === "number" ? "Value" : "Nominal"} inputMode="decimal" size={6} value={nom} onChange={(e) => setNom(e.target.value)} />
          {def.kind !== "number" && <input aria-label="Maximum" inputMode="decimal" size={6} value={max} onChange={(e) => setMax(e.target.value)} />}
          <select aria-label="Unit" value={unit} onChange={(e) => setUnit(e.target.value)}>
            {units.map((u) => (
              <option key={u}>{u}</option>
            ))}
          </select>
        </div>
      )}
      {withValue && (
        <div className="row">
          <select aria-label="Source kind" value={sourceKind} onChange={(e) => setSourceKind(e.target.value as ReqSourceKind)}>
            {SOURCE_KINDS.map((k) => (
              <option key={k}>{k}</option>
            ))}
          </select>
          <input aria-label="Source reference" value={reference} onChange={(e) => setReference(e.target.value)} />
        </div>
      )}
      <input aria-label="Owner" placeholder={status === "CONFIRMED" ? "who confirms it" : "who must answer or confirm it"} value={owner} onChange={(e) => setOwner(e.target.value)} />
      {status === "NEEDS_CONFIRMATION" && <input aria-label="Risk if wrong" placeholder="what goes wrong if this value is wrong" value={risk} onChange={(e) => setRisk(e.target.value)} />}
      <input aria-label="Note" placeholder={status === "NOT_APPLICABLE" ? "why it does not apply" : "note"} value={note} onChange={(e) => setNote(e.target.value)} />
      <p className="muted">{def.question}</p>
      <div className="row">
        <button disabled={busy} title="A different value is kept beside the recorded one as a conflict, never written over it" onClick={() => submit(false)}>
          Record
        </button>
        {f.recorded && (
          <button disabled={busy} title="Replace the recorded value and drop its conflicting values" onClick={() => submit(true)}>
            Replace
          </button>
        )}
        <button onClick={() => setEditing(null)}>Cancel</button>
      </div>
    </div>
  );
}

function Classification({ ctx }: { ctx: Ctx }) {
  const { view, busy, person, needName, editing, run, opened, toggled } = ctx;
  const cl = view.classification;
  const recorded = view.record.fields ?? {};
  const decide = (key: string, applies: boolean) => {
    const old = recorded[key];
    const source = old?.status === "CONFIRMED" && old.source ? old.source : { kind: "engineer" as const, reference: "decided in the Requirements panel" };
    return run(setField(key, "CONFIRMED", { kind: "flag", value: applies }, source, person, null, null, true));
  };
  const domainRows = view.report.domain;
  return (
    <section aria-label="Domain classification" className="reqs-domain">
      <h3 className="side-h">Domain classification</h3>
      <p className="muted">
        {cl.evaluator}: deterministic, from the requirement record only. {view.recorded_up_to_date ? "The recorded classification matches the record." : "The record has changed since the last classification."}
      </p>
      <button disabled={busy || view.recorded_up_to_date} title={view.recorded_up_to_date ? "Nothing to record: the classification already matches the record" : "Run the classifier and record its result (ClassifyDomain)"} onClick={() => run({ type: "ClassifyDomain" })}>
        Classify
      </button>
      {cl.classes.length === 0 ? (
        <p className="muted">No class has evidence yet. A class is never assumed: record supplies, interfaces, voltages and the environment.</p>
      ) : (
        <ul aria-label="Domain classes" className="reqs-list">
          {cl.classes.map((x) => {
            const e = recorded[x.key];
            return (
              <li key={x.id} data-class={x.id} data-verdict={x.verdict} data-recorded={e?.status ?? "none"} data-value={showValue(e?.value) ?? undefined}>
                <strong>{x.label}</strong> <span className="muted">({x.kind}, from DESK {x.desk})</span> <span className="badge">{VERDICT_LABEL[x.verdict]}</span>{" "}
                <span className="muted">recorded: {e ? `${STATUS_LABEL[e.status]} ${showValue(e.value) ?? ""}` : "not yet"}</span>
                <Evidence title="For" items={x.supports} />
                <Evidence title="Against" items={x.contradicts} />
                <div className="row">
                  <button disabled={busy || !person} title={needName ?? "Record that this class applies, CONFIRMED by you"} onClick={() => decide(x.key, true)}>
                    Confirm
                  </button>
                  <button disabled={busy || !person} title={needName ?? "Record that this class does not apply, CONFIRMED by you"} onClick={() => decide(x.key, false)}>
                    Reject
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}
      {domainRows.length > 0 && (
        <details open={opened.has("domain") || domainRows.some((f) => (f.conflicts?.length ?? 0) > 0 || f.path === editing)} onToggle={(e) => toggled("domain", e.currentTarget.open)}>
          <summary>Recorded classes ({domainRows.length})</summary>
          <ul className="reqs-fields" aria-label="Recorded classes">
            {domainRows.map((f) => (
              <FieldRow key={f.path} f={f} ctx={ctx} />
            ))}
          </ul>
        </details>
      )}
      <details>
        <summary>Not indicated ({cl.not_indicated.length}): no evidence yet, not ruled out</summary>
        <ul className="reqs-list muted">
          {cl.not_indicated.map((n) => (
            <li key={n.id}>
              {n.label}
              {n.unknown_inputs.length ? `: still UNKNOWN ${n.unknown_inputs.join(", ")}` : ""}
            </li>
          ))}
        </ul>
      </details>
    </section>
  );
}

function Evidence({ title, items }: { title: string; items: DomainEvidence[] }) {
  if (items.length === 0) return null;
  return (
    <ul aria-label={`Evidence ${title.toLowerCase()}`} className="reqs-evidence">
      {items.map((e) => (
        <li key={`${e.path}|${e.rule}`}>
          {title}: <code>{e.path}</code> = {e.observed}{" "}
          <span className="muted">
            ({e.status}, {e.level}, {e.strength})
          </span>
          : {e.rule}
        </li>
      ))}
    </ul>
  );
}

function AddItem({ busy, run }: { busy: boolean; run: (c: Command) => Promise<boolean> }) {
  const [name, setName] = useState("");
  const add = (kind: ReqItemKind) => run({ type: "UpsertRequirementItem", item_id: null, kind, name: name.trim(), parent: null }).then((ok) => ok && setName(""));
  return (
    <div className="row reqs-add" aria-label="Add a rail or interface">
      <input aria-label="New item name" placeholder="rail or interface name" value={name} onChange={(e) => setName(e.target.value)} />
      <button disabled={busy || !name.trim()} title="Add a supply rail with its own fields" onClick={() => add("rail")}>
        Add rail
      </button>
      <button disabled={busy || !name.trim()} title="Add an external interface with its own fields" onClick={() => add("interface")}>
        Add interface
      </button>
    </div>
  );
}
