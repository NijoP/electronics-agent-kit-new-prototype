// Terminal workspace: the real Herdr (docs/HERDR_INTEGRATION.md) running in a PTY that EAK
// supervises, filling the main area. Herdr provides spaces, tabs, splits, agent status and
// session persistence; EAK adds project spaces and the server health line. Without Herdr
// installed, EAK's built-in PTY terminal is offered instead.
import { useCallback, useEffect, useState } from "react";
import { describeStatus, term, type HerdrView, type Overview, type SessionInfo } from "./api";
import { disposeTerminal, XTermView } from "./XTermView";

export interface ProjectContext { dir: string; name: string; id: string }

interface Props {
  project: ProjectContext | null;
  visible: boolean;
  /** Bumped by the top bar "+" to open a space for the current project. */
  newSpaceNonce: number;
}

const live = (s: SessionInfo) => !["ended", "completed", "failed"].includes(s.status.state);
const sameArgv = (s: SessionInfo, argv: string[]) => s.spec.program.kind === "command" && s.spec.program.argv.join("\0") === argv.join("\0");

export function TerminalWorkspace({ project, visible, newSpaceNonce }: Props) {
  const [herdr, setHerdr] = useState<HerdrView | null>(null);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [agents, setAgents] = useState<Record<string, number>>({});
  const [confirmClose, setConfirmClose] = useState<SessionInfo | null>(null);

  const refresh = useCallback(async () => setOverview(await term.overview()), []);
  const client = herdr?.client ?? null;
  const herdrSession = client && overview ? overview.sessions.find((s) => sameArgv(s, client)) ?? null : null;

  // First show: start/attach Herdr, or fall back to built-in sessions.
  useEffect(() => {
    if (!visible || herdr) return;
    void (async () => {
      const st = await term.herdrStatus();
      if (!st.installed || st.error) {
        setHerdr(st);
        await refresh();
        return;
      }
      try {
        const started = await term.herdrStart();
        setHerdr(started);
        const ov = await term.overview();
        const argv = started.client!;
        if (!ov.sessions.some((s) => sameArgv(s, argv) && live(s))) {
          const old = ov.sessions.find((s) => sameArgv(s, argv));
          if (old) await term.restart(old.id);
          else await term.create({ command: argv, name: "Herdr" });
        }
        await refresh();
      } catch (e) {
        setHerdr({ ...st, error: String(e) });
        await refresh();
      }
    })();
  }, [visible, herdr, refresh]);

  // Keep status fresh while visible (process states change without output).
  useEffect(() => {
    if (!visible) return;
    const t = setInterval(() => {
      void refresh();
      if (herdr?.health) {
        void term.herdrStatus().then(setHerdr);
        void term
          .herdrAgents()
          .then((r) => {
            const c: Record<string, number> = {};
            for (const a of r.agents ?? []) {
              const k = a.agent_status ?? a.status ?? "unknown";
              c[k] = (c[k] ?? 0) + 1;
            }
            setAgents(c);
          })
          .catch(() => setAgents({}));
      }
    }, 2000);
    return () => clearInterval(t);
  }, [visible, herdr?.health, refresh]);

  const openProjectSpace = useCallback(async () => {
    if (!project) return;
    try {
      await term.herdrOpenProject(project.dir, project.name, project.id);
      setMessage(`Opened the ${project.name} space in Herdr (${project.dir})`);
    } catch (e) {
      setMessage(String(e));
    }
  }, [project]);

  useEffect(() => {
    if (newSpaceNonce > 0 && herdr?.health?.running) void openProjectSpace();
  }, [newSpaceNonce, herdr?.health?.running, openProjectSpace]);

  if (!herdr) return <div className="term-ws" aria-label="Terminal workspace"><p className="muted pad">Starting the terminal workspace…</p></div>;

  const h = herdr.health;
  if (h && herdrSession) {
    const attached = live(herdrSession);
    return (
      <div className="term-ws" aria-label="Terminal workspace">
        <header className="term-head">
          <strong>Herdr</strong>
          <span className={h.running ? "dot ok" : "dot bad"} title={h.running ? "server running" : "server not running"} />
          <span className="muted">
            {h.version ?? "?"} · session {h.session} · {h.running ? "server running" : "server down"}
            {h.restarts > 0 ? ` · restarted ${h.restarts}×` : ""}
          </span>
          {Object.keys(agents).length > 0 && (
            <span className="term-agents" aria-label="Agent activity">
              {Object.entries(agents).map(([k, n]) => (
                <span key={k} className={`chip st-${k}`}>{n} {k}</span>
              ))}
            </span>
          )}
          <span className="spacer" />
          {project && (
            <button type="button" onClick={() => void openProjectSpace()} title={`Herdr workspace rooted at ${project.dir}`}>
              Open {project.name} space
            </button>
          )}
          {!attached && (
            <button type="button" onClick={() => void term.restart(herdrSession.id).then(refresh)}>
              Reattach
            </button>
          )}
        </header>
        {message && <div className="term-msg" role="status">{message}</div>}
        <div className="term-main">
          {attached ? (
            <XTermView id={herdrSession.id} focused={visible} />
          ) : (
            <p className="pad">
              The Herdr client is detached ({describeStatus(herdrSession.status)}). Your Herdr sessions keep running in the
              server; press Reattach to show them again.
            </p>
          )}
        </div>
        <footer className="term-status">
          Herdr {h.running ? "server running" : "server not running"} · processes are owned by the Herdr server and keep
          running while EAK shows the EDA workspace or is closed.
        </footer>
      </div>
    );
  }

  return <BuiltIn herdr={herdr} overview={overview} refresh={refresh} visible={visible} confirm={confirmClose} setConfirm={setConfirmClose} />;
}

