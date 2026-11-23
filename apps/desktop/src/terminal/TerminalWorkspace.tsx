// Terminal workspace: EAK's own PTY engine (crates/eak-terminal) shown as Spaces (one per project
// root or folder) and an aggregated Agents list in the left sidebar, a tab strip, split panes with
// their own headers, an optional details panel and a status bar. Every session is a real OS process
// on a pseudo-terminal owned by the backend, so it keeps running while the EDA workspace is shown or
// the window reloads. The default action opens a normal shell; the user runs whichever coding agent
// CLI they installed themselves. Agents are only detected passively (see ./agents.ts), never started.
import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { currentTask, taskSummary, taskTitle, useAgentTasks } from "../agentTasks";
import { describeStatus, term, type Overview, type SessionInfo, type Space, type Tab } from "./api";
import { disposeTerminal, XTermView } from "./XTermView";
import { StatusDot } from "../design/ui";
import { AGENT_DOT, agentEntries, rollup, STATE_WORD, tabTitle } from "./agents";
import { boundToOtherProject, spaceForProject } from "./projectSpace";

export interface ProjectContext { dir: string; name: string; id: string; revision?: number | null }

interface Props {
  project: ProjectContext | null;
  visible: boolean;
  /** Bumped by the top bar "+" to open (or create) the current project's space. */
  newSpaceNonce: number;
}

type Menu = { x: number; y: number; items: { label: string; run: () => void; disabled?: boolean }[] };
type Confirm = { text: string; action: string; run: () => Promise<void> };
type Rename = { label: string; value: string; save: (v: string) => Promise<void> };

const live = (s: SessionInfo) => !["ended", "completed", "failed"].includes(s.status.state);
const needsAttention = (s: SessionInfo) => s.status.state === "waiting_for_input" || s.status.state === "blocked";
const key = (e: React.KeyboardEvent, k: string) => e.ctrlKey && e.shiftKey && e.key.toLowerCase() === k;
const folder = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

