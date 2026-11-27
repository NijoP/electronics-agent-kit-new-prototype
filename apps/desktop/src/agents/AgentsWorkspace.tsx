// Agents workspace (M2.1/M2.2, docs/product/EAK_AGENT_WORKSPACE_SPEC.md, ADR-0058): the control
// room for agent work on the open project. Context bar · roster (Needs you, Agents, Tasks,
// Sessions, Runtimes) · work surface for the selection · inspector · activity timeline.
//
// It reads only what EAK recorded: the WEFT journal (`weft_state`), the engineer's review queue and
// conflicts (ADR-0053), the activity timeline, and the AI tools' sessions and probes (ADR-0054). Its
// actions are the existing paths: Launch / Sign in / Connect / Stop / Restart through the AI tools
// IPC, a verdict on the WEFT bus, and Apply inside EAK's proposal review. A capability that does not
// exist yet (send a message to a session, interrupt, resume, create a task) is said to be
// unavailable, with the reason; nothing here is simulated. It polls files while shown, never a model.
import { useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { api } from "../api";
import { EmptyState, Pill } from "../design/ui";
import {
  connectBlocker,
  connectedText,
  eventText,
  foundText,
  launchBlocker,
  loginBlocker,
  reasonText,
  SESSION_STATE_LABEL,
  signedInText,
  type AgentSession,
  type Surface,
  type ToolStatus,
} from "../settings/aiTools";
import { actorName, type ActivityItem, type EngineerRequest, type WeftConflict } from "../shell/activity";
import { ProposalReviewDialog } from "../shell/ProposalReviewDialog";
import {
  clock,
  CONTRACT_GAPS,
  delivery,
  inProject,
  matches,
  MESSAGE_KIND_LABEL,
  needsYou,
  OVERVIEW,
  permissionText,
  revisionText,
  runtimeText,
  selectionKey,
  SESSION_TONE,
  sessionOf,
  stopBlocker,
  TASK_STATUS_LABEL,
  TASK_TONE,
  taskTree,
  threadOf,
  who,
  type NeedsYou,
  type Selection,
} from "./agentsModel";
import type { WeftMessageView, WeftView } from "./weft";
import "./agents.css";

/** How often the journals and sessions are re-read while the workspace is shown. */
export const POLL_MS = 2000;
const NO_PROJECT = "No project is open: the WEFT bus, its tasks and the activity timeline belong to a project. Sessions and runtimes are listed in the roster.";
const message = (e: unknown) => String(e instanceof Error ? e.message : e).replace(/^Error: /, "");
const secs = (s: number) => (s ? new Date(s * 1000).toLocaleString() : "—");

interface Props {
  active: boolean;
  projectId: string | null;
  projectName: string | null;
  projectDir: string | null;
  /** Opens Model settings (providers and models), when the shell offers it. */
  openModelSettings?: () => void;
  /** Shows another top-level workspace (the Terminal), when the shell offers it. */
  showWorkspace?: (id: string) => void;
}

function Facts({ rows }: { rows: [string, ReactNode][] }) {
  return (
    <dl className="facts ag-facts">
      {rows.map(([k, v]) => (
        <div className="fact" key={k}>
          <dt>{k}</dt>
          <dd>{v}</dd>
        </div>
      ))}
    </dl>
  );
}

export function AgentsWorkspace({ active, projectId, projectName, projectDir, openModelSettings, showWorkspace }: Props) {
  const [weft, setWeft] = useState<WeftView | null>(null);
  const [indexError, setIndexError] = useState<string | null>(null);
  const [queue, setQueue] = useState<EngineerRequest[]>([]);
  const [conflicts, setConflicts] = useState<WeftConflict[]>([]);
  const [activity, setActivity] = useState<ActivityItem[]>([]);
  const [readErrors, setReadErrors] = useState<string[]>([]);
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [sessionsError, setSessionsError] = useState<string | null>(null);
  const [tools, setTools] = useState<ToolStatus[] | null>(null);
  const [toolsError, setToolsError] = useState<string | null>(null);
  const [probing, setProbing] = useState(false);
  const [probed, setProbed] = useState(false);
  const [sel, setSel] = useState<Selection>(OVERVIEW);
  const [filter, setFilter] = useState("");
  const [thisProject, setThisProject] = useState(true);
  const [activityOpen, setActivityOpen] = useState(true);
  const [log, setLog] = useState<{ id: string; text: string; error: string | null } | null>(null);
  const [notice, setNotice] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [review, setReview] = useState<string | null>(null);
  const [surface, setSurface] = useState<Surface>("terminal");
  const [prompt, setPrompt] = useState("");
  const filterRef = useRef<HTMLInputElement>(null);

  const loadProject = useCallback(async () => {
    if (!projectId) {
      setWeft(null);
      setQueue([]);
      setConflicts([]);
      setActivity([]);
      setReadErrors([]);
      return;
    }
    const [w, q, c, a] = await Promise.allSettled([api.weftState(), api.weftEngineerQueue(), api.weftConflicts(), api.activityTimeline()]);
    const errors: string[] = [];
    // A failed read shows its error and no stale list as if it were current.
    if (w.status === "fulfilled") {
      setWeft(w.value);
    } else {
      setWeft(null);
      errors.push(`WEFT journal: ${message(w.reason)}`);
    }
    if (q.status === "fulfilled") {
      setQueue(Array.isArray(q.value) ? q.value : []);
    } else {
      setQueue([]);
      errors.push(`Review queue: ${message(q.reason)}`);
    }
    if (c.status === "fulfilled") {
      setConflicts(Array.isArray(c.value) ? c.value : []);
    } else {
      setConflicts([]);
      errors.push(`Conflicts: ${message(c.reason)}`);
    }
    if (a.status === "fulfilled") {
      setActivity(Array.isArray(a.value) ? a.value : []);
    } else {
      setActivity([]);
      errors.push(`Activity: ${message(a.reason)}`);
    }
    setReadErrors(errors);
  }, [projectId]);

  const loadSessions = useCallback(async () => {
    try {
      const s = await api.agentSessions();
      setSessions(Array.isArray(s) ? s : []);
      setSessionsError(null);
    } catch (e) {
      setSessions([]);
      setSessionsError(message(e));
    }
    // A session index EAK could not write is said, not hidden (ADR-0058).
    try {
      setIndexError((await api.agentSessionIndexError()) ?? null);
    } catch (e) {
      setIndexError(`EAK could not check its session index: ${message(e)}`);
    }
  }, []);

  const probe = useCallback(async () => {
    setProbing(true);
    try {
      const t = await api.agentTools();
      setTools(Array.isArray(t) ? t : []);
      setToolsError(null);
    } catch (e) {
      setTools(null);
      setToolsError(message(e));
    } finally {
      setProbing(false);
    }
  }, []);

  const refresh = useCallback(async () => {
    await Promise.all([loadProject(), loadSessions()]);
  }, [loadProject, loadSessions]);

  // Re-read while shown; nothing runs while another workspace is shown.
  useEffect(() => {
    if (!active) return undefined;
    let running = false;
    const tick = async () => {
      if (running) return;
      running = true;
      try {
        await refresh();
      } finally {
        running = false;
      }
    };
    void tick();
    const t = window.setInterval(() => void tick(), POLL_MS);
    return () => window.clearInterval(t);
  }, [active, refresh]);

  useEffect(() => {
    if (active && !probed) {
      setProbed(true);
      void probe();
    }
  }, [active, probed, probe]);

  const selSession = sel.kind === "session" ? (sessions.find((s) => s.id === sel.id) ?? null) : null;
  const selSessionId = selSession?.id ?? null;
  const selRunning = selSession?.state === "running";
  // The selected session's log; a live tail while it runs.
  useEffect(() => {
    if (!selSessionId) return undefined;
    let live = true;
    const read = () =>
      api.agentSessionLog(selSessionId, 200).then(
        (text) => {
          if (live) setLog({ id: selSessionId, text, error: null });
        },
        (e) => {
          if (live) setLog({ id: selSessionId, text: "", error: message(e) });
        },
      );
    void read();
    const t = selRunning && active ? window.setInterval(() => void read(), POLL_MS) : null;
    return () => {
      live = false;
      if (t !== null) window.clearInterval(t);
    };
  }, [selSessionId, selRunning, active]);

  /** Run one action; its text (or error) is the notice, then everything is re-read. */
  const act = async (key: string, run: () => Promise<string>) => {
    setBusy(key);
    setNotice(null);
    try {
      setNotice({ ok: true, text: await run() });
    } catch (e) {
      setNotice({ ok: false, text: message(e) });
    } finally {
      setBusy(null);
      void refresh();
    }
  };

  // The engineer's answer goes on the WEFT bus to the proposal's author, then the proposal is
  // discarded (weft::engineer_verdict, ADR-0053). Nothing here applies a proposal.
  const verdict = (id: string, v: "request_changes" | "reject", body: string, done: string) =>
    act(`verdict:${id}`, async () => {
      await api.weftEngineerVerdict(id, v, body, true);
      return done;
    });

  const revision = weft?.revision ?? null;
  const dir = weft?.project_dir ?? projectDir;
  const needs = needsYou(weft, queue, conflicts, sessions.filter((s) => inProject(s, dir)));
  const agents = (weft?.agents ?? []).filter((a) => matches(filter, a.card.id, a.card.name, a.card.tool, a.card.model));
  const tree = taskTree(weft?.tasks ?? []);
  const tasks = tree.filter((r) => matches(filter, r.task.task.id, r.task.task.title, r.task.task.owner, r.task.task.status));
  const shownSessions = sessions.filter((s) => (!thisProject || inProject(s, dir)) && matches(filter, s.id, s.label, s.tool, s.state, s.prompt));
  const runtimes = (tools ?? []).filter((t) => matches(filter, t.id, t.label));
  const messages = weft?.messages ?? [];
  const off = busy !== null;

  const select = (s: Selection) => {
    setSel(s);
    setNotice(null);
  };
  const isSel = (s: Selection) => selectionKey(s) === selectionKey(sel);
  const row = (s: Selection, body: ReactNode, label: string) => (
    <button type="button" className={isSel(s) ? "ag-row on" : "ag-row"} data-roster-row={selectionKey(s)} aria-current={isSel(s) ? "true" : undefined} aria-label={label} onClick={() => select(s)}>
      {body}
    </button>
  );

  // Keyboard: "/" focuses the filter, Escape goes back to the overview (or clears the filter),
  // ↑/↓ move between roster rows. Keys typed into a field stay there.
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (review) return;
    const t = e.target as HTMLElement;
    const typing = t.tagName === "INPUT" || t.tagName === "TEXTAREA";
    if (typing) {
      if (e.key === "Escape" && t === filterRef.current && filter) setFilter("");
      return;
    }
    if (e.key === "/") {
      e.preventDefault();
      filterRef.current?.focus();
    } else if (e.key === "Escape") {
      select(OVERVIEW);
    } else if ((e.key === "ArrowDown" || e.key === "ArrowUp") && t.dataset.rosterRow) {
      const rows = [...e.currentTarget.querySelectorAll<HTMLButtonElement>("[data-roster-row]")];
      const next = rows[rows.indexOf(t as HTMLButtonElement) + (e.key === "ArrowDown" ? 1 : -1)];
      if (next) {
        e.preventDefault();
        next.focus();
      }
    }
  };

  const reviewActions = (pid: string) => (
    <span className="ag-actions">
      <button type="button" className="primary" disabled={off || !projectId} title={!projectId ? "Open the project first" : "Open EAK's review of this proposal (Apply is there)"} onClick={() => setReview(pid)}>
        Review proposal
      </button>
      <button
        type="button"
        disabled={off}
        title="Discard it and ask its agent on the WEFT bus to propose again on the current revision"
        onClick={() => void verdict(pid, "request_changes", `Propose again on revision ${revision ?? "now"}.`, `Proposal ${pid} sent back: discarded, and its agent was asked on the WEFT bus to propose again.`)}
      >
        Request changes
      </button>
      <button type="button" disabled={off} title="Discard it and tell its agent on the WEFT bus" onClick={() => void verdict(pid, "reject", "Rejected by the engineer.", `Proposal ${pid} rejected: discarded, and its agent was told on the WEFT bus.`)}>
        Reject
      </button>
    </span>
  );

  const needsRow = (n: NeedsYou) => (
    <li key={n.key} className="ag-need" data-need={n.kind}>
      <span className="ag-need-text">
        <strong>{n.title}</strong>
        <span className="muted">{n.detail}</span>
      </span>
      {n.proposalId && reviewActions(n.proposalId)}
      {n.select && !n.proposalId && (
        <button type="button" onClick={() => select(n.select!)}>
          Open
        </button>
      )}
    </li>
  );

  const thread = (ms: WeftMessageView[], empty: string) =>
    ms.length === 0 ? (
      <p className="muted">{empty}</p>
    ) : (
      <ol className="ag-thread" aria-label="Messages">
        {ms.map((m) => {
          const d = delivery(m, messages);
          return (
            <li key={m.seq} data-message={m.message.kind}>
              <div className="ag-msg-head">
                <Pill tone={m.message.kind === "blocker" ? "warn" : m.message.kind === "review_request" ? "info" : "neutral"}>{MESSAGE_KIND_LABEL[m.message.kind]}</Pill>
                <span>
                  {who(m.message.from)} → {who(m.message.to)}
                </span>
                <span className="muted">
                  {clock(m.ts_ms)} · r{m.revision}
                </span>
              </div>
              {m.message.body && <p className="ag-msg-body">{m.message.body}</p>}
              <div className="ag-msg-meta muted">
                {m.message.verdict && <span>verdict: {m.message.verdict.replace("_", " ")} · </span>}
                {m.message.status && <span>status: {TASK_STATUS_LABEL[m.message.status]} · </span>}
                {m.message.proposal_id && <span>proposal {m.message.proposal_id} · </span>}
                <span data-delivery={d.answered === null ? "none" : d.answered ? "answered" : "awaiting"}>{d.text}</span>
              </div>
            </li>
          );
        })}
      </ol>
    );

  const taskRow = (id: string, title: string, extra: ReactNode) => <li key={id}>{row({ kind: "task", id }, <><span>{title}</span>{extra}</>, `Task ${title}`)}</li>;

  // ---- Work surface ------------------------------------------------------------------------
  const surfaceFor = (): ReactNode => {
    if (sel.kind === "overview") {
      return (
        <>
          <h2>Overview</h2>
          {!projectId && <EmptyState title="No project open">{NO_PROJECT}</EmptyState>}
          <section className="ag-block" aria-label="Needs you now">
            <h3>Needs you</h3>
            {needs.length === 0 ? <p className="muted">Nothing waits on you: no review request, conflict, blocked task or failed session in this project.</p> : <ul className="ag-needs">{needs.map(needsRow)}</ul>}
          </section>
          <section className="ag-block" aria-label="Task graph">
            <h3>Task graph</h3>
            {tree.length === 0 ? (
              <p className="muted">No task delegated on this project's WEFT bus yet. Agents delegate to each other with weft_send (kind delegate); EAK does not invent tasks for them.</p>
            ) : (
              <table className="ag-table" aria-label="Tasks">
                <thead>
                  <tr>
                    <th>Task</th>
                    <th>Status</th>
                    <th>Owner</th>
                    <th>Waits on</th>
                    <th>Revision</th>
                  </tr>
                </thead>
                <tbody>
                  {tree.map(({ task: t, depth }) => (
                    <tr key={t.task.id} data-task={t.task.id}>
                      <td style={{ paddingLeft: `calc(${depth} * var(--s-2) + var(--s-1))` }}>
                        <button type="button" className="ag-link" onClick={() => select({ kind: "task", id: t.task.id })}>
                          {t.task.title}
                        </button>
                      </td>
                      <td>
                        <Pill tone={TASK_TONE[t.task.status]}>{TASK_STATUS_LABEL[t.task.status]}</Pill>
                        {t.ready && <span className="muted"> ready</span>}
                      </td>
                      <td>{who(t.task.owner)}</td>
                      <td>{t.waiting_on.length ? t.waiting_on.join(", ") : "—"}</td>
                      <td className="muted">{revision === null ? `r${t.created_revision}` : revisionText(t.created_revision, revision)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </section>
          <section className="ag-block" aria-label="Recent messages">
            <h3>Recent messages</h3>
            {thread(messages.slice(-8), "No message on this project's WEFT bus yet.")}
          </section>
        </>
      );
    }
    if (sel.kind === "agent") {
      const a = weft?.agents.find((x) => x.card.id === sel.id) ?? null;
      const owned = (weft?.tasks ?? []).filter((t) => t.task.owner === sel.id);
      const delegated = (weft?.tasks ?? []).filter((t) => t.task.created_by === sel.id);
      const s = a ? sessionOf(a, sessions) : null;
      return (
        <>
          <h2>{who(sel.id)}</h2>
          {!a && <EmptyState title="Not registered on this project's WEFT bus">{sel.id} sent or was named in a message but has no registration in the journal.</EmptyState>}
          {s && (
            <p>
              Started from EAK as session{" "}
              <button type="button" className="ag-link" onClick={() => select({ kind: "session", id: s.id })}>
                {s.id}
              </button>{" "}
              <Pill tone={SESSION_TONE[s.state]}>{SESSION_STATE_LABEL[s.state]}</Pill>
            </p>
          )}
          <section className="ag-block" aria-label="Tasks it owns">
            <h3>Owns</h3>
            {owned.length === 0 ? <p className="muted">No task delegated to it.</p> : <ul className="ag-list">{owned.map((t) => taskRow(t.task.id, t.task.title, <Pill tone={TASK_TONE[t.task.status]}>{TASK_STATUS_LABEL[t.task.status]}</Pill>))}</ul>}
          </section>
          <section className="ag-block" aria-label="Tasks it delegated">
            <h3>Delegated</h3>
            {delegated.length === 0 ? <p className="muted">It has not delegated a task.</p> : <ul className="ag-list">{delegated.map((t) => taskRow(t.task.id, t.task.title, <span className="muted">to {who(t.task.owner)}</span>))}</ul>}
          </section>
          <section className="ag-block" aria-label="Agent messages">
            <h3>Messages</h3>
            {thread(threadOf(sel, messages), "No message from or to this agent.")}
          </section>
        </>
      );
    }
    if (sel.kind === "task") {
      const t = weft?.tasks.find((x) => x.task.id === sel.id) ?? null;
      if (!t) return <EmptyState title="Task not found">No task {sel.id} in this project's WEFT journal.</EmptyState>;
      const children = (weft?.tasks ?? []).filter((x) => x.task.parent === t.task.id);
      const ms = threadOf(sel, messages);
      const proposals = [...new Set(ms.map((m) => m.message.proposal_id).filter((p): p is string => !!p))];
      const dep = (id: string) => weft?.tasks.find((x) => x.task.id === id);
      return (
        <>
          <h2>{t.task.title}</h2>
          <p>
            <Pill tone={TASK_TONE[t.task.status]}>{TASK_STATUS_LABEL[t.task.status]}</Pill> owned by{" "}
            <button type="button" className="ag-link" onClick={() => select({ kind: "agent", id: t.task.owner })}>
              {who(t.task.owner)}
            </button>
            , delegated by{" "}
            <button type="button" className="ag-link" onClick={() => select({ kind: "agent", id: t.task.created_by })}>
              {who(t.task.created_by)}
            </button>
          </p>
          <section className="ag-block" aria-label="Dependencies">
            <h3>Depends on</h3>
            {t.task.depends_on.length === 0 ? (
              <p className="muted">No dependency.</p>
            ) : (
              <ul className="ag-list">
                {t.task.depends_on.map((d) => {
                  const x = dep(d);
                  return taskRow(d, x?.task.title ?? d, x ? <Pill tone={TASK_TONE[x.task.status]}>{TASK_STATUS_LABEL[x.task.status]}</Pill> : null);
                })}
              </ul>
            )}
          </section>
          {children.length > 0 && (
            <section className="ag-block" aria-label="Sub-tasks">
              <h3>Sub-tasks</h3>
              <ul className="ag-list">{children.map((c) => taskRow(c.task.id, c.task.title, <Pill tone={TASK_TONE[c.task.status]}>{TASK_STATUS_LABEL[c.task.status]}</Pill>))}</ul>
            </section>
          )}
          {proposals.length > 0 && (
            <section className="ag-block" aria-label="Proposals">
              <h3>Proposals</h3>
              <ul className="ag-needs">
                {proposals.map((p) => (
                  <li key={p} className="ag-need">
                    <span className="ag-need-text">
                      <strong>{p}</strong>
                    </span>
                    <button type="button" disabled={off || !projectId} title={!projectId ? "Open the project first" : "Open EAK's review of this proposal"} onClick={() => setReview(p)}>
                      Review proposal
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
          <section className="ag-block" aria-label="Task messages">
            <h3>Messages</h3>
            {thread(ms, "No message about this task.")}
          </section>
        </>
      );
    }
    if (sel.kind === "session") {
      const s = selSession;
      if (!s) return <EmptyState title="Session not found">No session {sel.id} in EAK's session index.</EmptyState>;
      const stop = stopBlocker(s);
      const shownLog = log?.id === s.id ? log : null;
      const terminalBlocker = !showWorkspace
        ? "The Terminal workspace is not available here"
        : s.from_earlier_run
          ? "Its terminal tab belonged to an earlier EAK run and closed with it"
          : s.state !== "running"
            ? "Its terminal tab is closed"
            : null;
      return (
        <>
          <h2>
            {s.label} · <code>{s.id}</code>
          </h2>
          <p>
            <Pill tone={SESSION_TONE[s.state]}>{SESSION_STATE_LABEL[s.state]}</Pill> <span className="muted">{s.detail}</span>
            {s.exit_code !== null && <span className="muted"> · exit {s.exit_code}</span>}
          </p>
          <section className="ag-block" aria-label="First message">
            <h3>First message</h3>
            <p className={s.prompt ? "ag-brief" : "muted"}>{s.prompt ?? "None: it started without a first message."}</p>
          </section>
          <div className="ag-actions">
            <button
              type="button"
              disabled={off || !!stop}
              title={stop ?? "Stop it: close its terminal tab or end its process"}
              onClick={() => void act(`stop:${s.id}`, async () => `${s.label} session: ${SESSION_STATE_LABEL[(await api.agentToolStop(s.id)).state]}`)}
            >
              Stop
            </button>
            <button
              type="button"
              disabled={off}
              title={s.state === "running" ? "Stop it and start it again with the same first message (a new session)" : "Start it again with the same runtime, surface and first message (a new session)"}
              onClick={() =>
                void act(`restart:${s.id}`, async () => {
                  const v = await api.agentSessionRestart(s.id);
                  setSel({ kind: "session", id: v.id });
                  return `${v.label} started again as session ${v.id}`;
                })
              }
            >
              Restart
            </button>
            {s.surface === "terminal" && (
              <button type="button" disabled={!!terminalBlocker} title={terminalBlocker ?? "Show the Terminal workspace, where its tab runs"} onClick={() => showWorkspace?.("terminal")}>
                Show in Terminal
              </button>
            )}
          </div>
          <p className="muted">Interrupt, resume and sending a message to a running session are not available: no adapter declares them yet (M2.8, M2.3/M2.4).</p>
          <section className="ag-block" aria-label="Session events">
            <h3>Events</h3>
            {s.events.length === 0 ? (
              <p className="muted">{s.surface === "terminal" ? "A terminal session reports no parsed events; its output is in its tab." : "No event parsed from its stream yet."}</p>
            ) : (
              <ol className="ag-events">
                {s.events.map((e, i) => (
                  <li key={i}>{eventText(e)}</li>
                ))}
              </ol>
            )}
          </section>
          <section className="ag-block" aria-label="Session log">
            <h3>Log {s.state === "running" && <span className="muted">(live, every {POLL_MS / 1000} s)</span>}</h3>
            {shownLog?.error ? <p className="muted">{shownLog.error}</p> : <pre className="ag-log">{shownLog ? shownLog.text || "(no output yet)" : "Reading…"}</pre>}
          </section>
        </>
      );
    }
    const t = (tools ?? []).find((x) => x.id === sel.id) ?? null;
    if (!t) return <EmptyState title="Runtime not found">No coding-agent runtime {sel.id} was probed.</EmptyState>;
    const projectOpen = !!projectId;
    const launch = launchBlocker(t, surface, prompt, projectOpen);
    const login = loginBlocker(t);
    const connect = connectBlocker(t, projectOpen);
    return (
      <>
        <h2>{t.label}</h2>
        <p className="muted">{reasonText(t) ?? "Ready: installed, and connected to this project."}</p>
        <section className="ag-block" aria-label="New session">
          <h3>New session</h3>
          <div className="ag-launch">
            <label className="inline">
              <input type="radio" name="ag-surface" checked={surface === "terminal"} onChange={() => setSurface("terminal")} /> In a terminal tab
            </label>
            <label className="inline">
              <input type="radio" name="ag-surface" checked={surface === "headless"} onChange={() => setSurface("headless")} /> Headless (log shown here)
            </label>
            <label>
              First message
              <input type="text" aria-label="First message" value={prompt} onChange={(e) => setPrompt(e.target.value)} placeholder="e.g. Review the power rails" />
            </label>
            <p className="muted">Sessions started here run EAK's harness in propose mode: design changes come back as proposals for you to review and apply.</p>
          </div>
          <div className="ag-actions">
            <button
              type="button"
              className="primary"
              disabled={off || !!launch}
              title={launch ?? `Start ${t.label} on this project`}
              onClick={() =>
                void act(`launch:${t.id}`, async () => {
                  const v = await api.agentToolLaunch(t.id, surface, prompt.trim() || null);
                  setSel({ kind: "session", id: v.id });
                  return `${v.label} session ${v.id} started (${v.surface === "terminal" ? "a new tab in the Terminal workspace" : "headless"})`;
                })
              }
            >
              Launch session
            </button>
            <button
              type="button"
              disabled={off || !!login}
              title={login ?? `Run ${t.label}'s own sign-in in a Terminal tab`}
              onClick={() =>
                void act(`login:${t.id}`, async () => {
                  await api.agentToolLogin(t.id);
                  return `${t.label} sign-in opened in a new tab of the Terminal workspace. Finish it there, then press Check again.`;
                })
              }
            >
              Sign in
            </button>
            <button
              type="button"
              disabled={off || !!connect}
              title={connect ?? `Register EAK's MCP server with ${t.label} for this project`}
              onClick={() =>
                void act(`connect:${t.id}`, async () => {
                  const r = await api.agentToolConnect(t.id);
                  setTools((ts) => ts?.map((x) => (x.id === r.id ? r : x)) ?? null);
                  return `${r.label}: connected to this project: ${connectedText(r)}`;
                })
              }
            >
              Connect
            </button>
          </div>
        </section>
      </>
    );
  };

  // ---- Inspector ---------------------------------------------------------------------------
  const inspectorFor = (): ReactNode => {
    if (sel.kind === "agent") {
      const a = weft?.agents.find((x) => x.card.id === sel.id);
      if (!a) return <p className="muted">No registration to inspect.</p>;
      return (
        <>
          <h3>Agent card</h3>
          <Facts
            rows={[
              ["WEFT id", <code key="id">{a.card.id}</code>],
              ["Runtime", a.card.tool || "not declared"],
              ["Model", a.card.model || "not declared"],
              ["Capabilities", a.card.capabilities.length ? a.card.capabilities.join(", ") : "none declared"],
              ["Registered", `${clock(a.registered_ms)} · r${a.registered_revision}`],
              ["Permissions", permissionText(sessionOf(a, sessions))],
            ]}
          />
          <p className="muted">Declared by its launcher; EAK does not verify the model or the capabilities.</p>
        </>
      );
    }
    if (sel.kind === "task") {
      const t = weft?.tasks.find((x) => x.task.id === sel.id);
      if (!t) return <p className="muted">No task to inspect.</p>;
      return (
        <>
          <h3>Task contract</h3>
          <Facts
            rows={[
              ["Task id", <code key="id">{t.task.id}</code>],
              ["Parent", t.task.parent ?? "none"],
              ["Owner", who(t.task.owner)],
              ["Delegated by", who(t.task.created_by)],
              ["Status", TASK_STATUS_LABEL[t.task.status]],
              ["Depends on", t.task.depends_on.join(", ") || "nothing"],
              ["Waits on", t.waiting_on.join(", ") || (t.ready ? "nothing: ready" : "nothing")],
              ["Revision", revision === null ? `r${t.created_revision}` : revisionText(t.created_revision, revision)],
              ["Created", clock(t.created_ms)],
              ["Last change", clock(t.updated_ms)],
            ]}
          />
          <h3>Not in the task contract yet</h3>
          <ul className="ag-gaps" aria-label="Not recorded">
            {CONTRACT_GAPS.map((g) => (
              <li key={g.field}>
                {g.field} <span className="muted">({g.goal})</span>
              </li>
            ))}
          </ul>
        </>
      );
    }
    if (sel.kind === "session") {
      const s = selSession;
      if (!s) return <p className="muted">No session to inspect.</p>;
      return (
        <>
          <h3>Session</h3>
          <Facts
            rows={[
              ["Runtime", s.label],
              ["Where", s.surface === "terminal" ? "Terminal tab" : "Headless process"],
              ["Project folder", <code key="p">{s.project}</code>],
              ["Started", secs(s.started_at)],
              ["Process", s.pid !== null ? `pid ${s.pid}` : s.from_earlier_run ? "earlier run: not supervised" : "—"],
              ["Terminal tab", s.terminal_session !== null ? String(s.terminal_session) : "—"],
              ["Log file", s.log_file ? <code key="l">{s.log_file}</code> : "none"],
              ["WEFT id", s.weft_id ? <code key="w">{s.weft_id}</code> : "not known"],
              ["Permissions", permissionText(s)],
            ]}
          />
          {s.from_earlier_run && <p className="muted">Restored from EAK's session index: this session ran before EAK last closed.</p>}
        </>
      );
    }
    if (sel.kind === "runtime") {
      const t = (tools ?? []).find((x) => x.id === sel.id);
      if (!t) return <p className="muted">No runtime to inspect.</p>;
      return (
        <>
          <h3>Runtime</h3>
          <Facts
            rows={[
              ["Installed", foundText(t)],
              ["Program", t.binary ? <code key="b">{t.binary}</code> : "not found"],
              ["Signed in", signedInText(t)],
              ["Sign-in route", t.auth_detail ?? "not reported"],
              ["Connected to this project", connectedText(t)],
              ["Registration", t.connection_scope ?? "—"],
              ["Session surfaces", t.surfaces.join(", ") || "none"],
            ]}
          />
          <p className="muted">Each value comes from the tool's own commands; "unknown" means the tool could not tell EAK.</p>
        </>
      );
    }
    return (
      <>
        <h3>Project</h3>
        <Facts
          rows={[
            ["Project", projectName ?? "none open"],
            ["Folder", dir ? <code key="d">{dir}</code> : "—"],
            ["Revision", revision === null ? "—" : `r${revision}`],
            ["WEFT journal", weft ? `${weft.entries} entr${weft.entries === 1 ? "y" : "ies"}${weft.unreadable ? `, ${weft.unreadable} unreadable line(s) skipped` : ""}` : "not read"],
            ["Agents · tasks", weft ? `${weft.agents.length} · ${weft.tasks.length}` : "—"],
            ["Sessions here", String(sessions.filter((s) => inProject(s, dir)).length)],
          ]}
        />
        <p className="muted">Sessions started from EAK run in propose mode: agents create proposals; only you apply them.</p>
      </>
    );
  };

  // ---- Activity ----------------------------------------------------------------------------
  const activityFilter = (it: ActivityItem): boolean => {
    if (sel.kind === "agent") return it.actor === sel.id;
    if (sel.kind === "task") return it.task_id === sel.id;
    if (sel.kind === "session") return !!selSession?.weft_id && it.actor === selSession.weft_id;
    return true;
  };
  const shownActivity = activity.filter(activityFilter).slice(-200);

  return (
    <div className="agents-ws" role="region" aria-label="Agents workspace" onKeyDown={onKey}>
      <header className="ag-context" aria-label="Agents context">
        <h1>Agents</h1>
        <span className="ag-project">{projectName ?? "No project open"}</span>
        {revision !== null && (
          <Pill tone="info" title="The project's current revision">
            r{revision}
          </Pill>
        )}
        {weft && <span className="muted">WEFT journal: {weft.entries} entr{weft.entries === 1 ? "y" : "ies"}</span>}
        <span className="muted">App-started sessions: propose mode</span>
        <span className="spacer" />
        <button type="button" disabled={off} title="Read the journals and sessions again now" onClick={() => void refresh()}>
          Refresh
        </button>
        {openModelSettings && (
          <button type="button" title="Model providers and models (Model settings)" onClick={openModelSettings}>
            Model settings
          </button>
        )}
      </header>
      {readErrors.length > 0 && (
        <div role="alert" className="ag-errors">
          {readErrors.map((e) => (
            <p key={e}>{e}</p>
          ))}
        </div>
      )}
      {notice && (
        <p role="status" className={notice.ok ? "ag-notice" : "ag-notice bad"}>
          {notice.text}
        </p>
      )}
      <div className="ag-body">
        <nav className="ag-roster" aria-label="Roster">
          <input ref={filterRef} type="search" className="ag-filter" aria-label="Filter agents, tasks and sessions" placeholder="Filter  (/)" value={filter} onChange={(e) => setFilter(e.target.value)} />
          {row(OVERVIEW, <span>Overview</span>, "Overview")}
          <section aria-label="Needs you">
            <h3>
              Needs you <span className="count">{needs.length}</span>
            </h3>
            {needs.length === 0 ? (
              <p className="muted">Nothing waits on you.</p>
            ) : (
              <ul className="ag-list">
                {needs.map((n) => (
                  <li key={n.key}>{n.select ? row(n.select, <span>{n.title}</span>, n.title) : <span className="ag-static">{n.title}</span>}</li>
                ))}
              </ul>
            )}
          </section>
          <section aria-label="Agents">
            <h3>
              Agents <span className="count">{agents.length}</span>
            </h3>
            {!projectId ? (
              <p className="muted">Open a project to see its agents.</p>
            ) : agents.length === 0 ? (
              <p className="muted">No agent registered on this project's WEFT bus. Sessions started from EAK register themselves; any MCP client can call weft_register.</p>
            ) : (
              <ul className="ag-list">
                {agents.map((a) => {
                  const open = (weft?.tasks ?? []).filter((t) => t.task.owner === a.card.id && t.task.status !== "done" && t.task.status !== "rejected").length;
                  return (
                    <li key={a.card.id}>
                      {row(
                        { kind: "agent", id: a.card.id },
                        <>
                          <span>{a.card.name}</span>
                          <span className="muted">{a.card.tool || "tool not declared"}</span>
                          {open > 0 && <span className="count">{open}</span>}
                        </>,
                        `Agent ${actorName(a.card.id)}`,
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
          </section>
          <section aria-label="Tasks">
            <h3>
              Tasks <span className="count">{tasks.length}</span>
            </h3>
            {tasks.length === 0 ? (
              <p className="muted">{projectId ? "No task delegated yet." : "Open a project to see its tasks."}</p>
            ) : (
              <ul className="ag-list">
                {tasks.map(({ task: t, depth }) => (
                  <li key={t.task.id} style={{ paddingLeft: `calc(${depth} * var(--s-1h))` }}>
                    {row(
                      { kind: "task", id: t.task.id },
                      <>
                        <span>{t.task.title}</span>
                        <Pill tone={TASK_TONE[t.task.status]}>{TASK_STATUS_LABEL[t.task.status]}</Pill>
                      </>,
                      `Task ${t.task.title}`,
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>
          <section aria-label="Sessions">
            <h3>
              Sessions <span className="count">{shownSessions.length}</span>
            </h3>
            <label className="inline ag-toggle">
              <input type="checkbox" checked={thisProject} onChange={(e) => setThisProject(e.target.checked)} /> This project only
            </label>
            {sessionsError && <p className="muted">Could not list the sessions: {sessionsError}</p>}
            {indexError && (
              <p role="alert" className="ag-notice bad">
                {indexError}
              </p>
            )}
            {shownSessions.length === 0 ? (
              <p className="muted">No session started from EAK{thisProject ? " in this project" : ""}. Start one from a runtime below.</p>
            ) : (
              <ul className="ag-list">
                {shownSessions.map((s) => (
                  <li key={s.id}>
                    {row(
                      { kind: "session", id: s.id },
                      <>
                        <span>{s.label}</span>
                        <Pill tone={SESSION_TONE[s.state]}>{SESSION_STATE_LABEL[s.state]}</Pill>
                        {s.from_earlier_run && <span className="muted">earlier run</span>}
                      </>,
                      `Session ${s.id}`,
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>
          <section aria-label="Runtimes">
            <h3>
              Runtimes <span className="count">{runtimes.length}</span>
            </h3>
            <button type="button" disabled={probing} title={probing ? "Checking the coding-agent CLIs on this computer…" : "Probe the coding-agent CLIs on this computer again"} onClick={() => void probe()}>
              {probing ? "Checking…" : "Check again"}
            </button>
            {toolsError && <p className="muted">Could not check the runtimes: {toolsError}</p>}
            {!tools && !toolsError && <p className="muted">Checking the coding-agent CLIs on this computer…</p>}
            <ul className="ag-list">
              {runtimes.map((t) => (
                <li key={t.id} data-runtime={t.id}>
                  {row(
                    { kind: "runtime", id: t.id },
                    <>
                      <span>{t.label}</span>
                      <span className="muted">{runtimeText(t)}</span>
                    </>,
                    `Runtime ${t.label}`,
                  )}
                </li>
              ))}
            </ul>
          </section>
        </nav>
        <main className="ag-surface" aria-label="Work surface" data-selection={selectionKey(sel)}>
          {surfaceFor()}
        </main>
        <aside className="ag-inspector" aria-label="Inspector">
          {inspectorFor()}
        </aside>
      </div>
      <section className={activityOpen ? "ag-activity" : "ag-activity closed"} aria-label="Activity">
        <header>
          <h3>
            Activity <span className="count">{shownActivity.length}</span>
          </h3>
          <span className="muted">{sel.kind === "overview" || sel.kind === "runtime" ? "everything recorded in this project" : `for the selected ${sel.kind}`} · from the project journals</span>
          <span className="spacer" />
          <button type="button" aria-expanded={activityOpen} onClick={() => setActivityOpen((o) => !o)}>
            {activityOpen ? "Hide activity" : "Show activity"}
          </button>
        </header>
        {activityOpen &&
          (shownActivity.length === 0 ? (
            <p className="muted">{projectId ? "Nothing recorded for this selection yet." : "Open a project to see its activity."}</p>
          ) : (
            <ol className="ag-timeline">
              {shownActivity.map((it, i) => (
                <li key={`${it.source}-${it.ts_ms}-${i}`} data-activity={it.source} data-kind={it.kind}>
                  <span className="muted">{clock(it.ts_ms)}</span> <Pill>{it.source}</Pill> <span>{actorName(it.actor)}</span> <strong>{it.kind}</strong> <span>{it.summary}</span>
                  {it.revision_after !== null && <span className="muted"> r{it.revision_after}</span>}
                  {it.source === "check" && it.errors !== null && <span className="muted"> · {it.errors} error(s)</span>}
                </li>
              ))}
            </ol>
          ))}
      </section>
      {review && (
        <ProposalReviewDialog
          revision={revision ?? 0}
          initialId={review}
          busy={false}
          onApplied={(out) => {
            setReview(null);
            setNotice({ ok: true, text: `Applied as one undo step: proposal ${out.proposal.id} → revision ${out.response.revision_after} (checkpoint ${out.checkpoint})` });
            void refresh();
          }}
          onClose={() => setReview(null)}
        />
      )}
    </div>
  );
}