/** EAK's own PTY terminal, used when Herdr is unavailable. */
function BuiltIn({ herdr, overview, refresh, visible, confirm, setConfirm }: {
  herdr: HerdrView;
  overview: Overview | null;
  refresh: () => Promise<void>;
  visible: boolean;
  confirm: SessionInfo | null;
  setConfirm: (s: SessionInfo | null) => void;
}) {
  const [shells, setShells] = useState<string[]>([]);
  const [shell, setShell] = useState("");
  useEffect(() => void term.shells().then(setShells), []);
  const sessions = overview?.sessions ?? [];
  const focused = sessions.find((s) => s.id === overview?.focused) ?? sessions[sessions.length - 1] ?? null;
  const newTerminal = async () => {
    await term.create({ shell: shell || undefined });
    await refresh();
  };
  const close = async (s: SessionInfo) => {
    await term.close(s.id);
    disposeTerminal(s.id);
    setConfirm(null);
    await refresh();
  };
  return (
    <div className="term-ws" aria-label="Terminal workspace">
      <div className="term-msg" role="status">
        {herdr.installed
          ? `Herdr could not be started: ${herdr.error}. Using EAK's built-in terminal.`
          : "Herdr is not installed (https://herdr.dev). Using EAK's built-in terminal; install Herdr for spaces, splits and agent tracking."}
      </div>
      <div className="tabs term-tabs" role="tablist">
        {sessions.map((s) => (
          <button key={s.id} type="button" role="tab" aria-selected={s.id === focused?.id} className={s.id === focused?.id ? "tab on" : "tab"} onClick={() => void term.focus(s.id).then(refresh)}>
            <span className={`dot st-${s.status.state}`} /> {s.name}
          </button>
        ))}
        <select aria-label="Shell" value={shell} onChange={(e) => setShell(e.target.value)}>
          <option value="">Default shell</option>
          {shells.map((s) => <option key={s} value={s}>{s}</option>)}
        </select>
        <button type="button" onClick={() => void newTerminal()}>+ New terminal</button>
        {focused && <button type="button" onClick={() => void term.restart(focused.id).then(refresh)}>Restart</button>}
        {focused && <button type="button" onClick={() => (live(focused) ? setConfirm(focused) : void close(focused))}>Close</button>}
      </div>
      {confirm && (
        <div className="term-msg" role="alertdialog" aria-label="Confirm close">
          Close “{confirm.name}”? Its process ({confirm.status.foreground ?? "shell"}) is hung up (SIGHUP, then killed).
          <button type="button" onClick={() => void close(confirm)}>Close and terminate</button>
          <button type="button" onClick={() => setConfirm(null)}>Cancel</button>
        </div>
      )}
      <div className="term-main">
        {focused ? <XTermView key={focused.id} id={focused.id} focused={visible} /> : <p className="pad muted">No terminal yet: press “+ New terminal”.</p>}
      </div>
      {focused && (
        <footer className="term-status">
          {describeStatus(focused.status)} · {focused.spec.cwd}
          {focused.pid ? ` · pid ${focused.pid}` : ""}
          {focused.status.foreground ? ` · ${focused.status.foreground}` : ""}
        </footer>
      )}
    </div>
  );
}