export function TerminalWorkspace({ project, visible, newSpaceNonce }: Props) {
  const [ov, setOv] = useState<Overview | null>(null);
  const [shells, setShells] = useState<string[]>([]);
  const [activeSpace, setActiveSpace] = useState<number | null>(null);
  const [sizes, setSizes] = useState<Record<number, number[]>>({});
  const [menu, setMenu] = useState<Menu | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [rename, setRename] = useState<Rename | null>(null);
  const [newSpace, setNewSpace] = useState<{ name: string; root: string } | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [details, setDetails] = useState(false);
  const dragTab = useRef<number | null>(null);
  // Same revision and task phase as the EDA status bar (ADR-0036).
  const agentTask = currentTask(useAgentTasks(project?.id ?? null, project?.revision ?? 0));

  const refresh = useCallback(async () => setOv(await term.overview()), []);
  const act = useCallback(
    (f: () => Promise<unknown>) =>
      void f()
        .then(refresh)
        .catch((e) => setMessage(String(e))),
    [refresh],
  );

  useEffect(() => {
    void refresh();
    void term.shells().then(setShells).catch(() => {});
  }, [refresh]);
  // The backend binds spaces opened on the project folder to this project, so their sessions
  // export EAK_PROJECT_DIR / EAK_PROJECT_ID for the harness (docs/AGENT_HARNESS.md).
  const [pDir, pName, pId] = [project?.dir ?? null, project?.name ?? null, project?.id ?? null];
  useEffect(() => {
    void term.setContext(pDir, pName, pId).catch(() => {});
  }, [pDir, pName, pId]);
  // Process states change without output (a job finishes, a prompt waits): poll while shown.
  useEffect(() => {
    if (!visible) return;
    const t = setInterval(() => void refresh().catch(() => {}), 1500);
    return () => clearInterval(t);
  }, [visible, refresh]);

  const sessions = useMemo(() => ov?.sessions ?? [], [ov]);
  const byId = useMemo(() => new Map((ov?.sessions ?? []).map((s) => [s.id, s])), [ov]);
  const focused = ov?.focused != null ? byId.get(ov.focused) ?? null : null;
  const spaceId = activeSpace ?? focused?.space ?? ov?.spaces[0]?.id ?? null;
  const space = ov?.spaces.find((s) => s.id === spaceId) ?? null;
  const tabs = useMemo(() => (ov?.tabs ?? []).filter((t) => t.space === spaceId), [ov, spaceId]);
  const tab: Tab | null = tabs.find((t) => focused && t.panes.includes(focused.id)) ?? tabs[0] ?? null;
  const current = focused && tab?.panes.includes(focused.id) ? focused : tab ? byId.get(tab.panes[0]) ?? null : null;
  const attention = sessions.filter(needsAttention);
  const agents = useMemo(() => agentEntries(ov), [ov]);

  const openSession = (opts: Parameters<typeof term.create>[0]) =>
    act(async () => {
      const s = await term.create(opts);
      setActiveSpace(s.space);
    });
  /** A new tab with a normal shell (the default shell unless one is chosen). */
  const newTerminal = (sp: number | null = spaceId, shell?: string) =>
    openSession({ ...(sp != null ? { space: sp } : {}), ...(shell ? { shell } : {}) });
  const split = (direction: "row" | "column", of: SessionInfo | null = current) => of && openSession({ split_of: of.id, direction });
  const focus = (s: SessionInfo) => {
    setActiveSpace(s.space);
    act(() => term.focus(s.id));
  };
  const selectTab = (t: Tab) => {
    const s = byId.get(t.panes[0]);
    if (s && !(focused && t.panes.includes(focused.id))) focus(s);
  };
  const selectSpace = (sp: Space) => {
    setActiveSpace(sp.id);
    const first = (ov?.tabs ?? []).find((t) => t.space === sp.id);
    const s = first && byId.get(first.panes[0]);
    if (s) act(() => term.focus(s.id));
  };
  const closeSessions = (list: SessionInfo[], what: string) => {
    const run = async () => {
      for (const s of list) {
        await term.close(s.id);
        disposeTerminal(s.id);
      }
    };
    const running = list.filter(live);
    if (running.length === 0) return act(run);
    setConfirm({
      text: `Close ${what}? ${running.length} running process(es) (${running.map((s) => s.status.foreground ?? "shell").join(", ")}) are hung up: SIGHUP, then killed if they do not exit.`,
      action: "Close and terminate",
      run,
    });
  };
  const closeSession = (s: SessionInfo) => closeSessions([s], `“${s.name}”`);
  const closeTab = (t: Tab) =>
    closeSessions(t.panes.flatMap((p) => byId.get(p) ?? []), `tab “${tabTitle(t, byId)}”`);
  const closeSpace = (sp: Space) => {
    const inside = sessions.filter((s) => s.space === sp.id);
    setConfirm({
      text: `Close space “${sp.name}”? ${inside.filter(live).length} running session(s) in it are terminated.`,
      action: "Close space",
      run: async () => {
        await term.spaceClose(sp.id);
        inside.forEach((s) => disposeTerminal(s.id));
        setActiveSpace(null);
      },
    });
  };
  const renameSession = (s: SessionInfo) => setRename({ label: "Rename terminal", value: s.name, save: (v) => term.rename(s.id, v) });
  const renameTab = (t: Tab) => setRename({ label: "Rename tab", value: tabTitle(t, byId), save: (v) => term.tabRename(t.id, v) });
  const renameSpace = (sp: Space) => setRename({ label: "Rename space", value: sp.name, save: (v) => term.spaceRename(sp.id, v) });
  const moveTab = (t: Tab, by: number) => {
    const i = tabs.findIndex((x) => x.id === t.id) + by;
    if (i >= 0 && i < tabs.length) act(() => term.moveTab(t.id, i));
  };
  // A session inherits EAK_PROJECT_DIR / EAK_PROJECT_ID from its space: an agent started in a
  // space bound to another project edits that project, and nothing appears in the open editors.
  const otherProject = boundToOtherProject(space, project);
  const projectSpace = useCallback(async () => {
    if (!project) return;
    const ovNow = await term.overview();
    // By project id, else by folder (separators, trailing slash, Windows drive case ignored).
    const have = spaceForProject(ovNow.spaces, project);
    const sp = have ?? (await term.spaceCreate(project.name, project.dir));
    setActiveSpace(sp.id);
    if (!ovNow.sessions.some((s) => s.space === sp.id)) await term.create({ space: sp.id });
    await refresh();
  }, [project, refresh]);

  useEffect(() => {
    if (newSpaceNonce > 0) void projectSpace().catch((e) => setMessage(String(e)));
  }, [newSpaceNonce, projectSpace]);

  const shortcuts = (e: React.KeyboardEvent) => {
    const run = (f: () => void) => {
      e.preventDefault();
      e.stopPropagation();
      f();
    };
    const digit = /^Digit([1-9])$/.exec(e.code)?.[1];
    if (key(e, "t")) run(() => newTerminal());
    else if (key(e, "w") && current) run(() => closeSession(current));
    else if (key(e, "d")) run(() => split("row"));
    else if (key(e, "e")) run(() => split("column"));
    else if (key(e, "r") && tab) run(() => renameTab(tab));
    else if (key(e, "n")) run(() => setNewSpace({ name: project?.name ?? "", root: project?.dir ?? "" }));
    else if (key(e, "p")) run(() => setDetails((d) => !d));
    else if (key(e, "a") && attention[0]) run(() => focus(attention[0]));
    else if (e.ctrlKey && e.shiftKey && digit && tabs[Number(digit) - 1]) run(() => selectTab(tabs[Number(digit) - 1]));
    else if (e.ctrlKey && e.shiftKey && e.key.startsWith("Arrow") && tab && current && tab.panes.length > 1) {
      const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
      const next = byId.get(tab.panes[(tab.panes.indexOf(current.id) + step + tab.panes.length) % tab.panes.length]);
      if (step && next) run(() => focus(next));
    } else if (e.ctrlKey && (e.key === "PageDown" || e.key === "PageUp") && tab && tabs.length > 1) {
      const i = tabs.indexOf(tab) + (e.key === "PageDown" ? 1 : -1);
      run(() => selectTab(tabs[(i + tabs.length) % tabs.length]));
    }
  };

  const ctx = (e: React.MouseEvent, items: Menu["items"]) => {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, items });
  };

  const fractions = (t: Tab) => {
    const f = sizes[t.id];
    return f && f.length === t.panes.length ? f : t.panes.map(() => 1 / t.panes.length);
  };
  const dragDivider = (t: Tab, i: number, e: React.PointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget.parentElement!.parentElement!.getBoundingClientRect();
    const total = t.direction === "row" ? box.width : box.height;
    const start = t.direction === "row" ? e.clientX : e.clientY;
    const f0 = fractions(t);
    const move = (ev: PointerEvent) => {
      const d = ((t.direction === "row" ? ev.clientX : ev.clientY) - start) / total;
      const a = Math.min(Math.max(f0[i] + d, 0.1), f0[i] + f0[i + 1] - 0.1);
      const f = [...f0];
      f[i + 1] = f0[i] + f0[i + 1] - a;
      f[i] = a;
      setSizes((s) => ({ ...s, [t.id]: f }));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  return (
    <div className="term-ws" aria-label="Terminal workspace" onKeyDownCapture={shortcuts} onClick={() => setMenu(null)}>
      <div className="term-body">
        <aside className="term-side" aria-label="Spaces and agents">
          <section className="term-sec" aria-label="Spaces">
            <header className="term-side-head">
              <span>SPACES</span>
              <span className="spacer" />
              {attention.length > 0 && (
                <button type="button" title="Go to the session that needs input (Ctrl+Shift+A)" onClick={() => focus(attention[0])}>
                  {attention.length} need input
                </button>
              )}
            </header>
            <button type="button" className="term-new-space" title="New space (Ctrl+Shift+N)" onClick={() => setNewSpace({ name: project?.name ?? "", root: project?.dir ?? "" })}>
              + New Space
            </button>
            {(ov?.spaces ?? []).map((sp) => {
              const inside = sessions.filter((s) => s.space === sp.id);
              return (
                <Fragment key={sp.id}>
                <div
                  className={sp.id === spaceId ? "term-space-row on" : "term-space-row"}
                  role="button"
                  tabIndex={0}
                  aria-current={sp.id === spaceId}
                  title={sp.root}
                  onClick={() => selectSpace(sp)}
                  onKeyDown={(e) => e.key === "Enter" && selectSpace(sp)}
                  onDoubleClick={() => renameSpace(sp)}
                  onContextMenu={(e) =>
                    ctx(e, [
                      { label: "New terminal", run: () => newTerminal(sp.id) },
                      { label: "Rename space", run: () => renameSpace(sp) },
                      { label: "Close space", run: () => closeSpace(sp) },
                    ])
                  }
                >
                  <StatusDot state={rollup(inside)} />
                  <span className="term-space-text">
                    <strong>{sp.name}</strong>
                    <small>{sp.project_id ? `project · ${folder(sp.root)}` : folder(sp.root)}</small>
                  </span>
                  <span className="chip" title={`${inside.length} terminal session(s)`}>{inside.length}</span>
                  <button type="button" className="sess-x" aria-label={`Close space ${sp.name}`} title="Close space" onClick={(e) => { e.stopPropagation(); closeSpace(sp); }}>×</button>
                </div>
                {agents
                  .filter((a) => a.space === sp.id)
                  .map((a) => (
                    // Nested under its workspace with a live status dot; the agent name is drawn by
                    // CSS (data-agent) so the AGENTS list below stays the one textual listing.
                    <div
                      key={`n${a.session}`}
                      className={a.session === current?.id ? "term-nested on" : "term-nested"}
                      role="button"
                      tabIndex={0}
                      data-agent={a.agent}
                      aria-label={`${a.agent}, ${a.state}, ${a.tabTitle} pane ${a.pane}`}
                      title={`${a.agent}: ${a.state}`}
                      onClick={() => {
                        const s = byId.get(a.session);
                        if (s) focus(s);
                      }}
                      onKeyDown={(e) => {
                        const s = byId.get(a.session);
                        if (e.key === "Enter" && s) focus(s);
                      }}
                    >
                      <StatusDot state={AGENT_DOT[a.state]} />
                    </div>
                  ))}
                </Fragment>
              );
            })}
            {ov && ov.spaces.length === 0 && <p className="muted pad">No spaces yet. Create one, or press “+” to open a terminal.</p>}
          </section>
          <section className="term-sec" aria-label="Agents">
            <header className="term-side-head">
              <span>AGENTS</span>
              <span className="spacer" />
              <span className="chip">{agents.length}</span>
            </header>
            {agents.map((a) => (
              <div
                key={a.session}
                className={a.session === current?.id ? "term-agent on" : "term-agent"}
                role="button"
                tabIndex={0}
                title={`${a.agent} in ${a.spaceName} / ${a.tabTitle} / pane ${a.pane}: click to go to it`}
                onClick={() => {
                  const s = byId.get(a.session);
                  if (s) focus(s);
                }}
                onKeyDown={(e) => {
                  const s = byId.get(a.session);
                  if (e.key === "Enter" && s) focus(s);
                }}
              >
                <StatusDot state={AGENT_DOT[a.state]} />
                <span className="term-space-text">
                  <strong>{a.agent}</strong>
                  <small>
                    {a.spaceName} / {a.tabTitle} / pane {a.pane}
                  </small>
                </span>
                <span className={`sess-state st-${AGENT_DOT[a.state]}`}>{a.state}</span>
              </div>
            ))}
            {agents.length === 0 && (
              <p className="muted pad">
                No coding agent detected. Run your own CLI in any terminal; EAK only watches the foreground program name and never starts, installs or configures an agent.
              </p>
            )}
          </section>
        </aside>
        <section className="term-center">
          <div className="term-sessbar">
            <div className="term-tabs" role="tablist" aria-label="Terminal tabs">
              {tabs.map((t, i) => {
                const title = tabTitle(t, byId);
                const panes = t.panes.flatMap((p) => byId.get(p) ?? []);
                const state = rollup(panes);
                const first = panes[0];
                return (
                  <div
                    key={t.id}
                    className={t.id === tab?.id ? "sess-tab on" : "sess-tab"}
                    draggable
                    onDragStart={() => (dragTab.current = t.id)}
                    onDragOver={(e) => e.preventDefault()}
                    onDrop={() => {
                      const from = dragTab.current;
                      dragTab.current = null;
                      if (from != null && from !== t.id) act(() => term.moveTab(from, i));
                    }}
                    onContextMenu={(e) =>
                      ctx(e, [
                        { label: "Rename tab", run: () => renameTab(t) },
                        { label: "Split right", run: () => first && split("row", first) },
                        { label: "Split down", run: () => first && split("column", first) },
                        { label: "Move left", run: () => moveTab(t, -1), disabled: i === 0 },
                        { label: "Move right", run: () => moveTab(t, 1), disabled: i === tabs.length - 1 },
                        { label: "Close tab", run: () => closeTab(t) },
                      ])
                    }
                  >
                    <button
                      type="button"
                      role="tab"
                      aria-selected={t.id === tab?.id}
                      className="sess-main"
                      title={first ? `${describeStatus(first.status)} · ${first.spec.cwd}` : undefined}
                      onClick={() => selectTab(t)}
                      onDoubleClick={() => renameTab(t)}
                    >
                      <StatusDot state={state} />
                      <span className="sess-name">{title}{t.panes.length > 1 ? ` +${t.panes.length - 1}` : ""}</span>
                      <span className={`sess-state st-${state}`}>{STATE_WORD[state] ?? state}</span>
                    </button>
                    <button type="button" className="sess-x" aria-label={`Close tab ${title}`} title="Close tab" onClick={() => closeTab(t)}>×</button>
                  </div>
                );
              })}
            </div>
            <div className="term-sessctl" role="toolbar" aria-label="New terminal">
              <button type="button" className="tab-plus" aria-label="New terminal" title="New terminal: a normal shell (Ctrl+Shift+T)" onClick={() => newTerminal()}>
                +
              </button>
              <button
                type="button"
                className="tab-plus"
                aria-label="Choose shell"
                title="Open a terminal with a specific shell"
                onClick={(e) => {
                  e.stopPropagation();
                  const r = e.currentTarget.getBoundingClientRect();
                  setMenu({
                    x: r.left,
                    y: r.bottom,
                    items: [
                      { label: "Default shell", run: () => newTerminal() },
                      ...shells.map((s) => ({ label: s, run: () => newTerminal(spaceId, s) })),
                    ],
                  });
                }}
              >
                ▾
              </button>
              <button type="button" aria-pressed={details} title="Show or hide the details panel (Ctrl+Shift+P)" onClick={() => setDetails((d) => !d)}>
                Details
              </button>
            </div>
          </div>
          {project && project.revision != null && (
            <div className="term-msg" data-testid="agent-task" title={taskTitle(agentTask)}>
              {project.name}: {taskSummary(agentTask, project.revision)}
            </div>
          )}
          {otherProject && project && space && (
            <div className="term-msg term-other-project" role="alert">
              Space “{space.name}” belongs to another project, not “{project.name}” (open in the editors). Agents here
              edit that project, so their changes will not appear in the schematic or PCB.
              <button type="button" onClick={() => void projectSpace().catch((e) => setMessage(String(e)))}>
                Go to {project.name}’s space
              </button>
            </div>
          )}
          {message && (
            <div className="term-msg" role="status">
              {message} <button type="button" onClick={() => setMessage(null)}>Dismiss</button>
            </div>
          )}
          <div className="term-main">
            {tab ? (
              <div className={`term-panes ${tab.direction}`}>
                {tab.panes.map((id, i) => {
                  const s = byId.get(id);
                  const on = id === current?.id;
                  return (
                    <div key={id} className="term-pane-wrap" style={{ flex: `${fractions(tab)[i]} 1 0%` }}>
                      <div className={on ? "term-pane on" : "term-pane"} onPointerDownCapture={() => s && s.id !== ov?.focused && void term.focus(s.id).then(refresh)}>
                        {s && (
                          <div className="term-pane-head" onContextMenu={(e) => ctx(e, [
                            { label: "Rename", run: () => renameSession(s) },
                            { label: "Restart", run: () => act(() => term.restart(s.id)) },
                            { label: "Close pane", run: () => closeSession(s) },
                          ])}>
                            <StatusDot state={s.status.state} />
                            <span className="pane-name" title={`${describeStatus(s.status)} · ${s.spec.cwd}`}>{s.name}</span>
                            {s.status.agent && <span className="chip" title="Detected from the foreground program name">{s.status.agent}</span>}
                            <span className={`sess-state st-${s.status.state}`}>{STATE_WORD[s.status.state] ?? s.status.state}</span>
                            <span className="spacer" />
                            <button type="button" aria-label={`Split ${s.name} right`} title="Split right (Ctrl+Shift+D)" onClick={() => split("row", s)}>⇥</button>
                            <button type="button" aria-label={`Split ${s.name} down`} title="Split down (Ctrl+Shift+E)" onClick={() => split("column", s)}>⇩</button>
                            <button type="button" aria-label={`Rename ${s.name}`} title="Rename" onClick={() => renameSession(s)}>✎</button>
                            <button type="button" aria-label={`Restart ${s.name}`} title="Restart" onClick={() => act(() => term.restart(s.id))}>↻</button>
                            <button type="button" aria-label={`Close pane ${s.name}`} title="Close pane (Ctrl+Shift+W)" onClick={() => closeSession(s)}>×</button>
                          </div>
                        )}
                        <div className="term-pane-body">
                          {!s ? (
                            <div className="term-pick" role="group" aria-label="Empty pane">
                              <span>No session in this pane.</span>
                              <button type="button" onClick={() => newTerminal()}>New terminal</button>
                              {sessions.filter((o) => o.space === spaceId && o.id !== current?.id).slice(0, 4).map((o) => (
                                <button key={o.id} type="button" onClick={() => focus(o)}>Show {o.name}</button>
                              ))}
                            </div>
                          ) : s.status.state === "ended" ? (
                            <div className="pad">
                              {s.name} ended when EAK last exited ({s.spec.cwd}). Its process is gone.{" "}
                              <button type="button" onClick={() => act(() => term.restart(s.id))}>Restart</button>
                            </div>
                          ) : (
                            <XTermView id={id} visible={visible} focused={visible && on} />
                          )}
                        </div>
                      </div>
                      {i < tab.panes.length - 1 && (
                        <div className={`term-divider ${tab.direction}`} role="separator" aria-label="Resize panes" onPointerDown={(e) => dragDivider(tab, i, e)} />
                      )}
                    </div>
                  );
                })}
              </div>
            ) : (
              <p className="pad muted">No terminal in this space yet: press “+” (Ctrl+Shift+T) to open a shell.</p>
            )}
            {details && (
              <aside className="term-aside" aria-label="Session details">
                <header className="term-side-head"><span>DETAILS</span></header>
                {current ? (
                  <dl>
                    <dt>Name</dt><dd>{current.name}</dd>
                    <dt>Status</dt><dd>{describeStatus(current.status)}</dd>
                    <dt>Evidence</dt><dd>{current.status.detail}</dd>
                    <dt>Runs</dt><dd>{current.spec.program.kind === "shell" ? current.spec.program.path ?? "default shell" : current.spec.program.argv.join(" ")}</dd>
                    <dt>Foreground</dt><dd>{current.status.foreground ?? "-"}</dd>
                    <dt>Agent</dt><dd>{current.status.agent ?? "none detected"}</dd>
                    <dt>Folder</dt><dd>{current.spec.cwd}</dd>
                    <dt>Process</dt><dd>{current.pid ?? "-"}</dd>
                  </dl>
                ) : (
                  <p className="muted pad">No session selected.</p>
                )}
              </aside>
            )}
          </div>
        </section>
      </div>
      <footer className="term-status" aria-label="Session status">
        {current ? (
          <>
            <StatusDot state={current.status.state} /> {describeStatus(current.status)} · {current.status.detail} · cwd {current.spec.cwd}
            {current.pid ? ` · pid ${current.pid}` : ""}
            {current.status.agent ? ` · agent ${current.status.agent}` : ""}
            {space ? ` · space ${space.name}` : ""}
            {tab ? ` · tab ${tabTitle(tab, byId)} (pane ${tab.panes.indexOf(current.id) + 1}/${tab.panes.length})` : ""}
          </>
        ) : (
          "No session selected"
        )}
      </footer>
      {menu && (
        <ul className="term-menu" role="menu" style={{ left: menu.x, top: menu.y }}>
          {menu.items.map((it) => (
            <li key={it.label}>
              <button type="button" role="menuitem" disabled={it.disabled} onClick={() => { setMenu(null); it.run(); }}>{it.label}</button>
            </li>
          ))}
        </ul>
      )}
      {confirm && (
        <div className="term-msg term-confirm" role="alertdialog" aria-label="Confirm close">
          {confirm.text}
          <button type="button" onClick={() => { const c = confirm; setConfirm(null); act(c.run); }}>{confirm.action}</button>
          <button type="button" onClick={() => setConfirm(null)}>Cancel</button>
        </div>
      )}
      {rename && (
        <form
          className="term-msg term-confirm"
          aria-label={rename.label}
          onSubmit={(e) => {
            e.preventDefault();
            const r = rename;
            setRename(null);
            act(() => r.save(r.value));
          }}
        >
          <label>
            {rename.label} <input autoFocus value={rename.value} onChange={(e) => setRename({ ...rename, value: e.target.value })} />
          </label>
          <button type="submit">Save</button>
          <button type="button" onClick={() => setRename(null)}>Cancel</button>
        </form>
      )}
      {newSpace && (
        <form
          className="term-msg term-confirm"
          aria-label="New space"
          onSubmit={(e) => {
            e.preventDefault();
            const n = newSpace;
            setNewSpace(null);
            act(async () => {
              const sp = await term.spaceCreate(n.name, n.root);
              setActiveSpace(sp.id);
              await term.create({ space: sp.id });
            });
          }}
        >
          <label>
            Name <input value={newSpace.name} onChange={(e) => setNewSpace({ ...newSpace, name: e.target.value })} />
          </label>
          <label>
            Root folder <input required value={newSpace.root} placeholder="/path/to/project" onChange={(e) => setNewSpace({ ...newSpace, root: e.target.value })} />
          </label>
          <button type="submit">Create space</button>
          <button type="button" onClick={() => setNewSpace(null)}>Cancel</button>
        </form>
      )}
    </div>
  );
}
