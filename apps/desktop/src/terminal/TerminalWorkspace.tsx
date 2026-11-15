// Terminal workspace: EAK's own PTY engine (crates/eak-terminal) shown as spaces (one per
// project root or folder), tabs, split panes and live process status. Every session is a real
// OS process on a pseudo-terminal owned by the backend, so it keeps running while the EDA
// workspace is shown or the window reloads.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { describeStatus, term, type Overview, type SessionInfo, type Space, type Tab } from "./api";
import { disposeTerminal, XTermView } from "./XTermView";
import { StatusDot } from "../design/ui";
import { agentRows, launchArgv, STATE_WORD, type AgentRow } from "./agents";
import { boundToOtherProject, spaceForProject } from "./projectSpace";

export interface ProjectContext { dir: string; name: string; id: string }

interface Props {
  project: ProjectContext | null;
  visible: boolean;
  /** Bumped by the top bar "+" to open (or create) the current project's space. */
  newSpaceNonce: number;
}

type Menu = { x: number; y: number; items: { label: string; run: () => void }[] };
type Confirm = { text: string; action: string; run: () => Promise<void> };
type Rename = { label: string; value: string; save: (v: string) => Promise<void> };

const live = (s: SessionInfo) => !["ended", "completed", "failed"].includes(s.status.state);
const needsAttention = (s: SessionInfo) => s.status.state === "waiting_for_input" || s.status.state === "blocked";
const key = (e: React.KeyboardEvent, k: string) => e.ctrlKey && e.shiftKey && e.key.toLowerCase() === k;

