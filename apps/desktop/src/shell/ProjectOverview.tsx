// Project overview (side panel "Overview"): what an engineer needs to know on opening a project,
// with links into the view that acts on it. Everything shown is read from real state:
//  * the engine snapshot (schematic, PCB, library, revision, undo),
//  * check reports run in this session (ERC/DRC/DFM; a check not run here says "Not run", never
//    "passing"; a report older than the current revision is marked stale),
//  * the engine's connectivity analysis (unrouted connections, shorts) for this revision,
//  * the domain classifier's report (project type and verification requirements, with maturity),
//  * engine events (recent activity) and the terminal runtime (sessions bound to this project).
// Nothing is estimated: when a value is unknown the row says so.
import { useEffect, useState } from "react";
import { ago } from "../ProjectHome";
import { Fact, Pill, Section, StatusDot, type Tone } from "../design/ui";
import { term, type SessionInfo } from "../terminal/api";
import type { Check, CheckResult, ConnectivityReport, DomainReport, EngineEvent, Snapshot } from "../types";
import { netNames } from "./panels";

interface Props {
  snapshot: Snapshot;
  results: Partial<Record<Check, CheckResult>>;
  conn: ConnectivityReport | null;
  domain: DomainReport | null;
  events: EngineEvent[];
  busy: boolean;
  /** Show an editor or registered EDA view ("schematic", "pcb", "viewer3d"). */
  onView: (id: string) => void;
  /** Ids of registered EDA views, so a link is offered only when its view exists. */
  views: string[];
  onCheck: (c: Check) => void;
  onFindings: () => void;
  onLayers: () => void;
  onParts: () => void;
}

/** Numbers derived from the snapshot alone (pure). */
export function designFacts(s: Snapshot) {
  const { schematic, pcb, library } = s.project;
  const symbols = Object.values(schematic.symbols);
  const placed = new Set(Object.values(pcb.footprints).map((f) => f.symbol_id));
  return {
    sheets: Object.keys(schematic.sheets).length,
    parts: symbols.length,
    wires: Object.keys(schematic.wires).length,
    nets: netNames(s).length,
    footprints: Object.keys(pcb.footprints).length,
    /** Schematic parts with no footprint on the board yet (Update PCB not run for them). */
    missingFootprints: symbols.filter((sym) => !placed.has(sym.id)).length,
    tracks: Object.keys(pcb.tracks).length,
    vias: Object.keys(pcb.vias).length,
    zones: Object.keys(pcb.zones).length,
    outline: pcb.board.outline.length >= 3,
    copperLayers: pcb.board.stackup ? pcb.board.stackup.layers.length : null,
    bomLines: new Set(symbols.map((sym) => `${sym.component_id}|${sym.value}`)).size,
    withDistributor: symbols.filter((sym) => !!library?.[sym.component_id]?.distributor).length,
  };
}

/** One check's state as shown: never "pass", only what the report says, and whether it is current. */
export function checkState(r: CheckResult | undefined, revision: number): { tone: Tone; text: string } {
  if (!r) return { tone: "neutral", text: "Not run" };
  const { errors, warnings, revision: at } = r.report;
  const stale = at !== revision ? ` · r${at}, stale` : "";
  if (errors > 0) return { tone: "bad", text: `${errors} error${errors > 1 ? "s" : ""}${warnings ? `, ${warnings} warn` : ""}${stale}` };
  if (warnings > 0) return { tone: "warn", text: `${warnings} warning${warnings > 1 ? "s" : ""}${stale}` };
  return { tone: stale ? "neutral" : "ok", text: `No errors in implemented checks${stale}` };
}

const CHECKS: { id: Check; label: string }[] = [
  { id: "erc", label: "ERC" },
  { id: "drc", label: "DRC" },
  { id: "dfm", label: "DFM" },
];