export function TerminalWorkspace({ project, visible, newSpaceNonce }: Props) {
  const [ov, setOv] = useState<Overview | null>(null);
  const [shells, setShells] = useState<string[]>([]);
  const [shell, setShell] = useState("");
  const [launchers, setLaunchers] = useState<{ name: string; command: string; path: string | null }[]>([]);
  const [activeSpace, setActiveSpace] = useState<number | null>(null);
  const [collapsed, setCollapsed] = useState<Record<number, boolean>>({});
  const [sizes, setSizes] = useState<Record<number, number[]>>({});
  const [menu, setMenu] = useState<Menu | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [rename, setRename] = useState<Rename | null>(null);
  const [newSpace, setNewSpace] = useState<{ name: string; root: string } | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const dragTab = useRef<number | null>(null);

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
    void term.launchers().then(setLaunchers).catch(() => {});
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

  const sessions = ov?.sessions ?? [];
  const byId = useMemo(() => new Map(sessions.map((s) => [s.id, s])), [sessions]);
  const focused = ov?.focused != null ? byId.get(ov.focused) ?? null : null;
  const spaceId = activeSpace ?? focused?.space ?? ov?.spaces[0]?.id ?? null;
  const space = ov?.spaces.find((s) => s.id === spaceId) ?? null;
  const tabs = (ov?.tabs ?? []).filter((t) => t.space === spaceId);
  const tab: Tab | null = tabs.find((t) => focused && t.panes.includes(focused.id)) ?? tabs[0] ?? null;
  const current = focused && tab?.panes.includes(focused.id) ? focused : tab ? byId.get(tab.panes[0]) ?? null : null;
  const attention = sessions.filter(needsAttention);

  const openSession = (opts: Parameters<typeof term.create>[0]) =>
    act(async () => {
      const s = await term.create({ shell: shell || undefined, ...opts });
      setActiveSpace(s.space);
    });
  const newTerminal = (sp: number | null = spaceId) => openSession(sp != null ? { space: sp } : {});
  const split = (direction: "row" | "column") => current && openSession({ split_of: current.id, direction });
  // The open project's space (found by project id or folder, created when missing). Its sessions
  // export EAK_PROJECT_DIR / EAK_PROJECT_ID for the project the editors show.
  const ensureProjectSpace = useCallback(async (): Promise<Space | null> => {
    if (!project) return null;
    const ovNow = await term.overview();
    return spaceForProject(ovNow.spaces, project) ?? (await term.spaceCreate(project.name, project.dir));
  }, [project]);
  // Runs the agent executable itself as the PTY program (never a shell labelled as the agent).
  // With a project open the agent always starts in that project's space: started in another
  // project's space it would edit that project, and nothing would appear in the open editors.
  const launch = (a: AgentRow) =>
    act(async () => {
      const target = (await ensureProjectSpace())?.id ?? spaceId;
      const s = await term.create({ ...(target != null ? { space: target } : {}), name: a.label, command: launchArgv(a) });
      setActiveSpace(s.space);
      await term.focus(s.id);
    });
  const otherProject = boundToOtherProject(space, project);
  const focus = (s: SessionInfo) => {
    setActiveSpace(s.space);
    act(() => term.focus(s.id));
  };
  const closeSession = (s: SessionInfo) => {
    const run = async () => {
      await term.close(s.id);
      disposeTerminal(s.id);
    };
    if (!live(s)) return act(run);
    setConfirm({
      text: `Close “${s.name}”? Its process (${s.status.foreground ?? "shell"}, pid ${s.pid ?? "?"}) is hung up: SIGHUP, then killed if it does not exit.`,
      action: "Close and terminate",
      run,
    });
  };
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
  const renameSpace = (sp: Space) => setRename({ label: "Rename space", value: sp.name, save: (v) => term.spaceRename(sp.id, v) });
  const projectSpace = useCallback(async () => {
    const sp = await ensureProjectSpace();
    if (!sp) return;
    setActiveSpace(sp.id);
    if (!(await term.overview()).sessions.some((s) => s.space === sp.id)) await term.create({ space: sp.id });
    await refresh();
  }, [ensureProjectSpace, refresh]);

  useEffect(() => {
    if (newSpaceNonce > 0) void projectSpace().catch((e) => setMessage(String(e)));
  }, [newSpaceNonce, projectSpace]);

  const shortcuts = (e: React.KeyboardEvent) => {
    const run = (f: () => void) => {
      e.preventDefault();
      e.stopPropagation();
      f();
    };
    if (key(e, "t")) run(() => newTerminal());
    else if (key(e, "w") && current) run(() => closeSession(current));
    else if (key(e, "d")) run(() => split("row"));
    else if (key(e, "e")) run(() => split("column"));
    else if (key(e, "a") && attention[0]) run(() => focus(attention[0]));
    else if (e.ctrlKey && (e.key === "PageDown" || e.key === "PageUp") && tab && tabs.length > 1) {
      const i = tabs.indexOf(tab) + (e.key === "PageDown" ? 1 : -1);
      const next = tabs[(i + tabs.length) % tabs.length];
      const s = byId.get(next.panes[0]);
      if (s) run(() => focus(s));
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
    const box = e.currentTarget.parentElement!.getBoundingClientRect();
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
        <aside className="term-side" aria-label="Spaces">
          <header className="term-side-head">
            <span>SPACES</span>
            <span className="spacer" />
            {attention.length > 0 && (
              <button type="button" title="Go to the session that needs input (Ctrl+Shift+A)" onClick={() => focus(attention[0])}>
                {attention.length} need input
              </button>
            )}
          </header>
          <button type="button" className="term-new-space" onClick={() => setNewSpace({ name: project?.name ?? "", root: project?.dir ?? "" })}>
            + New Space
          </button>
          {(ov?.spaces ?? []).map((sp) => (
            <div key={sp.id} className="term-space">
              <div
                className={sp.id === spaceId ? "term-space-row on" : "term-space-row"}
                title={sp.root}
                onClick={() => setActiveSpace(sp.id)}
                onContextMenu={(e) =>
                  ctx(e, [
                    { label: "New terminal", run: () => newTerminal(sp.id) },
                    { label: "Rename space", run: () => renameSpace(sp) },
                    { label: "Close space", run: () => closeSpace(sp) },
                  ])
                }
              >
                <button
                  type="button"
                  className="caret"
                  aria-label={collapsed[sp.id] ? `Expand ${sp.name}` : `Collapse ${sp.name}`}
                  aria-expanded={!collapsed[sp.id]}
                  onClick={(e) => {
                    e.stopPropagation();
                    setCollapsed((c) => ({ ...c, [sp.id]: !c[sp.id] }));
                  }}
                >
                  {collapsed[sp.id] ? "▸" : "▾"}
                </button>
                <strong>{sp.name}</strong>
              </div>
              {!collapsed[sp.id] &&
                sessions
                  .filter((s) => s.space === sp.id)
                  .map((s) => (
                    <div
                      key={s.id}
                      className={s.id === current?.id ? "term-session on" : "term-session"}
                      title={`${describeStatus(s.status)} · ${s.spec.cwd}`}
                      onClick={() => focus(s)}
                      onContextMenu={(e) =>
                        ctx(e, [
                          { label: "Rename", run: () => renameSession(s) },
                          { label: "Restart", run: () => act(() => term.restart(s.id)) },
                          { label: "Close", run: () => closeSession(s) },
                        ])
                      }
                    >
                      <StatusDot state={s.status.state} />
                      <span className="grow">{s.name}</span>
                      {s.status.agent && <span className="chip">{s.status.agent}</span>}
                    </div>
                  ))}
            </div>
          ))}
          {ov && ov.spaces.length === 0 && <p className="muted pad">No spaces yet. Create one, or press “+ New terminal”.</p>}
        </aside>
        <section className="term-center">
          <div className="term-launch" role="toolbar" aria-label="New agent session">
            <span className="term-launch-label">New agent</span>
            {agentRows(launchers).map((a) => (
              <button
                key={a.id}
                type="button"
                className={a.installed ? "agent-btn" : "agent-btn missing"}
                disabled={!a.installed}
                title={a.installed ? `Start ${a.label} in a new session (${a.path ?? a.command})` : `${a.label} is not installed: "${a.id}" was not found on PATH`}
                onClick={() => launch(a)}
              >
                <span className="agent-ico" aria-hidden="true">{a.glyph}</span>
                <span className="agent-txt">
                  {a.label}
                  {!a.installed && <small>not installed</small>}
                </span>
              </button>
            ))}
            <span className="spacer" />
            <select aria-label="Shell" value={shell} onChange={(e) => setShell(e.target.value)}>
              <option value="">Default shell</option>
              {shells.map((s) => (
                <option key={s} value={s}>{s}</option>
              ))}
            </select>
            <button type="button" className="agent-btn" title="New terminal (Ctrl+Shift+T)" onClick={() => newTerminal()}>
              + New terminal
            </button>
          </div>
          <div className="term-sessbar">
            <div className="term-tabs" role="tablist" aria-label="Terminal sessions">
              {tabs.map((t, i) => {
                const s = byId.get(t.panes[0]);
                const state = s?.status.state ?? "ended";
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
                  >
                    <button
                      type="button"
                      role="tab"
                      aria-selected={t.id === tab?.id}
                      className="sess-main"
                      title={s ? `${describeStatus(s.status)} · ${s.spec.cwd}` : undefined}
                      onClick={() => s && focus(s)}
                      onDoubleClick={() => s && renameSession(s)}
                    >
                      <StatusDot state={state} />
                      <span className="sess-name">{s?.name ?? "terminal"}{t.panes.length > 1 ? ` +${t.panes.length - 1}` : ""}</span>
                      <span className={`sess-state st-${state}`}>{STATE_WORD[state] ?? state}</span>
                    </button>
                    {s && (
                      <button type="button" className="sess-x" aria-label={`Close ${s.name}`} title="Close session" onClick={() => closeSession(s)}>×</button>
                    )}
                  </div>
                );
              })}
            </div>
            {current && (
              <div className="term-sessctl" role="toolbar" aria-label="Session controls">
                <button type="button" title="Split right (Ctrl+Shift+D)" onClick={() => split("row")}>Split right</button>
                <button type="button" title="Split down (Ctrl+Shift+E)" onClick={() => split("column")}>Split down</button>
                <button type="button" onClick={() => renameSession(current)}>Rename</button>
                <button type="button" onClick={() => act(() => term.restart(current.id))}>Restart</button>
                <button type="button" title="Close (Ctrl+Shift+W)" onClick={() => closeSession(current)}>Close</button>
              </div>
            )}
          </div>
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
              <div className={`term-panes split-${tab.direction}`}>
                {tab.panes.map((id, i) => {
                  const s = byId.get(id);
                  return (
                    <div key={id} className="term-pane-wrap" style={{ flex: `${fractions(tab)[i]} 1 0%` }}>
                      <div className="term-pane" onPointerDownCapture={() => s && s.id !== ov?.focused && void term.focus(s.id).then(refresh)}>
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
                          <XTermView id={id} visible={visible} focused={visible && id === current?.id} />
                        )}
                      </div>
                      {i < tab.panes.length - 1 && (
                        <div className={`term-divider split-${tab.direction}`} role="separator" aria-label="Resize panes" onPointerDown={(e) => dragDivider(tab, i, e)} />
                      )}
                    </div>
                  );
                })}
              </div>
            ) : (
              <p className="pad muted">No terminal in this space yet: press “+ New terminal” (Ctrl+Shift+T).</p>
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
          </>
        ) : (
          "No session selected"
        )}
      </footer>
      {menu && (
        <ul className="term-menu" role="menu" style={{ left: menu.x, top: menu.y }}>
          {menu.items.map((it) => (
            <li key={it.label}>
              <button type="button" role="menuitem" onClick={() => { setMenu(null); it.run(); }}>{it.label}</button>
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