export function ProjectOverview({ snapshot, results, conn, domain, events, busy, onView, views, onCheck, onFindings, onLayers, onParts }: Props) {
  const { project } = snapshot;
  const f = designFacts(snapshot);
  const live = conn && conn.revision === project.revision ? conn : null;
  const cls = domain?.classification;
  const guidance = cls?.guidance ?? null;
  const sessions = useProjectSessions(project.id);
  const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;

  return (
    <div className="panel-body overview">
      <Section title="Project">
        <dl className="facts">
          <Fact name="Name">{project.name}</Fact>
          <Fact name="Folder"><span className="mono">{snapshot.dir ?? "—"}</span></Fact>
          <Fact name="Revision">
            r{project.revision}
            {snapshot.can_undo ? <span className="muted"> · undo available</span> : null}
          </Fact>
          <Fact name="Type">
            {cls?.primary ? (
              <>
                {cls.primary}{" "}
                <Pill tone={cls.status === "CLASSIFIED" ? "info" : "warn"}>{cls.status === "CLASSIFIED" ? `${Math.round(cls.confidence * 100)} %` : "review"}</Pill>
              </>
            ) : (
              <span className="muted">{domain ? "Unclassified" : "Classifying…"}</span>
            )}
          </Fact>
          {snapshot.blocked && (
            <Fact name="Writes">
              <Pill tone="bad">Blocked</Pill>
            </Fact>
          )}
        </dl>
      </Section>

      <Section title="Brief" label="Design brief">
        {(project.requirements ?? []).length === 0 ? (
          <p className="muted overview-note">No design brief yet. Starting a project from a description on Project Home records one, line by line.</p>
        ) : (
          <ol className="overview-list" aria-label="Brief lines">
            {(project.requirements ?? []).map((r) => (
              <li key={r.id}>{r.text}</li>
            ))}
          </ol>
        )}
      </Section>

      <Section title="Schematic" aside={<button type="button" className="linkish" onClick={() => onView("schematic")}>Open</button>}>
        <dl className="facts">
          <Fact name="Sheets">{f.sheets}</Fact>
          <Fact name="Parts">{f.parts}</Fact>
          <Fact name="Nets">{f.nets}</Fact>
          <Fact name="Wires">{f.wires}</Fact>
        </dl>
      </Section>

      <Section title="PCB" aside={<button type="button" className="linkish" onClick={() => onView("pcb")}>Open</button>}>
        <dl className="facts">
          <Fact name="Board outline">{f.outline ? "Defined" : <Pill tone="warn">Not defined</Pill>}</Fact>
          {f.copperLayers !== null && <Fact name="Copper layers">{f.copperLayers}</Fact>}
          <Fact name="Footprints">
            {f.footprints}
            {f.missingFootprints > 0 && (
              <>
                {" "}
                <Pill tone="warn" title="Schematic parts with no footprint on the board: run Update PCB">{f.missingFootprints} not on board</Pill>
              </>
            )}
          </Fact>
          <Fact name="Routing">
            {live ? (
              live.ratsnest.length === 0 ? (
                f.tracks > 0 ? "No unrouted connections" : <span className="muted">No connections to route</span>
              ) : (
                <Pill tone="warn">{live.ratsnest.length} unrouted</Pill>
              )
            ) : (
              <span className="muted">Not analysed for r{project.revision}</span>
            )}
            {live && live.shorts.length > 0 && (
              <>
                {" "}
                <Pill tone="bad">{plural(live.shorts.length, "short")}</Pill>
              </>
            )}
          </Fact>
          <Fact name="Copper">
            {plural(f.tracks, "track")} · {plural(f.vias, "via")} · {plural(f.zones, "zone")}
          </Fact>
        </dl>
        <div className="overview-links">
          <button type="button" className="linkish" onClick={onLayers}>Layers</button>
          {views.includes("viewer3d") && (
            <button type="button" className="linkish" onClick={() => onView("viewer3d")}>3D view</button>
          )}
        </div>
      </Section>

      <Section title="Components / BOM" aside={<button type="button" className="linkish" onClick={onParts}>Library</button>}>
        <dl className="facts">
          <Fact name="BOM lines">{f.bomLines}</Fact>
          <Fact name="With Distributor part">{f.parts === 0 ? "—" : `${f.withDistributor} of ${f.parts}`}</Fact>
        </dl>
      </Section>

      <Section title="Validation" aside={<button type="button" className="linkish" onClick={onFindings}>Findings</button>}>
        <ul className="overview-checks">
          {CHECKS.map((c) => {
            const st = checkState(results[c.id], project.revision);
            const r = results[c.id]?.report;
            return (
              <li key={c.id}>
                <span className="overview-check">{c.label}</span>
                <Pill tone={st.tone}>{st.text}</Pill>
                {r && r.not_implemented.length > 0 && (
                  <span className="muted" title={r.not_implemented.join("\n")}>
                    {r.not_implemented.length} not implemented
                  </span>
                )}
                <span className="spacer" />
                <button type="button" className="linkish" disabled={busy} onClick={() => onCheck(c.id)} aria-label={`Run ${c.label} from overview`}>
                  Run
                </button>
              </li>
            );
          })}
        </ul>
        <p className="muted overview-note">Results are from checks run in this session. EAK never reports a design as manufacturing-ready.</p>
      </Section>

      {guidance && guidance.verification_requirements.length > 0 && (
        <Section
          title="Requirements"
          aside={
            <Pill tone="neutral" title={guidance.ceiling.claim}>
              {guidance.ceiling.label.toLowerCase().replace(/_/g, " ")}
            </Pill>
          }
        >
          <ul className="overview-list">
            {guidance.verification_requirements.slice(0, 5).map((r) => (
              <li key={r}>{r}</li>
            ))}
          </ul>
          {cls && cls.missing_information.length > 0 && <p className="muted overview-note">Missing information: {cls.missing_information.slice(0, 3).join("; ")}</p>}
        </Section>
      )}

      <Section title="Recent activity">
        {events.length === 0 ? (
          <p className="muted overview-note">No engine activity in this session yet.</p>
        ) : (
          <ol className="overview-rows">
            {events
              .slice(-6)
              .reverse()
              .map((e) => (
                <li key={e.seq} title={`${e.kind} by ${e.actor}`}>
                  <span className="grow">{e.summary}</span>
                  <span className="muted mono">r{e.revision}</span>
                  <span className="muted">{ago(e.timestamp_ms)}</span>
                </li>
              ))}
          </ol>
        )}
      </Section>

      {sessions && (
        <Section title="Terminal sessions">
          {sessions.length === 0 ? (
            <p className="muted overview-note">No terminal sessions on this project. Open the Terminal workspace from the app bar.</p>
          ) : (
            <ul className="overview-rows">
              {sessions.map((s) => (
                <li key={s.id} title={`${s.status.detail} · ${s.spec.cwd}`}>
                  <StatusDot state={s.status.state} />
                  <span className="grow">{s.name}</span>
                  {s.status.agent && <span className="chip">{s.status.agent}</span>}
                  <span className="muted">{s.status.state.replace(/_/g, " ")}</span>
                </li>
              ))}
            </ul>
          )}
        </Section>
      )}
    </div>
  );
}

/** Terminal sessions in spaces bound to this project, polled while the overview is shown.
 *  `null` when the terminal runtime is unavailable (the section is then hidden, not faked). */
function useProjectSessions(projectId: string): SessionInfo[] | null {
  const [sessions, setSessions] = useState<SessionInfo[] | null>(null);
  useEffect(() => {
    let live = true;
    const load = () =>
      term
        .overview()
        .then((ov) => {
          if (!live) return;
          const mine = new Set(ov.spaces.filter((sp) => sp.project_id === projectId).map((sp) => sp.id));
          setSessions(ov.sessions.filter((s) => mine.has(s.space)));
        })
        .catch(() => {
          if (live) setSessions(null);
        });
    void load();
    const t = window.setInterval(load, 4000);
    return () => {
      live = false;
      window.clearInterval(t);
    };
  }, [projectId]);
  return sessions;
}
